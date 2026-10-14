import Foundation
import Security

/// Entropy trait 的宿主实现：OS CSPRNG（SecRandomCopyBytes）。
/// 用途：生成器与密钥派生的随机来源（core 不自取系统随机）。
final class SystemEntropy: Entropy {
    func fill(length: UInt32) -> Data {
        var bytes = Data(count: Int(length))
        let status: OSStatus = bytes.withUnsafeMutableBytes { (buffer: UnsafeMutableRawBufferPointer) in
            SecRandomCopyBytes(kSecRandomDefault, Int(length), buffer.baseAddress!)
        }
        // 熵源失败必须 fail-closed：绝不返回弱随机。
        guard status == errSecSuccess else {
            fatalError("SecRandomCopyBytes 失败（status=\(status)）：拒绝返回弱随机")
        }
        return bytes
    }
}
