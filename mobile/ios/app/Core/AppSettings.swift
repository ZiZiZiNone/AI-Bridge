import Foundation

/// 非秘密偏好（S2 空闲锁 / S4 填充确认 / S8 系统集成 / A6.3 定时备份）。
///
/// 纪律：秘密（主口令、cookie 值、TOTP 种子、token）一律不进 UserDefaults（docs/安全模型.md §4）；
/// 本类只存布尔与秒数。实例由组合根构造并显式传递，不是单例。
final class AppSettings {
    private enum Key {
        static let autoLockSeconds = "settings.auto_lock_seconds"
        static let confirmBeforeFill = "settings.confirm_before_fill"
        static let scheduledBackup = "settings.scheduled_backup"
        static let systemNotifications = "settings.system_notifications"
        static let quickUnlockEnabled = "settings.quick_unlock_enabled"
    }

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    /// 空闲自动锁秒数（S2 同口径，默认 5 分钟）。
    var autoLockSeconds: Int {
        get {
            let stored = defaults.integer(forKey: Key.autoLockSeconds)
            return stored > 0 ? stored : 300
        }
        set { defaults.set(newValue, forKey: Key.autoLockSeconds) }
    }

    /// "每次填充前确认"开关（S4，偏执档）。
    var confirmBeforeFill: Bool {
        get { defaults.bool(forKey: Key.confirmBeforeFill) }
        set { defaults.set(newValue, forKey: Key.confirmBeforeFill) }
    }

    /// 定时备份开关（A6.3，可选，默认关）。
    var scheduledBackup: Bool {
        get { defaults.bool(forKey: Key.scheduledBackup) }
        set { defaults.set(newValue, forKey: Key.scheduledBackup) }
    }

    /// 系统通知开关（S8）。
    var systemNotifications: Bool {
        get { defaults.bool(forKey: Key.systemNotifications) }
        set { defaults.set(newValue, forKey: Key.systemNotifications) }
    }

    /// 生物识别快速解锁开关（S1，默认关）。
    ///
    /// 审查报告 P0-2 口径：解锁成功不得无条件保存主口令因子——只有用户显式
    /// 开启本开关后 QuickUnlockStore 才保存；关闭时调用方须同时 clear 因子。
    var quickUnlockEnabled: Bool {
        get { defaults.bool(forKey: Key.quickUnlockEnabled) }
        set { defaults.set(newValue, forKey: Key.quickUnlockEnabled) }
    }
}
