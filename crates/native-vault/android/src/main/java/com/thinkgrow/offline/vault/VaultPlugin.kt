package com.thinkgrow.offline.vault

import android.app.Activity
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.net.URL
import java.security.KeyStore
import java.util.concurrent.Executors
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.net.ssl.HttpsURLConnection
import org.json.JSONObject

@InvokeArg
internal class VaultArgs { lateinit var data: String; lateinit var entropy: String }

@TauriPlugin
class VaultPlugin(private val activity: Activity): Plugin(activity) {
    private val executor = Executors.newSingleThreadExecutor()
    private fun key(create: Boolean): SecretKey {
        val alias = activity.packageName + ".thinkgrow-vault.v1"
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (store.containsAlias(alias)) return (store.getEntry(alias, null) as KeyStore.SecretKeyEntry).secretKey
        check(create) // Never replace a missing key while trying to decrypt old data.
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).setRandomizedEncryptionRequired(true).build())
        return generator.generateKey()
    }
    private fun execute(invoke: Invoke, operation: (ByteArray, ByteArray) -> ByteArray) {
        val args = try { invoke.parseArgs(VaultArgs::class.java) } catch (_: Exception) { invoke.reject("Native operation rejected"); return }
        executor.execute {
            var input: ByteArray? = null
            try {
                require(args.data.length <= 2 * 1024 * 1024 && args.entropy.length <= 512)
                val bytes = Base64.decode(args.data, Base64.NO_WRAP); input = bytes
                val entropy = Base64.decode(args.entropy, Base64.NO_WRAP)
                val result = operation(bytes, entropy)
                try { invoke.resolve(JSObject().put("data", Base64.encodeToString(result,Base64.NO_WRAP))) }
                finally { result.fill(0) }
            } catch (_: Exception) { invoke.reject("Native protected operation failed; existing data retained") }
            finally { input?.fill(0) }
        }
    }
    @Command
    fun seal(invoke: Invoke) = execute(invoke) { data, entropy ->
        require(String(entropy, Charsets.UTF_8) == activity.packageName)
        VaultCipher.seal(data,entropy,key(true))
    }
    @Command
    fun unseal(invoke: Invoke) = execute(invoke) { data, entropy ->
        require(String(entropy, Charsets.UTF_8) == activity.packageName)
        VaultCipher.unseal(data,entropy,key(false))
    }
    @Command
    fun redeem(invoke: Invoke) = post(invoke, "devices/redeem", 16384, 262144, false)
    @Command
    fun lessons(invoke: Invoke) = post(invoke, "lessons/prepare", 131072, 1048576, true)
    @Command
    fun roleExchange(invoke: Invoke) = post(invoke, "roles/exchange", 262144, 2097152, true)
    private fun post(invoke: Invoke, path: String, maxRequest: Int, maxResponse: Int, signedRequest: Boolean) = execute(invoke) { data, entropy ->
        require(entropy.isEmpty() && data.size <= maxRequest)
        val request = JSONObject(String(data,Charsets.UTF_8))
        val bound = if(signedRequest) JSONObject(request.getString("payload")) else request
        require(bound.getString("app_id") == activity.packageName)
        val connection = URL("https://schoolclassscorecheck.web.app/api/offline/v1/" + path).openConnection() as HttpsURLConnection
        try {
            connection.instanceFollowRedirects = false
            connection.connectTimeout = 8000; connection.readTimeout = 20000
            connection.requestMethod = "POST"; connection.doOutput = true; connection.useCaches = false
            connection.setRequestProperty("Content-Type", "application/json")
            connection.setFixedLengthStreamingMode(data.size)
            connection.outputStream.use { it.write(data) }
            if (path == "roles/exchange" && connection.responseCode == 409) return@execute "{\"conflict\":true}".toByteArray()
            require(connection.responseCode in listOf(200,201))
            require(connection.contentLengthLong <= maxResponse)
            connection.inputStream.use { stream ->
                val result = java.io.ByteArrayOutputStream(); val buffer = ByteArray(8192)
                while (true) { val count = stream.read(buffer); if(count < 0) break; require(result.size()+count <= maxResponse); result.write(buffer,0,count) }
                buffer.fill(0); result.toByteArray()
            }
        } finally { connection.disconnect() }
    }
}
