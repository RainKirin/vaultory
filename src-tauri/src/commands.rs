use crate::crypto::{self, CryptoManager};
use crate::db::Database;
use crate::generators;
use crate::models::*;
use base64::Engine;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::State;

pub struct AppState {
    pub db: Database,
    pub crypto: Mutex<Option<CryptoManager>>,
    pub unlock_attempts: Mutex<UnlockAttemptState>,
}

#[derive(Default)]
pub struct UnlockAttemptState {
    pub failures: u32,
    pub last_failure: Option<Instant>,
}

/// 应用磁盘上的元数据文件。
///
/// SQLCipher 整库加密把所有数据库内容（包括 schema、auth 表里的盐）都密文化了，
/// 而打开加密数据库本身又需要主密钥——这是一个鸡生蛋问题。所以我们把"派生整库
/// 密钥所需的盐"独立存到一个非敏感的 sidecar 文件里。盐本身公开无害。
#[derive(Serialize, Deserialize)]
struct VaultMeta {
    version: u32,
    db_salt_b64: String,
}

const VAULT_META_VERSION: u32 = 1;

fn meta_path(db_path: &Path) -> PathBuf {
    db_path.with_file_name("vault.meta.json")
}

fn read_meta(db_path: &Path) -> Result<Option<VaultMeta>, String> {
    let p = meta_path(db_path);
    if !p.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&p).map_err(|e| format!("读取元数据失败: {}", e))?;
    let meta: VaultMeta =
        serde_json::from_str(&content).map_err(|e| format!("解析元数据失败: {}", e))?;
    Ok(Some(meta))
}

fn write_meta(db_path: &Path, meta: &VaultMeta) -> Result<(), String> {
    let p = meta_path(db_path);
    let content =
        serde_json::to_string_pretty(meta).map_err(|e| format!("序列化元数据失败: {}", e))?;
    std::fs::write(&p, content).map_err(|e| format!("写入元数据失败: {}", e))?;
    Ok(())
}

fn delete_meta(db_path: &Path) -> Result<(), String> {
    let p = meta_path(db_path);
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| format!("删除元数据失败: {}", e))?;
    }
    Ok(())
}

fn make_meta(db_salt: &[u8; 32]) -> VaultMeta {
    VaultMeta {
        version: VAULT_META_VERSION,
        db_salt_b64: base64::engine::general_purpose::STANDARD.encode(db_salt),
    }
}

fn parse_db_salt(meta: &VaultMeta) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(&meta.db_salt_b64)
        .map_err(|_| "元数据中的数据库 salt 无效".to_string())
}

fn compute_backoff(failures: u32) -> Duration {
    if failures < 2 {
        return Duration::from_millis(0);
    }
    let secs = (1u64 << (failures - 2).min(6)).min(60);
    Duration::from_secs(secs)
}

fn require_unlocked(state: &State<AppState>) -> Result<(), String> {
    let guard = state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())?;
    if guard.is_some() {
        Ok(())
    } else {
        Err("保险库已锁定".to_string())
    }
}

fn record_failure(state: &State<AppState>) -> Result<(), String> {
    let mut attempts = state
        .unlock_attempts
        .lock()
        .map_err(|_| "尝试状态被污染".to_string())?;
    attempts.failures = attempts.failures.saturating_add(1);
    attempts.last_failure = Some(Instant::now());
    Ok(())
}

fn clear_failures(state: &State<AppState>) -> Result<(), String> {
    let mut attempts = state
        .unlock_attempts
        .lock()
        .map_err(|_| "尝试状态被污染".to_string())?;
    attempts.failures = 0;
    attempts.last_failure = None;
    Ok(())
}

#[tauri::command]
pub fn is_initialized(state: State<AppState>) -> Result<bool, String> {
    // SQLCipher 改造后无法在没有主密码的情况下访问 auth 表，所以"是否已初始化"
    // 改为通过文件存在性判断：数据库文件存在即视为已初始化，meta 文件缺失代表
    // 这是上一版的明文数据库，需要在 unlock 流程里走迁移。
    Ok(state.db.file_exists())
}

#[tauri::command]
pub fn setup_master_password(password: String, state: State<AppState>) -> Result<(), String> {
    if password.len() < 8 {
        return Err("主密码至少需要 8 个字符".to_string());
    }
    if state.db.file_exists() {
        return Err("保险库已初始化".to_string());
    }

    let hash_salt = crypto::generate_salt();
    let encrypt_salt = crypto::generate_salt();
    let db_salt = crypto::generate_salt();

    let hash = CryptoManager::hash_master_password_with_salt(&password, &hash_salt)?;
    let db_key = CryptoManager::derive_key(&password, &db_salt)?;
    let db_key_hex = crypto::format_sqlcipher_key_hex(&db_key);
    let content_key = CryptoManager::derive_key(&password, &encrypt_salt)?;

    // 先写 meta，再创建加密 db。如果 db 创建失败，meta 残留也没影响（下次启动
    // 会发现 db 不存在，is_initialized 返回 false，重新走 setup）。
    let meta = make_meta(&db_salt);
    write_meta(state.db.path(), &meta)?;

    // open_with_key 在 db 文件不存在时会创建它，并通过 ensure_schema + migrate 初始化 schema。
    if let Err(e) = state.db.open_with_key(&db_key_hex) {
        // 创建失败时清理 meta，避免下次启动时进入"已初始化但无 db"的怪状态
        let _ = delete_meta(state.db.path());
        return Err(e);
    }

    state.db.setup_auth(&hash, &hash_salt, &encrypt_salt)?;

    let settings = AppSettings::default();
    state
        .db
        .save_setting("auto_lock_minutes", &settings.auto_lock_minutes.to_string())?;
    state
        .db
        .save_setting("dark_mode", &settings.dark_mode.to_string())?;
    state.db.save_setting("language", &settings.language)?;

    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())? = Some(CryptoManager::new(&content_key));

    Ok(())
}

#[tauri::command]
pub fn unlock(password: String, state: State<AppState>) -> Result<bool, String> {
    {
        let attempts = state
            .unlock_attempts
            .lock()
            .map_err(|_| "尝试状态被污染".to_string())?;
        if let Some(last) = attempts.last_failure {
            let delay = compute_backoff(attempts.failures);
            let elapsed = last.elapsed();
            if elapsed < delay {
                let remaining = (delay - elapsed).as_secs() + 1;
                return Err(format!("尝试失败次数过多，请 {} 秒后再试", remaining));
            }
        }
    }

    if !state.db.file_exists() {
        return Err("保险库尚未初始化".to_string());
    }

    let meta = read_meta(state.db.path())?;

    match meta {
        Some(meta) => unlock_encrypted(&password, &meta, &state),
        None => {
            // meta 缺失只有一种合法情形：从旧明文版本升级上来的第一次解锁。
            // 如果 db 已经是 SQLCipher 加密格式但 meta 不见了，说明 meta 文件被
            // 误删——给用户一个明确错误，而不是把它当成"密码错误"反复重试。
            if !state.db.is_plaintext()? {
                return Err(
                    "vault.meta.json 缺失，但数据库已是加密格式。请勿删除该文件——\
                     如果仍有备份请放回原位置；否则当前密码无法解密数据库。"
                        .to_string(),
                );
            }
            unlock_legacy_and_migrate(&password, &state)
        }
    }
}

/// 新版（SQLCipher 加密）数据库的解锁路径。
fn unlock_encrypted(
    password: &str,
    meta: &VaultMeta,
    state: &State<AppState>,
) -> Result<bool, String> {
    let db_salt = parse_db_salt(meta)?;
    let db_key = CryptoManager::derive_key(password, &db_salt)?;
    let db_key_hex = crypto::format_sqlcipher_key_hex(&db_key);

    // open_with_key 内部会用一次 SELECT 验证密钥是否能正确解密 schema 页。
    // 错误密码下这一步会失败，于是我们把错误归一化为"密码错误"语义返回 false。
    if state.db.open_with_key(&db_key_hex).is_err() {
        state.db.close();
        record_failure(state)?;
        return Ok(false);
    }

    // 再用 auth 表中的 PHC 哈希做一次密码校验。理论上 SQLCipher 解密成功就足以
    // 证明密码正确，但保留 Argon2 哈希校验作为冗余防御，与历史行为保持一致，
    // 同时避免"密钥能解密但 hash_salt/encrypt_salt 已损坏"的诡异状态被误判成解锁成功。
    let (stored_hash, _hash_salt, encrypt_salt) = match state.db.get_auth() {
        Ok(v) => v,
        Err(_) => {
            state.db.close();
            record_failure(state)?;
            return Ok(false);
        }
    };
    if !CryptoManager::verify_master_password(password, &stored_hash)? {
        state.db.close();
        record_failure(state)?;
        return Ok(false);
    }

    clear_failures(state)?;
    let content_key = CryptoManager::derive_key(password, &encrypt_salt)?;
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())? = Some(CryptoManager::new(&content_key));

    Ok(true)
}

/// 旧版（明文 SQLite）数据库的解锁 + 迁移路径。
///
/// 检测条件：数据库文件存在但 meta 文件缺失。这种情况只发生在升级到 SQLCipher
/// 改造版的第一次解锁。流程：
///   1. 直接读旧明文 db 的 auth 行
///   2. 用 Argon2 PHC 验证密码（密码错误立即返回，不动数据库）
///   3. 密码正确 → 生成新 db_salt → 派生新 SQLCipher key
///   4. 调用 Database::migrate_plaintext_to_encrypted 用 sqlcipher_export 重写文件
///   5. 写 meta；用新 key 打开数据库；派生 content key
fn unlock_legacy_and_migrate(password: &str, state: &State<AppState>) -> Result<bool, String> {
    let old_conn = Connection::open(state.db.path())
        .map_err(|e| format!("打开旧数据库失败: {}", e))?;
    let row: (String, Option<String>, Option<String>, Option<String>) = old_conn
        .query_row(
            "SELECT password_hash, hash_salt, encrypt_salt, salt FROM auth WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| "未找到旧版认证数据".to_string())?;
    drop(old_conn);

    let (stored_hash, _hash_salt_b64, encrypt_salt_b64, old_salt_b64) = row;
    let encrypt_salt_b64 = encrypt_salt_b64
        .or(old_salt_b64)
        .ok_or("旧数据库缺少加密 salt")?;

    if !CryptoManager::verify_master_password(password, &stored_hash)? {
        record_failure(state)?;
        return Ok(false);
    }

    clear_failures(state)?;

    // 派生 SQLCipher 整库密钥，开始把明文 db 转换为加密 db。
    let db_salt = crypto::generate_salt();
    let db_key = CryptoManager::derive_key(password, &db_salt)?;
    let db_key_hex = crypto::format_sqlcipher_key_hex(&db_key);

    // 先写 meta 确认磁盘权限/空间正常——如果连一个小 JSON 文件都写不下去，
    // 后续的整库迁移更不可能成功，提前失败比把数据库迁移到一半再失败安全得多。
    write_meta(state.db.path(), &make_meta(&db_salt))?;

    // 迁移本身用临时文件 + 原子 rename 实现，不会留下半态文件。但如果迁移失败，
    // 必须把 meta 清掉，否则下次启动 unlock_encrypted 会因为"meta 存在但 db 仍是明文"
    // 而把密码当成错的。
    if let Err(e) = state.db.migrate_plaintext_to_encrypted(&db_key_hex) {
        let _ = delete_meta(state.db.path());
        return Err(e);
    }

    state.db.open_with_key(&db_key_hex)?;

    let encrypt_salt = base64::engine::general_purpose::STANDARD
        .decode(&encrypt_salt_b64)
        .map_err(|_| "旧数据库加密 salt 无效".to_string())?;
    let content_key = CryptoManager::derive_key(password, &encrypt_salt)?;
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())? = Some(CryptoManager::new(&content_key));

    Ok(true)
}

#[tauri::command]
pub fn lock(state: State<AppState>) -> Result<(), String> {
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())? = None;
    state.db.close();
    Ok(())
}

#[tauri::command]
pub fn change_master_password(
    current_password: String,
    new_password: String,
    state: State<AppState>,
) -> Result<(), String> {
    if new_password.len() < 8 {
        return Err("主密码至少需要 8 个字符".to_string());
    }

    let (stored_hash, _hash_salt, encrypt_salt) = state.db.get_auth()?;
    let verified = CryptoManager::verify_master_password(&current_password, &stored_hash)?;
    if !verified {
        return Err("当前主密码不正确".to_string());
    }

    let old_key = CryptoManager::derive_key(&current_password, &encrypt_salt)?;
    let old_crypto = CryptoManager::new(&old_key);

    let new_hash_salt = crypto::generate_salt();
    let new_encrypt_salt = crypto::generate_salt();
    let new_db_salt = crypto::generate_salt();

    let new_hash = CryptoManager::hash_master_password_with_salt(&new_password, &new_hash_salt)?;
    let new_content_key = CryptoManager::derive_key(&new_password, &new_encrypt_salt)?;
    let new_content_crypto = CryptoManager::new(&new_content_key);
    let new_db_key = CryptoManager::derive_key(&new_password, &new_db_salt)?;
    let new_db_key_hex = crypto::format_sqlcipher_key_hex(&new_db_key);

    // 1) AES-GCM 内容层：把每条 entry 用新 content key 重新加密 + 更新 auth 行。
    state.db.rotate_master_password(
        &old_crypto,
        &new_content_crypto,
        &new_hash,
        &new_hash_salt,
        &new_encrypt_salt,
    )?;

    // 2) SQLCipher 整库层：PRAGMA rekey 直接用新密钥重写每一页。
    state.db.rekey(&new_db_key_hex)?;

    // 3) 更新 meta，让下次启动用新 db_salt 派生 SQLCipher key。
    write_meta(state.db.path(), &make_meta(&new_db_salt))?;

    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())? = Some(new_content_crypto);

    Ok(())
}

#[tauri::command]
pub fn reset_vault(state: State<AppState>) -> Result<(), String> {
    state.db.reset_vault()?;
    delete_meta(state.db.path())?;
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())? = None;
    {
        let mut attempts = state
            .unlock_attempts
            .lock()
            .map_err(|_| "尝试状态被污染".to_string())?;
        attempts.failures = 0;
        attempts.last_failure = None;
    }
    Ok(())
}

fn with_crypto<F, R>(state: &State<AppState>, f: F) -> Result<R, String>
where
    F: FnOnce(&CryptoManager) -> Result<R, String>,
{
    let guard = state
        .crypto
        .lock()
        .map_err(|_| "保险库状态被污染".to_string())?;
    match guard.as_ref() {
        Some(cm) => f(cm),
        None => Err("保险库已锁定".to_string()),
    }
}

#[tauri::command]
pub fn get_all_entries(state: State<AppState>) -> Result<Vec<VaultEntry>, String> {
    with_crypto(&state, |cm| state.db.get_all_entries(cm))
}

#[tauri::command]
pub fn get_entry(id: String, state: State<AppState>) -> Result<Option<VaultEntry>, String> {
    with_crypto(&state, |cm| state.db.get_entry(&id, cm))
}

#[tauri::command]
pub fn save_entry(mut entry: VaultEntry, state: State<AppState>) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    if entry.id.is_empty() {
        entry.id = uuid::Uuid::new_v4().to_string();
        entry.created_at = now;
    }
    entry.updated_at = now;
    with_crypto(&state, |cm| state.db.save_entry(&entry, cm))
}

#[tauri::command]
pub fn delete_entry(id: String, state: State<AppState>) -> Result<(), String> {
    require_unlocked(&state)?;
    state.db.delete_entry(&id)
}

#[tauri::command]
pub fn search_entries(query: String, state: State<AppState>) -> Result<Vec<VaultEntry>, String> {
    with_crypto(&state, |cm| state.db.search_entries(&query, cm))
}

#[tauri::command]
pub fn generate_password_cmd(config: PasswordGeneratorConfig) -> GeneratedPassword {
    generators::generate_password(&config)
}

#[tauri::command]
pub fn generate_username_cmd(config: UsernameConfig) -> String {
    generators::generate_username(&config)
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    let auto_lock = state.db.get_setting("auto_lock_minutes")?;
    let dark_mode = state.db.get_setting("dark_mode")?;
    let language = state
        .db
        .get_setting("language")?
        .unwrap_or_else(|| "zh".to_string());

    let mut auto_lock_minutes: u32 = auto_lock.and_then(|v| v.parse().ok()).unwrap_or(5);
    if auto_lock_minutes == 0 {
        auto_lock_minutes = 5;
        state.db.save_setting("auto_lock_minutes", "5")?;
        state.db.save_setting("auto_lock_migrated", "1")?;
    }

    Ok(AppSettings {
        auto_lock_minutes,
        dark_mode: dark_mode.and_then(|v| v.parse().ok()).unwrap_or(false),
        language,
    })
}

#[tauri::command]
pub fn save_settings(settings: AppSettings, state: State<AppState>) -> Result<(), String> {
    require_unlocked(&state)?;
    if settings.auto_lock_minutes == 0 {
        return Err("自动锁定时间不能为 0".to_string());
    }
    state
        .db
        .save_setting("auto_lock_minutes", &settings.auto_lock_minutes.to_string())?;
    state
        .db
        .save_setting("dark_mode", &settings.dark_mode.to_string())?;
    state.db.save_setting("language", &settings.language)?;
    Ok(())
}

#[tauri::command]
pub fn get_folders(state: State<AppState>) -> Result<Vec<Folder>, String> {
    require_unlocked(&state)?;
    state.db.get_all_folders()
}

#[tauri::command]
pub fn save_folder(folder: Folder, state: State<AppState>) -> Result<(), String> {
    require_unlocked(&state)?;
    if let Some(pid) = &folder.parent_id {
        if would_create_cycle(&folder.id, pid, &state.db)? {
            return Err("不能把文件夹移到自己的子树".into());
        }
    }
    state.db.save_folder(&folder)
}

fn would_create_cycle(self_id: &str, new_parent: &str, db: &Database) -> Result<bool, String> {
    let mut cur = Some(new_parent.to_string());
    while let Some(id) = cur {
        if id == self_id {
            return Ok(true);
        }
        cur = db.get_folder_parent(&id)?;
    }
    Ok(false)
}

#[tauri::command]
pub fn delete_folder(id: String, strategy: String, state: State<AppState>) -> Result<(), String> {
    require_unlocked(&state)?;
    state.db.delete_folder_with_transaction(&id, &strategy)
}

#[tauri::command]
pub fn get_entries_in_folder(
    folder_id: String,
    include_descendants: bool,
    state: State<AppState>,
) -> Result<Vec<VaultEntry>, String> {
    with_crypto(&state, |cm| {
        state
            .db
            .get_entries_in_folder(&folder_id, include_descendants, cm)
    })
}

#[tauri::command]
pub fn consume_migration_notice(state: State<AppState>) -> Result<Option<String>, String> {
    match state.db.get_setting("auto_lock_migrated")? {
        Some(_) => {
            state.db.delete_setting("auto_lock_migrated")?;
            Ok(Some(
                "旧版「永不锁定」选项已移除，自动锁定已调整为 5 分钟，可在设置中修改。"
                    .to_string(),
            ))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_backoff_should_grow_exponentially_and_cap_at_60s() {
        assert_eq!(compute_backoff(0), Duration::from_secs(0));
        assert_eq!(compute_backoff(1), Duration::from_secs(0));
        assert_eq!(compute_backoff(2), Duration::from_secs(1));
        assert_eq!(compute_backoff(3), Duration::from_secs(2));
        assert_eq!(compute_backoff(4), Duration::from_secs(4));
        assert_eq!(compute_backoff(5), Duration::from_secs(8));
        assert_eq!(compute_backoff(6), Duration::from_secs(16));
        assert_eq!(compute_backoff(7), Duration::from_secs(32));
        assert_eq!(compute_backoff(8), Duration::from_secs(60));
        assert_eq!(compute_backoff(100), Duration::from_secs(60));
    }

    #[test]
    fn meta_roundtrip_should_preserve_salt() {
        use std::env;
        let tmpdir = env::temp_dir().join(format!(
            "vaultory-meta-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmpdir).expect("temp dir should create");
        let db_path = tmpdir.join("vault.db");

        let salt = [42u8; 32];
        write_meta(&db_path, &make_meta(&salt)).expect("meta should write");
        let read = read_meta(&db_path).expect("meta should read").unwrap();
        let parsed = parse_db_salt(&read).expect("salt should parse");
        assert_eq!(parsed, salt.to_vec());
        assert_eq!(read.version, VAULT_META_VERSION);

        delete_meta(&db_path).expect("meta should delete");
        assert!(read_meta(&db_path).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }
}
