package com.aibridge.mobile.core

import com.aibridge.mobile.ffi.Cookie
import com.aibridge.mobile.ffi.SameSite

/**
 * Cookie 保真编解码（M5 容器会话读写）。
 *
 * 严格拒绝含 `\r`、`\n`、`\u0000` 或非法分隔符 `;` 的字段，防止 Set-Cookie 属性/头部注入。
 */
object CookieFidelity {

    private val FORBIDDEN_CHARS = charArrayOf('\r', '\n', '\u0000')

    private fun requireCleanToken(field: String, value: String, forbidSemicolon: Boolean = true) {
        if (value.any { it in FORBIDDEN_CHARS }) {
            throw IllegalArgumentException("Cookie $field 含非法控制字符（CR/LF/NUL）")
        }
        if (forbidSemicolon && value.contains(';')) {
            throw IllegalArgumentException("Cookie $field 不得包含分号 ';'")
        }
    }

    fun toSetCookieHeader(cookie: Cookie): String {
        if (cookie.name.isBlank() || cookie.name.contains('=')) {
            throw IllegalArgumentException("Cookie name 不能为空且不得包含 '='")
        }
        requireCleanToken("name", cookie.name, forbidSemicolon = true)
        requireCleanToken("value", cookie.value, forbidSemicolon = true)

        val parts = mutableListOf("${cookie.name}=${cookie.value}")

        cookie.domain?.takeIf { it.isNotEmpty() }?.let { domain ->
            requireCleanToken("domain", domain, forbidSemicolon = true)
            parts.add("Domain=$domain")
        }
        cookie.path?.takeIf { it.isNotEmpty() }?.let { path ->
            requireCleanToken("path", path, forbidSemicolon = true)
            parts.add("Path=$path")
        }
        cookie.expiresAt?.takeIf { it.isNotEmpty() }?.let { expires ->
            requireCleanToken("expires_at", expires, forbidSemicolon = true)
            parts.add("Expires=$expires")
        }
        if (cookie.secure == true) {
            parts.add("Secure")
        }
        if (cookie.httpOnly == true) {
            parts.add("HttpOnly")
        }
        cookie.sameSite?.let { sameSite ->
            val token = when (sameSite) {
                SameSite.STRICT -> "Strict"
                SameSite.LAX -> "Lax"
                SameSite.NONE -> "None"
            }
            parts.add("SameSite=$token")
        }
        return parts.joinToString("; ")
    }
}
