package com.aibridge.mobile

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
 * `AibridgeCore` 与其类型来自 mobile/ffi/aibridge.udl 生成的 Kotlin 绑定（包名见 uniffi.toml）。
 * 五个 trait 由宿主实现（core 经构造注入调用），实现归属：webview / credential / 适配层。
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
)
