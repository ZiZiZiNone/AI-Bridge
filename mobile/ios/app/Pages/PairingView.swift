import CryptoKit
import Foundation
import SwiftUI

/// M7 配对与设备视图（iOS 指纹语义对齐 `core/sync`）：
/// QR 载荷承载的是「X25519 公钥 + 设备信息」（或显式 64 位 hex `fingerprint`），
/// 视图层解析出公钥并计算 SHA-256 小写 hex 指纹后再比对，绝不把整段 QR 原文当作指纹。
public enum PairingQrParser {

    public struct ParsedOffer: Equatable {
        public let ticketId: String
        public let deviceKind: String
        public let fingerprint: String
    }

    public static func isValidFingerprint(_ text: String) -> Bool {
        guard text.count == 64 else { return false }
        return text.utf8.allSatisfy { b in
            (b >= UInt8(ascii: "0") && b <= UInt8(ascii: "9")) ||
            (b >= UInt8(ascii: "a") && b <= UInt8(ascii: "f"))
        }
    }

    public static func sha256Hex(_ data: Data) -> String {
        let digest = SHA256.hash(data: data)
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    public static func decodeHex(_ hex: String) -> Data? {
        guard hex.count % 2 == 0 else { return nil }
        var out = Data(capacity: hex.count / 2)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
            out.append(byte)
            index = next
        }
        return out
    }

    /// 从 QR 原始文本提取与 `core/sync` 一致的 64 位小写 hex 指纹（`sha256(public_key_bytes)`）。
    public static func parseQrPayload(_ rawQr: String) -> ParsedOffer? {
        let trimmed = rawQr.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let data = trimmed.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }

        let ticketId = (json["ticket_id"] as? String) ?? ""
        let deviceKind = (json["device_kind"] as? String) ?? "unknown"

        if let pubKeyHex = (json["public_key"] as? String)?.lowercased(),
           pubKeyHex.count == 64,
           let pubKeyBytes = decodeHex(pubKeyHex) {
            let computed = sha256Hex(pubKeyBytes)
            if let declaredFp = (json["fingerprint"] as? String)?.lowercased(),
               !declaredFp.isEmpty,
               declaredFp != computed {
                return nil
            }
            return ParsedOffer(ticketId: ticketId, deviceKind: deviceKind, fingerprint: computed)
        }

        if let fingerprint = (json["fingerprint"] as? String)?.lowercased(),
           isValidFingerprint(fingerprint) {
            return ParsedOffer(ticketId: ticketId, deviceKind: deviceKind, fingerprint: fingerprint)
        }

        return nil
    }
}

public struct PairingView: View {
    @State private var localFingerprint: String
    @State private var scannedQrPayload: String = ""
    @State private var verificationMessage: String = ""

    public init(localFingerprint: String) {
        self._localFingerprint = State(initialValue: localFingerprint.lowercased())
    }

    public var parsedRemoteOffer: PairingQrParser.ParsedOffer? {
        PairingQrParser.parseQrPayload(scannedQrPayload)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("设备同步（局域网）· 配对与指纹确认")
                .font(.headline)

            Text("本机指纹：\(localFingerprint)")
                .font(.system(.caption, design: .monospaced))

            if let offer = parsedRemoteOffer {
                Text("对端设备：\(offer.deviceKind)")
                    .font(.subheadline)
                Text("对端公钥指纹：\(offer.fingerprint)")
                    .font(.system(.caption, design: .monospaced))
            } else if !scannedQrPayload.isEmpty {
                Text("二维码载荷格式非法（需包含 X25519 公钥或 64 位 hex 指纹）")
                    .foregroundColor(.red)
                    .font(.caption)
            }

            if !verificationMessage.isEmpty {
                Text(verificationMessage)
                    .font(.caption)
            }
        }
        .padding()
    }
}
