package com.aibridge.mobile.ui.pages

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PlaceholderScreen

/**
 * M9 系统自动填充 UI（页面空壳 / 系统层说明位）。
 *
 * M9 的实际形态由系统渲染：Android 侧为 AutofillService
 * （见 ../../autofill/AIBridgeAutofillService.kt）与 CredentialProviderService
 * （见 ../../credential/AIBridgeCredentialProviderService.kt），不是 App 内页面。
 * 本文件只保留 App 内的入口位（跳转系统设置说明）与编号锚点，避免 M1–M9 编号出现缺口。
 * 边界：仅账号类条目（密码 / passkey / 验证码），不设支付信息 / 身份分类。
 */
@Composable
fun SystemAutoFillScreen(dependencies: AppDependencies, modifier: Modifier = Modifier) {
    PlaceholderScreen(
        screenId = "M9",
        title = "系统自动填充 UI（系统层：AutofillService / CredentialProviderService）",
        functionPoints = "E4",
        modifier = modifier,
    )
}
