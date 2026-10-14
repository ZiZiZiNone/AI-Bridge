import SwiftUI

/// M2 主屏骨架：底部五 tab（库 / 生成器 / 浏览器 / 设备 / 设置）。
///
/// 页面归属：库 → M2（M3 由其内导航进入）· 生成器 → M4 · 浏览器 → M5（M6 为其次级页面）·
///           设备 → M7 · 设置 → M8 · M1 为前置锁屏（RootFlowView），M9 由系统层自带形态
///           （Credential Provider Extension，不经本视图）。
/// 薄入口：本视图只做页面装配与依赖传递，无业务判断。
struct RootView: View {
    let environment: AppEnvironment
    let onLock: () -> Void

    @State private var selectedTab: Tab = .vault
    @State private var containerTarget: URL?

    private enum Tab: Hashable {
        case vault, generator, browser, devices, settings
    }

    var body: some View {
        TabView(selection: $selectedTab) {
            NavigationStack {
                VaultListView(environment: environment, onOpenInContainer: openInContainer)
            }
            .tabItem { Label("库", systemImage: "list.bullet") }
            .tag(Tab.vault)

            NavigationStack {
                GeneratorView(environment: environment)
            }
            .tabItem { Label("生成器", systemImage: "wand.and.stars") }
            .tag(Tab.generator)

            NavigationStack {
                ContainerBrowserView(environment: environment, initialURL: containerTarget)
            }
            .tabItem { Label("浏览器", systemImage: "globe") }
            .tag(Tab.browser)

            NavigationStack {
                PairingView(environment: environment)
            }
            .tabItem { Label("设备", systemImage: "qrcode") }
            .tag(Tab.devices)

            NavigationStack {
                SettingsBackupView(environment: environment, onLock: onLock)
            }
            .tabItem { Label("设置", systemImage: "gearshape") }
            .tag(Tab.settings)
        }
    }

    /// M3 → M5「在容器中打开」（E5 / M-F5 c 级）：切到浏览器 tab 并带址打开。
    private func openInContainer(_ url: URL) {
        containerTarget = url
        selectedTab = .browser
    }
}
