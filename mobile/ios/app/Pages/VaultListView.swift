import SwiftUI

/// M2 库列表 + 搜索（A1.2；主屏五 tab 之「库」）。
///
/// 薄入口：检索、去重、组织排序等业务规则全在 core，本视图只转发查询并渲染
/// core 返回的 EntrySummary；条目详情 / 编辑表单在 M3（NavigationLink 进入）。
struct VaultListView: View {
    let environment: AppEnvironment
    let onOpenInContainer: (URL) -> Void

    @State private var query = ""
    @State private var entries: [EntrySummary] = []
    @State private var trashEntries: [EntrySummary] = []
    @State private var showTrash = false
    @State private var errorMessage: String?

    var body: some View {
        PageScaffold(screenId: "M2", title: "库列表 + 搜索", functionPoints: "A1.2") {
            List {
                if let errorMessage {
                    Section { Text(errorMessage).font(.footnote).foregroundStyle(.red) }
                }
                Section("条目") {
                    if entries.isEmpty {
                        Text("暂无条目")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(entries, id: \.accountRef) { summary in
                        NavigationLink {
                            EntryDetailView(environment: environment, summary: summary, onOpenInContainer: onOpenInContainer)
                        } label: {
                            entryRow(summary)
                        }
                    }
                }
            }
            .searchable(text: $query, prompt: "搜索条目")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("回收站") { showTrash = true }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink("新建") {
                        EntryDetailView(environment: environment, summary: nil, onOpenInContainer: onOpenInContainer)
                    }
                }
            }
            .task(id: query) { reload() }
            .sheet(isPresented: $showTrash) { trashSheet }
        }
        .navigationTitle("库")
    }

    private func entryRow(_ summary: EntrySummary) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(summary.title).font(.body)
            HStack(spacing: 6) {
                Text(credentialKindLabel(summary.kind)).font(.caption2)
                if let username = summary.username {
                    Text(username).font(.caption2).foregroundStyle(.secondary)
                }
            }
        }
    }

    private var trashSheet: some View {
        NavigationStack {
            List {
                if trashEntries.isEmpty {
                    Text("回收站为空").foregroundStyle(.secondary)
                }
                ForEach(trashEntries, id: \.accountRef) { summary in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(summary.title)
                            Text(credentialKindLabel(summary.kind)).font(.caption2).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("恢复（A1.5）") { restore(summary) }
                            .buttonStyle(.borderless)
                    }
                }
            }
            .navigationTitle("回收站")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("完成") { showTrash = false }
                }
            }
            .task { reloadTrash() }
        }
    }

    private func reload() {
        do {
            entries = try environment.core.vault().listEntries(query: query.isEmpty ? nil : query)
            errorMessage = nil
        } catch {
            errorMessage = "加载失败（\(error)）"
        }
    }

    private func reloadTrash() {
        trashEntries = environment.core.vault().trash()
    }

    private func restore(_ summary: EntrySummary) {
        do {
            try environment.core.vault().restore(accountRef: summary.accountRef)
            reloadTrash()
            reload()
        } catch {
            showTrash = false
            errorMessage = "恢复失败（\(error)）"
        }
    }
}
