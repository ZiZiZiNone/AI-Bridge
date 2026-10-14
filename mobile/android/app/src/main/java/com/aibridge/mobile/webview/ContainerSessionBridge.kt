package com.aibridge.mobile.webview

import android.webkit.CookieManager
import com.aibridge.mobile.core.CookieFidelity
import com.aibridge.mobile.ffi.Cookie
import com.aibridge.mobile.ffi.SessionSource

private const val SOURCE_ID = "android-container"

/**
 * 容器会话桥（M-F2 / E1/E2 的 Android 取数与注入点）。
 *
 * 写入时统一经过 `CookieFidelity.toSetCookieHeader` 校验，拒绝含 CR/LF/NUL 或分号注入的畸形属性。
 */
class ContainerSessionBridge(
    private val cookieManager: CookieManager,
) : SessionSource {

    override fun availableSources(): List<String> = listOf(SOURCE_ID)

    override fun capture(sourceId: String, domain: String): List<Cookie> {
        val cleanDomain = domain.trim().trimStart('.')
        if (sourceId != SOURCE_ID || cleanDomain.isEmpty()) {
            return emptyList()
        }
        val rawHeader = cookieManager.getCookie("https://$cleanDomain") ?: return emptyList()
        return rawHeader
            .split(';')
            .mapNotNull { segment ->
                val pair = segment.trim()
                val eq = pair.indexOf('=')
                if (eq <= 0) null
                else Cookie(
                    name = pair.substring(0, eq).trim(),
                    value = pair.substring(eq + 1).trim(),
                    domain = cleanDomain,
                    path = "/",
                    secure = true,
                    httpOnly = null,
                    sameSite = null,
                    expiresAt = null,
                )
            }
    }

    override fun inject(sourceId: String, cookies: List<Cookie>) {
        if (sourceId != SOURCE_ID) {
            throw IllegalArgumentException("未知的会话来源：$sourceId")
        }
        for (cookie in cookies) {
            val header = CookieFidelity.toSetCookieHeader(cookie)
            val host = cookie.domain?.trim()?.trimStart('.')?.takeIf { it.isNotEmpty() }
                ?: throw IllegalArgumentException("注入容器的 Cookie 必须提供 domain")
            val scheme = if (cookie.secure == false) "http" else "https"
            val path = cookie.path?.takeIf { it.startsWith("/") } ?: "/"
            cookieManager.setCookie("$scheme://$host$path", header)
        }
        cookieManager.flush()
    }
}
