import Foundation
import Security

/// VaultStore trait 的宿主实现：Keychain（kSecClassGenericPassword）存取不透明字节。
///
/// 归属：宿主实现 core 的 `VaultStore` trait（core 经构造注入调用）；键值语义由 core 决定，
/// 本类只做 key → 条目的字节存取，不做任何解释 / 转换 / 业务判断。
/// 纪律：秘密不进日志（本类不打印任何 value）；错误只含定位信息（key 名 / OSStatus）。
final class KeychainVaultStore: VaultStore {
    private let service: String

    init(service: String) {
        self.service = service
    }

    func read(key: String) throws -> Data {
        var query = baseQuery(key: key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess, let data = item as? Data else {
            throw VaultStoreError.notFound(key: key, status: status)
        }
        return data
    }

    func write(key: String, value: Data) throws {
        let attributes: [String: Any] = [kSecValueData as String: value]
        let updateStatus = SecItemUpdate(baseQuery(key: key) as CFDictionary, attributes as CFDictionary)
        if updateStatus == errSecSuccess { return }

        var insert = baseQuery(key: key)
        insert[kSecValueData as String] = value
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let addStatus = SecItemAdd(insert as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw VaultStoreError.writeFailed(key: key, status: addStatus)
        }
    }

    func remove(key: String) throws {
        let status = SecItemDelete(baseQuery(key: key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw VaultStoreError.removeFailed(key: key, status: status)
        }
    }

    func listKeys() -> [String] {
        var query = baseQuery(key: nil)
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitAll
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess, let items = result as? [[String: Any]] else { return [] }
        return items.compactMap { $0[kSecAttrAccount as String] as? String }.sorted()
    }

    private func baseQuery(key: String?) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
        ]
        if let key = key {
            query[kSecAttrAccount as String] = key
        }
        return query
    }
}

/// VaultStore 宿主侧错误（不含任何秘密值；与 core 的 CoreError 无映射关系，core 自行翻译）。
enum VaultStoreError: Error {
    case notFound(key: String, status: OSStatus)
    case writeFailed(key: String, status: OSStatus)
    case removeFailed(key: String, status: OSStatus)
}
