package com.aibridge.mobile.core

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Android 生物识别快速解锁存储（P0-2 闭环）：
 * 1. Keystore 密钥强制 `setUserAuthenticationRequired(true)` 与 `setInvalidatedByBiometricEnrollment(true)`；
 * 2. 仅当用户显式开启快速解锁（`userOptIn == true`）时才允许保存，禁止默认静默落盘。
 */
class QuickUnlockStore(private val context: Context) {

    companion object {
        private const val KEYSTORE_PROVIDER = "AndroidKeyStore"
        private const val KEY_ALIAS = "aibridge_quick_unlock_v1"
        private const val PREFS_NAME = "aibridge_quick_unlock"
        private const val KEY_OPT_IN = "opt_in"
        private const val KEY_CIPHERTEXT = "ciphertext"
        private const val KEY_IV = "iv"
    }

    private val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    var isUserOptedIn: Boolean
        get() = prefs.getBoolean(KEY_OPT_IN, false)
        set(value) {
            prefs.edit().putBoolean(KEY_OPT_IN, value).apply()
            if (!value) {
                clear()
            }
        }

    private fun getOrCreateBiometricKey(): SecretKey {
        val ks = KeyStore.getInstance(KEYSTORE_PROVIDER).apply { load(null) }
        (ks.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }

        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE_PROVIDER)
        val spec = KeyGenParameterSpec.Builder(
            KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
            .build()
        generator.init(spec)
        return generator.generateKey()
    }

    fun saveIfOptedIn(masterPassword: String, authenticatedCipher: Cipher): Boolean {
        if (!isUserOptedIn) {
            return false
        }
        val ciphertext = authenticatedCipher.doFinal(masterPassword.toByteArray(Charsets.UTF_8))
        val iv = authenticatedCipher.iv
        prefs.edit()
            .putString(KEY_CIPHERTEXT, android.util.Base64.encodeToString(ciphertext, android.util.Base64.NO_WRAP))
            .putString(KEY_IV, android.util.Base64.encodeToString(iv, android.util.Base64.NO_WRAP))
            .apply()
        return true
    }

    fun createEncryptCipher(): Cipher {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, getOrCreateBiometricKey())
        return cipher
    }

    fun createDecryptCipher(): Cipher? {
        if (!isUserOptedIn) return null
        val ivBase64 = prefs.getString(KEY_IV, null) ?: return null
        val iv = android.util.Base64.decode(ivBase64, android.util.Base64.NO_WRAP)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, getOrCreateBiometricKey(), GCMParameterSpec(128, iv))
        return cipher
    }

    fun unlockWithAuthenticatedCipher(authenticatedCipher: Cipher): String? {
        if (!isUserOptedIn) return null
        val ctBase64 = prefs.getString(KEY_CIPHERTEXT, null) ?: return null
        val ct = android.util.Base64.decode(ctBase64, android.util.Base64.NO_WRAP)
        return String(authenticatedCipher.doFinal(ct), Charsets.UTF_8)
    }

    fun clear() {
        prefs.edit().remove(KEY_CIPHERTEXT).remove(KEY_IV).apply()
        val ks = KeyStore.getInstance(KEYSTORE_PROVIDER).apply { load(null) }
        if (ks.containsAlias(KEY_ALIAS)) {
            ks.deleteEntry(KEY_ALIAS)
        }
    }
}
