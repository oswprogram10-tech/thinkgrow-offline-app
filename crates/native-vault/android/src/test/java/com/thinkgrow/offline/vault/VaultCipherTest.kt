package com.thinkgrow.offline.vault
import javax.crypto.KeyGenerator
import org.junit.Assert.*
import org.junit.Test
class VaultCipherTest {
    private fun key() = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
    @Test fun roundtripAndNonce() {
        val key=key();val bytes="합성 보호 자료".toByteArray();val aad="student".toByteArray()
        val first=VaultCipher.seal(bytes,aad,key);val second=VaultCipher.seal(bytes,aad,key)
        assertArrayEquals(bytes,VaultCipher.unseal(first,aad,key));assertFalse(first.contentEquals(second))
    }
    @Test fun roleAndKeyBinding() {
        val key=key();val sealed=VaultCipher.seal("content".toByteArray(),"student".toByteArray(),key)
        for (wrong in listOf<Pair<ByteArray,javax.crypto.SecretKey>>(Pair("parent".toByteArray(),key),Pair("student".toByteArray(),key()))) {
            try { VaultCipher.unseal(sealed,wrong.first,wrong.second);fail("must reject") }catch (_:javax.crypto.AEADBadTagException) {}
        }
    }
    @Test fun corruptionAndUnsupportedFormat() {
        val key=key();val aad="student".toByteArray();val sealed=VaultCipher.seal("content".toByteArray(),aad,key)
        sealed[sealed.lastIndex]=(sealed.last().toInt() xor 1).toByte()
        try{VaultCipher.unseal(sealed,aad,key);fail("must reject corruption")}catch (_:javax.crypto.AEADBadTagException){}
        try{VaultCipher.unseal(byteArrayOf(2),aad,key);fail("must reject format")}catch (_:IllegalArgumentException){}
    }
}
