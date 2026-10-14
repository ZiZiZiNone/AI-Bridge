import aibridge

/// UI 文案口径集中处（便于审 docs/功能点全景.md §5 边界速查）。
///
/// 边界：条目类型只有 账号密码 / passkey / TOTP（不设支付信息 / 身份分类）；
/// 设备仅 Windows / iOS / Android（不出现 macOS）；同步文案只写「设备同步（局域网）」，
/// 全部 UI 不出现"云同步"。

/// 条目类型标签（A1：账号密码 / passkey / TOTP）。
func credentialKindLabel(_ kind: CredentialKind) -> String {
    switch kind {
    case .password: return "账号密码"
    case .passkey: return "passkey"
    case .totp: return "TOTP"
    @unknown default: return "账号类"
    }
}

/// 目标类型标签（会话矩阵列）。
func targetKindLabel(_ kind: TargetKind) -> String {
    switch kind {
    case .web: return "Web"
    case .app: return "App"
    case .container: return "容器"
    @unknown default: return "-"
    }
}

/// 来源标记（A2.3：每条会话标注来自哪个端 / 转换器 / 扩散事件）。
func sourceTypeLabel(_ source: SourceType) -> String {
    switch source {
    case .`extension`: return "扩展"
    case .converter: return "转换器"
    case .mobile: return "手机"
    case .manual: return "手动"
    case .sync: return syncItemLabel
    @unknown default: return "-"
    }
}

/// 设备类型标签（E3：设备仅 Windows / iOS / Android；core 侧枚举值原样映射）。
func deviceKindLabel(_ rawKind: String) -> String {
    switch rawKind.lowercased() {
    case "windows": return "Windows"
    case "ios": return "iOS"
    case "android": return "Android"
    default: return rawKind
    }
}

/// 同步项唯一文案（M8：禁"云同步"）。
let syncItemLabel = "设备同步（局域网）"
