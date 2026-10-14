package com.aibridge.mobile.ui

import com.aibridge.mobile.ffi.AccessLevel
import com.aibridge.mobile.ffi.AuditAction
import com.aibridge.mobile.ffi.AuditResult
import com.aibridge.mobile.ffi.CredentialKind
import com.aibridge.mobile.ffi.SameSite
import com.aibridge.mobile.ffi.SessionKind
import com.aibridge.mobile.ffi.SourceType
import com.aibridge.mobile.ffi.TargetKind

/**
 * 契约枚举的 UI 展示文案映射：全部 `when` 分支显式穷尽枚举变体，不使用 `else` 分支，
 * 由 Kotlin 编译器保证枚举新增变体时编译期即刻检出。
 */
object Labels {

    fun accessLevel(level: AccessLevel): String = when (level) {
        AccessLevel.C -> "c 会话注入"
        AccessLevel.B -> "b 凭据填表"
        AccessLevel.A -> "a 仅启动"
    }

    fun credentialKind(kind: CredentialKind): String = when (kind) {
        CredentialKind.PASSWORD -> "账号密码"
        CredentialKind.PASSKEY -> "通行密钥 (Passkey)"
        CredentialKind.TOTP -> "动态验证码 (TOTP)"
    }

    fun sessionKind(kind: SessionKind): String = when (kind) {
        SessionKind.COOKIE -> "Cookie 会话"
        SessionKind.TOKEN -> "令牌会话"
        SessionKind.CREDENTIAL -> "凭据会话"
    }

    fun targetKind(kind: TargetKind): String = when (kind) {
        TargetKind.WEB -> "网页站点"
        TargetKind.APP -> "桌面应用"
        TargetKind.CONTAINER -> "容器浏览器"
    }

    fun sameSite(sameSite: SameSite): String = when (sameSite) {
        SameSite.STRICT -> "Strict"
        SameSite.LAX -> "Lax"
        SameSite.NONE -> "None"
    }

    fun sourceType(sourceType: SourceType): String = when (sourceType) {
        SourceType.EXTENSION -> "浏览器扩展"
        SourceType.CONVERTER -> "标准转换器"
        SourceType.MOBILE -> "移动容器"
        SourceType.MANUAL -> "手动录入"
        SourceType.SYNC -> "设备同步（局域网）"
    }

    fun auditAction(action: AuditAction): String = when (action) {
        AuditAction.UNLOCK -> "解锁"
        AuditAction.LOCK -> "锁定"
        AuditAction.CAPTURE -> "捕获会话"
        AuditAction.INJECT -> "注入会话"
        AuditAction.FILL -> "填充凭据"
        AuditAction.ISSUE -> "签发临时会话"
        AuditAction.RELEASE -> "释放临时会话"
        AuditAction.REVOKE -> "吊销临时会话"
        AuditAction.REVOKE_ALL -> "全部吊销"
        AuditAction.SWEEP -> "过期清理"
        AuditAction.EXTRACT -> "转换器提取"
        AuditAction.CONVERTER_LOAD -> "加载转换器"
        AuditAction.CONVERTER_REJECT -> "拒载转换器"
        AuditAction.PAIR -> "设备配对"
        AuditAction.SYNC -> "设备同步（局域网）"
        AuditAction.VAULT_WRITE -> "写入条目"
        AuditAction.VAULT_DELETE -> "删除条目"
        AuditAction.RESTORE -> "恢复备份"
        AuditAction.EXPORT -> "加密导出"
    }

    fun auditResult(result: AuditResult): String = when (result) {
        AuditResult.OK -> "成功"
        AuditResult.DENIED -> "拒绝"
        AuditResult.FAILED -> "失败"
    }
}
