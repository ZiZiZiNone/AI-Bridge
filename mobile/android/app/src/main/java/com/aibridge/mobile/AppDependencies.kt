package com.aibridge.mobile

import com.aibridge.mobile.core.AppSettings
import com.aibridge.mobile.core.BiometricGate
import com.aibridge.mobile.core.QuickUnlockStore
import com.aibridge.mobile.ffi.AibridgeCore
import com.aibridge.mobile.ffi.Clock
import com.aibridge.mobile.ffi.Entropy
import com.aibridge.mobile.ffi.SessionSource
import com.aibridge.mobile.ffi.Transport
import com.aibridge.mobile.ffi.VaultStore

/**
 * AI-Bridge · Android 组合根依赖集合。
 *
 * 只承载依赖引用：无单例访问器、无静态门面、无服务定位器；页面按需接收本结构（构造注入）。
 * `AibridgeCore` 与五个 core trait 类型来自 mobile/ffi/aibridge.udl 生成的 Kotlin 绑定
 * （包名见 mobile/ffi/uniffi.toml：`com.aibridge.mobile.ffi`；生成物不入库、禁止手改；
 * 生成物核对状态见 mobile/android/README.md「FFI 依赖与绑定生成物核对状态」）。
 * 五个 core trait 由宿主实现（core 经构造注入调用）：webview / core 目录下的实现类。
 * quickUnlock / biometricGate / settings 是宿主能力（非 core trait），与 iOS AppEnvironment 同构。
 */
data class AppDependencies(
    /** core 公开面：Vault / StateBus / Broker / SyncEngine / ConverterHost / AuditLog。 */
    val core: AibridgeCore,
    /** 宿主实现的 core traits（存储 / 会话来源 / 传输 / 时钟 / 熵）。 */
    val vaultStore: VaultStore,
    val sessionSource: SessionSource,
    val transport: Transport,
    val clock: Clock,
    val entropy: Entropy,
    /** 宿主能力：快速解锁因子存储（仅解锁因子）、生物识别提示、非秘密偏好。 */
    val quickUnlock: QuickUnlockStore,
    val biometricGate: BiometricGate,
    val settings: AppSettings,
)
