package com.aibridge.mobile.core

import android.content.SharedPreferences

private const val KEY_AUTO_LOCK_SECONDS = "settings.auto_lock_seconds"
private const val KEY_CONFIRM_BEFORE_FILL = "settings.confirm_before_fill"
private const val KEY_SCHEDULED_BACKUP = "settings.scheduled_backup"
private const val KEY_SYSTEM_NOTIFICATIONS = "settings.system_notifications"
private const val KEY_QUICK_UNLOCK = "settings.quick_unlock_enabled"

/**
 * 非秘密偏好（S2 空闲锁 / S4 填充确认 / S8 系统集成 / A6.3 定时备份）。
 *
 * 纪律：秘密（主口令、cookie 值、TOTP 种子、token）一律不进 SharedPreferences
 * （docs/安全模型.md「内存与输出卫生」）；本类只存布尔与秒数。
 * 实例由组合根构造并显式传递，不是单例。
 */
class AppSettings(private val preferences: SharedPreferences) {

    /** 空闲自动锁秒数（S2 同口径，默认 5 分钟）。 */
    var autoLockSeconds: Int
        get() = preferences.getInt(KEY_AUTO_LOCK_SECONDS, 0).takeIf { it > 0 } ?: 300
        set(value) {
            preferences.edit().putInt(KEY_AUTO_LOCK_SECONDS, value).apply()
        }

    /** "每次填充前确认"开关（S4，偏执档）。 */
    var confirmBeforeFill: Boolean
        get() = preferences.getBoolean(KEY_CONFIRM_BEFORE_FILL, false)
        set(value) {
            preferences.edit().putBoolean(KEY_CONFIRM_BEFORE_FILL, value).apply()
        }

    /** 定时备份开关（A6.3，可选，默认关）。 */
    var scheduledBackup: Boolean
        get() = preferences.getBoolean(KEY_SCHEDULED_BACKUP, false)
        set(value) {
            preferences.edit().putBoolean(KEY_SCHEDULED_BACKUP, value).apply()
        }

    /** 系统通知开关（S8）。 */
    var systemNotifications: Boolean
        get() = preferences.getBoolean(KEY_SYSTEM_NOTIFICATIONS, false)
        set(value) {
            preferences.edit().putBoolean(KEY_SYSTEM_NOTIFICATIONS, value).apply()
        }

    /**
     * 生物识别快速解锁开关（S1，默认关）。
     *
     * 审查报告 P0-2：解锁成功不得无条件保存主口令因子——只有用户显式开启本开关后，
     * QuickUnlockStore 才会保存；关闭时调用方须同时 `QuickUnlockStore.clear()`。
     */
    var quickUnlockEnabled: Boolean
        get() = preferences.getBoolean(KEY_QUICK_UNLOCK, false)
        set(value) {
            preferences.edit().putBoolean(KEY_QUICK_UNLOCK, value).apply()
        }
}
