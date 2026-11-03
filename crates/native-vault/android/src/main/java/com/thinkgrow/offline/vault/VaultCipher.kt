package com.thinkgrow.offline.vault

import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

internal object VaultCipher {
    private const val LIMIT = 1048576 + 8192
    fun seal(input: ByteArray, entropy: ByteArray, key: SecretKey): ByteArray {
        require(input.size in 1..LIMIT)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        require(cipher.iv.size == 12)
        cipher.updateAAD(entropy)
        return byteArrayOf(1) + cipher.iv + cipher.doFinal(input)
    }
    fun unseal(input: ByteArray, entropy: ByteArray, key: SecretKey): ByteArray {
        require(input.size in 29..LIMIT && input[0] == 1.toByte())
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, input.copyOfRange(1,13)))
        cipher.updateAAD(entropy)
        return cipher.doFinal(input.copyOfRange(13,input.size))
    }
}
