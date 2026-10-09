//! native messaging host 主循环（浏览器扩展 ⇄ 主程序，仅本地进程内 stdio）。
//!
//! 只做协议翻译：读帧 → 交注入的处理函数 → 写帧。
//! **不含业务判断**、不落盘、不记明文；处理函数经参数注入（core 的 Transport / SessionSource
//! 实现在 core 落地后由此处转发），本层不持有任何全局状态。
//!
//! 未验证：stdio 真实往返未运行（本机无 cargo）；`serve` 的纯逻辑单测已写但未运行。

use std::io::{Read, Write};

use super::ext_protocol::{
    check_length, decode_length, decode_payload, encode_frame, ExtProtocolError, ExtRequest,
    ExtResponse,
};

/// 宿主退出原因。
#[derive(Debug)]
pub enum MessagingHostError {
    /// stdio 读写失败
    Io(std::io::Error),
    /// 帧结构不合法
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

/// 逐帧处理循环（与 stdio 解耦，可直接用内存读写单测）。
///
/// 退出语义：对端关闭（EOF）视为正常退出，返回 `Ok(())`。
pub fn serve<R: Read, W: Write, F>(
    reader: &mut R,
    writer: &mut W,
    mut handle: F,
) -> Result<(), MessagingHostError>
where
    F: FnMut(ExtRequest) -> ExtResponse,
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
        let response = handle(request);

        writer.write_all(&encode_frame(&response)?)?;
        writer.flush()?;
    }
}

/// 读取 4 字节帧头；对端在帧边界处关闭返回 `false`。
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

/// 启动 stdio 宿主（由浏览器拉起本进程后调用，见 main.rs 的参数分流）。
///
/// 骨架：处理函数只回「未接入 core」；任何业务动作都等 core 落地后经注入进入本处，
/// 本函数不写业务规则。
pub fn run_stdio() -> Result<(), MessagingHostError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    serve(&mut reader, &mut writer, |request| ExtResponse {
        id: request.id,
        ok: false,
        error: Some("桥骨架未接入 core".to_string()),
        payload: serde_json::Value::Null,
    })
}

#[cfg(test)]
mod tests {
    // 未验证：本机无 cargo，未运行。
    use super::*;
    use crate::bridge::ext_protocol::{encode_frame, ExtRequest};
    use serde_json::{json, Value};
    use std::io::Cursor;

    fn request(id: &str) -> ExtRequest {
        ExtRequest {
            id: id.into(),
            action: "list_sessions".into(),
            payload: json!({}),
        }
    }

    fn echo(call: ExtRequest) -> ExtResponse {
        ExtResponse {
            id: call.id,
            ok: true,
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

        // 两帧响应：读回长度前缀解析出的两条 JSON
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
        // 协议翻译层不解读 payload：任意 JSON 原样回传
        let mut input = Vec::new();
        input.extend(encode_frame(&request("9")).unwrap());
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        serve(&mut reader, &mut output, |_call| ExtResponse {
            id: "9".into(),
            ok: true,
            error: None,
            payload: Value::String("opaque".into()),
        })
        .unwrap();

        let declared = decode_length([
            output[0], output[1], output[2], output[3],
        ])
        .unwrap();
        let response: ExtResponse = decode_payload(&output[4..4 + declared]).unwrap();
        assert_eq!(response.payload, Value::String("opaque".into()));
    }
}
