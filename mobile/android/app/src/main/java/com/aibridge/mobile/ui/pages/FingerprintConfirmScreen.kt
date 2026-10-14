package com.aibridge.mobile.ui.pages

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.aibridge.mobile.core.groupedFingerprint

/**
 * M7 指纹确认屏（E3 / M-F4）：展示双方指纹，用户目视比对后确认或拒绝。
 *
 * 强制规则：双方指纹不一致必须拒绝 —— 比对与拒绝由 core（`SyncEngine.confirm`）强制执行
 * （confirm 在 remote_fingerprint 与对端实测指纹不一致时抛错，配对不成立）；
 * 本屏是薄入口，只展示与转发，不自行判定一致性（docs/流程/移动端流程.md M-F4）。
 * 与 iOS `app/Pages/FingerprintConfirmView.swift` 职责与文案一致。
 */
@Composable
fun FingerprintConfirmScreen(
    localFingerprint: String,
    remoteFingerprint: String,
    onConfirm: () -> Unit,
    onReject: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
    ) {
        Text(text = "M7", style = MaterialTheme.typography.labelMedium)
        Text(text = "指纹确认屏", style = MaterialTheme.typography.titleLarge)
        Text(
            text = "承载功能点：E3",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        Spacer(modifier = Modifier.height(16.dp))
        Text(text = "指纹确认（E3）", style = MaterialTheme.typography.titleMedium)
        Text(
            text = "请核对两端指纹是否完全一致；不一致必须拒绝。",
            style = MaterialTheme.typography.bodySmall,
        )

        Spacer(modifier = Modifier.height(16.dp))
        fingerprintBlock(title = "本机指纹", value = localFingerprint)
        Spacer(modifier = Modifier.height(8.dp))
        fingerprintBlock(title = "对端指纹（扫码所得）", value = remoteFingerprint)

        Spacer(modifier = Modifier.height(16.dp))
        Row {
            OutlinedButton(onClick = onReject, modifier = Modifier.weight(1f)) {
                Text("拒绝（不一致）")
            }
            Spacer(modifier = Modifier.padding(horizontal = 4.dp))
            Button(onClick = onConfirm, modifier = Modifier.weight(1f)) {
                Text("确认配对")
            }
        }

        Spacer(modifier = Modifier.height(8.dp))
        TextButton(onClick = onReject) { Text("取消") }
    }
}

@Composable
private fun fingerprintBlock(title: String, value: String) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = MaterialTheme.shapes.medium,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(modifier = Modifier.padding(10.dp)) {
            Text(text = title, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(text = groupedFingerprint(value), style = MaterialTheme.typography.bodyLarge)
        }
    }
}
