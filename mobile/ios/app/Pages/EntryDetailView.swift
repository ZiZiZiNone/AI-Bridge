import SwiftUI
import UIKit

/// M3 条目详情 + 编辑表单（A1.1、B5.1、E5 深链跳转）。
///
/// 薄入口：字段校验、去重、强度 / 体检判定全在 core；本视图只收集表单、转发
/// put_entry / delete_entry / totp 并渲染结果。类型仅 账号密码 / passkey / TOTP
/// （不设支付信息 / 身份分类）。
struct EntryDetailView: View {
    let environment: AppEnvironment
    /// nil = 新建条目。
    let summary: EntrySummary?
    let onOpenInContainer: (URL) -> Void

    @State private var accountRef = ""
    @State private var title = ""
    @State private var kind: CredentialKind = .password
    @State private var username = ""
    @State private var secret = ""
    @State private var totpSeed = ""
    @State private var urlString = ""
    @State private var note = ""
    @State private var totpCode = ""
    @State private var errorMessage: String?
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    var body: some View {
        PageScaffold(screenId: "M3", title: "条目详情 + 编辑表单", functionPoints: "A1.1、B5.1、深链跳转（E5）") {
            Form {
                Section("条目") {
                    TextField("标题", text: $title)
                    Picker("类型", selection: $kind) {
                        Text("账号密码").tag(CredentialKind.password)
                        Text("passkey").tag(CredentialKind.passkey)
                        Text("TOTP").tag(CredentialKind.totp)
                    }
                    TextField("用户名", text: $username)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    SecureField("密码 / 秘钥", text: $secret)
                    TextField("TOTP 种子", text: $totpSeed)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("URL", text: $urlString)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("备注", text: $note)
                }

                if !username.isEmpty || !secret.isEmpty {
                    Section("一次性复制（B5.1）") {
                        if !username.isEmpty {
                            Button("复制用户名") { UIPasteboard.general.string = username }
                        }
                        if !secret.isEmpty {
                            Button("复制密码 / 秘钥") { UIPasteboard.general.string = secret }
                        }
                    }
                }

                if summary != nil {
                    Section("验证码（B7.1）") {
                        Button("获取 TOTP 验证码") { loadTotp() }
                        if !totpCode.isEmpty {
                            Text(totpCode).font(.title3.monospacedDigit())
                        }
                    }

                    Section("跳转（E5 / M-F5）") {
                        Button("填充并打开") { openExternal() }
                        Button("在容器中打开（c 级）") { openInContainer() }
                    }
                }

                if let errorMessage {
                    Section { Text(errorMessage).font(.footnote).foregroundStyle(.red) }
                }

                Section {
                    Button("保存") { save() }
                    if summary != nil {
                        Button("删除", role: .destructive) { delete() }
                    }
                }
            }
        }
        .navigationTitle(summary?.title ?? "新建条目")
        .task { load() }
    }

    private func load() {
        guard let summary else {
            // 新建：account_ref 的分配语义待确认（UDL 无 allocate API）——暂由宿主生成 UUID。
            if accountRef.isEmpty { accountRef = UUID().uuidString }
            return
        }
        accountRef = summary.accountRef
        do {
            let entry = try environment.core.vault().getEntry(accountRef: summary.accountRef)
            title = entry.title
            kind = entry.kind
            username = entry.username ?? ""
            secret = entry.secret ?? ""
            totpSeed = entry.totpSeed ?? ""
            urlString = entry.url ?? ""
            note = entry.note ?? ""
        } catch {
            errorMessage = "加载失败（\(error)）"
        }
    }

    private func save() {
        do {
            let entry = Entry(
                accountRef: accountRef,
                title: title,
                kind: kind,
                username: username.isEmpty ? nil : username,
                secret: secret.isEmpty ? nil : secret,
                totpSeed: totpSeed.isEmpty ? nil : totpSeed,
                url: urlString.isEmpty ? nil : urlString,
                note: note.isEmpty ? nil : note
            )
            try environment.core.vault().putEntry(entry: entry)
            errorMessage = nil
            dismiss()
        } catch {
            errorMessage = "保存失败（\(error)）"
        }
    }

    private func delete() {
        do {
            try environment.core.vault().deleteEntry(accountRef: accountRef)
            dismiss()
        } catch {
            errorMessage = "删除失败（\(error)）"
        }
    }

    private func loadTotp() {
        do {
            totpCode = try environment.core.vault().totp(accountRef: accountRef)
        } catch {
            errorMessage = "获取验证码失败（\(error)）"
        }
    }

    /// M-F5：库条目「填充并打开」→ 深链启动目标，系统自动填充接管（b 级）。
    private func openExternal() {
        guard let url = URL(string: urlString) else {
            errorMessage = "URL 无效"
            return
        }
        openURL(url)
    }

    /// M-F5：容器内目标直接带态打开（c 级）。
    private func openInContainer() {
        guard let url = URL(string: urlString) else {
            errorMessage = "URL 无效"
            return
        }
        onOpenInContainer(url)
    }
}
