//! 转换器宿主：加载 · 验签 · 沙盒执行 · 等级裁剪 · 路径守卫的编排点。
//!
//! 编排只做"顺序 + 收敛错误"，不含任何新业务规则：等级规则在 [`crate::level`]，
//! 路径规则在 [`crate::path`]，结构校验在 `ab-contract`。

use ab_contract::{AccessLevel, ConverterManifest, StandardAccess};

use crate::error::{ConverterError, SandboxError};
use crate::level::{effective_level, prune};
use crate::path::PathGuard;
use crate::traits::{SandboxRequest, SandboxRunner, SignatureVerifier};

/// 加载 + 验签已完成的转换器。
///
/// 字段私有且无公开构造：未验签的清单无法进入 [`ConverterHost::run_extract`]，
/// "验签失败拒载"由类型系统兜住，而不是靠调用方自觉。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedConverter {
    manifest: ConverterManifest,
}

impl VerifiedConverter {
    /// 已验签的清单（宿主用于审计、UI 展示等级与版本）。
    pub fn manifest(&self) -> &ConverterManifest {
        &self.manifest
    }
}

/// 转换器宿主；两个能力经构造注入。
pub struct ConverterHost<V: SignatureVerifier, S: SandboxRunner> {
    verifier: V,
    runner: S,
}

impl<V: SignatureVerifier, S: SandboxRunner> ConverterHost<V, S> {
    /// 注入验签与沙盒执行能力。
    pub fn new(verifier: V, runner: S) -> Self {
        Self { verifier, runner }
    }

    /// ① 加载清单 + ② 验签。任一环节失败即拒载，不返回半成品。
    ///
    /// 顺序固定为"先结构校验、后验签"：结构都不合法的清单无需消耗验签实现的开销。
    pub fn load(&self, manifest_json: &str) -> Result<VerifiedConverter, ConverterError> {
        let manifest: ConverterManifest = serde_json::from_str(manifest_json)
            .map_err(|err| ConverterError::InvalidManifest(err.to_string()))?;
        manifest.validate()?;
        self.verifier
            .verify(&manifest)
            .map_err(|err| ConverterError::SignatureRejected(err.reason))?;
        Ok(VerifiedConverter { manifest })
    }

    /// ③ 沙盒执行 + ⑤ 路径守卫 + ④ 等级裁剪。
    ///
    /// `requested` 是调用方想要的等级；实际有效等级取清单能力与请求中更弱的一方。
    /// 沙盒输出先按契约校验结构，再裁剪——裁剪不替代校验，二者都不越权。
    pub fn run_extract(
        &self,
        converter: &VerifiedConverter,
        requested: AccessLevel,
    ) -> Result<StandardAccess, ConverterError> {
        let effective = effective_level(converter.manifest.level, requested);
        let guard = PathGuard::new(&converter.manifest.readable_paths);
        let request = SandboxRequest {
            manifest: &converter.manifest,
            level: effective,
            guard: &guard,
        };
        let mut access = self.runner.run(&request).map_err(map_sandbox_error)?;
        access.validate()?;
        prune(&mut access, effective);
        Ok(access)
    }
}

/// 沙盒错误 → 宿主统一错误的收敛；路径越界单独成类，便于审计与 UI 区分"越界"与"执行失败"。
fn map_sandbox_error(err: SandboxError) -> ConverterError {
    match err {
        SandboxError::PathDenied(denied) => ConverterError::PathOutsideScope(denied.path),
        SandboxError::Failed(reason) => ConverterError::SandboxFailed(reason),
    }
}
