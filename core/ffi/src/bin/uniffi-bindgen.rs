//! uniffi-bindgen 入口（生成 Swift / Kotlin 绑定用，非运行时组件）。
//! 命令见 core/ffi/README.md「生成命令」。

fn main() {
    uniffi::uniffi_bindgen_main()
}
