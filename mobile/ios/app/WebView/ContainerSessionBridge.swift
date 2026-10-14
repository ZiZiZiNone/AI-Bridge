import Foundation
import WebKit
import aibridge

private let sourceID = "ios-container"

/// 容器会话桥（M-F2 / E1/E2 的 iOS 取数与注入点）。
///
/// 归属：宿主实现 core 的 `SessionSource` trait（core 通过构造注入调用；实现类不持有业务规则）。
/// 职责边界：把 WKHTTPCookieStore 的原始 cookie 原样搬运给 core（含 HttpOnly 与全部属性），
///           反向把 core 给出的 cookie 写回容器；cookie 属性完整性是本层唯一硬要求（零丢弃）。
/// 域名过滤（trait 参数语义）：按标准后缀匹配过滤，不做业务分级 / 去重 / 健康判定（全在 core）。
/// 线程：WKHTTPCookieStore 为异步 API，本实现以信号量桥接同步 trait；
///       core 侧应避免在主线程调用 capture / inject（见 README 未验证项）。
final class ContainerSessionBridge: SessionSource {
    private let cookieStore: WKHTTPCookieStore

    init(cookieStore: WKHTTPCookieStore) {
        self.cookieStore = cookieStore
    }

    func availableSources() -> [String] {
        [sourceID]
    }

    /// 读取指定域下的全部 cookie（含 HttpOnly），原样交 core 归一化为 SessionEnvelope。
    func capture(sourceId: String, domain: String) -> [Cookie] {
        let done = DispatchSemaphore(value: 0)
        var all: [HTTPCookie] = []
        cookieStore.getAllCookies { cookies in
            all = cookies
            done.signal()
        }
        _ = done.wait(timeout: .now() + 5)
        return all
            .filter { matches(domain: domain, cookieDomain: $0.domain) }
            .map { CookieFidelity.contractCookie(from: $0) }
    }

    /// 把 core 给出的 cookie 写回容器（PC 扩散来会话的场景）；属性零丢弃。
    func inject(sourceId: String, cookies: [Cookie]) {
        for cookie in cookies {
            guard let httpCookie = CookieFidelity.httpCookie(from: cookie) else { continue }
            let done = DispatchSemaphore(value: 0)
            cookieStore.setCookie(httpCookie) { done.signal() }
            _ = done.wait(timeout: .now() + 5)
        }
    }

    /// 标准 cookie 域匹配（host-only 或点前缀域）；适配层过滤，不含业务判断。
    private func matches(domain: String, cookieDomain: String) -> Bool {
        let host = domain.lowercased()
        let cookie = cookieDomain.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: "."))
        guard !host.isEmpty, !cookie.isEmpty else { return false }
        return host == cookie || host.hasSuffix("." + cookie)
    }
}
