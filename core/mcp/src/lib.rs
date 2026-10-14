//! `ab-mcp` —— 本地 MCP server（C1）：stdio + 仅本机回环 Streamable HTTP。
//!
//! 职责（`docs/架构与契约.md` §2）：MCP 协议 + 工具面；**零业务规则**，五个工具逐一映射
//! `ab-broker`（签发/续期/释放/总览/吊销）与 `ab-state`（`acquire_session` 的真会话查询）。
//!
//! 核心承诺（`docs/流程/AI端流程.md`）：AI 永远拿不到真实凭据——`acquire_session` 返回的
//! 是 ab-broker 签发的不透明句柄；真实会话由 broker 代持；一切动作由 broker 入审计（fail-closed）。
//!
//! 明确不做：业务判断（一律转 broker）、网络监听（Streamable HTTP 只绑 127.0.0.1）、
//! 浏览器/文件 IO。
#![forbid(unsafe_code)]

use std::io;
use std::sync::Arc;

use ab_broker::{BrokerError, BrokerService, GrantStatus, IssueRequest};
use ab_contract::time::format_rfc3339_secs;
use ab_contract::{ScopeSpec, SessionEnvelope, Target, TargetKind, SCHEMA_VERSION};
use ab_crypto::Entropy;
use ab_state::{Clock, SessionEntry, SessionSource, StateError, StateService, StateStore};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::schemars;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 会话查询注入面：`ab-state` 的 [`StateService`] 经此成为工具面的真会话来源。
/// 对象安全（Streamable HTTP 每会话一个服务实例，工厂需 `'static`）。
pub trait SessionLookup: Send + Sync {
    fn sessions_for_target(&self, target: &Target) -> Result<Vec<SessionEntry>, StateError>;
}

impl<S: StateStore, C: Clock, R: SessionSource> SessionLookup for StateService<S, C, R> {
    fn sessions_for_target(&self, target: &Target) -> Result<Vec<SessionEntry>, StateError> {
        StateService::sessions_for_target(self, target)
    }
}

/// MCP 错误：只透出定位信息，绝不携带会话/凭据内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    Broker(BrokerError),
    State(StateError),
    /// 目标没有可用真会话。
    NoSession,
    /// 同站多账号并存且未指定 `account_ref`。
    AccountRequired(Vec<String>),
    /// 工具入参非法（target.kind / 字段缺失等）。
    InvalidParams(String),
    Io(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Broker(e) => write!(f, "{e}"),
            McpError::State(e) => write!(f, "会话状态错误: {e:?}"),
            McpError::NoSession => write!(f, "目标没有可用会话"),
            McpError::AccountRequired(accounts) => write!(
                f,
                "同站多账号并存，请指定 account_ref（候选: {}）",
                accounts.join(", ")
            ),
            McpError::InvalidParams(msg) => write!(f, "入参非法: {msg}"),
            McpError::Io(msg) => write!(f, "传输失败: {msg}"),
        }
    }
}

impl std::error::Error for McpError {}

impl From<BrokerError> for McpError {
    fn from(e: BrokerError) -> Self {
        McpError::Broker(e)
    }
}

impl From<StateError> for McpError {
    fn from(e: StateError) -> Self {
        McpError::State(e)
    }
}

impl From<McpError> for rmcp::ErrorData {
    fn from(e: McpError) -> Self {
        match &e {
            McpError::InvalidParams(_) | McpError::AccountRequired(_) | McpError::NoSession => {
                rmcp::ErrorData::invalid_params(e.to_string(), None)
            }
            _ => rmcp::ErrorData::internal_error(e.to_string(), None),
        }
    }
}

/* ------------------------- 工具面 DTO（线格式，非契约类型） ------------------------- */

fn map_kind(raw: &str) -> Result<TargetKind, McpError> {
    match raw {
        "web" => Ok(TargetKind::Web),
        "app" => Ok(TargetKind::App),
        "container" => Ok(TargetKind::Container),
        other => Err(McpError::InvalidParams(format!(
            "target.kind 非法: {other}"
        ))),
    }
}

fn kind_str(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Web => "web",
        TargetKind::App => "app",
        TargetKind::Container => "container",
    }
}

/// 工具面目标（与契约 `Target` 同构；契约类型不依赖 schemars，故在此映射）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpTarget {
    /// "web" | "app" | "container"
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

impl McpTarget {
    fn to_contract(&self) -> Result<Target, McpError> {
        Ok(Target {
            kind: map_kind(&self.kind)?,
            domain: self.domain.clone(),
            app_id: self.app_id.clone(),
        })
    }

    fn from_contract(target: &Target) -> Self {
        McpTarget {
            kind: kind_str(target.kind).to_string(),
            domain: target.domain.clone(),
            app_id: target.app_id.clone(),
        }
    }
}

/// 工具面授权范围（与契约 `ScopeSpec` 同构）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpScope {
    /// 域名白名单；空数组 = 不允许任何 web 目标。
    pub domain_allowlist: Vec<String>,
    pub read_only: bool,
    pub single_use: bool,
}

impl McpScope {
    fn to_contract(&self) -> ScopeSpec {
        ScopeSpec {
            domain_allowlist: self.domain_allowlist.clone(),
            read_only: self.read_only,
            single_use: self.single_use,
        }
    }

    fn from_contract(scope: &ScopeSpec) -> Self {
        McpScope {
            domain_allowlist: scope.domain_allowlist.clone(),
            read_only: scope.read_only,
            single_use: scope.single_use,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AcquireParams {
    pub target: McpTarget,
    pub scope: McpScope,
    /// 缺省 1h；上限 24h（超限由 broker 拒绝）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
    /// 同站多账号并存时消歧；唯一匹配时可省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
}

/// 签发结果：`handle` 是代持通道的不透明临时凭证，仅此一次返回。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AcquireResult {
    pub session_id: String,
    pub handle: String,
    pub account_ref: String,
    pub target: McpTarget,
    pub scope: McpScope,
    pub issued_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionIdParams {
    pub session_id: String,
    /// 缺省 1h；续期不得超出签发 + 24h 绝对上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
}

/// 总览行（A2.2：剩余 TTL / scope / 来源）；不含句柄。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionSummary {
    pub session_id: String,
    pub account_ref: String,
    pub target: McpTarget,
    pub scope: McpScope,
    pub issued_at: String,
    pub expires_at: String,
    pub remaining_ttl_secs: u64,
    /// "active" | "released" | "revoked" | "consumed"
    pub state: String,
    pub who: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListResult {
    pub sessions: Vec<SessionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevokeAllResult {
    pub revoked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseResult {
    pub released: bool,
}

fn state_str(state: ab_broker::GrantState) -> &'static str {
    match state {
        ab_broker::GrantState::Active => "active",
        ab_broker::GrantState::Released => "released",
        ab_broker::GrantState::Revoked => "revoked",
        ab_broker::GrantState::Consumed => "consumed",
    }
}

fn summary(status: &GrantStatus) -> SessionSummary {
    SessionSummary {
        session_id: status.session_id.clone(),
        account_ref: status.account_ref.clone(),
        target: McpTarget::from_contract(&status.target),
        scope: McpScope::from_contract(&status.scope),
        issued_at: status.issued_at.clone(),
        expires_at: status.expires_at.clone(),
        remaining_ttl_secs: status.remaining_ttl_secs,
        state: state_str(status.state).to_string(),
        who: status.who.clone(),
    }
}

/// 真会话条目 → 契约信封（broker 代持的载体）；`issued_at` = 构造时刻。
fn to_envelope(entry: &SessionEntry, now_unix_secs: i64) -> SessionEnvelope {
    SessionEnvelope {
        schema_version: SCHEMA_VERSION.to_string(),
        account_ref: entry.record.account_ref.clone(),
        target: entry.record.target.clone(),
        kind: entry.record.kind,
        payload: entry.record.payload.clone(),
        issued_at: format_rfc3339_secs(now_unix_secs),
        expires_at: entry.record.expires_at.clone(),
        source: entry.record.source.clone(),
        scope: None,
        client_env: entry.record.client_env.clone(),
    }
}

/// 多账号消歧（A4.1）：指定 account_ref 精确取；唯一候选直接取；否则报候选清单。
fn pick_session(
    candidates: &[SessionEntry],
    account_ref: Option<&str>,
) -> Result<SessionEntry, McpError> {
    match (account_ref, candidates.len()) {
        (Some(want), _) => candidates
            .iter()
            .find(|entry| entry.record.account_ref == want)
            .cloned()
            .ok_or(McpError::NoSession),
        (None, 1) => Ok(candidates[0].clone()),
        (None, 0) => Err(McpError::NoSession),
        (None, _) => Err(McpError::AccountRequired(
            candidates
                .iter()
                .map(|entry| entry.record.account_ref.clone())
                .collect(),
        )),
    }
}

/* ------------------------------- MCP 服务与工具面 ------------------------------- */

/// MCP 服务：依赖全部注入（broker / 会话查询 / 时钟 / 熵源 + 调用方身份），无全局单例。
pub struct McpService {
    broker: Arc<BrokerService>,
    sessions: Arc<dyn SessionLookup>,
    clock: Arc<dyn Clock>,
    entropy: Arc<dyn Entropy>,
    /// 审计「谁」：每个 MCP 连接一个身份（宿主配置）。
    who: String,
}

impl McpService {
    pub fn new(
        broker: Arc<BrokerService>,
        sessions: Arc<dyn SessionLookup>,
        clock: Arc<dyn Clock>,
        entropy: Arc<dyn Entropy>,
        who: impl Into<String>,
    ) -> Self {
        Self {
            broker,
            sessions,
            clock,
            entropy,
            who: who.into(),
        }
    }

    /// `acquire_session` 的纯逻辑（C1.2 / C2.1）：真会话代持，返回不透明句柄。
    pub fn acquire_session(&self, params: &AcquireParams) -> Result<AcquireResult, McpError> {
        let target = params.target.to_contract()?;
        let candidates = self.sessions.sessions_for_target(&target)?;
        let picked = pick_session(&candidates, params.account_ref.as_deref())?;
        let now = self.clock.now_unix_secs();
        let envelope = to_envelope(&picked, now);
        let grant = self.broker.issue(
            &self.who,
            &envelope,
            IssueRequest {
                scope: params.scope.to_contract(),
                ttl_secs: params.ttl_secs,
            },
            &*self.entropy,
            now,
        )?;
        Ok(AcquireResult {
            session_id: grant.session_id,
            handle: grant.handle,
            account_ref: grant.account_ref,
            target: McpTarget::from_contract(&grant.target),
            scope: McpScope::from_contract(&grant.scope),
            issued_at: grant.issued_at,
            expires_at: grant.expires_at,
        })
    }

    /// `renew_session`（C2.2）：上限内续期。
    pub fn renew_session(&self, params: &SessionIdParams) -> Result<SessionSummary, McpError> {
        let now = self.clock.now_unix_secs();
        let status = self
            .broker
            .renew(&self.who, &params.session_id, params.ttl_secs, now)?;
        Ok(summary(&status))
    }

    /// `release_session`（C2.2）：显式释放；只失效我方凭证。
    pub fn release_session(&self, session_id: &str) -> Result<(), McpError> {
        self.broker
            .release(&self.who, session_id, self.clock.now_unix_secs())?;
        Ok(())
    }

    /// `list_sessions`（A2.2）：可见临时会话总览。
    pub fn list_sessions(&self) -> Result<ListResult, McpError> {
        let now = self.clock.now_unix_secs();
        Ok(ListResult {
            sessions: self.broker.list(now)?.iter().map(summary).collect(),
        })
    }

    /// `revoke_all`（C5.1）：一键吊销全部。
    pub fn revoke_all(&self) -> Result<RevokeAllResult, McpError> {
        let revoked = self
            .broker
            .revoke_all(&self.who, self.clock.now_unix_secs())?;
        Ok(RevokeAllResult { revoked })
    }
}

#[tool_router(server_handler)]
impl McpService {
    #[tool(
        name = "acquire_session",
        description = "签发临时会话（真会话由 broker 代持）：入参 target(web/app/container)+scope(域名白名单/只读/单次)+ttl_secs(默认 3600，上限 86400)，同站多账号并存时加 account_ref；返回一次性的不透明 handle（临时凭证）与到期时间"
    )]
    fn acquire_session_tool(
        &self,
        Parameters(params): Parameters<AcquireParams>,
    ) -> Result<Json<AcquireResult>, rmcp::ErrorData> {
        self.acquire_session(&params).map(Json).map_err(Into::into)
    }

    #[tool(
        name = "renew_session",
        description = "在绝对上限（签发 + 24h）内续期临时会话；返回剩余 TTL 与到期时间"
    )]
    fn renew_session_tool(
        &self,
        Parameters(params): Parameters<SessionIdParams>,
    ) -> Result<Json<SessionSummary>, rmcp::ErrorData> {
        self.renew_session(&params).map(Json).map_err(Into::into)
    }

    #[tool(
        name = "release_session",
        description = "显式释放临时会话：只失效我方凭证，默认不登出第三方"
    )]
    fn release_session_tool(
        &self,
        Parameters(params): Parameters<SessionIdParams>,
    ) -> Result<Json<ReleaseResult>, rmcp::ErrorData> {
        self.release_session(&params.session_id)?;
        Ok(Json(ReleaseResult { released: true }))
    }

    #[tool(
        name = "list_sessions",
        description = "可见临时会话总览：剩余 TTL / scope / 来源"
    )]
    fn list_sessions_tool(&self) -> Result<Json<ListResult>, rmcp::ErrorData> {
        self.list_sessions().map(Json).map_err(Into::into)
    }

    #[tool(name = "revoke_all", description = "一键吊销全部临时会话（C5.1）")]
    fn revoke_all_tool(&self) -> Result<Json<RevokeAllResult>, rmcp::ErrorData> {
        self.revoke_all().map(Json).map_err(Into::into)
    }
}

/* --------------------------------- 通道装配 --------------------------------- */

/// C1.1 stdio 通道：MCP 客户端以子进程方式启动本服务。
pub async fn serve_stdio(service: McpService) -> Result<(), McpError> {
    use rmcp::ServiceExt;
    let running = service
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| McpError::Io(e.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|e| McpError::Io(e.to_string()))
        .map(|_| ())
}

/// C1.1 Streamable HTTP，**仅本机回环**（127.0.0.1，端口由宿主给）；每会话一个服务实例。
pub async fn serve_http_loopback<F>(service_factory: F, port: u16) -> Result<(), McpError>
where
    F: Fn() -> Result<McpService, io::Error> + Send + Sync + 'static,
{
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    let service = StreamableHttpService::new(
        service_factory,
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().fallback_service(service);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| McpError::Io(e.to_string()))?;
    axum::serve(listener, router)
        .await
        .map_err(|e| McpError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_audit::{AuditFilter, AuditSink, InMemoryAuditSink};
    use ab_contract::{
        Cookie, SameSite, SessionKind, SessionRecord, Source, SourceType,
    };
    use ab_crypto::OsEntropy;
    use ab_state::{
        Endpoint, EndpointKind, HealthPolicy, InMemoryStateStore, SessionEntry, StateStore,
    };

    const MASTER: [u8; 32] = [7u8; 32];

    struct FixedClock(i64);
    impl Clock for FixedClock {
        fn now_unix_secs(&self) -> i64 {
            self.0
        }
    }

    struct NoSource;
    impl SessionSource for NoSource {
        fn read(&self, _endpoint: &Endpoint) -> Result<Vec<SessionRecord>, StateError> {
            Ok(Vec::new())
        }
    }

    fn session_entry(account_ref: &str, domain: &str) -> SessionEntry {
        SessionEntry::new(
            Endpoint::new(EndpointKind::Browser, "chromium"),
            SessionRecord {
                account_ref: account_ref.into(),
                target: Target {
                    kind: TargetKind::Web,
                    domain: Some(domain.into()),
                    app_id: None,
                },
                kind: SessionKind::Cookie,
                payload: {
                    let mut payload = ab_contract::Payload::default();
                    payload.cookies.push(Cookie {
                        name: "sid".into(),
                        value: "secret-cookie".into(),
                        domain: Some(domain.into()),
                        path: Some("/".into()),
                        secure: Some(true),
                        http_only: Some(true),
                        same_site: Some(SameSite::Lax),
                        expires_at: None,
                    });
                    payload
                },
                expires_at: None,
                source: Source {
                    source_type: SourceType::Extension,
                    id: "chromium".into(),
                },
                client_env: None,
            },
        )
    }

    struct Fixture {
        service: McpService,
        audit: Arc<InMemoryAuditSink>,
        at: i64,
    }

    fn fixture(entries: Vec<SessionEntry>, at: i64) -> Fixture {
        let audit = Arc::new(InMemoryAuditSink::new());
        let broker = Arc::new(BrokerService::new(audit.clone(), &MASTER).unwrap());
        let store = InMemoryStateStore::new();
        store.replace_sessions(entries).unwrap();
        let state = StateService::new(
            store,
            FixedClock(at),
            NoSource,
            HealthPolicy::default(),
        );
        let service = McpService::new(
            broker,
            Arc::new(state),
            Arc::new(FixedClock(at)),
            Arc::new(OsEntropy),
            "ai-client",
        );
        Fixture { service, audit, at }
    }

    fn acquire_params(domain: &str) -> AcquireParams {
        AcquireParams {
            target: McpTarget {
                kind: "web".into(),
                domain: Some(domain.into()),
                app_id: None,
            },
            scope: McpScope {
                domain_allowlist: vec![domain.into()],
                read_only: false,
                single_use: false,
            },
            ttl_secs: None,
            account_ref: None,
        }
    }

    #[test]
    fn acquire_session_issues_handle_and_audits() {
        let fx = fixture(vec![session_entry("acc-1", "github.com")], 1_800_000_000);
        let result = fx.service.acquire_session(&acquire_params("github.com")).unwrap();

        assert_eq!(result.account_ref, "acc-1");
        assert_eq!(result.handle.len(), 64, "不透明句柄 = 32 随机字节 hex");
        assert_eq!(result.target.domain.as_deref(), Some("github.com"));
        assert!(result.scope.domain_allowlist.contains(&"github.com".to_string()));

        let events = fx.audit.query(&AuditFilter::default()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, ab_audit::AuditAction::Issue);
        assert_eq!(events[0].who, "ai-client");
        assert!(!serde_json::to_string(&events).unwrap().contains(&result.handle));
    }

    #[test]
    fn acquire_requires_account_for_multi_account_targets() {
        let fx = fixture(
            vec![
                session_entry("acc-1", "github.com"),
                session_entry("acc-2", "github.com"),
            ],
            1_800_000_000,
        );
        let err = fx
            .service
            .acquire_session(&acquire_params("github.com"))
            .unwrap_err();
        match err {
            McpError::AccountRequired(accounts) => {
                assert_eq!(accounts, vec!["acc-1".to_string(), "acc-2".to_string()]);
            }
            other => panic!("期望 AccountRequired，得到 {other:?}"),
        }

        let mut params = acquire_params("github.com");
        params.account_ref = Some("acc-2".into());
        assert_eq!(
            fx.service.acquire_session(&params).unwrap().account_ref,
            "acc-2"
        );
    }

    #[test]
    fn acquire_without_matching_session_fails() {
        let fx = fixture(vec![session_entry("acc-1", "github.com")], 1_800_000_000);
        assert_eq!(
            fx.service.acquire_session(&acquire_params("example.com")).unwrap_err(),
            McpError::NoSession
        );
    }

    #[test]
    fn invalid_target_kind_rejected() {
        let fx = fixture(vec![], 1_800_000_000);
        let mut params = acquire_params("github.com");
        params.target.kind = "macos".into();
        assert!(matches!(
            fx.service.acquire_session(&params),
            Err(McpError::InvalidParams(_))
        ));
    }

    #[test]
    fn renew_release_list_revoke_all_lifecycle() {
        let fx = fixture(vec![session_entry("acc-1", "github.com")], 1_800_000_000);
        let grant = fx.service.acquire_session(&acquire_params("github.com")).unwrap();

        // 总览（A2.2）：剩余 TTL / scope / 来源，无句柄。
        let list = fx.service.list_sessions().unwrap();
        assert_eq!(list.sessions.len(), 1);
        let row = &list.sessions[0];
        assert_eq!(row.session_id, grant.session_id);
        assert_eq!(row.remaining_ttl_secs, 3600);
        assert_eq!(row.state, "active");
        let row_json = serde_json::to_string(row).unwrap();
        assert!(!row_json.contains(&grant.handle), "总览不得泄露句柄");

        // 续期（C2.2）。
        let renewed = fx
            .service
            .renew_session(&SessionIdParams {
                session_id: grant.session_id.clone(),
                ttl_secs: Some(600),
            })
            .unwrap();
        assert_eq!(renewed.expires_at, ab_contract::time::format_rfc3339_secs(fx.at + 600));

        // 释放（C2.2）→ 总览状态翻转。
        fx.service.release_session(&grant.session_id).unwrap();
        assert_eq!(fx.service.list_sessions().unwrap().sessions[0].state, "released");

        // 一键吊销（C5.1）只对活动会话计数。
        let second = fx
            .service
            .acquire_session(&acquire_params("github.com"))
            .unwrap();
        assert_eq!(fx.service.revoke_all().unwrap().revoked, 1);
        assert_eq!(fx.service.revoke_all().unwrap().revoked, 0);
        assert_eq!(second.session_id != grant.session_id, true);
    }

    #[test]
    fn tool_router_exposes_exactly_the_documented_tools() {
        let fx = fixture(vec![], 1_800_000_000);
        let router = McpService::tool_router();
        let mut names: Vec<String> = router.list_all().iter().map(|t| t.name.to_string()).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "acquire_session".to_string(),
                "list_sessions".to_string(),
                "release_session".to_string(),
                "renew_session".to_string(),
                "revoke_all".to_string(),
            ],
            "MCP 工具面 = 功能点全景 §4 的五个工具，不多不少"
        );
        let _ = &fx;
    }
}
