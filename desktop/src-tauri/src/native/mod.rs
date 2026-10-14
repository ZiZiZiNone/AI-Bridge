//! Windows 系统原语封装。

pub mod clipboard;
pub mod dpapi;
pub mod hello;
pub mod protocol;
pub mod uia_typing;

/// 宿主原语错误：只带错误码与静态说明，不含凭据、明文或敏感路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeError {
    pub code: u32,
    pub message: &'static str,
}

impl NativeError {
    pub const fn new(code: u32, message: &'static str) -> Self {
        Self { code, message }
    }
}

impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}（错误码 {:#010x}）", self.message, self.code)
    }
}

impl std::error::Error for NativeError {}
