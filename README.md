# 玄匣 Vaultory

[English](README_EN.md)

玄匣 Vaultory 是一个安全、轻量的本地密码管理桌面应用，基于 Tauri + React + Rust 构建。当前版本为本地版：数据保存在本机 SQLite 数据库中，不需要账号，也不会把密码上传到服务器。

## 当前发行版

- 版本：`v1.0.0`
- 平台：Windows x64
- GitHub Release：<https://github.com/RainKirin/vaultory/releases/tag/v1.0.0>

## 下载与安装

打开 [Releases](https://github.com/RainKirin/vaultory/releases) 页面，下载 `v1.0.0` 中的文件。

| 文件 | 用途 |
| --- | --- |
| `Password.Manager_1.0.0_x64-setup.exe` | 推荐给大多数 Windows 用户，双击安装 |
| `Password.Manager_1.0.0_x64_en-US.msi` | 适合需要 MSI 安装包的环境 |
| `password-manager.exe` | 单文件应用程序，无需安装器；下载后可直接运行 |
| `SHA256SUMS-v1.0.0.txt` | 用于校验下载文件是否完整 |

系统要求：Windows 10/11 x64。应用基于 Tauri，需要 WebView2 Runtime；较新的 Windows 10/11 通常已预装。

说明：`password-manager.exe` 不需要运行安装程序，但它不是完全绿色便携版。运行时仍依赖系统 WebView2 Runtime，并会把保险库数据写入 `%APPDATA%\com.password-manager.app\vault.db`。

### 校验下载文件

下载 `SHA256SUMS-v1.0.0.txt` 后，可用 PowerShell 校验安装包：

```powershell
Get-FileHash "Password.Manager_1.0.0_x64-setup.exe" -Algorithm SHA256
```

把输出的哈希值与 `SHA256SUMS-v1.0.0.txt` 中对应文件的值比较。如果不一致，请重新下载，不要运行该文件。

## 首次使用教程

1. 启动应用。
2. 设置主密码，至少 8 个字符。
3. 妥善保存主密码；主密码丢失后无法恢复保险库数据。
4. 解锁后点击新建按钮添加密码条目或 API Key。
5. 可在设置中创建文件夹、切换语言、启用深色模式、调整自动锁定时间。
6. 使用完毕后可手动锁定；应用空闲达到设定时间后也会自动锁定。

## 日常使用

### 添加密码或 API Key

在保险库页面点击新建按钮，选择条目类型后填写名称、用户名、密码、网址、API Key、备注等信息。保存后，条目内容会加密写入本地数据库。

### 使用文件夹整理条目

在设置页面创建文件夹或子文件夹。条目可以归入文件夹，侧边栏选择文件夹后会显示该文件夹及其子文件夹内的条目。

### 搜索与收藏

搜索框支持按名称、用户名、网址和 API Key 搜索。可把重要条目标记为收藏，收藏条目会优先显示。

### 生成密码和用户名

生成器页面支持：

- 配置密码长度。
- 选择是否包含大写字母、小写字母、数字、符号。
- 排除易混淆字符。
- 生成随机用户名。

## 数据保存、备份和恢复

Windows 下应用数据通常保存在：

```text
%APPDATA%\com.password-manager.app\vault.db
```

备份方法：

1. 关闭应用。
2. 复制 `vault.db` 到安全位置。
3. 恢复时关闭应用，再把备份文件放回同一路径。

注意事项：

- 只有知道主密码才能解锁备份数据。
- 删除 `vault.db` 会清空保险库并重新开始初始化。
- 当前版本会加密密码/API Key 条目的内容；文件夹名称和部分应用设置作为本地元数据保存在 SQLite 中。

## 安全特性

- AES-256-GCM 认证加密，用于保护保险库条目内容。
- Argon2id 密钥派生，用于主密码哈希和加密密钥派生。
- 哈希 Salt 与加密 Salt 分离。
- 核心加密管理器释放时清零密钥副本。
- 关键数据库操作使用事务保证原子性。
- 连续解锁失败会触发递增等待时间。
- 本地应用，无账号系统，无远端同步。

## 从源码运行和构建

前置要求：

- Node.js 20 LTS 或更新版本。
- Rust stable 工具链。
- Windows 上的 WebView2 Runtime。

安装依赖：

```bash
npm install
```

开发模式：

```bash
npm run tauri dev
```

构建发布版本：

```bash
npm run tauri build
```

构建完成后，产物位于：

```text
src-tauri/target/release/password-manager.exe
src-tauri/target/release/bundle/nsis/Password Manager_1.0.0_x64-setup.exe
src-tauri/target/release/bundle/msi/Password Manager_1.0.0_x64_en-US.msi
```

这些文件属于本地构建产物，位于 `src-tauri/target/`，默认被 Git 忽略，不会提交到仓库。

## 测试

前端测试：

```bash
npm test
```

Rust 测试：

```bash
cd src-tauri
cargo test
```

如果本地旧构建缓存导致 Tauri build script 读取旧路径失败，可以改用独立 target 目录：

```bash
cargo test --target-dir "%TEMP%\vaultory-cargo-test-target"
```

## 技术栈

- 前端：React 18 + TypeScript + Vite
- 桌面框架：Tauri 2
- 后端：Rust
- 数据库：SQLite + rusqlite
- 加密：AES-256-GCM + Argon2id

## 数据库结构

- `vault_entries`：加密的保险库条目
- `folders`：文件夹和子文件夹
- `settings`：应用设置
- `auth`：主密码哈希和 Salt 信息

数据库结构会随版本迁移扩展。首次运行新版本时会自动执行本地迁移。

## 故障排查

### 忘记主密码

主密码无法恢复。如果确认不再需要旧数据，可以删除 `%APPDATA%\com.password-manager.app\vault.db` 后重新初始化。

### 想完全卸载

先通过 Windows 设置卸载应用，再按需删除 `%APPDATA%\com.password-manager.app\` 中的本地数据。

### Release 页面没有看到文件

请确认打开的是 `v1.0.0` Release 页面：<https://github.com/RainKirin/vaultory/releases/tag/v1.0.0>。

## License

MIT
