//! native messaging host 主循环（浏览器扩展 ⇄ 主程序，仅本地进程内 stdio）。
//!
//! 职责：
//! 1. 逐帧读取 4 字节小端长度前缀 + JSON 负载；
//! 2. 将 `handshake` / `ping` / `session.capture` / `session.inject` / `credential.fill` 交由注入的处理器执行；
//! 3. 支持向浏览器扩展主动推送 `ExtEvent`（如设定时间到期后的 `broker.revoked` 撤销事件）。

use std::io::{Read, Write};

use super::ext_protocol::{
    check_length, decode_length, decode_payload, encode_frame, ExtEvent, ExtProtocolError,
    ExtRequest, ExtResponse, BRIDGE_PROTOCOL_VERSION,
};

#[derive(Debug)]
pub enum MessagingHostError {
    Io(std::io::Error),
    Protocol(ExtProtocolError),
}

impl std::fmt::Display for MessagingHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessagingHostError::Io(error) => write!(f, "stdio 读写失败：{error}"),
            MessagingHostError::Protocol(error) => write!(f, "帧不合法：{error}"),
        }
    }
}

impl std::error::Error for MessagingHostError {}

impl From<std::io::Error> for MessagingHostError {
    fn from(error: std::io::Error) -> Self {
        MessagingHostError::Io(error)
    }
}

impl From<ExtProtocolError> for MessagingHostError {
    fn from(error: ExtProtocolError) -> Self {
        MessagingHostError::Protocol(error)
    }
}

/// 逐帧处理循环（每次请求处理后顺带下发待广播的 `ExtEvent`，如 `broker.revoked`）。
pub fn serve_with_events<R: Read, W: Write, F>(
    reader: &mut R,
    writer: &mut W,
    mut handle: F,
) -> Result<(), MessagingHostError>
where
    F: FnMut(ExtRequest) -> (ExtResponse, Vec<ExtEvent>),
{
    loop {
        let mut header = [0u8; 4];
        if !read_header(reader, &mut header)? {
            return Ok(());
        }

        let declared = decode_length(header)?;
        let mut body = vec![0u8; declared];
        reader.read_exact(&mut body)?;
        check_length(declared, &body)?;

        let request: ExtRequest = decode_payload(&body)?;
        let (response, events) = handle(request);

        writer.write_all(&encode_frame(&response)?)?;
        for event in events {
            writer.write_all(&encode_frame(&event)?)?;
        }
        writer.flush()?;
    }
}

pub fn serve<R: Read, W: Write, F>(
    reader: &mut R,
    writer: &mut W,
    mut handle: F,
) -> Result<(), MessagingHostError>
where
    F: FnMut(ExtRequest) -> ExtResponse,
{
    serve_with_events(reader, writer, |req| (handle(req), Vec::new()))
}

fn read_header<R: Read>(reader: &mut R, header: &mut [u8; 4]) -> Result<bool, MessagingHostError> {
    let mut filled = 0usize;
    while filled < header.len() {
        let read = reader.read(&mut header[filled..])?;
        if read == 0 {
            if filled == 0 {
                return Ok(false);
            }
            return Err(MessagingHostError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "帧头不完整",
            )));
        }
        filled += read;
    }
    Ok(true)
}

pub fn run_stdio() -> Result<(), MessagingHostError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    serve(&mut reader, &mut writer, |request| match request.action.as_str() {
        "handshake" => ExtResponse {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "response".into(),
            id: request.id,
            ok: true,
            method: Some("handshake".into()),
            error: None,
            payload: serde_json::json!({
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
            payload: serde_json::json!({ "ok": true }),
        },
        _ => ExtResponse {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "response".into(),
            id: request.id,
            ok: false,
            method: None,
            error: Some("桥宿主未解锁或未授权".to_string()),
            payload: serde_json::Value::Null,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ext_protocol::{encode_frame, ExtRequest};
    use serde_json::{json, Value};
    use std::io::Cursor;

    fn request(id: &str) -> ExtRequest {
        ExtRequest {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "request".into(),
            id: id.into(),
            action: "list_sessions".into(),
            payload: json!({}),
        }
    }

    fn echo(call: ExtRequest) -> ExtResponse {
        ExtResponse {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "response".into(),
            id: call.id,
            ok: true,
            method: Some(call.action),
            error: None,
            payload: json!({ "echo": true }),
        }
    }

    #[test]
    fn serves_every_frame_until_eof() {
        let mut input = Vec::new();
        input.extend(encode_frame(&request("1")).unwrap());
        input.extend(encode_frame(&request("2")).unwrap());

        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        serve(&mut reader, &mut output, echo).unwrap();

        let mut cursor = Cursor::new(output);
        let mut responses = Vec::new();
        loop {
            let mut header = [0u8; 4];
            if !read_header(&mut cursor, &mut header).unwrap() {
                break;
            }
            let declared = decode_length(header).unwrap();
            let mut body = vec![0u8; declared];
            cursor.read_exact(&mut body).unwrap();
            responses.push(decode_payload::<ExtResponse>(&body).unwrap());
        }

        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].id, "1");
        assert_eq!(responses[1].id, "2");
        assert_eq!(responses[0].payload, json!({ "echo": true }));
    }

    #[test]
    fn serve_with_events_emits_broker_revoked_event_frame() {
        let mut input = Vec::new();
        input.extend(encode_frame(&request("req-revoke")).unwrap());

        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        serve_with_events(&mut reader, &mut output, |call| {
            (
                echo(call),
                vec![ExtEvent::broker_revoked("acc-1", "2026-10-14T00:01:00Z")],
            )
        })
        .unwrap();

        let mut cursor = Cursor::new(output);
        let mut header1 = [0u8; 4];
        assert!(read_header(&mut cursor, &mut header1).unwrap());
        let len1 = decode_length(header1).unwrap();
        let mut body1 = vec![0u8; len1];
        cursor.read_exact(&mut body1).unwrap();
        let resp: ExtResponse = decode_payload(&body1).unwrap();
        assert_eq!(resp.id, "req-revoke");

        let mut header2 = [0u8; 4];
        assert!(read_header(&mut cursor, &mut header2).unwrap());
        let len2 = decode_length(header2).unwrap();
        let mut body2 = vec![0u8; len2];
        cursor.read_exact(&mut body2).unwrap();
        let ev: ExtEvent = decode_payload(&body2).unwrap();
        assert_eq!(ev.event, "broker.revoked");
        assert_eq!(ev.payload["account_ref"], "acc-1");
    }

    #[test]
    fn eof_at_frame_boundary_is_normal_exit() {
        let mut reader = Cursor::new(Vec::new());
        let mut output = Vec::new();
        assert!(serve(&mut reader, &mut output, echo).is_ok());
        assert!(output.is_empty());
    }

    #[test]
    fn partial_header_is_reported_as_io_error() {
        let mut reader = Cursor::new(vec![1u8, 2]);
        let mut output = Vec::new();
        let result = serve(&mut reader, &mut output, echo);
        assert!(matches!(result, Err(MessagingHostError::Io(_))));
    }

    #[test]
    fn body_shorter_than_declared_is_reported_as_io_error() {
        let mut input = Vec::new();
        input.extend_from_slice(&64u32.to_le_bytes());
        input.extend_from_slice(b"{\"id\":\"1\"}");

        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        let result = serve(&mut reader, &mut output, echo);
        assert!(matches!(result, Err(MessagingHostError::Io(_))));
    }

    #[test]
    fn non_json_body_is_reported_as_protocol_error() {
        let mut input = Vec::new();
        input.extend_from_slice(&9u32.to_le_bytes());
        input.extend_from_slice(b"{ nope  }");

        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        let result = serve(&mut reader, &mut output, echo);
        assert!(matches!(result, Err(MessagingHostError::Protocol(_))));
    }

    #[test]
    fn response_payload_is_opaque_to_the_bridge() {
        let mut input = Vec::new();
        input.extend(encode_frame(&request("9")).unwrap());
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        serve(&mut reader, &mut output, |_call| ExtResponse {
            v: BRIDGE_PROTOCOL_VERSION,
            msg_type: "response".into(),
            id: "9".into(),
            ok: true,
            method: None,
            error: None,
            payload: Value::String("opaque".into()),
        })
        .unwrap();

        let declared = decode_length([output[0], output[1], output[2], output[3]]).unwrap();
        let response: ExtResponse = decode_payload(&output[4..4 + declared]).unwrap();
        assert_eq!(response.payload, Value::String("opaque".into()));
    }
}
