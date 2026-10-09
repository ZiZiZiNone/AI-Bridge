package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M5 容器浏览器（全屏 WebView，页面空壳；主屏 tab「浏览器」）。
 *
 * 承载：登录 → 会话完整捕获 → 归一化 SessionEnvelope → 局域网直传 / A8 离线包（M-F2、E1/E2）。
 * 实现归属：WebView 容器见 ../../webview/ContainerWebView.kt，会话搬运见 ContainerSessionBridge.kt。
 * 薄入口：匹配、归一化、扩散决策在 core；本层只承载容器与用户动作。
 * 边界：Android Chrome 无扩展通道，其会话不在扩散范围（本页为 App 内容器，不受此限）。
 */
@Composable
fun ContainerBrowserScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(
        screenId = "M5",
        title = "容器浏览器（全屏 WebView）",
        functionPoints = "E1、E2",
        modifier = modifier,
    )
}
