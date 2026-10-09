package com.aibridge.mobile.webview

import android.webkit.CookieManager
import com.aibridge.mobile.ffi.Cookie
import com.aibridge.mobile.ffi.SessionSource

private const val SOURCE_ID = "android-container"

/**
 * 容器会话桥（M-F2 / E1/E2 的 Android 取数与注入点）。
 *
 * 归属：宿主实现 core 的 `SessionSource` trait（core 通过构造注入调用；实现类不持有业务规则）。
 * 职责边界：把 CookieManager 的原始 cookie 原样搬运给 core，反向把 core 给出的 cookie 写回；
 *           cookie 属性完整性是本层唯一的硬要求（不得丢弃 HttpOnly / Secure / SameSite / 到期）。
 * 边界：Android Chrome 无扩展通道，其会话不在此列。
 */
class ContainerSessionBridge(
    private val cookieManager: CookieManager,
) : SessionSource {

    override fun availableSources(): List<String> = listOf(SOURCE_ID)

    /** 读取指定域下的全部 cookie，原样交 core 归一化为 SessionEnvelope。 */
    override fun capture(sourceId: String, domain: String): List<Cookie> {
        // 骨架：`CookieManager.getCookie(url)` 是同步字符串视图，正常路径需配合
        // androidx.webkit CookieManagerCompat 以取得含属性的 cookie 列表；属性映射由 core 负责。
        return emptyList()
    }

    /** 把 core 给出的 cookie 写回容器（PC 扩散来会话的场景）。 */
    override fun inject(sourceId: String, cookies: List<Cookie>) {
        // 骨架：逐条 setCookie 后由系统负责落盘；失败处理与重试语义在 core。
    }
}
