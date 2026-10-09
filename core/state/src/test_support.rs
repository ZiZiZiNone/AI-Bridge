//! 单测共用夹具（仅 `cfg(test)` 编译）：固定时钟、静态来源、构造器与时间常量。
//!
//! 时间常量的纪元秒由 Python 3.11 独立算出（见交付说明的实跑校验），用于把
//! 「到期倒计时 / 分级边界」写成确定性断言，不依赖真实时间。

use ab_contract::{
    Cookie, Payload, SameSite, SessionKind, SessionRecord, Source, SourceType, Target, TargetKind,
};

use crate::clock::Clock;
use crate::error::StateError;
use crate::health::HealthPolicy;
use crate::model::{canonicalize, Endpoint, EndpointKind, SessionEntry};
use crate::source::SessionSource;
use crate::store::{InMemoryStateStore, StateStore};
use crate::StateService;

/// 基准时刻：`2026-10-09T00:00:00Z`。
pub(crate) const BASE: i64 = 1_791_504_000;
/// 基准时刻字面量。
pub(crate) const T_BASE: &str = "2026-10-09T00:00:00Z";
/// 基准 +101 秒（告警窗口外侧）。
pub(crate) const T_PLUS_101: &str = "2026-10-09T00:01:41Z";
/// 基准 +100 秒（告警窗口边界上，含）。
pub(crate) const T_PLUS_100: &str = "2026-10-09T00:01:40Z";
/// 基准 +1 秒。
pub(crate) const T_PLUS_1: &str = "2026-10-09T00:00:01Z";
/// 基准 -1 秒。
pub(crate) const T_MINUS_1: &str = "2026-10-08T23:59:59Z";
/// 基准 +1 天。
pub(crate) const T_PLUS_DAY: &str = "2026-10-10T00:00:00Z";
/// [`T_PLUS_DAY`] 的纪元秒。
pub(crate) const T_PLUS_DAY_SECS: i64 = 1_791_590_400;

/// 固定时钟。
#[derive(Debug, Clone, Copy)]
pub(crate) struct FixedClock {
    pub(crate) now: i64,
}

impl FixedClock {
    pub(crate) fn at(now: i64) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now_unix_secs(&self) -> i64 {
        self.now
    }
}

/// 静态会话来源：`read` 恒返回预置记录（与端无关）。
#[derive(Debug, Clone)]
pub(crate) struct StaticSource {
    pub(crate) records: Vec<SessionRecord>,
}

impl StaticSource {
    pub(crate) fn with(records: Vec<SessionRecord>) -> Self {
        Self { records }
    }
}

impl SessionSource for StaticSource {
    fn read(&self, _endpoint: &Endpoint) -> Result<Vec<SessionRecord>, StateError> {
        Ok(self.records.clone())
    }
}

/// 测试用服务类型。
pub(crate) type TestService = StateService<InMemoryStateStore, FixedClock, StaticSource>;

/// 预置会话的内存存储（写前按身份规范化，与 service 写入行为一致）。
pub(crate) fn store_with(entries: Vec<SessionEntry>) -> InMemoryStateStore {
    let store = InMemoryStateStore::new();
    store
        .replace_sessions(canonicalize(entries))
        .expect("夹具写入失败");
    store
}

/// 组装测试服务。
pub(crate) fn service(
    store: InMemoryStateStore,
    records: Vec<SessionRecord>,
    now: i64,
    health_policy: HealthPolicy,
) -> TestService {
    StateService::new(
        store,
        FixedClock::at(now),
        StaticSource::with(records),
        health_policy,
    )
}

/// 单条 cookie：属性齐全（契约「会话保真度」要求）。
pub(crate) fn cookie() -> Cookie {
    Cookie {
        name: "sid".to_string(),
        value: "v".to_string(),
        domain: Some("github.com".to_string()),
        path: Some("/".to_string()),
        secure: Some(true),
        http_only: Some(true),
        same_site: Some(SameSite::Lax),
        expires_at: Some(T_PLUS_DAY.to_string()),
    }
}

/// 合法 web 会话记录（来源固定 extension/chromium）。
pub(crate) fn record(account: &str, domain: &str, expires_at: Option<&str>) -> SessionRecord {
    record_full(
        account,
        domain,
        expires_at,
        SourceType::Extension,
        "chromium",
    )
}

/// 指定来源的合法 web 会话记录。
pub(crate) fn record_full(
    account: &str,
    domain: &str,
    expires_at: Option<&str>,
    source_type: SourceType,
    source_id: &str,
) -> SessionRecord {
    SessionRecord {
        account_ref: account.to_string(),
        target: Target {
            kind: TargetKind::Web,
            domain: Some(domain.to_string()),
            app_id: None,
        },
        kind: SessionKind::Cookie,
        payload: Payload {
            cookies: vec![cookie()],
            ..Payload::default()
        },
        expires_at: expires_at.map(|value| value.to_string()),
        source: Source {
            source_type,
            id: source_id.to_string(),
        },
    }
}

/// 条目（来源固定 extension/chromium）。
pub(crate) fn entry(
    kind: EndpointKind,
    endpoint_id: &str,
    account: &str,
    domain: &str,
    expires_at: Option<&str>,
) -> SessionEntry {
    entry_full(
        kind,
        endpoint_id,
        account,
        domain,
        expires_at,
        SourceType::Extension,
        "chromium",
    )
}

/// 指定来源的条目。
pub(crate) fn entry_full(
    kind: EndpointKind,
    endpoint_id: &str,
    account: &str,
    domain: &str,
    expires_at: Option<&str>,
    source_type: SourceType,
    source_id: &str,
) -> SessionEntry {
    SessionEntry::new(
        Endpoint::new(kind, endpoint_id),
        record_full(account, domain, expires_at, source_type, source_id),
    )
}
