import SwiftUI

/// M7 指纹确认屏（E3 / M-F4）：展示双方指纹，用户目视比对后确认或拒绝。
///
/// 强制规则：双方指纹不一致必须拒绝 —— 比对与拒绝由 core（`SyncEngine.confirm`）强制执行
/// （confirm 在 remote_fingerprint 与对端实测指纹不一致时抛 CoreError，配对不成立）；
/// 本视图是薄入口，只展示与转发，不自行判定一致性（docs/移动端流程.md M-F4）。
struct FingerprintConfirmView: View {
    let localFingerprint: String
    let remoteFingerprint: String
    let onConfirm: () -> Void
    let onReject: () -> Void

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text("指纹确认（E3）")
                    .font(.headline)
                Text("请核对两端指纹是否完全一致；不一致必须拒绝。")
                    .font(.footnote)

                fingerprintBlock(title: "本机指纹", value: localFingerprint)
                fingerprintBlock(title: "对端指纹（扫码所得）", value: remoteFingerprint)

                HStack {
                    Button("拒绝（不一致）", role: .destructive, action: onReject)
                        .frame(maxWidth: .infinity)
                    Button("确认配对", action: onConfirm)
                        .buttonStyle(.borderedProminent)
                        .frame(maxWidth: .infinity)
                }

                Spacer()
            }
            .padding()
            .navigationTitle("指纹确认屏")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消", action: onReject)
                }
            }
        }
    }

    private func fingerprintBlock(title: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(groupedFingerprint(value))
                .font(.body.monospaced())
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 10))
    }
}

/// 指纹分组展示（纯显示格式化，不含比对判断；比对与拒绝在 core）。
func groupedFingerprint(_ value: String) -> String {
    var result = ""
    for (index, character) in value.enumerated() {
        if index > 0, index % 4 == 0 { result += " " }
        result.append(character)
    }
    return result
}
