import SwiftUI

/// M5 容器浏览器（全屏 WebView；登录、会话捕获、E1/E2 扩散发送）。
///
/// 薄入口：会话归一化（SessionEnvelope）、扩散、打包全在 core；本视图只收集输入并转发
/// StateBus.capture / SyncEngine.transfer / SyncEngine.pack。
/// 会话捕获/注入由 ContainerSessionBridge（SessionSource trait 实现）完成，含 HttpOnly 与属性（A2 零丢弃）。
struct ContainerBrowserView: View {
    let environment: AppEnvironment
    let initialURL: URL?

    @State private var address: String
    @State private var currentURL: URL
    @State private var captured: SessionEnvelope?
    @State private var message: String?
    @State private var exportedURL: URL?

    private static let defaultURL = URL(string: "about:blank")!

    init(environment: AppEnvironment, initialURL: URL?) {
        self.environment = environment
        self.initialURL = initialURL
        let start = initialURL ?? Self.defaultURL
        _address = State(initialValue: start.absoluteString)
        _currentURL = State(initialValue: start)
    }

    var body: some View {
        PageScaffold(screenId: "M5", title: "容器浏览器", functionPoints: "登录、会话捕获、E1/E2 扩散发送") {
            VStack(spacing: 8) {
                HStack {
                    TextField("网址", text: $address)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .textFieldStyle(.roundedBorder)
                    Button("前往") { go() }
                }

                WebViewContainer(url: currentURL, onNavigate: { address = $0 })

                HStack {
                    Button("捕获会话") { capture() }
                    Button("发送到设备") { sendToPeer() }
                    Button("导出离线包（A8）") { exportPackage() }
                    NavigationLink("状态 / 临时会话") {
                        SessionStatusView(environment: environment)
                    }
                }
                .font(.footnote)

                if let exportedURL {
                    ShareLink(item: exportedURL) {
                        Label("分享离线传送包", systemImage: "square.and.arrow.up")
                    }
                    .font(.footnote)
                }

                if let captured {
                    Text("已捕获会话：\(captured.accountRef)（\(captured.target.domain ?? "-")）")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                if let message {
                    Text(message).font(.caption2).foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("容器浏览器")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink("M6 状态") {
                    SessionStatusView(environment: environment)
                }
            }
        }
    }

    private func go() {
        guard let url = URL(string: address), url.scheme != nil else {
            message = "网址无效"
            return
        }
        currentURL = url
    }

    /// M-F2 步骤 2–3：会话完整捕获（经 core 归一化为 SessionEnvelope）。
    private func capture() {
        do {
            let domain = URL(string: address)?.host ?? address
            captured = try environment.core.state().capture(sourceId: "ios-container", domain: domain)
            message = "捕获成功（含 HttpOnly 与全部属性）"
        } catch {
            message = "捕获失败（\(error)）"
        }
    }

    /// A8 离线传送包导出（对端公钥加密 + 签名时间戳在 core；本层只落文件交系统分享）。
    private func exportPackage() {
        guard let captured else {
            message = "请先捕获会话"
            return
        }
        do {
            let package = try environment.core.sync().pack(envelopes: [captured])
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("aibridge-offline-\(Int(Date().timeIntervalSince1970)).a8pkg")
            try package.write(to: url)
            exportedURL = url
            message = "离线传送包已生成（内容零泄露，可任意渠道搬运）。"
        } catch {
            message = "导出失败（\(error)）"
        }
    }

    /// M-F2 步骤 3：局域网直传到对端（E1/E2）。
    private func sendToPeer() {
        guard let captured else {
            message = "请先捕获会话"
            return
        }
        let peers = environment.transport.peers()
        guard !peers.isEmpty else {
            message = "局域网内未发现设备"
            return
        }
        message = "可选设备：" + peers.map { "\($0.peerId)（\(deviceKindLabel($0.deviceKind))）" }.joined(separator: "、")
        // ponytail: 先把首台设备作为直传目标；多设备选择表单在配对联调后补。
        let target = peers[0]
        do {
            let report = try environment.core.sync().transfer(peerId: target.peerId, envelopes: [captured])
            message = "已发送到 \(target.peerId)：成功 \(report.sent) 条"
        } catch {
            message = "发送失败（\(error)）"
        }
    }
}
