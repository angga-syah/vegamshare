package com.vegamshare.vegamshare

import android.content.Context
import android.util.Log
import moe.shizuku.manager.adb.AdbClient
import moe.shizuku.manager.adb.AdbKey
import moe.shizuku.manager.adb.AdbMdns
import moe.shizuku.manager.adb.AdbPairingClient
import moe.shizuku.manager.adb.PreferenceAdbKeyStore
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

object EmbeddedServer {

    private const val TAG = "EmbeddedServer"
    private const val PREFS_NAME = "embedded_adb"
    private const val CONNECT_TIMEOUT_MS = 30_000L

    fun isAvailable(): Boolean = try {
        AdbPairingClient.available()
    } catch (_: Exception) {
        false
    }

    fun getKey(context: Context): AdbKey {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        return AdbKey(PreferenceAdbKeyStore(prefs), "${context.packageName}@VegamShare")
    }

    fun extractApk(context: Context): File {
        val dest = File(context.getExternalFilesDir(null) ?: context.filesDir, "shizuku.apk")
        if (!dest.exists() || dest.length() < 1_000_000L) {
            context.assets.open("shizuku.apk").use { input ->
                dest.outputStream().use { output -> input.copyTo(output) }
            }
            Log.d(TAG, "Extracted shizuku.apk to ${dest.absolutePath}")
        }
        return dest
    }

    /**
     * Pair with device via ADB wireless debugging.
     * Must be called from a background thread.
     */
    fun pair(context: Context, host: String, port: Int, code: String) {
        val key = getKey(context)
        AdbPairingClient(host, port, code, key).use { client ->
            val ok = client.start()
            if (!ok) throw AdbPairException("Pairing failed — check the code and try again.")
        }
        Log.d(TAG, "Pairing successful")
    }

    /**
     * Discover ADB connect port via mDNS, then connect and start the embedded server.
     * Must be called from a background thread.
     */
    fun connectAndStart(context: Context, host: String) {
        val connectPort = waitForConnectPort(context, host)
        Log.d(TAG, "ADB connect port: $connectPort")

        val key = getKey(context)
        val apkFile = extractApk(context)
        val soPath = "${context.applicationInfo.nativeLibraryDir}/libshizuku.so"

        AdbClient(host, connectPort, key).use { client ->
            client.connect()
            val cmd = "exec 3>&1; $soPath --apk=${apkFile.absolutePath}"
            Log.d(TAG, "Starting server: $cmd")
            client.shellCommand(cmd) { bytes ->
                Log.d(TAG, "Server: ${String(bytes).trim()}")
            }
        }
        Log.d(TAG, "Server started successfully")
    }

    private fun waitForConnectPort(context: Context, expectedHost: String): Int {
        var foundPort = -1
        val latch = CountDownLatch(1)

        val mdns = AdbMdns(context, AdbMdns.TLS_CONNECT) { host, port ->
            if (port != -1) {
                foundPort = port
                latch.countDown()
            }
        }
        mdns.start()
        latch.await(CONNECT_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        mdns.stop()

        if (foundPort == -1) {
            throw AdbPairException("ADB connect port not found. Make sure Wireless debugging is enabled.")
        }
        return foundPort
    }
}

class AdbPairException(message: String) : Exception(message)
