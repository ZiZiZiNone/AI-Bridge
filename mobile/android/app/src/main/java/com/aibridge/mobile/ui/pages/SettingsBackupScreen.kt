package com.aibridge.mobile.ui.pages

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.core.SYNC_ITEM_LABEL
import com.aibridge.mobile.ui.PageScaffold

/**
 * M8 备份与设置（A6、S1/S2/S4/S8；主屏五 tab 之「设置」）。
 *
 * 文案口径：同步项只写「设备同步（局域网）」，不出现"云同步"（docs/功能点全景.md §5）。
 * 薄入口：备份格式、最后验证时间、恢复演练判定在 core；本层只呈现与转发。
 * 纪律：设置项只存布尔 / 秒数（AppSettings）；秘密不进 SharedPreferences。
 * 待确认：A6 备份接口不在 UDL 公开面 —— 手动备份 / 最后验证时间 / 恢复演练 / 恢复导入
 *         显示「待接入」（与 iOS 同口径）。
 */
@Composable
fun SettingsBackupScreen(
    dependencies: AppDependencies,
    onLock: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var autoLockMinutes by remember { mutableIntStateOf(5) }
    var confirmBeforeFill by remember { mutableStateOf(false) }
    var scheduledBackup by remember { mutableStateOf(false) }
    var systemNotifications by remember { mutableStateOf(false) }
    // P0-2：默认值与持久化偏好一致（默认关），不先真后假闪跳。
    var quickUnlockEnabled by remember { mutableStateOf(dependencies.settings.quickUnlockEnabled) }
    var syncReachable by remember { mutableStateOf(false) }
    var showAutoFill by remember { mutableStateOf(false) }

    if (showAutoFill) {
        SystemAutoFillScreen(
            dependencies = dependencies,
            onBack = { showAutoFill = false },
            modifier = Modifier.fillMaxWidth(),
        )
        return
    }

    PageScaffold(screenId = "M8", title = "备份与设置（设备同步（局域网））", functionPoints = "A6、S1/S2/S4/S8", modifier = modifier) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            Text(text = "备份（A6）", style = MaterialTheme.typography.titleMedium)
            valueRow("手动备份（A6.1）", "待接入")
            valueRow("最后验证时间（A6.7）", "待接入")
            Row(verticalAlignment = Alignment.CenterVertically) {
                Switch(
                    checked = scheduledBackup,
                    onCheckedChange = {
                        scheduledBackup = it
                        dependencies.settings.scheduledBackup = it
                    },
                )
                Text("定时备份（A6.3）")
            }
            valueRow("恢复演练（A6.8）", "待接入")
            valueRow("恢复 / 导入（A6.9）", "待接入")
            Text(
                text = "备份格式、加密、校验与恢复演练语义在 core；A6 接口不在 FFI 公开面（见 README 待确认）。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "同步", style = MaterialTheme.typography.titleMedium)
            valueRow(SYNC_ITEM_LABEL, if (syncReachable) "可达" else "不可达")
            Text(
                text = "跨设备只有：同局域网直传 + 面对面 QR + 离线传送包。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "安全（S1/S2/S4/S8）", style = MaterialTheme.typography.titleMedium)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Switch(
                    checked = quickUnlockEnabled,
                    onCheckedChange = {
                        quickUnlockEnabled = it
                        dependencies.settings.quickUnlockEnabled = it
                        if (!it) dependencies.quickUnlock.clear()
                    },
                )
                Text("生物识别快速解锁（S1，仅解锁因子）")
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedButton(onClick = { if (autoLockMinutes > 1) autoLockMinutes -= 1 }) { Text("−") }
                Text(
                    text = "空闲自动锁：$autoLockMinutes 分钟（S2）",
                    modifier = Modifier.padding(horizontal = 8.dp),
                )
                OutlinedButton(onClick = {
                    if (autoLockMinutes < 30) autoLockMinutes += 1
                }) { Text("＋") }
            }
            LaunchedEffect(autoLockMinutes) {
                dependencies.settings.autoLockSeconds = autoLockMinutes * 60
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Switch(
                    checked = confirmBeforeFill,
                    onCheckedChange = {
                        confirmBeforeFill = it
                        dependencies.settings.confirmBeforeFill = it
                    },
                )
                Text("每次填充前确认（S4）")
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Switch(
                    checked = systemNotifications,
                    onCheckedChange = {
                        systemNotifications = it
                        dependencies.settings.systemNotifications = it
                    },
                )
                Text("系统通知（S8）")
            }
            Button(onClick = {
                dependencies.core.vault().lock()
                onLock()
            }) { Text("立即锁定（S7）") }

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "系统集成", style = MaterialTheme.typography.titleMedium)
            OutlinedButton(onClick = { showAutoFill = true }) { Text("系统自动填充（E4 / M9）") }

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "关于", style = MaterialTheme.typography.titleMedium)
            valueRow("应用", "AI-Bridge")
            Text(
                text = "纯本地凭据与登录态管理器：数据只在本机加密存储。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }

    LaunchedEffect(Unit) {
        autoLockMinutes = maxOf(1, dependencies.settings.autoLockSeconds / 60)
        confirmBeforeFill = dependencies.settings.confirmBeforeFill
        scheduledBackup = dependencies.settings.scheduledBackup
        systemNotifications = dependencies.settings.systemNotifications
        quickUnlockEnabled = dependencies.settings.quickUnlockEnabled
        syncReachable = dependencies.core.state().health().syncReachable
    }
}

@Composable
private fun valueRow(label: String, value: String) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
        Text(text = label, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
