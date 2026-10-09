//! `aibridge://` 自定义协议（D3 反向唤起）—— 宿主入口，不是 core 注入面。
//!
//! 分工：本文件只做「URI → 动作」的机械解析；动作如何落到窗口由 `ui::window`
//! 与 `ui::panels` 决定，涉及会话与凭据的判断一律在 core。
//!
//! 未验证：`register` / `unregister` 为 todo!() 骨架（需写注册表，未编译验证）；
//! `parse` 是纯函数，单测已写但本机无 cargo 未运行。

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

/// 注册协议（安装期或首次运行写 HKCU，免管理员权限）。
pub fn register() -> Result<(), NativeError> {
    todo!("写 HKCU\\Software\\Classes\\aibridge\\shell\\open\\command")
}

/// 反注册协议（卸载期）。
pub fn unregister() -> Result<(), NativeError> {
    todo!("删除 HKCU\\Software\\Classes\\aibridge")
}

#[cfg(test)]
mod tests {
    // 未验证：本机无 cargo，未运行。
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
}
