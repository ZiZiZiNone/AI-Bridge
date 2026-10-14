package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M8 备份与设置（页面空壳；主屏 tab「设置」）。
 *
 * 文案口径：同步项只写「设备同步（局域网）」；不得出现"云同步"（docs/功能点全景.md §5）。
 * 薄入口：备份格式、最后验证时间、恢复演练判定在 core；本层只呈现与转发。
 */
@Composable
fun SettingsBackupScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(
        screenId = "M8",
        title = "备份与设置（设备同步（局域网））",
        functionPoints = "A6、S1/S2/S4/S8",
        modifier = modifier,
    )
}
