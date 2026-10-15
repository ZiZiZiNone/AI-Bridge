package com.aibridge.mobile.ui.pages

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import com.aibridge.mobile.core.groupedFingerprint
import com.aibridge.mobile.ffi.PairingTicket
import com.aibridge.mobile.ffi.PeerInfo
import com.aibridge.mobile.ui.PageScaffold

/**
 * M7 配对与设备（E3、A8；主屏五 tab 之「设备」）。
 *
 * 含指纹确认屏（FingerprintConfirmScreen）：双方指纹比对、不一致必须拒绝（M-F4）。
 * 设备仅 Windows / iOS / Android —— 不出现 macOS。
 * 薄入口：X25519 指纹配对、设备状态、离线包验时（A8.3 过期拒解）全在 core（ab-sync / ab-state）；
 * 本屏只收集输入并转发 pair / confirm / unpack 与 Transport.peers。
 * 待确认：设备命名 / 解绑（E3）不在 UDL 公开面，设备列表仅展示 Transport.peers()
 *         （与 iOS 同口径）。
 */
@Composable
fun PairingScreen(
    dependencies: AppDependencies,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    var ticket by remember { mutableStateOf<PairingTicket?>(null) }
    var remoteFingerprint by remember { mutableStateOf("") }
    var showScanner by remember { mutableStateOf(false) }
    var showConfirm by remember { mutableStateOf(false) }
    var peers by remember { mutableStateOf<List<PeerInfo>>(emptyList()) }
    var message by remember { mutableStateOf<String?>(null) }

    // A8 导入：过期拒解在 core（A8.3）。
    val importLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        if (uri != null) {
            try {
                val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                    ?: throw IllegalStateException("文件不可读")
                val envelopes = dependencies.core.sync().unpack(bytes)
                message = "导入成功：${envelopes.size} 条会话（过期包由 core 拒解）。"
            } catch (e: Exception) {
                message = "导入失败（${errorText(e)}）"
            }
        }
    }

    if (showScanner) {
        QRScanScreen(
            onResult = { code ->
                showScanner = false
                // M-F4 步骤 1–2：扫 QR → 建配对票据（本机指纹由 core 派生）。
                remoteFingerprint = code
                try {
                    ticket = dependencies.core.sync().pair()
                    showConfirm = true
                } catch (e: Exception) {
                    message = "配对失败（${errorText(e)}）"
                }
            },
            onCancel = { showScanner = false },
            modifier = Modifier.fillMaxSize(),
        )
        return
    }

    val currentTicket = ticket
    if (showConfirm && currentTicket != null) {
        FingerprintConfirmScreen(
            localFingerprint = currentTicket.fingerprint,
            remoteFingerprint = remoteFingerprint,
            onConfirm = {
                // M-F4 步骤 2–3：双方指纹比对（core 强制，不一致抛错并拒绝）→ 设备入列表。
                try {
                    dependencies.core.sync().confirm(currentTicket.ticketId, remoteFingerprint)
                    showConfirm = false
                    message = "配对完成，设备已入列表。"
                    peers = dependencies.transport.peers()
                } catch (e: Exception) {
                    showConfirm = false
                    message = "配对被拒绝（${errorText(e)}）：指纹不一致必须拒绝。"
                }
            },
            onReject = {
                showConfirm = false
                message = "已拒绝配对（指纹不一致时必须拒绝）。"
            },
            modifier = Modifier.fillMaxSize(),
        )
        return
    }

    PageScaffold(screenId = "M7", title = "配对与设备（含指纹确认屏）", functionPoints = "E3、A8", modifier = modifier) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            Text(text = "配对（E3）", style = MaterialTheme.typography.titleMedium)
            Button(onClick = { showScanner = true }) { Text("扫描桌面 QR") }
            Text(
                text = "仅局域网 / 面对面配对（E3）。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "设备列表（E3）", style = MaterialTheme.typography.titleMedium)
            if (peers.isEmpty()) {
                Text("未发现设备", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            peers.forEach { peer ->
                Column(modifier = Modifier.padding(vertical = 4.dp)) {
                    Text(text = "${peer.peerId}（${deviceKindLabel(peer.deviceKind)}）")
                    Text(
                        text = "指纹：${groupedFingerprint(peer.fingerprint)}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Text(
                text = "设备仅 Windows / iOS / Android。解绑入口待 core 接口（见 README 待确认）。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "离线传送包（A8）", style = MaterialTheme.typography.titleMedium)
            Button(onClick = { importLauncher.launch(arrayOf("*/*")) }) { Text("导入离线传送包") }
            Text(
                text = "导入时校验时间戳、拒解过期包（A8.3，由 core 强制）。导出入口在「浏览器」捕获会话后（M5）。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            message?.let { text ->
                Spacer(modifier = Modifier.height(8.dp))
                Text(text = text, style = MaterialTheme.typography.bodySmall)
            }
        }
    }

    LaunchedEffect(Unit) {
        peers = dependencies.transport.peers()
    }
}
