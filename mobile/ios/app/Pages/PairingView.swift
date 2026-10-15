import SwiftUI
import UniformTypeIdentifiers

/// M7 配对与设备（E3、A8；主屏五 tab 之「设备」）。
///
/// 含指纹确认屏（FingerprintConfirmView）：双方指纹比对、不一致必须拒绝（M-F4）。
/// 设备仅 Windows / iOS / Android —— 不出现 macOS。
/// 薄入口：X25519 指纹配对、设备状态、离线包验时（A8.3 过期拒解）全在 core（ab-sync / ab-state）；
/// 本视图只收集输入并转发 pair / confirm / unpack 与 Transport.peers。
struct PairingView: View {
    let environment: AppEnvironment

    @State private var ticket: PairingTicket?
    @State private var remoteFingerprint = ""
    @State private var showScanner = false
    @State private var showConfirm = false
    @State private var showImporter = false
    @State private var peers: [PeerInfo] = []
    @State private var message: String?

    var body: some View {
        PageScaffold(screenId: "M7", title: "配对与设备（含指纹确认屏）", functionPoints: "E3、A8") {
            List {
                Section("配对（E3）") {
                    Button("扫描桌面 QR") { showScanner = true }
                    Text("仅局域网 / 面对面配对（E3）。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Section("设备列表（E3）") {
                    if peers.isEmpty {
                        Text("未发现设备").foregroundStyle(.secondary)
                    }
                    ForEach(peers, id: \.peerId) { peer in
                        VStack(alignment: .leading, spacing: 2) {
                            Text("\(peer.peerId)（\(deviceKindLabel(peer.deviceKind))）")
                            Text("指纹：\(groupedFingerprint(peer.fingerprint))")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Text("设备仅 Windows / iOS / Android。解绑入口待 core 接口（见 README 待确认）。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Section("离线传送包（A8）") {
                    Button("导入离线传送包") { showImporter = true }
                    Text("导入时校验时间戳、拒解过期包（A8.3，由 core 强制）。导出入口在「浏览器」捕获会话后（M5）。")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                if let message {
                    Section { Text(message).font(.footnote) }
                }
            }
            .fullScreenCover(isPresented: $showScanner) {
                QRScanView { code in
                    showScanner = false
                    startPairing(remoteFingerprint: code)
                }
                .ignoresSafeArea()
            }
            .sheet(isPresented: $showConfirm) {
                if let ticket {
                    FingerprintConfirmView(
                        localFingerprint: ticket.fingerprint,
                        remoteFingerprint: remoteFingerprint,
                        onConfirm: { confirmPairing(ticket) },
                        onReject: {
                            showConfirm = false
                            message = "已拒绝配对（指纹不一致时必须拒绝）。"
                        }
                    )
                }
            }
            .fileImporter(isPresented: $showImporter, allowedContentTypes: [.data]) { result in
                importPackage(result)
            }
            .task {
                peers = environment.transport.peers()
            }
        }
        .navigationTitle("设备")
    }

    /// M-F4 步骤 1–2：扫 QR → 建配对票据（本机指纹由 core 派生）。
    private func startPairing(remoteFingerprint: String) {
        do {
            self.remoteFingerprint = remoteFingerprint
            ticket = try environment.core.sync().pair()
            showConfirm = true
        } catch {
            message = "配对失败（\(error)）"
        }
    }

    /// M-F4 步骤 2–3：双方指纹比对（core 强制，不一致抛错并拒绝）→ 设备入列表。
    private func confirmPairing(_ ticket: PairingTicket) {
        do {
            _ = try environment.core.sync().confirm(
                ticketId: ticket.ticketId,
                remoteFingerprint: remoteFingerprint
            )
            showConfirm = false
            message = "配对完成，设备已入列表。"
            peers = environment.transport.peers()
        } catch {
            // 指纹不一致（或票据失效）→ 配对不成立；拒绝语义由 core 强制（M-F4）。
            showConfirm = false
            message = "配对被拒绝（\(error)）：指纹不一致必须拒绝。"
        }
    }

    /// A8：导入离线传送包（过期拒解在 core）。
    private func importPackage(_ result: Result<URL, Error>) {
        do {
            let url = try result.get()
            let data = try Data(contentsOf: url)
            let envelopes = try environment.core.sync().unpack(package: data)
            message = "导入成功：\(envelopes.count) 条会话（过期包由 core 拒解）。"
        } catch {
            message = "导入失败（\(error)）"
        }
    }
}
