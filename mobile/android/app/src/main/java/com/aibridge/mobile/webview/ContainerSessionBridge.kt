package com.aibridge.mobile.webview

import android.database.sqlite.SQLiteDatabase
import android.webkit.CookieManager
import com.aibridge.mobile.ffi.Cookie
import com.aibridge.mobile.ffi.SessionSource
import java.io.File

private const val SOURCE_ID = "android-container"

/**
 * 容器会话桥（M-F2 / E1/E2 的 Android 取数与注入点）。
 *
 * 归属：宿主实现 core 的 `SessionSource` trait（core 通过构造注入调用；实现类不持有业务规则）。
 * 职责边界：把容器 cookie 原样搬运给 core，反向把 core 给出的 cookie 写回；
 *           cookie 属性完整性是本层唯一的硬要求（不得丢弃 HttpOnly / Secure / SameSite / 到期）。
 * 边界：Android Chrome 无扩展通道，其会话不在此列（本桥仅服务 App 内 WebView 容器）。
 *
 * 捕获通道（平台差异，见 README 出入说明）：
 *  1) 主路径 —— WebView cookie 库（App 私有 SQLite）直读：8 字段全量（含 HttpOnly）；
 *  2) 回退 —— `CookieManager.getCookie`：平台公开 API 仅返回 name/value 视图，属性为 null
 *     （不臆造；库不可读 / schema 变化时触发，schema 映射待真机核对）。
 * 注入通道：`CookieManager.setCookie` 以 Set-Cookie 形态值串写入，8 字段全量携带；
 *          契约 Cookie 缺 domain 时无法寻址容器存储（与 iOS `guard … else continue` 同口径跳过，
 *          由 core 记审计）。
 * 域名过滤（trait 参数语义）：按标准后缀匹配过滤，不做业务分级 / 去重 / 健康判定（全在 core）。
 */
class ContainerSessionBridge(
    private val cookieManager: CookieManager,
    private val webViewDataDir: File,
) : SessionSource {

    override fun availableSources(): List<String> = listOf(SOURCE_ID)

    /** 读取指定域下的全部 cookie（含 HttpOnly 与全部属性），原样交 core 归一化为 SessionEnvelope。 */
    override fun capture(sourceId: String, domain: String): List<Cookie> {
        readCookieStore(domain)?.let { return it }
        val raw = cookieManager.getCookie("https://$domain") ?: return emptyList()
        return parseCookiePairs(raw).map { (name, value) ->
            Cookie(
                name = name,
                value = value,
                domain = null,
                path = null,
                secure = null,
                httpOnly = null,
                sameSite = null,
                expiresAt = null,
            )
        }
    }

    /** 把 core 给出的 cookie 写回容器（PC 扩散来会话的场景）；属性零丢弃。 */
    override fun inject(sourceId: String, cookies: List<Cookie>) {
        for (cookie in cookies) {
            val host = cookie.domain?.trim('.')?.takeIf { it.isNotBlank() } ?: continue
            cookieManager.setCookie("https://$host${cookie.path ?: "/"}", cookieHeaderValue(cookie))
        }
        cookieManager.flush()
    }

    // ---- 主路径：WebView cookie 库直读（8 字段全量） ----

    private fun readCookieStore(domain: String): List<Cookie>? {
        val dbFile = COOKIE_DB_CANDIDATES
            .map { File(webViewDataDir, it) }
            .firstOrNull { it.isFile }
            ?: return null
        return try {
            SQLiteDatabase.openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
                queryCookies(db, domain)
            }
        } catch (_: Exception) {
            null // 库被占用 / schema 不符 → 回退名称值视图（属性为 null，不臆造）
        }
    }

    private fun queryCookies(db: SQLiteDatabase, domain: String): List<Cookie> {
        val columns = resolveColumns(db) ?: return emptyList()
        val result = mutableListOf<Cookie>()
        db.rawQuery(
            "SELECT name, value, host_key, path, expires_utc, ${columns.secure}, ${columns.httpOnly}, ${columns.sameSite} FROM cookies",
            null,
        ).use { cursor ->
            while (cursor.moveToNext()) {
                val name = cursor.getString(0) ?: continue
                val cookieDomain = cursor.getString(2) ?: continue
                if (!matchesDomain(domain, cookieDomain)) continue
                result += Cookie(
                    name = name,
                    value = cursor.getString(1) ?: "",
                    domain = cookieDomain,
                    path = cursor.getString(3),
                    secure = cursor.getInt(5) != 0,
                    httpOnly = cursor.getInt(6) != 0,
                    sameSite = sameSiteFromChromeInt(cursor.getInt(7)),
                    expiresAt = isoFromChromeMicros(cursor.getLong(4)),
                )
            }
        }
        return result
    }

    /** 按库内实际列名解析属性列（新旧 schema 的 secure / httponly 列名不同）。 */
    private fun resolveColumns(db: SQLiteDatabase): CookieColumns? {
        val names = mutableSetOf<String>()
        db.rawQuery("PRAGMA table_info(cookies)", null).use { cursor ->
            val nameIndex = cursor.getColumnIndex("name")
            while (cursor.moveToNext()) {
                cursor.getString(nameIndex)?.let { names += it }
            }
        }
        val secure = listOf("is_secure", "secure").firstOrNull { it in names } ?: return null
        val httpOnly = listOf("is_httponly", "httponly").firstOrNull { it in names } ?: return null
        val sameSite = listOf("samesite", "same_site").firstOrNull { it in names } ?: return null
        return CookieColumns(secure = secure, httpOnly = httpOnly, sameSite = sameSite)
    }

    /** 标准 cookie 域匹配（host-only 或点前缀域）；适配层过滤，不含业务判断。 */
    private fun matchesDomain(domain: String, cookieDomain: String): Boolean {
        val host = domain.lowercase()
        val cookie = cookieDomain.lowercase().trim('.')
        if (host.isEmpty() || cookie.isEmpty()) return false
        return host == cookie || host.endsWith(".$cookie")
    }

    private data class CookieColumns(val secure: String, val httpOnly: String, val sameSite: String)
}

private val COOKIE_DB_CANDIDATES = listOf("app_webview/Default/Cookies", "app_webview/Cookies")
