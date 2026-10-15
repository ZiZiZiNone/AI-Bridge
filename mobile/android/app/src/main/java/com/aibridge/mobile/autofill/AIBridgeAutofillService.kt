package com.aibridge.mobile.autofill

import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import com.aibridge.mobile.AppDependencies

/**
 * M9 / M-F3：Android 系统自动填充服务骨架（b 级填表）。
 *
 * 职责边界（薄入口）：
 * - 只做「系统请求 → 调用 core 公开面 → 返回 FillResponse」的系统适配与类型翻译；
 * - 解锁判定、账号匹配、去重、保存语义全部在 core；
 * - 仅账号类条目（密码 / passkey / 验证码），不设支付信息 / 身份分类。
 *
 * 装配：本服务由系统实例化（AndroidManifest 声明）。依赖在首次使用时于本实例内构造注入，
 *       不引入全局单例 / 静态门面 / 服务定位器（`by lazy` 是实例级延迟，非全局状态）。
 */
class AIBridgeAutofillService : AutofillService() {

    private val dependencies: AppDependencies by lazy {
        // 装配待 core ab-ffi 绑定与 Android 适配层落地后接入（见 mobile/README.md）。
        TODO("AIBridgeAutofillService 装配待 mobile/ffi 绑定生成后接入")
    }

    override fun onFillRequest(
        request: FillRequest,
        cancellationSignal: CancellationSignal,
        callback: FillCallback,
    ) {
        // 骨架：解析 request.fillContexts 的 AssistStructure → 交 core 匹配账号 →
        // 以 FillResponse（含 Dataset / Authentication）回填。
        // 业务规则一律在 core，本层不做任何匹配或过滤判断。
        callback.onSuccess(null)
    }

    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
        // 骨架：保存需用户确认后交 core 落库（明文只经内存传递，不写日志）。
        callback.onSuccess()
    }
}
