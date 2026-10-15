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
 * M9 / M-F3：Android 凭据提供方骨架（androidx.credentials，Android 14+ 系统凭据入口）。
 *
 * 覆盖 passkey 与密码类条目；不含支付信息 / 身份分类。
 * 职责边界：只做「系统请求 → core 候选 → 系统条目类型」的翻译；匹配与解锁判定在 core。
 * 装配：系统实例化本服务；依赖在本实例内构造注入（无全局单例 / 静态门面 / 服务定位器）。
 */
class AIBridgeCredentialProviderService : CredentialProviderService() {

    private val dependencies: AppDependencies by lazy {
        // 装配待 core ab-ffi 绑定与 Android 适配层落地后接入（见 mobile/README.md）。
        TODO("AIBridgeCredentialProviderService 装配待 mobile/ffi 绑定生成后接入")
    }

    override fun onBeginGetCredentialRequest(
        request: BeginGetCredentialRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<BeginGetCredentialResponse, GetCredentialException>,
    ) {
        // 骨架：按 request.callingRequest 的请求类型（密码 / 公钥）向 core 取候选，
        // 转成 PasswordCredentialEntry / PublicKeyCredentialEntry 后随 BeginGetCredentialResponse 返回。
        callback.onResult(BeginGetCredentialResponse.Builder().build())
    }

    override fun onClearCredentialStateRequest(
        request: BeginClearCredentialStateRequest,
        cancellationSignal: CancellationSignal,
        callback: OutcomeReceiver<Void, ClearCredentialException>,
    ) {
        // 骨架：清理本应用的凭据状态（撤销临时会话的凭据可见性）。
        callback.onResult(null)
    }
}
