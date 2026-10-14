import SwiftUI
import UIKit

/// M4 生成器（A5；默认 20 位 · 不存生成历史）。
///
/// 薄入口：字符集与强度规则在 core（Vault.generate），本视图只收集 GeneratorSpec 并展示结果。
/// 明文纪律：不存生成历史 —— 本视图不保留任何历史状态，离开即失（docs/功能点全景.md §5）。
struct GeneratorView: View {
    let environment: AppEnvironment

    @State private var length = 20
    @State private var lowercase = true
    @State private var uppercase = true
    @State private var digits = true
    @State private var symbols = true
    @State private var generated: GeneratedSecret?
    @State private var errorMessage: String?

    var body: some View {
        PageScaffold(screenId: "M4", title: "生成器", functionPoints: "A5") {
            Form {
                Section("参数") {
                    Stepper("长度：\(length) 位", value: $length, in: 8...64)
                    Toggle("小写", isOn: $lowercase)
                    Toggle("大写", isOn: $uppercase)
                    Toggle("数字", isOn: $digits)
                    Toggle("符号", isOn: $symbols)
                }

                Section {
                    Button("生成") { generate() }
                }

                if let generated {
                    Section("结果") {
                        Text(generated.value)
                            .font(.body.monospaced())
                            .textSelection(.enabled)
                        Text("字符集：\(generated.charset)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        Button("复制") { UIPasteboard.general.string = generated.value }
                    }
                }

                if let errorMessage {
                    Section { Text(errorMessage).font(.footnote).foregroundStyle(.red) }
                }

                Section {
                    Text("不保存生成历史（明文纪律）。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("生成器")
    }

    private func generate() {
        do {
            let spec = GeneratorSpec(
                length: UInt32(length),
                lowercase: lowercase,
                uppercase: uppercase,
                digits: digits,
                symbols: symbols
            )
            generated = try environment.core.vault().generate(spec: spec)
            errorMessage = nil
        } catch {
            errorMessage = "生成失败（\(error)）"
        }
    }
}
