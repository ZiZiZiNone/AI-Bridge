package com.aibridge.mobile.webview

import com.aibridge.mobile.ffi.Cookie
import com.aibridge.mobile.ffi.SameSite
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/**
 * Cookie 保真度映射（A2）：契约 Cookie 的 8 字段（name / value / domain / path /
 * secure / http_only / same_site / expires_at）逐一携带，任何环节零丢弃。
 *
 * 使用方：ContainerSessionBridge（容器会话捕获 / 注入）。
 * 与 iOS `app/WebView/CookieFidelity.swift` 同口径的纯函数映射（无状态、无依赖获取）。
 * 纪律：平台读不到的属性以 null 表达（不臆造值）；注入侧 8 字段全量写入 Set-Cookie 形态值串。
 * 待确认：WebView cookie 库 `samesite` 整数码表按 Chromium CookieSameSite（-1 未指定 /
 *         0 None / 1 Lax / 2 Strict）映射，待真机核对（见 README）。
 */

private const val ISO_PATTERN = "yyyy-MM-dd'T'HH:mm:ss'Z'"
private const val HTTP_DATE_PATTERN = "EEE, dd MMM yyyy HH:mm:ss 'GMT'"

/** Chromium 时间戳（自 1601-01-01 UTC 的微秒数）→ 契约 date-time（ISO-8601，UTC）。 */
fun isoFromChromeMicros(microsSince1601: Long): String? {
    if (microsSince1601 <= 0L) return null // 会话级 cookie，无到期时间
    val unixMillis = microsSince1601 / 1000L - 11644473600000L
    return isoFormatter().format(Date(unixMillis))
}

/** 契约 same_site 整数码表（Chromium CookieSameSite）→ 枚举；未指定 / 未知返回 null（不臆造）。 */
fun sameSiteFromChromeInt(raw: Int): SameSite? = when (raw) {
    0 -> SameSite.NONE
    1 -> SameSite.LAX
    2 -> SameSite.STRICT
    else -> null
}

/** 契约 same_site 枚举 → Set-Cookie 属性值（闭合枚举，逐一映射）。 */
fun sameSiteLabel(sameSite: SameSite): String = when (sameSite) {
    SameSite.STRICT -> "Strict"
    SameSite.LAX -> "Lax"
    SameSite.NONE -> "None"
}

/** 契约 Cookie → Set-Cookie 形态值串（`CookieManager.setCookie` 逐属性写入，零丢弃）。 */
private fun containsControlOrDelimiter(text: String): Boolean =
    text.any { it == '\r' || it == '\n' || it == '\u0000' || it == ';' }

fun cookieHeaderValue(cookie: Cookie): String {
    require(!containsControlOrDelimiter(cookie.name) && !cookie.name.contains('=')) {
        "非法 cookie 名称"
    }
    require(!containsControlOrDelimiter(cookie.value)) {
        "非法 cookie 值"
    }
    val parts = mutableListOf("${cookie.name}=${cookie.value}")
    cookie.domain?.takeIf { it.isNotBlank() }?.let { parts += "Domain=$it" }
    parts += "Path=${cookie.path?.takeIf { it.isNotBlank() } ?: "/"}"
    cookie.expiresAt?.let { iso -> httpDateFromIso(iso)?.let { parts += "Expires=$it" } }
    if (cookie.secure == true) parts += "Secure"
    if (cookie.httpOnly == true) parts += "HttpOnly"
    cookie.sameSite?.let { parts += "SameSite=${sameSiteLabel(it)}" }
    return parts.joinToString("; ")
}

/** `CookieManager.getCookie` 的名称值视图解析（仅 name / value；属性由其余通道携带）。 */
fun parseCookiePairs(raw: String): List<Pair<String, String>> =
    raw.split(';')
        .mapNotNull { segment ->
            val index = segment.indexOf('=')
            if (index <= 0) return@mapNotNull null
            segment.substring(0, index).trim() to segment.substring(index + 1).trim()
        }

/** 契约 date-time（ISO-8601，UTC）→ HTTP 日期（Set-Cookie Expires 属性）。 */
fun httpDateFromIso(iso: String): String? = try {
    httpDateFormatter().format(isoFormatter().parse(iso) ?: return null)
} catch (_: Exception) {
    null // 无法解析的到期时间不写入（不臆造）；其余属性照常携带
}

private fun isoFormatter(): SimpleDateFormat =
    SimpleDateFormat(ISO_PATTERN, Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") }

private fun httpDateFormatter(): SimpleDateFormat =
    SimpleDateFormat(HTTP_DATE_PATTERN, Locale.US).apply { timeZone = TimeZone.getTimeZone("GMT") }
