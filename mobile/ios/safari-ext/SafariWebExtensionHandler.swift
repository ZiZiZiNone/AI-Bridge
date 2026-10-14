import Foundation

/// M-F2 / E1/E2：Safari Web Extension 原生 handler（SafariServices · NSExtensionRequestHandling）。
///
/// 职责边界（薄入口）：只做「JS 消息 ⇄ core 公开面」的协议翻译；业务判断全在 core。
/// 消息形态（与 background.js 约定；字段名待与扩展资源包联调校准，未验证）：
///   {type: "capture", domain: <string>, cookies: [<契约 cookie json>]}  → {ok: true}
///   {type: "pull-inject"}                                               → {cookies: [<契约 cookie json>]}
///   {type: "handshake"}                                                 → {ok: true, source: <string>}
/// cookie 属性：逐字段全量往返（A2 保真度，零丢弃），映射见 SafariCookieJSON。
final class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {

    /// 本扩展的 SessionSource 实现（Safari 侧取数 / 注入桥）。
    private let bridge = SafariCookieBridge()

    private lazy var environment: AppEnvironment = {
        do {
            return try bootstrapAppEnvironment(sessionSource: bridge)
        } catch {
            fatalError("Safari 扩展装配失败：\(error)")
        }
    }()

    func beginRequest(with context: NSExtensionContext) {
        let message = extractMessage(from: context.inputItems)
        let response: [String: Any]

        switch message?["type"] as? String {
        case "capture":
            let cookies = (message?["cookies"] as? [[String: Any]] ?? [])
                .compactMap { SafariCookieJSON.cookie(from: $0) }
            bridge.deliverCapture(cookies)
            // 归一化与审计在 core；本层不解析载荷语义。
            if let domain = message?["domain"] as? String {
                _ = try? environment.core.state().capture(sourceId: "safari-web-extension", domain: domain)
            }
            response = ["ok": true]
        case "pull-inject":
            let cookies = bridge.takePendingInject().map { SafariCookieJSON.json(from: $0) }
            response = ["cookies": cookies]
        case "handshake":
            response = ["ok": true, "source": "safari-web-extension"]
        default:
            response = ["ok": false, "reason": "unknown-type"]
        }

        let item = NSExtensionItem()
        item.userInfo = ["response": response]
        context.completeRequest(returningItems: [item], completionHandler: nil)
    }

    /// 消息提取：优先 userInfo["message"]（runtime.sendNativeMessage 形态），
    /// 其次附件 property list / JSON（extension.sendNativeMessage 形态）。最终形态待联调校准（未验证）。
    private func extractMessage(from inputItems: [Any]) -> [String: Any]? {
        guard let item = inputItems.first as? NSExtensionItem else { return nil }
        if let message = item.userInfo?["message"] as? [String: Any] {
            return message
        }
        guard let provider = item.attachments?.first else { return nil }
        var result: [String: Any]?
        let semaphore = DispatchSemaphore(value: 0)
        provider.loadItem(forTypeIdentifier: "public.property-list", options: nil) { loaded, _ in
            if let dictionary = loaded as? [String: Any] {
                result = dictionary["message"] as? [String: Any] ?? dictionary
            }
            semaphore.signal()
        }
        _ = semaphore.wait(timeout: .now() + 5)
        return result
    }
}
