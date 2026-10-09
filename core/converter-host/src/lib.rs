//! `ab-converter` —— 转换器宿主：加载清单 · 验签 · 沙盒执行 · 接入等级裁剪 · 只读路径守卫。
//!
//! 边界（`docs/架构与契约.md` §2）：本 crate 只依赖 `ab-contract`；无网络、无文件 IO、
//! 不引任何密码学库与沙盒运行时——验签经 [`SignatureVerifier`]、执行经 [`SandboxRunner`]
//! 由宿主注入（桌面端接 ed25519 实现与 wasmtime，测试用 mock）。
//!
//! 全链路：
//! 1. [`ConverterHost::load`]：JSON → 契约类型 → 结构校验（`ab-contract`）→ 验签，任一步失败即拒载；
//! 2. [`ConverterHost::run_extract`]：构造 [`PathGuard`]（读路径的唯一来源）→ 交 [`SandboxRunner`]
//!    执行 → 校验输出契约结构 → 按有效等级裁剪（[`prune`]）。
//!
//! 业务规则各只写一份：等级裁剪 [`prune`] / 有效等级 [`effective_level`] 在 [`mod@level`]，
//! 路径判定在 [`mod@path`]。依赖全部经构造注入，无全局单例、静态门面、服务定位器。
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
