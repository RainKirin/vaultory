#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod crypto;
mod db;
mod generators;
mod models;

use commands::*;
use db::Database;
use std::{error::Error, io, path::PathBuf, sync::Mutex};
use tauri::Manager;

fn main() {
    if let Err(error) = run() {
        let msg = format!("Application startup failed: {error}");
        eprintln!("{}", msg);
        if let Ok(appdata) = std::env::var("APPDATA") {
            let log_dir = PathBuf::from(appdata).join("com.password-manager.app");
            let _ = std::fs::create_dir_all(&log_dir);
            let _ = std::fs::write(log_dir.join("startup-error.log"), &msg);
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    tauri::Builder::default()
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            let db_path = app_dir.join("vault.db");
            let database = Database::new(&db_path).map_err(io::Error::other)?;

            app.manage(AppState {
                db: database,
                crypto: Mutex::new(None),
                unlock_attempts: Mutex::new(UnlockAttemptState::default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            is_initialized,
            setup_master_password,
            unlock,
            lock,
            change_master_password,
            reset_vault,
            get_all_entries,
            get_entry,
            save_entry,
            delete_entry,
            search_entries,
            generate_password_cmd,
            generate_username_cmd,
            get_settings,
            save_settings,
            get_folders,
            save_folder,
            delete_folder,
            get_entries_in_folder,
            consume_migration_notice,
        ])
        .run(tauri::generate_context!())?;

    Ok(())
}
