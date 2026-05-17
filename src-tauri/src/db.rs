use crate::crypto::CryptoManager;
use crate::models::*;
use base64::Engine;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(db_path: &PathBuf) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|e| format!("打开数据库失败: {}", e))?;

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

        Self::migrate(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
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

    pub fn is_initialized(&self) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM auth", [], |row| row.get(0))
            .map_err(|_| "检查初始化状态失败".to_string())?;
        Ok(count > 0)
    }

    pub fn setup_auth(
        &self,
        password_hash: &str,
        hash_salt: &[u8],
        encrypt_salt: &[u8],
    ) -> Result<(), String> {
        let hash_salt_b64 = base64::engine::general_purpose::STANDARD.encode(hash_salt);
        let encrypt_salt_b64 = base64::engine::general_purpose::STANDARD.encode(encrypt_salt);
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
    }

    pub fn get_auth(&self) -> Result<(String, Vec<u8>, Vec<u8>), String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
        let encrypt_salt_b64 = encrypt_salt_b64
            .or(old_salt_b64)
            .ok_or("缺少加密 salt")?;

        let hash_salt = base64::engine::general_purpose::STANDARD
            .decode(&hash_salt_b64)
            .map_err(|_| "哈希 salt 无效".to_string())?;
        let encrypt_salt = base64::engine::general_purpose::STANDARD
            .decode(&encrypt_salt_b64)
            .map_err(|_| "加密 salt 无效".to_string())?;
        Ok((hash, hash_salt, encrypt_salt))
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

        let mut conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
            .map_err(|e| format!("更新保险库条目密文失败: {}", e))?;
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

        tx.commit()
            .map_err(|_| "提交密码轮换失败".to_string())?;
        Ok(())
    }

    pub fn save_entry(&self, entry: &VaultEntry, crypto: &CryptoManager) -> Result<(), String> {
        let data = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        let encrypted = crypto.encrypt(&data)?;
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
    }

    pub fn get_all_entries(&self, crypto: &CryptoManager) -> Result<Vec<VaultEntry>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
            let entry: VaultEntry = serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub fn get_entry(
        &self,
        id: &str,
        crypto: &CryptoManager,
    ) -> Result<Option<VaultEntry>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
    }

    pub fn delete_entry(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
        conn.execute("DELETE FROM vault_entries WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
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
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok()
        .map_or(Ok(None), |v: String| Ok(Some(v)))
    }

    pub fn delete_setting(&self, key: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
            .map_err(|e| format!("删除设置失败: {}", e))?;
        Ok(())
    }

    pub fn save_folder(&self, folder: &Folder) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
    }

    pub fn get_all_folders(&self) -> Result<Vec<Folder>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
    }

    pub fn get_folder_parent(&self, id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
        let result: Option<String> = conn
            .query_row(
                "SELECT parent_id FROM folders WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        Ok(result)
    }

    pub fn get_entries_in_folder(
        &self,
        folder_id: &str,
        include_descendants: bool,
        crypto: &CryptoManager,
    ) -> Result<Vec<VaultEntry>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
            let entry: VaultEntry = serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub fn delete_folder_with_transaction(&self, id: &str, strategy: &str) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
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
                    .map_err(|_| "读取子孙文件夹失败".to_string())?
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
                .map_err(|_| "删除所有文件夹失败".to_string())?;
            }
            _ => return Err("未知策略".to_string()),
        }

        tx.commit()
            .map_err(|_| "提交事务失败".to_string())?;
        Ok(())
    }

    pub fn reset_vault(&self) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|_| "数据库连接不可用".to_string())?;
        conn.execute_batch("PRAGMA secure_delete = ON;")
            .map_err(|e| format!("启用安全删除失败: {}", e))?;

        let tx = conn
            .transaction()
            .map_err(|_| "启动重置事务失败".to_string())?;
        tx.execute("DELETE FROM vault_entries", [])
            .map_err(|e| format!("清空保险库条目失败: {}", e))?;
        tx.execute("DELETE FROM folders", [])
            .map_err(|e| format!("清空文件夹失败: {}", e))?;
        tx.execute("DELETE FROM settings", [])
            .map_err(|e| format!("清空设置失败: {}", e))?;
        tx.execute("DELETE FROM auth", [])
            .map_err(|e| format!("清空认证数据失败: {}", e))?;

        tx.commit()
            .map_err(|_| "提交保险库重置失败".to_string())?;
        conn.execute_batch("VACUUM;")
            .map_err(|e| format!("VACUUM 失败: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::CryptoManager;
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

        Database {
            conn: Mutex::new(conn),
        }
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

        Database {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn delete_folder_with_transaction_should_preserve_nested_hierarchy_when_merging_up() {
        let db = create_test_database();
        let conn = db.conn.lock().expect("database mutex should lock");

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
        drop(conn);

        db.delete_folder_with_transaction("a", "merge_up")
            .expect("merge_up should succeed");

        let conn = db.conn.lock().expect("database mutex should lock");
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

        let conn = db.conn.lock().expect("database mutex should lock");
        conn.execute(
            "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![entry.id, "login", encrypted, "", Option::<String>::None, 0, 0, 0],
        )
        .expect("legacy row should insert");
        drop(conn);

        let matches = db
            .search_entries("github", &crypto)
            .expect("search should succeed");

        let conn = db.conn.lock().expect("database mutex should lock");
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
    }

    #[test]
    fn delete_folder_with_transaction_should_delete_entries_when_cascading() {
        let db = create_test_database();
        let conn = db.conn.lock().expect("database mutex should lock");

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
        drop(conn);

        db.delete_folder_with_transaction("a", "cascade")
            .expect("cascade should succeed");

        let conn = db.conn.lock().expect("database mutex should lock");
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
        assert_eq!(version, 5);
    }

    #[test]
    fn save_folder_should_not_replace_existing_folder_when_name_conflicts() {
        let db = create_test_database();
        let conn = db.conn.lock().expect("database mutex should lock");
        conn.execute(
            "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
            params!["existing", "shared-name", Option::<String>::None, 0],
        )
        .expect("existing folder should insert");
        drop(conn);

        let err = db
            .save_folder(&Folder {
                id: "new-folder".to_string(),
                name: "shared-name".to_string(),
                parent_id: None,
                sort_order: Some(1),
            })
            .expect_err("duplicate folder name should fail");

        let conn = db.conn.lock().expect("database mutex should lock");
        let folder_ids: Vec<String> = conn
            .prepare("SELECT id FROM folders ORDER BY id")
            .expect("statement should prepare")
            .query_map([], |row| row.get(0))
            .expect("query should succeed")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows should collect");

        assert!(err.contains("UNIQUE"));
        assert_eq!(folder_ids, vec!["existing".to_string()]);
    }

    #[test]
    fn setup_auth_should_succeed_with_legacy_not_null_salt_column() {
        let db = create_legacy_auth_test_database();

        db.setup_auth("hash", &[1; 32], &[2; 32])
            .expect("legacy auth insert should succeed");

        let conn = db.conn.lock().expect("database mutex should lock");
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

    #[test]
    fn reset_vault_should_remove_all_sensitive_data() {
        let db = create_test_database();

        db.setup_auth("hash", &[1; 32], &[2; 32])
            .expect("auth setup should succeed");
        db.save_setting("language", "zh")
            .expect("setting should save");

        let conn = db.conn.lock().expect("database mutex should lock");
        conn.execute(
            "INSERT INTO folders (id, name, parent_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
            params!["folder-1", "folder-1", Option::<String>::None, 0],
        )
        .expect("folder should insert");
        conn.execute(
            "INSERT INTO vault_entries (id, entry_type, encrypted_data, search_index, folder, favorite, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                "entry-reset",
                "login",
                "encrypted",
                "",
                "folder-1",
                0,
                0,
                0
            ],
        )
        .expect("entry should insert");
        drop(conn);

        db.reset_vault().expect("vault reset should succeed");

        let conn = db.conn.lock().expect("database mutex should lock");
        let auth_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM auth", [], |row| row.get(0))
            .expect("auth count query should succeed");
        let entry_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM vault_entries", [], |row| row.get(0))
            .expect("entry count query should succeed");
        let folder_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM folders", [], |row| row.get(0))
            .expect("folder count query should succeed");
        let setting_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .expect("setting count query should succeed");
        drop(conn);

        assert_eq!(auth_count, 0);
        assert_eq!(entry_count, 0);
        assert_eq!(folder_count, 0);
        assert_eq!(setting_count, 0);
        assert!(!db.is_initialized().expect("init check should succeed"));
    }
}
