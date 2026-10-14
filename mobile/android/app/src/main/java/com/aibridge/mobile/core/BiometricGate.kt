package com.aibridge.mobile.core

import android.content.Context
import android.hardware.biometrics.BiometricPrompt
import android.os.CancellationSignal
import java.util.concurrent.Executor

/**
 * 系统生物识别提示（S1 解锁因子门 / M-F1）。
 *
 * 归属：宿主能力（非 core trait）；只做提示与回调转发，解锁语义仍经 core `Vault.unlock`。
 * 平台 API：android.hardware.biometrics.BiometricPrompt（框架 API 28+，minSdk 29 覆盖）。
 * 边界：生物识别仅作解锁因子，主密码仍是唯一根；本类不持有任何因子内容。
 * 实例由组合根构造并显式传递，不是单例。
 */
class BiometricGate(private val context: Context) {

    /** 触发系统生物识别提示；成功回调 onSuccess，失败 / 取消回调 onError（消息不含任何秘密）。 */
    fun authenticate(
        title: String,
        subtitle: String,
        negativeLabel: String,
        onSuccess: () -> Unit,
        onError: (String) -> Unit,
    ) {
        val executor: Executor = context.mainExecutor
        val prompt = BiometricPrompt.Builder(context)
            .setTitle(title)
            .setSubtitle(subtitle)
            .setNegativeButton(negativeLabel, executor) { _, _ -> onError("已取消") }
            .build()
        prompt.authenticate(
            CancellationSignal(),
            executor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = onSuccess()

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) =
                    onError(errString.toString())

                override fun onAuthenticationFailed() {
                    // 单次识别失败：系统继续提示，不改任何状态。
                }
            },
        )
    }
}
