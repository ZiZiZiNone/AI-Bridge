package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M6 状态 / 临时会话（页面空壳；浏览器 tab 的次级页面）。
 *
 * 含新建会话表单：域名白名单、只读 / 单次、TTL。
 * 薄入口：TTL 上限、白名单校验、撤销语义在 core（`ab-broker`）；本层只收集表单并展示状态。
 */
@Composable
fun SessionStatusScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(
        screenId = "M6",
        title = "状态 / 临时会话（含新建会话表单）",
        functionPoints = "A2、C1–C5",
        modifier = modifier,
    )
}
