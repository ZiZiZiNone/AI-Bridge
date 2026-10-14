package com.aibridge.mobile.core

import android.content.Context
import android.webkit.CookieManager
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ffi.CoreConfig
import com.aibridge.mobile.ffi.SessionSource
import com.aibridge.mobile.ffi.newCore
import com.aibridge.mobile.webview.ContainerSessionBridge
import java.io.File

/**
 * 组合根装配（全仓唯一装配逻辑）：构造宿主 trait 实现 → 调用 FFI 构造 core。
 *
 * 调用点：MainActivity（App）与两个系统服务（AutofillService / CredentialProviderService），
 * 各实例分别装配（无全局单例 / 静态门面 / 服务定位器；与 iOS CoreAssembly 同构）。
 * 依赖方向：宿主 → core 公开面（mobile/ffi 生成绑定）→ 契约；core 永不依赖本目录任何类型。
 * 参数：`overrideSessionSource` 允许调用方传入各自的 SessionSource 实现，缺省为容器桥
 *      （Android WebView CookieManager / cookie 库）。
 * 骨架阶段策略：装配失败即 fail-fast（调用点不捕获），不以占位依赖静默启动。
 *
 * 绑定命名待生成后核对：`newCore(...)` 按 UDL 命名空间函数 `new_core` 的 UniFFI 惯例生成为
 * 包级自由函数（Kotlin 参数名取 camelCase）。
 */
fun bootstrapAppDependencies(context: Context, overrideSessionSource: SessionSource? = null): AppDependencies {
    val appContext = context.applicationContext
    val filesDir = appContext.filesDir
    val vaultStore = KeystoreVaultStore(File(filesDir, "aibridge-vault-store"))
    val sessionSource = overrideSessionSource
        ?: ContainerSessionBridge(CookieManager.getInstance(), File(appContext.applicationInfo.dataDir))
    val transport = LanTransport(appContext)
    val clock = SystemClock()
    val entropy = SystemEntropy()
    val config = CoreConfig(
        dataDir = File(filesDir, "aibridge").apply { mkdirs() }.absolutePath,
        displayName = "AI-Bridge",
    )
    val core = newCore(
        config = config,
        vaultStore = vaultStore,
        sessionSource = sessionSource,
        transport = transport,
        clock = clock,
        entropy = entropy,
    )
    return AppDependencies(
        core = core,
        vaultStore = vaultStore,
        sessionSource = sessionSource,
        transport = transport,
        clock = clock,
        entropy = entropy,
        quickUnlock = QuickUnlockStore(File(filesDir, "aibridge-quick-unlock")),
        biometricGate = BiometricGate(appContext),
        settings = AppSettings(appContext.getSharedPreferences("aibridge-settings", Context.MODE_PRIVATE)),
    )
}
