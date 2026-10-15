//! 转换器宿主：加载 · 验签 · 沙盒执行 · 等级裁剪 · 路径守卫的编排点。
//! 只做顺序与错误收敛；等级规则在 [`crate::level`]，路径规则在 [`crate::path`]。

use ab_contract::{AccessLevel, ConverterManifest, StandardAccess};

use crate::error::{ConverterError, SandboxError};
use crate::level::{effective_level, prune};
use crate::path::PathGuard;
use crate::traits::{SandboxRequest, SandboxRunner, SignatureVerifier};

/// 加载 + 验签完成的转换器。字段私有：未验签的进不了 `run_extract`。
/// `module` 是验签时那份字节，执行时原样交沙盒——验过的就是执行的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedConverter {
    manifest: ConverterManifest,
    module: Vec<u8>,
}

impl VerifiedConverter {
    pub fn manifest(&self) -> &ConverterManifest {
        &self.manifest
    }

    pub fn module(&self) -> &[u8] {
        &self.module
    }
}

pub struct ConverterHost<V: SignatureVerifier, S: SandboxRunner> {
    verifier: V,
    runner: S,
}

impl<V: SignatureVerifier, S: SandboxRunner> ConverterHost<V, S> {
    pub fn new(verifier: V, runner: S) -> Self {
        Self { verifier, runner }
    }

    /// 先结构校验后验签：结构非法的清单不必消耗验签开销。任一环节失败即拒载。
    pub fn load(
        &self,
        manifest_json: &str,
        module: &[u8],
    ) -> Result<VerifiedConverter, ConverterError> {
        let manifest: ConverterManifest = serde_json::from_str(manifest_json)
            .map_err(|err| ConverterError::InvalidManifest(err.to_string()))?;
        manifest.validate()?;
        self.verifier
            .verify(&manifest, module)
            .map_err(ConverterError::SignatureRejected)?;
        Ok(VerifiedConverter {
            manifest,
            module: module.to_vec(),
        })
    }

    /// 有效等级取清单能力与请求中更弱的一方；沙盒输出先校验后裁剪。
    pub fn run_extract(
        &self,
        converter: &VerifiedConverter,
        requested: AccessLevel,
    ) -> Result<StandardAccess, ConverterError> {
        let effective = effective_level(converter.manifest.level, requested);
        let guard = PathGuard::new(&converter.manifest.readable_paths);
        let request = SandboxRequest {
            manifest: &converter.manifest,
            module: &converter.module,
            level: effective,
            guard: &guard,
        };
        let mut access = self.runner.run(&request).map_err(map_sandbox_error)?;
        access.validate()?;
        if access.app_id != converter.manifest.id {
            return Err(ConverterError::SandboxFailed(format!(
                "产物 app_id ({}) 与清单 id ({}) 不一致",
                access.app_id, converter.manifest.id
            )));
        }
        prune(&mut access, effective);
        Ok(access)
    }
}

/// 越界单独成类，便于审计与 UI 区分「越界」与「执行失败」。
fn map_sandbox_error(err: SandboxError) -> ConverterError {
    match err {
        SandboxError::PathDenied(denied) => ConverterError::PathOutsideScope(denied.path),
        SandboxError::Failed(reason) => ConverterError::SandboxFailed(reason),
    }
}
