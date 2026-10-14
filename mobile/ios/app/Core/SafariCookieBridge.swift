import Foundation

/// Safari 扩展与 WKWebView 容器 Cookie 桥（修补跨域混用缺陷）：
/// `capture` 按 `domain` 参数执行精确匹配或子域后缀匹配（`cookieDomain == target || cookieDomain.hasSuffix("." + target)`），
/// 严禁把上次交付的全量 Cookie 无过滤返回给其他域。
public final class SafariCookieBridge {

    public struct CookieItem: Equatable, Codable {
        public let name: String
        public let value: String
        public let domain: String?
        public let path: String?
        public let secure: Bool?
        public let httpOnly: Bool?
        public let sameSite: String?
        public let expiresAt: String?

        public init(
            name: String,
            value: String,
            domain: String? = nil,
            path: String? = nil,
            secure: Bool? = nil,
            httpOnly: Bool? = nil,
            sameSite: String? = nil,
            expiresAt: String? = nil
        ) {
            self.name = name
            self.value = value
            self.domain = domain
            self.path = path
            self.secure = secure
            self.httpOnly = httpOnly
            self.sameSite = sameSite
            self.expiresAt = expiresAt
        }
    }

    private let lock = NSLock()
    private var stagedCookies: [CookieItem] = []

    public init() {}

    public static func normalizeDomain(_ raw: String) -> String {
        var cleaned = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        while cleaned.hasPrefix(".") {
            cleaned.removeFirst()
        }
        return cleaned
    }

    /// 域名匹配规则（与 `core/broker` 一致）：精确相等或以 `.target` 结尾的子域。
    public static func matchesDomain(cookieDomain: String?, requestedDomain: String) -> Bool {
        let target = normalizeDomain(requestedDomain)
        guard !target.isEmpty, let rawCookieDomain = cookieDomain else {
            return false
        }
        let candidate = normalizeDomain(rawCookieDomain)
        guard !candidate.isEmpty else {
            return false
        }
        return candidate == target || candidate.hasSuffix("." + target)
    }

    public func updateStagedCookies(_ cookies: [CookieItem]) {
        lock.lock()
        defer { lock.unlock() }
        stagedCookies = cookies
    }

    public func capture(sourceId: String, domain: String) -> [CookieItem] {
        let target = Self.normalizeDomain(domain)
        guard !sourceId.isEmpty, !target.isEmpty else {
            return []
        }
        lock.lock()
        defer { lock.unlock() }
        return stagedCookies.filter { cookie in
            Self.matchesDomain(cookieDomain: cookie.domain, requestedDomain: target)
        }
    }
}
