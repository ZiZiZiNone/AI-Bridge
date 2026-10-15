package com.aibridge.mobile.ui.pages

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.unit.dp
import com.aibridge.mobile.AppDependencies
import com.aibridge.mobile.core.credentialKindLabel
import com.aibridge.mobile.core.errorText
import com.aibridge.mobile.ffi.EntrySummary
import com.aibridge.mobile.ui.PageScaffold

/**
 * M2 库列表 + 搜索（A1.2；主屏五 tab 之「库」）。
 *
 * 薄入口：检索、去重、组织排序等业务规则全在 core，本屏只转发查询并渲染
 * core 返回的 EntrySummary；条目详情 / 编辑表单在 M3（点条目进入）。
 * 回收站（A1.5）：软删除 / 恢复语义在 core，本屏只转发 restore。
 */
@Composable
fun VaultListScreen(
    dependencies: AppDependencies,
    onOpenInContainer: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var query by remember { mutableStateOf("") }
    var entries by remember { mutableStateOf<List<EntrySummary>>(emptyList()) }
    var trashEntries by remember { mutableStateOf<List<EntrySummary>>(emptyList()) }
    var showTrash by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var detailTarget by remember { mutableStateOf<EntrySummary?>(null) }
    var showNewEntry by remember { mutableStateOf(false) }

    fun reload() {
        try {
            entries = dependencies.core.vault().listEntries(query.takeIf { it.isNotBlank() })
            errorMessage = null
        } catch (e: Exception) {
            errorMessage = "加载失败（${errorText(e)}）"
        }
    }

    fun reloadTrash() {
        trashEntries = dependencies.core.vault().trash()
    }

    if (showNewEntry) {
        EntryDetailScreen(
            dependencies = dependencies,
            summary = null,
            onOpenInContainer = onOpenInContainer,
            onDone = { showNewEntry = false },
        )
        return
    }

    detailTarget?.let { target ->
        EntryDetailScreen(
            dependencies = dependencies,
            summary = target,
            onOpenInContainer = onOpenInContainer,
            onDone = { detailTarget = null },
        )
        return
    }

    PageScaffold(screenId = "M2", title = "库列表 + 搜索", functionPoints = "A1.2", modifier = modifier) {
        Column {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    label = { Text("搜索条目") },
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                )
                TextButton(onClick = {
                    reloadTrash()
                    showTrash = true
                }) { Text("回收站") }
                Button(onClick = { showNewEntry = true }) { Text("新建") }
            }

            errorMessage?.let { message ->
                Spacer(modifier = Modifier.height(8.dp))
                Text(text = message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "条目", style = MaterialTheme.typography.titleMedium)

            if (entries.isEmpty()) {
                Text(
                    text = "暂无条目",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 8.dp),
                )
            } else {
                LazyColumn {
                    items(entries, key = { it.accountRef }) { summary ->
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { detailTarget = summary }
                                .padding(vertical = 8.dp),
                        ) {
                            Text(text = summary.title, style = MaterialTheme.typography.bodyLarge)
                            Row {
                                Text(
                                    text = credentialKindLabel(summary.kind),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                                summary.username?.let { username ->
                                    Text(
                                        text = "  $username",
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if (showTrash) {
        AlertDialog(
            onDismissRequest = { showTrash = false },
            title = { Text("回收站") },
            text = {
                if (trashEntries.isEmpty()) {
                    Text("回收站为空", color = MaterialTheme.colorScheme.onSurfaceVariant)
                } else {
                    LazyColumn {
                        items(trashEntries, key = { it.accountRef }) { summary ->
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(text = summary.title)
                                    Text(
                                        text = credentialKindLabel(summary.kind),
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                                TextButton(onClick = {
                                    try {
                                        dependencies.core.vault().restore(summary.accountRef)
                                        reloadTrash()
                                        reload()
                                    } catch (e: Exception) {
                                        showTrash = false
                                        errorMessage = "恢复失败（${errorText(e)}）"
                                    }
                                }) { Text("恢复（A1.5）") }
                            }
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { showTrash = false }) { Text("完成") }
            },
        )
    }

    LaunchedEffect(query) { reload() }
}
