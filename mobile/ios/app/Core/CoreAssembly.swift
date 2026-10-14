import Foundation
import WebKit
import aibridge

/// 组合根装配（全仓唯一装配逻辑）：构造宿主 trait 实现 → 调用 FFI 构造 core。
///
/// 调用点：`AIBridgeApp.init` 与两个扩展各自的入口（扩展是独立进程，各自实例化；
/// 无全局单例 / 静态门面 / 服务定位器）。
/// 依赖方向：宿主 → core 公开面（mobile/ffi 生成绑定）→ 契约；core 永不依赖本目录任何类型。
/// 参数：`sessionSource` 允许扩展传入各自的 SessionSource 实现（Safari 扩展 ≠ 容器），
///      缺省为容器桥（WKWebView 容器）。
/// 骨架阶段策略：装配失败即 fail-fast（调用点 fatalError），不以占位依赖静默启动。
func bootstrapAppEnvironment(sessionSource overrideSessionSource: SessionSource? = nil) throws -> AppEnvironment {
    let dataDir = try coreDataDirectory()
    let vaultStore = KeychainVaultStore(service: "com.aibridge.mobile.vault-store")
    let sessionSource = overrideSessionSource
        ?? ContainerSessionBridge(cookieStore: WKWebsiteDataStore.default().httpCookieStore)
    let transport = LanTransport()
    let clock = SystemClock()
    let entropy = SystemEntropy()
    let config = CoreConfig(dataDir: dataDir.path, displayName: "AI-Bridge")
    let core = newCore(
        config: config,
        vaultStore: vaultStore,
        sessionSource: sessionSource,
        transport: transport,
        clock: clock,
        entropy: entropy
    )
    return AppEnvironment(
        core: core,
        vaultStore: vaultStore,
        sessionSource: sessionSource,
        transport: transport,
        clock: clock,
        entropy: entropy,
        quickUnlock: QuickUnlockStore(),
        settings: AppSettings()
    )
}

/// core 数据目录（core 内部布局用；不透明字节本身经 VaultStore 注入）。
private func coreDataDirectory() throws -> URL {
    let base = try FileManager.default.url(
        for: .applicationSupportDirectory,
        in: .userDomainMask,
        appropriateFor: nil,
        create: true
    )
    let dir = base.appendingPathComponent("aibridge", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
}
