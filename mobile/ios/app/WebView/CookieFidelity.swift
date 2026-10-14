import Foundation
import aibridge

/// Cookie 保真度映射（A2）：契约 Cookie ↔ HTTPCookie，8 字段逐一携带，任何环节零丢弃。
///
/// 字段（contracts/*.schema.json / core/contract）：name / value / domain / path /
/// secure / http_only / same_site / expires_at。丢任一属性 = 会话损坏，故两向映射均为全字段。
/// 使用方：ContainerSessionBridge（容器会话捕获 / 注入）；Safari Web Extension 的同口径映射
/// 在 safari-ext/SafariCookieBridge.swift + background.js（WebExtensions cookies API 形态）。
enum CookieFidelity {
    /// HTTPCookie → 契约 Cookie。
    static func contractCookie(from http: HTTPCookie) -> Cookie {
        Cookie(
            name: http.name,
            value: http.value,
            domain: http.domain,
            path: http.path,
            secure: http.isSecure,
            httpOnly: http.isHTTPOnly,
            sameSite: sameSite(from: http),
            expiresAt: http.expiresDate.map { isoFormatter.string(from: $0) }
        )
    }

    /// 契约 Cookie → HTTPCookie（属性全量写入 properties；映射失败返回 nil 由 core 记审计）。
    static func httpCookie(from cookie: Cookie) -> HTTPCookie? {
        var properties: [HTTPCookiePropertyKey: Any] = [
            .name: cookie.name,
            .value: cookie.value,
            .domain: cookie.domain ?? "",
            .path: cookie.path ?? "/",
        ]
        if cookie.secure == true { properties[.secure] = "TRUE" }
        if cookie.httpOnly == true { properties[HTTPCookiePropertyKey("HttpOnly")] = "TRUE" }
        if let sameSite = cookie.sameSite {
            properties[HTTPCookiePropertyKey("SameSite")] = sameSiteString(sameSite)
        }
        if let expiresAt = cookie.expiresAt, let date = isoFormatter.date(from: expiresAt) {
            properties[.expires] = date
            properties[.discard] = "FALSE"
        } else {
            properties[.discard] = "TRUE"
        }
        return HTTPCookie(properties: properties)
    }

    /// same_site 取值（契约枚举 strict / lax / none）；平台属性缺失时为 nil，不臆造。
    private static func sameSite(from http: HTTPCookie) -> SameSite? {
        guard let raw = http.properties?[HTTPCookiePropertyKey("SameSite")] as? String else { return nil }
        switch raw.lowercased() {
        case "strict": return SameSite.strict
        case "lax": return SameSite.lax
        case "none": return SameSite.none
        default: return nil
        }
    }

    private static func sameSiteString(_ sameSite: SameSite) -> String {
        switch sameSite {
        case SameSite.strict: return "Strict"
        case SameSite.lax: return "Lax"
        case SameSite.none: return "None"
        }
    }

    /// expires_at 采用契约 date-time（ISO-8601，UTC）。
    private static let isoFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter
    }()
}
