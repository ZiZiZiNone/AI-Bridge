//! `aibridge://` 自定义协议解析与注册（D3 反向唤起）。

use super::NativeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolAction {
    /// `aibridge://candidate` —— 唤起候选条（B6.1）
    ShowCandidate,
    /// `aibridge://panel/<短名>` —— 打开面板
    ShowPanel(String),
    /// `aibridge://pair` —— 打开配对向导（E3）
    Pair,
}

pub const SCHEME: &str = "aibridge";

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

pub fn register_command_line(exe_path: &str) -> Result<String, NativeError> {
    if exe_path.trim().is_empty() || exe_path.contains('"') || exe_path.contains('\0') {
        return Err(NativeError::new(0x8007_0057, "可执行文件路径非法"));
    }
    Ok(format!("\"{exe_path}\" \"%1\""))
}

pub fn register() -> Result<(), NativeError> {
    let exe = std::env::current_exe()
        .map_err(|_| NativeError::new(0x8007_0002, "无法获取当前可执行文件路径"))?;
    let _cmd = register_command_line(&exe.to_string_lossy())?;
    Ok(())
}

pub fn unregister() -> Result<(), NativeError> {
    Ok(())
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
    fn command_line_rejects_quotes_and_empty() {
        assert!(register_command_line("").is_err());
        assert!(register_command_line("C:\\bad\"path.exe").is_err());
        assert_eq!(
            register_command_line("C:\\Program Files\\AI-Bridge\\ai-bridge.exe").unwrap(),
            "\"C:\\Program Files\\AI-Bridge\\ai-bridge.exe\" \"%1\""
        );
    }
}
