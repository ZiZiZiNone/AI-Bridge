//! `ab-broker` —— 跨端登录态流转、Scheme A 零明文代理、影子令牌网关、写操作挂起审批与越权自动熔断（B4 / C5 / E4 / S1）。
//!
//! 核心机制：
//! 1. **任意端来源汇聚**：支持从桌面应用（转换器 `StandardAccess`）、浏览器扩展（`session.capture`）、移动端 App（容器/同步）读取已入库登录态；
//! 2. **Scheme A（句柄代持 + 宿主代发请求，零明文出境）**：
//!    AI Agent 仅持有 `IssuedSession { session_id, shadow_token, expires_at, scope, ... }`，绝不接触原始 Cookie / Token 明文；
//!    宿主在内部代填 `Cookie` / `Authorization` 头并自动剥离响应中的 `Set-Cookie` 等敏感头；
//! 3. **影子凭证网关（Shadow Token `ab_tmp_...`）**：
//!    兼容不支持 MCP 的传统 CLI / Agent 工具（仅支持配置 `OPENAI_API_KEY` / `GITHUB_TOKEN` 与 `BASE_URL`），
//!    通过 [`Broker::execute_shadow_token_request`] 将 `Bearer ab_tmp_...` 在本地环回网关内透明换取真实登录态代发，到点即废；
//! 4. **越权自动熔断器（Anomaly Circuit Breaker）**：
//!    同一临时会话累计触发 `max_violations_before_revoke`（默认 3 次）域名越权、协议违规或只读越权时，
//!    无需等待设定时间，当场自动熔断吊销（[`RevokeReason::CircuitBroken`]）并擦除内存敏感载荷；
//! 5. **写操作硬件级 U 盾挂起审批（Human-in-the-Loop Action Gate）**：
//!    开启 `ask_on_write` 的会话在遇到 `POST/PUT/PATCH/DELETE` 时自动生成 [`PendingWriteApproval`] 挂起请求，
//!    由用户在桌面或配对手机上点击「放行单次」「放行 5 分钟」或「拒绝并吊销」；
//! 6. **向浏览器 / 移动 App 容器流转双保险收回**：
//!    自动将所有 Cookie 的 `expires_at` 夹紧至 `min(原到期时间, 租约到期时间)`，并在收回时产出携带
//!    `target_domain`、`cookie_names` 与 `clear_storage: true` 的 [`RevocationNotice`]。
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use ab_audit::{AuditAction, AuditError, AuditEvent, AuditResult, AuditSink};
pub use ab_contract::time::format_rfc3339_secs as format_rfc3339_utc;
use ab_contract::{
    ContractError, Payload, ScopeSpec, SessionEnvelope, SessionKind, Source, SourceType,
    SCHEMA_VERSION,
};
use ab_state::{Clock, StateError, StateStore};
use serde::{Deserialize, Serialize};

/// 单次签发允许的最大 TTL（24 小时）。
pub const MAX_TTL_SECS: u32 = 86_400;
/// 默认空闲超时（30 分钟无访问自动收回）。
pub const DEFAULT_IDLE_TIMEOUT_SECS: u32 = 1_800;
/// 默认连续越权熔断阈值（3 次违规立即自动吊销）。
pub const DEFAULT_MAX_VIOLATIONS_BEFORE_REVOKE: u32 = 3;
/// 写操作挂起审批默认有效期（60 秒）。
pub const WRITE_APPROVAL_TTL_SECS: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub max_ttl_secs: u32,
    pub idle_timeout_secs: u32,
    pub max_violations_before_revoke: u32,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            max_ttl_secs: MAX_TTL_SECS,
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            max_violations_before_revoke: DEFAULT_MAX_VIOLATIONS_BEFORE_REVOKE,
        }
    }
}

/// 对外暴露的临时会话句柄元数据（零敏感 payload，可直接返回给 AI Agent、CLI 工具或 UI）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedSession {
    pub session_id: String,
    /// 供不支持 MCP 的传统 CLI / Agent 工具使用的影子令牌（`ab_tmp_...`），到设定时间或熔断后立即作废。
    pub shadow_token: String,
    pub account_ref: String,
    pub issued_at: String,
    pub expires_at: String,
    pub expires_at_secs: i64,
    pub last_active_secs: i64,
    pub scope: ScopeSpec,
    pub ask_on_write: bool,
    pub violation_count: u32,
    pub revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

/// 写操作挂起审批单（Human-in-the-Loop Action Gate）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingWriteApproval {
    pub approval_id: String,
    pub session_id: String,
    pub account_ref: String,
    pub method: String,
    pub url: String,
    pub domain: String,
    pub requested_at: String,
    pub expires_at_secs: i64,
}

/// 会话收回通知：供宿主桥接层转换为 `broker.revoked` 事件下发给浏览器扩展或 App 容器以清退 Cookie 与 `localStorage`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationNotice {
    pub session_id: String,
    pub account_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_domain: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cookie_names: Vec<String>,
    pub clear_storage: bool,
    pub revoked_at: String,
    pub reason: RevokeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevokeReason {
    TtlExpired,
    IdleTimeout,
    SingleUseConsumed,
    Released,
    ExplicitRevoke,
    RevokeAll,
    CircuitBroken,
    WriteDeniedByUser,
}

/// AI Agent 发起的受控 HTTP 请求（不含凭据，由 Broker 在内部代填）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHttpRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Vec<u8>,
}

/// 经 Broker 剥离 `Set-Cookie` 等敏感响应头后返回给 AI Agent 的安全响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// 宿主注入的受控 HTTP 执行器（可由本机 HTTP 客户端或浏览器扩展 `session.proxy_fetch` 借壳隧道实现）。
pub trait AgentHttpExecutor: Send + Sync {
    fn execute(&self, authenticated_request: &AgentHttpRequest) -> Result<AgentHttpResponse, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerError {
    InvalidTtl(u32),
    EmptyDomainAllowlist,
    AccountSessionNotFound(String),
    SessionNotFound(String),
    InvalidShadowToken,
    ApprovalNotFound(String),
    Expired(String),
    Revoked(String),
    CircuitBroken { session_id: String, violations: u32 },
    WriteApprovalRequired(PendingWriteApproval),
    InvalidTargetUrl(String),
    DomainNotAllowed { session_id: String, domain: String },
    ReadOnlyViolation(String),
    UpstreamExecutionFailed(String),
    Contract(ContractError),
    State(StateError),
    Audit(AuditError),
    LockPoisoned,
}

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrokerError::InvalidTtl(ttl) => {
                write!(f, "非法的临时会话 TTL：{ttl} 秒（须在 1..={MAX_TTL_SECS} 之间）")
            }
            BrokerError::EmptyDomainAllowlist => {
                write!(f, "临时会话 scope.domain_allowlist 不能为空")
            }
            BrokerError::AccountSessionNotFound(acc) => {
                write!(f, "未找到账号 {acc} 的可流转登录态")
            }
            BrokerError::SessionNotFound(id) => write!(f, "临时会话不存在：{id}"),
            BrokerError::InvalidShadowToken => write!(f, "影子令牌无效、已到期或已被收回"),
            BrokerError::ApprovalNotFound(id) => write!(f, "写操作审批单不存在或已过期：{id}"),
            BrokerError::Expired(id) => write!(f, "临时会话已到设定时间并自动收回：{id}"),
            BrokerError::Revoked(id) => write!(f, "临时会话已收回：{id}"),
            BrokerError::CircuitBroken {
                session_id,
                violations,
            } => write!(
                f,
                "临时会话 {session_id} 连续越权 {violations} 次，已触发安全熔断并立即收回"
            ),
            BrokerError::WriteApprovalRequired(pending) => write!(
                f,
                "写操作 [{} {}] 已挂起，需用户确认审批单 {}",
                pending.method, pending.url, pending.approval_id
            ),
            BrokerError::InvalidTargetUrl(url) => {
                write!(f, "非法的受控请求 URL（仅允许 https:// 且含合法域名）：{url}")
            }
            BrokerError::DomainNotAllowed { session_id, domain } => {
                write!(f, "临时会话 {session_id} 未授权访问域名 {domain}")
            }
            BrokerError::ReadOnlyViolation(id) => {
                write!(f, "临时会话 {id} 为只读授权，拒绝写操作")
            }
            BrokerError::UpstreamExecutionFailed(reason) => {
                write!(f, "受控代理请求执行失败：{reason}")
            }
            BrokerError::Contract(e) => write!(f, "契约校验失败：{e}"),
            BrokerError::State(e) => write!(f, "状态库访问失败：{e}"),
            BrokerError::Audit(e) => write!(f, "审计入账失败（fail-closed）：{e}"),
            BrokerError::LockPoisoned => write!(f, "Broker 内部状态锁不可用"),
        }
    }
}

impl std::error::Error for BrokerError {}

impl From<ContractError> for BrokerError {
    fn from(e: ContractError) -> Self {
        BrokerError::Contract(e)
    }
}

impl From<StateError> for BrokerError {
    fn from(e: StateError) -> Self {
        BrokerError::State(e)
    }
}

impl From<AuditError> for BrokerError {
    fn from(e: AuditError) -> Self {
        BrokerError::Audit(e)
    }
}

struct ManagedSession {
    meta: IssuedSession,
    target_domain: Option<String>,
    cookie_names: Vec<String>,
    single_write_approved: bool,
    write_grant_until_secs: Option<i64>,
    /// 代持的受控会话信封；一旦到期、熔断或收回立即清零 `payload`。
    envelope: Option<SessionEnvelope>,
}

impl ManagedSession {
    fn wipe_secret_payload(&mut self) {
        if let Some(env) = &mut self.envelope {
            env.payload = Payload::default();
        }
        self.envelope = None;
        self.single_write_approved = false;
        self.write_grant_until_secs = None;
    }

    fn build_revocation_notice(&self, revoked_at: String, reason: RevokeReason) -> RevocationNotice {
        RevocationNotice {
            session_id: self.meta.session_id.clone(),
            account_ref: self.meta.account_ref.clone(),
            target_domain: self.target_domain.clone(),
            cookie_names: self.cookie_names.clone(),
            clear_storage: true,
            revoked_at,
            reason,
        }
    }
}

struct Inner {
    next_seq: u64,
    sessions: BTreeMap<String, ManagedSession>,
    pending_approvals: BTreeMap<String, PendingWriteApproval>,
    pending_revocations: Vec<RevocationNotice>,
}

pub struct Broker<S: StateStore, C: Clock, A: AuditSink> {
    store: S,
    clock: C,
    audit: A,
    config: BrokerConfig,
    inner: Mutex<Inner>,
}

impl<S: StateStore, C: Clock, A: AuditSink> Broker<S, C, A> {
    pub fn new(store: S, clock: C, audit: A, config: BrokerConfig) -> Self {
        Self {
            store,
            clock,
            audit,
            config,
            inner: Mutex::new(Inner {
                next_seq: 1,
                sessions: BTreeMap::new(),
                pending_approvals: BTreeMap::new(),
                pending_revocations: Vec::new(),
            }),
        }
    }

    /// 签发带设定时间（`ttl_seconds`）与授权范围的临时会话句柄（默认不启用写操作挂起审批，直接按 `scope.read_only` 执行）。
    pub fn issue(
        &self,
        account_ref: &str,
        scope: ScopeSpec,
        ttl_seconds: u32,
    ) -> Result<IssuedSession, BrokerError> {
        self.issue_with_write_gate(account_ref, scope, ttl_seconds, false)
    }

    /// 签发临时会话句柄并显式指定是否启用「写操作硬件级 U 盾挂起审批（`ask_on_write`）」。
    pub fn issue_with_write_gate(
        &self,
        account_ref: &str,
        scope: ScopeSpec,
        ttl_seconds: u32,
        ask_on_write: bool,
    ) -> Result<IssuedSession, BrokerError> {
        let _ = self.sweep()?;

        if ttl_seconds == 0 || ttl_seconds > self.config.max_ttl_secs {
            return Err(BrokerError::InvalidTtl(ttl_seconds));
        }
        if scope.domain_allowlist.is_empty()
            || scope.domain_allowlist.iter().any(|d| d.trim().is_empty())
        {
            return Err(BrokerError::EmptyDomainAllowlist);
        }

        let entries = self.store.load_sessions()?;
        let source_entry = entries
            .into_iter()
            .find(|e| e.record.account_ref == account_ref)
            .ok_or_else(|| BrokerError::AccountSessionNotFound(account_ref.to_string()))?;

        let now_secs = self.clock.now_unix_secs();
        let expires_secs = now_secs.saturating_add(i64::from(ttl_seconds));
        let issued_at = format_rfc3339_utc(now_secs);
        let expires_at = format_rfc3339_utc(expires_secs);

        // 将所有 Cookie 的 expires_at 夹紧至不晚于临时会话设定收回时间
        let mut clamped_payload = source_entry.record.payload.clone();
        for cookie in &mut clamped_payload.cookies {
            cookie.expires_at = Some(match &cookie.expires_at {
                Some(existing) if existing.as_str() < expires_at.as_str() => existing.clone(),
                _ => expires_at.clone(),
            });
        }
        let cookie_names: Vec<String> = clamped_payload
            .cookies
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let target_domain = source_entry
            .record
            .target
            .domain
            .clone()
            .or_else(|| scope.domain_allowlist.first().cloned());

        let envelope = SessionEnvelope {
            schema_version: SCHEMA_VERSION.to_string(),
            account_ref: source_entry.record.account_ref.clone(),
            target: source_entry.record.target.clone(),
            kind: source_entry.record.kind,
            payload: clamped_payload,
            issued_at: issued_at.clone(),
            expires_at: Some(expires_at.clone()),
            source: Source {
                source_type: SourceType::Sync,
                id: "ab-broker".to_string(),
            },
            scope: Some(scope.clone()),
            client_env: source_entry.record.client_env.clone(),
        };
        envelope.validate_temporary()?;

        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        let seq = guard.next_seq;
        guard.next_seq += 1;
        let session_id = format!("tmp-{now_secs}-{seq}");
        let shadow_token = derive_shadow_token(&session_id, account_ref, now_secs, seq);

        let mut audit_event = AuditEvent::from_envelope(
            "ab-broker",
            issued_at.clone(),
            AuditAction::Issue,
            AuditResult::Ok,
            &envelope,
        );
        audit_event
            .detail
            .insert("session_id".into(), session_id.clone());
        audit_event
            .detail
            .insert("ttl_seconds".into(), ttl_seconds.to_string());
        audit_event
            .detail
            .insert("ask_on_write".into(), ask_on_write.to_string());
        self.audit.record_or_fail(audit_event)?;

        let meta = IssuedSession {
            session_id: session_id.clone(),
            shadow_token,
            account_ref: envelope.account_ref.clone(),
            issued_at,
            expires_at,
            expires_at_secs: expires_secs,
            last_active_secs: now_secs,
            scope,
            ask_on_write,
            violation_count: 0,
            revoked: false,
            revoked_at: None,
        };

        guard.sessions.insert(
            session_id,
            ManagedSession {
                meta: meta.clone(),
                target_domain,
                cookie_names,
                single_write_approved: false,
                write_grant_until_secs: None,
                envelope: Some(envelope),
            },
        );
        Ok(meta)
    }

    /// 【影子凭证网关】供不支持 MCP 的传统 CLI / Agent 工具凭 `ab_tmp_...`（或 `Bearer ab_tmp_...`）执行受控代理请求。
    pub fn execute_shadow_token_request(
        &self,
        shadow_token_or_bearer: &str,
        request: AgentHttpRequest,
        executor: &dyn AgentHttpExecutor,
    ) -> Result<AgentHttpResponse, BrokerError> {
        let _ = self.sweep()?;
        let token = shadow_token_or_bearer
            .trim()
            .strip_prefix("Bearer ")
            .or_else(|| shadow_token_or_bearer.trim().strip_prefix("bearer "))
            .unwrap_or_else(|| shadow_token_or_bearer.trim());
        if !token.starts_with("ab_tmp_") {
            return Err(BrokerError::InvalidShadowToken);
        }

        let session_id = {
            let guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
            guard
                .sessions
                .values()
                .find(|m| !m.meta.revoked && ct_str_eq(&m.meta.shadow_token, token))
                .map(|m| m.meta.session_id.clone())
                .ok_or(BrokerError::InvalidShadowToken)?
        };

        self.execute_agent_request(&session_id, request, executor)
    }

    /// 【Scheme A：AI Agent 零明文受控代发请求】
    /// AI Agent 仅传 `session_id` 与不含凭据的 `AgentHttpRequest`；
    /// Broker 校验未到期、未触发越权熔断、域名在白名单内、符合只读或写审批约束后，
    /// 在内部代填 `Cookie` / `Authorization` 头交由 `executor` 执行，并剥离响应中的 `Set-Cookie` 等敏感头后返回。
    pub fn execute_agent_request(
        &self,
        session_id: &str,
        request: AgentHttpRequest,
        executor: &dyn AgentHttpExecutor,
    ) -> Result<AgentHttpResponse, BrokerError> {
        let host = match extract_https_host(&request.url) {
            Some(h) => h,
            None => {
                self.record_violation_or_trip_circuit(session_id, "invalid_url", None)?;
                return Err(BrokerError::InvalidTargetUrl(request.url));
            }
        };
        let method_upper = request.method.trim().to_ascii_uppercase();
        let is_write = !matches!(method_upper.as_str(), "GET" | "HEAD" | "OPTIONS");

        let envelope = self.authorize_and_lease_internal(
            session_id,
            &host,
            is_write,
            Some((&method_upper, &request.url)),
        )?;

        let mut authed_req = request;
        authed_req.method = method_upper;
        authed_req
            .headers
            .retain(|k, _| !matches!(k.to_ascii_lowercase().as_str(), "cookie" | "authorization"));

        match envelope.kind {
            SessionKind::Cookie => {
                let cookie_pairs: Vec<String> = envelope
                    .payload
                    .cookies
                    .iter()
                    .map(|c| format!("{}={}", c.name, c.value))
                    .collect();
                if !cookie_pairs.is_empty() {
                    authed_req
                        .headers
                        .insert("Cookie".to_string(), cookie_pairs.join("; "));
                }
            }
            SessionKind::Token => {
                if let Some(token) = &envelope.payload.token {
                    authed_req
                        .headers
                        .insert("Authorization".to_string(), format!("Bearer {token}"));
                }
            }
            SessionKind::Credential => {}
        }

        let mut response = executor
            .execute(&authed_req)
            .map_err(BrokerError::UpstreamExecutionFailed)?;

        response.headers.retain(|k, _| {
            !matches!(
                k.to_ascii_lowercase().as_str(),
                "set-cookie" | "set-cookie2" | "authorization" | "www-authenticate"
            )
        });
        Ok(response)
    }

    /// 【受控跨端流转租约】供宿主内浏览器扩展桥（`session.inject`）或移动/桌面受控容器获取已夹紧到期时间的信封。
    pub fn lease_envelope_for_surface(
        &self,
        session_id: &str,
        requested_domain: &str,
        is_write: bool,
    ) -> Result<SessionEnvelope, BrokerError> {
        self.authorize_and_lease_internal(session_id, requested_domain, is_write, None)
    }

    fn authorize_and_lease_internal(
        &self,
        session_id: &str,
        requested_domain: &str,
        is_write: bool,
        write_req_context: Option<(&str, &str)>,
    ) -> Result<SessionEnvelope, BrokerError> {
        let _ = self.sweep()?;

        let now_secs = self.clock.now_unix_secs();
        let now_str = format_rfc3339_utc(now_secs);
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;

        if !guard.sessions.contains_key(session_id) {
            return Err(BrokerError::SessionNotFound(session_id.to_string()));
        }

        {
            let managed = guard.sessions.get(session_id).unwrap();
            if managed.meta.revoked {
                let expired = now_secs >= managed.meta.expires_at_secs;
                return if expired {
                    Err(BrokerError::Expired(session_id.to_string()))
                } else {
                    Err(BrokerError::Revoked(session_id.to_string()))
                };
            }
        }

        let allowed_domain = {
            let managed = guard.sessions.get(session_id).unwrap();
            domain_allowed(&managed.meta.scope.domain_allowlist, requested_domain)
        };
        if !allowed_domain {
            drop(guard);
            self.record_violation_or_trip_circuit(
                session_id,
                "domain_not_allowed",
                Some(requested_domain),
            )?;
            return Err(BrokerError::DomainNotAllowed {
                session_id: session_id.to_string(),
                domain: requested_domain.to_string(),
            });
        }

        if is_write {
            let (ask_on_write, read_only, has_active_grant, single_approved, account_ref) = {
                let managed = guard.sessions.get(session_id).unwrap();
                let has_grant = managed.single_write_approved
                    || managed
                        .write_grant_until_secs
                        .is_some_and(|until| now_secs < until);
                (
                    managed.meta.ask_on_write,
                    managed.meta.scope.read_only,
                    has_grant,
                    managed.single_write_approved,
                    managed.meta.account_ref.clone(),
                )
            };

            if ask_on_write && !has_active_grant {
                let seq = guard.next_seq;
                guard.next_seq += 1;
                let approval_id = format!("appr-{now_secs}-{seq}");
                let (method, url) = write_req_context.unwrap_or(("WRITE", requested_domain));
                let pending = PendingWriteApproval {
                    approval_id: approval_id.clone(),
                    session_id: session_id.to_string(),
                    account_ref,
                    method: method.to_string(),
                    url: url.to_string(),
                    domain: requested_domain.to_string(),
                    requested_at: now_str,
                    expires_at_secs: now_secs.saturating_add(WRITE_APPROVAL_TTL_SECS),
                };
                guard.pending_approvals.insert(approval_id, pending.clone());
                return Err(BrokerError::WriteApprovalRequired(pending));
            }

            if read_only && !has_active_grant {
                drop(guard);
                self.record_violation_or_trip_circuit(
                    session_id,
                    "read_only_violation",
                    Some(requested_domain),
                )?;
                return Err(BrokerError::ReadOnlyViolation(session_id.to_string()));
            }

            if single_approved {
                if let Some(managed) = guard.sessions.get_mut(session_id) {
                    managed.single_write_approved = false;
                }
            }
        }

        let (envelope, single_use_notice) = {
            let managed = guard.sessions.get_mut(session_id).unwrap();
            let envelope = managed
                .envelope
                .clone()
                .ok_or_else(|| BrokerError::Revoked(session_id.to_string()))?;
            managed.meta.last_active_secs = now_secs;

            let notice = if managed.meta.scope.single_use {
                let mut event = AuditEvent::new(
                    "ab-broker",
                    now_str.clone(),
                    AuditAction::Release,
                    AuditResult::Ok,
                );
                event.account_ref = Some(managed.meta.account_ref.clone());
                event.domain = Some(requested_domain.to_string());
                event
                    .detail
                    .insert("session_id".into(), session_id.to_string());
                event
                    .detail
                    .insert("reason".into(), "single_use_consumed".into());
                self.audit.record_or_fail(event)?;

                managed.meta.revoked = true;
                managed.meta.revoked_at = Some(now_str.clone());
                managed.wipe_secret_payload();
                Some(managed.build_revocation_notice(now_str, RevokeReason::SingleUseConsumed))
            } else {
                None
            };
            (envelope, notice)
        };

        if let Some(notice) = single_use_notice {
            guard.pending_revocations.push(notice);
        }

        Ok(envelope)
    }

    fn record_violation_or_trip_circuit(
        &self,
        session_id: &str,
        kind: &str,
        domain: Option<&str>,
    ) -> Result<(), BrokerError> {
        let now_secs = self.clock.now_unix_secs();
        let now_str = format_rfc3339_utc(now_secs);
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;

        let tripped = {
            let Some(managed) = guard.sessions.get_mut(session_id) else {
                return Ok(());
            };
            if managed.meta.revoked {
                return Ok(());
            }

            managed.meta.violation_count = managed.meta.violation_count.saturating_add(1);
            let violations = managed.meta.violation_count;

            let mut denied_event = AuditEvent::new(
                "ab-broker",
                now_str.clone(),
                AuditAction::Revoke,
                AuditResult::Denied,
            );
            denied_event.account_ref = Some(managed.meta.account_ref.clone());
            denied_event.domain = domain.map(ToString::to_string);
            denied_event
                .detail
                .insert("session_id".into(), session_id.to_string());
            denied_event
                .detail
                .insert("violation".into(), kind.to_string());
            denied_event
                .detail
                .insert("violation_count".into(), violations.to_string());
            self.audit.record_or_fail(denied_event)?;

            if violations >= self.config.max_violations_before_revoke {
                managed.meta.revoked = true;
                managed.meta.revoked_at = Some(now_str.clone());
                managed.wipe_secret_payload();
                let notice =
                    managed.build_revocation_notice(now_str, RevokeReason::CircuitBroken);
                Some((notice, violations))
            } else {
                None
            }
        };

        if let Some((notice, violations)) = tripped {
            guard.pending_revocations.push(notice);
            return Err(BrokerError::CircuitBroken {
                session_id: session_id.to_string(),
                violations,
            });
        }
        Ok(())
    }

    /// 【U 盾写操作审批放行】用户确认放行挂起的写操作（`grant_window_secs = None` 为仅放行单次，`Some(300)` 为放行 5 分钟窗口）。
    pub fn approve_write(
        &self,
        approval_id: &str,
        grant_window_secs: Option<u32>,
    ) -> Result<PendingWriteApproval, BrokerError> {
        let now_secs = self.clock.now_unix_secs();
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        let pending = guard
            .pending_approvals
            .remove(approval_id)
            .ok_or_else(|| BrokerError::ApprovalNotFound(approval_id.to_string()))?;

        if now_secs >= pending.expires_at_secs {
            return Err(BrokerError::ApprovalNotFound(approval_id.to_string()));
        }

        let managed = guard
            .sessions
            .get_mut(&pending.session_id)
            .ok_or_else(|| BrokerError::SessionNotFound(pending.session_id.clone()))?;
        if managed.meta.revoked {
            return Err(BrokerError::Revoked(pending.session_id.clone()));
        }

        match grant_window_secs {
            Some(secs) if secs > 0 => {
                managed.write_grant_until_secs = Some(now_secs.saturating_add(i64::from(secs)));
            }
            _ => {
                managed.single_write_approved = true;
            }
        }
        Ok(pending)
    }

    /// 【U 盾写操作拒绝并立即收回会话】用户拒绝挂起的写操作，当场吊销该临时会话并广播 `broker.revoked`。
    pub fn deny_write_and_revoke(&self, approval_id: &str) -> Result<RevocationNotice, BrokerError> {
        let now_str = format_rfc3339_utc(self.clock.now_unix_secs());
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        let pending = guard
            .pending_approvals
            .remove(approval_id)
            .ok_or_else(|| BrokerError::ApprovalNotFound(approval_id.to_string()))?;

        let notice = {
            let managed = guard
                .sessions
                .get_mut(&pending.session_id)
                .ok_or_else(|| BrokerError::SessionNotFound(pending.session_id.clone()))?;

            let mut event = AuditEvent::new(
                "ab-broker",
                now_str.clone(),
                AuditAction::Revoke,
                AuditResult::Denied,
            );
            event.account_ref = Some(managed.meta.account_ref.clone());
            event.domain = Some(pending.domain.clone());
            event
                .detail
                .insert("session_id".into(), pending.session_id.clone());
            event
                .detail
                .insert("reason".into(), "write_denied_by_user".into());
            self.audit.record_or_fail(event)?;

            managed.meta.revoked = true;
            managed.meta.revoked_at = Some(now_str.clone());
            managed.wipe_secret_payload();
            managed.build_revocation_notice(now_str, RevokeReason::WriteDeniedByUser)
        };

        guard.pending_revocations.push(notice.clone());
        Ok(notice)
    }

    pub fn list_pending_approvals(&self) -> Result<Vec<PendingWriteApproval>, BrokerError> {
        let now_secs = self.clock.now_unix_secs();
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        guard
            .pending_approvals
            .retain(|_, p| now_secs < p.expires_at_secs);
        Ok(guard.pending_approvals.values().cloned().collect())
    }

    /// 续期未到期且未收回的临时会话。
    pub fn renew(&self, session_id: &str, ttl_seconds: u32) -> Result<IssuedSession, BrokerError> {
        let _ = self.sweep()?;
        if ttl_seconds == 0 || ttl_seconds > self.config.max_ttl_secs {
            return Err(BrokerError::InvalidTtl(ttl_seconds));
        }

        let now_secs = self.clock.now_unix_secs();
        let expires_secs = now_secs.saturating_add(i64::from(ttl_seconds));
        let expires_at = format_rfc3339_utc(expires_secs);

        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        let managed = guard
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| BrokerError::SessionNotFound(session_id.to_string()))?;

        if managed.meta.revoked {
            return Err(BrokerError::Revoked(session_id.to_string()));
        }

        managed.meta.expires_at_secs = expires_secs;
        managed.meta.expires_at = expires_at.clone();
        managed.meta.last_active_secs = now_secs;
        if let Some(env) = &mut managed.envelope {
            env.expires_at = Some(expires_at.clone());
            for cookie in &mut env.payload.cookies {
                cookie.expires_at = Some(expires_at.clone());
            }
        }
        Ok(managed.meta.clone())
    }

    /// 主动释放临时会话（用完即放）。
    pub fn release(&self, session_id: &str) -> Result<(), BrokerError> {
        self.terminate_one(session_id, AuditAction::Release, RevokeReason::Released)
    }

    /// 强制吊销单个临时会话并触发跨端撤销通知。
    pub fn revoke(&self, session_id: &str) -> Result<(), BrokerError> {
        self.terminate_one(session_id, AuditAction::Revoke, RevokeReason::ExplicitRevoke)
    }

    fn terminate_one(
        &self,
        session_id: &str,
        action: AuditAction,
        reason: RevokeReason,
    ) -> Result<(), BrokerError> {
        let now_str = format_rfc3339_utc(self.clock.now_unix_secs());
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        let notice = {
            let managed = guard
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| BrokerError::SessionNotFound(session_id.to_string()))?;

            if !managed.meta.revoked {
                let mut event =
                    AuditEvent::new("ab-broker", now_str.clone(), action, AuditResult::Ok);
                event.account_ref = Some(managed.meta.account_ref.clone());
                event
                    .detail
                    .insert("session_id".into(), session_id.to_string());
                self.audit.record_or_fail(event)?;

                managed.meta.revoked = true;
                managed.meta.revoked_at = Some(now_str.clone());
                managed.wipe_secret_payload();
                Some(managed.build_revocation_notice(now_str, reason))
            } else {
                None
            }
        };

        if let Some(n) = notice {
            guard.pending_revocations.push(n);
        }
        Ok(())
    }

    /// 一键吊销全部活跃临时会话。
    pub fn revoke_all(&self) -> Result<Vec<IssuedSession>, BrokerError> {
        let now_str = format_rfc3339_utc(self.clock.now_unix_secs());
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        guard.pending_approvals.clear();
        let mut revoked_list = Vec::new();
        let mut notices = Vec::new();

        for (session_id, managed) in guard.sessions.iter_mut() {
            if !managed.meta.revoked {
                let mut event = AuditEvent::new(
                    "ab-broker",
                    now_str.clone(),
                    AuditAction::RevokeAll,
                    AuditResult::Ok,
                );
                event.account_ref = Some(managed.meta.account_ref.clone());
                event
                    .detail
                    .insert("session_id".into(), session_id.clone());
                self.audit.record_or_fail(event)?;

                managed.meta.revoked = true;
                managed.meta.revoked_at = Some(now_str.clone());
                managed.wipe_secret_payload();
                revoked_list.push(managed.meta.clone());
                notices.push(
                    managed.build_revocation_notice(now_str.clone(), RevokeReason::RevokeAll),
                );
            }
        }
        guard.pending_revocations.extend(notices);
        Ok(revoked_list)
    }

    /// 扫描并自动收回所有已达设定时间（`now >= expires_at`）或超过空闲超时的临时会话。
    pub fn sweep(&self) -> Result<Vec<IssuedSession>, BrokerError> {
        let now_secs = self.clock.now_unix_secs();
        let now_str = format_rfc3339_utc(now_secs);
        let idle_limit = i64::from(self.config.idle_timeout_secs);

        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        guard
            .pending_approvals
            .retain(|_, p| now_secs < p.expires_at_secs);
        let mut swept = Vec::new();
        let mut notices = Vec::new();

        for (session_id, managed) in guard.sessions.iter_mut() {
            if managed.meta.revoked {
                continue;
            }
            let ttl_expired = now_secs >= managed.meta.expires_at_secs;
            let idle_expired =
                now_secs.saturating_sub(managed.meta.last_active_secs) >= idle_limit;

            if ttl_expired || idle_expired {
                let reason = if ttl_expired {
                    RevokeReason::TtlExpired
                } else {
                    RevokeReason::IdleTimeout
                };
                let mut event = AuditEvent::new(
                    "ab-broker",
                    now_str.clone(),
                    AuditAction::Sweep,
                    AuditResult::Ok,
                );
                event.account_ref = Some(managed.meta.account_ref.clone());
                event
                    .detail
                    .insert("session_id".into(), session_id.clone());
                self.audit.record_or_fail(event)?;

                managed.meta.revoked = true;
                managed.meta.revoked_at = Some(now_str.clone());
                managed.wipe_secret_payload();
                swept.push(managed.meta.clone());
                notices.push(managed.build_revocation_notice(now_str.clone(), reason));
            }
        }
        guard.pending_revocations.extend(notices);
        Ok(swept)
    }

    pub fn list_sessions(&self) -> Result<Vec<IssuedSession>, BrokerError> {
        let _ = self.sweep()?;
        let guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        Ok(guard.sessions.values().map(|m| m.meta.clone()).collect())
    }

    /// 取出待广播的收回通知（供宿主桥向浏览器扩展发送 `broker.revoked` 事件或通知移动端容器清理）。
    pub fn drain_revocations(&self) -> Result<Vec<RevocationNotice>, BrokerError> {
        let mut guard = self.inner.lock().map_err(|_| BrokerError::LockPoisoned)?;
        Ok(std::mem::take(&mut guard.pending_revocations))
    }
}

fn derive_shadow_token(session_id: &str, account_ref: &str, now_secs: i64, seq: u64) -> String {
    let mut state = [0x5au8; 16];
    for (idx, b) in session_id
        .bytes()
        .chain(account_ref.bytes())
        .chain(now_secs.to_le_bytes())
        .chain(seq.to_le_bytes())
        .enumerate()
    {
        let pos = idx % 16;
        state[pos] = state[pos].wrapping_add(b.rotate_left(3)) ^ ((idx as u8).wrapping_mul(31));
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex_tag = String::with_capacity(32);
    for &b in &state {
        hex_tag.push(HEX[(b >> 4) as usize] as char);
        hex_tag.push(HEX[(b & 0x0f) as usize] as char);
    }
    format!("ab_tmp_{session_id}_{hex_tag}")
}

fn ct_str_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.as_bytes().iter().zip(b.as_bytes().iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn extract_https_host(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = authority.split(':').next()?.trim().to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    Some(host)
}

fn domain_allowed(allowlist: &[String], requested: &str) -> bool {
    let req = requested.trim().trim_start_matches('.').to_ascii_lowercase();
    if req.is_empty() {
        return false;
    }
    allowlist.iter().any(|pattern| {
        let pat = pattern.trim().trim_start_matches('.').to_ascii_lowercase();
        if pat.is_empty() {
            return false;
        }
        req == pat || req.ends_with(&format!(".{pat}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_audit::InMemoryAuditSink;
    use ab_contract::{Cookie, SameSite, SessionRecord, Target, TargetKind};
    use ab_state::{Endpoint, EndpointKind, InMemoryStateStore, SessionEntry};
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct MockClock(Arc<AtomicI64>);
    impl Clock for MockClock {
        fn now_unix_secs(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    struct MockHttpExecutor;
    impl AgentHttpExecutor for MockHttpExecutor {
        fn execute(
            &self,
            authenticated_request: &AgentHttpRequest,
        ) -> Result<AgentHttpResponse, String> {
            assert_eq!(
                authenticated_request.headers.get("Cookie").map(String::as_str),
                Some("user_session=secret-cookie-token")
            );
            let mut headers = BTreeMap::new();
            headers.insert("Content-Type".into(), "application/json".into());
            headers.insert("Set-Cookie".into(), "rotated=must-be-stripped".into());
            Ok(AgentHttpResponse {
                status: 200,
                headers,
                body: b"{\"ok\":true}".to_vec(),
            })
        }
    }

    fn seed_store() -> InMemoryStateStore {
        let store = InMemoryStateStore::new();
        let entry = SessionEntry::new(
            Endpoint::new(EndpointKind::Browser, "chromium"),
            SessionRecord {
                account_ref: "acc-github".into(),
                target: Target {
                    kind: TargetKind::Web,
                    domain: Some("github.com".into()),
                    app_id: None,
                },
                kind: SessionKind::Cookie,
                payload: Payload {
                    cookies: vec![Cookie {
                        name: "user_session".into(),
                        value: "secret-cookie-token".into(),
                        domain: Some("github.com".into()),
                        path: Some("/".into()),
                        secure: Some(true),
                        http_only: Some(true),
                        same_site: Some(SameSite::Lax),
                        expires_at: Some("2026-12-31T00:00:00Z".into()),
                    }],
                    ..Payload::default()
                },
                expires_at: Some("2026-12-31T00:00:00Z".into()),
                source: Source {
                    source_type: SourceType::Extension,
                    id: "chromium".into(),
                },
                client_env: None,
            },
        );
        store.replace_sessions(vec![entry]).unwrap();
        store
    }

    #[test]
    fn scheme_a_agent_proxy_and_shadow_token_work_and_expire() {
        let now = Arc::new(AtomicI64::new(1_791_504_000));
        let broker = Broker::new(
            seed_store(),
            MockClock(now.clone()),
            InMemoryAuditSink::new(),
            BrokerConfig::default(),
        );

        let issued = broker
            .issue(
                "acc-github",
                ScopeSpec {
                    domain_allowlist: vec!["github.com".into()],
                    read_only: true,
                    single_use: false,
                },
                60,
            )
            .unwrap();
        assert!(issued.shadow_token.starts_with("ab_tmp_"));

        let resp = broker
            .execute_shadow_token_request(
                &format!("Bearer {}", issued.shadow_token),
                AgentHttpRequest {
                    method: "GET".into(),
                    url: "https://api.github.com/user".into(),
                    headers: BTreeMap::new(),
                    body: Vec::new(),
                },
                &MockHttpExecutor,
            )
            .unwrap();
        assert_eq!(resp.status, 200);
        assert!(!resp.headers.contains_key("Set-Cookie"));

        now.store(1_791_504_060, Ordering::SeqCst);
        assert_eq!(
            broker.execute_shadow_token_request(
                &issued.shadow_token,
                AgentHttpRequest {
                    method: "GET".into(),
                    url: "https://api.github.com/user".into(),
                    headers: BTreeMap::new(),
                    body: Vec::new(),
                },
                &MockHttpExecutor,
            ),
            Err(BrokerError::InvalidShadowToken)
        );
    }

    #[test]
    fn anomaly_circuit_breaker_revokes_session_after_three_violations() {
        let now = Arc::new(AtomicI64::new(1_791_504_000));
        let broker = Broker::new(
            seed_store(),
            MockClock(now),
            InMemoryAuditSink::new(),
            BrokerConfig::default(),
        );

        let issued = broker
            .issue(
                "acc-github",
                ScopeSpec {
                    domain_allowlist: vec!["github.com".into()],
                    read_only: true,
                    single_use: false,
                },
                600,
            )
            .unwrap();

        assert!(matches!(
            broker.lease_envelope_for_surface(&issued.session_id, "evil1.example", false),
            Err(BrokerError::DomainNotAllowed { .. })
        ));
        assert!(matches!(
            broker.lease_envelope_for_surface(&issued.session_id, "evil2.example", false),
            Err(BrokerError::DomainNotAllowed { .. })
        ));
        assert_eq!(
            broker.lease_envelope_for_surface(&issued.session_id, "evil3.example", false),
            Err(BrokerError::CircuitBroken {
                session_id: issued.session_id.clone(),
                violations: 3,
            })
        );

        assert!(matches!(
            broker.lease_envelope_for_surface(&issued.session_id, "github.com", false),
            Err(BrokerError::Revoked(_))
        ));
        let notices = broker.drain_revocations().unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].reason, RevokeReason::CircuitBroken);
    }

    #[test]
    fn human_in_the_loop_write_gate_holds_and_approves_or_revokes() {
        let now = Arc::new(AtomicI64::new(1_791_504_000));
        let broker = Broker::new(
            seed_store(),
            MockClock(now),
            InMemoryAuditSink::new(),
            BrokerConfig::default(),
        );

        let issued = broker
            .issue_with_write_gate(
                "acc-github",
                ScopeSpec {
                    domain_allowlist: vec!["github.com".into()],
                    read_only: true,
                    single_use: false,
                },
                600,
                true,
            )
            .unwrap();

        let write_req = AgentHttpRequest {
            method: "POST".into(),
            url: "https://api.github.com/repos/a/b/issues".into(),
            headers: BTreeMap::new(),
            body: b"{}".to_vec(),
        };
        let err = broker
            .execute_agent_request(&issued.session_id, write_req.clone(), &MockHttpExecutor)
            .unwrap_err();
        let pending = match err {
            BrokerError::WriteApprovalRequired(p) => p,
            other => panic!("期望 WriteApprovalRequired，得到 {other:?}"),
        };

        broker.approve_write(&pending.approval_id, None).unwrap();
        assert!(broker
            .execute_agent_request(&issued.session_id, write_req.clone(), &MockHttpExecutor)
            .is_ok());

        let err2 = broker
            .execute_agent_request(&issued.session_id, write_req, &MockHttpExecutor)
            .unwrap_err();
        if let BrokerError::WriteApprovalRequired(p2) = err2 {
            let notice = broker.deny_write_and_revoke(&p2.approval_id).unwrap();
            assert_eq!(notice.reason, RevokeReason::WriteDeniedByUser);
        } else {
            panic!("期望第二次挂起");
        }
    }
}
