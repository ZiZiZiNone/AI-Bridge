//! `ab-converter` —— 转换器宿主：加载清单 · 验签 · 沙盒执行 · 等级裁剪 · 只读路径守卫。
//!
//! 只依赖 `ab-contract`；无网络、无文件 IO、不引密码学与沙盒运行时——
//! 验签经 [`SignatureVerifier`]、执行经 [`SandboxRunner`] 由宿主注入。
//! 等级规则在 [`mod@level`]，路径判定在 [`mod@path`]，各只写一份。
#![forbid(unsafe_code)]

pub mod error;
pub mod host;
pub mod level;
pub mod path;
pub mod traits;

#[cfg(test)]
mod tests;

pub use error::{ConverterError, PathDenied, SandboxError, VerifyError};
pub use host::{ConverterHost, VerifiedConverter};
pub use level::{effective_level, prune};
pub use path::PathGuard;
pub use traits::{SandboxRequest, SandboxRunner, SignatureVerifier};
