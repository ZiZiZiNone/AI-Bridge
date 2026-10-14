package com.aibridge.mobile.credential

import android.os.CancellationSignal
import android.os.OutcomeReceiver
import androidx.credentials.exceptions.ClearCredentialException
import androidx.credentials.exceptions.GetCredentialException
import androidx.credentials.provider.BeginClearCredentialStateRequest
import androidx.credentials.provider.BeginGetCredentialRequest
import androidx.credentials.provider.BeginGetCredentialResponse
import androidx.credentials.provider.CredentialProviderService
import com.aibridge.mobile.AppDependencies

/**
 * M9 / M-F3：Android 凭据提供方服务（Android 14+ 系统凭据入口）。
 *
 * 未完成依赖装配或库处于锁定态时返回空候选列表，不触发未实现异常。
 */
class AIBridgeCredentialProviderService(
    private val dependenciesProvider: () -> AppDependencies? = { null },
) : CredentialProviderService() {

    private val dependencies: AppDependencies?
        get() = dependenciesProvider()

    override fun onBeginGetCredentialRequest(
        request: BeginGetCredentialRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<BeginGetCredentialResponse, GetCredentialException>,
    ) {
        if (cancellationSignal.isCanceled) {
            callback.onResult(BeginGetCredentialResponse.Builder().build())
            return
        }
        val deps = dependencies
        if (deps == null || deps.core.vault().isLocked()) {
            callback.onResult(BeginGetCredentialResponse.Builder().build())
            return
        }
        callback.onResult(BeginGetCredentialResponse.Builder().build())
    }

    override fun onClearCredentialStateRequest(
        request: BeginClearCredentialStateRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<Void, ClearCredentialException>,
    ) {
        dependencies?.core?.broker()?.revokeAll()
        callback.onResult(null)
    }
}
