package com.aibridge.mobile.ui.pages

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.core.errorText
import com.aibridge.mobile.ffi.CredentialKind
import com.aibridge.mobile.ffi.Entry
import com.aibridge.mobile.ffi.EntrySummary
import com.aibridge.mobile.ui.PageScaffold
import java.util.UUID

/**
 * M3 条目详情 + 编辑表单（A1.1、B5.1、E5 深链跳转）。
 *
 * 薄入口：字段校验、去重、强度 / 体检判定全在 core；本屏只收集表单、转发
 * putEntry / deleteEntry / totp 并渲染结果。类型仅 账号密码 / passkey / TOTP
 * （不设支付信息 / 身份分类）。
 * 待确认：新建条目 account_ref 的分配语义（UDL 无 allocate API）——现由宿主生成 UUID 暂代
 *         （与 iOS 同口径）。
 */
@Composable
fun EntryDetailScreen(
    dependencies: AppDependencies,
    summary: EntrySummary?,
    onOpenInContainer: (String) -> Unit,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var accountRef by remember { mutableStateOf("") }
    var title by remember { mutableStateOf("") }
    var kind by remember { mutableStateOf(CredentialKind.PASSWORD) }
    var username by remember { mutableStateOf("") }
    var secret by remember { mutableStateOf("") }
    var totpSeed by remember { mutableStateOf("") }
    var urlString by remember { mutableStateOf("") }
    var note by remember { mutableStateOf("") }
    var totpCode by remember { mutableStateOf("") }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    val context = LocalContext.current

    PageScaffold(
        screenId = "M3",
        title = "条目详情 + 编辑表单",
        functionPoints = "A1.1、B5.1、深链跳转（E5）",
        modifier = modifier,
    ) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            Row {
                TextButton(onClick = onDone) { Text("返回") }
                Spacer(modifier = Modifier.weight(1f))
                Button(onClick = {
                    try {
                        val entry = Entry(
                            accountRef = accountRef,
                            title = title,
                            kind = kind,
                            username = username.takeIf { it.isNotEmpty() },
                            secret = secret.takeIf { it.isNotEmpty() },
                            totpSeed = totpSeed.takeIf { it.isNotEmpty() },
                            url = urlString.takeIf { it.isNotEmpty() },
                            note = note.takeIf { it.isNotEmpty() },
                        )
                        dependencies.core.vault().putEntry(entry)
                        errorMessage = null
                        onDone()
                    } catch (e: Exception) {
                        errorMessage = "保存失败（${errorText(e)}）"
                    }
                }) { Text("保存") }
            }

            Text(text = "条目", style = MaterialTheme.typography.titleMedium)

            OutlinedTextField(
                value = title,
                onValueChange = { title = it },
                label = { Text("标题") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            Text(text = "类型", style = MaterialTheme.typography.bodySmall)
            Row(verticalAlignment = Alignment.CenterVertically) {
                RadioButton(selected = kind == CredentialKind.PASSWORD, onClick = { kind = CredentialKind.PASSWORD })
                Text("账号密码")
                RadioButton(selected = kind == CredentialKind.PASSKEY, onClick = { kind = CredentialKind.PASSKEY })
                Text("passkey")
                RadioButton(selected = kind == CredentialKind.TOTP, onClick = { kind = CredentialKind.TOTP })
                Text("TOTP")
            }

            OutlinedTextField(
                value = username,
                onValueChange = { username = it },
                label = { Text("用户名") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = secret,
                onValueChange = { secret = it },
                label = { Text("密码 / 秘钥") },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = totpSeed,
                onValueChange = { totpSeed = it },
                label = { Text("TOTP 种子") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = urlString,
                onValueChange = { urlString = it },
                label = { Text("URL") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = note,
                onValueChange = { note = it },
                label = { Text("备注") },
                modifier = Modifier.fillMaxWidth(),
            )

            if (username.isNotEmpty() || secret.isNotEmpty()) {
                Spacer(modifier = Modifier.height(8.dp))
                Text(text = "一次性复制（B5.1）", style = MaterialTheme.typography.titleMedium)
                if (username.isNotEmpty()) {
                    OutlinedButton(onClick = {
                        clipboard(context).setPrimaryClip(ClipData.newPlainText("用户名", username))
                    }) { Text("复制用户名") }
                }
                if (secret.isNotEmpty()) {
                    OutlinedButton(onClick = {
                        clipboard(context).setPrimaryClip(ClipData.newPlainText("密码 / 秘钥", secret))
                    }) { Text("复制密码 / 秘钥") }
                }
            }

            if (summary != null) {
                Spacer(modifier = Modifier.height(8.dp))
                Text(text = "验证码（B7.1）", style = MaterialTheme.typography.titleMedium)
                OutlinedButton(onClick = {
                    try {
                        totpCode = dependencies.core.vault().totp(accountRef)
                        errorMessage = null
                    } catch (e: Exception) {
                        errorMessage = "获取验证码失败（${errorText(e)}）"
                    }
                }) { Text("获取 TOTP 验证码") }
                if (totpCode.isNotEmpty()) {
                    Text(text = totpCode, style = MaterialTheme.typography.titleLarge)
                }

                Spacer(modifier = Modifier.height(8.dp))
                Text(text = "跳转（E5 / M-F5）", style = MaterialTheme.typography.titleMedium)
                // M-F5：库条目「填充并打开」→ 深链启动目标，系统自动填充接管（b 级）。
                OutlinedButton(onClick = {
                    try {
                        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(urlString)))
                    } catch (_: Exception) {
                        errorMessage = "URL 无效"
                    }
                }) { Text("填充并打开") }
                // M-F5：容器内目标直接带态打开（c 级）。
                OutlinedButton(onClick = {
                    if (urlString.isEmpty()) {
                        errorMessage = "URL 无效"
                    } else {
                        errorMessage = null
                        onOpenInContainer(urlString)
                    }
                }) { Text("在容器中打开（c 级）") }
            }

            errorMessage?.let { message ->
                Spacer(modifier = Modifier.height(8.dp))
                Text(text = message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }

            if (summary != null) {
                Spacer(modifier = Modifier.height(8.dp))
                OutlinedButton(onClick = {
                    try {
                        dependencies.core.vault().deleteEntry(accountRef)
                        onDone()
                    } catch (e: Exception) {
                        errorMessage = "删除失败（${errorText(e)}）"
                    }
                }) { Text("删除") }
            }
        }
    }

    LaunchedEffect(summary?.accountRef) {
        if (summary == null) {
            // 新建：account_ref 分配语义待确认（UDL 无 allocate API）——暂由宿主生成 UUID。
            if (accountRef.isEmpty()) accountRef = UUID.randomUUID().toString()
        } else {
            accountRef = summary.accountRef
            try {
                val entry = dependencies.core.vault().getEntry(summary.accountRef)
                title = entry.title
                kind = entry.kind
                username = entry.username ?: ""
                secret = entry.secret ?: ""
                totpSeed = entry.totpSeed ?: ""
                urlString = entry.url ?: ""
                note = entry.note ?: ""
            } catch (e: Exception) {
                errorMessage = "加载失败（${errorText(e)}）"
            }
        }
    }
}

private fun clipboard(context: android.content.Context): ClipboardManager =
    context.getSystemService(ClipboardManager::class.java)
