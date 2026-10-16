//! `ab-broker` —— 临时会话 broker：签发 / TTL / 空闲 / 撤销表 / scope 强制。
//!
//! 核心承诺（`docs/安全模型.md` §5、`docs/流程/AI端流程.md`）：
//! - **真实会话由本服务代持**，AI 只拿不透明随机句柄，永远拿不到真实凭据（C2.1）。
//! - 句柄表只存 HMAC 后的键（`ab-crypto` `BrokerToken` 域密钥），表泄露不可反推可用句柄。
//! - 释放三重触发先到先生效：显式 release / 空闲 30min / TTL 到期（默认 1h，上限 24h）（C2.2）。
//! - scope 业务强制在本 crate（C4）：域名白名单 / 只读 / 单次；空数组 = 不允许任何 web 目标。
//! - 一切动作进追加式审计且 **fail-closed**：审计写入失败则敏感操作不执行（C3.1）。
//! - 只吊销我方凭证，默认不登出第三方；强登出是宿主层显式动作。
//!
//! 明确不做：UI、网络句柄、直接登出第三方、持久化（临时会话随进程存续，重启即清）。
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use ab_audit::{AuditAction, AuditError, AuditEvent, AuditResult, AuditSink};
use ab_contract::time::format_rfc3339_secs as format_rfc3339;
use ab_contract::{ContractError, ScopeSpec, SessionEnvelope, Source, Target, TargetKind};
use ab_crypto::{derive_domain, hmac_sign, CryptoError, DomainLabel, Entropy, Zeroizing};

const REDACTED: &str = "[REDACTED]";

/// TTL 默认 1h（C2.2）。
pub const DEFAULT_TTL_SECS: u64 = 3600;
/// TTL 上限 24h；绝对寿命同样以 24h 封顶，任何续期手段不得超过（C2.2）。
pub const MAX_TTL_SECS: u64 = 86_400;
/// 空闲 30min 自动释放（C2.2）。
pub const IDLE_TIMEOUT_SECS: u64 = 1800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerError {
    Contract(ContractError),
    Audit(AuditError),
    Crypto(CryptoError),
    /// 锁中毒等服务不可用。
    Unavailable,
    EntropyFailed,
    InvalidTtl,
    InvalidScope,
    TargetNotInScope,
    BadHandle,
    UnknownSession,
    NotActive,
    Expired,
    SingleUseConsumed,
    ReadOnlyViolation,
    DomainNotAllowed,
}

impl BrokerError {
    /// 稳定错误码（审计 detail 用，不含任何秘密与细节）。
    fn code(&self) -> &'static str {
        match self {
            BrokerError::Contract(_) => "contract_invalid",
            BrokerError::Audit(_) => "audit_unavailable",
            BrokerError::Crypto(_) => "crypto_failed",
            BrokerError::Unavailable => "unavailable",
            BrokerError::EntropyFailed => "entropy_failed",
            BrokerError::InvalidTtl => "invalid_ttl",
            BrokerError::InvalidScope => "invalid_scope",
            BrokerError::TargetNotInScope => "target_not_in_scope",
            BrokerError::BadHandle => "bad_handle",
            BrokerError::UnknownSession => "unknown_session",
            BrokerError::NotActive => "not_active",
            BrokerError::Expired => "expired",
            BrokerError::SingleUseConsumed => "single_use_consumed",
            BrokerError::ReadOnlyViolation => "read_only_violation",
            BrokerError::DomainNotAllowed => "domain_not_allowed",
        }
    }
}

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BrokerError::Contract(e) => return write!(f, "契约校验失败: {e}"),
            BrokerError::Audit(e) => return write!(f, "审计不可用: {e}"),
            BrokerError::Crypto(e) => return write!(f, "密码学操作失败: {e}"),
            BrokerError::Unavailable => "broker 状态不可用",
            BrokerError::EntropyFailed => "随机源不可用",
            BrokerError::InvalidTtl => "TTL 非法（要求 1s..=24h）",
            BrokerError::InvalidScope => "scope 非法（白名单条目不能为空）",
            BrokerError::TargetNotInScope => "目标不在域名白名单内",
            BrokerError::BadHandle => "句柄格式非法",
            BrokerError::UnknownSession => "会话不存在",
            BrokerError::NotActive => "会话已结束（释放/吊销）",
            BrokerError::Expired => "会话已过期（TTL 或空闲）",
            BrokerError::SingleUseConsumed => "单次会话已使用",
            BrokerError::ReadOnlyViolation => "只读会话拒绝写操作",
            BrokerError::DomainNotAllowed => "域名不在白名单内",
        };
        f.write_str(s)
    }
}

impl std::error::Error for BrokerError {}

impl From<ContractError> for BrokerError {
    fn from(e: ContractError) -> Self {
        BrokerError::Contract(e)
    }
}

impl From<AuditError> for BrokerError {
    fn from(e: AuditError) -> Self {
        BrokerError::Audit(e)
    }
}

impl From<CryptoError> for BrokerError {
    fn from(e: CryptoError) -> Self {
        BrokerError::Crypto(e)
    }
}

/// 会话生命周期状态；终态记录保留为撤销表条目（句柄永不再被接受）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Active,
    Released,
    Revoked,
    Consumed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Read,
    Write,
}

/// 签发请求：scope 必填，`ttl_secs` 缺省 1h。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIssue {
    pub scope: ScopeSpec,
    pub ttl_secs: Option<u64>,
}

/// 签发结果：`handle` 是代持通道的不透明凭证，**只返回一次**，表内不留明文。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TemporaryGrant {
    pub session_id: String,
    pub handle: String,
    pub account_ref: String,
    pub target: Target,
    pub scope: ScopeSpec,
    pub issued_at: String,
    pub expires_at: String,
    pub source: Source,
}

// 手工 Debug：句柄脱敏（F7 内存卫生）。
impl fmt::Debug for TemporaryGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TemporaryGrant")
            .field("session_id", &self.session_id)
            .field("handle", &REDACTED)
            .field("account_ref", &self.account_ref)
            .field("target", &self.target)
            .field("scope", &self.scope)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("source", &self.source)
            .finish()
    }
}

/// 总览行（A2.2：剩余 TTL / scope / 来源）；不含句柄。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionStatus {
    pub session_id: String,
    pub account_ref: String,
    pub target: Target,
    pub scope: ScopeSpec,
    pub source: Source,
    pub issued_at: String,
    pub expires_at: String,
    pub remaining_ttl_secs: u64,
    pub state: SessionState,
    pub who: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionUse<'a> {
    pub handle: &'a str,
    /// web 目标按域名裁决；app / container 目标不需要。
    pub domain: Option<&'a str>,
    pub action: SessionAction,
}

/// Scheme A：AI Agent 仅凭不透明句柄发起受控 HTTP 请求，宿主内部代填 Cookie/Token 并剥离响应 `Set-Cookie`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Scheme A：剥离 `Set-Cookie` 后的受控 HTTP 响应。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentHttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Scheme A：宿主注入的受控 HTTP 执行器接口。
pub trait AgentHttpExecutor: Send + Sync {
    fn execute(&self, authenticated_request: &AgentHttpRequest) -> Result<AgentHttpResponse, String>;
}

/// 授权许可：携带代持真会话（仅供代持通道使用，禁止回流给 AI）。
#[derive(Debug, Clone)]
pub struct UsePermit {
    pub session_id: String,
    pub envelope: SessionEnvelope,
}

struct GrantRecord {
    session_id: String,
    who: String,
    account_ref: String,
    target: Target,
    scope: ScopeSpec,
    source: Source,
    issued_at: String,
    expires_at: String,
    expires_secs: i64,
    idle_deadline_secs: i64,
    absolute_deadline_secs: i64,
    state: SessionState,
    /// 真会话副本；进入终态即清空（清本方副本）。
    real: Option<SessionEnvelope>,
}

impl GrantRecord {
    fn status(&self, now_unix_secs: i64) -> SessionStatus {
        SessionStatus {
            session_id: self.session_id.clone(),
            account_ref: self.account_ref.clone(),
            target: self.target.clone(),
            scope: self.scope.clone(),
            source: self.source.clone(),
            issued_at: self.issued_at.clone(),
            expires_at: self.expires_at.clone(),
            remaining_ttl_secs: (self.expires_secs - now_unix_secs).max(0) as u64,
            state: self.state,
            who: self.who.clone(),
        }
    }
}

/// 临时会话服务：状态自持（进程内存，重启即清），审计与熵源经参数注入，无全局单例。
/// 审计出口持 `Arc`：桌面 UI 与 MCP server 共享同一服务实例。
pub type Broker = SessionBroker;

pub struct SessionBroker {
    audit: Arc<dyn AuditSink>,
    mac_key: Zeroizing<[u8; 32]>,
    records: Mutex<BTreeMap<[u8; 32], GrantRecord>>,
}

impl SessionBroker {
    /// `master_key` 只用于派生句柄查表键（`aibridge/v1/broker-token` 域），不落表。
    pub fn new(audit: Arc<dyn AuditSink>, master_key: &[u8; 32]) -> Result<Self, BrokerError> {
        let mac_key = derive_domain(master_key, None, DomainLabel::BrokerToken)?;
        Ok(Self {
            audit,
            mac_key,
            records: Mutex::new(BTreeMap::new()),
        })
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, BTreeMap<[u8; 32], GrantRecord>>, BrokerError> {
        self.records.lock().map_err(|_| BrokerError::Unavailable)
    }

    /// fail-closed 唯一写账出口：写账失败则调用方敏感操作不得执行。
    fn audit_or_fail(&self, event: AuditEvent) -> Result<(), BrokerError> {
        self.audit.record_or_fail(event).map_err(BrokerError::Audit)
    }

    /// 拒绝路径：记 Denied 审计后返回原错误；审计失败则返回审计错误。
    fn deny(
        &self,
        who: &str,
        when: String,
        action: AuditAction,
        err: BrokerError,
        real: Option<&SessionEnvelope>,
    ) -> BrokerError {
        let mut event = match real {
            Some(env) => AuditEvent::from_envelope(who, when, action, AuditResult::Denied, env),
            None => AuditEvent::new(who, when, action, AuditResult::Denied),
        };
        event.detail.insert("reason".into(), err.code().into());
        match self.audit.record_or_fail(event) {
            Ok(()) => err,
            Err(audit_err) => BrokerError::Audit(audit_err),
        }
    }

    /// 签发临时会话（C2.1 / C4.1）：真会话代持，返回一次性的不透明句柄。
    pub fn issue(
        &self,
        who: &str,
        real: &SessionEnvelope,
        request: SessionIssue,
        entropy: &dyn Entropy,
        now_unix_secs: i64,
    ) -> Result<TemporaryGrant, BrokerError> {
        let when = format_rfc3339(now_unix_secs);
        if let Err(err) = real.validate().map_err(BrokerError::from) {
            return Err(self.deny(who, when, AuditAction::Issue, err, Some(real)));
        }
        if let Err(err) = check_scope(&request.scope) {
            return Err(self.deny(who, when, AuditAction::Issue, err, Some(real)));
        }
        let ttl_secs = match check_ttl(request.ttl_secs) {
            Ok(ttl) => ttl,
            Err(err) => return Err(self.deny(who, when, AuditAction::Issue, err, Some(real))),
        };
        if real.target.kind == TargetKind::Web {
            let domain = real.target.domain.as_deref().unwrap_or_default();
            // 双向校验：目标必须被白名单覆盖，且白名单不得比目标更宽
            //（审查报告 P1-3：`["com"]` 这类超集白名单会使 C4.1 形同虚设）。
            if !domain_covered(&request.scope.domain_allowlist, domain)
                || !allowlist_narrows_target(&request.scope.domain_allowlist, domain)
            {
                return Err(self.deny(
                    who,
                    when,
                    AuditAction::Issue,
                    BrokerError::TargetNotInScope,
                    Some(real),
                ));
            }
        }

        let mut id_bytes = [0u8; 16];
        let mut handle_bytes = [0u8; 32];
        if let Err(err) = entropy
            .fill(&mut id_bytes)
            .and_then(|_| entropy.fill(&mut handle_bytes))
            .map_err(|_| BrokerError::EntropyFailed)
        {
            return Err(self.deny(who, when, AuditAction::Issue, err, Some(real)));
        }
        let session_id = hex(&id_bytes);
        let handle = hex(&handle_bytes);
        let handle_mac = hmac_sign(&*self.mac_key, &handle_bytes)?;

        let expires_secs = now_unix_secs + ttl_secs as i64;
        let mut event = AuditEvent::from_envelope(
            who,
            when,
            AuditAction::Issue,
            AuditResult::Ok,
            real,
        );
        event
            .detail
            .insert("session_id".into(), session_id.clone());
        event
            .detail
            .insert("scope".into(), scope_summary(&request.scope));
        event.detail.insert("ttl_secs".into(), ttl_secs.to_string());

        let record = GrantRecord {
            session_id: session_id.clone(),
            who: who.to_string(),
            account_ref: real.account_ref.clone(),
            target: real.target.clone(),
            scope: request.scope.clone(),
            source: real.source.clone(),
            issued_at: format_rfc3339(now_unix_secs),
            expires_at: format_rfc3339(expires_secs),
            expires_secs,
            idle_deadline_secs: now_unix_secs + IDLE_TIMEOUT_SECS as i64,
            absolute_deadline_secs: now_unix_secs + MAX_TTL_SECS as i64,
            state: SessionState::Active,
            real: Some(real.clone()),
        };
        let grant = TemporaryGrant {
            session_id,
            handle,
            account_ref: record.account_ref.clone(),
            target: record.target.clone(),
            scope: record.scope.clone(),
            issued_at: record.issued_at.clone(),
            expires_at: record.expires_at.clone(),
            source: record.source.clone(),
        };

        // fail-closed 顺序：先写账，后落表；写账失败则不签发。
        let mut records = self.lock()?;
        self.audit_or_fail(event)?;
        records.insert(handle_mac, record);
        Ok(grant)
    }

    /// 续期（C2.2）：只在绝对寿命（签发 + 24h）内生效，超限拒绝而非截断。
    pub fn renew(
        &self,
        who: &str,
        session_id: &str,
        ttl_secs: Option<u64>,
        now_unix_secs: i64,
    ) -> Result<SessionStatus, BrokerError> {
        let when = format_rfc3339(now_unix_secs);
        let mut records = self.lock()?;

        let (event, new_expires) = {
            let record = match find_by_id(&records, session_id) {
                Some(record) => record,
                None => {
                    return Err(self.deny(
                        who,
                        when,
                        AuditAction::Renew,
                        BrokerError::UnknownSession,
                        None,
                    ))
                }
            };
            if record.state != SessionState::Active {
                let err = state_error(record.state);
                return Err(self.deny(who, when, AuditAction::Renew, err, record.real.as_ref()));
            }
            let real = match record.real.as_ref() {
                Some(real) => real,
                None => {
                    return Err(self.deny(
                        who,
                        when,
                        AuditAction::Renew,
                        BrokerError::Unavailable,
                        None,
                    ))
                }
            };
            if now_unix_secs >= record.expires_secs || now_unix_secs >= record.idle_deadline_secs {
                let err = BrokerError::Expired;
                return Err(self.deny(who, when, AuditAction::Renew, err, Some(real)));
            }
            let ttl = match check_ttl(ttl_secs) {
                Ok(ttl) => ttl,
                Err(err) => return Err(self.deny(who, when, AuditAction::Renew, err, Some(real))),
            };
            let new_expires = now_unix_secs + ttl as i64;
            if new_expires > record.absolute_deadline_secs {
                let err = BrokerError::InvalidTtl;
                return Err(self.deny(who, when, AuditAction::Renew, err, Some(real)));
            }
            let mut event =
                AuditEvent::from_envelope(who, when, AuditAction::Renew, AuditResult::Ok, real);
            event.detail.insert("session_id".into(), session_id.to_string());
            event.detail.insert("ttl_secs".into(), ttl.to_string());
            (event, new_expires)
        };

        self.audit_or_fail(event)?;
        let record = find_by_id_mut(&mut records, session_id).ok_or(BrokerError::Unavailable)?;
        record.expires_secs = new_expires;
        record.expires_at = format_rfc3339(new_expires);
        record.idle_deadline_secs = now_unix_secs + IDLE_TIMEOUT_SECS as i64;
        Ok(record.status(now_unix_secs))
    }

    /// 显式释放（C2.2）：置撤销表失效 + 清本方副本；不触碰第三方会话。
    pub fn release(
        &self,
        who: &str,
        session_id: &str,
        now_unix_secs: i64,
    ) -> Result<(), BrokerError> {
        self.finish(
            who,
            when(now_unix_secs),
            session_id,
            AuditAction::Release,
            SessionState::Released,
            "explicit",
        )
    }

    /// 单会话吊销（C5.1）：用户侧动作，同样只失效我方凭证。
    pub fn revoke(
        &self,
        who: &str,
        session_id: &str,
        now_unix_secs: i64,
    ) -> Result<(), BrokerError> {
        self.finish(
            who,
            when(now_unix_secs),
            session_id,
            AuditAction::Revoke,
            SessionState::Revoked,
            "explicit",
        )
    }

    /// 一键吊销全部（C5.1）。
    pub fn revoke_all(&self, who: &str, now_unix_secs: i64) -> Result<usize, BrokerError> {
        let mut records = self.lock()?;
        let ids: Vec<[u8; 32]> = records
            .iter()
            .filter(|(_, r)| r.state == SessionState::Active)
            .map(|(mac, _)| *mac)
            .collect();
        if ids.is_empty() {
            return Ok(0);
        }
        let id_list: Vec<String> = ids
            .iter()
            .filter_map(|mac| records.get(mac).map(|r| r.session_id.clone()))
            .collect();
        let mut event = AuditEvent::new(
            who,
            when(now_unix_secs),
            AuditAction::RevokeAll,
            AuditResult::Ok,
        );
        event.detail.insert("count".into(), id_list.len().to_string());
        event.detail.insert("sessions".into(), id_list.join(","));
        self.audit_or_fail(event)?;
        for mac in &ids {
            if let Some(record) = records.get_mut(mac) {
                record.state = SessionState::Revoked;
                record.real = None;
            }
        }
        Ok(ids.len())
    }

    /// 清扫（C2.2）：TTL 到期或空闲超时的活动会话置为释放，先到先生效；返回被释放的 id。
    pub fn sweep(&self, who: &str, now_unix_secs: i64) -> Result<Vec<String>, BrokerError> {
        let mut records = self.lock()?;
        let expired: Vec<[u8; 32]> = records
            .iter()
            .filter(|(_, r)| {
                r.state == SessionState::Active
                    && (now_unix_secs >= r.expires_secs || now_unix_secs >= r.idle_deadline_secs)
            })
            .map(|(mac, _)| *mac)
            .collect();
        let mut released = Vec::new();
        for mac in expired {
            let reason = {
                let record = records.get(&mac).ok_or(BrokerError::Unavailable)?;
                if now_unix_secs >= record.expires_secs { "ttl" } else { "idle" }
            };
            let real = {
                let record = records.get(&mac).ok_or(BrokerError::Unavailable)?;
                match record.real.as_ref() {
                    Some(real) => real.clone(),
                    None => {
                        return Err(self.deny(
                            who,
                            when(now_unix_secs),
                            AuditAction::Sweep,
                            BrokerError::Unavailable,
                            None,
                        ))
                    }
                }
            };
            let session_id = {
                let record = records.get(&mac).ok_or(BrokerError::Unavailable)?;
                record.session_id.clone()
            };
            let mut event = AuditEvent::from_envelope(
                who,
                when(now_unix_secs),
                AuditAction::Sweep,
                AuditResult::Ok,
                &real,
            );
            event.detail.insert("session_id".into(), session_id.clone());
            event.detail.insert("reason".into(), reason.into());
            self.audit_or_fail(event)?;
            let record = records.get_mut(&mac).ok_or(BrokerError::Unavailable)?;
            record.state = SessionState::Released;
            record.real = None;
            released.push(session_id);
        }
        Ok(released)
    }

    /// 代持通道的使用授权（C4）：句柄认证 + scope 裁决，单次会话在此消费。
    pub fn authorize(
        &self,
        who: &str,
        request: SessionUse<'_>,
        now_unix_secs: i64,
    ) -> Result<UsePermit, BrokerError> {
        let when = when(now_unix_secs);
        let handle_bytes = match decode_hex(request.handle) {
            Some(bytes) if bytes.len() == 32 => bytes,
            _ => return Err(self.deny(who, when, AuditAction::Use, BrokerError::BadHandle, None)),
        };
        let handle_mac = hmac_sign(&*self.mac_key, &handle_bytes)?;
        let mut records = self.lock()?;
        let record = match records.get_mut(&handle_mac) {
            Some(record) => record,
            None => {
                return Err(self.deny(
                    who,
                    when,
                    AuditAction::Use,
                    BrokerError::UnknownSession,
                    None,
                ))
            }
        };
        if record.state != SessionState::Active {
            let err = state_error(record.state);
            return Err(self.deny(who, when, AuditAction::Use, err, record.real.as_ref()));
        }
        let real = match record.real.as_ref() {
            Some(real) => real.clone(),
            None => {
                return Err(self.deny(
                    who,
                    when,
                    AuditAction::Use,
                    BrokerError::Unavailable,
                    None,
                ))
            }
        };

        if now_unix_secs >= record.expires_secs || now_unix_secs >= record.idle_deadline_secs {
            let err = BrokerError::Expired;
            return Err(self.deny(who, when, AuditAction::Use, err, Some(&real)));
        }
        if record.target.kind == TargetKind::Web {
            match request.domain {
                Some(domain) if domain_covered(&record.scope.domain_allowlist, domain) => {}
                _ => {
                    let err = BrokerError::DomainNotAllowed;
                    return Err(self.deny(who, when, AuditAction::Use, err, Some(&real)));
                }
            }
        }
        if record.scope.read_only && request.action == SessionAction::Write {
            let err = BrokerError::ReadOnlyViolation;
            return Err(self.deny(who, when, AuditAction::Use, err, Some(&real)));
        }

        let session_id = record.session_id.clone();
        let single_use = record.scope.single_use;
        let mut event =
            AuditEvent::from_envelope(who, when, AuditAction::Use, AuditResult::Ok, &real);
        event.detail.insert("session_id".into(), session_id.clone());
        if let Some(domain) = request.domain {
            event.detail.insert("domain".into(), domain.to_string());
        }
        self.audit_or_fail(event)?;

        if single_use {
            record.state = SessionState::Consumed;
            record.real = None;
        } else {
            record.idle_deadline_secs = now_unix_secs + IDLE_TIMEOUT_SECS as i64;
        }
        Ok(UsePermit {
            session_id,
            envelope: real,
        })
    }


    /// Scheme A：AI Agent 零明文受控代理请求（核验句柄 + 自动注入 Cookie/Bearer + 剥离响应 `Set-Cookie`）。
    pub fn execute_agent_request(
        &self,
        who: &str,
        handle: &str,
        request: AgentHttpRequest,
        executor: &dyn AgentHttpExecutor,
        now_unix_secs: i64,
    ) -> Result<AgentHttpResponse, BrokerError> {
        let method_upper = request.method.trim().to_ascii_uppercase();
        let action = if matches!(method_upper.as_str(), "GET" | "HEAD" | "OPTIONS") {
            SessionAction::Read
        } else {
            SessionAction::Write
        };
        let host = extract_url_host(&request.url);
        let permit = self.authorize(
            who,
            SessionUse {
                handle,
                domain: host.as_deref(),
                action,
            },
            now_unix_secs,
        )?;
        let mut authed = request;
        if !permit.envelope.payload.cookies.is_empty() {
            let cookie_header: String = permit
                .envelope
                .payload
                .cookies
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; ");
            authed.headers.insert("Cookie".to_string(), cookie_header);
        }
        if let Some(token) = &permit.envelope.payload.token {
            authed
                .headers
                .insert("Authorization".to_string(), format!("Bearer {token}"));
        }
        let mut resp = executor
            .execute(&authed)
            .map_err(|_| BrokerError::Unavailable)?;
        resp.headers
            .retain(|k, _| !k.eq_ignore_ascii_case("set-cookie") && !k.eq_ignore_ascii_case("set-cookie2"));
        Ok(resp)
    }

    /// 跨端注入信封租约：核验句柄并将所有 Cookie 的 `expires_at` 夹紧至不超过临时会话到期时刻。
    pub fn lease_surface_envelope(
        &self,
        who: &str,
        handle: &str,
        domain: Option<&str>,
        action: SessionAction,
        now_unix_secs: i64,
    ) -> Result<SessionEnvelope, BrokerError> {
        let permit = self.authorize(
            who,
            SessionUse {
                handle,
                domain,
                action,
            },
            now_unix_secs,
        )?;
        let records = self.lock()?;
        let lease_expires_at = find_by_id(&records, &permit.session_id)
            .map(|r| (r.expires_secs, r.expires_at.clone()));
        drop(records);

        let mut env = permit.envelope;
        if let Some((lease_secs, lease_str)) = lease_expires_at {
            env.expires_at = Some(lease_str.clone());
            for cookie in &mut env.payload.cookies {
                let clamp = match &cookie.expires_at {
                    None => true,
                    Some(existing) => ab_contract::time::parse_rfc3339_secs(existing)
                        .map(|secs| secs > lease_secs)
                        .unwrap_or(true),
                };
                if clamp {
                    cookie.expires_at = Some(lease_str.clone());
                }
            }
        }
        Ok(env)
    }

    /// 总览（A2.2）：只读视图，不改状态、不入账。
    pub fn list(&self, now_unix_secs: i64) -> Result<Vec<SessionStatus>, BrokerError> {
        let records = self.lock()?;
        Ok(records.values().map(|r| r.status(now_unix_secs)).collect())
    }

    /// release / revoke 共用的终态迁移：先写账后落表，写账失败不迁移。
    fn finish(
        &self,
        who: &str,
        when: String,
        session_id: &str,
        action: AuditAction,
        state: SessionState,
        reason: &str,
    ) -> Result<(), BrokerError> {
        let mut records = self.lock()?;
        let record = match find_by_id(&records, session_id) {
            Some(record) => record,
            None => {
                return Err(self.deny(
                    who,
                    when,
                    action,
                    BrokerError::UnknownSession,
                    None,
                ))
            }
        };
        if record.state != SessionState::Active {
            let err = state_error(record.state);
            return Err(self.deny(who, when, action, err, record.real.as_ref()));
        }
        let real = match record.real.as_ref() {
            Some(real) => real.clone(),
            None => {
                return Err(self.deny(who, when, action, BrokerError::Unavailable, None))
            }
        };
        let mut event = AuditEvent::from_envelope(who, when, action, AuditResult::Ok, &real);
        event.detail.insert("session_id".into(), session_id.to_string());
        event.detail.insert("reason".into(), reason.into());
        self.audit_or_fail(event)?;
        let record = find_by_id_mut(&mut records, session_id).ok_or(BrokerError::Unavailable)?;
        record.state = state;
        record.real = None;
        Ok(())
    }
}


fn extract_url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or("").trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

fn when(now_unix_secs: i64) -> String {
    format_rfc3339(now_unix_secs)
}

fn state_error(state: SessionState) -> BrokerError {
    if state == SessionState::Consumed {
        BrokerError::SingleUseConsumed
    } else {
        BrokerError::NotActive
    }
}

fn check_ttl(ttl_secs: Option<u64>) -> Result<u64, BrokerError> {
    let ttl = ttl_secs.unwrap_or(DEFAULT_TTL_SECS);
    if ttl == 0 || ttl > MAX_TTL_SECS {
        return Err(BrokerError::InvalidTtl);
    }
    Ok(ttl)
}

fn check_scope(scope: &ScopeSpec) -> Result<(), BrokerError> {
    if scope
        .domain_allowlist
        .iter()
        .any(|entry| entry.trim().trim_matches('.').is_empty())
    {
        return Err(BrokerError::InvalidScope);
    }
    Ok(())
}

/// 域名覆盖：精确相等或子域（`a.b.example.com` 被 `example.com` 覆盖）。
fn domain_covered(allowlist: &[String], domain: &str) -> bool {
    let d = normalize_domain(domain);
    if d.is_empty() {
        return false;
    }
    allowlist.iter().any(|entry| subdomain_covers(entry, &d))
}

/// 规范化域名：去空白、去首尾点、小写。
fn normalize_domain(domain: &str) -> String {
    domain
        .trim()
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// 白名单项 `entry` 是否合法覆盖目标域 `domain`（相等或目标是其子域）。
fn subdomain_covers(entry: &str, domain: &str) -> bool {
    let e = normalize_domain(entry);
    !e.is_empty() && (domain == e || domain.ends_with(&format!(".{e}")))
}

/// 白名单项是否为 `domain` 的祖先超集（如 `com` 对 `github.com`）。
/// C4.1 fail-closed：AI 请求的白名单只允许收窄（目标域或其子域），
/// 不允许把范围放大到目标的父域/公共后缀——否则 scope 形同虚设。
fn allowlist_narrows_target(allowlist: &[String], domain: &str) -> bool {
    let d = normalize_domain(domain);
    if d.is_empty() {
        return false;
    }
    // 收窄 = entry 是目标本身，或 entry 是目标的子域（entry 在目标之下）。
    // entry 是目标的父域/后缀（目标在 entry 之下）即为超集，拒绝。
    allowlist.iter().all(|entry| {
        let e = normalize_domain(entry);
        e == d || (e.ends_with(&format!(".{d}")) && !d.ends_with(&format!(".{e}")))
    })
}

fn scope_summary(scope: &ScopeSpec) -> String {
    format!(
        "domains={}; read_only={}; single_use={}",
        scope.domain_allowlist.join(","),
        scope.read_only,
        scope.single_use
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

fn find_by_id<'m>(
    records: &'m BTreeMap<[u8; 32], GrantRecord>,
    session_id: &str,
) -> Option<&'m GrantRecord> {
    records.values().find(|r| r.session_id == session_id)
}

fn find_by_id_mut<'m>(
    records: &'m mut BTreeMap<[u8; 32], GrantRecord>,
    session_id: &str,
) -> Option<&'m mut GrantRecord> {
    records.values_mut().find(|r| r.session_id == session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_audit::{AuditFilter, InMemoryAuditSink};
    use ab_crypto::OsEntropy;

    const NOW: i64 = 1_800_000_000;
    const MASTER: [u8; 32] = [7u8; 32];

    struct FailSink;
    impl AuditSink for FailSink {
        fn record_or_fail(&self, _event: AuditEvent) -> Result<(), AuditError> {
            Err(AuditError::Unavailable)
        }
        fn query(&self, _filter: &AuditFilter) -> Result<Vec<AuditEvent>, AuditError> {
            Err(AuditError::Unavailable)
        }
        fn count(&self) -> Result<usize, AuditError> {
            Err(AuditError::Unavailable)
        }
    }

    fn web_envelope() -> SessionEnvelope {
        SessionEnvelope {
            schema_version: ab_contract::SCHEMA_VERSION.to_string(),
            account_ref: "acc-1".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some("github.com".into()),
                app_id: None,
            },
            kind: ab_contract::SessionKind::Cookie,
            payload: {
                let mut payload = ab_contract::Payload::default();
                payload.cookies.push(ab_contract::Cookie {
                    name: "sid".into(),
                    value: "secret-cookie".into(),
                    domain: Some("github.com".into()),
                    path: Some("/".into()),
                    secure: Some(true),
                    http_only: Some(true),
                    same_site: Some(ab_contract::SameSite::Lax),
                    expires_at: None,
                });
                payload
            },
            issued_at: "2026-10-09T00:00:00Z".into(),
            expires_at: None,
            source: Source {
                source_type: ab_contract::SourceType::Extension,
                id: "chromium".into(),
            },
            scope: None,
            client_env: None,
        }
    }

    fn scope(domains: &[&str], read_only: bool, single_use: bool) -> ScopeSpec {
        ScopeSpec {
            domain_allowlist: domains.iter().map(|d| d.to_string()).collect(),
            read_only,
            single_use,
        }
    }

    fn issue_one<'a>(
        broker: &SessionBroker,
        scope: ScopeSpec,
        ttl_secs: Option<u64>,
    ) -> TemporaryGrant {
        broker
            .issue(
                "ai-client",
                &web_envelope(),
                SessionIssue { scope, ttl_secs },
                &OsEntropy,
                NOW,
            )
            .expect("签发失败")
    }

    #[test]
    fn issue_grants_scoped_handle_and_audits_without_secrets() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), None);

        assert_eq!(grant.account_ref, "acc-1");
        assert_eq!(grant.expires_at, format_rfc3339(NOW + DEFAULT_TTL_SECS as i64));
        assert_eq!(grant.handle.len(), 64, "句柄 = 32 随机字节的 hex");

        let events = sink.query(&AuditFilter::default()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, AuditAction::Issue);
        assert_eq!(events[0].result, AuditResult::Ok);
        assert_eq!(events[0].account_ref.as_deref(), Some("acc-1"));
        assert_eq!(events[0].domain.as_deref(), Some("github.com"));
        assert_eq!(events[0].detail.get("session_id").unwrap(), &grant.session_id);
        let audit_text = serde_json::to_string(&events).unwrap();
        assert!(!audit_text.contains(&grant.handle), "审计不得出现句柄");
        assert!(!audit_text.contains("secret-cookie"), "审计不得出现秘密");

        let debug = format!("{grant:?}");
        assert!(debug.contains(REDACTED) && !debug.contains(&grant.handle));
    }

    #[test]
    fn issue_rejects_target_outside_scope_and_audits_denied() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let err = broker
            .issue(
                "ai-client",
                &web_envelope(),
                SessionIssue {
                    scope: scope(&["example.com"], false, false),
                    ttl_secs: None,
                },
                &OsEntropy,
                NOW,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::TargetNotInScope);

        let events = sink.query(&AuditFilter::default()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].result, AuditResult::Denied);
        assert_eq!(events[0].detail.get("reason").unwrap(), "target_not_in_scope");
    }

    #[test]
    fn issue_rejects_wider_allowlist_than_target() {
        // P1-3：白名单不得比目标（github.com）更宽。
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        for domains in [
            vec!["com"],
            vec!["github.com", "com"],
            vec!["hub.com"],
            vec!["github.com.evil.test"],
        ] {
            let err = broker
                .issue(
                    "ai-client",
                    &web_envelope(),
                    SessionIssue {
                        scope: scope(&domains, false, false),
                        ttl_secs: None,
                    },
                    &OsEntropy,
                    NOW,
                )
                .unwrap_err();
            assert_eq!(err, BrokerError::TargetNotInScope, "白名单 {domains:?} 必须被拒");
        }

        // 合法收窄仍通过：目标本身或其子域。
        for domains in [
            vec!["github.com"],
            vec!["github.com", "api.github.com"],
        ] {
            assert!(
                broker
                    .issue(
                        "ai-client",
                        &web_envelope(),
                        SessionIssue {
                            scope: scope(&domains, false, false),
                            ttl_secs: None,
                        },
                        &OsEntropy,
                        NOW,
                    )
                    .is_ok(),
                "合法白名单 {domains:?} 必须通过"
            );
        }
    }

    #[test]
    fn issue_rejects_bad_ttl_empty_allowlist_and_bad_scope() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let real = web_envelope();

        for (scope, ttl_secs, want) in [
            (scope(&["github.com"], false, false), Some(0), BrokerError::InvalidTtl),
            (
                scope(&["github.com"], false, false),
                Some(MAX_TTL_SECS + 1),
                BrokerError::InvalidTtl,
            ),
            (scope(&[], false, false), None, BrokerError::TargetNotInScope),
            (scope(&[""], false, false), None, BrokerError::InvalidScope),
            (scope(&["..."], false, false), None, BrokerError::InvalidScope),
        ] {
            let err = broker
                .issue(
                    "ai-client",
                    &real,
                    SessionIssue { scope, ttl_secs },
                    &OsEntropy,
                    NOW,
                )
                .unwrap_err();
            assert_eq!(err, want);
        }
    }

    #[test]
    fn authorize_enforces_domain_allowlist_including_subdomains() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), None);

        let permit = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("api.github.com"),
                    action: SessionAction::Read,
                },
                NOW + 10,
            )
            .unwrap();
        assert_eq!(permit.session_id, grant.session_id);
        assert_eq!(permit.envelope.account_ref, "acc-1");

        let err = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("evil.com"),
                    action: SessionAction::Read,
                },
                NOW + 20,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::DomainNotAllowed);
    }

    #[test]
    fn read_only_scope_blocks_writes() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], true, false), None);

        broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Read,
                },
                NOW,
            )
            .unwrap();
        let err = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Write,
                },
                NOW,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::ReadOnlyViolation);
    }

    #[test]
    fn single_use_grant_is_consumed_after_one_authorize() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, true), None);

        broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Read,
                },
                NOW,
            )
            .unwrap();
        let err = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Read,
                },
                NOW + 1,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::SingleUseConsumed);
    }

    #[test]
    fn sweep_releases_idle_and_ttl_expired_first_trigger_wins() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let idle_grant = issue_one(&broker, scope(&["github.com"], false, false), Some(MAX_TTL_SECS));
        let ttl_grant = issue_one(&broker, scope(&["github.com"], false, false), Some(60));

        // 两重触发先到先生效：TTL 60s 先于空闲 30min。
        assert!(broker.sweep("system", NOW + 59).unwrap().is_empty());
        let released = broker.sweep("system", NOW + 60).unwrap();
        assert_eq!(released, vec![ttl_grant.session_id.clone()]);

        assert!(broker.sweep("system", NOW + IDLE_TIMEOUT_SECS as i64 - 1).unwrap().is_empty());
        let released = broker.sweep("system", NOW + IDLE_TIMEOUT_SECS as i64).unwrap();
        assert_eq!(released, vec![idle_grant.session_id.clone()]);

        let statuses = broker.list(NOW + 100_000).unwrap();
        assert!(statuses.iter().all(|s| s.state == SessionState::Released));

        let err = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &idle_grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Read,
                },
                NOW + 200_000,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::NotActive);
    }

    #[test]
    fn authorize_on_expired_grant_fails_even_before_sweep() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), Some(60));
        let err = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Read,
                },
                NOW + 61,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::Expired);
    }

    #[test]
    fn renew_extends_within_absolute_cap_only() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), Some(60));

        let status = broker.renew("ai-client", &grant.session_id, Some(120), NOW + 10).unwrap();
        assert_eq!(status.expires_at, format_rfc3339(NOW + 10 + 120));

        // 续期不得超过签发 + 24h 的绝对上限。
        let err = broker
            .renew("ai-client", &grant.session_id, Some(MAX_TTL_SECS), NOW + 10)
            .unwrap_err();
        assert_eq!(err, BrokerError::InvalidTtl);
    }

    #[test]
    fn release_drops_real_copy_and_keeps_handle_dead() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), None);

        broker.release("ai-client", &grant.session_id, NOW + 5).unwrap();
        let statuses = broker.list(NOW + 6).unwrap();
        assert_eq!(statuses[0].state, SessionState::Released);

        let err = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Read,
                },
                NOW + 7,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::NotActive);

        let actions: Vec<AuditAction> = sink
            .query(&AuditFilter::default())
            .unwrap()
            .iter()
            .filter(|e| e.result == AuditResult::Ok)
            .map(|e| e.action)
            .collect();
        assert_eq!(actions, vec![AuditAction::Issue, AuditAction::Release]);
    }

    #[test]
    fn revoke_and_revoke_all_audit_with_vocabulary() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let a = issue_one(&broker, scope(&["github.com"], false, false), None);
        let b = issue_one(&broker, scope(&["github.com"], false, false), None);

        broker.revoke("owner", &a.session_id, NOW + 1).unwrap();
        assert_eq!(broker.revoke_all("owner", NOW + 2).unwrap(), 1);
        assert_eq!(broker.revoke_all("owner", NOW + 3).unwrap(), 0);

        let events = sink.query(&AuditFilter::default()).unwrap();
        let tail: Vec<AuditAction> = events.iter().skip(2).map(|e| e.action).collect();
        assert_eq!(tail, vec![AuditAction::Revoke, AuditAction::RevokeAll]);
        assert_eq!(events[3].detail.get("count").unwrap(), "1");
        assert_eq!(events[3].detail.get("sessions").unwrap(), &b.session_id);
    }

    #[test]
    fn table_stores_only_hashed_handles() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), None);

        let handle_bytes = decode_hex(&grant.handle).unwrap();
        let mac = hmac_sign(&*broker.mac_key, &handle_bytes).unwrap();
        let records = broker.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        let (key, record) = records.iter().next().unwrap();
        assert_eq!(*key, mac, "查表键 = HMAC(域密钥, 句柄)");
        assert_ne!(&key[..], &handle_bytes[..], "表内不得出现句柄原文");
        assert_eq!(record.session_id, grant.session_id);
    }

    #[test]
    fn audit_failure_blocks_issue_without_leaving_state() {
        let sink = Arc::new(FailSink);
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let err = broker
            .issue(
                "ai-client",
                &web_envelope(),
                SessionIssue {
                    scope: scope(&["github.com"], false, false),
                    ttl_secs: None,
                },
                &OsEntropy,
                NOW,
            )
            .unwrap_err();
        assert_eq!(err, BrokerError::Audit(AuditError::Unavailable));
        assert!(broker.list(NOW).unwrap().is_empty(), "写账失败不得留痕");
    }

    #[test]
    fn unknown_session_and_bad_handle_denied_and_audited() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();

        assert_eq!(
            broker.release("ai-client", "nope", NOW).unwrap_err(),
            BrokerError::UnknownSession
        );
        assert_eq!(
            broker
                .authorize(
                    "ai-client",
                    SessionUse {
                        handle: "zz",
                        domain: None,
                        action: SessionAction::Read,
                    },
                    NOW,
                )
                .unwrap_err(),
            BrokerError::BadHandle
        );

        let events = sink.query(&AuditFilter::default()).unwrap();
        assert!(events.iter().all(|e| e.result == AuditResult::Denied));
    }

    #[test]
    fn debug_output_never_contains_handle() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), None);
        let permit = broker
            .authorize(
                "ai-client",
                SessionUse {
                    handle: &grant.handle,
                    domain: Some("github.com"),
                    action: SessionAction::Read,
                },
                NOW,
            )
            .unwrap();
        assert!(!format!("{permit:?}").contains("secret-cookie"));
    }

    struct MockHttpExecutor;
    impl AgentHttpExecutor for MockHttpExecutor {
        fn execute(&self, req: &AgentHttpRequest) -> Result<AgentHttpResponse, String> {
            assert_eq!(req.headers.get("Cookie").map(String::as_str), Some("sid=secret-cookie"));
            let mut headers = BTreeMap::new();
            headers.insert("Content-Type".into(), "application/json".into());
            headers.insert("Set-Cookie".into(), "sid=rotated-secret".into());
            Ok(AgentHttpResponse {
                status: 200,
                headers,
                body: b"{\"ok\":true}".to_vec(),
            })
        }
    }

    #[test]
    fn scheme_a_zero_plaintext_proxy_and_cookie_ttl_clamping() {
        let sink = Arc::new(InMemoryAuditSink::new());
        let broker = SessionBroker::new(sink.clone(), &MASTER).unwrap();
        let grant = issue_one(&broker, scope(&["github.com"], false, false), Some(120));

        let resp = broker
            .execute_agent_request(
                "ai-client",
                &grant.handle,
                AgentHttpRequest {
                    method: "GET".into(),
                    url: "https://api.github.com/user".into(),
                    headers: BTreeMap::new(),
                    body: Vec::new(),
                },
                &MockHttpExecutor,
                NOW + 10,
            )
            .unwrap();
        assert_eq!(resp.status, 200);
        assert!(!resp.headers.contains_key("Set-Cookie"), "Set-Cookie 必须被剥离");

        let leased = broker
            .lease_surface_envelope(
                "ai-client",
                &grant.handle,
                Some("github.com"),
                SessionAction::Read,
                NOW + 20,
            )
            .unwrap();
        assert_eq!(leased.expires_at.as_deref(), Some(grant.expires_at.as_str()));
        assert_eq!(
            leased.payload.cookies[0].expires_at.as_deref(),
            Some(grant.expires_at.as_str()),
            "注入侧 Cookie 到期时间必须夹紧至临时会话到期时刻"
        );
    }
}
