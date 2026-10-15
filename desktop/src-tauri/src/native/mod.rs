//! Windows 系统能力封装：core 注入 trait 的 Windows 实现。
//!
//! 规则（docs/架构与契约.md §0 / §3）：
//! - 本层只提供系统原语，**不含任何业务判断**（规则一律在 core）；
//! - core 的 trait 定义在 core 自身 crate（唯一事实源），本层**不重复声明 trait**；
//!   每个子模块的文档头写明它实现哪个 core trait、对应哪些功能点；
//! - 本层不得依赖 `tauri` 与 `ui`（禁止反向依赖与宿主对象）；错误只带系统错误码，不带业务语义。
//!
//! core 注入面（架构与契约 §0）与桌面实现的分工：
//! | core trait | 桌面实现位置 |
//! |---|---|
//! | `ab_vault::VaultStore` | `native::dpapi::DpapiVaultStore`（DPAPI 外壳 + 原子替换） |
//! | `ab_crypto::Entropy` | `native::dpapi::CngEntropy`（BCryptGenRandom） |
//! | `ab_sync::Transport` | 局域网直传信道（当前为未实现桩，见 commands::pairing） |
//! | `ab_state::SessionSource` | 由浏览器扩展侧提供会话，经 bridge 送入（当前空实现） |
//! | `ab_state::Clock` | 用 `std::time`，无需宿主实现 |
//!
//! 验证状态：DPAPI / CNG / 剪贴板 / 注册表实跑单测；Windows Hello（WinRT 往返）与
//! UIA / SendInput 填表需交互会话，未实测（见 desktop/README.md）。

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
