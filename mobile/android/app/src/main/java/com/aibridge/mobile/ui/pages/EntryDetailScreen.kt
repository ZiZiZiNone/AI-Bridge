package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M3 条目详情 + 编辑表单（页面空壳）。
 *
 * 薄入口：字段校验、类型切换（密码 / passkey / TOTP）、落库加密全部在 core。
 * 边界：条目类型仅账号密码 / passkey / TOTP / 备注，不含支付信息与身份分类。
 */
@Composable
fun EntryDetailScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(
        screenId = "M3",
        title = "条目详情 + 编辑表单",
        functionPoints = "A1.1、B5.1、E5",
        modifier = modifier,
    )
}
