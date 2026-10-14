import Foundation
import Security

/// 快速解锁（生物识别）因子的宿主存储（S1 / M-F1）。
///
/// 语义：生物识别仅作解锁因子，主密码仍是唯一根；解锁成功后主口令以 Keychain
/// 生物门控保存（kSecAttrAccessControl + .biometryCurrentSet + ThisDeviceOnly），
/// 读取时由系统弹出 Face ID / Touch ID，解锁仍最终调用 core 的 `Vault.unlock`。
/// 纪律：主口令不进 UserDefaults / 不进日志 / 不进错误文本。
/// 待确认：主口令的包装保存形态（本实现 = Keychain 生物门控直接保存）需与 core 密钥层次设计对齐。
final class QuickUnlockStore {
    private let service = "com.aibridge.mobile.quick-unlock"
    private let account = "master-password"

    /// 首次主口令解锁成功后调用；失败不影响主口令解锁路径。
    func save(masterPassword: String) throws {
        let access = try makeAccessControl()
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrAccessControl as String: access,
            kSecValueData as String: Data(masterPassword.utf8),
        ]
        SecItemDelete(query as CFDictionary)
        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else { throw QuickUnlockError.saveFailed(status) }
    }

    /// 触发系统生物识别提示；成功返回主口令（随后仍须调用 core 解锁）。
    func retrieveMasterPassword() throws -> String {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess, let data = item as? Data, let password = String(data: data, encoding: .utf8) else {
            throw QuickUnlockError.retrieveFailed(status)
        }
        return password
    }

    /// 关闭快速解锁（S1 设置项）：删除因子，不影响主口令。
    func clear() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }

    private func makeAccessControl() throws -> SecAccessControl {
        var error: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly,
            .biometryCurrentSet,
            &error
        ) else {
            throw QuickUnlockError.accessControlUnavailable
        }
        return access
    }
}

enum QuickUnlockError: Error {
    case saveFailed(OSStatus)
    case retrieveFailed(OSStatus)
    case accessControlUnavailable
}
