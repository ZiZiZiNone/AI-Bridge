import SwiftUI

/// M9 候选列表（解锁后选账号填入）。
///
/// 薄入口：账号匹配、去重、解锁判定在 core；本视图只转发查询并渲染 EntrySummary，
/// 选择后取条目字段交给系统填入。仅账号类条目（不设支付信息 / 身份分类）。
struct CredentialListView: View {
    let environment: AppEnvironment
    let serviceQuery: String
    let onSelect: (String, String) -> Void

    @State private var masterPassword = ""
    @State private var entries: [EntrySummary] = []
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            Group {
                if environment.core.vault().isLocked() {
                    unlockForm
                } else {
                    entryList
                }
            }
            .navigationTitle("AI-Bridge")
        }
        .task { reload() }
    }

    private var unlockForm: some View {
        Form {
            Section("解锁（S1）") {
                SecureField("主密码", text: $masterPassword)
                    .textContentType(.password)
                Button("解锁") { unlock() }
                    .disabled(masterPassword.isEmpty)
            }
            if let errorMessage {
                Section { Text(errorMessage).font(.footnote).foregroundStyle(.red) }
            }
        }
    }

    private var entryList: some View {
        List {
            if entries.isEmpty {
                Text("无匹配账号").foregroundStyle(.secondary)
            }
            ForEach(entries, id: \.accountRef) { summary in
                Button {
                    select(summary)
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(summary.title)
                        Text("\(credentialKindLabel(summary.kind)) · \(summary.username ?? "-")")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            if let errorMessage {
                Section { Text(errorMessage).font(.footnote).foregroundStyle(.red) }
            }
        }
    }

    private func unlock() {
        do {
            try environment.core.vault().unlock(masterPassword: masterPassword)
            masterPassword = ""
            errorMessage = nil
            reload()
        } catch {
            errorMessage = "解锁失败（\(error)）"
        }
    }

    private func reload() {
        guard !environment.core.vault().isLocked() else { return }
        do {
            entries = try environment.core.vault().listEntries(query: serviceQuery.isEmpty ? nil : serviceQuery)
        } catch {
            errorMessage = "加载失败（\(error)）"
        }
    }

    private func select(_ summary: EntrySummary) {
        do {
            let entry = try environment.core.vault().getEntry(accountRef: summary.accountRef)
            // 待确认：passkey / TOTP 条目的系统填充形态（UDL 无填充形状定义）；
            // 当前仅填充 username / secret 字段（M-F3：选账号 → 系统填入 → 用完即走）。
            onSelect(entry.username ?? "", entry.secret ?? "")
        } catch {
            errorMessage = "读取条目失败（\(error)）"
        }
    }
}
