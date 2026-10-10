//! 注入点定义：验签与沙盒执行由宿主实现，本 crate 只定契约。

use ab_contract::{AccessLevel, ConverterManifest, StandardAccess};

use crate::error::{SandboxError, VerifyError};
use crate::path::PathGuard;

/// 验签 + 模块字节一致性；实现方在宿主层（本 crate 不引密码学）。
///
/// 实现义务：签名覆盖清单全部字段（含 `module_sha256`）且非占位；
/// `sha256(module)` 与 `manifest.module_sha256` 相符。任一不符返回 [`VerifyError`]。
pub trait SignatureVerifier {
    /// `Err` 即拒载。
    fn verify(&self, manifest: &ConverterManifest, module: &[u8]) -> Result<(), VerifyError>;
}

pub struct SandboxRequest<'a> {
    pub manifest: &'a ConverterManifest,
    /// 已验签的模块字节：执行的就是这份，防止验完换文件。
    pub module: &'a [u8],
    pub level: AccessLevel,
    pub guard: &'a PathGuard,
}

/// 沙盒硬约束（实现保证）：无网络；只读 [`SandboxRequest::guard`] 放行的路径；
/// 只输出 [`StandardAccess`]；越界路径必须经 [`SandboxError::PathDenied`] 失败退出。
pub trait SandboxRunner {
    fn run(&self, request: &SandboxRequest<'_>) -> Result<StandardAccess, SandboxError>;
}
