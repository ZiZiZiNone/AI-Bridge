package com.aibridge.mobile.core

import com.aibridge.mobile.ffi.Entropy
import java.security.SecureRandom

/**
 * Entropy trait 的宿主实现（生成器与密钥派生的随机来源，core 不自取系统随机）。
 * 实例由组合根构造并显式传递，不是单例。
 * 绑定命名待生成后核对：UDL `u32 length` 按 UniFFI 惯例映射为 Kotlin `UInt`。
 */
class SystemEntropy : Entropy {
    private val secureRandom = SecureRandom()

    override fun fill(length: UInt): ByteArray =
        ByteArray(length.toInt()).also { secureRandom.nextBytes(it) }
}
