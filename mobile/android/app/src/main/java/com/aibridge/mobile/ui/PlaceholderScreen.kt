package com.aibridge.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * 页面空壳脚手架（仅 UI 层共用）。
 *
 * 用途：本阶段只呈现界面编号、名称与承载功能点，便于与 docs/功能点全景.md §3.3 逐条对照。
 * 约束：不含业务判断、不做数据转换；真实内容由各 `*Screen` 在后续任务中补齐。
 */
@Composable
fun PlaceholderScreen(
    screenId: String,
    title: String,
    functionPoints: String,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(text = screenId, style = MaterialTheme.typography.labelMedium)
        Text(text = title, style = MaterialTheme.typography.titleLarge)
        Text(text = "承载功能点：$functionPoints", style = MaterialTheme.typography.bodySmall)
    }
}
