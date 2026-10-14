import Foundation
import Network

/// Transport trait 的宿主实现：仅局域网直传（E1/E2 会话扩散、A8 离线包搬运的传输通道）。
///
/// 边界（docs/安全模型.md §7）：仅局域网 —— 不出网、不打洞、无中继、无云；
/// core 不持有任何网络句柄，载荷（bytes）语义由 core 解释，本类只做搬运。
/// 骨架约定（待与桌面端对齐 · 未验证）：
///   - Bonjour 服务类型 `_aibridge._tcp`；
///   - TXT 记录携带 `device_kind`（Windows / iOS / Android）与 `fingerprint`（配对公钥指纹）；
///   - 线帧 = 4 字节大端长度前缀 + 载荷。
final class LanTransport: Transport {
    private let serviceType = "_aibridge._tcp"
    private let deviceKind = "ios"
    private let queue = DispatchQueue(label: "com.aibridge.mobile.lan-transport")

    private var browser: NWBrowser?
    private var listener: NWListener?
    private var discovered: [String: NWBrowser.Result] = [:]
    private var inbound: [Data] = []
    private let lock = NSLock()

    init() {
        startBrowser()
        startListener()
    }

    /// 局域网在线设备（PeerInfo：peer_id / device_kind / fingerprint，取自 Bonjour 服务与 TXT）。
    func peers() -> [PeerInfo] {
        lock.lock()
        defer { lock.unlock() }
        return discovered.map { name, result -> PeerInfo in
            let txt = txtRecords(of: result)
            return PeerInfo(
                peerId: name,
                deviceKind: txt["device_kind"] ?? "unknown",
                fingerprint: txt["fingerprint"] ?? ""
            )
        }
        .sorted { $0.peerId < $1.peerId }
    }

    /// 向指定设备直发载荷（失败抛错；重试语义在 core）。
    func send(peerId: String, payload: Data) throws {
        lock.lock()
        let result = discovered[peerId]
        lock.unlock()
        guard let result = result else { throw TransportError.peerNotFound(peerId) }

        let connection = NWConnection(to: result.endpoint, using: .tcp)
        let done = DispatchSemaphore(value: 0)
        var failure: Error?
        connection.stateUpdateHandler = { [weak connection] state in
            switch state {
            case .ready:
                connection?.send(content: Self.framed(payload), completion: .contentProcessed { error in
                    failure = error.map { _ in TransportError.sendFailed(peerId) }
                    connection?.cancel()
                    done.signal()
                })
            case .failed(let error):
                failure = error
                connection?.cancel()
                done.signal()
            case .cancelled:
                done.signal()
            default:
                break
            }
        }
        connection.start(queue: queue)
        if done.wait(timeout: .now() + 10) == .timedOut {
            connection.cancel()
            throw TransportError.sendFailed(peerId)
        }
        if let failure = failure { throw failure }
    }

    /// 收取对端载荷（超时返回 nil；超时毫秒由 core 传入）。
    func receive(timeoutMillis: UInt32) -> Data? {
        let deadline = DispatchTime.now() + .milliseconds(Int(timeoutMillis))
        while true {
            lock.lock()
            if !inbound.isEmpty {
                let data = inbound.removeFirst()
                lock.unlock()
                return data
            }
            lock.unlock()
            if DispatchTime.now() >= deadline { return nil }
            Thread.sleep(forTimeInterval: 0.05)
        }
    }

    // MARK: - 发现与监听

    private func startBrowser() {
        let browser = NWBrowser(for: .bonjour(type: serviceType, domain: nil), using: .tcp)
        browser.stateUpdateHandler = { [weak self] _, changes in
            guard let self = self else { return }
            self.lock.lock()
            for change in changes {
                switch change {
                case .added(let result), .changed(let result, _, _):
                    self.discovered[self.name(of: result)] = result
                case .removed(let result):
                    self.discovered.removeValue(forKey: self.name(of: result))
                default:
                    break
                }
            }
            self.lock.unlock()
        }
        browser.start(queue: queue)
        self.browser = browser
    }

    private func startListener() {
        let parameters = NWParameters.tcp
        parameters.includePeerToPeer = true
        guard let listener = try? NWListener(using: parameters, service: NWListener.Service(name: nil, type: serviceType, txtRecord: Self.txtRecord())) else {
            return
        }
        listener.newConnectionHandler = { [weak self] connection in
            self?.accept(connection)
        }
        listener.start(queue: queue)
        self.listener = listener
    }

    private func accept(_ connection: NWConnection) {
        connection.start(queue: queue)
        receiveChunk(from: connection, buffer: Data())
    }

    /// 按 4 字节大端长度前缀重组载荷；完整载荷入 inbound 队列。
    private func receiveChunk(from connection: NWConnection, buffer: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 4 * 1024 * 1024) { [weak self] data, _, isComplete, error in
            guard let self = self else { return }
            var buffer = buffer
            if let data = data, !data.isEmpty { buffer.append(data) }
            while buffer.count >= 4 {
                let length = Int(buffer.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 0, as: UInt32.self) }.bigEndian)
                guard buffer.count >= 4 + length else { break }
                let payload = buffer.subdata(in: 4..<(4 + length))
                buffer.removeSubrange(0..<(4 + length))
                self.lock.lock()
                self.inbound.append(payload)
                self.lock.unlock()
            }
            if isComplete || error != nil {
                connection.cancel()
            } else {
                self.receiveChunk(from: connection, buffer: buffer)
            }
        }
    }

    private func name(of result: NWBrowser.Result) -> String {
        if case let .service(name, _, _, _) = result.endpoint { return name }
        return "\(result.endpoint)"
    }

    private func txtRecords(of result: NWBrowser.Result) -> [String: String] {
        guard case let .service(_, _, _, txtRecord) = result.endpoint, let record = txtRecord else { return [:] }
        var values: [String: String] = [:]
        for key in ["device_kind", "fingerprint"] {
            values[key] = record[key]
        }
        return values
    }

    private static func framed(_ payload: Data) -> Data {
        var length = UInt32(payload.count).bigEndian
        var frame = Data(bytes: &length, count: 4)
        frame.append(payload)
        return frame
    }

    private static func txtRecord() -> NWTXTRecord? {
        NWTXTRecord(["device_kind": "ios"])
    }
}

enum TransportError: Error {
    case peerNotFound(String)
    case sendFailed(String)
}
