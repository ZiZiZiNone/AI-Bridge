package com.aibridge.mobile.ui.pages

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.unit.dp

/**
 * M7 QR 扫描（E3 / M-F4 步骤 1：扫桌面 QR 取对端指纹）。
 *
 * 职责边界：只负责取得「对端指纹（QR 内容）」并回调；指纹配对与比对在 core（ab-sync）。
 * 平台出入（与 iOS QRScanView 的差异，见 README）：Android 侧相机扫码通道依赖相机 / 条码库，
 * 不在 mobile/README.md §5.3 依赖清单内 —— 依「不臆造平台 API」暂以手动输入承载同一职责，
 * 相机通道待依赖决策后接入（待确认）。
 */
@Composable
fun QRScanScreen(
    onResult: (String) -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var fingerprintText by remember { mutableStateOf("") }

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
    ) {
        Text(text = "M7", style = MaterialTheme.typography.labelMedium)
        Text(text = "扫描桌面 QR", style = MaterialTheme.typography.titleLarge)
        Text(
            text = "承载功能点：E3",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        Spacer(modifier = Modifier.height(16.dp))
        Text(
            text = "相机扫码通道待接入（依赖待确认，见 mobile/android/README.md）；当前以手动输入承载同一职责。",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = fingerprintText,
            onValueChange = { fingerprintText = it },
            label = { Text("对端指纹（QR 内容）") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        Spacer(modifier = Modifier.height(16.dp))
        Button(
            onClick = { if (fingerprintText.isNotBlank()) onResult(fingerprintText.trim()) },
            enabled = fingerprintText.isNotBlank(),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("使用此指纹")
        }
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) { Text("取消") }
    }
}
