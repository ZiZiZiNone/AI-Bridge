import aibridge

/// AI-Bridge · iOS 组合根依赖集合（构造注入，无单例）。
///
/// 只承载依赖引用：无 `static let shared`、无静态门面、无服务定位器；
/// 页面与扩展按需接收本结构（构造参数显式传递，不使用 @EnvironmentObject）。
/// `AibridgeCore` 与五个 trait 协议来自 mobile/ffi/aibridge.udl 生成的 Swift 绑定
/// （模块名见 mobile/ffi/uniffi.toml：`aibridge`；生成物不入库、禁止手改）。
struct AppEnvironment {
    /// core 公开面：Vault / StateBus / Broker / SyncEngine / ConverterHost / AuditLog。
    let core: AibridgeCore
    /// 宿主实现的 core traits（存储 / 会话来源 / 传输 / 时钟 / 熵），core 经构造注入调用。
    let vaultStore: VaultStore
    let sessionSource: SessionSource
    let transport: Transport
    let clock: Clock
    let entropy: Entropy
    /// 宿主能力（非 core trait）：快速解锁因子存储、非秘密偏好。
    let quickUnlock: QuickUnlockStore
    let settings: AppSettings
}
