//! `ab-contract` —— 契约层唯一实现：数据定义 + 结构校验，**零业务规则**。
//!
//! 契约唯一事实源为仓库根 `contracts/*.schema.json`；本 crate 的类型与其逐字段一致。
//! 消费者：core 全部 crate、转换器 SDK、扩展/移动的信封编解码。
//!
//! 分工边界：schema 管"数据长什么样"，本 crate 的 `validate` 管"字段之间的结构一致性"，
//! 业务语义（TTL 策略、scope 授权等）一律在 `ab-broker` / `ab-mcp`，不许写进本 crate。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

/// 契约版本；`schema_version` 与之不符一律拒绝。
pub const SCHEMA_VERSION: &str = "1.0";

/* ---------------- 枚举 ---------------- */

/// 接入等级：c = 会话注入，b = 凭据填表，a = 仅启动。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessLevel {
    C,
    B,
    A,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    Password,
    Passkey,
    Totp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Cookie,
    Token,
    Credential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Web,
    App,
    Container,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Extension,
    Converter,
    Mobile,
    Manual,
    Sync,
}

/* ---------------- 公共结构 ---------------- */

/// cookie 必须带完整属性：会话保真度关键，任何环节不得丢弃。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_site: Option<SameSite>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub kind: TargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlainCredential {
    pub username: String,
    pub secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Payload {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cookies: Vec<Cookie>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<PlainCredential>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub local_storage: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub id: String,
}

/* ---------------- StandardAccess（standard-access.schema.json） ---------------- */

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub account_ref: String,
    pub kind: CredentialKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub account_ref: String,
    pub target: Target,
    pub kind: SessionKind,
    pub payload: Payload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandardAccess {
    pub schema_version: String,
    pub app_id: String,
    pub level: AccessLevel,
    pub generated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<CredentialRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<SessionRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, Value>,
}

/* ---------------- SessionEnvelope（session-envelope.schema.json） ---------------- */

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEnvelope {
    pub schema_version: String,
    pub account_ref: String,
    pub target: Target,
    pub kind: SessionKind,
    pub payload: Payload,
    pub issued_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub source: Source,
}

/* ---------------- ConverterManifest（converter-manifest.schema.json） ---------------- */

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub algorithm: String,
    pub signer: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConverterManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub level: AccessLevel,
    pub readable_paths: Vec<String>,
    pub entry: String,
    pub signature: Signature,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
}

/* ---------------- 校验 ---------------- */

/// 结构校验错误。业务语义错误不在此列（属 ab-broker / ab-mcp）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    UnsupportedSchemaVersion(String),
    InvalidId { field: &'static str, value: String },
    EmptyField(&'static str),
    TargetFieldMissing(&'static str),
    PayloadMismatch { kind: SessionKind, expected: &'static str },
    UnsupportedAlgorithm(String),
    MissingExpiry,
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractError::UnsupportedSchemaVersion(v) => {
                write!(f, "不支持的契约版本 {v}（当前 {SCHEMA_VERSION}）")
            }
            ContractError::InvalidId { field, value } => {
                write!(f, "{field} 非法：{value}（要求 ^[a-z0-9][a-z0-9-]{{1,63}}$）")
            }
            ContractError::EmptyField(name) => write!(f, "{name} 不能为空"),
            ContractError::TargetFieldMissing(name) => write!(f, "target 缺少必填字段 {name}"),
            ContractError::PayloadMismatch { kind, expected } => {
                write!(f, "kind={kind:?} 与 payload 不一致，应提供 {expected}")
            }
            ContractError::UnsupportedAlgorithm(a) => write!(f, "不支持的签名算法 {a}（仅 ed25519）"),
            ContractError::MissingExpiry => write!(f, "临时会话必须携带 expires_at"),
        }
    }
}

impl std::error::Error for ContractError {}

fn valid_id(s: &str) -> bool {
    let len = s.len();
    if !(2..=64).contains(&len) {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn check_schema_version(v: &str) -> Result<(), ContractError> {
    if v == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ContractError::UnsupportedSchemaVersion(v.to_string()))
    }
}

fn check_target(t: &Target) -> Result<(), ContractError> {
    match t.kind {
        TargetKind::Web => {
            if t.domain.as_deref().unwrap_or("").is_empty() {
                return Err(ContractError::TargetFieldMissing("domain"));
            }
        }
        TargetKind::App => {
            if t.app_id.as_deref().unwrap_or("").is_empty() {
                return Err(ContractError::TargetFieldMissing("app_id"));
            }
        }
        TargetKind::Container => {}
    }
    Ok(())
}

fn check_payload(kind: SessionKind, payload: &Payload) -> Result<(), ContractError> {
    match kind {
        SessionKind::Cookie => {
            if payload.cookies.is_empty() {
                return Err(ContractError::PayloadMismatch {
                    kind,
                    expected: "cookies",
                });
            }
        }
        SessionKind::Token => {
            if payload.token.as_deref().unwrap_or("").is_empty() {
                return Err(ContractError::PayloadMismatch { kind, expected: "token" });
            }
        }
        SessionKind::Credential => {
            if payload.credential.is_none() {
                return Err(ContractError::PayloadMismatch {
                    kind,
                    expected: "credential",
                });
            }
        }
    }
    Ok(())
}

impl SessionRecord {
    pub fn validate(&self) -> Result<(), ContractError> {
        check_target(&self.target)?;
        check_payload(self.kind, &self.payload)
    }
}

impl SessionEnvelope {
    /// 结构校验：schema_version、target、kind/payload 一致性。
    pub fn validate(&self) -> Result<(), ContractError> {
        check_schema_version(&self.schema_version)?;
        self.as_record().validate()
    }

    /// 临时会话校验：结构校验 + 必须携带过期时间（TTL 语义由 ab-broker 管）。
    pub fn validate_temporary(&self) -> Result<(), ContractError> {
        self.validate()?;
        if self.expires_at.as_deref().unwrap_or("").is_empty() {
            return Err(ContractError::MissingExpiry);
        }
        Ok(())
    }

    /// 复用 SessionRecord 的结构校验（两者会话部分字段一致）。
    fn as_record(&self) -> SessionRecord {
        SessionRecord {
            account_ref: self.account_ref.clone(),
            target: self.target.clone(),
            kind: self.kind,
            payload: self.payload.clone(),
            expires_at: self.expires_at.clone(),
            source: self.source.clone(),
        }
    }
}

impl StandardAccess {
    pub fn validate(&self) -> Result<(), ContractError> {
        check_schema_version(&self.schema_version)?;
        if !valid_id(&self.app_id) {
            return Err(ContractError::InvalidId {
                field: "app_id",
                value: self.app_id.clone(),
            });
        }
        if self.generated_at.is_empty() {
            return Err(ContractError::EmptyField("generated_at"));
        }
        for c in &self.credentials {
            if c.account_ref.is_empty() {
                return Err(ContractError::EmptyField("credentials.account_ref"));
            }
        }
        for s in &self.sessions {
            s.validate()?;
        }
        Ok(())
    }
}

impl ConverterManifest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !valid_id(&self.id) {
            return Err(ContractError::InvalidId {
                field: "id",
                value: self.id.clone(),
            });
        }
        if self.name.is_empty() {
            return Err(ContractError::EmptyField("name"));
        }
        if self.version.is_empty() {
            return Err(ContractError::EmptyField("version"));
        }
        if self.readable_paths.is_empty() {
            return Err(ContractError::EmptyField("readable_paths"));
        }
        if self.entry.is_empty() {
            return Err(ContractError::EmptyField("entry"));
        }
        if self.signature.algorithm != "ed25519" {
            return Err(ContractError::UnsupportedAlgorithm(
                self.signature.algorithm.clone(),
            ));
        }
        if self.signature.signer.is_empty() || self.signature.value.is_empty() {
            return Err(ContractError::EmptyField("signature"));
        }
        Ok(())
    }
}

/* ---------------- 测试 ---------------- */

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie() -> Cookie {
        Cookie {
            name: "sid".into(),
            value: "abc".into(),
            domain: Some("github.com".into()),
            path: Some("/".into()),
            secure: Some(true),
            http_only: Some(true),
            same_site: Some(SameSite::Lax),
            expires_at: Some("2026-12-01T00:00:00Z".into()),
        }
    }

    fn source() -> Source {
        Source {
            source_type: SourceType::Extension,
            id: "chromium".into(),
        }
    }

    fn envelope() -> SessionEnvelope {
        SessionEnvelope {
            schema_version: SCHEMA_VERSION.into(),
            account_ref: "acc-1".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some("github.com".into()),
                app_id: None,
            },
            kind: SessionKind::Cookie,
            payload: Payload {
                cookies: vec![cookie()],
                ..Payload::default()
            },
            issued_at: "2026-10-09T00:00:00Z".into(),
            expires_at: None,
            source: source(),
        }
    }

    #[test]
    fn envelope_roundtrip_keeps_cookie_attributes() {
        let env = envelope();
        let json = serde_json::to_string(&env).unwrap();
        let back: SessionEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env, back);
        let c = &back.payload.cookies[0];
        assert_eq!(c.http_only, Some(true));
        assert_eq!(c.secure, Some(true));
        assert_eq!(c.same_site, Some(SameSite::Lax));
        assert!(c.expires_at.is_some());
    }

    #[test]
    fn envelope_valid() {
        assert!(envelope().validate().is_ok());
    }

    #[test]
    fn payload_kind_mismatch_rejected() {
        let mut env = envelope();
        env.payload.cookies.clear();
        assert_eq!(
            env.validate(),
            Err(ContractError::PayloadMismatch {
                kind: SessionKind::Cookie,
                expected: "cookies"
            })
        );
    }

    #[test]
    fn web_target_requires_domain() {
        let mut env = envelope();
        env.target.domain = None;
        assert_eq!(
            env.validate(),
            Err(ContractError::TargetFieldMissing("domain"))
        );
    }

    #[test]
    fn temporary_requires_expiry() {
        let env = envelope();
        assert_eq!(env.validate_temporary(), Err(ContractError::MissingExpiry));
        let mut env2 = envelope();
        env2.expires_at = Some("2026-10-09T01:00:00Z".into());
        assert!(env2.validate_temporary().is_ok());
    }

    #[test]
    fn schema_version_mismatch_rejected() {
        let mut env = envelope();
        env.schema_version = "0.9".into();
        assert!(matches!(
            env.validate(),
            Err(ContractError::UnsupportedSchemaVersion(_))
        ));
    }

    #[test]
    fn manifest_valid_and_rejects_bad_shapes() {
        let m = ConverterManifest {
            id: "qq".into(),
            name: "QQ".into(),
            version: "1.2.0".into(),
            level: AccessLevel::C,
            readable_paths: vec!["C:\\Users\\...\\QQ".into()],
            entry: "qq.wasm".into(),
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: "official".into(),
                value: "aabb".into(),
            },
            min_app_version: None,
        };
        assert!(m.validate().is_ok());

        let mut bad_id = m.clone();
        bad_id.id = "Bad!".into();
        assert!(bad_id.validate().is_err());

        let mut no_paths = m.clone();
        no_paths.readable_paths.clear();
        assert_eq!(no_paths.validate(), Err(ContractError::EmptyField("readable_paths")));

        let mut bad_algo = m.clone();
        bad_algo.signature.algorithm = "rsa".into();
        assert!(matches!(
            bad_algo.validate(),
            Err(ContractError::UnsupportedAlgorithm(_))
        ));
    }

    #[test]
    fn standard_access_valid_and_checks_sessions() {
        let sa = StandardAccess {
            schema_version: SCHEMA_VERSION.into(),
            app_id: "qq".into(),
            level: AccessLevel::C,
            generated_at: "2026-10-09T00:00:00Z".into(),
            credentials: vec![CredentialRecord {
                account_ref: "acc-1".into(),
                kind: CredentialKind::Password,
                username: Some("john".into()),
                secret: Some("s3cret".into()),
                totp_seed: None,
                url: Some("https://github.com".into()),
            }],
            sessions: vec![SessionRecord {
                account_ref: "acc-1".into(),
                target: Target {
                    kind: TargetKind::Web,
                    domain: Some("github.com".into()),
                    app_id: None,
                },
                kind: SessionKind::Cookie,
                payload: Payload {
                    cookies: vec![cookie()],
                    ..Payload::default()
                },
                expires_at: None,
                source: source(),
            }],
            meta: BTreeMap::new(),
        };
        assert!(sa.validate().is_ok());

        let mut bad = sa.clone();
        bad.app_id = "QQ!".into();
        assert!(bad.validate().is_err());
    }
}
