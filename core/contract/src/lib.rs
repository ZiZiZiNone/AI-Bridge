//! `ab-contract` —— 契约类型、结构校验、官方信任根常量与转换器签名规范化字节；唯一事实源 `contracts/*.schema.json`。
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

pub const SCHEMA_VERSION: &str = "1.0";

/// 官方转换器签名者标识（唯一信任根 ID，见 `docs/安全模型.md` 附录 A）。
pub const OFFICIAL_SIGNER_ID: &str = "aibridge-official-2026";

/// 官方转换器 Ed25519 验签公钥（小写十六进制，64 字符 / 32 字节）。
pub const OFFICIAL_CONVERTER_PUBKEY_HEX: &str =
    "2c6a2895c4b3fce893ca6626c15c9d08dcf4a9a1b3f7d42d9346e8dd4203afd1";

/// 官方转换器 Ed25519 验签公钥原始字节（32 字节）。
pub const OFFICIAL_CONVERTER_PUBKEY: [u8; 32] = [
    44, 106, 40, 149, 196, 179, 252, 232, 147, 202, 102, 38, 193, 92, 157, 8, 220, 244, 169, 161,
    179, 247, 212, 45, 147, 70, 232, 221, 66, 3, 175, 209,
];

const REDACTED: &str = "[REDACTED]";

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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl fmt::Debug for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cookie")
            .field("name", &self.name)
            .field("value", &REDACTED)
            .field("domain", &self.domain)
            .field("path", &self.path)
            .field("secure", &self.secure)
            .field("http_only", &self.http_only)
            .field("same_site", &self.same_site)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub kind: TargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlainCredential {
    pub username: String,
    pub secret: String,
}

impl fmt::Debug for PlainCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlainCredential")
            .field("username", &self.username)
            .field("secret", &REDACTED)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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

impl fmt::Debug for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Payload")
            .field("cookies", &self.cookies)
            .field("token", &self.token.as_deref().map(|_| REDACTED))
            .field("credential", &self.credential)
            .field(
                "local_storage",
                &self.local_storage.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeSpec {
    pub domain_allowlist: Vec<String>,
    pub read_only: bool,
    pub single_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientEnv {
    pub user_agent: String,
    pub platform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl fmt::Debug for CredentialRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialRecord")
            .field("account_ref", &self.account_ref)
            .field("kind", &self.kind)
            .field("username", &self.username)
            .field("secret", &self.secret.as_deref().map(|_| REDACTED))
            .field("totp_seed", &self.totp_seed.as_deref().map(|_| REDACTED))
            .field("url", &self.url)
            .finish()
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_env: Option<ClientEnv>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_env: Option<ClientEnv>,
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopeSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_env: Option<ClientEnv>,
}

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
    pub module_sha256: String,
    pub signature: Signature,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    UnsupportedSchemaVersion(String),
    InvalidId { field: &'static str, value: String },
    EmptyField(&'static str),
    TargetFieldMissing(&'static str),
    PayloadMismatch { kind: SessionKind, expected: &'static str },
    UnsupportedAlgorithm(String),
    MissingExpiry,
    MissingScope,
    PlaceholderSignature,
    InvalidModuleHash,
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
            ContractError::MissingScope => write!(f, "临时会话必须携带 scope"),
            ContractError::PlaceholderSignature => write!(f, "签名是占位值（全零），拒绝加载"),
            ContractError::InvalidModuleHash => {
                write!(f, "module_sha256 必须是 64 位小写 hex 且不得为占位值")
            }
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

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_all_zero_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b == b'0')
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

/// 转换器清单官方签名的唯一规范化字节实现（`converters/sdk/README.md` §4）：
/// 去掉 `signature` 字段后，按 JSON 键升序紧凑序列化（无多余空白、UTF-8、不转义非 ASCII）。
/// 使用 `BTreeMap` 固定字典序，不受工作区 `serde_json/preserve_order` 特性影响。
pub fn canonical_signing_bytes(manifest: &ConverterManifest) -> Vec<u8> {
    let mut map: BTreeMap<&'static str, Value> = BTreeMap::new();
    map.insert("entry", Value::String(manifest.entry.clone()));
    map.insert("id", Value::String(manifest.id.clone()));
    let level_str = match manifest.level {
        AccessLevel::C => "c",
        AccessLevel::B => "b",
        AccessLevel::A => "a",
    };
    map.insert("level", Value::String(level_str.to_string()));
    if let Some(min_app_version) = &manifest.min_app_version {
        map.insert("min_app_version", Value::String(min_app_version.clone()));
    }
    map.insert("module_sha256", Value::String(manifest.module_sha256.clone()));
    map.insert("name", Value::String(manifest.name.clone()));
    map.insert(
        "readable_paths",
        Value::Array(
            manifest
                .readable_paths
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    map.insert("version", Value::String(manifest.version.clone()));
    serde_json::to_vec(&map).unwrap_or_default()
}

impl SessionRecord {
    pub fn validate(&self) -> Result<(), ContractError> {
        check_target(&self.target)?;
        check_payload(self.kind, &self.payload)
    }
}

impl SessionEnvelope {
    pub fn validate(&self) -> Result<(), ContractError> {
        check_schema_version(&self.schema_version)?;
        self.as_record().validate()
    }

    pub fn validate_temporary(&self) -> Result<(), ContractError> {
        self.validate()?;
        if self.expires_at.as_deref().unwrap_or("").is_empty() {
            return Err(ContractError::MissingExpiry);
        }
        if self.scope.is_none() {
            return Err(ContractError::MissingScope);
        }
        Ok(())
    }

    fn as_record(&self) -> SessionRecord {
        SessionRecord {
            account_ref: self.account_ref.clone(),
            target: self.target.clone(),
            kind: self.kind,
            payload: self.payload.clone(),
            expires_at: self.expires_at.clone(),
            source: self.source.clone(),
            client_env: self.client_env.clone(),
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
        if self.module_sha256.is_empty() {
            return Err(ContractError::EmptyField("module_sha256"));
        }
        if !is_lower_hex(&self.module_sha256, 64) || is_all_zero_hex(&self.module_sha256) {
            return Err(ContractError::InvalidModuleHash);
        }
        if self.signature.algorithm != "ed25519" {
            return Err(ContractError::UnsupportedAlgorithm(
                self.signature.algorithm.clone(),
            ));
        }
        if self.signature.signer.is_empty() || self.signature.value.is_empty() {
            return Err(ContractError::EmptyField("signature"));
        }
        if is_all_zero_hex(&self.signature.value) {
            return Err(ContractError::PlaceholderSignature);
        }
        Ok(())
    }

    pub fn canonical_signing_bytes(&self) -> Vec<u8> {
        canonical_signing_bytes(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE_SHA256: &str = "3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788";
    const SIGNATURE_128: &str = "6f1d0b2a4c8e7395a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f6071\
                                 8899aabbccddeeff00112233445566778899aabbccddeeff0011223344556677";

    fn cookie() -> Cookie {
        Cookie {
            name: "sid".into(),
            value: "abc-secret-cookie".into(),
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

    fn scope() -> ScopeSpec {
        ScopeSpec {
            domain_allowlist: vec!["github.com".into()],
            read_only: true,
            single_use: true,
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
            scope: None,
            client_env: None,
        }
    }

    #[test]
    fn official_trust_root_constants_are_consistent() {
        assert_eq!(OFFICIAL_SIGNER_ID, "aibridge-official-2026");
        assert_eq!(OFFICIAL_CONVERTER_PUBKEY_HEX.len(), 64);
        let hex_from_bytes: String = OFFICIAL_CONVERTER_PUBKEY
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hex_from_bytes, OFFICIAL_CONVERTER_PUBKEY_HEX);
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
    fn temporary_requires_expiry_and_scope() {
        assert_eq!(envelope().validate_temporary(), Err(ContractError::MissingExpiry));

        let mut env = envelope();
        env.expires_at = Some("2026-10-09T01:00:00Z".into());
        assert_eq!(env.validate_temporary(), Err(ContractError::MissingScope));

        env.scope = Some(scope());
        assert!(env.validate_temporary().is_ok());
    }

    #[test]
    fn scope_and_client_env_roundtrip() {
        let mut env = envelope();
        env.scope = Some(scope());
        env.client_env = Some(ClientEnv {
            user_agent: "Mozilla/5.0 (Windows NT 10.0)".into(),
            platform: "windows".into(),
            device_id: Some("dev-fp-1".into()),
        });
        let json = serde_json::to_string(&env).unwrap();
        let back: SessionEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env, back);
        let s = back.scope.unwrap();
        assert_eq!(s.domain_allowlist, vec!["github.com".to_string()]);
        assert!(s.read_only);
        assert!(s.single_use);
        assert_eq!(back.client_env.unwrap().platform, "windows");
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
    fn debug_output_redacts_secrets() {
        let env = envelope();
        let text = format!("{env:?}");
        assert!(text.contains("[REDACTED]"));
        assert!(!text.contains("abc-secret-cookie"));

        let mut cred = CredentialRecord {
            account_ref: "acc-1".into(),
            kind: CredentialKind::Totp,
            username: Some("john".into()),
            secret: Some("s3cret-value".into()),
            totp_seed: Some("JBSWY3DPEHPK3PXP".into()),
            url: None,
        };
        let text = format!("{cred:?}");
        assert!(!text.contains("s3cret-value"));
        assert!(!text.contains("JBSWY3DPEHPK3PXP"));
        assert!(text.contains("john"));

        cred.secret = None;
        cred.totp_seed = None;
        assert_eq!(
            format!("{cred:?}"),
            "CredentialRecord { account_ref: \"acc-1\", kind: Totp, username: Some(\"john\"), \
             secret: None, totp_seed: None, url: None }"
        );

        let payload = Payload {
            token: Some("bearer-token-value".into()),
            local_storage: BTreeMap::from([("ls_key".to_string(), "ls-secret".to_string())]),
            ..Payload::default()
        };
        let text = format!("{payload:?}");
        assert!(!text.contains("bearer-token-value"));
        assert!(!text.contains("ls-secret"));
        assert!(text.contains("ls_key"));
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
            module_sha256: MODULE_SHA256.into(),
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: "official".into(),
                value: SIGNATURE_128.into(),
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
    fn manifest_rejects_placeholder_signature_and_bad_module_hash() {
        let m = ConverterManifest {
            id: "qq".into(),
            name: "QQ".into(),
            version: "1.2.0".into(),
            level: AccessLevel::C,
            readable_paths: vec!["C:\\Users\\...\\QQ".into()],
            entry: "qq.wasm".into(),
            module_sha256: MODULE_SHA256.into(),
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: "official".into(),
                value: SIGNATURE_128.into(),
            },
            min_app_version: None,
        };

        let mut placeholder = m.clone();
        placeholder.signature.value = "0".repeat(128);
        assert_eq!(placeholder.validate(), Err(ContractError::PlaceholderSignature));

        let mut empty_hash = m.clone();
        empty_hash.module_sha256 = String::new();
        assert_eq!(empty_hash.validate(), Err(ContractError::EmptyField("module_sha256")));

        let mut bad_hash = m.clone();
        bad_hash.module_sha256 = "zz".repeat(32);
        assert_eq!(bad_hash.validate(), Err(ContractError::InvalidModuleHash));

        let mut short_hash = m.clone();
        short_hash.module_sha256 = "abcd".into();
        assert_eq!(empty_hash.validate(), Err(ContractError::EmptyField("module_sha256")));
        assert_eq!(short_hash.validate(), Err(ContractError::InvalidModuleHash));

        let mut placeholder_hash = m.clone();
        placeholder_hash.module_sha256 = "0".repeat(64);
        assert_eq!(placeholder_hash.validate(), Err(ContractError::InvalidModuleHash));
    }

    #[test]
    fn canonical_signing_bytes_matches_golden_vectors_and_ignores_signature() {
        let mut with_min = ConverterManifest {
            id: "qq".into(),
            name: "QQ 本地数据转换器".into(),
            version: "1.2.0".into(),
            level: AccessLevel::C,
            readable_paths: vec!["C:\\Users\\me\\AppData\\Roaming\\QQ".into()],
            entry: "qq.wasm".into(),
            module_sha256: MODULE_SHA256.into(),
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: "official-2026".into(),
                value: SIGNATURE_128.into(),
            },
            min_app_version: Some("0.1.0".into()),
        };

        let expected_with_min = concat!(
            r#"{"entry":"qq.wasm","id":"qq","level":"c","min_app_version":"0.1.0","#,
            r#""module_sha256":"3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788","#,
            r#""name":"QQ 本地数据转换器","readable_paths":["C:\\Users\\me\\AppData\\Roaming\\QQ"],"version":"1.2.0"}"#,
        );
        let bytes_with_min = canonical_signing_bytes(&with_min);
        assert_eq!(
            std::str::from_utf8(&bytes_with_min).unwrap(),
            expected_with_min
        );
        assert_eq!(with_min.canonical_signing_bytes(), bytes_with_min);

        with_min.signature.signer = "other-signer".into();
        with_min.signature.value = "a".repeat(128);
        assert_eq!(canonical_signing_bytes(&with_min), bytes_with_min);

        let mut without_min = with_min;
        without_min.min_app_version = None;
        let expected_without_min = concat!(
            r#"{"entry":"qq.wasm","id":"qq","level":"c","#,
            r#""module_sha256":"3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788","#,
            r#""name":"QQ 本地数据转换器","readable_paths":["C:\\Users\\me\\AppData\\Roaming\\QQ"],"version":"1.2.0"}"#,
        );
        assert_eq!(
            std::str::from_utf8(&canonical_signing_bytes(&without_min)).unwrap(),
            expected_without_min
        );
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
                client_env: None,
            }],
            meta: BTreeMap::new(),
            client_env: None,
        };
        assert!(sa.validate().is_ok());

        let mut bad = sa.clone();
        bad.app_id = "QQ!".into();
        assert!(bad.validate().is_err());
    }
}
