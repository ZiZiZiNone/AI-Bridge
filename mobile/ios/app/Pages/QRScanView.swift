import Foundation
import SwiftUI

/// M7 扫码视图：扫码结果必须先经 `PairingQrParser.parseQrPayload` 提取 64 位 SHA-256 公钥指纹，
/// 禁止将原始 QR 字符串直接当作 `remote_fingerprint` 传给 `confirm`。
public struct QRScanView: View {
    public let onConfirmOffer: (PairingQrParser.ParsedOffer) -> Void
    @State private var rawQrInput: String = ""
    @State private var errorMessage: String?

    public init(onConfirmOffer: @escaping (PairingQrParser.ParsedOffer) -> Void) {
        self.onConfirmOffer = onConfirmOffer
    }

    public func submitScannedPayload(_ raw: String) -> Bool {
        guard let offer = PairingQrParser.parseQrPayload(raw),
              PairingQrParser.isValidFingerprint(offer.fingerprint) else {
            errorMessage = "二维码不含有效的 X25519 公钥指纹，已拒绝"
            return false
        }
        errorMessage = nil
        onConfirmOffer(offer)
        return true
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("扫描配对二维码")
                .font(.headline)

            TextField("粘贴或扫描配对二维码载荷", text: $rawQrInput)
                .textFieldStyle(.roundedBorder)

            if let errorMessage {
                Text(errorMessage)
                    .foregroundColor(.red)
                    .font(.caption)
            }

            Button("解析并核对指纹") {
                _ = submitScannedPayload(rawQrInput)
            }
        }
        .padding()
    }
}
