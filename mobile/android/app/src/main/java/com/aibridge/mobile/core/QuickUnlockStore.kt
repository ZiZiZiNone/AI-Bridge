package com.aibridge.mobile.core

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.io.IOException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private const val KEYSTORE_ALIAS = "com.aibridge.mobile.quick-unlock"
private const val CIPHER_TRANSFORMATION = "AES/GCM/NoPadding"
private const val GCM_TAG_BITS = 128
private const val IV_LENGTH = 12

/**
 * 快速解锁（生物识别）因子的宿主存储（S1 / M-F1）。
 *
 * 语义：生物识别仅作解锁因子，主密码仍是唯一根；解锁成功后主口令经 Android Keystore
 * AES-GCM 密钥包裹保存在应用私有目录（密钥不可导出、仅本设备），读取由调用方在系统
 * 生物识别提示通过后触发（见 BiometricGate），解锁仍最终调用 core 的 `Vault.unlock`。
 * 纪律：主口令不进 SharedPreferences / 不进日志 / 不进错误文本。
 * 待确认：主口令的包装保存形态（本实现 = Keystore 包裹直接保存）需与 core 密钥层次设计对齐
 *         （与 iOS QuickUnlockStore 同口径）。
 * 实例由组合根构造并显式传递，不是单例。
 */
class QuickUnlockStore(private val directory: File) {

    /** 首次主口令解锁成功后调用；失败不影响主口令解锁路径。 */
    fun save(masterPassword: String) {
        if (!directory.isDirectory && !directory.mkdirs()) throw IOException("快速解锁因子目录不可用")
        val iv = ByteArray(IV_LENGTH).also { java.security.SecureRandom().nextBytes(it) }
        val ciphertext = cipher(Cipher.ENCRYPT_MODE, iv).doFinal(masterPassword.toByteArray(Charsets.UTF_8))
        File(directory, FACTOR_FILE).writeBytes(iv + ciphertext)
    }

    /** 生物识别提示通过后读取主口令（随后仍须调用 core 解锁）。 */
    fun retrieveMasterPassword(): String {
        val file = File(directory, FACTOR_FILE)
        if (!file.isFile) throw IOException("快速解锁因子不存在")
        val blob = file.readBytes()
        if (blob.size <= IV_LENGTH) throw IOException("快速解锁因子损坏")
        val iv = blob.copyOfRange(0, IV_LENGTH)
        val ciphertext = blob.copyOfRange(IV_LENGTH, blob.size)
        return String(cipher(Cipher.DECRYPT_MODE, iv).doFinal(ciphertext), Charsets.UTF_8)
    }

    /** 关闭快速解锁（S1 设置项）：删除因子，不影响主密码。 */
    fun clear() {
        File(directory, FACTOR_FILE).delete()
        runCatching {
            KeyStore.getInstance("AndroidKeyStore").apply { load(null) }.deleteEntry(KEYSTORE_ALIAS)
        }
    }

    private fun cipher(mode: Int, iv: ByteArray): Cipher =
        Cipher.getInstance(CIPHER_TRANSFORMATION).apply {
            init(mode, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
        }

    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getEntry(KEYSTORE_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(
                KEYSTORE_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build(),
        )
        return generator.generateKey()
    }
}

private const val FACTOR_FILE = "quick-unlock.factor"
