//! Windows 系统能力封装：core 注入 trait 的实现骨架。
//!
//! 规则（docs/架构与契约.md §0 / §3）：
//! - 本层只提供系统原语，**不含任何业务判断**（规则一律在 core）；
//! - core 的 trait 定义在 core 自身 crate（唯一事实源），本层**不重复声明 trait**；
//!   每个子模块的文档头写明它要实现哪个 core trait、对应哪些功能点，
//!   接入方式为 `impl <core trait> for <本层类型>`（core 落地后补 impl 头，方法签名已在骨架固定）；
//! - 本层不得依赖 `ui`（禁止反向依赖）；错误只带系统错误码，不带业务语义。
//!
//! core 注入面（架构与契约 §0）与桌面实现的分工：
//! | core trait | 桌面实现位置 |
//! |---|---|
//! | `VaultStore` | `native::dpapi::DpapiVaultStore` |
//! | `Entropy` | `native::dpapi::CngEntropy` |
//! | `Transport` | `bridge::messaging_host`（扩展桥的帧收发原语） |
//! | `SessionSource` | 由浏览器扩展侧实现（extensions/shared 读 cookie），桌面端经 bridge 接收 |
//! | `Clock` | 用 `std::time`，无需宿主实现 |
//!
//! 未验证：全部子模块为 todo!() 级骨架，未编译、未运行、未调用任何 Windows API。

pub mod clipboard;
pub mod dpapi;
pub mod hello;
pub mod protocol;
pub mod uia_typing;

/// 宿主原语错误：只带系统错误码与静态说明，不含业务语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeError {
    /// Windows 错误码 / HRESULT
    pub code: u32,
    /// 静态说明（禁止携带凭据、明文、完整路径等敏感内容）
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
