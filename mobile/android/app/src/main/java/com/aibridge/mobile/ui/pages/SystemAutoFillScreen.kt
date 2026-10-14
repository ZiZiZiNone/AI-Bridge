package com.aibridge.mobile.ui.pages

import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.view.autofill.AutofillManager
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.ui.PageScaffold

/**
 * M9 系统自动填充 UI（App 内说明 / 状态位；实际填充由系统层承接）。
 *
 * M9 的实际形态由系统渲染：Android 侧为 AutofillService（见
 * ../../autofill/AIBridgeAutofillService.kt）与 CredentialProviderService（见
 * ../../credential/AIBridgeCredentialProviderService.kt），不是 App 内页面。
 * 本屏只保留 App 内的编号锚点、系统设置入口与口径说明，避免 M1–M9 编号出现缺口。
 * 边界：仅账号类条目（账号密码 / passkey / TOTP），不设支付信息 / 身份分类。
 * 与 iOS 的出入：iOS 无 App 内 M9 说明位（M9 全部在 Credential Provider Extension）；
 * 本屏为 Android 侧补充落点（系统开关入口），见 README 对齐表。
 */
@Composable
fun SystemAutoFillScreen(
    dependencies: AppDependencies,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val autofillEnabled = context.getSystemService(AutofillManager::class.java)
        ?.hasEnabledAutofillServices() == true

    PageScaffold(
        screenId = "M9",
        title = "系统自动填充 UI（系统层：AutofillService / CredentialProviderService）",
        functionPoints = "E4",
        modifier = modifier,
    ) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            TextButton(onClick = onBack) { Text("返回") }

            Text(text = "系统填充只填账号类（账号密码 / passkey / TOTP），不设支付信息 / 身份分类。")

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "自动填充服务", style = MaterialTheme.typography.titleMedium)
            Row {
                Text(
                    text = "状态：",
                    style = MaterialTheme.typography.bodyMedium,
                )
                Text(
                    text = if (autofillEnabled) "已启用" else "未启用",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            OutlinedButton(onClick = {
                runCatching {
                    context.startActivity(
                        Intent(
                            Settings.ACTION_REQUEST_SET_AUTOFILL_SERVICE,
                            Uri.parse("package:${context.packageName}"),
                        ),
                    )
                }
            }) { Text("打开系统设置") }

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "凭据提供方（Android 14+）", style = MaterialTheme.typography.titleMedium)
            Text(
                text = "系统凭据入口由 CredentialProviderService 承接（Android 14+）；在系统设置 → 密码中启用 AI-Bridge。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Spacer(modifier = Modifier.height(8.dp))
            Text(
                text = "解锁 → 选账号 → 系统填入 → 用完即走（M-F3）。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
