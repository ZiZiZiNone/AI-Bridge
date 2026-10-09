//! `ab-audit` —— 追加式审计：事件入账与查询，**历史不可删改**。
//!
//! 职责与边界：
//! - 只做审计事件的数据定义（`AuditEvent`）与筛选查询逻辑；不落盘、不联网、不碰任何宿主对象。
//! - 出口经 `AuditSink` trait 注入：内存实现 `InMemoryAuditSink`，落盘 / 上报形态由宿主实现；
//!   本 crate 与使用者均不得引入全局单例、静态门面或服务定位器。
//! - 结构上 append-only：本 crate 不提供、也不导出任何删除或修改已入账事件的 API，
//!   `InMemoryAuditSink` 内部只做 push。
//! - `when` 由调用方注入（字符串），本 crate 不自带时钟，避免与宿主注入的 `Clock` 重复。
//! - 依赖 `ab-contract` 的唯一用途是 [`AuditEvent::from_envelope`]：把会话信封映射为审计身份。
//!   account_ref / domain 的映射规则只此一份，宿主与其他 core crate 不得再各写一遍。
//! - 敏感值（密码、cookie 原文、token）不得进 `detail`；审计可读，但不可泄密。

use ab_contract::SessionEnvelope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

/* ---------------- 事件 ---------------- */

/// 单条审计事件。字段名与契约层命名风格一致（snake_case）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// 主体：谁做的（账号名 / 设备 id / 扩展 id）。
    pub who: String,
    /// 发生时刻：调用方注入的 RFC 3339 字符串。
    pub when: String,
    /// 账号引用；语义同 `ab-contract` 的 `account_ref`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
    /// 域名；语义同 `ab-contract::Target.domain`（app / container 目标为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// 动作：capture / issue / release / revoke ...（命名由调用方按业务给出）。
    pub action: String,
    /// 结果：ok / denied / failed ...（命名由调用方按业务给出）。
    pub result: String,
    /// 明细：自由键值，仅追责与展示用；敏感值不得入账。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detail: BTreeMap<String, Value>,
}

impl AuditEvent {
    /// 建一条事件；可选字段留空，入账前可按需赋值（入账后即不可改）。
    pub fn new(
        who: impl Into<String>,
        when: impl Into<String>,
        action: impl Into<String>,
        result: impl Into<String>,
    ) -> Self {
        Self {
            who: who.into(),
            when: when.into(),
            account_ref: None,
            domain: None,
            action: action.into(),
            result: result.into(),
            detail: BTreeMap::new(),
        }
    }

    /// 由契约层会话信封派生审计身份：`account_ref` 取信封的 account_ref，`domain` 取 target.domain。
    /// target 为 app / container 时 domain 为 `None`，需要时由调用方把 app_id 写入 `detail`。
    pub fn from_envelope(
        who: impl Into<String>,
        when: impl Into<String>,
        action: impl Into<String>,
        result: impl Into<String>,
        envelope: &SessionEnvelope,
    ) -> Self {
        Self {
            who: who.into(),
            when: when.into(),
            account_ref: Some(envelope.account_ref.clone()),
            domain: envelope.target.domain.clone(),
            action: action.into(),
            result: result.into(),
            detail: BTreeMap::new(),
        }
    }

    /// 审计完整性：who / when / action / result 不得为空。
    /// 规则只此一份，所有 `AuditSink` 实现（含宿主落盘实现）都应先调用本方法。
    pub fn validate(&self) -> Result<(), AuditError> {
        let required = [
            ("who", self.who.as_str()),
            ("when", self.when.as_str()),
            ("action", self.action.as_str()),
            ("result", self.result.as_str()),
        ];
        for (name, value) in required {
            if value.trim().is_empty() {
                return Err(AuditError::EmptyField(name));
            }
        }
        Ok(())
    }
}

/* ---------------- 查询 ---------------- */

/// 查询筛选：字段为 `None` 表示不限制；给出时按**精确相等**匹配。默认值 = 取全部。
///
/// 不含时间范围条件：`when` 是字符串，范围比较的语义（时区 / 格式）属宿主时钟范围，
/// 不在本 crate 里另发明一套。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditFilter {
    pub who: Option<String>,
    pub account_ref: Option<String>,
    pub domain: Option<String>,
    pub action: Option<String>,
    pub result: Option<String>,
}

impl AuditFilter {
    fn matches(&self, event: &AuditEvent) -> bool {
        let text = |want: &Option<String>, got: &str| match want {
            None => true,
            Some(w) => w.as_str() == got,
        };
        let optional = |want: &Option<String>, got: &Option<String>| match want {
            None => true,
            Some(w) => got.as_deref() == Some(w.as_str()),
        };
        text(&self.who, &event.who)
            && text(&self.action, &event.action)
            && text(&self.result, &event.result)
            && optional(&self.account_ref, &event.account_ref)
            && optional(&self.domain, &event.domain)
    }
}

/* ---------------- 出口 ---------------- */

/// 审计出口。宿主可注入不同实现（内存 / 落盘 / 上报），一律经依赖注入，无全局实例。
///
/// 契约：实现只允许追加，不得丢弃、改写或删除任何已入账事件。
pub trait AuditSink: Send + Sync {
    /// 追加一条事件。
    fn record(&self, event: AuditEvent) -> Result<(), AuditError>;

    /// 按筛选条件返回事件，**保持追加顺序**。
    fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, AuditError>;

    /// 已入账事件总数（不受筛选影响）。
    fn count(&self) -> Result<usize, AuditError>;
}

/// 审计出口错误。业务语义（谁能填哪个域）不在此列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    /// 出口不可用（如内存锁中毒）；宿主实现把自己的 IO / 持久化失败也映射到此处。
    Unavailable,
    /// 审计完整性：该必填字段为空，事件拒绝入账。
    EmptyField(&'static str),
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditError::Unavailable => write!(f, "审计出口不可用"),
            AuditError::EmptyField(name) => write!(f, "审计事件字段 {name} 不能为空"),
        }
    }
}

impl std::error::Error for AuditError {}

/// 内存审计出口（测试与单机运行用）。内部 `Mutex` 保证 `&self` 可追加，便于注入为 trait 对象。
///
/// 只有追加路径：无 remove / update / clear / truncate 之类 API。
#[derive(Debug, Default)]
pub struct InMemoryAuditSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl InMemoryAuditSink {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }
}

impl AuditSink for InMemoryAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        event.validate()?;
        let mut events = self.events.lock().map_err(|_| AuditError::Unavailable)?;
        events.push(event);
        Ok(())
    }

    fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, AuditError> {
        let events = self.events.lock().map_err(|_| AuditError::Unavailable)?;
        Ok(events
            .iter()
            .filter(|event| filter.matches(event))
            .cloned()
            .collect())
    }

    fn count(&self) -> Result<usize, AuditError> {
        let events = self.events.lock().map_err(|_| AuditError::Unavailable)?;
        Ok(events.len())
    }
}

/* ---------------- 测试 ---------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use ab_contract::{
        Cookie, Payload, SameSite, SessionEnvelope, SessionKind, Source, SourceType, Target,
        TargetKind,
    };

    fn at(secs: u32) -> String {
        format!("2026-10-09T00:00:{secs:02}Z")
    }

    /// 带 account_ref / domain 的常规事件。
    fn ev(who: &str, secs: u32, action: &str, result: &str) -> AuditEvent {
        let mut event = AuditEvent::new(who, at(secs), action, result);
        event.account_ref = Some("acc-1".into());
        event.domain = Some("github.com".into());
        event
    }

    /// 合法的 cookie 会话信封（kind=cookie 必须携带非空 cookies，否则 ab-contract 拒绝）。
    fn envelope() -> SessionEnvelope {
        SessionEnvelope {
            schema_version: "1.0".into(),
            account_ref: "acc-9".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some("github.com".into()),
                app_id: None,
            },
            kind: SessionKind::Cookie,
            payload: Payload {
                cookies: vec![Cookie {
                    name: "sid".into(),
                    value: "abc".into(),
                    domain: Some("github.com".into()),
                    path: Some("/".into()),
                    secure: Some(true),
                    http_only: Some(true),
                    same_site: Some(SameSite::Lax),
                    expires_at: None,
                }],
                ..Payload::default()
            },
            issued_at: at(1),
            expires_at: None,
            source: Source {
                source_type: SourceType::Extension,
                id: "chromium".into(),
            },
        }
    }

    #[test]
    fn appended_events_are_immutable_after_record() {
        let sink = InMemoryAuditSink::new();
        let mut original = ev("alice", 1, "login", "ok");
        sink.record(original.clone()).unwrap();

        // 调用方手里那份后续怎么改，都不影响已入账事件
        original.action = "tampered".into();
        original.detail.insert("note".into(), Value::String("改了".into()));

        let stored = sink.query(&AuditFilter::default()).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].action, "login");
        assert!(stored[0].detail.is_empty());

        // 历史只增不改：重复查询结果稳定，条数不变
        assert_eq!(sink.query(&AuditFilter::default()).unwrap(), stored);
        assert_eq!(sink.count().unwrap(), 1);
    }

    #[test]
    fn incomplete_event_rejected_and_log_unchanged() {
        let sink = InMemoryAuditSink::new();
        sink.record(ev("alice", 1, "login", "ok")).unwrap();

        let incomplete = AuditEvent::new("   ", at(2), "login", "ok");
        assert_eq!(incomplete.validate(), Err(AuditError::EmptyField("who")));
        assert_eq!(
            sink.record(incomplete),
            Err(AuditError::EmptyField("who"))
        );
        assert_eq!(
            sink.record(AuditEvent::new("alice", "", "login", "ok")),
            Err(AuditError::EmptyField("when"))
        );
        assert_eq!(
            sink.record(AuditEvent::new("alice", at(3), "", "ok")),
            Err(AuditError::EmptyField("action"))
        );
        assert_eq!(
            sink.record(AuditEvent::new("alice", at(4), "login", "")),
            Err(AuditError::EmptyField("result"))
        );

        // 被拒事件不入账
        assert_eq!(sink.count().unwrap(), 1);
    }

    #[test]
    fn query_filters_by_single_field_and_combination() {
        let sink = InMemoryAuditSink::new();
        sink.record(ev("alice", 1, "login", "ok")).unwrap();
        sink.record(ev("bob", 2, "capture", "ok")).unwrap();
        let mut denied = ev("bob", 3, "capture", "denied");
        denied.domain = Some("qq.com".into());
        sink.record(denied).unwrap();
        let mut no_ref = ev("carol", 4, "release", "ok");
        no_ref.account_ref = None;
        no_ref.domain = None;
        sink.record(no_ref).unwrap();

        assert_eq!(sink.query(&AuditFilter::default()).unwrap().len(), 4);
        assert_eq!(sink.count().unwrap(), 4);

        let by_who = sink
            .query(&AuditFilter {
                who: Some("bob".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_who.len(), 2);
        assert!(by_who.iter().all(|e| e.who == "bob"));

        let by_combo = sink
            .query(&AuditFilter {
                action: Some("capture".into()),
                result: Some("denied".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_combo.len(), 1);
        assert_eq!(by_combo[0].when, at(3));
        assert_eq!(by_combo[0].who, "bob");

        let by_domain = sink
            .query(&AuditFilter {
                domain: Some("github.com".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_domain.len(), 2);

        // account_ref 为 None 的事件不会被 Some("acc-1") 命中
        let by_ref = sink
            .query(&AuditFilter {
                account_ref: Some("acc-1".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_ref.len(), 3);

        let none = sink
            .query(&AuditFilter {
                who: Some("nobody".into()),
                ..Default::default()
            })
            .unwrap();
        assert!(none.is_empty());

        // 查询不改写历史
        assert_eq!(sink.count().unwrap(), 4);
    }

    #[test]
    fn query_preserves_append_order() {
        let sink = InMemoryAuditSink::new();
        for (index, action) in ["unlock", "capture", "issue", "release"]
            .iter()
            .enumerate()
        {
            sink.record(ev("alice", index as u32 + 1, action, "ok"))
                .unwrap();
        }

        let events = sink.query(&AuditFilter::default()).unwrap();
        assert_eq!(sink.count().unwrap(), 4);

        let actions: Vec<String> = events.iter().map(|e| e.action.clone()).collect();
        let expected: Vec<String> = ["unlock", "capture", "issue", "release"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(actions, expected);

        let times: Vec<String> = events.iter().map(|e| e.when.clone()).collect();
        assert_eq!(times, vec![at(1), at(2), at(3), at(4)]);

        // 筛选后的子序列同样保持追加顺序
        let ok_only = sink
            .query(&AuditFilter {
                result: Some("ok".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(ok_only.len(), 4);
        assert_eq!(
            ok_only.iter().map(|e| e.action.clone()).collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn sink_is_injectable_as_trait_object() {
        let sink: Box<dyn AuditSink> = Box::new(InMemoryAuditSink::new());
        sink.record(ev("alice", 1, "login", "ok")).unwrap();
        assert_eq!(sink.count().unwrap(), 1);
        assert_eq!(sink.query(&AuditFilter::default()).unwrap().len(), 1);
    }

    #[test]
    fn identity_derives_from_contract_envelope() {
        let env = envelope();
        assert!(env.validate().is_ok(), "信封样例本身必须合法");

        let event = AuditEvent::from_envelope("sync", at(2), "capture", "ok", &env);
        assert_eq!(event.account_ref.as_deref(), Some("acc-9"));
        assert_eq!(event.domain.as_deref(), Some("github.com"));
        assert!(event.detail.is_empty());
        assert!(event.validate().is_ok());

        let sink = InMemoryAuditSink::new();
        sink.record(event).unwrap();
        let hit = sink
            .query(&AuditFilter {
                account_ref: Some("acc-9".into()),
                domain: Some("github.com".into()),
                action: Some("capture".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hit.len(), 1);
    }

    #[test]
    fn identity_from_app_target_has_no_domain() {
        let mut env = envelope();
        env.target = Target {
            kind: TargetKind::App,
            domain: None,
            app_id: Some("qq".into()),
        };
        assert!(env.validate().is_ok());

        let event = AuditEvent::from_envelope("converter", at(2), "extract", "ok", &env);
        assert_eq!(event.account_ref.as_deref(), Some("acc-9"));
        assert_eq!(event.domain, None);
    }

    #[test]
    fn serde_uses_snake_case_and_skips_empty_optionals() {
        let json = serde_json::to_value(ev("alice", 1, "capture", "ok")).unwrap();
        let object = json.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["account_ref", "action", "domain", "result", "when", "who"]
        );
        assert_eq!(object["when"], Value::String(at(1)));
        assert_eq!(object["account_ref"], Value::String("acc-1".into()));

        // 可选字段为空时不落键，缺失也能还原
        let bare = AuditEvent::new("carol", at(9), "release", "ok");
        let bare_json = serde_json::to_value(&bare).unwrap();
        assert!(bare_json.get("account_ref").is_none());
        assert!(bare_json.get("domain").is_none());
        assert!(bare_json.get("detail").is_none());
        let back: AuditEvent = serde_json::from_value(bare_json).unwrap();
        assert_eq!(back, bare);

        // detail 支持结构化 JSON，且往返不丢
        let mut with_detail = bare.clone();
        with_detail
            .detail
            .insert("cookies".into(), Value::from(3));
        with_detail
            .detail
            .insert("note".into(), Value::String("bridge".into()));
        let round_trip: AuditEvent =
            serde_json::from_value(serde_json::to_value(&with_detail).unwrap()).unwrap();
        assert_eq!(round_trip, with_detail);
    }
}
