//! UDL ↔ core 公开 API 的机械映射：逐字段转发 + 错误分类，零业务规则。
//!
//! 约定：
//! - core 类型一律带命名空间别名引用（`contract::Cookie` 等），与本层同名镜像类型区分；
//! - 错误按固定分类表落 `CoreError` 变体（见 `core/ffi/README.md`「错误映射表」）；
//! - 秘密字段不进入任何文本（`Debug`/`Display` 均脱敏，见 `types.rs`）。

use ab_audit as audit;
use ab_broker as broker;
use ab_contract as contract;
use ab_converter as converter;
use ab_crypto as crypto;
use ab_mcp as mcp;
use ab_state as state;
use ab_sync as sync;
use ab_vault as vault;

use crate::types::*;

/* ============================== 枚举：UDL → core ============================== */

impl From<AccessLevel> for contract::AccessLevel {
    fn from(v: AccessLevel) -> Self {
        match v {
            AccessLevel::C => contract::AccessLevel::C,
            AccessLevel::B => contract::AccessLevel::B,
            AccessLevel::A => contract::AccessLevel::A,
        }
    }
}

impl From<CredentialKind> for contract::CredentialKind {
    fn from(v: CredentialKind) -> Self {
        match v {
            CredentialKind::PASSWORD => contract::CredentialKind::Password,
            CredentialKind::PASSKEY => contract::CredentialKind::Passkey,
            CredentialKind::TOTP => contract::CredentialKind::Totp,
        }
    }
}

impl From<SessionKind> for contract::SessionKind {
    fn from(v: SessionKind) -> Self {
        match v {
            SessionKind::COOKIE => contract::SessionKind::Cookie,
            SessionKind::TOKEN => contract::SessionKind::Token,
            SessionKind::CREDENTIAL => contract::SessionKind::Credential,
        }
    }
}

impl From<TargetKind> for contract::TargetKind {
    fn from(v: TargetKind) -> Self {
        match v {
            TargetKind::WEB => contract::TargetKind::Web,
            TargetKind::APP => contract::TargetKind::App,
            TargetKind::CONTAINER => contract::TargetKind::Container,
        }
    }
}

impl From<SameSite> for contract::SameSite {
    fn from(v: SameSite) -> Self {
        match v {
            SameSite::STRICT => contract::SameSite::Strict,
            SameSite::LAX => contract::SameSite::Lax,
            SameSite::NONE => contract::SameSite::None,
        }
    }
}

impl From<SourceType> for contract::SourceType {
    fn from(v: SourceType) -> Self {
        match v {
            SourceType::EXTENSION => contract::SourceType::Extension,
            SourceType::CONVERTER => contract::SourceType::Converter,
            SourceType::MOBILE => contract::SourceType::Mobile,
            SourceType::MANUAL => contract::SourceType::Manual,
            SourceType::SYNC => contract::SourceType::Sync,
        }
    }
}

impl From<AuditAction> for audit::AuditAction {
    fn from(v: AuditAction) -> Self {
        match v {
            AuditAction::UNLOCK => audit::AuditAction::Unlock,
            AuditAction::LOCK => audit::AuditAction::Lock,
            AuditAction::CAPTURE => audit::AuditAction::Capture,
            AuditAction::INJECT => audit::AuditAction::Inject,
            AuditAction::FILL => audit::AuditAction::Fill,
            AuditAction::ISSUE => audit::AuditAction::Issue,
            AuditAction::RENEW => audit::AuditAction::Renew,
            AuditAction::RELEASE => audit::AuditAction::Release,
            AuditAction::REVOKE => audit::AuditAction::Revoke,
            AuditAction::REVOKE_ALL => audit::AuditAction::RevokeAll,
            AuditAction::SWEEP => audit::AuditAction::Sweep,
            AuditAction::USE => audit::AuditAction::Use,
            AuditAction::EXTRACT => audit::AuditAction::Extract,
            AuditAction::CONVERTER_LOAD => audit::AuditAction::ConverterLoad,
            AuditAction::CONVERTER_REJECT => audit::AuditAction::ConverterReject,
            AuditAction::PAIR => audit::AuditAction::Pair,
            AuditAction::SYNC => audit::AuditAction::Sync,
            AuditAction::VAULT_WRITE => audit::AuditAction::VaultWrite,
            AuditAction::VAULT_DELETE => audit::AuditAction::VaultDelete,
            AuditAction::RESTORE => audit::AuditAction::Restore,
            AuditAction::EXPORT => audit::AuditAction::Export,
        }
    }
}

impl From<AuditResult> for audit::AuditResult {
    fn from(v: AuditResult) -> Self {
        match v {
            AuditResult::OK => audit::AuditResult::Ok,
            AuditResult::DENIED => audit::AuditResult::Denied,
            AuditResult::FAILED => audit::AuditResult::Failed,
        }
    }
}

/* ============================== 枚举：core → UDL ============================== */

impl From<contract::AccessLevel> for AccessLevel {
    fn from(v: contract::AccessLevel) -> Self {
        match v {
            contract::AccessLevel::C => AccessLevel::C,
            contract::AccessLevel::B => AccessLevel::B,
            contract::AccessLevel::A => AccessLevel::A,
        }
    }
}

impl From<contract::CredentialKind> for CredentialKind {
    fn from(v: contract::CredentialKind) -> Self {
        match v {
            contract::CredentialKind::Password => CredentialKind::PASSWORD,
            contract::CredentialKind::Passkey => CredentialKind::PASSKEY,
            contract::CredentialKind::Totp => CredentialKind::TOTP,
        }
    }
}

impl From<contract::SessionKind> for SessionKind {
    fn from(v: contract::SessionKind) -> Self {
        match v {
            contract::SessionKind::Cookie => SessionKind::COOKIE,
            contract::SessionKind::Token => SessionKind::TOKEN,
            contract::SessionKind::Credential => SessionKind::CREDENTIAL,
        }
    }
}

impl From<contract::TargetKind> for TargetKind {
    fn from(v: contract::TargetKind) -> Self {
        match v {
            contract::TargetKind::Web => TargetKind::WEB,
            contract::TargetKind::App => TargetKind::APP,
            contract::TargetKind::Container => TargetKind::CONTAINER,
        }
    }
}

impl From<contract::SameSite> for SameSite {
    fn from(v: contract::SameSite) -> Self {
        match v {
            contract::SameSite::Strict => SameSite::STRICT,
            contract::SameSite::Lax => SameSite::LAX,
            contract::SameSite::None => SameSite::NONE,
        }
    }
}

impl From<contract::SourceType> for SourceType {
    fn from(v: contract::SourceType) -> Self {
        match v {
            contract::SourceType::Extension => SourceType::EXTENSION,
            contract::SourceType::Converter => SourceType::CONVERTER,
            contract::SourceType::Mobile => SourceType::MOBILE,
            contract::SourceType::Manual => SourceType::MANUAL,
            contract::SourceType::Sync => SourceType::SYNC,
        }
    }
}

impl From<audit::AuditAction> for AuditAction {
    fn from(v: audit::AuditAction) -> Self {
        match v {
            audit::AuditAction::Unlock => AuditAction::UNLOCK,
            audit::AuditAction::Lock => AuditAction::LOCK,
            audit::AuditAction::Capture => AuditAction::CAPTURE,
            audit::AuditAction::Inject => AuditAction::INJECT,
            audit::AuditAction::Fill => AuditAction::FILL,
            audit::AuditAction::Issue => AuditAction::ISSUE,
            audit::AuditAction::Renew => AuditAction::RENEW,
            audit::AuditAction::Release => AuditAction::RELEASE,
            audit::AuditAction::Revoke => AuditAction::REVOKE,
            audit::AuditAction::RevokeAll => AuditAction::REVOKE_ALL,
            audit::AuditAction::Sweep => AuditAction::SWEEP,
            audit::AuditAction::Use => AuditAction::USE,
            audit::AuditAction::Extract => AuditAction::EXTRACT,
            audit::AuditAction::ConverterLoad => AuditAction::CONVERTER_LOAD,
            audit::AuditAction::ConverterReject => AuditAction::CONVERTER_REJECT,
            audit::AuditAction::Pair => AuditAction::PAIR,
            audit::AuditAction::Sync => AuditAction::SYNC,
            audit::AuditAction::VaultWrite => AuditAction::VAULT_WRITE,
            audit::AuditAction::VaultDelete => AuditAction::VAULT_DELETE,
            audit::AuditAction::Restore => AuditAction::RESTORE,
            audit::AuditAction::Export => AuditAction::EXPORT,
        }
    }
}

impl From<audit::AuditResult> for AuditResult {
    fn from(v: audit::AuditResult) -> Self {
        match v {
            audit::AuditResult::Ok => AuditResult::OK,
            audit::AuditResult::Denied => AuditResult::DENIED,
            audit::AuditResult::Failed => AuditResult::FAILED,
        }
    }
}

/* ============================== 记录：UDL ↔ core ============================== */

impl From<Cookie> for contract::Cookie {
    fn from(v: Cookie) -> Self {
        contract::Cookie {
            name: v.name,
            value: v.value,
            domain: v.domain,
            path: v.path,
            secure: v.secure,
            http_only: v.http_only,
            same_site: v.same_site.map(Into::into),
            expires_at: v.expires_at,
        }
    }
}

impl From<contract::Cookie> for Cookie {
    fn from(v: contract::Cookie) -> Self {
        Cookie {
            name: v.name,
            value: v.value,
            domain: v.domain,
            path: v.path,
            secure: v.secure,
            http_only: v.http_only,
            same_site: v.same_site.map(Into::into),
            expires_at: v.expires_at,
        }
    }
}

impl From<Target> for contract::Target {
    fn from(v: Target) -> Self {
        contract::Target {
            kind: v.kind.into(),
            domain: v.domain,
            app_id: v.app_id,
        }
    }
}

impl From<contract::Target> for Target {
    fn from(v: contract::Target) -> Self {
        Target {
            kind: v.kind.into(),
            domain: v.domain,
            app_id: v.app_id,
        }
    }
}

impl From<PlainCredential> for contract::PlainCredential {
    fn from(v: PlainCredential) -> Self {
        contract::PlainCredential {
            username: v.username,
            secret: v.secret,
        }
    }
}

impl From<contract::PlainCredential> for PlainCredential {
    fn from(v: contract::PlainCredential) -> Self {
        PlainCredential {
            username: v.username,
            secret: v.secret,
        }
    }
}

impl From<Payload> for contract::Payload {
    fn from(v: Payload) -> Self {
        contract::Payload {
            cookies: v.cookies.into_iter().map(Into::into).collect(),
            token: v.token,
            credential: v.credential.map(Into::into),
            local_storage: v.local_storage.into_iter().collect(),
        }
    }
}

impl From<contract::Payload> for Payload {
    fn from(v: contract::Payload) -> Self {
        Payload {
            cookies: v.cookies.into_iter().map(Into::into).collect(),
            token: v.token,
            credential: v.credential.map(Into::into),
            local_storage: v.local_storage.into_iter().collect(),
        }
    }
}

impl From<Source> for contract::Source {
    fn from(v: Source) -> Self {
        contract::Source {
            source_type: v.source_type.into(),
            id: v.id,
        }
    }
}

impl From<contract::Source> for Source {
    fn from(v: contract::Source) -> Self {
        Source {
            source_type: v.source_type.into(),
            id: v.id,
        }
    }
}

impl From<ScopeSpec> for contract::ScopeSpec {
    fn from(v: ScopeSpec) -> Self {
        contract::ScopeSpec {
            domain_allowlist: v.domain_allowlist,
            read_only: v.read_only,
            single_use: v.single_use,
        }
    }
}

impl From<contract::ScopeSpec> for ScopeSpec {
    fn from(v: contract::ScopeSpec) -> Self {
        ScopeSpec {
            domain_allowlist: v.domain_allowlist,
            read_only: v.read_only,
            single_use: v.single_use,
        }
    }
}

impl From<ClientEnv> for contract::ClientEnv {
    fn from(v: ClientEnv) -> Self {
        contract::ClientEnv {
            user_agent: v.user_agent,
            platform: v.platform,
            device_id: v.device_id,
        }
    }
}

impl From<contract::ClientEnv> for ClientEnv {
    fn from(v: contract::ClientEnv) -> Self {
        ClientEnv {
            user_agent: v.user_agent,
            platform: v.platform,
            device_id: v.device_id,
        }
    }
}

impl From<CredentialRecord> for contract::CredentialRecord {
    fn from(v: CredentialRecord) -> Self {
        contract::CredentialRecord {
            account_ref: v.account_ref,
            kind: v.kind.into(),
            username: v.username,
            secret: v.secret,
            totp_seed: v.totp_seed,
            url: v.url,
        }
    }
}

impl From<contract::CredentialRecord> for CredentialRecord {
    fn from(v: contract::CredentialRecord) -> Self {
        CredentialRecord {
            account_ref: v.account_ref,
            kind: v.kind.into(),
            username: v.username,
            secret: v.secret,
            totp_seed: v.totp_seed,
            url: v.url,
        }
    }
}

impl From<SessionRecord> for contract::SessionRecord {
    fn from(v: SessionRecord) -> Self {
        contract::SessionRecord {
            account_ref: v.account_ref,
            target: v.target.into(),
            kind: v.kind.into(),
            payload: v.payload.into(),
            expires_at: v.expires_at,
            source: v.source.into(),
            client_env: v.client_env.map(Into::into),
        }
    }
}

impl From<contract::SessionRecord> for SessionRecord {
    fn from(v: contract::SessionRecord) -> Self {
        SessionRecord {
            account_ref: v.account_ref,
            target: v.target.into(),
            kind: v.kind.into(),
            payload: v.payload.into(),
            expires_at: v.expires_at,
            source: v.source.into(),
            client_env: v.client_env.map(Into::into),
        }
    }
}

impl From<SessionEnvelope> for contract::SessionEnvelope {
    fn from(v: SessionEnvelope) -> Self {
        contract::SessionEnvelope {
            schema_version: v.schema_version,
            account_ref: v.account_ref,
            target: v.target.into(),
            kind: v.kind.into(),
            payload: v.payload.into(),
            issued_at: v.issued_at,
            expires_at: v.expires_at,
            source: v.source.into(),
            scope: v.scope.map(Into::into),
            client_env: v.client_env.map(Into::into),
        }
    }
}

impl From<contract::SessionEnvelope> for SessionEnvelope {
    fn from(v: contract::SessionEnvelope) -> Self {
        SessionEnvelope {
            schema_version: v.schema_version,
            account_ref: v.account_ref,
            target: v.target.into(),
            kind: v.kind.into(),
            payload: v.payload.into(),
            issued_at: v.issued_at,
            expires_at: v.expires_at,
            source: v.source.into(),
            scope: v.scope.map(Into::into),
            client_env: v.client_env.map(Into::into),
        }
    }
}

impl From<StandardAccess> for contract::StandardAccess {
    fn from(v: StandardAccess) -> Self {
        contract::StandardAccess {
            schema_version: v.schema_version,
            app_id: v.app_id,
            level: v.level.into(),
            generated_at: v.generated_at,
            credentials: v.credentials.into_iter().map(Into::into).collect(),
            sessions: v.sessions.into_iter().map(Into::into).collect(),
            meta: v.meta.into_iter().map(|(k, val)| (k, val.into())).collect(),
            client_env: v.client_env.map(Into::into),
        }
    }
}

impl From<contract::StandardAccess> for StandardAccess {
    fn from(v: contract::StandardAccess) -> Self {
        StandardAccess {
            schema_version: v.schema_version,
            app_id: v.app_id,
            level: v.level.into(),
            generated_at: v.generated_at,
            credentials: v.credentials.into_iter().map(Into::into).collect(),
            sessions: v.sessions.into_iter().map(Into::into).collect(),
            // core `meta` 值是任意 JSON；UDL 是 record<string,string>，标量保持原文，
            // 其余降为 JSON 文本（字段级漂移，见 README）。
            meta: v
                .meta
                .into_iter()
                .map(|(k, val)| (k, json_value_to_string(val)))
                .collect(),
            client_env: v.client_env.map(Into::into),
        }
    }
}

fn json_value_to_string(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

impl From<Signature> for contract::Signature {
    fn from(v: Signature) -> Self {
        contract::Signature {
            algorithm: v.algorithm,
            signer: v.signer,
            value: v.value,
        }
    }
}

impl From<contract::Signature> for Signature {
    fn from(v: contract::Signature) -> Self {
        Signature {
            algorithm: v.algorithm,
            signer: v.signer,
            value: v.value,
        }
    }
}

impl From<ConverterManifest> for contract::ConverterManifest {
    fn from(v: ConverterManifest) -> Self {
        contract::ConverterManifest {
            id: v.id,
            name: v.name,
            version: v.version,
            level: v.level.into(),
            readable_paths: v.readable_paths,
            entry: v.entry,
            module_sha256: v.module_sha256,
            signature: v.signature.into(),
            min_app_version: v.min_app_version,
        }
    }
}

impl From<contract::ConverterManifest> for ConverterManifest {
    fn from(v: contract::ConverterManifest) -> Self {
        ConverterManifest {
            id: v.id,
            name: v.name,
            version: v.version,
            level: v.level.into(),
            readable_paths: v.readable_paths,
            entry: v.entry,
            module_sha256: v.module_sha256,
            signature: v.signature.into(),
            min_app_version: v.min_app_version,
        }
    }
}

impl From<GeneratorSpec> for vault::GeneratorSpec {
    fn from(v: GeneratorSpec) -> Self {
        vault::GeneratorSpec {
            length: v.length as usize,
            lowercase: v.lowercase,
            uppercase: v.uppercase,
            digits: v.digits,
            symbols: v.symbols,
        }
    }
}

impl From<vault::GeneratedSecret> for GeneratedSecret {
    fn from(v: vault::GeneratedSecret) -> Self {
        GeneratedSecret {
            value: v.value.to_string(),
            charset: v.charset,
        }
    }
}

/// A5.2 口令词模式：词表/词数/分隔符逐字段搬运，无默认值（core 亦无内置词表）。
impl From<PassphraseSpec> for vault::PassphraseSpec {
    fn from(v: PassphraseSpec) -> Self {
        vault::PassphraseSpec {
            words: v.words,
            word_count: v.word_count as usize,
            separator: v.separator,
        }
    }
}

impl From<Entry> for vault::Entry {
    fn from(v: Entry) -> Self {
        // uuid 由 core 持有（UDL 无该字段）：调用方（Vault 服务）负责按 account_ref
        // 找回既有 uuid 或经 `Entry::new` 生成，此处只做字段级映射。
        vault::Entry {
            uuid: [0u8; 16],
            title: v.title,
            account_ref: v.account_ref,
            kind: v.kind.into(),
            username: v.username,
            secret: v.secret,
            totp_seed: v.totp_seed,
            url: v.url,
            note: v.note,
            // A1.1/A1.2（core 新增）：UDL 侧 Entry 记录当前无 tags/favorite，
            // FFI 入向默认空标签、未收藏；出向映射见下。
            tags: Vec::new(),
            favorite: false,
        }
    }
}

impl From<&vault::Entry> for Entry {
    fn from(v: &vault::Entry) -> Self {
        Entry {
            account_ref: v.account_ref.clone(),
            title: v.title.clone(),
            kind: v.kind.into(),
            username: v.username.clone(),
            secret: v.secret.clone(),
            totp_seed: v.totp_seed.clone(),
            url: v.url.clone(),
            note: v.note.clone(),
        }
    }
}

impl From<&vault::Entry> for EntrySummary {
    fn from(v: &vault::Entry) -> Self {
        EntrySummary {
            account_ref: v.account_ref.clone(),
            title: v.title.clone(),
            kind: v.kind.into(),
            username: v.username.clone(),
            url: v.url.clone(),
            // core `ab_vault::Entry` 无时间戳字段（UDL 漂移，见 README）。
            updated_at: None,
        }
    }
}

impl From<&state::MatrixRow> for MatrixRow {
    fn from(v: &state::MatrixRow) -> Self {
        MatrixRow {
            account_ref: v.account_ref.clone(),
            target_kind: v.target.kind.into(),
            domain: v.target.domain.clone(),
            app_id: v.target.app_id.clone(),
            source_type: v.source.source_type.into(),
            expires_at: v.expires_at.clone(),
            // core 矩阵行无「最后所见」维度（UDL 漂移，见 README）。
            last_seen_at: None,
        }
    }
}

impl From<&state::SnapshotMeta> for SnapshotSummary {
    fn from(v: &state::SnapshotMeta) -> Self {
        SnapshotSummary {
            snapshot_id: v.id.clone(),
            created_at: contract::time::format_rfc3339_secs(v.created_at),
            entry_count: v.entry_count as u32,
        }
    }
}

impl From<&broker::GrantStatus> for IssuedSession {
    fn from(v: &broker::GrantStatus) -> Self {
        IssuedSession {
            session_id: v.session_id.clone(),
            account_ref: v.account_ref.clone(),
            expires_at: v.expires_at.clone(),
            revoked: v.state == broker::GrantState::Revoked,
        }
    }
}

impl From<AuditEvent> for audit::AuditEvent {
    fn from(v: AuditEvent) -> Self {
        audit::AuditEvent {
            who: v.who,
            when: v.when,
            account_ref: v.account_ref,
            domain: v.domain,
            action: v.action.into(),
            result: v.result.into(),
            detail: v.detail.into_iter().collect(),
        }
    }
}

impl From<audit::AuditEvent> for AuditEvent {
    fn from(v: audit::AuditEvent) -> Self {
        AuditEvent {
            who: v.who,
            when: v.when,
            account_ref: v.account_ref,
            domain: v.domain,
            action: v.action.into(),
            result: v.result.into(),
            detail: v.detail.into_iter().collect(),
        }
    }
}

impl From<AuditFilter> for audit::AuditFilter {
    fn from(v: AuditFilter) -> Self {
        audit::AuditFilter {
            who: v.who,
            account_ref: v.account_ref,
            domain: v.domain,
            action: v.action.map(Into::into),
            result: v.result.map(Into::into),
        }
    }
}

/* ============================== 错误映射（core → CoreError） ============================== */

impl From<contract::ContractError> for ContractError {
    fn from(v: contract::ContractError) -> Self {
        match v {
            contract::ContractError::UnsupportedSchemaVersion(version) => {
                ContractError::UnsupportedSchemaVersion { version }
            }
            contract::ContractError::InvalidId { field, value } => ContractError::InvalidId {
                field: field.to_string(),
                value,
            },
            contract::ContractError::EmptyField(name) => ContractError::EmptyField {
                name: name.to_string(),
            },
            contract::ContractError::TargetFieldMissing(name) => ContractError::TargetFieldMissing {
                name: name.to_string(),
            },
            contract::ContractError::PayloadMismatch { kind, expected } => {
                ContractError::PayloadMismatch {
                    kind: kind.into(),
                    expected: expected.to_string(),
                }
            }
            contract::ContractError::UnsupportedAlgorithm(algorithm) => {
                ContractError::UnsupportedAlgorithm { algorithm }
            }
            contract::ContractError::MissingExpiry => ContractError::MissingExpiry,
            contract::ContractError::MissingScope => ContractError::MissingScope,
            contract::ContractError::PlaceholderSignature => ContractError::PlaceholderSignature,
            contract::ContractError::InvalidModuleHash => ContractError::InvalidModuleHash,
        }
    }
}

impl From<contract::ContractError> for CoreError {
    fn from(_: contract::ContractError) -> Self {
        CoreError::CONTRACT
    }
}

impl From<crypto::CryptoError> for CoreError {
    fn from(_: crypto::CryptoError) -> Self {
        CoreError::CRYPTO
    }
}

impl From<audit::AuditError> for CoreError {
    fn from(v: audit::AuditError) -> Self {
        match v {
            audit::AuditError::Unavailable => CoreError::STORAGE,
            audit::AuditError::EmptyField(_) => CoreError::CONTRACT,
        }
    }
}

impl From<vault::VaultError> for CoreError {
    fn from(v: vault::VaultError) -> Self {
        match v {
            vault::VaultError::Store => CoreError::STORAGE,
            vault::VaultError::Kdbx(kdbx) => match kdbx {
                vault::KdbxError::BadCredentials => CoreError::CRYPTO,
                vault::KdbxError::Malformed => CoreError::STORAGE,
                vault::KdbxError::UnsupportedKdf => CoreError::CRYPTO,
            },
            vault::VaultError::WeakKdf => CoreError::CRYPTO,
            vault::VaultError::Malformed => CoreError::STORAGE,
            vault::VaultError::NotFound => CoreError::NOT_FOUND,
            vault::VaultError::NoSuchEntry => CoreError::NOT_FOUND,
            vault::VaultError::DuplicateEntry => CoreError::CONTRACT,
        }
    }
}

impl From<state::StateError> for CoreError {
    fn from(v: state::StateError) -> Self {
        match v {
            state::StateError::Store(_) => CoreError::STORAGE,
            state::StateError::Source(_) => CoreError::STORAGE,
            state::StateError::Contract(_) => CoreError::CONTRACT,
            state::StateError::SnapshotNotFound(_) => CoreError::NOT_FOUND,
            state::StateError::InvalidEntry { .. } => CoreError::CONTRACT,
        }
    }
}

impl From<broker::BrokerError> for CoreError {
    fn from(v: broker::BrokerError) -> Self {
        match v {
            broker::BrokerError::Contract(_) => CoreError::CONTRACT,
            broker::BrokerError::Audit(e) => e.into(),
            broker::BrokerError::Crypto(_) => CoreError::CRYPTO,
            broker::BrokerError::Unavailable => CoreError::INTERNAL,
            broker::BrokerError::EntropyFailed => CoreError::CRYPTO,
            broker::BrokerError::InvalidTtl => CoreError::CONTRACT,
            broker::BrokerError::InvalidScope => CoreError::CONTRACT,
            broker::BrokerError::TargetNotInScope => CoreError::DENIED,
            broker::BrokerError::BadHandle => CoreError::CONTRACT,
            broker::BrokerError::UnknownSession => CoreError::NOT_FOUND,
            broker::BrokerError::NotActive => CoreError::DENIED,
            broker::BrokerError::Expired => CoreError::DENIED,
            broker::BrokerError::SingleUseConsumed => CoreError::DENIED,
            broker::BrokerError::ReadOnlyViolation => CoreError::DENIED,
            broker::BrokerError::DomainNotAllowed => CoreError::DENIED,
        }
    }
}

impl From<sync::SyncError> for CoreError {
    fn from(v: sync::SyncError) -> Self {
        match v {
            sync::SyncError::Crypto(_) => CoreError::CRYPTO,
            sync::SyncError::EntropyFailed => CoreError::CRYPTO,
            sync::SyncError::InvalidOffer => CoreError::CONTRACT,
            sync::SyncError::Malformed => CoreError::CONVERSION,
            sync::SyncError::UnknownDevice => CoreError::NOT_FOUND,
            sync::SyncError::AlreadyPaired => CoreError::CONTRACT,
            sync::SyncError::Expired => CoreError::DENIED,
            sync::SyncError::InvalidValidity => CoreError::CONTRACT,
            sync::SyncError::Transport => CoreError::TRANSPORT,
        }
    }
}

impl From<converter::ConverterError> for CoreError {
    fn from(v: converter::ConverterError) -> Self {
        match v {
            converter::ConverterError::InvalidManifest(_) => CoreError::CONVERSION,
            converter::ConverterError::Contract(_) => CoreError::CONTRACT,
            converter::ConverterError::SignatureRejected(_) => CoreError::CRYPTO,
            converter::ConverterError::PathOutsideScope(_) => CoreError::DENIED,
            converter::ConverterError::SandboxFailed(_) => CoreError::INTERNAL,
        }
    }
}

impl From<mcp::McpError> for CoreError {
    fn from(v: mcp::McpError) -> Self {
        match v {
            mcp::McpError::Broker(e) => e.into(),
            mcp::McpError::State(e) => e.into(),
            mcp::McpError::NoSession => CoreError::NOT_FOUND,
            mcp::McpError::AccountRequired(_) => CoreError::CONTRACT,
            mcp::McpError::InvalidParams(_) => CoreError::CONTRACT,
            mcp::McpError::Io(_) => CoreError::TRANSPORT,
        }
    }
}

/* ============================== 小工具 ============================== */

