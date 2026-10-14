package com.aibridge.mobile.autofill

import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import com.aibridge.mobile.AppDependencies

/**
 * M9 / M-F3：Android 系统自动填充服务（b 级填表）。
 *
 * 未完成宿主依赖注入或库处于锁定态时安全返回空结果，不抛异常、不阻塞系统填充链路。
 */
class AIBridgeAutofillService(
    private val dependenciesProvider: () -> AppDependencies? = { null },
) : AutofillService() {

    private val dependencies: AppDependencies?
        get() = dependenciesProvider()

    override fun onFillRequest(
        request: FillRequest,
        cancellationSignal: CancellationSignal,
        callback: FillCallback,
    ) {
        if (cancellationSignal.isCanceled) {
            callback.onSuccess(null)
            return
        }
        val deps = dependencies
        if (deps == null || deps.core.vault().isLocked()) {
            callback.onSuccess(null)
            return
        }
        callback.onSuccess(null)
    }

    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
        val deps = dependencies
        if (deps == null || deps.core.vault().isLocked()) {
            callback.onFailure("凭据库未解锁，无法保存条目")
            return
        }
        callback.onSuccess()
    }
}
