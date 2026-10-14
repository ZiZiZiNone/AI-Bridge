package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M1 锁屏 / 解锁（页面空壳）。
 *
 * 薄入口：解锁因子（主密码 / 生物识别）、空闲自动锁策略、失败节流全部在 core；
 * 生物识别仅作解锁因子，主密码仍是唯一根（见 docs/流程/移动端流程.md M-F1）。
 */
@Composable
fun UnlockScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(screenId = "M1", title = "锁屏 / 解锁", functionPoints = "S1", modifier = modifier)
}
