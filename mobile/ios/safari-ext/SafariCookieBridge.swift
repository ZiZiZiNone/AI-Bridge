import Foundation
import aibridge

private let sourceID = "safari-web-extension"

/// Safari 会话桥（M-F2 / E1/E2 的 Safari 侧取数与注入点；与容器共享同一契约 Cookie 形态）。
///
/// 归属：宿主实现 core 的 `SessionSource` trait；cookie 属性完整性是本层唯一硬要求
/// （A2 保真度：name/value/domain/path/secure/http_only/same_site/expires_at 零丢弃）。
/// 通信形态：Safari Web Extension 由 JS 侧驱动（iOS 上原生侧无法主动推送消息给扩展 JS），
///           捕获 = JS 读 cookie 后经 native message 交付；注入 = JS 拉取本桥暂存的 cookie 后写回。
/// 载荷字段：与 contracts/session-envelope.schema.json 的 cookie 字段逐字段一致（snake_case）。
final class SafariCookieBridge: SessionSource {
    private let lock = NSLock()
    private var captured: [Cookie] = []
    private var pendingInject: [Cookie] = []

    func availableSources() -> [String] {
        [sourceID]
    }

    /// core 取数：返回 JS 最近交付的捕获结果（属性全量，未做任何过滤 / 裁剪）。
    func capture(sourceId: String, domain: String) -> [Cookie] {
        lock.lock()
        defer { lock.unlock() }
        return captured
    }

    /// core 注入：暂存待写回 cookie，由 JS 下次拉取后写入 Safari（属性全量）。
    func inject(sourceId: String, cookies: [Cookie]) {
        lock.lock()
        defer { lock.unlock() }
        pendingInject = cookies
    }

    // MARK: - JS 侧调用面（经 SafariWebExtensionHandler 转发）

    func deliverCapture(_ cookies: [Cookie]) {
        lock.lock()
        defer { lock.unlock() }
        captured = cookies
    }

    func takePendingInject() -> [Cookie] {
        lock.lock()
        defer { lock.unlock() }
        let cookies = pendingInject
        pendingInject = []
        return cookies
    }
}

/// 契约 Cookie ⇄ JSON（snake_case，跨端载荷口径）；逐字段全量映射，零丢弃。
enum SafariCookieJSON {
    static func cookie(from json: [String: Any]) -> Cookie? {
        guard let name = json["name"] as? String, let value = json["value"] as? String else { return nil }
        return Cookie(
            name: name,
            value: value,
            domain: json["domain"] as? String,
            path: json["path"] as? String,
            secure: json["secure"] as? Bool,
            httpOnly: json["http_only"] as? Bool,
            sameSite: sameSite(from: json["same_site"] as? String),
            expiresAt: json["expires_at"] as? String
        )
    }

    static func json(from cookie: Cookie) -> [String: Any] {
        var json: [String: Any] = [
            "name": cookie.name,
            "value": cookie.value,
        ]
        if let domain = cookie.domain { json["domain"] = domain }
        if let path = cookie.path { json["path"] = path }
        if let secure = cookie.secure { json["secure"] = secure }
        if let httpOnly = cookie.httpOnly { json["http_only"] = httpOnly }
        if let sameSite = cookie.sameSite { json["same_site"] = sameSiteString(sameSite) }
        if let expiresAt = cookie.expiresAt { json["expires_at"] = expiresAt }
        return json
    }

    private static func sameSite(from raw: String?) -> SameSite? {
        switch raw?.lowercased() {
        case "strict": return SameSite.strict
        case "lax": return SameSite.lax
        case "none": return SameSite.none
        default: return nil
        }
    }

    private static func sameSiteString(_ sameSite: SameSite) -> String {
        switch sameSite {
        case SameSite.strict: return "strict"
        case SameSite.lax: return "lax"
        case SameSite.none: return "none"
        }
    }
}
