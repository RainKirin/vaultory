use crate::crypto::{self, CryptoManager};
use crate::db::Database;
use crate::generators;
use crate::models::*;
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
        .map_err(|_| "保险库状态不可用".to_string())?;
    if guard.is_some() {
        Ok(())
    } else {
        Err("保险库已锁定".to_string())
    }
}

#[tauri::command]
pub fn is_initialized(state: State<AppState>) -> Result<bool, String> {
    state.db.is_initialized()
}

#[tauri::command]
pub fn setup_master_password(password: String, state: State<AppState>) -> Result<(), String> {
    if password.len() < 8 {
        return Err("主密码至少需要 8 个字符".to_string());
    }
    if state.db.is_initialized()? {
        return Err("主密码已设置".to_string());
    }
    let hash_salt = crypto::generate_salt();
    let encrypt_salt = crypto::generate_salt();
    let hash = CryptoManager::hash_master_password_with_salt(&password, &hash_salt)?;
    state.db.setup_auth(&hash, &hash_salt, &encrypt_salt)?;

    let key = CryptoManager::derive_key(&password, &encrypt_salt)?;
    let cm = CryptoManager::new(&key);
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态不可用".to_string())? = Some(cm);

    let settings = AppSettings::default();
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
pub fn unlock(password: String, state: State<AppState>) -> Result<bool, String> {
    {
        let attempts = state
            .unlock_attempts
            .lock()
            .map_err(|_| "解锁状态不可用".to_string())?;
        if let Some(last) = attempts.last_failure {
            let delay = compute_backoff(attempts.failures);
            let elapsed = last.elapsed();
            if elapsed < delay {
                let remaining = (delay - elapsed).as_secs() + 1;
                return Err(format!("连续失败次数过多，请 {} 秒后再试", remaining));
            }
        }
    }

    let (hash, _hash_salt, encrypt_salt) = state.db.get_auth()?;
    let verified = CryptoManager::verify_master_password(&password, &hash)?;

    {
        let mut attempts = state
            .unlock_attempts
            .lock()
            .map_err(|_| "解锁状态不可用".to_string())?;
        if !verified {
            attempts.failures = attempts.failures.saturating_add(1);
            attempts.last_failure = Some(Instant::now());
            return Ok(false);
        }
        attempts.failures = 0;
        attempts.last_failure = None;
    }

    let key = CryptoManager::derive_key(&password, &encrypt_salt)?;
    let cm = CryptoManager::new(&key);
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态不可用".to_string())? = Some(cm);
    Ok(true)
}

#[tauri::command]
pub fn lock(state: State<AppState>) -> Result<(), String> {
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态不可用".to_string())? = None;
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
    let new_hash = CryptoManager::hash_master_password_with_salt(&new_password, &new_hash_salt)?;
    let new_key = CryptoManager::derive_key(&new_password, &new_encrypt_salt)?;
    let new_crypto = CryptoManager::new(&new_key);

    state.db.rotate_master_password(
        &old_crypto,
        &new_crypto,
        &new_hash,
        &new_hash_salt,
        &new_encrypt_salt,
    )?;

    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态不可用".to_string())? = Some(new_crypto);

    Ok(())
}

#[tauri::command]
pub fn reset_vault(state: State<AppState>) -> Result<(), String> {
    state.db.reset_vault()?;
    *state
        .crypto
        .lock()
        .map_err(|_| "保险库状态不可用".to_string())? = None;
    {
        let mut attempts = state
            .unlock_attempts
            .lock()
            .map_err(|_| "解锁状态不可用".to_string())?;
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
        .map_err(|_| "保险库状态不可用".to_string())?;
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
        return Err("自动锁定时间必须大于 0".to_string());
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
            return Err("不能把文件夹移到自己或后代下".into());
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
}
