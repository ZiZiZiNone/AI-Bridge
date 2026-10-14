import SwiftUI
import UIKit

/// M4 生成器（A5；默认 20 位 · 不存生成历史 · 实时强度条 · 口令词模式）。
///
/// 薄入口：字符集 / 词表规则与强度数值（香农熵 bit）全在 core——
/// `Vault.generate` / `Vault.generate_passphrase` / 自由函数 `generator_entropy_bits` /
/// `passphrase_entropy_bits`（mobile/ffi/aibridge.udl）；本视图只收集 spec 并展示 core 返回值，
/// 强度条不做任何本地重算与强弱分级（数值只有 core 一处）。
/// A5.2：词表 / 词数 / 分隔符全部由 UI 配置提供（core 不设默认、不内置词表）。
/// 明文纪律：不存生成历史 —— 本视图不保留任何历史状态，离开即失（docs/功能点全景.md §5）。
/// FFI 命名按 UniFFI 惯例书写（generatorEntropyBits / passphraseEntropyBits / generatePassphrase /
/// PassphraseSpec），生成物未定稿前标注待核对（见 README §3）。
struct GeneratorView: View {
    let environment: AppEnvironment

    // 字符模式参数（A5.1：默认 20 位，字符集可配）。
    @State private var length = 20
    @State private var lowercase = true
    @State private var uppercase = true
    @State private var digits = true
    @State private var symbols = true

    // 口令词模式参数（A5.2：词表 / 词数 / 分隔符全由调用方配置，core 无默认）。
    @State private var wordsText = ""
    @State private var wordCount = 6
    @State private var separator = "-"

    @State private var mode: Mode = .characters
    @State private var generated: GeneratedSecret?
    @State private var generatedPhrase: String?
    @State private var entropyBits: Double?
    @State private var errorMessage: String?

    enum Mode: Hashable {
        case characters, passphrase
    }

    var body: some View {
        PageScaffold(screenId: "M4", title: "生成器", functionPoints: "A5（默认 20 位 · 不存历史）") {
            Form {
                Section("模式（A5.2）") {
                    Picker("模式", selection: $mode) {
                        Text("字符").tag(Mode.characters)
                        Text("口令词").tag(Mode.passphrase)
                    }
                    .pickerStyle(.segmented)
                }

                switch mode {
                case .characters:
                    characterSection
                case .passphrase:
                    passphraseSection
                }

                strengthSection

                Section {
                    Button("生成") { generate() }
                }

                resultSection

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
        .task(id: entropyKey) { refreshEntropy() }
    }

    private var characterSection: some View {
        Section("参数（A5.1）") {
            Stepper("长度：\(length) 位", value: $length, in: 8...64)
            Toggle("小写", isOn: $lowercase)
            Toggle("大写", isOn: $uppercase)
            Toggle("数字", isOn: $digits)
            Toggle("符号", isOn: $symbols)
        }
    }

    private var passphraseSection: some View {
        Section("参数（A5.2）") {
            TextField("词表（每行一个词）", text: $wordsText, axis: .vertical)
                .lineLimit(3...8)
            Stepper("词数：\(wordCount) 个", value: $wordCount, in: 1...64)
            TextField("分隔符", text: $separator)
            Text("词表 / 词数 / 分隔符全部由你配置（core 不设默认、不内置词表）。")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
    }

    /// A5.1 实时强度条：数值 = core 的香农熵（bit），参数变化即刷新；
    /// 条形满刻度 128 bit 仅为可视化刻度，UI 不设强弱分级、不重算。
    private var strengthSection: some View {
        Section("强度（A5.1 实时）") {
            if let entropyBits {
                ProgressView(value: entropyBits, total: 128)
                Text("熵：\(entropyBits, specifier: "%.1f") bit（core 计算）")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else {
                Text("熵：—")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var resultSection: some View {
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
        if let generatedPhrase {
            Section("结果") {
                Text(generatedPhrase)
                    .font(.body.monospaced())
                    .textSelection(.enabled)
                Button("复制") { UIPasteboard.general.string = generatedPhrase }
            }
        }
    }

    private func generate() {
        do {
            switch mode {
            case .characters:
                generated = try environment.core.vault().generate(spec: characterSpec())
                generatedPhrase = nil
            case .passphrase:
                generatedPhrase = try environment.core.vault().generatePassphrase(spec: passphraseSpec())
                generated = nil
            }
            errorMessage = nil
        } catch {
            errorMessage = "生成失败（\(error)）"
        }
    }

    /// 强度数值刷新（A5.1 实时）：任一参数变化 → 请 core 计算；UI 只显示返回值。
    private func refreshEntropy() {
        do {
            switch mode {
            case .characters:
                entropyBits = try generatorEntropyBits(spec: characterSpec())
            case .passphrase:
                entropyBits = try passphraseEntropyBits(spec: passphraseSpec())
            }
        } catch {
            entropyBits = nil
        }
    }

    /// 任一参数变化即触发强度刷新（.task(id:) 的刷新键）。
    private var entropyKey: String {
        "\(mode)-\(length)-\(lowercase)-\(uppercase)-\(digits)-\(symbols)-\(wordsText)-\(wordCount)-\(separator)"
    }

    private func characterSpec() -> GeneratorSpec {
        GeneratorSpec(
            length: UInt32(length),
            lowercase: lowercase,
            uppercase: uppercase,
            digits: digits,
            symbols: symbols
        )
    }

    private func passphraseSpec() -> PassphraseSpec {
        PassphraseSpec(words: wordList(), wordCount: UInt32(wordCount), separator: separator)
    }

    private func wordList() -> [String] {
        wordsText.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }
}
