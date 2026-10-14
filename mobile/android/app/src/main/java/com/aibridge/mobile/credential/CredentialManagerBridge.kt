package com.aibridge.mobile.credential

import androidx.credentials.CredentialManager
import com.aibridge.mobile.ffi.AibridgeCore
import com.aibridge.mobile.ffi.Entry

/**
 * CredentialManager 接入桥（客户端方向，支撑 M-F5 一键跳转带态）。
 */
class CredentialManagerBridge(
    private val core: AibridgeCore,
    private val credentialManager: CredentialManager,
) {

    suspend fun publishToSystem(entry: Entry) {
        if (core.vault().isLocked()) {
            return
        }
        core.vault().putEntry(entry)
    }

    suspend fun adoptFromSystem(): List<Entry> {
        if (core.vault().isLocked()) {
            return emptyList()
        }
        return core.vault().listEntries(null).map { summary ->
            core.vault().getEntry(summary.accountRef)
        }
    }
}
