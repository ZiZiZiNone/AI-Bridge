import SwiftUI

/// AI-Bridge · iOS 宿主入口（薄入口）+ 组合根装配点。
///
/// 职责：装配组合根（bootstrapAppEnvironment → AppEnvironment）并渲染根视图；
/// 不含业务判断、不做数据转换、不做状态计算。
/// 依赖注入：一律构造注入并逐层显式传递；全仓禁止全局单例 / 静态门面 / 服务定位器。
/// 显示名一律 AI-Bridge。
@main
struct AIBridgeApp: App {
    let environment: AppEnvironment

    init() {
        do {
            environment = try bootstrapAppEnvironment()
        } catch {
            // 装配点 fail-fast：不以占位依赖静默启动（错误文本不含任何秘密）。
            fatalError("AI-Bridge 装配失败：\(error)")
        }
    }

    var body: some Scene {
        WindowGroup {
            RootFlowView(environment: environment)
        }
    }
}

/// M1 锁屏门（S1/S2）：锁定态显示解锁屏，解锁后进入 M2 主结构。
/// 空闲自动锁（S2 同口径）：以前后台切换时长判定（见 ponytail 注释），策略值来自 AppSettings。
struct RootFlowView: View {
    let environment: AppEnvironment
    @State private var unlocked = false
    @State private var backgroundedAt: Date?
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        Group {
            if unlocked {
                RootView(environment: environment, onLock: { lock() })
            } else {
                UnlockView(environment: environment, onUnlock: { unlocked = true })
            }
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .background:
                backgroundedAt = Date()
            case .active:
                // ponytail: 空闲判定只覆盖前后台切换时长；如需页面内交互级空闲计时再补。
                if let since = backgroundedAt,
                   Date().timeIntervalSince(since) > Double(environment.settings.autoLockSeconds) {
                    lock()
                }
                backgroundedAt = nil
            default:
                break
            }
        }
    }

    private func lock() {
        environment.core.vault().lock()
        unlocked = false
    }
}
