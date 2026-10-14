import SwiftUI

/// M8 备份与设置（A6、S1/S2/S4/S8；主屏五 tab 之「设置」）。
///
/// 文案口径：同步项只写「设备同步（局域网）」，不出现"云同步"（docs/功能点全景.md §5）。
/// 薄入口：备份格式、最后验证时间、恢复演练判定在 core；本层只呈现与转发。
/// 纪律：设置项只存布尔 / 秒数（AppSettings）；秘密不进 UserDefaults。
struct SettingsBackupView: View {
    let environment: AppEnvironment
    let onLock: () -> Void

    @State private var health: HealthReport?
    @State private var autoLockMinutes = 5
    @State private var confirmBeforeFill = false
    @State private var scheduledBackup = false
    @State private var systemNotifications = false
    @State private var quickUnlockEnabled = true

    var body: some View {
        PageScaffold(screenId: "M8", title: "备份与设置", functionPoints: "A6、S1/S2/S4/S8") {
            Form {
                Section("备份（A6）") {
                    LabeledContent("手动备份（A6.1）", value: "待接入")
                    LabeledContent("最后验证时间（A6.7）", value: "待接入")
                    Toggle("定时备份（A6.3）", isOn: $scheduledBackup)
                        .onChange(of: scheduledBackup) { _, value in
                            environment.settings.scheduledBackup = value
                        }
                    LabeledContent("恢复演练（A6.8）", value: "待接入")
                    LabeledContent("恢复 / 导入（A6.9）", value: "待接入")
                    Text("备份格式、加密、校验与恢复演练语义在 core；A6 接口不在 FFI 公开面（见 README 待确认）。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Section("同步") {
                    LabeledContent(syncItemLabel, value: health?.syncReachable == true ? "可达" : "不可达")
                    Text("跨设备只有：同局域网直传 + 面对面 QR + 离线传送包。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Section("安全（S1/S2/S4/S8）") {
                    Toggle("生物识别快速解锁（S1，仅解锁因子）", isOn: $quickUnlockEnabled)
                        .onChange(of: quickUnlockEnabled) { _, value in
                            if value == false { environment.quickUnlock.clear() }
                        }
                    Stepper("空闲自动锁：\(autoLockMinutes) 分钟（S2）", value: $autoLockMinutes, in: 1...30)
                        .onChange(of: autoLockMinutes) { _, value in
                            environment.settings.autoLockSeconds = value * 60
                        }
                    Toggle("每次填充前确认（S4）", isOn: $confirmBeforeFill)
                        .onChange(of: confirmBeforeFill) { _, value in
                            environment.settings.confirmBeforeFill = value
                        }
                    Toggle("系统通知（S8）", isOn: $systemNotifications)
                        .onChange(of: systemNotifications) { _, value in
                            environment.settings.systemNotifications = value
                        }
                    Button("立即锁定（S7）") {
                        environment.core.vault().lock()
                        onLock()
                    }
                }

                Section("关于") {
                    LabeledContent("应用", value: "AI-Bridge")
                    Text("纯本地凭据与登录态管理器：数据只在本机加密存储。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("设置")
        .task {
            autoLockMinutes = max(1, environment.settings.autoLockSeconds / 60)
            confirmBeforeFill = environment.settings.confirmBeforeFill
            scheduledBackup = environment.settings.scheduledBackup
            systemNotifications = environment.settings.systemNotifications
            health = environment.core.state().health()
        }
    }
}
