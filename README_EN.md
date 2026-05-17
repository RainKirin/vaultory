# Password Manager 3.1

A secure, lightweight desktop password management application built with Tauri + React + Rust.

## Versions

| Version | Description |
|---------|-------------|
| **Local** | Pure local storage, data saved in local SQLite database, no network required |
| **GitHub** | Supports data sync via GitHub, suitable for multi-device usage |

## Features

- 🔐 **Master Password Protection** - Argon2id key derivation + AES-256-GCM encryption
- 📁 **Folder Management** - Multi-level nested folder classification
- 🔍 **Smart Search** - Quick search by name, username, URL, API Key
- 🎲 **Password Generator** - Configurable length, character types, exclude ambiguous characters
- 👤 **Username Generator** - Generate random usernames
- 🌙 **Dark Mode** - One-click toggle between light and dark themes
- 🌐 **Multi-language** - Supports Chinese/English interface
- ⏱️ **Auto-Lock** - Configurable idle auto-lock time
- ⭐ **Favorites** - Quick access to important entries

## Security Features

- ✅ AES-256-GCM authenticated encryption
- ✅ Argon2id key derivation (brute-force resistant)
- ✅ Separate hash/encryption salts (rainbow table attack resistant)
- ✅ Automatic memory key zeroization (memory dump attack resistant)
- ✅ Database transactions ensure operation atomicity
- ✅ Sensitive error messages are not leaked

## Quick Start

### Direct Usage

Double-click `password-manager.exe` to run.

On first launch, you need to set a master password. Please keep it safe, **data cannot be recovered if lost**.

### Build from Source

```bash
# Install dependencies
npm install

# Development mode
npm run tauri dev

# Build release version
npm run tauri build
```

Build artifacts are located in `src-tauri/target/release/` directory.

## Tech Stack

- **Frontend**: React 18 + TypeScript + Vite
- **Backend**: Rust + Tauri 2
- **Database**: SQLite (rusqlite)
- **Encryption**: AES-256-GCM + Argon2id

## Database Structure

- `vault_entries` - Encrypted password entries
- `folders` - Folder categories (supports nesting)
- `settings` - Application settings
- `auth` - Authentication info (password hash + salts)

## Database Migration

The first time you run the new version, the old database will be automatically migrated. If you encounter issues, you can delete `%APPDATA%\com.password-manager.app\vault.db` to recreate it (**data will be lost**).

## License

MIT
