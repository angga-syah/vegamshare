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
        val key = getKey(context)
        val cmd = PrivServer.startCommand(context.applicationInfo.sourceDir, context.applicationInfo.uid)

        // mDNS is the normal way to learn the port, but some phones stop announcing it
        // (seen on ColorOS after a restart); then look for adbd among the local ports.
        val mdnsPort = findConnectPortByMdns(context)
        val candidates = if (mdnsPort != -1) listOf(mdnsPort) else localOpenPorts()
        Log.d(TAG, "ADB connect port candidates: $candidates")
        var started = false
        var lastError: Exception? = null
        for (port in candidates) {
            try {
                AdbClient(LOCALHOST, port, key).use { client ->
                    client.connect()
                    Log.d(TAG, "Starting server on port $port: $cmd")
                    client.shellCommand(cmd) { bytes -> Log.d(TAG, "Server: ${String(bytes).trim()}") }
                }
                started = true
                break
            } catch (e: Exception) {
                lastError = e
            }
        }
        if (!started) {
            throw AdbPairException(
                if (candidates.isEmpty()) "ADB connect port not found. Make sure Wireless debugging is enabled."
                else lastError?.message ?: "ADB connection failed"
            )
        }
        val deadline = System.currentTimeMillis() + BINDER_TIMEOUT_MS
        while (Priv.service == null) {
            if (System.currentTimeMillis() > deadline) throw AdbPairException("The helper did not start.")
            Thread.sleep(200)
        }
        Log.d(TAG, "Server started successfully")
    }

    private const val BINDER_TIMEOUT_MS = 10_000L
    private const val LOCALHOST = "127.0.0.1"

    /** The wireless debugging port announced over mDNS, or -1. */
    private fun findConnectPortByMdns(context: Context): Int {
        var foundPort = -1
        val latch = CountDownLatch(1)
        val mdns = AdbMdns(context, AdbMdns.TLS_CONNECT) { _, port ->
            if (port != -1) {
                foundPort = port
                latch.countDown()
            }
        }
        mdns.start()
        latch.await(MDNS_WAIT_MS, TimeUnit.MILLISECONDS)
        mdns.stop()
        return foundPort
    }

    /** Ports accepting connections on this phone in the range adbd picks from. */
    private fun localOpenPorts(): List<Int> {
        val open = java.util.concurrent.ConcurrentLinkedQueue<Int>()
        val pool = java.util.concurrent.Executors.newFixedThreadPool(32)
        for (start in SCAN_FROM..SCAN_TO step 1000) {
            pool.execute {
                for (p in start until minOf(start + 1000, SCAN_TO + 1)) {
                    try {
                        java.net.Socket().use { it.connect(java.net.InetSocketAddress(LOCALHOST, p), 50) }
                        open += p
                    } catch (_: Exception) {
                    }
                }
            }
        }
        pool.shutdown()
        pool.awaitTermination(60, TimeUnit.SECONDS)
        return open.sorted()
    }

    private const val MDNS_WAIT_MS = 8_000L
    private const val SCAN_FROM = 30000
    private const val SCAN_TO = 65535
}

class AdbPairException(message: String) : Exception(message)
