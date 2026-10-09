package com.aibridge.mobile.credential

import androidx.credentials.CredentialManager
import com.aibridge.mobile.ffi.AIBridgeCore
import com.aibridge.mobile.ffi.Entry

/**
 * CredentialManager 接入骨架（androidx.credentials 客户端方向）。
 *
 * 与 [AIBridgeCredentialProviderService] 互补：
 * - `AIBridgeCredentialProviderService`：系统 → 本库（作为凭据提供方响应系统请求）；
 * - 本类：本库 → 系统（App 内触发系统填充 / 把库内条目发布为系统凭据，支撑 M-F5 一键跳转带态）。
 *
 * 边界：本地优先，不联网、无遥测；系统侧凭据只是通道，唯一事实源始终是加密库。
 * 依赖：core 与 CredentialManager 均经构造注入（无单例 / 静态门面 / 服务定位器）。
 */
class CredentialManagerBridge(
    private val core: AIBridgeCore,
    private val credentialManager: CredentialManager,
) {

    /**
     * 出库：把库内条目发布为系统凭据（PasswordCredential / PublicKeyCredential）。
     * 骨架：映射待 core 条目模型（Entry）落地后补齐；本阶段不写系统凭据存储。
     */
    suspend fun publishToSystem(entry: Entry) {
        // 骨架：以 CreateCredentialRequest 提交；仅发布用户显式选择的条目。
    }

    /**
     * 入库：把系统返回的凭据结果归一化为 core 契约凭据记录。
     * 骨架：映射待 core 凭据模型落地后补齐。
     */
    suspend fun adoptFromSystem(): List<Entry> {
        // 骨架：以 GetCredentialRequest 取回后交 core，由 core 决定是否落库。
        return emptyList()
    }
}
