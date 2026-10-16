# AI-Bridge 使用手册（桌面 / 浏览器扩展 / MCP / 移动端）

> 时点：2026-10-15，基线 `HEAD = dd370b7`（含本次修复），门禁 `tools/verify-all.ps1` **20/20 通过**。
> 本文所有命令与结论均在本机实测，凡未验证处已逐条标注「未验证」。

---

## 0. 先读这一节：各端「现在能用」的真实程度

| 端 | 可构建 | 可启动 | 可用程度 |
|---|---|---|---|
| **桌面端** | ✅ | ✅ | **可用**：44 个业务 Tauri 命令 + 3 个窗口命令 + 11 项托盘菜单 + 15 个界面；已实测 GUI 存活、`--mcp-stdio` 真实响应 MCP 握手 |
| **MCP（AI 端）** | ✅ | ✅ | **可用**：`--mcp-stdio` 与回环 HTTP 两种通道；5 个工具已实测列出 |
| **浏览器扩展** | ⚠️ 需先编译 | ⚠️ | **类型检查与单测通过，但无构建产物**，且 native 桥是骨架（回「未接入 core」） |
| **移动端** | ❌ | ❌ | **仅有源码**：无 `gradlew`、无 `.xcodeproj`/`Package.swift`，本机无法构建 |

**一句话**：桌面端和 MCP 今天就能跑；扩展需要先补一步编译（并接受桥是骨架）；移动端需要先补构建工程文件。

---

## 1. 通用环境前置

每条命令前都要设置（本机 Rust 不在默认 `PATH`，且依赖离线 vendor）：

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge
$env:PATH = "C:\Users\HKX\AppData\Local\puccinialin\puccinialin\Cache\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;$env:PATH"
$env:CARGO_HOME = "C:\Users\HKX\Desktop\AI-Bridge\.cargo-home"

# 仅测试需要：把数据目录指向仓库内可写目录，避免写 %APPDATA% 被拒
$env:AIBRIDGE_DATA_DIR = "C:\Users\HKX\Desktop\AI-Bridge\target\test-data-dir"
```

**追加依赖必须已在离线 vendor 中**（2964 个 crate，清单见 `ai-bridge-full-context.zip` 内的 `.ab-vendor-list.txt`），否则 `--offline` 失败。

---

## 2. 桌面端（Windows · Tauri 2）

### 2.1 构建

```powershell
cargo build --manifest-path desktop/src-tauri/Cargo.toml --offline
# 产物：desktop/src-tauri/target/debug/ab-desktop.exe（实测 24.7 MB）
```

实测：**0 错误**，退出码 0（30 个 `never used` 类 warning，不影响运行）。

### 2.1b 已接线的命令面（44 个业务命令，按模块）

| 模块 | 命令 |
|---|---|
| `library`（12） | `vault_create` `vault_unlock` `vault_lock` `vault_is_unlocked` `vault_list_entries` `vault_list_trash` `vault_get_entry` `vault_add_entry` `vault_update_entry` `vault_trash_entry` `vault_restore_entry` `vault_purge_trash` |
| `backup`（5） | `backup_run` `backup_status` `backup_drill` `backup_restore_with_password` `backup_restore_with_recovery` |
| `state`（5） | `state_matrix` `state_health` `state_snapshots` `state_rollback` `state_capture` |
| `sessions`（7） | `session_issue` `session_list` `session_renew` `session_release` `session_revoke` `session_revoke_all` `session_sweep` |
| `candidate`（1） | `candidate_data` |
| `converters`（2） | `converter_list` `converter_install` |
| `pairing`（8） | `pairing_offer` `pairing_review` `pairing_confirm` `pairing_devices` `pairing_unbind` `sync_transfer` `sync_export_pack` `sync_import_pack` |
| `audit`（2） | `audit_query` `audit_count` |
| `mcp`（2） | `mcp_start_http` `mcp_status` |

窗口控制另有 3 个：`ui::window::open_window` / `close_window` / `toggle_window`。

发布版（需 `cargo tauri` CLI，本机未装且不在 vendor）：

```powershell
# 需先在有网环境：cargo install tauri-cli --version "^2"
cd desktop; cargo tauri build   # 产出 MSI / NSIS
```

### 2.2 启动

```powershell
& "C:\Users\HKX\Desktop\AI-Bridge\desktop\src-tauri\target\debug\ab-desktop.exe"
```

- 常驻**托盘**、**无主窗口**（设计如此，`tauri.conf.json` 的 `windows: []`）。
- 启动后看右下角系统托盘的 AI-Bridge 图标。
- 实测：进程存活、无崩溃、内存约数十 MB。

### 2.3 托盘菜单（11 项，id 与文案为源码事实）

| id | 文案 |
|---|---|
| `unlock-lock` | 解锁 / 锁定 |
| `quick-fill` | 快捷填充 |
| `library` | 库 |
| `status` | 状态清单 |
| `sessions` | 临时会话 |
| `audit` | 审计 |
| `backup` | 备份 |
| `converters` | 转换器 |
| `pairing` | 配对 |
| `settings` | 设置 |
| `quit` | 退出 |

### 2.4 界面预览（前端）

宿主内嵌的前端资源来自 `desktop/dist/`。**本机 `vite build` 被沙盒 `spawn EPERM` 拦住**（esbuild 需管道型 stdio 派生），当前 `dist/index.html` 是占位。在**普通终端**里执行：

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge\desktop
npm run dev      # http://localhost:1420/?window=panel-library 预览各窗口
npm run build    # 产出 dist/，供宿主内嵌
```

可用 `?window=<label>` 预览 13 个窗口标签：`candidate`、`panel-library`、`panel-status`、`panel-sessions`、`panel-audit`、`panel-backup`、`panel-converters`、`panel-pairing`、`panel-settings`、`wizard-init`、`wizard-pairing`、`wizard-restore`、`wizard-drill`；另有 `preview-tray-menu`、`preview-notification-toast`。

### 2.5 快捷键与协议

- **`Ctrl+Alt+P`**：唤起候选条（B6.1，跟随鼠标/光标可切）。
- **`aibridge://` 协议**：`aibridge://candidate`、`aibridge://pair`、`aibridge://panel/<短名>`（如 `aibridge://panel/library`）。

---

## 3. 浏览器扩展（Chrome / Edge / Firefox · MV3）

### 3.1 现状（务必先看）

- 仓库**只有 `.ts` 源码，没有 `.js` 产物**（实测 `extensions/**/*.js` 计数 = 0），而 `manifest.json` 引用的是 `background.js` 等 → **必须先编译**。
- `extensions/tsconfig.json` 是 `noEmit: true`，**不产出文件**，需要另配构建。
- **native 桥目前是骨架**：`desktop/src-tauri/src/bridge/messaging_host.rs` 的 `run_stdio()` 对任何请求都回 `{"ok": false, "error": "桥骨架未接入 core"}`。所以扩展能装上、能握手，但**业务动作拿不到数据**。

### 3.2 类型检查与单测（已实测通过）

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge
npx tsc --noEmit -p extensions/tsconfig.json          # 退出码 0
node --test --experimental-test-isolation=none extensions/shared/test/*.test.ts    # 38 pass / 0 fail
node --test --experimental-test-isolation=none extensions/chromium/test/*.test.ts # 4 pass / 0 fail
```

### 3.3 加载扩展（需先解决编译）

**当前没有现成的编译脚本**（`tools/` 下无扩展打包器）。最小可行做法是在普通终端用 esbuild 把 `.ts` 转成 `.js` 并重写导入后缀，然后加载解包目录：

```powershell
cd C:\Users\HKX\Desktop\AI-Bridge\extensions\chromium
npx esbuild background.ts --bundle --format=esm --outfile=background.js
# popup/content/onboarding 同理，按各自目录输出
```

随后：Chrome/Edge → `chrome://extensions` → 打开「开发者模式」→「加载已解压的扩展程序」→ 选 `extensions/chromium`。

Firefox → `about:debugging#/runtime/this-firefox` →「临时载入附加组件」→ 选 `extensions/firefox/manifest.json`。

**未验证**：本机未实际生成产物，也未在浏览器里加载过。扩展的运行时行为（`chrome.cookies` 的 Promise 形态、MV3 service worker 挂起后重连、Firefox 的 `background.type=module`）均**未在真机验证**（`extensions/README.md` 第 6 节已自述）。

### 3.4 native messaging 宿主注册（桥可用后才需要）

- 宿主名：`com.aibridge.bridge`（扩展侧常量 `BRIDGE_NATIVE_HOST_NAME`；`extensions/README.md` 标注为占位，待与宿主注册对齐）。
- 帧协议：**4 字节小端长度前缀 + JSON**，单帧上限 1 MiB。
- 宿主命令：`ab-desktop.exe --native-messaging-host`（实测：stdin 关闭时干净退出，exit code 0）。
- 仓库内**没有** host manifest 模板（实测 `*aibridge*.json` 匹配 host/native 的文件数为 0），需自行按浏览器要求生成并写入注册表。

---

## 4. MCP（AI 端）—— 已实测可用

**核心承诺**：AI 永远拿不到真实凭据，只持有短期、限域、可撤销、可审计的临时句柄。

### 4.1 通道一：stdio（推荐，已实测）

MCP 客户端以子进程方式启动本程序：

```jsonc
// 在 MCP 客户端配置里（以 Claude Desktop / 兼容客户端为例）
{
  "mcpServers": {
    "ai-bridge": {
      "command": "C:\\Users\\HKX\\Desktop\\AI-Bridge\\desktop\\src-tauri\\target\\debug\\ab-desktop.exe",
      "args": ["--mcp-stdio"],
      "env": { "AIBRIDGE_DATA_DIR": "C:\\Users\\HKX\\AppData\\Local\\AI-Bridge" }
    }
  }
}
```

**实测结果**（手工发帧验证）：

```
→ {"jsonrpc":"2.0","id":1,"method":"initialize",
   "params":{"protocolVersion":"2025-06-18","capabilities":{},
             "clientInfo":{"name":"probe","version":"1.0"}}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18",
   "capabilities":{"tools":{}},"serverInfo":{"name":"rmcp","version":"1.8.0"}}}
```

`tools/list` 实测返回工具定义（含完整 `inputSchema`）。

> **注意协议版本**：底层 `rmcp 1.8.0` 使用 MCP **2025-06-18**。用旧版 `2024-11-05` 握手**不会得到响应**——我第一次探测就踩了这个坑。

### 4.2 通道二：回环 Streamable HTTP

在宿主进程内启动，仅绑 `127.0.0.1`（`validate_loopback_addr` 拒绝非回环地址）：

- Tauri 命令：`commands::mcp::mcp_start_http(port)` / `mcp_status`。
- 命令面另有 `stdio_entry` 字段回显 `"--mcp-stdio"`。
- Origin 白名单：`http://localhost`、`http://127.0.0.1`、`http://[::1]` 及 https 同项（`LOOPBACK_ALLOWED_ORIGINS`），并拒绝 `http://localhost.evil.example` 这类前缀伪装。
- 重复启动会被拒（`"MCP HTTP 服务已在运行"`）；有对应单测 `http_status_is_fair_and_refuses_double_start` 实测通过。

### 4.3 五个工具（工具面 = C1–C5）

| 工具 | 语义 |
|---|---|
| `acquire_session` | 签发临时会话：`target`(web/app/container) + `scope`(域名白名单/只读/单次) + `ttl_secs`（默认 3600，上限 86400）；同站多账号并存时须给 `account_ref`。返回**一次性不透明 handle** |
| `renew_session` | 上限内续期 |
| `release_session` | 显式释放 |
| `list_sessions` | 可见临时会话总览（**不含 handle**） |
| `revoke_all` | 一键吊销全部 |

**释放三重触发，先到先生效**：显式 `release_session` / 空闲 30 分钟 / TTL 到期。默认**不登出第三方**（防误伤真实会话）。

### 4.4 前置条件

- 必须先有**真实会话**：`acquire_session` 从 `ab-state` 的会话矩阵取真会话；库里没有该目标的登录态时会返回「目标没有可用会话」（有单测覆盖）。
- 转换器验签需 `AIBRIDGE_OFFICIAL_SIGNER` + `AIBRIDGE_OFFICIAL_PUBKEY_HEX`；未配置时**拒载一切转换器**（fail-closed，设计如此）。

---

## 5. 移动端（iOS / Android）

### 5.1 现状：仅有源码，本机无法构建

实测缺失（这是**构建工程文件**，不是源码）：

| 平台 | 缺什么 | 现有 |
|---|---|---|
| Android | `gradlew`、`gradlew.bat`、`settings.gradle.kts`、`build.gradle.kts`、`app/build.gradle.kts` | ✅ `AndroidManifest.xml`、全套 Kotlin 源码（M1–M9 + Autofill/Credential/WebView） |
| iOS | `Package.swift`、`.xcodeproj` | ✅ SwiftUI 源码（M1–M9 + Credential Provider + Safari 扩展） |

**FFI 绑定产物已存在**：`mobile/ffi/gen/` 下有 `aibridge.kt`、`aibridge.swift`、`aibridgeFFI.h`、`aibridgeFFI.modulemap`。

### 5.2 完成后如何构建

```powershell
# 1) 若改了 UDL，先同步（事实源：mobile/ffi/aibridge.udl）
powershell -ExecutionPolicy Bypass -File tools/sync-ffi-udl.ps1
cargo run -p ab-ffi --bin uniffi-bindgen -- generate core\ffi\src\aibridge.udl `
  --crate ab_ffi -l kotlin -l swift -o mobile\ffi\gen --no-format

# 2) 补齐 Gradle 包装后可构建（需 Android SDK）
cd mobile\android; .\gradlew :app:assembleDebug

# 3) iOS 需 Xcode（macOS）：补 Package.swift / xcodeproj 后 Archive
```

**未验证**：本机无 Android SDK、无 Xcode，**以下全部未执行**——Gradle 构建、Xcode 构建、真机安装、iOS/Android 运行时行为、Autofill/Credential 服务注册、NSD/Bonjour 双机配对。

### 5.3 FFI 接口面（`mobile/ffi/aibridge.udl`）

`AibridgeCore`、`Vault`、`RecoveryKey`、`StateBus`、`Broker`、`SyncEngine`、`ConverterHost`、`AuditLog`、`McpServer`、`VaultStore`、`SessionSource`、`Transport`、`Clock`、`Entropy`；数据字典含 `Cookie`、`Target`、`Payload`、`SessionEnvelope`、`StandardAccess`、`CredentialRecord` 等。

---

## 6. 让各端真正「有数据」的最短路径

桌面端与 MCP 已经能跑，但要让界面和 AI 拿到真实数据，需要两步：

1. **喂入真实会话**：目前唯一完整的取数入口是**转换器**（`converters/qq` 为首个）。
   注意 `converters/qq/manifest.json` 当前是**全零占位签名**，被契约层正确拒载（这是设计内的未签名状态）。生产使用需用外部 Ed25519 密钥实签：
   ```powershell
   python tools/sign-converter.py converters/qq/manifest.json --print-hex   # 产出待签名字节
   ```
   并配置 `AIBRIDGE_OFFICIAL_SIGNER` / `AIBRIDGE_OFFICIAL_PUBKEY_HEX`。
2. **打通扩展桥**：`messaging_host.rs` 的 `run_stdio()` 现为骨架，需接入 `HostState::handle_extension_request`（该方法已实现，40 个公共方法之一），扩展才能通过 `session.capture` 上报 cookie。

---

## 7. 验证命令速查

```powershell
# 全仓门禁（目标：20/20，失败 0）—— 实测通过
powershell -ExecutionPolicy Bypass -File tools/verify-all.ps1

# 分层验证（失败依赖会隐藏下游错误，逐层跑）
cargo test --workspace --offline --quiet                                  # 148 项
cargo test --manifest-path desktop/src-tauri/Cargo.toml --offline         # 63 项
cargo test --manifest-path tests/recovery-drill/Cargo.toml --offline      # 4 项
cargo test --manifest-path tests/contract-compat/Cargo.toml --offline     # 10 项
cargo test --manifest-path tests/e2e-pipeline/Cargo.toml --offline

# 桌面端专项
cd desktop; npm run typecheck; npm run check:structure; npm run check:render   # 0 / 8-8 / 15-15

# 扩展专项
npx tsc --noEmit -p extensions/tsconfig.json

# 契约与转换器
python tools/check-contracts.py
node tools/contract-check/check.mjs
node converters/sdk/tests/conformance.mjs converters/qq
```

> **禁用裸 `cargo check`（不带 `--all-targets`）作为通过依据**：它不编译 dev-dependencies，实测会出现「`cargo check` 退出 0 而 `cargo test --no-run` 报错」的假绿灯。

---

## 8. 已知限制与环境坑（会影响你，逐条实测）

| 现象 | 性质 | 处理 |
|---|---|---|
| 托盘图标出现但各面板为空 | `desktop/dist/` 是占位（前端未构建） | 普通终端 `cd desktop && npm run build` |
| 扩展装上了但动作回「桥骨架未接入 core」 | `messaging_host.rs` 是骨架（设计内） | 需接入 `HostState::handle_extension_request` |
| MCP 握手无响应 | 协议版本不对 | 用 **2025-06-18**（rmcp 1.8.0） |
| `cargo` 报 `failed to write ... Cargo.lock`（`os error 5`） | 本会话沙盒禁止写该文件；ACL 与可写文件逐项一致 | 在可写环境跑；**禁用 `--locked`**（会报 `cannot update the lock file`） |
| `npm run dev` / `vite build` 报 `spawn EPERM` | 沙盒禁止管道型 stdio 派生（esbuild 需要） | 用 `node desktop/scripts/render-check.mjs` 做等效渲染校验；构建请在普通终端 |
| 桌面端测试报 19 项写入拒绝 | 沙盒禁写 `%APPDATA%` | 设 `$env:AIBRIDGE_DATA_DIR` |
| 注册表往返测试打印「跳过」 | 沙盒禁写 `HKCU`（`0x80070005`） | 预期行为，已在测试内显式跳过并说明 |
| 部分源文件写入被拒（如 `desktop/src-tauri/Cargo.toml`） | 环境限制，非项目缺陷 | 需切到有写权限的环境 |
| 改源码后中文乱码 / 编译失败 | Windows PowerShell 5.1 按 GBK 读写 | **禁用** `Get-Content`/`Set-Content` 往返改源码；用 `[System.IO.File]::WriteAllBytes` 或 Python `encoding="utf-8"` |

---

## 9. 未验证项（不得视为已通过）

1. `cargo tauri dev` / `cargo tauri build`——`cargo-tauri` 未安装且不在离线 vendor。
2. 桌面端 GUI 的**交互行为**（点菜单、填表、注入）——仅验证进程存活，未验证界面动作。
3. 浏览器扩展的**运行时行为**与真机加载——未生成产物、未加载。
4. 全部移动端构建与真机行为——无 SDK/Xcode。
5. 转换器实签与真机取数（QQ 等）——`qq/manifest.json` 仍为占位签名。
6. 双机配对（NSD/Bonjour）、Windows Hello 真机弹窗、KeePassXC 互操作打开 `.kdbx`。
