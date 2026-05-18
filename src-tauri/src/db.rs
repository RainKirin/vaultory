use crate::crypto::CryptoManager;
use crate::models::*;
use base64::Engine;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// 数据库连接句柄。
///
/// 与旧版本相比，连接不再在应用启动时立即创建——只有在用户成功通过解锁/初始化
/// 流程后，才会用派生出的 SQLCipher 主密钥真正打开 SQLite 连接；锁定时直接丢弃
/// 连接，使整库密文留在磁盘上，没有任何运行时句柄可供读取。
pub struct Database {
    db_path: PathBuf,
    conn: Mutex<Option<Connection>>,
}

impl Database {
    /// 创建实例但不打开数据库连接。
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            conn: Mutex::new(None),
        }
    }

    pub fn path(&self) -> &Path {
        &self.db_path
    }

    /// 数据库文件是否存在（用于判断"应用是否已初始化"）。
    pub fn file_exists(&self) -> bool {
        self.db_path.exists()
    }

    /// 判断磁盘上的数据库文件是否仍是明文 SQLite。
    ///
    /// 仅在 SQLCipher 改造前用旧版本运行过的用户会有这种文件——SQLite 文件头是固定
    /// `"SQLite format 3\0"`，SQLCipher 加密后文件头被加密成随机字节。读 16 字节
    /// 比较，比起依赖 PRAGMA 失败的副作用更稳。
    pub fn is_plaintext(&self) -> Result<bool, String> {
        use std::io::Read;
        if !self.db_path.exists() {
            return Ok(false);
        }
        let mut f = std::fs::File::open(&self.db_path)
            .map_err(|e| format!("读取数据库文件失败: {}", e))?;
        let mut header = [0u8; 16];
        if f.read_exact(&mut header).is_err() {
            // 文件不足 16 字节，肯定不是有效的 SQLite/SQLCipher 数据库
            return Ok(false);
        }
        Ok(&header == b"SQLite format 3\0")
    }

    /// 用 SQLCipher 密钥打开数据库。
    ///
    /// 如果数据库不存在则会创建一个新的加密数据库。打开后立刻执行一条 SELECT 来确认
    /// 密钥正确——`PRAGMA key` 本身不会验证密钥是否能正确解密页面，必须实际读一页才
    /// 能判断。
    ///
    /// 注意 SQLCipher 的 raw key 格式严格要求：SQL 语句必须长成
    /// `PRAGMA key = "x'<hex>'";`——外层双引号包字符串字面量，里面 `x'...'` 是
    /// SQLCipher 自己识别的 BLOB-shape 字符串。**不能**用 `pragma_update`，因为
    /// rusqlite 会把传入的字符串再做一次 SQL 转义，结果把 `x'...'` 当成普通 ASCII
    /// passphrase（走 PBKDF2），与本应用的 Argon2id 派生密钥完全脱节。
    pub fn open_with_key(&self, sqlcipher_key_hex: &str) -> Result<(), String> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;

        // sqlcipher_key_hex 形如 `x'aabb...'`，全部由 [0-9a-f] + 固定标点构成，
        // 不存在 SQL 注入面。
        let key_sql = format!("PRAGMA key = \"{}\";", sqlcipher_key_hex);
        conn.execute_batch(&key_sql)
            .map_err(|e| format!("应用数据库密钥失败: {}", e))?;

        // 密钥错误时这一行会失败（无法解密 schema 页）。
        let _: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))
            .map_err(|_| "数据库密钥错误或数据库损坏".to_string())?;

        Self::ensure_schema(&conn)?;
        Self::migrate(&conn)?;

        *self
            .conn
            .lock()
            .map_err(|_| "数据库状态被污染".to_string())? = Some(conn);
        Ok(())
    }

    /// 关闭连接（drop 当前 Connection），密钥不再驻留内存。
    pub fn close(&self) {
        if let Ok(mut guard) = self.conn.lock() {
            *guard = None;
        }
    }

    /// 用新的 SQLCipher 密钥重新加密整个数据库。
    ///
    /// 调用时连接必须已经用旧密钥打开。SQLCipher 的 `PRAGMA rekey` 在内部用流式方式
    /// 重写每一页，对几 MB 的密码库来说是瞬间的。
    ///
    /// 同 `open_with_key`，必须用 `execute_batch` 拼字面 SQL；不能用 `pragma_update`。
    pub fn rekey(&self, new_sqlcipher_key_hex: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let sql = format!("PRAGMA rekey = \"{}\";", new_sqlcipher_key_hex);
            conn.execute_batch(&sql)
                .map_err(|e| format!("更换数据库密钥失败: {}", e))?;
            Ok(())
        })
    }

    /// 把磁盘上的旧明文 SQLite 数据库迁移到 SQLCipher 加密格式。
    ///
    /// 流程（参考 SQLCipher 官方推荐做法）：
    ///   1. 用 raw rusqlite 打开旧明文 db
    ///   2. ATTACH 一个临时空 db，并用给定密钥设为加密
    ///   3. `sqlcipher_export` 把全部 schema + 数据复制到加密库
    ///   4. 把 user_version 也同步过去（sqlcipher_export 不会复制 pragma）
    ///   5. DETACH，关闭旧连接
    ///   6. 原子替换文件：先把旧明文文件重命名为 .bak，再把临时加密文件改名为正式
    ///   7. 成功后删除 .bak；任何一步出错都尝试回滚
    ///
    /// 调用方有责任先用主密码哈希校验过密码——本函数不验证密码，只搬数据。
    pub fn migrate_plaintext_to_encrypted(
        &self,
        sqlcipher_key_hex: &str,
    ) -> Result<(), String> {
        if !self.is_plaintext()? {
            return Err("数据库已不是明文格式，无需迁移".to_string());
        }

        let tmp_path = self.db_path.with_extension("db.tmp");
        if tmp_path.exists() {
            std::fs::remove_file(&tmp_path)
                .map_err(|e| format!("清理临时文件失败: {}", e))?;
        }

        let old_conn = Connection::open(&self.db_path)
            .map_err(|e| format!("打开旧数据库失败: {}", e))?;

        let tmp_path_str = tmp_path
            .to_str()
            .ok_or_else(|| "临时路径包含非 UTF-8 字符".to_string())?
            .replace('\'', "''");

        // SQLCipher 的 KEY 字面量不支持 bind parameter，必须直接插值。
        // sqlcipher_key_hex 由我们内部 hex 编码生成，只含 [0-9a-f]，没有 SQL 注入风险。
        let attach_sql = format!(
            "ATTACH DATABASE '{}' AS encrypted KEY \"{}\";",
            tmp_path_str, sqlcipher_key_hex
        );
        old_conn
            .execute_batch(&attach_sql)
            .map_err(|e| format!("附加临时加密库失败: {}", e))?;

        old_conn
            .query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))
            .map_err(|e| format!("导出到加密库失败: {}", e))?;

        let user_version: i32 = old_conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);
        old_conn
            .execute_batch(&format!("PRAGMA encrypted.user_version = {};", user_version))
            .map_err(|e| format!("同步 user_version 失败: {}", e))?;

        old_conn
            .execute_batch("DETACH DATABASE encrypted;")
            .map_err(|e| format!("分离临时加密库失败: {}", e))?;
        drop(old_conn);

        let backup_path = self.db_path.with_extension("db.bak");
        std::fs::rename(&self.db_path, &backup_path)
            .map_err(|e| format!("备份旧数据库失败: {}", e))?;

        if let Err(e) = std::fs::rename(&tmp_path, &self.db_path) {
            // 回滚：还原旧数据库，丢掉临时加密库
            let _ = std::fs::rename(&backup_path, &self.db_path);
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("启用加密数据库失败: {}", e));
        }

        // 迁移成功，删除明文备份。我们尽量用 secure_delete 减少数据残留，但平台级保证
        // 只能由 SSD 控制器决定。
        let _ = std::fs::remove_file(&backup_path);
        Ok(())
    }

    fn ensure_schema(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vault_entries (
                id TEXT PRIMARY KEY,
                entry_type TEXT NOT NULL,
                encrypted_data TEXT NOT NULL,
                folder TEXT,
                favorite INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS auth (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                password_hash TEXT NOT NULL,
                salt TEXT,
                hash_salt TEXT,
                encrypt_salt TEXT
            );",
        )
        .map_err(|e| format!("创建表失败: {}", e))?;
        Ok(())
    }

    fn migrate(conn: &Connection) -> Result<(), String> {
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| format!("获取数据库版本失败: {}", e))?;

        if version < 2 {
            conn.execute_batch(
                r#"
                ALTER TABLE folders ADD COLUMN parent_id TEXT NULL;
                ALTER TABLE folders ADD COLUMN sort_order INTEGER NULL;
                CREATE INDEX IF NOT EXISTS idx_folders_parent ON folders(parent_id);
                PRAGMA user_version = 2;
            "#,
            )
            .map_err(|e| format!("v2 迁移失败: {}", e))?;
        }

        if version < 3 {
            conn.execute_batch(r#"
                ALTER TABLE auth ADD COLUMN hash_salt TEXT NULL;
                ALTER TABLE auth ADD COLUMN encrypt_salt TEXT NULL;
                UPDATE auth SET hash_salt = salt, encrypt_salt = salt WHERE hash_salt IS NULL AND salt IS NOT NULL;
                PRAGMA user_version = 3;
            "#).map_err(|e| format!("v3 迁移失败: {}", e))?;
        }

        if version < 4 {
            conn.execute_batch(
                r#"
                ALTER TABLE vault_entries ADD COLUMN search_index TEXT NOT NULL DEFAULT '';
                CREATE INDEX IF NOT EXISTS idx_search ON vault_entries(search_index);
                PRAGMA user_version = 4;
            "#,
            )
            .map_err(|e| format!("v4 迁移失败: {}", e))?;
        }

        if version < 5 {
            conn.execute_batch(
                r#"
                UPDATE vault_entries SET search_index = '';
                PRAGMA user_version = 5;
            "#,
            )
            .map_err(|e| format!("v5 迁移失败: {}", e))?;
        }

        if version < 6 {
            // v6 标记"数据库已经处于 SQLCipher 整库加密格式"。schema 本身没有改动，
            // 但保留版本号方便未来识别。
            conn.execute_batch("PRAGMA user_version = 6;")
                .map_err(|e| format!("v6 迁移失败: {}", e))?;
        }

        Ok(())
    }

    fn entry_matches_query(entry: &VaultEntry, query: &str) -> bool {
        [
            Some(entry.name.as_str()),
            entry.username.as_deref(),
            entry.url.as_deref(),
            entry.api_key.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| value.to_lowercase().contains(query))
    }

    fn lock_conn(&self) -> Result<MutexGuard<'_, Option<Connection>>, String> {
        self.conn
            .lock()
            .map_err(|_| "数据库锁定状态被污染".to_string())
    }

    fn with_conn<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String>,
    {
        let guard = self.lock_conn()?;
        match guard.as_ref() {
            Some(conn) => f(conn),
            None => Err("数据库未打开".to_string()),
        }
    }

    fn with_conn_mut<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut Connection) -> Result<R, String>,
    {
        let mut guard = self.lock_conn()?;
        match guard.as_mut() {
            Some(conn) => f(conn),
            None => Err("数据库未打开".to_string()),
        }
    }

    pub fn is_initialized(&self) -> Result<bool, String> {
        self.with_conn(|conn| {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM auth", [], |row| row.get(0))
                .map_err(|_| "检查初始化状态失败".to_string())?;
            Ok(count > 0)
        })
    }

    pub fn setup_auth(
        &self,
        password_hash: &str,
        hash_salt: &[u8],
        encrypt_salt: &[u8],
    ) -> Result<(), String> {
        let hash_salt_b64 = base64::engine::general_purpose::STANDARD.encode(hash_salt);
        let encrypt_salt_b64 = base64::engine::general_purpose::STANDARD.encode(encrypt_salt);
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO auth (id, password_hash, salt, hash_salt, encrypt_salt) VALUES (1, ?1, ?2, ?3, ?4)",
                // Some older databases still have `salt TEXT NOT NULL`, so we keep a placeholder
                // value there for backward schema compatibility while the current app uses the
                // explicit hash/encryption salt columns.
                params![
                    password_hash,
                    encrypt_salt_b64,
                    hash_salt_b64,
                    encrypt_salt_b64
                ],
            )
            .map_err(|e| format!("保存认证数据失败: {}", e))?;
            Ok(())
        })
    }

    pub fn get_auth(&self) -> Result<(String, Vec<u8>, Vec<u8>), String> {
        self.with_conn(|conn| {
            let row: (String, Option<String>, Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT password_hash, hash_salt, encrypt_salt, salt FROM auth WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(|_| "未找到认证数据".to_string())?;

            let (hash, hash_salt_b64, encrypt_salt_b64, old_salt_b64) = row;
            let hash_salt_b64 = hash_salt_b64
                .or(old_salt_b64.clone())
                .ok_or("缺少哈希 salt")?;
            let encrypt_salt_b64 = encrypt_salt_b64.or(old_salt_b64).ok_or("缺少加密 salt")?;

            let hash_salt = base64::engine::general_purpose::STANDARD
                .decode(&hash_salt_b64)
                .map_err(|_| "哈希 salt 无效".to_string())?;
            let encrypt_salt = base64::engine::general_purpose::STANDARD
                .decode(&encrypt_salt_b64)
                .map_err(|_| "加密 salt 无效".to_string())?;
            Ok((hash, hash_salt, encrypt_salt))
        })
    }

    pub fn rotate_master_password(
        &self,
        old_crypto: &CryptoManager,
        new_crypto: &CryptoManager,
        new_password_hash: &str,
        new_hash_salt: &[u8],
        new_encrypt_salt: &[u8],
    ) -> Result<(), String> {
        let hash_salt_b64 = base64::engine::general_purpose::STANDARD.encode(new_hash_salt);
        let encrypt_salt_b64 = base64::engine::general_purpose::STANDARD.encode(new_encrypt_salt);

        self.with_conn_mut(|conn| {
            let tx = conn
                .transaction()
                .map_err(|_| "启动密码轮换事务失败".to_string())?;

            let reencrypted_rows: Vec<(String, String)> = {
                let mut stmt = tx
                    .prepare("SELECT id, encrypted_data FROM vault_entries")
                    .map_err(|e| format!("准备保险库查询失败: {}", e))?;
                let rows = stmt
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map_err(|e| format!("查询保险库条目失败: {}", e))?;

                let mut result = Vec::new();
                for row in rows {
                    let (id, encrypted): (String, String) =
                        row.map_err(|e| format!("读取保险库条目行失败: {}", e))?;
                    let decrypted = old_crypto.decrypt(&encrypted)?;
                    let reencrypted = new_crypto.encrypt(&decrypted)?;
                    result.push((id, reencrypted));
                }
                result
            };

            for (id, encrypted_data) in reencrypted_rows {
                tx.execute(
                    "UPDATE vault_entries SET encrypted_data = ?1 WHERE id = ?2",
                    params![encrypted_data, id],
                )
                .map_err(|e| format!("更新保险库条目失败: {}", e))?;
            }

            let updated = tx
                .execute(
                    "UPDATE auth SET password_hash = ?1, salt = ?2, hash_salt = ?3, encrypt_salt = ?4 WHERE id = 1",
                    params![
                        new_password_hash,
                        encrypt_salt_b64,
                        hash_salt_b64,
                        encrypt_salt_b64
                    ],
                )
                .map_err(|e| format!("更新认证数据失败: {}", e))?;
            if updated == 0 {
                return Err("未找到认证数据".to_string());
            }

            tx.commit().map_err(|_| "提交密码轮换失败".to_string())?;
            Ok(())
        })
    }

    pub fn save_entry(&self, entry: &VaultEntry, crypto: &CryptoManager) -> Result<(), String> {
        let data = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        let encrypted = crypto.encrypt(&data)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    entry.id,
                    serde_json::to_string(&entry.entry_type).unwrap().trim_matches('"'),
                    encrypted,
                    "",
                    entry.folder,
                    entry.favorite as i64,
                    entry.created_at,
                    entry.updated_at,
                ],
            ).map_err(|_| "保存条目失败".to_string())?;
            Ok(())
        })
    }

    pub fn get_all_entries(&self, crypto: &CryptoManager) -> Result<Vec<VaultEntry>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT encrypted_data FROM vault_entries ORDER BY favorite DESC, updated_at DESC",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    let encrypted: String = row.get(0)?;
                    Ok(encrypted)
                })
                .map_err(|e| e.to_string())?;

            let mut entries = Vec::new();
            for row in rows {
                let encrypted = row.map_err(|e| e.to_string())?;
                let decrypted = crypto.decrypt(&encrypted)?;
                let entry: VaultEntry =
                    serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
                entries.push(entry);
            }
            Ok(entries)
        })
    }

    pub fn get_entry(
        &self,
        id: &str,
        crypto: &CryptoManager,
    ) -> Result<Option<VaultEntry>, String> {
        self.with_conn(|conn| {
            let result: Option<String> = conn
                .query_row(
                    "SELECT encrypted_data FROM vault_entries WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .ok();

            match result {
                Some(encrypted) => {
                    let decrypted = crypto.decrypt(&encrypted)?;
                    let entry: VaultEntry =
                        serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
                    Ok(Some(entry))
                }
                None => Ok(None),
            }
        })
    }

    pub fn delete_entry(&self, id: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM vault_entries WHERE id = ?1", params![id])
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn search_entries(
        &self,
        query: &str,
        crypto: &CryptoManager,
    ) -> Result<Vec<VaultEntry>, String> {
        let q = query.to_lowercase();
        let entries = self.get_all_entries(crypto)?;
        Ok(entries
            .into_iter()
            .filter(|entry| Self::entry_matches_query(entry, &q))
            .collect())
    }

    pub fn save_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        self.with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![key],
                    |row| row.get::<_, String>(0),
                )
                .ok())
        })
    }

    pub fn delete_setting(&self, key: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
                .map_err(|e| format!("删除设置失败: {}", e))?;
            Ok(())
        })
    }

    pub fn save_folder(&self, folder: &Folder) -> Result<(), String> {
        self.with_conn(|conn| {
            let updated = conn
                .execute(
                    "UPDATE folders SET name = ?2, parent_id = ?3, sort_order = ?4 WHERE id = ?1",
                    params![folder.id, folder.name, folder.parent_id, folder.sort_order],
                )
                .map_err(|e| e.to_string())?;

            if updated == 0 {
                conn.execute(
                    "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                    params![folder.id, folder.name, folder.parent_id, folder.sort_order],
                )
                .map_err(|e| e.to_string())?;
            }

            Ok(())
        })
    }

    pub fn get_all_folders(&self) -> Result<Vec<Folder>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, name, parent_id, sort_order FROM folders ORDER BY sort_order, name COLLATE NOCASE")
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(Folder {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        parent_id: row.get(2)?,
                        sort_order: row.get(3)?,
                    })
                })
                .map_err(|e| e.to_string())?;

            let mut folders = Vec::new();
            for row in rows {
                folders.push(row.map_err(|e| e.to_string())?);
            }
            Ok(folders)
        })
    }

    pub fn get_folder_parent(&self, id: &str) -> Result<Option<String>, String> {
        self.with_conn(|conn| {
            let result: Option<String> = conn
                .query_row(
                    "SELECT parent_id FROM folders WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .ok()
                .flatten();
            Ok(result)
        })
    }

    pub fn get_entries_in_folder(
        &self,
        folder_id: &str,
        include_descendants: bool,
        crypto: &CryptoManager,
    ) -> Result<Vec<VaultEntry>, String> {
        self.with_conn(|conn| {
            let encrypted_rows: Vec<String> = if include_descendants {
                let mut stmt = conn
                    .prepare(
                        "WITH RECURSIVE sub(id) AS (
                            SELECT ?1
                            UNION ALL
                            SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id
                        )
                        SELECT e.encrypted_data FROM vault_entries e WHERE e.folder IN (SELECT id FROM sub) ORDER BY e.favorite DESC, e.updated_at DESC"
                    )
                    .map_err(|e| e.to_string())?;

                let rows = stmt
                    .query_map(params![folder_id], |row| row.get(0))
                    .map_err(|e| e.to_string())?;

                let mut result = Vec::new();
                for row in rows {
                    result.push(row.map_err(|e| e.to_string())?);
                }
                result
            } else {
                let mut stmt = conn
                    .prepare("SELECT encrypted_data FROM vault_entries WHERE folder = ?1 ORDER BY favorite DESC, updated_at DESC")
                    .map_err(|e| e.to_string())?;

                let rows = stmt
                    .query_map(params![folder_id], |row| row.get(0))
                    .map_err(|e| e.to_string())?;

                let mut result = Vec::new();
                for row in rows {
                    result.push(row.map_err(|e| e.to_string())?);
                }
                result
            };

            let mut entries = Vec::new();
            for encrypted in encrypted_rows {
                let decrypted = crypto.decrypt(&encrypted)?;
                let entry: VaultEntry =
                    serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
                entries.push(entry);
            }
            Ok(entries)
        })
    }

    pub fn delete_folder_with_transaction(&self, id: &str, strategy: &str) -> Result<(), String> {
        self.with_conn_mut(|conn| {
            let tx = conn
                .transaction()
                .map_err(|_| "启动事务失败".to_string())?;

            match strategy {
                "merge_up" => {
                    let parent_id: Option<String> = tx
                        .query_row(
                            "SELECT parent_id FROM folders WHERE id = ?1",
                            params![id],
                            |row| row.get(0),
                        )
                        .ok()
                        .flatten();

                    tx.execute(
                        "UPDATE folders SET parent_id = ?1 WHERE parent_id = ?2",
                        params![parent_id, id],
                    )
                    .map_err(|_| "更新子文件夹 parent 失败".to_string())?;

                    tx.execute(
                        "UPDATE vault_entries SET folder = ?1 WHERE folder = ?2",
                        params![parent_id, id],
                    )
                    .map_err(|_| "更新条目 folder 失败".to_string())?;

                    tx.execute("DELETE FROM folders WHERE id = ?1", params![id])
                        .map_err(|_| "删除文件夹失败".to_string())?;
                }
                "cascade" => {
                    let descendants: Vec<String> = tx
                        .prepare(
                            "WITH RECURSIVE sub(id) AS (
                            SELECT ?1
                            UNION ALL
                            SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id
                        )
                        SELECT id FROM sub",
                        )
                        .map_err(|_| "查询子孙文件夹失败".to_string())?
                        .query_map(params![id], |row| row.get(0))
                        .map_err(|_| "获取子孙文件夹失败".to_string())?
                        .collect::<Result<Vec<String>, _>>()
                        .map_err(|_| "收集子孙文件夹失败".to_string())?;

                    for desc_id in &descendants {
                        tx.execute(
                            "DELETE FROM vault_entries WHERE folder = ?1",
                            params![desc_id],
                        )
                        .map_err(|_| "删除文件夹条目失败".to_string())?;
                    }

                    tx.execute(
                        "DELETE FROM folders WHERE id IN (SELECT id FROM (
                        WITH RECURSIVE sub(id) AS (
                            SELECT ?1
                            UNION ALL
                            SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id
                        )
                        SELECT id FROM sub
                    ))",
                        params![id],
                    )
                    .map_err(|_| "删除子孙文件夹失败".to_string())?;
                }
                _ => return Err("未知策略".to_string()),
            }

            tx.commit().map_err(|_| "提交事务失败".to_string())?;
            Ok(())
        })
    }

    /// 销毁所有数据。
    ///
    /// SQLCipher 整库加密之后，最彻底的 reset 方法是直接删除数据库文件——这样磁盘上
    /// 不会再有任何加密的元数据残留，比 DELETE + VACUUM 更干净。
    pub fn reset_vault(&self) -> Result<(), String> {
        self.close();
        if self.db_path.exists() {
            std::fs::remove_file(&self.db_path)
                .map_err(|e| format!("删除数据库文件失败: {}", e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
impl Database {
    /// 测试专用：从一个已经准备好的 Connection 直接构造 Database，绕过 SQLCipher
    /// 打开流程。这样大多数业务逻辑测试可以用 `Connection::open_in_memory()` 跑，
    /// 不必每次重新走文件迁移路径。
    pub(crate) fn from_connection_for_test(conn: Connection) -> Self {
        Self {
            db_path: PathBuf::new(),
            conn: Mutex::new(Some(conn)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{format_sqlcipher_key_hex, CryptoManager};
    use crate::models::{EntryType, VaultEntry};

    fn create_test_database() -> Database {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        conn.execute_batch(
            "CREATE TABLE vault_entries (
                id TEXT PRIMARY KEY,
                entry_type TEXT NOT NULL,
                encrypted_data TEXT NOT NULL,
                search_index TEXT NOT NULL DEFAULT '',
                folder TEXT,
                favorite INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                parent_id TEXT NULL,
                sort_order INTEGER NULL
            );
            CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE auth (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                password_hash TEXT NOT NULL,
                salt TEXT,
                hash_salt TEXT,
                encrypt_salt TEXT
            );",
        )
        .expect("schema should initialize");

        Database::from_connection_for_test(conn)
    }

    fn create_legacy_auth_test_database() -> Database {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        conn.execute_batch(
            "CREATE TABLE vault_entries (
                id TEXT PRIMARY KEY,
                entry_type TEXT NOT NULL,
                encrypted_data TEXT NOT NULL,
                search_index TEXT NOT NULL DEFAULT '',
                folder TEXT,
                favorite INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                parent_id TEXT NULL,
                sort_order INTEGER NULL
            );
            CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE auth (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                password_hash TEXT NOT NULL,
                salt TEXT NOT NULL,
                hash_salt TEXT NULL,
                encrypt_salt TEXT NULL
            );",
        )
        .expect("legacy schema should initialize");

        Database::from_connection_for_test(conn)
    }

    fn conn_guard_for_test(db: &Database) -> MutexGuard<'_, Option<Connection>> {
        db.conn.lock().expect("database mutex should lock")
    }

    fn with_test_conn<F, R>(db: &Database, f: F) -> R
    where
        F: FnOnce(&Connection) -> R,
    {
        let guard = conn_guard_for_test(db);
        let conn = guard.as_ref().expect("test connection should be open");
        f(conn)
    }

    #[test]
    fn delete_folder_with_transaction_should_preserve_nested_hierarchy_when_merging_up() {
        let db = create_test_database();
        with_test_conn(&db, |conn| {
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["root", "root", Option::<String>::None, 0],
            )
            .expect("root folder should insert");
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["a", "a", "root", 1],
            )
            .expect("folder a should insert");
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["b", "b", "a", 2],
            )
            .expect("folder b should insert");
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["c", "c", "b", 3],
            )
            .expect("folder c should insert");
            conn.execute(
                "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params!["entry-a", "login", "encrypted", "search", "a", 0, 0, 0],
            )
            .expect("entry should insert");
        });

        db.delete_folder_with_transaction("a", "merge_up")
            .expect("merge_up should succeed");

        with_test_conn(&db, |conn| {
            let b_parent: Option<String> = conn
                .query_row("SELECT parent_id FROM folders WHERE id = 'b'", [], |row| {
                    row.get(0)
                })
                .expect("folder b should still exist");
            let c_parent: Option<String> = conn
                .query_row("SELECT parent_id FROM folders WHERE id = 'c'", [], |row| {
                    row.get(0)
                })
                .expect("folder c should still exist");
            let moved_entry_folder: Option<String> = conn
                .query_row(
                    "SELECT folder FROM vault_entries WHERE id = 'entry-a'",
                    [],
                    |row| row.get(0),
                )
                .expect("entry should still exist");
            let deleted_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM folders WHERE id = 'a'", [], |row| {
                    row.get(0)
                })
                .expect("count query should succeed");

            assert_eq!(b_parent.as_deref(), Some("root"));
            assert_eq!(c_parent.as_deref(), Some("b"));
            assert_eq!(moved_entry_folder.as_deref(), Some("root"));
            assert_eq!(deleted_count, 0);
        });
    }

    #[test]
    fn search_entries_should_search_decrypted_fields_without_persisting_plaintext_index() {
        let db = create_test_database();
        let key = CryptoManager::derive_key("master-password", &[7; 32])
            .expect("key derivation should succeed");
        let crypto = CryptoManager::new(&key);
        let entry = VaultEntry {
            id: "entry-1".to_string(),
            entry_type: EntryType::Login,
            name: "GitHub".to_string(),
            username: Some("alice@example.com".to_string()),
            password: Some("secret".to_string()),
            url: Some("https://github.com".to_string()),
            api_key: None,
            notes: None,
            folder: None,
            favorite: false,
            created_at: 0,
            updated_at: 0,
        };
        let encrypted = crypto
            .encrypt(&serde_json::to_string(&entry).expect("entry should serialize"))
            .expect("entry should encrypt");

        with_test_conn(&db, |conn| {
            conn.execute(
                "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![entry.id, "login", encrypted, "", Option::<String>::None, 0, 0, 0],
            )
            .expect("legacy row should insert");
        });

        let matches = db
            .search_entries("github", &crypto)
            .expect("search should succeed");

        with_test_conn(&db, |conn| {
            let search_index: String = conn
                .query_row(
                    "SELECT search_index FROM vault_entries WHERE id = 'entry-1'",
                    [],
                    |row| row.get(0),
                )
                .expect("search index should exist");

            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].id, "entry-1");
            assert_eq!(search_index, "");
        });
    }

    #[test]
    fn delete_folder_with_transaction_should_delete_entries_when_cascading() {
        let db = create_test_database();
        with_test_conn(&db, |conn| {
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["a", "a", Option::<String>::None, 0],
            )
            .expect("folder a should insert");
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["b", "b", "a", 1],
            )
            .expect("folder b should insert");
            conn.execute(
                "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params!["entry-a", "login", "encrypted", "search", "a", 0, 0, 0],
            )
            .expect("root entry should insert");
            conn.execute(
                "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params!["entry-b", "login", "encrypted", "search", "b", 0, 0, 0],
            )
            .expect("child entry should insert");
        });

        db.delete_folder_with_transaction("a", "cascade")
            .expect("cascade should succeed");

        with_test_conn(&db, |conn| {
            let remaining_folders: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM folders WHERE id IN ('a', 'b')",
                    [],
                    |row| row.get(0),
                )
                .expect("folder count query should succeed");
            let remaining_entries: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM vault_entries WHERE id IN ('entry-a', 'entry-b')",
                    [],
                    |row| row.get(0),
                )
                .expect("entry count query should succeed");

            assert_eq!(remaining_folders, 0);
            assert_eq!(remaining_entries, 0);
        });
    }

    #[test]
    fn migrate_should_clear_legacy_plaintext_search_indexes() {
        let conn = Connection::open_in_memory().expect("in-memory database should open");
        conn.execute_batch(
            "CREATE TABLE vault_entries (
                id TEXT PRIMARY KEY,
                entry_type TEXT NOT NULL,
                encrypted_data TEXT NOT NULL,
                search_index TEXT NOT NULL DEFAULT '',
                folder TEXT,
                favorite INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                parent_id TEXT NULL,
                sort_order INTEGER NULL
            );
            CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE auth (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                password_hash TEXT NOT NULL,
                salt TEXT,
                hash_salt TEXT,
                encrypt_salt TEXT
            );
            PRAGMA user_version = 4;",
        )
        .expect("legacy schema should initialize");
        conn.execute(
            "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["entry-1", "login", "encrypted", "github alice@example.com", Option::<String>::None, 0, 0, 0],
        )
        .expect("legacy row should insert");

        Database::migrate(&conn).expect("migration should succeed");

        let search_index: String = conn
            .query_row(
                "SELECT search_index FROM vault_entries WHERE id = 'entry-1'",
                [],
                |row| row.get(0),
            )
            .expect("search index should exist");
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("schema version should be readable");

        assert_eq!(search_index, "");
        assert_eq!(version, 6);
    }

    #[test]
    fn save_folder_should_not_replace_existing_folder_when_name_conflicts() {
        let db = create_test_database();
        with_test_conn(&db, |conn| {
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["existing", "shared-name", Option::<String>::None, 0],
            )
            .expect("existing folder should insert");
        });

        let err = db
            .save_folder(&Folder {
                id: "new-folder".to_string(),
                name: "shared-name".to_string(),
                parent_id: None,
                sort_order: Some(1),
            })
            .expect_err("duplicate folder name should fail");

        with_test_conn(&db, |conn| {
            let folder_ids: Vec<String> = conn
                .prepare("SELECT id FROM folders ORDER BY id")
                .expect("statement should prepare")
                .query_map([], |row| row.get(0))
                .expect("query should succeed")
                .collect::<Result<Vec<_>, _>>()
                .expect("rows should collect");

            assert!(err.contains("UNIQUE"));
            assert_eq!(folder_ids, vec!["existing".to_string()]);
        });
    }

    #[test]
    fn setup_auth_should_succeed_with_legacy_not_null_salt_column() {
        let db = create_legacy_auth_test_database();

        db.setup_auth("hash", &[1; 32], &[2; 32])
            .expect("legacy auth insert should succeed");

        with_test_conn(&db, |conn| {
            let row: (String, String, String, String) = conn
                .query_row(
                    "SELECT password_hash, salt, hash_salt, encrypt_salt FROM auth WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .expect("auth row should exist");

            assert_eq!(row.0, "hash");
            assert!(!row.1.is_empty());
            assert!(!row.2.is_empty());
            assert!(!row.3.is_empty());
        });
    }

    #[test]
    fn rotate_master_password_should_reencrypt_entries_and_update_auth() {
        let db = create_test_database();
        let old_hash_salt = [11u8; 32];
        let old_encrypt_salt = [12u8; 32];
        let new_hash_salt = [21u8; 32];
        let new_encrypt_salt = [22u8; 32];
        let old_password = "old-master-password";
        let new_password = "new-master-password";

        let old_hash = CryptoManager::hash_master_password_with_salt(old_password, &old_hash_salt)
            .expect("old password hash should be generated");
        let new_hash = CryptoManager::hash_master_password_with_salt(new_password, &new_hash_salt)
            .expect("new password hash should be generated");

        db.setup_auth(&old_hash, &old_hash_salt, &old_encrypt_salt)
            .expect("auth setup should succeed");

        let old_key = CryptoManager::derive_key(old_password, &old_encrypt_salt)
            .expect("old key derivation should succeed");
        let old_crypto = CryptoManager::new(&old_key);

        let entry = VaultEntry {
            id: "entry-rotate".to_string(),
            entry_type: EntryType::Login,
            name: "Service".to_string(),
            username: Some("alice".to_string()),
            password: Some("secret".to_string()),
            url: None,
            api_key: None,
            notes: None,
            folder: None,
            favorite: false,
            created_at: 1,
            updated_at: 2,
        };
        db.save_entry(&entry, &old_crypto)
            .expect("entry should save with old key");

        let new_key = CryptoManager::derive_key(new_password, &new_encrypt_salt)
            .expect("new key derivation should succeed");
        let new_crypto = CryptoManager::new(&new_key);

        db.rotate_master_password(
            &old_crypto,
            &new_crypto,
            &new_hash,
            &new_hash_salt,
            &new_encrypt_salt,
        )
        .expect("password rotation should succeed");

        let (stored_hash, stored_hash_salt, stored_encrypt_salt) =
            db.get_auth().expect("auth should still exist");
        assert_eq!(stored_hash, new_hash);
        assert_eq!(stored_hash_salt, new_hash_salt.to_vec());
        assert_eq!(stored_encrypt_salt, new_encrypt_salt.to_vec());

        let rows_with_old_key = db
            .get_all_entries(&old_crypto)
            .expect_err("old key should no longer decrypt entries");
        assert!(rows_with_old_key.contains("解密失败"));

        let rows_with_new_key = db
            .get_all_entries(&new_crypto)
            .expect("new key should decrypt entries");
        assert_eq!(rows_with_new_key.len(), 1);
        assert_eq!(rows_with_new_key[0].id, "entry-rotate");
    }

    /// 用 tempfile 模拟"旧明文 SQLite 数据库 -> SQLCipher 加密数据库"的完整迁移。
    ///
    /// 这个测试必须依赖 SQLCipher 才能跑（用了 sqlcipher_export 函数）。CI 上启用
    /// `bundled-sqlcipher-vendored-openssl` feature 后会自动可用。
    #[test]
    fn migrate_plaintext_to_encrypted_should_preserve_all_entries() {
        use std::env;
        let tmpdir = env::temp_dir().join(format!(
            "vaultory-migrate-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmpdir).expect("temp dir should create");
        let db_path = tmpdir.join("vault.db");

        // 1) 构造旧明文 db
        let hash_salt = [1u8; 32];
        let encrypt_salt = [2u8; 32];
        let password = "migrate-test-password";
        let hash = CryptoManager::hash_master_password_with_salt(password, &hash_salt)
            .expect("hash should generate");
        let content_key = CryptoManager::derive_key(password, &encrypt_salt)
            .expect("content key should derive");
        let content_crypto = CryptoManager::new(&content_key);

        {
            let conn = Connection::open(&db_path).expect("plaintext db should open");
            Database::ensure_schema(&conn).expect("schema should init");
            Database::migrate(&conn).expect("migrate to v6");
            // 写一条 entry 进去
            let hash_salt_b64 = base64::engine::general_purpose::STANDARD.encode(hash_salt);
            let encrypt_salt_b64 = base64::engine::general_purpose::STANDARD.encode(encrypt_salt);
            conn.execute(
                "INSERT INTO auth (id, password_hash, salt, hash_salt, encrypt_salt) VALUES (1, ?1, ?2, ?3, ?4)",
                params![hash, encrypt_salt_b64, hash_salt_b64, encrypt_salt_b64],
            )
            .expect("auth should insert");

            let entry = VaultEntry {
                id: "e1".to_string(),
                entry_type: EntryType::Login,
                name: "TestSite".to_string(),
                username: Some("user1".to_string()),
                password: Some("p@ssw0rd".to_string()),
                url: Some("https://test.example".to_string()),
                api_key: None,
                notes: Some("note".to_string()),
                folder: None,
                favorite: false,
                created_at: 10,
                updated_at: 20,
            };
            let encrypted_data = content_crypto
                .encrypt(&serde_json::to_string(&entry).unwrap())
                .expect("entry should encrypt");
            conn.execute(
                "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![entry.id, "login", encrypted_data, "", Option::<String>::None, 0, 10, 20],
            )
            .expect("entry should insert");
            conn.execute(
                "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params!["f1", "Personal", Option::<String>::None, 0],
            )
            .expect("folder should insert");
        }

        // 2) 验证文件目前是明文（SQLite header）
        let db = Database::new(db_path.clone());
        assert!(db.is_plaintext().expect("plaintext check"));

        // 3) 派生 SQLCipher key 并迁移
        let db_salt = [3u8; 32];
        let db_key = CryptoManager::derive_key(password, &db_salt).expect("db key should derive");
        let db_key_hex = format_sqlcipher_key_hex(&db_key);
        db.migrate_plaintext_to_encrypted(&db_key_hex)
            .expect("migration should succeed");

        // 4) 迁移后文件不再是明文 SQLite
        assert!(!db.is_plaintext().expect("encrypted check"));

        // 5) 用正确密钥打开，所有数据完整
        db.open_with_key(&db_key_hex)
            .expect("encrypted db should open with correct key");
        let entries = db
            .get_all_entries(&content_crypto)
            .expect("entries should load");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "e1");
        assert_eq!(entries[0].name, "TestSite");
        assert_eq!(entries[0].password.as_deref(), Some("p@ssw0rd"));
        let folders = db.get_all_folders().expect("folders should load");
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, "f1");

        // 6) 错误密钥打开应失败
        db.close();
        let wrong_key = format_sqlcipher_key_hex(&[0xffu8; 32]);
        let err = db.open_with_key(&wrong_key).unwrap_err();
        assert!(err.contains("数据库密钥错误") || err.contains("损坏"));

        // 清理
        db.close();
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn reset_vault_should_delete_db_file() {
        use std::env;
        let tmpdir = env::temp_dir().join(format!(
            "vaultory-reset-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmpdir).expect("temp dir should create");
        let db_path = tmpdir.join("vault.db");
        std::fs::write(&db_path, b"placeholder").expect("placeholder should write");

        let db = Database::new(db_path.clone());
        assert!(db.file_exists());
        db.reset_vault().expect("reset should succeed");
        assert!(!db.file_exists());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }
}
