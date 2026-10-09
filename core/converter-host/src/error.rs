//! 错误类型：验签、路径守卫、沙盒执行、加载链路。
//!
//! 各错误自有类型，宿主侧的 trait 实现可独立产出；`ConverterHost` 统一收敛为 [`ConverterError`]。

use ab_contract::ContractError;
use std::fmt;

/// 验签失败：由注入的 `SignatureVerifier` 判定，本 crate 不持任何密码学实现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyError {
    /// 失败原因（由验签实现给出，用于审计与 UI 提示，不含密钥材料）。
    pub reason: String,
}

impl VerifyError {
    /// 构造验签失败。
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "验签失败：{}", self.reason)
    }
}

impl std::error::Error for VerifyError {}

/// 路径越界：请求路径不在转换器清单 `readable_paths` 之内（或含 `..` 路径穿越）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDenied {
    /// 被拒绝的原始请求路径（原样回显，便于审计定位）。
    pub path: String,
}

impl PathDenied {
    /// 构造路径越界。
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

/// 沙盒执行失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// 请求了白名单之外的路径：宿主必须拒绝整次执行，不得降级放行。
    PathDenied(PathDenied),
    /// 其它执行失败（模块内部错误、陷阱、输出不可用等）。
    Failed(String),
}

impl SandboxError {
    /// 构造一般执行失败。
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

/// 加载 / 执行链路的统一错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConverterError {
    /// 清单不是合法 JSON，或缺必填字段导致无法反序列化。
    InvalidManifest(String),
    /// 清单结构校验失败（`ab-contract` 判定）。
    Contract(ContractError),
    /// 验签失败，拒绝加载。
    SignatureRejected(String),
    /// 沙盒请求了 `readable_paths` 之外的路径。
    PathOutsideScope(String),
    /// 沙盒执行失败。
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
            _ => None,
        }
    }
}
