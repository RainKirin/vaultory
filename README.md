# 玄匣 Vaultory

[English](README_EN.md)

安全、轻量的本地密码管理桌面应用，基于 Tauri + React + Rust 构建。

## 版本说明

当前仓库为本地版：纯本地存储，数据保存在本地 SQLite 数据库，无需网络。

## 功能特性

- 🔐 **主密码保护** - Argon2id 密钥派生 + AES-256-GCM 加密
- 📁 **文件夹管理** - 支持多级嵌套文件夹分类
- 🔍 **智能搜索** - 快速搜索名称、用户名、网址、API Key
- 🎲 **密码生成器** - 可配置长度、字符类型、排除易混淆字符
- 👤 **用户名生成器** - 生成随机用户名
- 🌙 **深色模式** - 一键切换明暗主题
- 🌐 **多语言** - 支持中文/英文界面
- ⏱️ **自动锁定** - 可配置空闲自动锁定时间
- ⭐ **收藏标记** - 快速访问重要条目

## 安全特性

- ✅ AES-256-GCM 认证加密（条目级别）
- ✅ SQLCipher 整库加密（数据库元数据也加密）
- ✅ Argon2id 密钥派生（抗暴力破解）
- ✅ 分离的哈希/加密/数据库 Salt（防彩虹表攻击）
- ✅ 内存密钥自动清零（防内存转储攻击）
- ✅ 锁定时关闭数据库连接（磁盘上只剩密文，进程内无可用句柄）
- ✅ 数据库事务保证操作原子性
- ✅ 敏感错误信息不泄露

## 加密设计

双层加密架构：

```
主密码 ──┬── + hash_salt    → Argon2id → PHC 哈希 (验证主密码)
         ├── + encrypt_salt → Argon2id → 32B → AES-256-GCM 密钥 (条目内容)
         └── + db_salt      → Argon2id → 32B → SQLCipher 主密钥 (整库)
```

三个盐独立生成，互不影响。即使 SQLCipher 层被攻破，条目内容仍受 AES-GCM 保护；反之亦然。

`db_salt` 存储于数据库旁的 `vault.meta.json` 中（盐本身不是秘密）。其他两个盐存储在加密的 `auth` 表内。

## 快速开始

### 直接使用

运行发布包中的可执行文件即可启动。

首次启动需要设置主密码，请妥善保管，**丢失后无法恢复数据**。

### 校验下载的二进制（推荐）

每个 Release 都会附带 `.sha256` 校验文件。下载后请先验证完整性，避免下载被篡改或网络损坏的文件：

**Windows PowerShell:**
```powershell
# 假设下载了 password-manager.exe 和 password-manager.exe.sha256
$expected = (Get-Content password-manager.exe.sha256).Split(' ')[0]
$actual = (Get-FileHash password-manager.exe -Algorithm SHA256).Hash.ToLower()
if ($expected -eq $actual) { "OK: $actual" } else { "MISMATCH" }
```

**Linux / macOS / WSL:**
```bash
sha256sum -c password-manager.exe.sha256
```

### 从源码构建

**前置依赖**: Node.js 20+、Rust stable、**Perl**（SQLCipher 的 vendored OpenSSL 编译需要，Windows 推荐 [Strawberry Perl](https://strawberryperl.com/)）。

```bash
# 安装依赖
npm install

# 开发模式
npm run tauri dev

# 构建发布版本
npm run tauri build
```

构建产物位于 `src-tauri/target/release/` 目录。

> 首次构建会从源码编译 OpenSSL 和 SQLCipher，耗时约 5–10 分钟。后续增量构建走 cargo 缓存，秒级完成。

## 技术栈

- **前端**: React 18 + TypeScript + Vite
- **后端**: Rust + Tauri 2
- **数据库**: SQLite + SQLCipher 整库加密（rusqlite with `bundled-sqlcipher-vendored-openssl`）
- **加密**: AES-256-GCM (条目层) + SQLCipher (整库层) + Argon2id (密钥派生)

## 数据库结构

- `vault_entries` - AES-GCM 加密的密码条目（外层再被 SQLCipher 整库加密）
- `folders` - 文件夹分类（支持嵌套）
- `settings` - 应用设置
- `auth` - 主密码 Argon2 PHC 哈希、hash_salt、encrypt_salt
- `vault.meta.json` (sidecar) - 数据库密钥派生需要的 `db_salt`、meta 版本号

## 数据库迁移

- 从仍是明文 SQLite 的旧版本升级时，**首次解锁会自动迁移到 SQLCipher 加密格式**（使用 `sqlcipher_export`，临时文件 + 原子 rename）。
- 后续版本间的 schema 升级在 `db.rs::migrate` 内基于 `PRAGMA user_version` 做幂等处理。
- 如果遇到问题，可删除 `%APPDATA%\com.password-manager.app\vault.db` 和 `vault.meta.json` 重新创建（**会丢失数据**）。
