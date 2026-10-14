//! 构建脚本：按本 crate 内 `src/aibridge.udl` + crate 根 `uniffi.toml` 生成 Rust 脚手架。
//! 生成物落在 OUT_DIR（`aibridge.uniffi.rs`），由 `include_scaffolding!` 引入。
//! UDL 事实源是 `mobile/ffi/aibridge.udl`；crate 内副本由 tools/sync-ffi-udl.ps1 同步
//! （uniffi-bindgen CLI 的 `generate` 要求 UDL 位于 `<crate>/src/` 下才能定位 crate 根）。

fn main() {
    println!("cargo:rerun-if-changed=src/aibridge.udl");
    println!("cargo:rerun-if-changed=uniffi.toml");
    uniffi::generate_scaffolding_for_crate("src/aibridge.udl", "ab_ffi")
        .expect("uniffi scaffolding generation failed for src/aibridge.udl");
}
