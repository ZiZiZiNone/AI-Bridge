package com.aibridge.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * 页面脚手架：呈现界面编号锚点、名称与承载功能点，便于与 docs/功能点全景.md §3.3 逐条对照。
 * 约束：不含业务判断、不做数据转换；内容由各页面组装（薄入口）。
 * 与 iOS `app/Pages/PageScaffold.swift` 职责对齐。
 */
@Composable
fun PageScaffold(
    screenId: String,
    title: String,
    functionPoints: String,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(text = screenId, style = MaterialTheme.typography.labelMedium)
            Text(text = title, style = MaterialTheme.typography.titleLarge)
            Text(text = "承载功能点：$functionPoints", style = MaterialTheme.typography.bodySmall)
        }
        content()
    }
}
