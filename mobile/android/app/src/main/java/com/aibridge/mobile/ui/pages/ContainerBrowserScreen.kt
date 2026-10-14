package com.aibridge.mobile.ui.pages

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.core.deviceKindLabel
import com.aibridge.mobile.core.errorText
import com.aibridge.mobile.ffi.SessionEnvelope
import com.aibridge.mobile.ui.PageScaffold
import com.aibridge.mobile.webview.ContainerWebView

/**
 * M5 容器浏览器（全屏 WebView；登录、会话捕获、E1/E2 扩散发送）。
 *
 * 薄入口：会话归一化（SessionEnvelope）、扩散、打包全在 core；本屏只收集输入并转发
 * StateBus.capture / SyncEngine.transfer / SyncEngine.pack。
 * 会话捕获 / 注入由 ContainerSessionBridge（SessionSource trait 实现）完成，
 * 含 HttpOnly 与属性（A2 零丢弃）。
 * 边界：Android Chrome 无扩展通道，其会话不在扩散范围（本页为 App 内容器，不受此限）。
 */
@Composable
fun ContainerBrowserScreen(
    dependencies: AppDependencies,
    initialUrl: String?,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    var address by remember { mutableStateOf(initialUrl ?: "about:blank") }
    var currentUrl by remember { mutableStateOf(initialUrl ?: "about:blank") }
    var captured by remember { mutableStateOf<SessionEnvelope?>(null) }
    var message by remember { mutableStateOf<String?>(null) }
    var pendingPackage by remember { mutableStateOf<ByteArray?>(null) }
    var showStatus by remember { mutableStateOf(false) }

    // A8 导出：pack 在 core（对端公钥加密 + 签名时间戳），本层只落文件交系统文件选择器。
    val exportLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri ->
        val bytes = pendingPackage
        if (uri != null && bytes != null) {
            try {
                context.contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
                message = "离线传送包已生成（内容零泄露，可任意渠道搬运）。"
            } catch (e: Exception) {
                message = "导出失败（${errorText(e)}）"
            }
        }
        pendingPackage = null
    }

    if (showStatus) {
        SessionStatusScreen(
            dependencies = dependencies,
            onBack = { showStatus = false },
            modifier = Modifier.fillMaxSize(),
        )
        return
    }

    PageScaffold(
        screenId = "M5",
        title = "容器浏览器",
        functionPoints = "登录、会话捕获、E1/E2 扩散发送",
        modifier = modifier,
    ) {
        Column {
            Row {
                OutlinedTextField(
                    value = address,
                    onValueChange = { address = it },
                    label = { Text("网址") },
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                )
                Button(onClick = {
                    val scheme = Uri.parse(address).scheme
                    if (scheme.isNullOrEmpty()) {
                        message = "网址无效"
                    } else {
                        currentUrl = address
                        message = null
                    }
                }) { Text("前往") }
            }

            Spacer(modifier = Modifier.height(8.dp))
            ContainerWebView(
                url = currentUrl,
                onNavigate = { finishedUrl -> address = finishedUrl },
                modifier = Modifier.weight(1f).fillMaxWidth(),
            )
            Spacer(modifier = Modifier.height(8.dp))

            Row {
                TextButton(onClick = {
                    // M-F2 步骤 2–3：会话完整捕获（经 core 归一化为 SessionEnvelope）。
                    try {
                        val domain = Uri.parse(address).host ?: address
                        captured = dependencies.core.state().capture(
                            sourceId = "android-container",
                            domain = domain,
                        )
                        message = "捕获成功（含 HttpOnly 与全部属性）"
                    } catch (e: Exception) {
                        message = "捕获失败（${errorText(e)}）"
                    }
                }) { Text("捕获会话") }

                TextButton(onClick = {
                    // M-F2 步骤 3：局域网直传到对端（E1/E2）。
                    val envelope = captured
                    if (envelope == null) {
                        message = "请先捕获会话"
                        return@TextButton
                    }
                    val peers = dependencies.transport.peers()
                    if (peers.isEmpty()) {
                        message = "局域网内未发现设备"
                        return@TextButton
                    }
                    message = "可选设备：" + peers.joinToString("、") { "${it.peerId}（${deviceKindLabel(it.deviceKind)}）" }
                    // 先把首台设备作为直传目标；多设备选择表单在配对联调后补。
                    val target = peers.first()
                    try {
                        val report = dependencies.core.sync().transfer(target.peerId, listOf(envelope))
                        message = "已发送到 ${target.peerId}：成功 ${report.sent} 条"
                    } catch (e: Exception) {
                        message = "发送失败（${errorText(e)}）"
                    }
                }) { Text("发送到设备") }

                TextButton(onClick = {
                    val envelope = captured
                    if (envelope == null) {
                        message = "请先捕获会话"
                        return@TextButton
                    }
                    try {
                        pendingPackage = dependencies.core.sync().pack(listOf(envelope))
                        exportLauncher.launch("aibridge-offline-${System.currentTimeMillis() / 1000}.a8pkg")
                    } catch (e: Exception) {
                        message = "导出失败（${errorText(e)}）"
                    }
                }) { Text("导出离线包（A8）") }

                TextButton(onClick = { showStatus = true }) { Text("状态 / 临时会话") }
            }

            captured?.let { envelope ->
                Text(
                    text = "已捕获会话：${envelope.accountRef}（${envelope.target.domain ?: "-"}）",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            message?.let { text ->
                Text(text = text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}
