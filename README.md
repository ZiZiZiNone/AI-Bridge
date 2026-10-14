# AI-Bridge

本地优先的登录态与凭据管理器：**密码存储 + 登录状态管理 + 登录状态扩散**。以 Windows 桌面端为中枢，浏览器扩展（Chrome / Edge / Firefox）与 iOS / Android 为两端，并经本地 MCP server 向 AI 签发短期、限域、可撤销、可审计的临时会话。

## 目录结构

```
AI-Bridge/
├─ contracts/                   契约层唯一事实源（3 份 JSON Schema）
│  ├─ standard-access.schema.json
│  ├─ converter-manifest.schema.json
│  └─ session-envelope.schema.json
├─ core/                        Rust 核心工作区（纯业务逻辑，零宿主依赖）
│  ├─ contract/                 ab-contract：契约类型、结构校验、canonical_signing_bytes
│  ├─ crypto/                   ab-crypto：Argon2id、HKDF、AES-256-GCM、HMAC、Ed25519、X25519
│  ├─ vault/                    ab-vault：KDBX4 容器、条目 CRUD、回收站、TOTP、密码/口令词生成、恢复密钥
│  ├─ state/                    ab-state：会话矩阵、覆盖前快照、回滚、健康分级
│  ├─ broker/                   ab-broker：AI 临时会话签发、TTL/空闲过期、scope 强制、吊销表
│  ├─ sync/                     ab-sync：QR 指纹配对、局域网 E2E 直传、A8 离线加密传送包
│  ├─ mcp/                      ab-mcp：本地 MCP 服务（stdio + 回环 Streamable HTTP）
│  ├─ converter-host/           ab-converter：转换器验签、等级裁剪、只读路径守卫
│  ├─ audit/                    ab-audit：追加式审计（record_or_fail fail-closed）
│  └─ ffi/                      ab-ffi：UniFFI 导出层（src/aibridge.udl 为同步副本，禁止直接手改）
├─ desktop/                     Windows 桌面端（Tauri 2 + Vue 3 + TypeScript）
│  ├─ src/                      W1–W15 界面视图与窗口注册表
│  ├─ src-tauri/                Tauri 宿主、系统原语（DPAPI/Hello/Clipboard/UIA/Protocol）、扩展桥
│  └─ scripts/                  结构与 SSR 渲染校验脚本
├─ extensions/                  浏览器扩展（MV3，Chrome / Edge / Firefox）
│  ├─ shared/                   宿主无关契约类型、Cookie 保真映射、SessionEnvelope 与桥协议 v1
│  ├─ chromium/                 Chromium / Edge 扩展实现（X1–X3）
│  └─ firefox/                  Firefox 扩展实现（X1–X3）
├─ mobile/                      移动端（iOS SwiftUI / Android Compose）
│  ├─ android/                  Android 宿主、LanTransport、CookieFidelity、QuickUnlockStore、Autofill/Credential 服务
│  ├─ ios/                      iOS 宿主、LanTransport、SafariCookieBridge、配对指纹解析
│  └─ ffi/                      aibridge.udl（唯一事实源）；mobile/ffi/gen/ 为生成物（禁止手改）
├─ converters/                  标准转换器
│  ├─ sdk/                      转换器 ABI、本地宿主垫片、conformance 与 schema 校验器
│  ├─ _template/                标准转换器模板（保持全零占位签名）
│  └─ qq/                       QQ 转换器（首接管线验证）
├─ tools/                       工程与离线验证脚本（fetch-vendor.py、sign-converter.py、sync-ffi-udl.ps1、contract-check/）
└─ tests/                       契约漂移锁（contract-compat）、恢复演练（recovery-drill）、转换器夹具、端到端管线（e2e-pipeline）
```

> **生成物标注（勿手改）**：`mobile/ffi/gen/`、`core/ffi/src/aibridge.udl`（由 `tools/sync-ffi-udl.ps1` 从 `mobile/ffi/aibridge.udl` 同步）、`desktop/dist/`、`target/`。

## 安装

### 前置条件

- **Rust**：`rustup` `stable-x86_64-pc-windows-msvc`（≥ 1.77）
- **Node.js**：≥ 22（支持原生 `.ts` 测试与 `node --test`）
- **Python**：≥ 3.11（含 `jsonschema`）
- **Windows 系统依赖**：WebView2 Runtime（Win11 自带）

### 安装步骤

```powershell
# 1. 进入仓库根目录并配置离线/本地工具链环境变量（Windows 本机示例）
cd C:\Users\HKX\Desktop\AI-Bridge
$env:PATH = "C:\Users\HKX\AppData\Local\puccinialin\puccinialin\Cache\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;$env:PATH"
$env:CARGO_HOME = "C:\Users\HKX\Desktop\AI-Bridge\.cargo-home"

# 2. 同步/校验离线 Rust vendor 依赖（增量幂等）
python tools/fetch-vendor.py

# 3. 安装桌面端前端依赖（受限环境使用本地缓存目录）
cd C:\Users\HKX\Desktop\AI-Bridge\desktop
npm install --ignore-scripts --cache .npm-cache
```

## 配置

全部配置通过环境变量注入，不设云端账号或外部服务配置文件：

| 配置项（环境变量） | 含义 | 取值范围 / 格式 | 默认值 |
|---|---|---|---|
| `AIBRIDGE_DATA_DIR` | 桌面端加密库（`vault.aibvault`）、DPAPI 附加熵（`.dpapi-entropy`）与受管转换器目录根路径 | 绝对目录路径 | Windows 下依次取 `%LOCALAPPDATA%\AI-Bridge`、`%APPDATA%\AI-Bridge`；POSIX 下取 `$HOME/.local/share/ai-bridge`（生产环境绝不回退 `%TEMP%`） |
| `AIBRIDGE_OFFICIAL_SIGNER` | 转换器官方发布签名者标识（对应 `manifest.signature.signer`） | 非空字符串（如 `official-2026`） | 未设置时宿主验签默认拒载一切转换器（fail-closed） |
| `AIBRIDGE_OFFICIAL_PUBKEY_HEX` | 转换器官方 Ed25519 公钥（32 字节） | 64 位小写 hex 字符串（`^[0-9a-f]{64}$`） | 未设置或格式非法时默认拒载一切转换器（fail-closed） |
| `APPDATA` / `LOCALAPPDATA` / `USERPROFILE` | 转换器 `readable_paths` 模板占位符展开来源 | 绝对目录路径 | 取宿主操作系统当前用户环境变量 |

## 运行

```powershell
# 1. 仅启动桌面端前端预览（端口 1420，可通过 ?window=<label> 预览各窗口）
cd C:\Users\HKX\Desktop\AI-Bridge\desktop
npm run dev
# 验证方式：浏览器访问 http://localhost:1420/?window=panel-library

# 2. 启动完整 Windows 桌面宿主（Tauri 2）
cd C:\Users\HKX\Desktop\AI-Bridge\desktop
cargo tauri dev

# 3. 作为浏览器扩展 Native Messaging 宿主启动（由浏览器自动拉起，走 stdio 4B 小端帧协议）
C:\Users\HKX\Desktop\AI-Bridge\desktop\src-tauri\target\release\ab-desktop.exe --native-messaging-host

# 4. 生成转换器清单规范化待签名字节（供外部 Ed25519 密钥签名）
cd C:\Users\HKX\Desktop\AI-Bridge
python tools/sign-converter.py converters/qq/manifest.json --print-hex
```

## 开发到生产的配置与部署变化

| 维度 | 开发 / 自测环境 | 生产发布环境 |
|---|---|---|
| **数据目录 (`AIBRIDGE_DATA_DIR`)** | 单测与沙盒显式指向临时可写目录（如 `target/test-data-dir`）隔离状态 | 不设临时目录变量，固定落入当前用户 `%LOCALAPPDATA%\AI-Bridge`，配合 `.dpapi-entropy` 与当前用户 DPAPI 保护 |
| **转换器验签信任根** | 模板 `_template` 使用全零占位签名，仅在 `conformance.mjs --expect-placeholder-signature` 下自测 | 生产宿主强制拒绝全零占位签名与占位 `module_sha256`；必须配置正式 `AIBRIDGE_OFFICIAL_SIGNER` 与 `AIBRIDGE_OFFICIAL_PUBKEY_HEX`（或编译期固定公钥）并对 `.wasm` 实签 |
| **MCP 回环服务** | 按需在回环端口（`127.0.0.1:0`）启停调试 | 默认仅开启 stdio 或绑定 `127.0.0.1`/`::1`，强制启用 `LOOPBACK_ALLOWED_ORIGINS` 白名单校验 |
| **构建产物与打包** | `cargo test` / `npm run dev` | 桌面端：`cd desktop && cargo tauri build` 产出 Windows MSI / NSIS 安装包；移动端：先跑 `tools/sync-ffi-udl.ps1` 与 `uniffi-bindgen`，再由 Android Studio (`gradlew :app:assembleRelease`) / Xcode Archive 打包 |
| **回滚方式** | 会话层直接调用 `ab-state::rollback(snapshot_id)`；库文件使用 `A6` 滚动快照恢复 | 升级失败时卸载新包并重装上一版 MSI/APK，使用托管备份目录（`A6.5`）下的 KDBX4 备份或恢复密钥（`S3`）执行 `W13` 恢复向导 |

## 测试

在仓库根目录（`C:\Users\HKX\Desktop\AI-Bridge`）执行：

```powershell
# 1. 一键全仓门禁验收
powershell -ExecutionPolicy Bypass -File tools/verify-all.ps1

# 2. 契约层与漂移锁（零第三方依赖）
node tools/contract-check/check.mjs
node tests/contract-compat/check-compat.mjs --selftest

# 3. 转换器 SDK、模板与签名基准向量互锁检查
python tools/sign-converter.py --self-test
node converters/sdk/tests/conformance.mjs converters/_template --expect-placeholder-signature
python converters/sdk/tests/schema-check.py converters/_template
python core/converter-host/tools/check_crate_rules.py

# 4. 桌面端结构检查（8/8）、SSR 渲染检查（15/15）与宿主 Rust 单测
node desktop/scripts/check-structure.mjs
node desktop/scripts/render-check.mjs
$env:AIBRIDGE_DATA_DIR = "$PWD\target\test-data-dir"
cargo test --manifest-path desktop/src-tauri/Cargo.toml

# 5. Rust 核心工作区与独立测试套件
cargo test
cargo test --manifest-path tests/contract-compat/Cargo.toml
cargo test --manifest-path tests/recovery-drill/Cargo.toml
cargo test --manifest-path tests/converter-fixtures/Cargo.toml
cargo test --manifest-path tests/e2e-pipeline/Cargo.toml

# 6. 浏览器扩展单测
node --test --experimental-test-isolation=none extensions/shared/test/types.test.ts extensions/shared/test/cookie.test.ts extensions/shared/test/envelope.test.ts extensions/shared/test/bridge.test.ts
```

## 常见问题

1. **报错：`cargo` 未找到或离线构建拉取 crates.io 失败**
   - **原因**：Rust 工具链不在默认 `PATH`，或新加依赖未同步进本地 `.cargo-home/vendor`。
   - **处理**：在每条终端命令前设置 `$env:PATH` 与 `$env:CARGO_HOME`，新增依赖后先运行 `python tools/fetch-vendor.py`（独立 workspace 加 `--extra-manifest`）。
2. **报错：桌面端 `cargo test` 出现 19 个环境性写入拒绝（PermissionDenied / AccessDenied）**
   - **原因**：受限沙盒禁止写入用户默认 `%APPDATA%` 目录。
   - **处理**：设置 `$env:AIBRIDGE_DATA_DIR = "$PWD\target\test-data-dir"` 后执行 `cargo test --manifest-path desktop/src-tauri/Cargo.toml`，宿主优先使用 `AIBRIDGE_DATA_DIR`。
3. **报错：`node --test` 或 `vite build` 报 `spawn EPERM`**
   - **原因**：受限沙盒禁止派生带管道 stdio 的子进程。
   - **处理**：`node --test` 追加 `--experimental-test-isolation=none`；桌面端视图渲染验证使用 `node desktop/scripts/render-check.mjs`。
4. **报错：PowerShell `Get-Content` / `Set-Content` 改写源码后出现中文乱码或编译失败**
   - **原因**：Windows PowerShell 5.1 默认按 GBK/ANSI 编码读写文件，会破坏无 BOM 的 UTF-8 源码。
   - **处理**：严禁使用 `Get-Content` / `Set-Content` 往返修改源文件；脚本写文件一律用 `[System.IO.File]::WriteAllBytes` 或 Python `encoding="utf-8"`。

## TODO

以下事项属非阻塞的后续增强或真机联调项（均已指明具体落点与影响面）：

- **官方转换器发布公钥入库与外部实签**（影响面：`desktop/src-tauri/src/host.rs`、`converters/qq/manifest.json`）：当前 `tools/sign-converter.py` 已产出与 `ab_contract::canonical_signing_bytes` 互锁的待签名字节，待外部硬件/离线密钥工具签发 `converters/qq/manifest.json` 并将官方公钥常量固化入库。
- **核心层 P2 纵深加固**（影响面：`core/vault/src/kdbx.rs`、`core/vault/src/xml.rs`、`core/sync/src/lib.rs`、`core/audit/src/lib.rs`）：包括 KDBX gunzip 解压输出上限与 XML `<Group>` 递归深度上限、主库与恢复副本世代计数防回滚、局域网直传序号防重放、审计日志哈希链落盘持久化。
- **真机首验清单**（影响面：`converters/qq/`、`desktop/src-tauri/src/native/`、`mobile/ios/`、`mobile/android/`）：QQ 真实环境数据路径与 schema 校准、KeePassXC 互操作打开 `.kdbx`、Windows Hello 真机弹窗、NSD/Bonjour 双机发现与配对、Xcode/Gradle 真机编译安装。
