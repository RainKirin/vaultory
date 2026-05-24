# Vaultory

[中文](README.md)

Vaultory is a secure, lightweight local desktop password manager built with Tauri + React + Rust. This repository contains the local edition: data is stored in a local SQLite database, no account is required, and passwords are not uploaded to a server.

## Current Release

- Version: `v1.0.0`
- Platform: Windows x64
- GitHub Release: <https://github.com/RainKirin/vaultory/releases/tag/v1.0.0>

## Download and Install

Open the [Releases](https://github.com/RainKirin/vaultory/releases) page and download the files from `v1.0.0`.

| File | Purpose |
| --- | --- |
| `Password.Manager_1.0.0_x64-setup.exe` | Recommended for most Windows users; double-click to install |
| `Password.Manager_1.0.0_x64_en-US.msi` | MSI package for environments that require MSI installers |
| `password-manager.exe` | Single-file app executable; run directly without an installer |
| `SHA256SUMS-v1.0.0.txt` | Checksums for verifying downloaded files |

System requirements: Windows 10/11 x64. The app is built with Tauri and requires the WebView2 Runtime; recent Windows 10/11 installations usually include it.

Note: `password-manager.exe` does not require an installer, but it is not a fully portable/green app. It still depends on the system WebView2 Runtime and stores vault data at `%APPDATA%\com.password-manager.app\vault.db`.

### Verify Downloads

After downloading `SHA256SUMS-v1.0.0.txt`, verify the installer with PowerShell:

```powershell
Get-FileHash "Password.Manager_1.0.0_x64-setup.exe" -Algorithm SHA256
```

Compare the hash with the matching line in `SHA256SUMS-v1.0.0.txt`. If the values do not match, download the file again and do not run it.

## First-Run Tutorial

1. Launch the app.
2. Set a master password with at least 8 characters.
3. Keep the master password safe; vault data cannot be recovered if it is lost.
4. After unlocking, click the new-entry button to add a password entry or API key.
5. Use Settings to create folders, switch languages, enable dark mode, and adjust the auto-lock timeout.
6. Lock the app manually when finished; it also locks automatically after the configured idle timeout.

## Daily Use

### Add Passwords or API Keys

On the vault page, click the new-entry button, choose an entry type, and fill in fields such as name, username, password, URL, API key, and notes. Saved entry content is encrypted before it is written to the local database.

### Organize Entries with Folders

Create folders and subfolders from Settings. Entries can be assigned to folders; selecting a folder in the sidebar shows entries in that folder and its descendants.

### Search and Favorites

The search box supports matching by name, username, URL, and API key. Mark important entries as favorites to show them first.

### Generate Passwords and Usernames

The generator page supports:

- Configurable password length.
- Uppercase, lowercase, number, and symbol options.
- Excluding ambiguous characters.
- Random username generation.

## Data Location, Backup, and Restore

On Windows, app data is usually stored at:

```text
%APPDATA%\com.password-manager.app\vault.db
```

Backup steps:

1. Close the app.
2. Copy `vault.db` to a safe location.
3. To restore, close the app and copy the backup file back to the same path.

Notes:

- You still need the master password to unlock backed-up data.
- Deleting `vault.db` resets the vault and starts first-time setup again.
- The current version encrypts password/API-key entry content; folder names and some app settings are stored as local SQLite metadata.

## Security Features

- AES-256-GCM authenticated encryption for vault entry content.
- Argon2id key derivation for master password hashing and encryption key derivation.
- Separate hash and encryption salts.
- Key copies held by the core crypto manager are zeroized when released.
- Critical database operations use transactions for atomicity.
- Repeated unlock failures trigger increasing wait times.
- Local app only: no account system and no remote sync.

## Run and Build from Source

Prerequisites:

- Node.js 20 LTS or newer.
- Rust stable toolchain.
- WebView2 Runtime on Windows.

Install dependencies:

```bash
npm install
```

Development mode:

```bash
npm run tauri dev
```

Build a release:

```bash
npm run tauri build
```

Build outputs are located at:

```text
src-tauri/target/release/password-manager.exe
src-tauri/target/release/bundle/nsis/Password Manager_1.0.0_x64-setup.exe
src-tauri/target/release/bundle/msi/Password Manager_1.0.0_x64_en-US.msi
```

These files are local build artifacts under `src-tauri/target/`. They are ignored by Git and are not committed to the repository.

## Tests

Frontend tests:

```bash
npm test
```

Rust tests:

```bash
cd src-tauri
cargo test
```

If stale local build cache causes the Tauri build script to read an old path, use a separate target directory:

```bash
cargo test --target-dir "%TEMP%\vaultory-cargo-test-target"
```

## Tech Stack

- Frontend: React 18 + TypeScript + Vite
- Desktop framework: Tauri 2
- Backend: Rust
- Database: SQLite + rusqlite
- Encryption: AES-256-GCM + Argon2id

## Database Structure

- `vault_entries`: encrypted vault entries
- `folders`: folders and subfolders
- `settings`: app settings
- `auth`: master password hash and salt information

The database schema may expand through versioned migrations. New versions run local migrations automatically on first launch.

## Troubleshooting

### Forgotten Master Password

The master password cannot be recovered. If you no longer need the old data, delete `%APPDATA%\com.password-manager.app\vault.db` and initialize the app again.

### Full Uninstall

Uninstall the app from Windows Settings, then delete local data under `%APPDATA%\com.password-manager.app\` if needed.

### Release Page Has No Files

Make sure you are opening the `v1.0.0` Release page: <https://github.com/RainKirin/vaultory/releases/tag/v1.0.0>.

## License

MIT
