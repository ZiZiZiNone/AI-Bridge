# ab-ffi —— UniFFI 绑定层

`mobile/ffi/aibridge.udl` 的机械镜像：接口 / 记录 / 错误逐个映射 core 各 crate 的公开 API，
只转发、零业务规则（docs/架构与契约.md §2「ab-ffi = UniFFI 绑定（代码生成，机械镜像）」）。

## 结构

```
core/ffi/
├─ Cargo.toml          cdylib（移动端链接）+ rlib（Rust 测试）+ uniffi-bindgen bin
├─ build.rs            脚手架生成（UDL 需在 crate 内 src/ 下，副本由 tools/sync-ffi-udl.ps1 同步）
├─ uniffi.toml         绑定配置（Kotlin 包名 / Swift 模块名，与 mobile/ffi/uniffi.toml 同步维护）
├─ src/aibridge.udl    构建副本（事实源 = mobile/ffi/aibridge.udl）
├─ src/types.rs        UDL 记录 → core 类型
├─ src/convert.rs      双向映射（From/Into）
├─ src/adapters.rs     宿主注入 trait → core trait（VaultStore/SessionSource/Transport/Clock/Entropy）
├─ src/traits.rs       UniFFI callback interface 声明
├─ src/services.rs     UDL 接口实现（Vault/StateBus/Broker/SyncEngine/ConverterHost/AuditLog/McpServer）
└─ tests/ffi_roundtrip.rs  集成测试（假 trait 实现 → new_core → 各服务全链）
```

## 绑定生成（实测命令，2026-10-12）

```powershell
$env:PATH = "…\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;$env:PATH"
$env:CARGO_HOME = "C:\Users\HKX\Desktop\AI-Bridge\.cargo-home"
cargo run -p ab-ffi --bin uniffi-bindgen -- generate core\ffi\src\aibridge.udl `
    --crate ab_ffi -l kotlin -l swift -o mobile\ffi\gen --no-format
```

产物：`mobile/ffi/gen/aibridge.kt`、`aibridge.swift`、`aibridgeFFI.h`、`modulemap`。
UDL 改动后必须先 `powershell tools/sync-ffi-udl.ps1`（同步事实源到构建副本）再生成。

## UDL ↔ core 漂移台账（Err(INTERNAL) 面，机械镜像不可能实现，待 UDL/core 双向对齐后消除）

以下 UDL 方法与 core 公开面**形状不匹配**，FFI 层按「不发明业务/不自造载荷」纪律显式失败
（`CoreError::INTERNAL`），而非伪造数据：

| UDL 面 | 漂移内容 | 消除路径 |
|---|---|---|
| `StateBus.capture(source_id, domain)` | core `StateService::capture(&Endpoint)` 需端点结构且 SessionSource 读面缺 account_ref 语义 | core 扩展捕获签名或 UDL 改形 |
| `Broker.issue(scope, ttl)` | core `BrokerService::issue` 需 `who` + 真实 `SessionEnvelope`（代持对象）；UDL 两者皆缺 | UDL 补 who+envelope 入参（推荐：宿主先从 state 取会话再传给 broker） |
| `SyncEngine.pair()` / `confirm(ticket_id, fingerprint)` | core `pair(&PairingOffer, now)` 需**远端** offer + 全量 `PairedDevice` 记录；UDL 无入参/无载荷 | UDL 改形为 offer/record 透传 |
| `SyncEngine.pack(envelopes)` | core `pack(to_device_id, envelope, validity_secs, now, entropy)` 需收件方 + 有效期；且 core 为单信封、UDL 为序列 | UDL 补参数并定单/多信封语义 |
| `ConverterHost.load/verify/run_extract` | core 需注入 `SignatureVerifier`/`SandboxRunner` + 模块字节；UDL 全缺 | UDL 补注入面（与 desktop host.rs 的实现策略对齐） |
| `McpServer.serve()` | core `serve_stdio` 为 async 且需 broker/state/who 装配 | MCP 启动属桌面宿主职责，建议 UDL 移除该面 |
| `Vault.list_entries(query)` | core 无查询语义 | UDL 去掉 query 参数（宿主本地过滤）或 core 补搜索 |
| `HealthReport` 三字段 | core `ab_state::HealthReport` 无该数据源 | core 补数据源（fail-safe 现状：恒报否） |
| Entry/EntrySummary 无 uuid、updated_at | core `Entry` 以 uuid 为主键 | UDL 补字段（uuid 经 FFI 传 16 字节）或宿主自管 account_ref↔uuid 映射（现实现：服务内按 account_ref 找回） |

## 其它已知事实

- **审计双 sink 已修复**（2026-10-12）：`new_core` 里 Broker 与 AuditLog 共用同一 `InMemoryAuditSink`。
- **身份 shim**：UDL 无 device_id/主密钥注入面，`new_core` 以注入 Entropy 生成一次性身份/密钥
  （重启即变）。持久身份待 UDL 补注入面——**移动端配对/transfer 现状因身份不持久而不可用**，
  这是设计内已知限制，非静默降级。
- `unlock` 首次调用（存储空）= 初始化新库；`list_entries(query≠None)` 显式拒绝（漂移面）。
- 跨 FFI 错误统一降维 `CoreError`（CONTRACT/CRYPTO/STORAGE/NOT_FOUND/DENIED/TRANSPORT/CONVERSION/INTERNAL），
  文本零秘密。

## 测试（实跑 2026-10-12）

`cargo test -p ab-ffi` → 4 passed（init-on-first-unlock + entry CRUD/TOTP/generator + 审计往返 + 漂移面 fail-closed）。
