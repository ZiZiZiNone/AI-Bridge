//! 错误类型：验签、路径守卫、沙盒执行、加载链路。

use ab_contract::ContractError;
use std::fmt;

/// 闭合枚举：拒载原因不允许自由文本（防日志注入与信息泄露）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    SignerNotAllowed,
    BadSignature,
    /// 模块字节与清单 `module_sha256` 不符（防「换模块保清单」）。
    ModuleHashMismatch,
    UnsupportedAlgorithm,
    Malformed,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            VerifyError::SignerNotAllowed => "签名者不受信",
            VerifyError::BadSignature => "签名与清单内容不匹配",
            VerifyError::ModuleHashMismatch => "模块字节与 module_sha256 不符",
            VerifyError::UnsupportedAlgorithm => "签名算法不受支持（仅 ed25519）",
            VerifyError::Malformed => "签名值或摘要格式非法",
        };
        f.write_str(s)
    }
}

impl std::error::Error for VerifyError {}

/// 请求路径不在 `readable_paths` 之内（或含 `..` 穿越）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDenied {
    /// 原样回显，便于审计定位。
    pub path: String,
}

impl PathDenied {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

impl fmt::Display for PathDenied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "路径越界，拒绝访问：{}", self.path)
    }
}

impl std::error::Error for PathDenied {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// 越界必须拒绝整次执行，不得降级放行。
    PathDenied(PathDenied),
    Failed(String),
}

impl SandboxError {
    pub fn failed(reason: impl Into<String>) -> Self {
        Self::Failed(reason.into())
    }
}

impl From<PathDenied> for SandboxError {
    fn from(denied: PathDenied) -> Self {
        Self::PathDenied(denied)
    }
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::PathDenied(denied) => write!(f, "{denied}"),
            SandboxError::Failed(reason) => write!(f, "沙盒执行失败：{reason}"),
        }
    }
}

impl std::error::Error for SandboxError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConverterError {
    InvalidManifest(String),
    Contract(ContractError),
    SignatureRejected(VerifyError),
    PathOutsideScope(String),
    SandboxFailed(String),
}

impl From<ContractError> for ConverterError {
    fn from(err: ContractError) -> Self {
        Self::Contract(err)
    }
}

impl fmt::Display for ConverterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConverterError::InvalidManifest(reason) => write!(f, "转换器清单解析失败：{reason}"),
            ConverterError::Contract(err) => write!(f, "转换器清单结构校验失败：{err}"),
            ConverterError::SignatureRejected(reason) => {
                write!(f, "验签失败，拒绝加载转换器：{reason}")
            }
            ConverterError::PathOutsideScope(path) => write!(f, "路径越界，拒绝执行：{path}"),
            ConverterError::SandboxFailed(reason) => write!(f, "沙盒执行失败：{reason}"),
        }
    }
}

impl std::error::Error for ConverterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConverterError::Contract(err) => Some(err),
            ConverterError::SignatureRejected(err) => Some(err),
            _ => None,
        }
    }
}
