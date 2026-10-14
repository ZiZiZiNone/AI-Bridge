//! 转换器宿主：加载 · 验签 · 沙盒执行 · 等级裁剪 · 路径守卫的编排点。

use ab_contract::{AccessLevel, ConverterManifest, StandardAccess};

use crate::error::{ConverterError, SandboxError};
use crate::level::{effective_level, prune};
use crate::path::PathGuard;
use crate::traits::{SandboxRequest, SandboxRunner, SignatureVerifier};

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

fn map_sandbox_error(err: SandboxError) -> ConverterError {
    match err {
        SandboxError::PathDenied(denied) => ConverterError::PathOutsideScope(denied.path),
        SandboxError::Failed(reason) => ConverterError::SandboxFailed(reason),
    }
}
