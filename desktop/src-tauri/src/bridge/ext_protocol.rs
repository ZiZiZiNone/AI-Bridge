//! native messaging 协议编解码（扩展 ⇄ 主程序，对齐 `extensions/shared/bridge.ts` v1）。
//!
//! 帧格式：4 字节小端无符号长度前缀 + UTF-8 JSON 负载，单条上限 1 MiB。
//! 支持三类消息：`request`（扩展→宿主）、`response`（宿主→扩展）、`event`（宿主→扩展主动推送，含 `broker.revoked`）。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const BRIDGE_PROTOCOL_VERSION: u8 = 1;

fn default_version() -> u8 {
    BRIDGE_PROTOCOL_VERSION
}

fn default_request_type() -> String {
    "request".to_string()
}

fn default_response_type() -> String {
    "response".to_string()
}

fn default_event_type() -> String {
    "event".to_string()
}

/// 扩展 → 主程序请求（兼容 `extensions/shared/bridge.ts` 的 `method`/`params` 与 `action`/`payload`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtRequest {
    #[serde(default = "default_version")]
    pub v: u8,
    #[serde(rename = "type", default = "default_request_type")]
    pub msg_type: String,
    pub id: String,
    #[serde(alias = "method")]
    pub action: String,
    #[serde(default, alias = "params")]
    pub payload: Value,
}

/// 主程序 → 扩展响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtResponse {
    #[serde(default = "default_version")]
    pub v: u8,
    #[serde(rename = "type", default = "default_response_type")]
    pub msg_type: String,
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, alias = "result")]
    pub payload: Value,
}

/// 主程序 → 扩展主动事件（对齐 `extensions/shared/bridge.ts` 的 `BridgeEvent`：
/// `state.changed` / `broker.revoked` / `host.disconnected`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtEvent {
    #[serde(default = "default_version")]
    pub v: u8,
    #[serde(rename = "type", default = "default_event_type")]
    pub msg_type: String,
    pub event: String,
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emitted_at: Option<String>,
}

impl ExtEvent {
    /// 构造 `broker.revoked` 事件帧，通知浏览器扩展立即清退到期或被吊销的临时会话 Cookie 与 `localStorage`。
    pub fn broker_revoked(account_ref: impl Into<String>, revoked_at: impl Into<String>) -> Self {
        let ts = revoked_at.into();
        Self {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "event".to_string(),
            event: "broker.revoked".to_string(),
            payload: json!({
                "account_ref": account_ref.into(),
                "revoked_at": ts,
            }),
            emitted_at: Some(ts),
        }
    }

    /// 构造携带目标域名、已注入 Cookie 名称清单与 `clear_storage` 标志的完整 `broker.revoked` 事件帧。
    pub fn broker_revoked_detailed(
        session_id: impl Into<String>,
        account_ref: impl Into<String>,
        target_domain: Option<String>,
        cookie_names: Vec<String>,
        clear_storage: bool,
        revoked_at: impl Into<String>,
    ) -> Self {
        let ts = revoked_at.into();
        Self {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "event".to_string(),
            event: "broker.revoked".to_string(),
            payload: json!({
                "session_id": session_id.into(),
                "account_ref": account_ref.into(),
                "domain": target_domain,
                "cookie_names": cookie_names,
                "clear_storage": clear_storage,
                "revoked_at": ts,
            }),
            emitted_at: Some(ts),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtProtocolError {
    TooLarge(usize),
    LengthMismatch { declared: usize, actual: usize },
    InvalidJson(String),
}

impl fmt::Display for ExtProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtProtocolError::TooLarge(size) => {
                write!(f, "消息过大：{size} 字节（上限 {MAX_MESSAGE_BYTES}）")
            }
            ExtProtocolError::LengthMismatch { declared, actual } => {
                write!(f, "帧长度不符：声明 {declared}，实际 {actual}")
            }
            ExtProtocolError::InvalidJson(reason) => write!(f, "负载不是合法 JSON：{reason}"),
        }
    }
}

impl std::error::Error for ExtProtocolError {}

pub fn encode_frame<T: Serialize>(message: &T) -> Result<Vec<u8>, ExtProtocolError> {
    let body = serde_json::to_vec(message)
        .map_err(|error| ExtProtocolError::InvalidJson(error.to_string()))?;

    if body.len() > MAX_MESSAGE_BYTES {
        return Err(ExtProtocolError::TooLarge(body.len()));
    }

    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn decode_length(header: [u8; 4]) -> Result<usize, ExtProtocolError> {
    let declared = u32::from_le_bytes(header) as usize;
    if declared > MAX_MESSAGE_BYTES {
        return Err(ExtProtocolError::TooLarge(declared));
    }
    Ok(declared)
}

pub fn decode_payload<T: DeserializeOwned>(body: &[u8]) -> Result<T, ExtProtocolError> {
    serde_json::from_slice(body).map_err(|error| ExtProtocolError::InvalidJson(error.to_string()))
}

pub fn check_length(declared: usize, body: &[u8]) -> Result<(), ExtProtocolError> {
    if declared == body.len() {
        Ok(())
    } else {
        Err(ExtProtocolError::LengthMismatch {
            declared,
            actual: body.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> ExtRequest {
        ExtRequest {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "request".into(),
            id: "req-1".into(),
            action: "session.capture".into(),
            payload: json!({ "domain": "github.com" }),
        }
    }

    #[test]
    fn frame_roundtrip_and_bridge_ts_wire_compat() {
        let frame = encode_frame(&request()).unwrap();
        let (header, body) = frame.split_at(4);
        let declared = decode_length([header[0], header[1], header[2], header[3]]).unwrap();

        assert_eq!(declared, body.len());
        check_length(declared, body).unwrap();

        let back: ExtRequest = decode_payload(body).unwrap();
        assert_eq!(back, request());

        let ts_wire = br#"{"v":1,"type":"request","id":"req-9","method":"ping","params":{}}"#;
        let parsed_ts: ExtRequest = decode_payload(ts_wire).unwrap();
        assert_eq!(parsed_ts.id, "req-9");
        assert_eq!(parsed_ts.action, "ping");

        let ev = ExtEvent::broker_revoked_detailed(
            "tmp-1",
            "acc-1",
            Some("github.com".into()),
            vec!["sid".into()],
            true,
            "2026-10-14T00:00:00Z",
        );
        let ev_frame = encode_frame(&ev).unwrap();
        let decoded_ev: ExtEvent = decode_payload(&ev_frame[4..]).unwrap();
        assert_eq!(decoded_ev.event, "broker.revoked");
        assert_eq!(decoded_ev.payload["account_ref"], "acc-1");
        assert_eq!(decoded_ev.payload["domain"], "github.com");
        assert_eq!(decoded_ev.payload["clear_storage"], true);
    }

    #[test]
    fn length_prefix_is_little_endian_u32() {
        let frame = encode_frame(&request()).unwrap();
        let declared = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(declared, frame.len() - 4);
    }

    #[test]
    fn oversize_message_is_rejected_before_encoding() {
        let big = ExtRequest {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "request".into(),
            id: "req-2".into(),
            action: "noop".into(),
            payload: json!({ "blob": "x".repeat(MAX_MESSAGE_BYTES + 1) }),
        };
        assert!(matches!(
            encode_frame(&big),
            Err(ExtProtocolError::TooLarge(_))
        ));
        let huge = (MAX_MESSAGE_BYTES as u32 + 1).to_le_bytes();
        assert!(matches!(
            decode_length(huge),
            Err(ExtProtocolError::TooLarge(_))
        ));
    }

    #[test]
    fn invalid_json_reports_reason_without_payload_text() {
        let error = decode_payload::<ExtRequest>(b"{ not json").unwrap_err();
        match error {
            ExtProtocolError::InvalidJson(reason) => assert!(!reason.is_empty()),
            other => panic!("期望 InvalidJson，得到 {other:?}"),
        }
    }

    #[test]
    fn mismatched_length_is_detected() {
        assert!(check_length(10, b"abc").is_err());
        assert!(check_length(3, b"abc").is_ok());
    }
}
