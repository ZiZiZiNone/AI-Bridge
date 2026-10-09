//! 注入点定义：验签与沙盒执行两个能力由宿主实现，本 crate 只定契约。
//!
//! 无全局单例 / 静态门面 / 服务定位器：能力一律经 `ConverterHost::new` 注入。

use ab_contract::{AccessLevel, ConverterManifest, StandardAccess};

use crate::error::{SandboxError, VerifyError};
use crate::path::PathGuard;

/// 验签能力：校验转换器清单的官方签名。
///
/// 实现方在宿主层（桌面端 ed25519 实现；测试用 mock）。本 crate 不引任何密码学依赖，
/// 也不解释 `signature.value` 的内容，只认"通过 / 不通过"。
pub trait SignatureVerifier {
    /// `Err` 即拒载：清单不得进入执行阶段。
    fn verify(&self, manifest: &ConverterManifest) -> Result<(), VerifyError>;
}

/// 一次沙盒执行的全部输入。
pub struct SandboxRequest<'a> {
    /// 已加载并验签的转换器清单（`entry` 即被执行的模块）。
    pub manifest: &'a ConverterManifest,
    /// 已按清单能力收敛的有效等级，转换器可据此跳过不必要的工作。
    pub level: AccessLevel,
    /// 只读路径白名单：运行器必须先经它取得路径，白名单之外无路可走。
    pub guard: &'a PathGuard,
}

/// 沙盒执行能力（后续接 wasmtime）。
///
/// 硬约束（由宿主实现保证）：无网络；只能只读 [`SandboxRequest::guard`] 放行的路径；
/// 只输出 [`StandardAccess`]；请求白名单之外的路径时必须经 [`SandboxError::PathDenied`] 失败退出。
pub trait SandboxRunner {
    /// 在沙盒内执行 `manifest.entry`，返回归一化后的标准接入数据。
    fn run(&self, request: &SandboxRequest<'_>) -> Result<StandardAccess, SandboxError>;
}
