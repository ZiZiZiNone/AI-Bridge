import Foundation
import Network

/// iOS 局域网传输（E1/E2/E3）：
/// 1. Bonjour TXT 记录强制携带并验证 64 位小写 hex `fingerprint`，仅放行受信任指纹；
/// 2. 接收队列有界（`maxInboundQueueSize = 64`，单帧上限 `maxFrameBytes = 1 MiB`）；
/// 3. 明确记录 `lastError`，禁止静默吞错。
public final class LanTransport {
    public static let serviceType = "_aibridge._tcp"
    public static let maxInboundQueueSize = 64
    public static let maxFrameBytes = 1024 * 1024

    public struct PeerRecord: Equatable {
        public let peerId: String
        public let deviceKind: String
        public let fingerprint: String
        public let endpoint: NWEndpoint
    }

    private let lock = NSLock()
    private let localPeerId: String
    private let localFingerprint: String
    private var trustedFingerprints: Set<String>
    private var discoveredPeers: [String: PeerRecord] = [:]
    private var inboundQueue: [Data] = []
    public private(set) var lastError: String?

    public init(
        localPeerId: String,
        localFingerprint: String,
        trustedFingerprints: Set<String> = []
    ) throws {
        guard Self.isValidFingerprint(localFingerprint) else {
            throw NSError(
                domain: "com.aibridge.mobile.lan",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "本机 fingerprint 必须是 64 位小写 hex"]
            )
        }
        self.localPeerId = localPeerId
        self.localFingerprint = localFingerprint
        self.trustedFingerprints = trustedFingerprints
    }

    public static func isValidFingerprint(_ value: String) -> Bool {
        guard value.count == 64 else { return false }
        return value.utf8.allSatisfy { byte in
            (byte >= UInt8(ascii: "0") && byte <= UInt8(ascii: "9")) ||
            (byte >= UInt8(ascii: "a") && byte <= UInt8(ascii: "f"))
        }
    }

    public func trustFingerprint(_ fingerprint: String) throws {
        guard Self.isValidFingerprint(fingerprint) else {
            throw NSError(
                domain: "com.aibridge.mobile.lan",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "对端 fingerprint 格式非法"]
            )
        }
        lock.lock()
        defer { lock.unlock() }
        trustedFingerprints.insert(fingerprint)
    }

    public func buildLocalTxtRecord() -> NWTXTRecord {
        var txt = NWTXTRecord()
        txt["peer_id"] = localPeerId
        txt["fingerprint"] = localFingerprint
        txt["device_kind"] = "ios"
        return txt
    }

    @discardableResult
    public func handleDiscoveredEndpoint(
        _ endpoint: NWEndpoint,
        txtRecord: NWTXTRecord
    ) -> Bool {
        lock.lock()
        defer { lock.unlock() }

        let peerId = (txtRecord["peer_id"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let fingerprint = (txtRecord["fingerprint"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let rawKind = (txtRecord["device_kind"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let deviceKind = rawKind.isEmpty ? "unknown" : rawKind

        guard !peerId.isEmpty, peerId != localPeerId else {
            lastError = "忽略无效 Bonjour 节点：\(peerId)"
            return false
        }
        guard Self.isValidFingerprint(fingerprint) else {
            lastError = "拒绝无合法 TXT fingerprint 的节点：\(peerId)"
            return false
        }
        if !trustedFingerprints.isEmpty && !trustedFingerprints.contains(fingerprint) {
            lastError = "拒绝未配对 fingerprint 的节点：\(peerId)"
            return false
        }

        discoveredPeers[peerId] = PeerRecord(
            peerId: peerId,
            deviceKind: deviceKind,
            fingerprint: fingerprint,
            endpoint: endpoint
        )
        return true
    }

    @discardableResult
    public func enqueueInboundFrame(_ frame: Data, fromFingerprint: String) -> Bool {
        lock.lock()
        defer { lock.unlock() }

        guard Self.isValidFingerprint(fromFingerprint) else {
            lastError = "拒绝未认证来源帧"
            return false
        }
        if !trustedFingerprints.isEmpty && !trustedFingerprints.contains(fromFingerprint) {
            lastError = "拒绝未配对设备发来的局域网帧"
            return false
        }
        guard !frame.isEmpty, frame.count <= Self.maxFrameBytes else {
            lastError = "拒绝长度越界帧：\(frame.count) 字节"
            return false
        }
        guard inboundQueue.count < Self.maxInboundQueueSize else {
            lastError = "接收队列已满（上限 \(Self.maxInboundQueueSize)），丢弃新帧"
            return false
        }
        inboundQueue.append(frame)
        return true
    }

    public func dequeueInboundFrame() -> Data? {
        lock.lock()
        defer { lock.unlock() }
        guard !inboundQueue.isEmpty else { return nil }
        return inboundQueue.removeFirst()
    }

    public func peers() -> [PeerRecord] {
        lock.lock()
        defer { lock.unlock() }
        return discoveredPeers.values.sorted { $0.peerId < $1.peerId }
    }
}
