//! UDL 类型镜像：`mobile/ffi/aibridge.udl` 声明的记录 / 枚举 / 错误逐个定义。
//!
//! 机械镜像纪律：
//! - 字段名、可选性与 UDL 逐字段一致（生成脚手架只消费签名，类型由本层提供）；
//! - 秘密字段的 `Debug` 沿用 core 脱敏口径（`[REDACTED]`），错误文本不含秘密；
//! - 本文件不含任何业务规则，仅承载结构。
//!
//! 枚举变体名逐字沿用 UDL（SCREAMING_SNAKE / 大写单字母），故关闭命名风格告警。
#![allow(non_camel_case_types)]

use std::collections::HashMap;
use std::fmt;

const REDACTED: &str = "[REDACTED]";

/* ============================== 枚举（UDL `enum`） ============================== */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessLevel {
    C,
    B,
    A,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CredentialKind {
    PASSWORD,
    PASSKEY,
    TOTP,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionKind {
    COOKIE,
    TOKEN,
    CREDENTIAL,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetKind {
    WEB,
    APP,
    CONTAINER,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SameSite {
    STRICT,
    LAX,
    NONE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceType {
    EXTENSION,
    CONVERTER,
    MOBILE,
    MANUAL,
    SYNC,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditAction {
    UNLOCK,
    LOCK,
    CAPTURE,
    INJECT,
    FILL,
    ISSUE,
    RENEW,
    RELEASE,
    REVOKE,
    REVOKE_ALL,
    SWEEP,
    USE,
    EXTRACT,
    CONVERTER_LOAD,
    CONVERTER_REJECT,
    PAIR,
    SYNC,
    VAULT_WRITE,
    VAULT_DELETE,
    RESTORE,
    EXPORT,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditResult {
    OK,
    DENIED,
    FAILED,
}

/* ============================== 错误（UDL `[Error]`） ============================== */

/// core 内部错误的统一出口（UDL 平坦错误）。
/// 跨 FFI 只降维成稳定变体 + `Display` 文本，文本不含任何秘密。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreError {
    CONTRACT,
    CRYPTO,
    STORAGE,
    NOT_FOUND,
    DENIED,
    TRANSPORT,
    CONVERSION,
    INTERNAL,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            CoreError::CONTRACT => "契约校验失败",
            CoreError::CRYPTO => "密码学操作失败",
            CoreError::STORAGE => "存储读写失败",
            CoreError::NOT_FOUND => "对象不存在",
            CoreError::DENIED => "操作被拒绝",
            CoreError::TRANSPORT => "传输信道失败",
            CoreError::CONVERSION => "数据格式转换失败",
            CoreError::INTERNAL => "内部错误（含 UDL↔core 未对齐面，见 core/ffi/README.md）",
        };
        f.write_str(s)
    }
}

impl std::error::Error for CoreError {}

/// `ab_contract::ContractError` 逐变体镜像（带字段错误枚举）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    UnsupportedSchemaVersion { version: String },
    InvalidId { field: String, value: String },
    EmptyField { name: String },
    TargetFieldMissing { name: String },
    PayloadMismatch { kind: SessionKind, expected: String },
    UnsupportedAlgorithm { algorithm: String },
    MissingExpiry,
    MissingScope,
    PlaceholderSignature,
    InvalidModuleHash,
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 文本口径与 core `ab_contract::ContractError::Display` 一致（无秘密字段）。
        match self {
            ContractError::UnsupportedSchemaVersion { version } => {
                write!(f, "不支持的契约版本 {version}（当前 1.0）")
            }
            ContractError::InvalidId { field, value } => {
                write!(f, "{field} 非法：{value}（要求 ^[a-z0-9][a-z0-9-]{{1,63}}$）")
            }
            ContractError::EmptyField { name } => write!(f, "{name} 不能为空"),
            ContractError::TargetFieldMissing { name } => {
                write!(f, "target 缺少必填字段 {name}")
            }
            ContractError::PayloadMismatch { kind, expected } => {
                write!(f, "kind={kind:?} 与 payload 不一致，应提供 {expected}")
            }
            ContractError::UnsupportedAlgorithm { algorithm } => {
                write!(f, "不支持的签名算法 {algorithm}（仅 ed25519）")
            }
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

/* ============================== 记录（UDL `dictionary`） ============================== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreConfig {
    pub data_dir: String,
    pub display_name: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub http_only: Option<bool>,
    pub same_site: Option<SameSite>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub kind: TargetKind,
    pub domain: Option<String>,
    pub app_id: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Clone, PartialEq, Eq, Default)]
pub struct Payload {
    pub cookies: Vec<Cookie>,
    pub token: Option<String>,
    pub credential: Option<PlainCredential>,
    pub local_storage: HashMap<String, String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub source_type: SourceType,
    pub id: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialRecord {
    pub account_ref: String,
    pub kind: CredentialKind,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub totp_seed: Option<String>,
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

#[derive(Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub account_ref: String,
    pub target: Target,
    pub kind: SessionKind,
    pub payload: Payload,
    pub expires_at: Option<String>,
    pub source: Source,
    pub client_env: Option<ClientEnv>,
}

impl fmt::Debug for SessionRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionRecord")
            .field("account_ref", &self.account_ref)
            .field("target", &self.target)
            .field("kind", &self.kind)
            .field("payload", &self.payload)
            .field("expires_at", &self.expires_at)
            .field("source", &self.source)
            .field("client_env", &self.client_env)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SessionEnvelope {
    pub schema_version: String,
    pub account_ref: String,
    pub target: Target,
    pub kind: SessionKind,
    pub payload: Payload,
    pub issued_at: String,
    pub expires_at: Option<String>,
    pub source: Source,
    pub scope: Option<ScopeSpec>,
    pub client_env: Option<ClientEnv>,
}

impl fmt::Debug for SessionEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionEnvelope")
            .field("schema_version", &self.schema_version)
            .field("account_ref", &self.account_ref)
            .field("target", &self.target)
            .field("kind", &self.kind)
            .field("payload", &self.payload)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("source", &self.source)
            .field("scope", &self.scope)
            .field("client_env", &self.client_env)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct StandardAccess {
    pub schema_version: String,
    pub app_id: String,
    pub level: AccessLevel,
    pub generated_at: String,
    pub credentials: Vec<CredentialRecord>,
    pub sessions: Vec<SessionRecord>,
    pub meta: HashMap<String, String>,
    pub client_env: Option<ClientEnv>,
}

impl fmt::Debug for StandardAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StandardAccess")
            .field("schema_version", &self.schema_version)
            .field("app_id", &self.app_id)
            .field("level", &self.level)
            .field("generated_at", &self.generated_at)
            .field("credentials", &self.credentials)
            .field("sessions", &self.sessions)
            .field("meta", &self.meta.keys().collect::<Vec<_>>())
            .field("client_env", &self.client_env)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub algorithm: String,
    pub signer: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConverterManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub level: AccessLevel,
    pub readable_paths: Vec<String>,
    pub entry: String,
    pub module_sha256: String,
    pub signature: Signature,
    pub min_app_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySummary {
    pub account_ref: String,
    pub title: String,
    pub kind: CredentialKind,
    pub username: Option<String>,
    pub url: Option<String>,
    /// UDL 有此字段、core `ab_vault::Entry` 无对应时间戳 → 恒为 `None`（见 README 漂移表）。
    pub updated_at: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub account_ref: String,
    pub title: String,
    pub kind: CredentialKind,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub totp_seed: Option<String>,
    pub url: Option<String>,
    pub note: Option<String>,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("account_ref", &self.account_ref)
            .field("title", &self.title)
            .field("kind", &self.kind)
            .field("username", &self.username)
            .field("secret", &self.secret.as_deref().map(|_| REDACTED))
            .field("totp_seed", &self.totp_seed.as_deref().map(|_| REDACTED))
            .field("url", &self.url)
            .field("note", &self.note.as_deref().map(|_| REDACTED))
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorSpec {
    pub length: u32,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
}

/// A5.2 口令词模式（core `ab_vault::PassphraseSpec` 镜像）：词表/词数/分隔符全由调用方配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseSpec {
    pub words: Vec<String>,
    pub word_count: u32,
    pub separator: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct GeneratedSecret {
    pub value: String,
    pub charset: String,
}

impl fmt::Debug for GeneratedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedSecret")
            .field("value", &REDACTED)
            .field("charset", &self.charset)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixRow {
    pub account_ref: String,
    pub target_kind: TargetKind,
    pub domain: Option<String>,
    pub app_id: Option<String>,
    pub source_type: SourceType,
    pub expires_at: Option<String>,
    /// UDL 有此字段、core 矩阵行无「最后所见」维度 → 恒为 `None`（见 README 漂移表）。
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotSummary {
    pub snapshot_id: String,
    pub created_at: String,
    pub entry_count: u32,
}

/// UDL 健康报告的三个字段在 core `ab_state::HealthReport` 无对应数据源，
/// 未回填（fail-safe `false` / 空），见 README 漂移表。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HealthReport {
    pub vault_readable: bool,
    pub sync_reachable: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeSpec {
    pub domain_allowlist: Vec<String>,
    pub read_only: bool,
    pub single_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientEnv {
    pub user_agent: String,
    pub platform: String,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedSession {
    pub session_id: String,
    pub account_ref: String,
    pub expires_at: String,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingTicket {
    pub ticket_id: String,
    pub fingerprint: String,
    pub remote_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransferReport {
    pub sent: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestVerification {
    pub valid: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub who: String,
    pub when: String,
    pub account_ref: Option<String>,
    pub domain: Option<String>,
    pub action: AuditAction,
    pub result: AuditResult,
    pub detail: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditFilter {
    pub who: Option<String>,
    pub account_ref: Option<String>,
    pub domain: Option<String>,
    pub action: Option<AuditAction>,
    pub result: Option<AuditResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    pub peer_id: String,
    pub device_kind: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTool {
    pub name: String,
    pub description: String,
}
