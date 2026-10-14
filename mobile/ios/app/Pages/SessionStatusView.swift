import SwiftUI

/// M6 状态 / 临时会话（A2、C 视图；含新建会话表单）。
///
/// 薄入口：TTL 上限、白名单校验、撤销语义在 core（ab-broker）；本视图只收集表单并展示状态。
/// 展示：会话矩阵（A2.1）/ 临时会话（A2.2）/ 快照回滚（A7）/ 会话健康（A3.1）。
struct SessionStatusView: View {
    let environment: AppEnvironment

    @State private var rows: [MatrixRow] = []
    @State private var sessions: [IssuedSession] = []
    @State private var snapshots: [SnapshotSummary] = []
    @State private var health: HealthReport?
    @State private var showNewSession = false
    @State private var errorMessage: String?

    var body: some View {
        PageScaffold(screenId: "M6", title: "状态 / 临时会话（含新建会话表单）", functionPoints: "A2、C 视图") {
            List {
                if let errorMessage {
                    Section { Text(errorMessage).font(.footnote).foregroundStyle(.red) }
                }

                Section("会话矩阵（A2.1）") {
                    if rows.isEmpty {
                        Text("暂无会话").foregroundStyle(.secondary)
                    }
                    ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.accountRef).font(.body)
                            Text("\(targetKindLabel(row.targetKind)) · \(row.domain ?? row.appId ?? "-") · 来源 \(sourceTypeLabel(row.sourceType))")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            if let expires = row.expiresAt {
                                Text("到期：\(expires)").font(.caption2).foregroundStyle(.secondary)
                            }
                        }
                    }
                }

                Section("临时会话（A2.2 / C）") {
                    if sessions.isEmpty {
                        Text("暂无临时会话").foregroundStyle(.secondary)
                    }
                    ForEach(Array(sessions.enumerated()), id: \.offset) { _, session in
                        VStack(alignment: .leading, spacing: 2) {
                            Text("\(session.accountRef)（\(session.sessionId)）").font(.body)
                            Text("到期：\(session.expiresAt)\(session.revoked ? " · 已吊销" : "")")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            HStack {
                                Button("释放（C2.2）") { release(session) }
                                    .buttonStyle(.borderless)
                                Button("吊销（C5.1）") { revoke(session) }
                                    .buttonStyle(.borderless)
                            }
                        }
                    }
                    Button("一键全部吊销（C5.1）") { revokeAll() }
                }

                Section("快照与回滚（A7）") {
                    if snapshots.isEmpty {
                        Text("暂无快照").foregroundStyle(.secondary)
                    }
                    ForEach(Array(snapshots.enumerated()), id: \.offset) { _, snapshot in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(snapshot.snapshotId).font(.caption)
                                Text("\(snapshot.createdAt) · \(snapshot.entryCount) 条")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button("回滚") { rollback(snapshot) }
                                .buttonStyle(.borderless)
                        }
                    }
                }

                Section("会话健康（A3.1）") {
                    if let health {
                        Text("库可读：\(health.vaultReadable ? "是" : "否")").font(.footnote)
                        Text("同步可达：\(health.syncReachable ? "是" : "否")").font(.footnote)
                        ForEach(health.warnings, id: \.self) { warning in
                            Text(warning).font(.caption).foregroundStyle(.orange)
                        }
                    } else {
                        Text("待加载").foregroundStyle(.secondary)
                    }
                }
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("新建会话") { showNewSession = true }
                }
            }
            .sheet(isPresented: $showNewSession) {
                NewSessionFormView(environment: environment, onIssued: reload)
            }
            .task { reload() }
        }
        .navigationTitle("状态 / 临时会话")
    }

    private func reload() {
        rows = environment.core.state().matrix()
        sessions = environment.core.broker().listSessions()
        snapshots = environment.core.state().snapshot()
        health = environment.core.state().health()
    }

    private func release(_ session: IssuedSession) {
        do {
            try environment.core.broker().release(sessionId: session.sessionId)
            reload()
        } catch {
            errorMessage = "释放失败（\(error)）"
        }
    }

    private func revoke(_ session: IssuedSession) {
        do {
            try environment.core.broker().revoke(sessionId: session.sessionId)
            reload()
        } catch {
            errorMessage = "吊销失败（\(error)）"
        }
    }

    private func revokeAll() {
        _ = environment.core.broker().revokeAll()
        reload()
    }

    private func rollback(_ snapshot: SnapshotSummary) {
        do {
            try environment.core.state().rollback(snapshotId: snapshot.snapshotId)
            reload()
        } catch {
            errorMessage = "回滚失败（\(error)）"
        }
    }
}

/// M6 新建临时会话表单（C4.1 范围限制 / C2.2 TTL）。
/// 薄入口：白名单校验、TTL 上限（24h）、单次 / 只读语义强制全在 core（ab-broker），本视图只收集转发。
struct NewSessionFormView: View {
    let environment: AppEnvironment
    let onIssued: () -> Void

    @State private var domainsText = ""
    @State private var readOnly = false
    @State private var singleUse = false
    @State private var ttlHours = 1
    @State private var errorMessage: String?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section("域名白名单（C4.1）") {
                    TextField("每行一个域名", text: $domainsText, axis: .vertical)
                        .lineLimit(3...6)
                    Text("留空 = 不允许任何 web 目标（由 core 校验）。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Section("范围（C4.1）") {
                    Toggle("只读", isOn: $readOnly)
                    Toggle("单次会话", isOn: $singleUse)
                }
                Section("TTL（C2.2）") {
                    Stepper("有效期：\(ttlHours) 小时", value: $ttlHours, in: 1...24)
                    Text("默认 1 小时，上限 24 小时（由 core 强制）；空闲 30 分钟自动释放。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                if let errorMessage {
                    Section { Text(errorMessage).font(.footnote).foregroundStyle(.red) }
                }
            }
            .navigationTitle("新建临时会话")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("签发") { issue() }
                }
            }
        }
    }

    private func issue() {
        do {
            let domains = domainsText
                .split(whereSeparator: \.isNewline)
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }
            let scope = ScopeSpec(domainAllowlist: domains, readOnly: readOnly, singleUse: singleUse)
            _ = try environment.core.broker().issue(scope: scope, ttlSeconds: UInt32(ttlHours * 3600))
            onIssued()
            dismiss()
        } catch {
            errorMessage = "签发失败（\(error)）"
        }
    }
}
