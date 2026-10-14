package com.aibridge.mobile.ui.pages

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
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
import com.aibridge.mobile.core.errorText
import com.aibridge.mobile.core.sourceTypeLabel
import com.aibridge.mobile.core.targetKindLabel
import com.aibridge.mobile.ffi.HealthReport
import com.aibridge.mobile.ffi.IssuedSession
import com.aibridge.mobile.ffi.MatrixRow
import com.aibridge.mobile.ffi.ScopeSpec
import com.aibridge.mobile.ffi.SnapshotSummary
import com.aibridge.mobile.ui.PageScaffold

/**
 * M6 状态 / 临时会话（A2、C 视图；含新建会话表单）。
 *
 * 薄入口：TTL 上限、白名单校验、撤销语义在 core（ab-broker）；本屏只收集表单并展示状态。
 * 展示：会话矩阵（A2.1）/ 临时会话（A2.2）/ 快照回滚（A7）/ 会话健康（A3.1）。
 */
@Composable
fun SessionStatusScreen(
    dependencies: AppDependencies,
    onBack: (() -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    var rows by remember { mutableStateOf<List<MatrixRow>>(emptyList()) }
    var sessions by remember { mutableStateOf<List<IssuedSession>>(emptyList()) }
    var snapshots by remember { mutableStateOf<List<SnapshotSummary>>(emptyList()) }
    var health by remember { mutableStateOf<HealthReport?>(null) }
    var showNewSession by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf<String?>(null) }

    fun reload() {
        rows = dependencies.core.state().matrix()
        sessions = dependencies.core.broker().listSessions()
        snapshots = dependencies.core.state().snapshot()
        health = dependencies.core.state().health()
    }

    PageScaffold(
        screenId = "M6",
        title = "状态 / 临时会话（含新建会话表单）",
        functionPoints = "A2、C 视图",
        modifier = modifier,
    ) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            Row {
                onBack?.let { TextButton(onClick = it) { Text("返回") } }
                Spacer(modifier = Modifier.weight(1f))
                Button(onClick = { showNewSession = true }) { Text("新建会话") }
            }

            errorMessage?.let { message ->
                Text(text = message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }

            Text(text = "会话矩阵（A2.1）", style = MaterialTheme.typography.titleMedium)
            if (rows.isEmpty()) {
                Text("暂无会话", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            rows.forEach { row ->
                Column(modifier = Modifier.padding(vertical = 4.dp)) {
                    Text(text = row.accountRef, style = MaterialTheme.typography.bodyLarge)
                    Text(
                        text = "${targetKindLabel(row.targetKind)} · ${row.domain ?: row.appId ?: "-"} · 来源 ${sourceTypeLabel(row.sourceType)}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    row.expiresAt?.let { expires ->
                        Text(text = "到期：$expires", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "临时会话（A2.2 / C）", style = MaterialTheme.typography.titleMedium)
            if (sessions.isEmpty()) {
                Text("暂无临时会话", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            sessions.forEach { session ->
                Column(modifier = Modifier.padding(vertical = 4.dp)) {
                    Text(text = "${session.accountRef}（${session.sessionId}）", style = MaterialTheme.typography.bodyLarge)
                    Text(
                        text = "到期：${session.expiresAt}${if (session.revoked) " · 已吊销" else ""}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Row {
                        TextButton(onClick = {
                            try {
                                dependencies.core.broker().release(session.sessionId)
                                reload()
                            } catch (e: Exception) {
                                errorMessage = "释放失败（${errorText(e)}）"
                            }
                        }) { Text("释放（C2.2）") }
                        TextButton(onClick = {
                            try {
                                dependencies.core.broker().revoke(session.sessionId)
                                reload()
                            } catch (e: Exception) {
                                errorMessage = "吊销失败（${errorText(e)}）"
                            }
                        }) { Text("吊销（C5.1）") }
                    }
                }
            }
            OutlinedButton(onClick = {
                dependencies.core.broker().revokeAll()
                reload()
            }) { Text("一键全部吊销（C5.1）") }

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "快照与回滚（A7）", style = MaterialTheme.typography.titleMedium)
            if (snapshots.isEmpty()) {
                Text("暂无快照", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            snapshots.forEach { snapshot ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(text = snapshot.snapshotId, style = MaterialTheme.typography.bodySmall)
                        Text(
                            text = "${snapshot.createdAt} · ${snapshot.entryCount} 条",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    TextButton(onClick = {
                        try {
                            dependencies.core.state().rollback(snapshot.snapshotId)
                            reload()
                        } catch (e: Exception) {
                            errorMessage = "回滚失败（${errorText(e)}）"
                        }
                    }) { Text("回滚") }
                }
            }

            Spacer(modifier = Modifier.height(8.dp))
            Text(text = "会话健康（A3.1）", style = MaterialTheme.typography.titleMedium)
            val currentHealth = health
            if (currentHealth == null) {
                Text("待加载", color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Text(text = "库可读：${if (currentHealth.vaultReadable) "是" else "否"}", style = MaterialTheme.typography.bodySmall)
                Text(text = "同步可达：${if (currentHealth.syncReachable) "是" else "否"}", style = MaterialTheme.typography.bodySmall)
                currentHealth.warnings.forEach { warning ->
                    Text(text = warning, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.tertiary)
                }
            }
        }
    }

    if (showNewSession) {
        NewSessionForm(
            dependencies = dependencies,
            onIssued = {
                showNewSession = false
                reload()
            },
            onDismiss = { showNewSession = false },
        )
    }

    LaunchedEffect(Unit) { reload() }
}

/**
 * M6 新建临时会话表单（C4.1 范围限制 / C2.2 TTL）。
 * 薄入口：白名单校验、TTL 上限（24h）、单次 / 只读语义强制全在 core（ab-broker），本表单只收集转发。
 */
@Composable
private fun NewSessionForm(
    dependencies: AppDependencies,
    onIssued: () -> Unit,
    onDismiss: () -> Unit,
) {
    var domainsText by remember { mutableStateOf("") }
    var readOnly by remember { mutableStateOf(false) }
    var singleUse by remember { mutableStateOf(false) }
    var ttlHours by remember { mutableStateOf(1) }
    var errorMessage by remember { mutableStateOf<String?>(null) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("新建临时会话") },
        text = {
            Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                Text(text = "域名白名单（C4.1）", style = MaterialTheme.typography.titleSmall)
                OutlinedTextField(
                    value = domainsText,
                    onValueChange = { domainsText = it },
                    label = { Text("每行一个域名") },
                    modifier = Modifier.fillMaxWidth(),
                )
                Text(
                    text = "留空 = 不允许任何 web 目标（由 core 校验）。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

                Spacer(modifier = Modifier.height(8.dp))
                Text(text = "范围（C4.1）", style = MaterialTheme.typography.titleSmall)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Switch(checked = readOnly, onCheckedChange = { readOnly = it })
                    Text("只读")
                    Switch(checked = singleUse, onCheckedChange = { singleUse = it })
                    Text("单次会话")
                }

                Spacer(modifier = Modifier.height(8.dp))
                Text(text = "TTL（C2.2）", style = MaterialTheme.typography.titleSmall)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedButton(onClick = { if (ttlHours > 1) ttlHours -= 1 }) { Text("−") }
                    Text(text = "有效期：$ttlHours 小时", modifier = Modifier.padding(horizontal = 8.dp))
                    OutlinedButton(onClick = { if (ttlHours < 24) ttlHours += 1 }) { Text("＋") }
                }
                Text(
                    text = "默认 1 小时，上限 24 小时（由 core 强制）；空闲 30 分钟自动释放。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

                errorMessage?.let { message ->
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(text = message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                try {
                    val domains = domainsText
                        .split('\n')
                        .map { it.trim() }
                        .filter { it.isNotEmpty() }
                    val scope = ScopeSpec(domainAllowlist = domains, readOnly = readOnly, singleUse = singleUse)
                    dependencies.core.broker().issue(scope = scope, ttlSeconds = (ttlHours * 3600).toUInt())
                    onIssued()
                } catch (e: Exception) {
                    errorMessage = "签发失败（${errorText(e)}）"
                }
            }) { Text("签发") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("取消") }
        },
    )
}
