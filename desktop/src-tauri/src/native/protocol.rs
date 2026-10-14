//! `aibridge://` 自定义协议（D3 反向唤起）—— 宿主入口，不是 core 注入面。
//!
//! 分工：本文件只做「URI → 动作」的机械解析与注册表登记；动作如何落到窗口由 `ui::window`
//! 与 `ui::panels` 决定，涉及会话与凭据的判断一律在 core。
//!
//! 待确认（Win32 语义）：注册写 `HKCU\Software\Classes`（免管理员）；多用户 / 便携安装的
//! 协议归属策略未与产品约定。
//!
//! 验证状态：`parse` 与注册表写入/删除实跑单测（临时 scratch 键）；真实浏览器唤起链路未验证。

use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_WRITE, REG_CREATE_KEY_DISPOSITION, REG_OPEN_CREATE_OPTIONS, REG_SZ,
    RRF_RT_REG_SZ,
};

use super::NativeError;

/// 协议动作（机械映射，不含业务判断）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolAction {
    /// `aibridge://candidate` —— 唤起候选条（B6.1）
    ShowCandidate,
    /// `aibridge://panel/<短名>` —— 打开面板（短名见 `ui::panels::PANEL_LABELS` 去掉 `panel-` 前缀）
    ShowPanel(String),
    /// `aibridge://pair` —— 打开配对向导（E3）
    Pair,
}

/// 协议 scheme（注册表与链接生成共用同一常量，避免两处硬编码）。
pub const SCHEME: &str = "aibridge";

/// 解析 `aibridge://` URI。
///
/// 未知主机 / 空串 / 非法短名一律返回 `None`（由宿主静默忽略，不 panic、不刷日志）。
pub fn parse(uri: &str) -> Option<ProtocolAction> {
    let rest = uri.strip_prefix("aibridge://")?;
    let rest = rest.trim_end_matches('/');
    let mut parts = rest.splitn(2, '/');
    let host = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    match host {
        "candidate" => Some(ProtocolAction::ShowCandidate),
        "pair" => Some(ProtocolAction::Pair),
        "panel"
            if !path.is_empty()
                && path
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') =>
        {
            Some(ProtocolAction::ShowPanel(path.to_string()))
        }
        _ => None,
    }
}

/// `shell\open\command` 的命令行：带引号的 exe + 透传 URI 参数（纯函数，便于单测）。
pub fn open_command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// 注册协议（安装期或首次运行写 HKCU，免管理员权限）。
pub fn register() -> Result<(), NativeError> {
    let exe = std::env::current_exe().map_err(|_| NativeError::new(2u32, "可执行文件路径获取失败"))?;
    register_under(
        HKEY_CURRENT_USER,
        protocol_subkey(SCHEME),
        &open_command(&exe),
    )
}

/// 反注册协议（卸载期）。
pub fn unregister() -> Result<(), NativeError> {
    unregister_under(HKEY_CURRENT_USER, protocol_subkey(SCHEME))
}

fn protocol_subkey(scheme: &str) -> String {
    format!("Software\\Classes\\{scheme}")
}

/// NUL 结尾 UTF-16 缓冲（Win32 字符串参数的最小载体；`w!` 宏只收字面量）。
struct WideCString(Vec<u16>);

impl WideCString {
    fn new(text: &str) -> Self {
        let mut buffer: Vec<u16> = text.encode_utf16().collect();
        buffer.push(0);
        Self(buffer)
    }

    fn as_pcwstr(&self) -> PCWSTR {
        PCWSTR(self.0.as_ptr())
    }
}

/// REG_SZ 值字节（UTF-16 LE + 结尾 NUL）。
fn reg_sz_bytes(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for wide in text.encode_utf16() {
        bytes.extend_from_slice(&wide.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

/// 写协议键：<subkey>（default = "URL:AI-Bridge"、"URL Protocol" = ""）
/// + <subkey>\shell\open\command（default = 命令行）。
fn register_under(root: HKEY, subkey: String, command: &str) -> Result<(), NativeError> {
    let subkey_w = WideCString::new(&subkey);
    let command_key_path = WideCString::new("shell\\open\\command");
    let url_protocol = WideCString::new("URL Protocol");
    let description = reg_sz_bytes("URL:AI-Bridge");
    let empty = reg_sz_bytes("");
    let command_bytes = reg_sz_bytes(command);
    let mut disposition = REG_CREATE_KEY_DISPOSITION(0);

    // SAFETY：句柄由 RegCreateKeyExW 分配，本函数负责 RegCloseKey；宽字符串缓冲
    // 生命周期覆盖各自调用。失败路径同样关句柄。
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            root,
            subkey_w.as_pcwstr(),
            None,
            PCWSTR::null(),
            REG_OPEN_CREATE_OPTIONS(0),
            KEY_WRITE,
            None,
            &mut key,
            Some(&mut disposition),
        )
        .ok()
        .map_err(|error| NativeError::new(error.code().0 as u32, "协议键创建失败"))?;

        let mut result = (|| {
            RegSetValueExW(key, PCWSTR::null(), None, REG_SZ, Some(&description))
                .ok()
                .map_err(|error| NativeError::new(error.code().0 as u32, "协议描述写入失败"))?;
            RegSetValueExW(key, url_protocol.as_pcwstr(), None, REG_SZ, Some(&empty))
                .ok()
                .map_err(|error| NativeError::new(error.code().0 as u32, "协议标记写入失败"))?;

            let mut command_key = HKEY::default();
            RegCreateKeyExW(
                key,
                command_key_path.as_pcwstr(),
                None,
                PCWSTR::null(),
                REG_OPEN_CREATE_OPTIONS(0),
                KEY_WRITE,
                None,
                &mut command_key,
                None,
            )
            .ok()
            .map_err(|error| NativeError::new(error.code().0 as u32, "协议命令键创建失败"))?;

            let write = RegSetValueExW(command_key, PCWSTR::null(), None, REG_SZ, Some(&command_bytes))
                .ok()
                .map_err(|error| NativeError::new(error.code().0 as u32, "协议命令写入失败"));
            let close = RegCloseKey(command_key)
                .ok()
                .map_err(|error| NativeError::new(error.code().0 as u32, "协议命令键关闭失败"));
            write.and(close)
        })();

        let close = RegCloseKey(key)
            .ok()
            .map_err(|error| NativeError::new(error.code().0 as u32, "协议键关闭失败"));
        if result.is_ok() {
            result = close;
        }
        result
    }
}

/// 删除协议键（整棵子树）。
fn unregister_under(root: HKEY, subkey: String) -> Result<(), NativeError> {
    let subkey_w = WideCString::new(&subkey);
    // SAFETY：RegDeleteTreeW 删除整棵子树；键不存在（错误码 2）视为已完成。
    unsafe {
        let status = RegDeleteTreeW(root, subkey_w.as_pcwstr());
        if status.0 == 0 || status.0 == 2 {
            Ok(())
        } else {
            Err(NativeError::new(status.0, "协议键删除失败"))
        }
    }
}

/// 读取注册表字符串值（单测与自检用；读失败返回 None）。
fn read_string(root: HKEY, subkey: &str, value_name: PCWSTR) -> Option<String> {
    let subkey_w = WideCString::new(subkey);
    let mut size = 0u32;
    // SAFETY：两段式读取（先取长度再取数据），缓冲按 size 分配。
    unsafe {
        let status = RegGetValueW(
            root,
            subkey_w.as_pcwstr(),
            value_name,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        );
        if status.0 != 0 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        let status = RegGetValueW(
            root,
            subkey_w.as_pcwstr(),
            value_name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut size),
        );
        if status.0 != 0 {
            return None;
        }
        let wide: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text: String = wide
            .iter()
            .take_while(|wide| **wide != 0)
            .map(|wide| char::from_u32(*wide as u32).unwrap_or_default())
            .collect();
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_targets() {
        assert_eq!(parse("aibridge://candidate"), Some(ProtocolAction::ShowCandidate));
        assert_eq!(parse("aibridge://pair"), Some(ProtocolAction::Pair));
        assert_eq!(
            parse("aibridge://panel/library"),
            Some(ProtocolAction::ShowPanel("library".into()))
        );
        assert_eq!(
            parse("aibridge://panel/library/"),
            Some(ProtocolAction::ShowPanel("library".into()))
        );
    }

    #[test]
    fn parse_rejects_unknown_or_dirty_input() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("aibridge://"), None);
        assert_eq!(parse("https://example.com"), None);
        assert_eq!(parse("aibridge://panel/"), None);
        assert_eq!(parse("aibridge://panel/Library!"), None);
        assert_eq!(parse("aibridge://unknown"), None);
    }

    #[test]
    fn open_command_quotes_exe_and_passes_uri() {
        let command = open_command(Path::new("C:\\Program Files\\AI-Bridge\\aibridge.exe"));
        assert_eq!(
            command,
            "\"C:\\Program Files\\AI-Bridge\\aibridge.exe\" \"%1\""
        );
    }

    #[test]
    fn registry_register_read_unregister_roundtrip() {
        // scratch 键（不碰真实协议键）：注册 → 读回 → 删除。
        let subkey = format!("Software\\Classes\\aibridge-test-{}", std::process::id());
        register_under(HKEY_CURRENT_USER, subkey.clone(), "\"app.exe\" \"%1\"").expect("注册失败");

        assert_eq!(
            read_string(HKEY_CURRENT_USER, &subkey, PCWSTR::null()).as_deref(),
            Some("URL:AI-Bridge"),
            "default 值应为协议描述"
        );
        assert_eq!(
            read_string(
                HKEY_CURRENT_USER,
                &format!("{subkey}\\shell\\open\\command"),
                PCWSTR::null(),
            )
            .as_deref(),
            Some("\"app.exe\" \"%1\""),
            "shell\\open\\command 应为命令行"
        );

        unregister_under(HKEY_CURRENT_USER, subkey.clone()).expect("删除失败");
        assert!(
            read_string(HKEY_CURRENT_USER, &subkey, PCWSTR::null()).is_none(),
            "删除后 default 值应不存在"
        );
    }
}
