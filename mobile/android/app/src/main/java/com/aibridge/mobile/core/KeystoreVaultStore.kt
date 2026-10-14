package com.aibridge.mobile.core

import android.net.Uri
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import com.aibridge.mobile.ffi.VaultStore
import java.io.File
import java.io.IOException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private const val KEYSTORE_ALIAS = "com.aibridge.mobile.vault-store"
private const val CIPHER_TRANSFORMATION = "AES/GCM/NoPadding"
private const val GCM_TAG_BITS = 128
private const val IV_LENGTH = 12

/**
 * VaultStore trait 的宿主实现：只存不透明字节，键语义由 core 决定。
 *
 * 保护形态：值经 Android Keystore 的 AES-GCM 密钥包裹后落应用私有目录
 * （密钥不可导出、仅本设备）；与 iOS `KeychainVaultStore` 职责对齐（Keychain 等价物）。
 * 纪律：不写日志、错误文本不含值内容与主口令。
 * 待确认：包裹形态与 core 密钥层次的最终对齐（同 iOS 待确认清单）。
 * 实例由组合根构造并显式传递，不是单例。
 */
class KeystoreVaultStore(private val directory: File) : VaultStore {

    init {
        if (!directory.isDirectory && !directory.mkdirs()) {
            throw IOException("VaultStore 目录不可用")
        }
    }

    override fun read(key: String): ByteArray {
        val file = fileFor(key)
        if (!file.isFile) throw IOException("VaultStore 键不存在")
        val blob = file.readBytes()
        if (blob.size <= IV_LENGTH) throw IOException("VaultStore 数据损坏")
        val iv = blob.copyOfRange(0, IV_LENGTH)
        val ciphertext = blob.copyOfRange(IV_LENGTH, blob.size)
        return cipher(Cipher.DECRYPT_MODE, iv).doFinal(ciphertext)
    }

    override fun write(key: String, value: ByteArray) {
        val iv = ByteArray(IV_LENGTH).also { java.security.SecureRandom().nextBytes(it) }
        val ciphertext = cipher(Cipher.ENCRYPT_MODE, iv).doFinal(value)
        fileFor(key).writeBytes(iv + ciphertext)
    }

    override fun remove(key: String) {
        fileFor(key).delete()
    }

    override fun listKeys(): List<String> =
        directory.listFiles { file -> file.isFile }
            ?.map { Uri.decode(it.name) }
            ?.sorted()
            ?: emptyList()

    /** 键名按可逆转义落文件名（键是 core 内部标识，非秘密）。 */
    private fun fileFor(key: String): File = File(directory, Uri.encode(key))

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
