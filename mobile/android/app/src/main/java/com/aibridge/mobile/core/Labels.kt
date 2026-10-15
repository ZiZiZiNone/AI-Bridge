package com.aibridge.mobile.core

import com.aibridge.mobile.ffi.CredentialKind
import com.aibridge.mobile.ffi.SourceType
import com.aibridge.mobile.ffi.TargetKind

/**
 * UI 文案口径集中处（便于审 docs/功能点全景.md §5 边界速查）。
 *
 * 边界：条目类型只有 账号密码 / passkey / TOTP（不设支付信息 / 身份分类）；
 * 设备仅 Windows / iOS / Android（不出现 macOS）；同步文案只写「设备同步（局域网）」，
 * 全部 UI 不出现"云同步"。
 * 本文件只含纯函数与常量（无状态、无依赖获取），与 iOS `app/Core/Labels.swift` 同口径。
 */

/** 同步项唯一文案（M8：禁"云同步"）。 */
const val SYNC_ITEM_LABEL = "设备同步（局域网）"

/** 条目类型标签（A1：账号密码 / passkey / TOTP）。 */
fun credentialKindLabel(kind: CredentialKind): String = when (kind) {
    CredentialKind.PASSWORD -> "账号密码"
    CredentialKind.PASSKEY -> "passkey"
    CredentialKind.TOTP -> "TOTP"
}

/** 目标类型标签（会话矩阵列）。 */
fun targetKindLabel(kind: TargetKind): String = when (kind) {
    TargetKind.WEB -> "Web"
    TargetKind.APP -> "App"
    TargetKind.CONTAINER -> "容器"
}

/** 来源标记（A2.3：每条会话标注来自哪个端 / 转换器 / 扩散事件）。 */
fun sourceTypeLabel(source: SourceType): String = when (source) {
    SourceType.EXTENSION -> "扩展"
    SourceType.CONVERTER -> "转换器"
    SourceType.MOBILE -> "手机"
    SourceType.MANUAL -> "手动"
    SourceType.SYNC -> SYNC_ITEM_LABEL
}

/** 设备类型标签（E3：设备仅 Windows / iOS / Android；core 侧枚举值原样映射）。 */
fun deviceKindLabel(rawKind: String): String = when (rawKind.lowercased()) {
    "windows" -> "Windows"
    "ios" -> "iOS"
    "android" -> "Android"
    else -> rawKind
}

/**
 * core 错误的展示文本（统一格式化，全仓一份）。
 * 纪律：错误文本不得携带秘密（主口令 / 秘钥 / cookie 值 / TOTP 种子）；
 * 绑定生成的错误类型命名待生成后核对，故以 `Throwable` 兜底取消息。
 */
fun errorText(error: Throwable): String = error.message ?: error.toString()

/**
 * 指纹分组展示（纯显示格式化，不含比对判断；比对与拒绝在 core）。
 */
fun groupedFingerprint(value: String): String {
    val builder = StringBuilder()
    value.forEachIndexed { index, character ->
        if (index > 0 && index % 4 == 0) builder.append(' ')
        builder.append(character)
    }
    return builder.toString()
}
