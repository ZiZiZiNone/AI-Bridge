package com.aibridge.mobile.ui.pages

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.core.errorText
import com.aibridge.mobile.ui.PageScaffold

/**
 * M1 锁屏 / 解锁（S1；M-F1）。
 *
 * 薄入口：解锁（Argon2id 派生等）与失败节流全在 core，本屏只收集口令并转发 `Vault.unlock`；
 * 生物识别仅作解锁因子（QuickUnlockStore 保存因子），主密码仍是唯一根。
 * S2 空闲自动锁的计时门在 MainActivity（与 iOS RootFlowView 同口径）。
 * 纪律：主口令只经内存传递 —— 不写日志、不进 SharedPreferences、不进错误文本。
 */
@Composable
fun UnlockScreen(
    dependencies: AppDependencies,
    onUnlock: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var masterPassword by remember { mutableStateOf("") }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    // P0-2：仅当用户显式开启 S1 快速解锁开关时才显示/保存因子（默认关）。
    var biometricAvailable by remember { mutableStateOf(dependencies.settings.quickUnlockEnabled) }

    PageScaffold(screenId = "M1", title = "锁屏 / 解锁", functionPoints = "S1", modifier = modifier) {
        Column {
            OutlinedTextField(
                value = masterPassword,
                onValueChange = { masterPassword = it },
                label = { Text("主密码") },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                modifier = Modifier.fillMaxWidth(),
            )

            Spacer(modifier = Modifier.height(12.dp))

            Button(
                onClick = {
                    try {
                        dependencies.core.vault().unlock(masterPassword)
                        // 快速解锁因子仅保存在本设备（Keystore 包裹 + 生物识别绑定）；
                        // P0-2：仅当用户开启 S1 开关才保存，保存失败不影响解锁。
                        if (dependencies.settings.quickUnlockEnabled) {
                            runCatching { dependencies.quickUnlock.save(masterPassword) }
                        }
                        masterPassword = ""
                        errorMessage = null
                        onUnlock()
                    } catch (e: Exception) {
                        errorMessage = "解锁失败（${errorText(e)}）"
                    }
                },
                enabled = masterPassword.isNotEmpty(),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("解锁")
            }

            if (biometricAvailable) {
                Spacer(modifier = Modifier.height(8.dp))
                OutlinedButton(
                    onClick = {
                        dependencies.biometricGate.authenticate(
                            title = "生物识别解锁",
                            subtitle = "仅作解锁因子",
                            negativeLabel = "使用主密码",
                            onSuccess = {
                                try {
                                    val password = dependencies.quickUnlock.retrieveMasterPassword()
                                    dependencies.core.vault().unlock(password)
                                    errorMessage = null
                                    onUnlock()
                                } catch (e: Exception) {
                                    biometricAvailable = false
                                    errorMessage = "快速解锁失败，请使用主密码（${errorText(e)}）"
                                }
                            },
                            onError = { message ->
                                biometricAvailable = false
                                errorMessage = "快速解锁失败，请使用主密码（$message）"
                            },
                        )
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("生物识别解锁（仅解锁因子）")
                }
            }

            errorMessage?.let { message ->
                Spacer(modifier = Modifier.height(8.dp))
                Text(text = message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }

            Spacer(modifier = Modifier.height(8.dp))
            Text(
                text = "主密码是唯一根；生物识别仅作解锁因子。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
