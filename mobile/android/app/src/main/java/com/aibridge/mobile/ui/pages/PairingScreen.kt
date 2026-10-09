package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M7 配对与设备（页面空壳；主屏 tab「设备」）。
 *
 * 含指纹确认屏：双方指纹不一致必须拒绝（M-F4、E3）。
 * 设备仅 Windows / iOS / Android —— 不出现 macOS。
 * 薄入口：指纹计算与比对、设备列表状态在 core（`ab-sync`）。
 */
@Composable
fun PairingScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(
        screenId = "M7",
        title = "配对与设备（含指纹确认屏）",
        functionPoints = "E3、A8",
        modifier = modifier,
    )
}
