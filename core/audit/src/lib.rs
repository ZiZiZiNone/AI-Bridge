//! `ab-audit` —— 追加式审计：入账与查询，历史不可删改；写入入口 `record_or_fail`（fail-closed）。
#![forbid(unsafe_code)]

use ab_contract::SessionEnvelope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Unlock,
    Lock,
    Capture,
    Inject,
    Fill,
    Issue,
    Release,
    Revoke,
    RevokeAll,
    Sweep,
    Extract,
    ConverterLoad,
    ConverterReject,
    Pair,
    Sync,
    VaultWrite,
    VaultDelete,
    Restore,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Ok,
    Denied,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub who: String,
    /// RFC 3339，由调用方注入（本 crate 无时钟）。
    pub when: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    pub action: AuditAction,
    pub result: AuditResult,
    /// 敏感值（密码 / cookie / token）不得入账。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detail: BTreeMap<String, String>,
}

impl AuditEvent {
    pub fn new(
        who: impl Into<String>,
        when: impl Into<String>,
        action: AuditAction,
        result: AuditResult,
    ) -> Self {
        Self {
            who: who.into(),
            when: when.into(),
            account_ref: None,
            domain: None,
            action,
            result,
            detail: BTreeMap::new(),
        }
    }

    /// 身份映射只此一份；app / container 目标无 domain，需要时把 app_id 写入 detail。
    pub fn from_envelope(
        who: impl Into<String>,
        when: impl Into<String>,
        action: AuditAction,
        result: AuditResult,
        envelope: &SessionEnvelope,
    ) -> Self {
        Self {
            who: who.into(),
            when: when.into(),
            account_ref: Some(envelope.account_ref.clone()),
            domain: envelope.target.domain.clone(),
            action,
            result,
            detail: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), AuditError> {
        for (name, value) in [("who", self.who.as_str()), ("when", self.when.as_str())] {
            if value.trim().is_empty() {
                return Err(AuditError::EmptyField(name));
            }
        }
        Ok(())
    }
}

/// `None` = 不限；给出时精确相等。无时间范围：范围语义属宿主时钟，不在本 crate 另发明。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditFilter {
    pub who: Option<String>,
    pub account_ref: Option<String>,
    pub domain: Option<String>,
    pub action: Option<AuditAction>,
    pub result: Option<AuditResult>,
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
            && self.action.is_none_or(|a| a == event.action)
            && self.result.is_none_or(|r| r == event.result)
            && optional(&self.account_ref, &event.account_ref)
            && optional(&self.domain, &event.domain)
    }
}

/// 实现只允许追加，不得丢弃 / 改写 / 删除已入账事件。
pub trait AuditSink: Send + Sync {
    /// 审计写入唯一入口：`Err` 即入账失败，调用方敏感操作必须中止（`?` 传播，禁止吞错）。
    fn record_or_fail(&self, event: AuditEvent) -> Result<(), AuditError>;

    /// 保持追加顺序。
    fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, AuditError>;

    fn count(&self) -> Result<usize, AuditError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    /// 出口不可用（锁中毒 / 宿主 IO 失败）。
    Unavailable,
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
    fn record_or_fail(&self, event: AuditEvent) -> Result<(), AuditError> {
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

    fn ev(who: &str, secs: u32, action: AuditAction, result: AuditResult) -> AuditEvent {
        let mut event = AuditEvent::new(who, at(secs), action, result);
        event.account_ref = Some("acc-1".into());
        event.domain = Some("github.com".into());
        event
    }

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
            scope: None,
            client_env: None,
        }
    }

    #[test]
    fn appended_events_are_immutable_after_record() {
        let sink = InMemoryAuditSink::new();
        let mut original = ev("alice", 1, AuditAction::Unlock, AuditResult::Ok);
        sink.record_or_fail(original.clone()).unwrap();

        original.action = AuditAction::Lock;
        original.detail.insert("note".into(), "改了".into());

        let stored = sink.query(&AuditFilter::default()).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].action, AuditAction::Unlock);
        assert!(stored[0].detail.is_empty());

        assert_eq!(sink.query(&AuditFilter::default()).unwrap(), stored);
        assert_eq!(sink.count().unwrap(), 1);
    }

    #[test]
    fn incomplete_event_rejected_and_log_unchanged() {
        let sink = InMemoryAuditSink::new();
        sink.record_or_fail(ev("alice", 1, AuditAction::Unlock, AuditResult::Ok))
            .unwrap();

        let incomplete = AuditEvent::new("   ", at(2), AuditAction::Unlock, AuditResult::Ok);
        assert_eq!(incomplete.validate(), Err(AuditError::EmptyField("who")));
        assert_eq!(
            sink.record_or_fail(incomplete),
            Err(AuditError::EmptyField("who"))
        );
        assert_eq!(
            sink.record_or_fail(AuditEvent::new(
                "alice",
                "",
                AuditAction::Unlock,
                AuditResult::Ok
            )),
            Err(AuditError::EmptyField("when"))
        );
        assert_eq!(sink.count().unwrap(), 1);
    }

    #[test]
    fn query_filters_by_single_field_and_combination() {
        let sink = InMemoryAuditSink::new();
        sink.record_or_fail(ev("alice", 1, AuditAction::Unlock, AuditResult::Ok))
            .unwrap();
        sink.record_or_fail(ev("bob", 2, AuditAction::Capture, AuditResult::Ok))
            .unwrap();
        let mut denied = ev("bob", 3, AuditAction::Capture, AuditResult::Denied);
        denied.domain = Some("qq.com".into());
        sink.record_or_fail(denied).unwrap();
        let mut no_ref = ev("carol", 4, AuditAction::Release, AuditResult::Ok);
        no_ref.account_ref = None;
        no_ref.domain = None;
        sink.record_or_fail(no_ref).unwrap();

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
                action: Some(AuditAction::Capture),
                result: Some(AuditResult::Denied),
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

        assert_eq!(sink.count().unwrap(), 4);
    }

    #[test]
    fn query_preserves_append_order() {
        let sink = InMemoryAuditSink::new();
        let actions = [
            AuditAction::Unlock,
            AuditAction::Capture,
            AuditAction::Issue,
            AuditAction::Release,
        ];
        for (index, action) in actions.iter().enumerate() {
            sink.record_or_fail(ev("alice", index as u32 + 1, *action, AuditResult::Ok))
                .unwrap();
        }

        let events = sink.query(&AuditFilter::default()).unwrap();
        assert_eq!(sink.count().unwrap(), 4);

        let got: Vec<AuditAction> = events.iter().map(|e| e.action).collect();
        assert_eq!(got, actions);

        let times: Vec<String> = events.iter().map(|e| e.when.clone()).collect();
        assert_eq!(times, vec![at(1), at(2), at(3), at(4)]);

        let ok_only = sink
            .query(&AuditFilter {
                result: Some(AuditResult::Ok),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(ok_only.len(), 4);
        assert_eq!(
            ok_only.iter().map(|e| e.action).collect::<Vec<_>>(),
            actions
        );
    }

    #[test]
    fn sink_is_injectable_as_trait_object() {
        let sink: Box<dyn AuditSink> = Box::new(InMemoryAuditSink::new());
        sink.record_or_fail(ev("alice", 1, AuditAction::Unlock, AuditResult::Ok))
            .unwrap();
        assert_eq!(sink.count().unwrap(), 1);
        assert_eq!(sink.query(&AuditFilter::default()).unwrap().len(), 1);
    }

    #[test]
    fn identity_derives_from_contract_envelope() {
        let env = envelope();
        assert!(env.validate().is_ok());

        let event = AuditEvent::from_envelope(
            "sync",
            at(2),
            AuditAction::Capture,
            AuditResult::Ok,
            &env,
        );
        assert_eq!(event.account_ref.as_deref(), Some("acc-9"));
        assert_eq!(event.domain.as_deref(), Some("github.com"));
        assert!(event.detail.is_empty());
        assert!(event.validate().is_ok());

        let sink = InMemoryAuditSink::new();
        sink.record_or_fail(event).unwrap();
        let hit = sink
            .query(&AuditFilter {
                account_ref: Some("acc-9".into()),
                domain: Some("github.com".into()),
                action: Some(AuditAction::Capture),
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

        let event = AuditEvent::from_envelope(
            "converter",
            at(2),
            AuditAction::Extract,
            AuditResult::Ok,
            &env,
        );
        assert_eq!(event.account_ref.as_deref(), Some("acc-9"));
        assert_eq!(event.domain, None);
    }

    #[test]
    fn serde_uses_snake_case_and_skips_empty_optionals() {
        let json = serde_json::to_value(ev("alice", 1, AuditAction::Capture, AuditResult::Ok))
            .unwrap();
        let object = json.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["account_ref", "action", "domain", "result", "when", "who"]
        );
        assert_eq!(object["when"], at(1));
        assert_eq!(object["account_ref"], "acc-1");

        assert_eq!(object["action"], "capture");
        assert_eq!(object["result"], "ok");
        assert_eq!(
            serde_json::to_value(AuditAction::ConverterLoad).unwrap(),
            "converter_load"
        );
        assert_eq!(
            serde_json::to_value(AuditAction::RevokeAll).unwrap(),
            "revoke_all"
        );
        assert_eq!(
            serde_json::to_value(AuditResult::Denied).unwrap(),
            "denied"
        );

        let bare = AuditEvent::new("carol", at(9), AuditAction::Release, AuditResult::Ok);
        let bare_json = serde_json::to_value(&bare).unwrap();
        assert!(bare_json.get("account_ref").is_none());
        assert!(bare_json.get("domain").is_none());
        assert!(bare_json.get("detail").is_none());
        let back: AuditEvent = serde_json::from_value(bare_json).unwrap();
        assert_eq!(back, bare);

        let mut with_detail = bare.clone();
        with_detail.detail.insert("cookies".into(), "3".into());
        with_detail.detail.insert("note".into(), "bridge".into());
        let round_trip: AuditEvent =
            serde_json::from_value(serde_json::to_value(&with_detail).unwrap()).unwrap();
        assert_eq!(round_trip, with_detail);
    }
}
