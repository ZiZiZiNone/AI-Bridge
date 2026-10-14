package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M4 生成器（页面空壳；主屏 tab「生成器」）。
 *
 * 口径：默认 20 位；**不存生成历史**（与明文纪律冲突，见 docs/功能点全景.md §5 边界速查）。
 * 薄入口：字符集、熵源、长度约束在 core（`ab-vault::generate` 经注入的 Entropy 取随机）。
 */
@Composable
fun GeneratorScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(
        screenId = "M4",
        title = "生成器（默认 20 位 · 不存历史）",
        functionPoints = "A5",
        modifier = modifier,
    )
}
