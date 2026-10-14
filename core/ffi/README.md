# `ab-ffi` · UniFFI 跨语言绑定桥

`mobile/ffi/aibridge.udl` 为 UDL 唯一事实源，`core/ffi/src/aibridge.udl` 为构建期同步副本（由 `tools/sync-ffi-udl.ps1` 同步）。

## 同步与生成命令

在仓库根目录执行：

```powershell
powershell -ExecutionPolicy Bypass -File tools/sync-ffi-udl.ps1
cargo run -p ab-ffi --bin uniffi-bindgen -- generate core\ffi\src\aibridge.udl --crate ab_ffi -l kotlin -l swift -o mobile\ffi\gen --no-format
```

## 类型映射说明

| UDL 声明 | Rust `core` 对应项 | 说明 |
|---|---|---|
| `Source.source_type` | `ab_contract::Source::source_type`（JSON `type`） | 避开 Swift/Kotlin `type` 关键字冲突 |
| `StandardAccess.meta` | `BTreeMap<String, Value>` ↔ `record<string, string>` | FFI 边界将 JSON 值序列化为字符串映射 |
| `SessionSource` | `ab_state::SessionSource` | 移动端容器与 Safari 扩展以 `Cookie[]` 交付，由适配层组装为 `SessionRecord` |
| `CoreError` | 各核心 crate 细分错误枚举 | FFI 边界按域收敛为 8 类顶层错误码 |
