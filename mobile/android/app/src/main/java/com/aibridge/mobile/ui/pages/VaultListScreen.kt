package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M2 库列表 + 搜索（页面空壳；主屏 tab「库」）。
 *
 * 薄入口：检索、排序、过滤规则在 core；本层只把用户输入交给 core 并渲染返回的结构化结果。
 * 本页是 M3（条目详情 / 编辑表单）的导航起点，也是深链跳转（E5）的落地页。
 */
@Composable
fun VaultListScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(screenId = "M2", title = "库列表 + 搜索", functionPoints = "A1.2", modifier = modifier)
}
