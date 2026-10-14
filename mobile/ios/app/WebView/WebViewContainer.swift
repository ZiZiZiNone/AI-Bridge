import SwiftUI
import WebKit

/// M5 容器浏览器的 WKWebView 承载（M-F2）。
///
/// 职责边界：承载页面、暴露导航回调；会话捕获 / 注入在 ContainerSessionBridge（SessionSource 实现），
/// 归一化与扩散在 core。依赖注入：导航回调以参数传入；无单例 / 静态门面。
/// 存储策略：默认持久化 cookie 存储（WKWebsiteDataStore.default）——会话捕获要求可读 cookie
///           （含 HttpOnly 与属性），不使用"用过即丢"的临时存储，避免隐性会话丢失。
/// 边界：本容器是 App 内容器（不涉及其他浏览器会话通道）。
struct WebViewContainer: UIViewRepresentable {
    let url: URL
    let onNavigate: (String) -> Void

    func makeUIView(context: Context) -> WKWebView {
        let webView = WKWebView(frame: .zero)
        webView.navigationDelegate = context.coordinator
        context.coordinator.currentURL = url
        webView.load(URLRequest(url: url))
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        guard context.coordinator.currentURL != url else { return }
        context.coordinator.currentURL = url
        webView.load(URLRequest(url: url))
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onNavigate: onNavigate)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        let onNavigate: (String) -> Void
        var currentURL: URL?

        init(onNavigate: @escaping (String) -> Void) {
            self.onNavigate = onNavigate
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            guard let url = webView.url?.absoluteString else { return }
            currentURL = webView.url
            onNavigate(url)
        }
    }
}
