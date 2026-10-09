package com.aibridge.mobile.webview

import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Android WebView 容器骨架（M5 容器浏览器 / M-F2）。
 *
 * 职责边界：承载页面、暴露导航回调；会话捕获与归一化不在本层（见 ContainerSessionBridge.kt）。
 * 依赖注入：导航回调以参数传入；无单例 / 静态门面。
 * 存储策略：使用 WebView 默认持久化 CookieManager——会话捕获要求可读取 cookie（含 HttpOnly），
 *           不使用"用过即丢"的临时存储，避免隐性会话丢失。
 * 边界：本容器是 App 内容器，不涉及 Android Chrome 会话（无扩展通道，明确不支持）。
 */
@Composable
fun ContainerWebView(
    url: String,
    onNavigate: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    AndroidView(
        modifier = modifier,
        factory = { context ->
            WebView(context).apply {
                settings.javaScriptEnabled = true
                cookieManager.setAcceptCookie(true)
                webViewClient = object : WebViewClient() {
                    override fun onPageFinished(view: WebView?, finishedUrl: String?) {
                        super.onPageFinished(view, finishedUrl)
                        finishedUrl?.let(onNavigate)
                    }
                }
                loadUrl(url)
            }
        },
        onRelease = { webView -> webView.destroy() },
    )
}
