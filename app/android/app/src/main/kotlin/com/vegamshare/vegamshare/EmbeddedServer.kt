package com.vegamshare.vegamshare

import android.content.Context
import android.util.Log
import moe.shizuku.manager.adb.AdbClient
import moe.shizuku.manager.adb.AdbKey
import moe.shizuku.manager.adb.AdbMdns
import moe.shizuku.manager.adb.AdbPairingClient
import moe.shizuku.manager.adb.PreferenceAdbKeyStore
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

object EmbeddedServer {

    private const val TAG = "EmbeddedServer"
    private const val PREFS_NAME = "embedded_adb"
    private const val CONNECT_TIMEOUT_MS = 30_000L
    private const val KEY_PAIRED = "paired"

    /** This app's ADB key was accepted before; Android keeps it until the user revokes it. */
    fun wasPaired(context: Context) =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).getBoolean(KEY_PAIRED, false)

    fun isAvailable(): Boolean = try {
        AdbPairingClient.available()
    } catch (_: Exception) {
        false
    }

    fun getKey(context: Context): AdbKey {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        return AdbKey(PreferenceAdbKeyStore(prefs), "${context.packageName}@VegamShare")
    }

    /**
     * Pair with device via ADB wireless debugging.
     * Must be called from a background thread.
     * @param host  Device IP (from mDNS or user input)
     * @param port  Pairing port shown on device
     * @param code  6-digit pairing code shown on device
     */
    fun pair(context: Context, host: String, port: Int, code: String) {
        val key = getKey(context)
        try {
            AdbPairingClient(host, port, code, key).use { client ->
                val ok = client.start()
                if (!ok) throw AdbPairException(context.getString(R.string.pair_wrong_code))
            }
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit().putBoolean(KEY_PAIRED, true).apply()
        } catch (_: java.net.ConnectException) {
            // Android stops the pairing service as soon as its dialog closes.
            throw AdbPairException(context.getString(R.string.pair_closed))
        }
        Log.d(TAG, "Pairing successful")
    }

    /**
     * Discover ADB connect port via mDNS, then connect and start the embedded server.
     * Must be called from a background thread.
     * @param host  Device IP (same as used for pairing)
     */
    fun connectAndStart(context: Context, host: String) {
        val connectPort = waitForConnectPort(context, host)
        Log.d(TAG, "ADB connect port: $connectPort")

        val key = getKey(context)
        val cmd = PrivServer.startCommand(context.applicationInfo.sourceDir, context.applicationInfo.uid)

        AdbClient(host, connectPort, key).use { client ->
            client.connect()
            Log.d(TAG, "Starting server: $cmd")
            client.shellCommand(cmd) { bytes ->
                Log.d(TAG, "Server: ${String(bytes).trim()}")
            }
        }
        val deadline = System.currentTimeMillis() + BINDER_TIMEOUT_MS
        while (Priv.service == null) {
            if (System.currentTimeMillis() > deadline) throw AdbPairException("The helper did not start.")
            Thread.sleep(200)
        }
        Log.d(TAG, "Server started successfully")
    }

    private const val BINDER_TIMEOUT_MS = 10_000L

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
