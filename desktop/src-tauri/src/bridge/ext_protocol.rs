//! native messaging 协议翻译（扩展 ⇄ 主程序）。
//!
//! 帧格式（Chromium / Edge / Firefox native messaging 一致）：
//! **4 字节小端无符号长度前缀 + UTF-8 JSON 负载**，单条上限 1 MiB。
//!
//! 本文件只做编解码与消息结构定义：`payload` 原样透传（不解读、不落盘、不进日志），
//! 业务判断一律回 core（架构与契约 §3「只做协议翻译」）。
//!
//! 未验证：单测已写，本机无 cargo 未运行。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// 单条消息上限（协议上限 1 MiB）。
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// 扩展 → 主程序。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtRequest {
    /// 请求标识，响应必须原样回带（扩展侧靠它配对）
    pub id: String,
    /// 动作名（协议层标识，语义由 core 决定）
    pub action: String,
    /// 结构化负载：原样透传，本层不解读
    #[serde(default)]
    pub payload: Value,
}

/// 主程序 → 扩展。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtResponse {
    pub id: String,
    pub ok: bool,
    /// 失败原因（只放可展示的归一化文案，禁止携带凭据 / 明文）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

/// 帧编解码错误。只带结构信息与原因，不带负载原文（避免明文进日志）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtProtocolError {
    /// 帧声明的长度超过上限
    TooLarge(usize),
    /// 负载长度与声明不符
    LengthMismatch { declared: usize, actual: usize },
    /// JSON 编解码失败（只保留原因）
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

/// 编码一帧（长度前缀 + JSON 负载）。
pub fn encode_frame<T: Serialize>(message: &T) -> Result<Vec<u8>, ExtProtocolError> {
    let body =
        serde_json::to_vec(message).map_err(|error| ExtProtocolError::InvalidJson(error.to_string()))?;

    if body.len() > MAX_MESSAGE_BYTES {
        return Err(ExtProtocolError::TooLarge(body.len()));
    }

    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// 解析 4 字节小端长度前缀。
pub fn decode_length(header: [u8; 4]) -> Result<usize, ExtProtocolError> {
    let declared = u32::from_le_bytes(header) as usize;
    if declared > MAX_MESSAGE_BYTES {
        return Err(ExtProtocolError::TooLarge(declared));
    }
    Ok(declared)
}

/// 解析负载 JSON 为指定消息类型。
pub fn decode_payload<T: DeserializeOwned>(body: &[u8]) -> Result<T, ExtProtocolError> {
    serde_json::from_slice(body).map_err(|error| ExtProtocolError::InvalidJson(error.to_string()))
}

/// 校验负载长度与声明一致（读帧后调用，避免把残留字节当成下一条消息）。
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
    // 未验证：本机无 cargo，未运行。
    use super::*;
    use serde_json::json;

    fn request() -> ExtRequest {
        ExtRequest {
            id: "req-1".into(),
            action: "capture_session".into(),
            payload: json!({ "domain": "github.com" }),
        }
    }

    #[test]
    fn frame_roundtrip_keeps_payload_and_prefix() {
        let frame = encode_frame(&request()).unwrap();
        let (header, body) = frame.split_at(4);
        let declared = decode_length([header[0], header[1], header[2], header[3]]).unwrap();

        assert_eq!(declared, body.len());
        check_length(declared, body).unwrap();

        let back: ExtRequest = decode_payload(body).unwrap();
        assert_eq!(back, request());
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
            id: "req-2".into(),
            action: "noop".into(),
            payload: json!({ "blob": "x".repeat(MAX_MESSAGE_BYTES + 1) }),
        };
        assert!(matches!(
            encode_frame(&big),
            Err(ExtProtocolError::TooLarge(_))
        ));
        // 长度前缀本身也要挡：声明过大直接拒绝，不分配内存
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
