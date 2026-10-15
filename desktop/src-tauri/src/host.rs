//! 桌面宿主中枢状态：串联「桌面应用 ↔ 浏览器 ↔ 移动 App ↔ AI Agents」登录态流转、
//! Scheme A 零明文代理执行、影子令牌网关、写操作挂起审批、越权自动熔断、扩展借壳隧道与官方转换器验签。
//!
//! - 数据目录优先读取环境变量 `AIBRIDGE_DATA_DIR`，生产路径禁止回退 `%TEMP%`；
//! - 转换器验签默认内置 `ab_contract::OFFICIAL_SIGNER_ID` 与 `ab_contract::OFFICIAL_CONVERTER_PUBKEY` 信任根，
//!   待签名字节直接调用 `ab_contract::canonical_signing_bytes(manifest)`（全仓唯一实现）；
//! - 汇聚 `InMemoryStateStore`（多端状态库）、`Broker`（句柄代持、影子令牌、写操作 U 盾挂起审批、越权自动熔断与设定时间收回）
//!   及 `InMemoryAuditSink`（追加式审计）；
//! - 全部状态初始化与互斥锁访问走 `Result` fail-closed 传播，非测试路径零 `expect`/`unwrap`/`panic!`。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use ab_audit::{AuditAction, AuditEvent, AuditFilter, AuditResult, AuditSink, InMemoryAuditSink};
use ab_broker::{
    AgentHttpExecutor, AgentHttpRequest, AgentHttpResponse, Broker, BrokerConfig, IssuedSession,
    PendingWriteApproval, RevocationNotice,
};
use ab_contract::time::format_rfc3339_secs;
use ab_contract::{
    canonical_signing_bytes, ConverterManifest, ScopeSpec, SessionEnvelope, SessionRecord, Source,
    SourceType, StandardAccess, OFFICIAL_CONVERTER_PUBKEY, OFFICIAL_SIGNER_ID, SCHEMA_VERSION,
};
use ab_converter::{SignatureVerifier, VerifyError};
use ab_crypto::{ed25519_verify, sha256};
use ab_state::{
    Clock, Endpoint, EndpointKind, InMemoryStateStore, RollbackReport, SessionEntry, Snapshot,
    SnapshotMeta, StateStore, SystemClock,
};
use serde_json::json;

use crate::bridge::ext_protocol::{ExtEvent, ExtRequest, ExtResponse, BRIDGE_PROTOCOL_VERSION};
use crate::native::dpapi::{resolve_data_dir, DpapiVaultStore};

pub const ENV_OFFICIAL_SIGNER: &str = "AIBRIDGE_OFFICIAL_SIGNER";
pub const ENV_OFFICIAL_PUBKEY_HEX: &str = "AIBRIDGE_OFFICIAL_PUBKEY_HEX";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    DataDirUnavailable(String),
    LockPoisoned(&'static str),
    Locked,
    CryptoInitFailed(String),
    InvalidConfig(String),
    FlowFailed(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::DataDirUnavailable(msg) => write!(f, "数据目录不可用：{msg}"),
            HostError::LockPoisoned(name) => write!(f, "宿主状态锁不可用（{name}），拒绝执行"),
            HostError::Locked => write!(f, "宿主处于锁定状态，拒绝访问敏感会话"),
            HostError::CryptoInitFailed(msg) => write!(f, "安全组件初始化失败：{msg}"),
            HostError::InvalidConfig(msg) => write!(f, "宿主配置非法：{msg}"),
            HostError::FlowFailed(msg) => write!(f, "会话流转或收回失败：{msg}"),
        }
    }
}

impl std::error::Error for HostError {}

fn decode_hex_fixed<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    let bytes = hex.as_bytes();
    for i in 0..N {
        let hi = (bytes[i * 2] as char).to_digit(16)?;
        let lo = (bytes[i * 2 + 1] as char).to_digit(16)?;
        out[i] = ((hi << 4) | lo) as u8;
    }
    Some(out)
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// 宿主侧转换器验签器：默认使用 `ab_contract` 内置官方公钥（`OFFICIAL_CONVERTER_PUBKEY`），任何不匹配均 fail-closed 拒载。
#[derive(Debug, Clone)]
pub struct HostSignatureVerifier {
    expected_signer: Option<String>,
    public_key: Option<[u8; 32]>,
}

impl Default for HostSignatureVerifier {
    fn default() -> Self {
        Self::official()
    }
}

impl HostSignatureVerifier {
    pub fn official() -> Self {
        Self {
            expected_signer: Some(OFFICIAL_SIGNER_ID.to_string()),
            public_key: Some(OFFICIAL_CONVERTER_PUBKEY),
        }
    }

    pub fn from_env() -> Self {
        let expected_signer = std::env::var(ENV_OFFICIAL_SIGNER)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| Some(OFFICIAL_SIGNER_ID.to_string()));
        let public_key = std::env::var(ENV_OFFICIAL_PUBKEY_HEX)
            .ok()
            .and_then(|hex| decode_hex_fixed::<32>(hex.trim()))
            .or(Some(OFFICIAL_CONVERTER_PUBKEY));
        Self {
            expected_signer,
            public_key,
        }
    }

    pub fn with_trust_root(signer: impl Into<String>, public_key: [u8; 32]) -> Self {
        Self {
            expected_signer: Some(signer.into()),
            public_key: Some(public_key),
        }
    }
}

impl SignatureVerifier for HostSignatureVerifier {
    fn verify(&self, manifest: &ConverterManifest, module: &[u8]) -> Result<(), VerifyError> {
        let signer = self
            .expected_signer
            .as_deref()
            .ok_or(VerifyError::SignerNotAllowed)?;
        let pubkey = self.public_key.as_ref().ok_or(VerifyError::SignerNotAllowed)?;

        if manifest.signature.signer != signer {
            return Err(VerifyError::SignerNotAllowed);
        }

        let actual_hash = encode_lower_hex(&sha256(module));
        if !ab_crypto::ct_eq(actual_hash.as_bytes(), manifest.module_sha256.as_bytes()) {
            return Err(VerifyError::ModuleHashMismatch);
        }

        let sig_bytes =
            decode_hex_fixed::<64>(&manifest.signature.value).ok_or(VerifyError::BadSignature)?;
        let message = canonical_signing_bytes(manifest);
        ed25519_verify(pubkey, &message, &sig_bytes).map_err(|_| VerifyError::BadSignature)
    }
}

#[derive(Clone)]
struct SharedAuditSink(Arc<InMemoryAuditSink>);

impl AuditSink for SharedAuditSink {
    fn record_or_fail(&self, event: AuditEvent) -> Result<(), ab_audit::AuditError> {
        self.0.record_or_fail(event)
    }
    fn query(
        &self,
        filter: &ab_audit::AuditFilter,
    ) -> Result<Vec<AuditEvent>, ab_audit::AuditError> {
        self.0.query(filter)
    }
    fn count(&self) -> Result<usize, ab_audit::AuditError> {
        self.0.count()
    }
}

#[derive(Clone)]
struct SharedClock(Arc<dyn Clock>);

impl Clock for SharedClock {
    fn now_unix_secs(&self) -> i64 {
        self.0.now_unix_secs()
    }
}

pub struct HostState {
    data_dir: PathBuf,
    vault_store: DpapiVaultStore,
    unlocked: Mutex<bool>,
    state_store: InMemoryStateStore,
    audit_sink: Arc<InMemoryAuditSink>,
    clock: Arc<dyn Clock>,
    broker: Broker<InMemoryStateStore, SharedClock, SharedAuditSink>,
}

impl HostState {
    pub fn try_new() -> Result<Self, HostError> {
        let data_dir =
            resolve_data_dir().map_err(|e| HostError::DataDirUnavailable(e.to_string()))?;
        Self::with_data_dir(data_dir)
    }

    pub fn with_data_dir(data_dir: impl Into<PathBuf>) -> Result<Self, HostError> {
        Self::with_data_dir_and_clock(data_dir, Arc::new(SystemClock))
    }

    pub fn with_data_dir_and_clock(
        data_dir: impl Into<PathBuf>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, HostError> {
        let dir = data_dir.into();
        if dir.as_os_str().is_empty() {
            return Err(HostError::DataDirUnavailable("数据目录路径为空".into()));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| HostError::DataDirUnavailable(format!("无法创建数据目录：{e}")))?;
        let vault_store = DpapiVaultStore::new(&dir);
        let state_store = InMemoryStateStore::new();
        let audit_sink = Arc::new(InMemoryAuditSink::new());
        let broker = Broker::new(
            state_store.clone(),
            SharedClock(clock.clone()),
            SharedAuditSink(audit_sink.clone()),
            BrokerConfig::default(),
        );
        Ok(Self {
            data_dir: dir,
            vault_store,
            unlocked: Mutex::new(false),
            state_store,
            audit_sink,
            clock,
            broker,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn converters_dir(&self) -> PathBuf {
        self.data_dir.join("converters")
    }

    pub fn vault_store(&self) -> &DpapiVaultStore {
        &self.vault_store
    }

    fn lock_unlocked(&self) -> Result<MutexGuard<'_, bool>, HostError> {
        self.unlocked
            .lock()
            .map_err(|_| HostError::LockPoisoned("unlocked"))
    }

    pub fn is_unlocked(&self) -> Result<bool, HostError> {
        Ok(*self.lock_unlocked()?)
    }

    /// 切换锁定状态；锁定时自动吊销全部活跃临时会话（E4 锁屏即焚）。
    pub fn set_unlocked(&self, value: bool) -> Result<(), HostError> {
        *self.lock_unlocked()? = value;
        if !value {
            let _ = self
                .broker
                .revoke_all()
                .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        }
        Ok(())
    }

    fn require_unlocked(&self) -> Result<(), HostError> {
        if !self.is_unlocked()? {
            return Err(HostError::Locked);
        }
        Ok(())
    }

    /// 【受控 `os_crypt` 本机解密原语】仅在宿主已解锁时允许转换器解封同机 Electron/CEF `Local State` DPAPI 密钥。
    pub fn unwrap_os_crypt_key_when_unlocked(
        &self,
        prefixed_blob: &[u8],
    ) -> Result<[u8; 32], HostError> {
        self.require_unlocked()?;
        self.vault_store
            .unwrap_os_crypt_encrypted_key(prefixed_blob)
            .map_err(|e| HostError::CryptoInitFailed(e.to_string()))
    }

    /// 【任意端 → 中枢】接收来自浏览器扩展（`EndpointKind::Browser`）或移动端 App（`EndpointKind::Mobile`）的 `SessionEnvelope` 并合入状态库。
    pub fn capture_endpoint_envelope(
        &self,
        endpoint_kind: EndpointKind,
        envelope: SessionEnvelope,
    ) -> Result<(), HostError> {
        self.require_unlocked()?;
        envelope
            .validate()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;

        let now_str = format_rfc3339_secs(self.clock.now_unix_secs());
        let who = match endpoint_kind {
            EndpointKind::Browser => "browser-extension",
            EndpointKind::Desktop => "desktop-app",
            EndpointKind::Mobile => "mobile-app",
        };
        let event = AuditEvent::from_envelope(
            who,
            now_str,
            AuditAction::Capture,
            AuditResult::Ok,
            &envelope,
        );
        self.audit_sink
            .record_or_fail(event)
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;

        let entry = SessionEntry::new(
            Endpoint::new(endpoint_kind, envelope.source.id.clone()),
            SessionRecord {
                account_ref: envelope.account_ref,
                target: envelope.target,
                kind: envelope.kind,
                payload: envelope.payload,
                expires_at: envelope.expires_at,
                source: envelope.source,
                client_env: envelope.client_env,
            },
        );
        entry
            .validate()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;

        let mut entries = self
            .state_store
            .load_sessions()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        entries.retain(|existing| {
            !(existing.record.account_ref == entry.record.account_ref
                && existing.endpoint == entry.endpoint)
        });
        entries.push(entry);
        self.state_store
            .replace_sessions(entries)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn capture_browser_envelope(&self, envelope: SessionEnvelope) -> Result<(), HostError> {
        self.capture_endpoint_envelope(EndpointKind::Browser, envelope)
    }

    pub fn capture_mobile_envelope(&self, envelope: SessionEnvelope) -> Result<(), HostError> {
        self.capture_endpoint_envelope(EndpointKind::Mobile, envelope)
    }

    /// 【桌面应用 → 中枢】接收桌面应用转换器（如 QQ 转换器）产出的 `StandardAccess`，合入状态库供浏览器、移动 App 或 Agent 流转。
    pub fn ingest_standard_access(&self, access: StandardAccess) -> Result<usize, HostError> {
        self.require_unlocked()?;
        access
            .validate()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;

        let now_str = format_rfc3339_secs(self.clock.now_unix_secs());
        let mut entries = self
            .state_store
            .load_sessions()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        let mut count = 0usize;

        for record in access.sessions {
            let mut event = AuditEvent::new(
                format!("converter:{}", access.app_id),
                now_str.clone(),
                AuditAction::Extract,
                AuditResult::Ok,
            );
            event.account_ref = Some(record.account_ref.clone());
            event.domain = record.target.domain.clone();
            self.audit_sink
                .record_or_fail(event)
                .map_err(|e| HostError::FlowFailed(e.to_string()))?;

            let endpoint = Endpoint::new(EndpointKind::Desktop, access.app_id.clone());
            let entry = SessionEntry::new(endpoint.clone(), record);
            entry
                .validate()
                .map_err(|e| HostError::FlowFailed(e.to_string()))?;
            entries.retain(|existing| {
                !(existing.record.account_ref == entry.record.account_ref
                    && existing.endpoint == endpoint)
            });
            entries.push(entry);
            count += 1;
        }

        self.state_store
            .replace_sessions(entries)
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        Ok(count)
    }

    /// 【中枢 → 浏览器 / 移动 App / AI Agents】签发带设定时间（`ttl_seconds`）与授权范围的临时会话句柄（含 `shadow_token`）。
    pub fn issue_temporary_session(
        &self,
        account_ref: &str,
        scope: ScopeSpec,
        ttl_seconds: u32,
    ) -> Result<IssuedSession, HostError> {
        self.require_unlocked()?;
        self.broker
            .issue(account_ref, scope, ttl_seconds)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 签发开启「写操作 U 盾挂起审批（`ask_on_write`）」的临时会话句柄。
    pub fn issue_temporary_session_with_write_gate(
        &self,
        account_ref: &str,
        scope: ScopeSpec,
        ttl_seconds: u32,
        ask_on_write: bool,
    ) -> Result<IssuedSession, HostError> {
        self.require_unlocked()?;
        self.broker
            .issue_with_write_gate(account_ref, scope, ttl_seconds, ask_on_write)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 【Scheme A：AI Agent 零明文受控代理请求】
    pub fn execute_agent_request(
        &self,
        session_id: &str,
        request: AgentHttpRequest,
        executor: &dyn AgentHttpExecutor,
    ) -> Result<AgentHttpResponse, HostError> {
        self.require_unlocked()?;
        self.broker
            .execute_agent_request(session_id, request, executor)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 【影子凭证网关】供不支持 MCP 的传统 CLI / Agent 工具凭 `ab_tmp_...` 执行受控代理请求。
    pub fn execute_shadow_token_request(
        &self,
        shadow_token: &str,
        request: AgentHttpRequest,
        executor: &dyn AgentHttpExecutor,
    ) -> Result<AgentHttpResponse, HostError> {
        self.require_unlocked()?;
        self.broker
            .execute_shadow_token_request(shadow_token, request, executor)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 【写操作 U 盾放行】放行单次（`None`）或指定秒数窗口（`Some(300)`）。
    pub fn approve_write_request(
        &self,
        approval_id: &str,
        grant_window_secs: Option<u32>,
    ) -> Result<PendingWriteApproval, HostError> {
        self.require_unlocked()?;
        self.broker
            .approve_write(approval_id, grant_window_secs)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 【写操作 U 盾拒绝并当场吊销会话】
    pub fn deny_write_and_revoke(&self, approval_id: &str) -> Result<RevocationNotice, HostError> {
        self.require_unlocked()?;
        self.broker
            .deny_write_and_revoke(approval_id)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn list_pending_write_approvals(&self) -> Result<Vec<PendingWriteApproval>, HostError> {
        self.broker
            .list_pending_approvals()
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 【扩展借壳执行隧道 `session.proxy_fetch` 请求构造】
    pub fn build_browser_tunnel_fetch_request(
        &self,
        request_id: impl Into<String>,
        session_id: &str,
        requested_domain: &str,
        method: &str,
        url: &str,
    ) -> Result<ExtRequest, HostError> {
        self.require_unlocked()?;
        let method_upper = method.trim().to_ascii_uppercase();
        let is_write = !matches!(method_upper.as_str(), "GET" | "HEAD" | "OPTIONS");
        let env = self
            .broker
            .lease_envelope_for_surface(session_id, requested_domain, is_write)
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;

        Ok(ExtRequest {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "request".into(),
            id: request_id.into(),
            action: "session.proxy_fetch".into(),
            payload: json!({
                "session_id": session_id,
                "account_ref": env.account_ref,
                "url": url,
                "method": method_upper,
            }),
        })
    }

    /// 【跨端受控注入租约】供宿主内浏览器扩展桥（`session.inject`）或移动 App 容器在设定时间内获取已夹紧到期时间的信封。
    pub fn lease_surface_envelope(
        &self,
        session_id: &str,
        requested_domain: &str,
        is_write: bool,
    ) -> Result<SessionEnvelope, HostError> {
        self.require_unlocked()?;
        self.broker
            .lease_envelope_for_surface(session_id, requested_domain, is_write)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn list_temporary_sessions(&self) -> Result<Vec<IssuedSession>, HostError> {
        self.broker
            .list_sessions()
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn renew_temporary_session(
        &self,
        session_id: &str,
        ttl_seconds: u32,
    ) -> Result<IssuedSession, HostError> {
        self.require_unlocked()?;
        self.broker
            .renew(session_id, ttl_seconds)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn release_temporary_session(&self, session_id: &str) -> Result<(), HostError> {
        self.broker
            .release(session_id)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 主动收回单个临时会话。
    pub fn revoke_temporary_session(&self, session_id: &str) -> Result<(), HostError> {
        self.broker
            .revoke(session_id)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 一键收回全部临时会话。
    pub fn revoke_all_temporary_sessions(&self) -> Result<Vec<IssuedSession>, HostError> {
        self.broker
            .revoke_all()
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 扫描并自动收回所有已达设定时间的临时会话。
    pub fn sweep_expired_sessions(&self) -> Result<Vec<IssuedSession>, HostError> {
        self.broker
            .sweep()
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn list_session_entries(&self) -> Result<Vec<SessionEntry>, HostError> {
        self.require_unlocked()?;
        self.state_store
            .load_sessions()
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>, HostError> {
        self.require_unlocked()?;
        self.state_store
            .list_snapshots()
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn create_snapshot(&self, snapshot_id: impl Into<String>) -> Result<SnapshotMeta, HostError> {
        self.require_unlocked()?;
        let entries = self
            .state_store
            .load_sessions()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        let snap = Snapshot {
            id: snapshot_id.into(),
            created_at: self.clock.now_unix_secs(),
            entries,
        };
        let meta = snap.meta();
        self.state_store
            .save_snapshot(snap)
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        Ok(meta)
    }

    pub fn rollback_snapshot(&self, snapshot_id: &str) -> Result<RollbackReport, HostError> {
        self.require_unlocked()?;
        let snap = self
            .state_store
            .load_snapshot(snapshot_id)
            .map_err(|e| HostError::FlowFailed(e.to_string()))?
            .ok_or_else(|| HostError::FlowFailed(format!("快照 {snapshot_id} 不存在")))?;
        let current = self
            .state_store
            .load_sessions()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        let replaced_entries = current.len();
        let restored_entries = snap.entries.len();
        self.state_store
            .replace_sessions(snap.entries)
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        Ok(RollbackReport {
            snapshot_id: snapshot_id.to_string(),
            restored_entries,
            replaced_entries,
        })
    }

    pub fn query_audit_events(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, HostError> {
        self.audit_sink
            .query(filter)
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    pub fn count_audit_events(&self) -> Result<usize, HostError> {
        self.audit_sink
            .count()
            .map_err(|e| HostError::FlowFailed(e.to_string()))
    }

    /// 取出待向浏览器扩展或移动端容器广播的 `broker.revoked` 协议事件帧。
    pub fn drain_bridge_revocation_events(&self) -> Result<Vec<ExtEvent>, HostError> {
        let notices: Vec<RevocationNotice> = self
            .broker
            .drain_revocations()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        Ok(notices
            .into_iter()
            .map(|n| {
                ExtEvent::broker_revoked_detailed(
                    n.session_id,
                    n.account_ref,
                    n.target_domain,
                    n.cookie_names,
                    n.clear_storage,
                    n.revoked_at,
                )
            })
            .collect())
    }

    /// 处理来自浏览器扩展的 Native Messaging 请求，并附带任何已触发设定时间收回或熔断的 `broker.revoked` 事件。
    pub fn handle_extension_request(&self, request: ExtRequest) -> (ExtResponse, Vec<ExtEvent>) {
        let _ = self.broker.sweep();
        let response = match request.action.as_str() {
            "handshake" => ExtResponse {
                v: BRIDGE_PROTOCOL_VERSION,
                msg_type: "response".into(),
                id: request.id,
                ok: true,
                method: Some("handshake".into()),
                error: None,
                payload: json!({
                    "protocol_version": BRIDGE_PROTOCOL_VERSION,
                    "host_version": env!("CARGO_PKG_VERSION"),
                }),
            },
            "ping" => ExtResponse {
                v: BRIDGE_PROTOCOL_VERSION,
                msg_type: "response".into(),
                id: request.id,
                ok: true,
                method: Some("ping".into()),
                error: None,
                payload: json!({ "ok": true }),
            },
            "session.capture" => {
                let env_val = request
                    .payload
                    .get("envelope")
                    .cloned()
                    .unwrap_or(request.payload);
                match serde_json::from_value::<SessionEnvelope>(env_val) {
                    Ok(env) => match self.capture_browser_envelope(env) {
                        Ok(()) => ExtResponse {
                            v: BRIDGE_PROTOCOL_VERSION,
                            msg_type: "response".into(),
                            id: request.id,
                            ok: true,
                            method: Some("session.capture".into()),
                            error: None,
                            payload: json!({ "stored": true }),
                        },
                        Err(err) => ExtResponse {
                            v: BRIDGE_PROTOCOL_VERSION,
                            msg_type: "response".into(),
                            id: request.id,
                            ok: false,
                            method: None,
                            error: Some(err.to_string()),
                            payload: serde_json::Value::Null,
                        },
                    },
                    Err(err) => ExtResponse {
                        v: BRIDGE_PROTOCOL_VERSION,
                        msg_type: "response".into(),
                        id: request.id,
                        ok: false,
                        method: None,
                        error: Some(format!("SessionEnvelope 格式非法：{err}")),
                        payload: serde_json::Value::Null,
                    },
                }
            }
            "session.inject" => {
                let env_val = request
                    .payload
                    .get("envelope")
                    .cloned()
                    .unwrap_or(request.payload);
                match serde_json::from_value::<SessionEnvelope>(env_val) {
                    Ok(env) => match (self.require_unlocked(), env.validate()) {
                        (Ok(()), Ok(())) => {
                            let injected = env.payload.cookies.len();
                            ExtResponse {
                                v: BRIDGE_PROTOCOL_VERSION,
                                msg_type: "response".into(),
                                id: request.id,
                                ok: true,
                                method: Some("session.inject".into()),
                                error: None,
                                payload: json!({ "injected": injected, "skipped": 0 }),
                            }
                        }
                        (Err(err), _) => ExtResponse {
                            v: BRIDGE_PROTOCOL_VERSION,
                            msg_type: "response".into(),
                            id: request.id,
                            ok: false,
                            method: None,
                            error: Some(err.to_string()),
                            payload: serde_json::Value::Null,
                        },
                        (_, Err(err)) => ExtResponse {
                            v: BRIDGE_PROTOCOL_VERSION,
                            msg_type: "response".into(),
                            id: request.id,
                            ok: false,
                            method: None,
                            error: Some(err.to_string()),
                            payload: serde_json::Value::Null,
                        },
                    },
                    Err(err) => ExtResponse {
                        v: BRIDGE_PROTOCOL_VERSION,
                        msg_type: "response".into(),
                        id: request.id,
                        ok: false,
                        method: None,
                        error: Some(format!("SessionEnvelope 格式非法：{err}")),
                        payload: serde_json::Value::Null,
                    },
                }
            }
            other => ExtResponse {
                v: BRIDGE_PROTOCOL_VERSION,
                msg_type: "response".into(),
                id: request.id,
                ok: false,
                method: None,
                error: Some(format!("未知或未授权动作：{other}")),
                payload: serde_json::Value::Null,
            },
        };
        let events = self.drain_bridge_revocation_events().unwrap_or_default();
        (response, events)
    }

    pub fn export_account_envelope(&self, account_ref: &str) -> Result<SessionEnvelope, HostError> {
        self.require_unlocked()?;
        let entries = self
            .state_store
            .load_sessions()
            .map_err(|e| HostError::FlowFailed(e.to_string()))?;
        let entry = entries
            .into_iter()
            .find(|e| e.record.account_ref == account_ref)
            .ok_or_else(|| HostError::FlowFailed(format!("账号 {account_ref} 不存在")))?;

        let issued_at = format_rfc3339_secs(self.clock.now_unix_secs());
        Ok(SessionEnvelope {
            schema_version: SCHEMA_VERSION.to_string(),
            account_ref: entry.record.account_ref,
            target: entry.record.target,
            kind: entry.record.kind,
            payload: entry.record.payload,
            issued_at,
            expires_at: entry.record.expires_at,
            source: Source {
                source_type: SourceType::Sync,
                id: "desktop-host".to_string(),
            },
            scope: None,
            client_env: entry.record.client_env,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_contract::{
        AccessLevel, Cookie, Payload, SameSite, SessionKind, Signature, Target, TargetKind,
    };
    use ab_crypto::{ed25519_keypair, ed25519_sign, CryptoError, Entropy};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicI64, Ordering};

    struct FixedEntropy(u8);
    impl Entropy for FixedEntropy {
        fn fill(&self, out: &mut [u8]) -> Result<(), CryptoError> {
            for (i, b) in out.iter_mut().enumerate() {
                *b = self.0.wrapping_add(i as u8).wrapping_mul(17);
            }
            Ok(())
        }
    }

    struct ManualClock(Arc<AtomicI64>);
    impl Clock for ManualClock {
        fn now_unix_secs(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    struct TestAgentExecutor;
    impl AgentHttpExecutor for TestAgentExecutor {
        fn execute(
            &self,
            authenticated_request: &AgentHttpRequest,
        ) -> Result<AgentHttpResponse, String> {
            assert_eq!(
                authenticated_request.headers.get("Cookie").map(String::as_str),
                Some("p_skey=qq-secret-session")
            );
            let mut headers = BTreeMap::new();
            headers.insert("Set-Cookie".into(), "p_skey=rotated-never-leak".into());
            Ok(AgentHttpResponse {
                status: 200,
                headers,
                body: b"{\"profile\":\"ok\"}".to_vec(),
            })
        }
    }

    #[test]
    fn verifier_uses_canonical_signing_bytes_and_rejects_tamper() {
        let (sk, pk) = ed25519_keypair(&FixedEntropy(5)).unwrap();
        let module = b"wasm-module-binary";
        let module_sha256 = encode_lower_hex(&sha256(module));

        let mut manifest = ConverterManifest {
            id: "qq".into(),
            name: "QQ 本地数据转换器".into(),
            version: "1.2.0".into(),
            level: AccessLevel::C,
            readable_paths: vec!["C:\\Users\\me\\AppData\\Roaming\\QQ".into()],
            entry: "qq.wasm".into(),
            module_sha256,
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: OFFICIAL_SIGNER_ID.into(),
                value: "0".repeat(128),
            },
            min_app_version: Some("0.1.0".into()),
        };

        let payload = canonical_signing_bytes(&manifest);
        let sig = ed25519_sign(&sk, &payload);
        manifest.signature.value = encode_lower_hex(&sig);

        let verifier = HostSignatureVerifier::with_trust_root(OFFICIAL_SIGNER_ID, pk);
        assert_eq!(verifier.verify(&manifest, module), Ok(()));
        assert_eq!(
            verifier.verify(&manifest, b"tampered-module"),
            Err(VerifyError::ModuleHashMismatch)
        );

        let mut tampered_manifest = manifest.clone();
        tampered_manifest.min_app_version = Some("0.2.0".into());
        assert_eq!(
            verifier.verify(&tampered_manifest, module),
            Err(VerifyError::BadSignature)
        );

        let mut wrong_signer = manifest;
        wrong_signer.signature.signer = "untrusted-signer".into();
        assert_eq!(
            verifier.verify(&wrong_signer, module),
            Err(VerifyError::SignerNotAllowed)
        );
    }

    #[test]
    fn end_to_end_session_flows_with_shadow_token_tunnel_and_timed_revocation() {
        let dir = std::env::temp_dir().join(format!("aibridge-e2e-flow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let now = Arc::new(AtomicI64::new(1_791_504_000)); // 2026-10-09T00:00:00Z
        let state =
            HostState::with_data_dir_and_clock(&dir, Arc::new(ManualClock(now.clone()))).unwrap();
        state.set_unlocked(true).unwrap();

        let app_access = StandardAccess {
            schema_version: "1.0".into(),
            app_id: "qq".into(),
            level: AccessLevel::C,
            generated_at: "2026-10-09T00:00:00Z".into(),
            credentials: Vec::new(),
            sessions: vec![SessionRecord {
                account_ref: "acc-qq-1".into(),
                target: Target {
                    kind: TargetKind::Web,
                    domain: Some("qzone.qq.com".into()),
                    app_id: Some("qq".into()),
                },
                kind: SessionKind::Cookie,
                payload: Payload {
                    cookies: vec![Cookie {
                        name: "p_skey".into(),
                        value: "qq-secret-session".into(),
                        domain: Some("qzone.qq.com".into()),
                        path: Some("/".into()),
                        secure: Some(true),
                        http_only: Some(true),
                        same_site: Some(SameSite::Lax),
                        expires_at: Some("2026-12-01T00:00:00Z".into()),
                    }],
                    ..Payload::default()
                },
                expires_at: Some("2026-12-01T00:00:00Z".into()),
                source: Source {
                    source_type: SourceType::Converter,
                    id: "qq".into(),
                },
                client_env: None,
            }],
            meta: BTreeMap::new(),
            client_env: None,
        };
        assert_eq!(state.ingest_standard_access(app_access).unwrap(), 1);

        let issued = state
            .issue_temporary_session_with_write_gate(
                "acc-qq-1",
                ScopeSpec {
                    domain_allowlist: vec!["qzone.qq.com".into()],
                    read_only: true,
                    single_use: false,
                },
                120,
                true,
            )
            .unwrap();

        // 1. 传统 CLI 工具凭 Shadow Token 发起请求
        let shadow_resp = state
            .execute_shadow_token_request(
                &issued.shadow_token,
                AgentHttpRequest {
                    method: "GET".into(),
                    url: "https://qzone.qq.com/feed".into(),
                    headers: BTreeMap::new(),
                    body: Vec::new(),
                },
                &TestAgentExecutor,
            )
            .unwrap();
        assert_eq!(shadow_resp.status, 200);

        // 2. 构造浏览器借壳执行隧道请求（session.proxy_fetch）
        let tunnel_req = state
            .build_browser_tunnel_fetch_request(
                "req-tunnel-1",
                &issued.session_id,
                "qzone.qq.com",
                "GET",
                "https://qzone.qq.com/feed",
            )
            .unwrap();
        assert_eq!(tunnel_req.action, "session.proxy_fetch");

        // 3. 到达设定时间（+120 秒）：Shadow Token、MCP 代理与借壳隧道同时失效，并广播 broker.revoked
        now.store(1_791_504_120, Ordering::SeqCst);
        assert!(state
            .execute_shadow_token_request(
                &issued.shadow_token,
                AgentHttpRequest {
                    method: "GET".into(),
                    url: "https://qzone.qq.com/feed".into(),
                    headers: BTreeMap::new(),
                    body: Vec::new(),
                },
                &TestAgentExecutor,
            )
            .is_err());

        let (_ping_resp, revoke_events) = state.handle_extension_request(ExtRequest {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "request".into(),
            id: "req-ping".into(),
            action: "ping".into(),
            payload: json!({}),
        });
        assert_eq!(revoke_events.len(), 1);
        assert_eq!(revoke_events[0].event, "broker.revoked");
        assert_eq!(revoke_events[0].payload["domain"], "qzone.qq.com");
        assert_eq!(revoke_events[0].payload["cookie_names"], json!(["p_skey"]));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
