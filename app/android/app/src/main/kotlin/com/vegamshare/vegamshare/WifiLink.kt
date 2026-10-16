package com.vegamshare.vegamshare

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.SoftApConfiguration
import android.net.wifi.WifiConfiguration
import android.net.wifi.WifiManager
import android.net.wifi.WifiNetworkSpecifier
import android.os.Build
import android.os.Handler
import android.os.Looper
import java.net.Inet4Address

/**
 * "No Wi-Fi" mode: one phone creates a local-only hotspot (no internet, no
 * mobile data used), the other joins it from the QR code.
 *
 * State lives here rather than in the activity so that the hotspot and the
 * joined network survive the activity being recreated (rotation, back).
 * Only call from the main thread.
 */
object WifiLink {
    data class Hotspot(val ssid: String, val passphrase: String, val wpa3: Boolean)

    private val main = Handler(Looper.getMainLooper())
    private var reservation: WifiManager.LocalOnlyHotspotReservation? = null
    private var hotspot: Hotspot? = null
    /** Callers waiting while a start request is in flight. */
    private var starting: MutableList<(Result<Hotspot>) -> Unit>? = null
    private var joinCallback: ConnectivityManager.NetworkCallback? = null
    /** Android 8-9: id of the Wi-Fi network we added, removed on leave. */
    private var legacyNetId: Int? = null

    /** Called (on the main thread) when the hotspot or joined network goes away. */
    var onLost: ((what: String) -> Unit)? = null

    val hotspotSupported get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
    val joined get() = joinCallback != null || legacyNetId != null

    fun currentHotspot(): Hotspot? = hotspot

    @SuppressLint("MissingPermission") // runtime permission is requested from Dart first
    fun startHotspot(context: Context, done: (Result<Hotspot>) -> Unit) {
        hotspot?.let { done(Result.success(it)); return }
        if (!hotspotSupported) {
            done(Result.failure(IllegalStateException("unsupported")))
            return
        }
        // A second caller while starting waits for the same result instead
        // of making Android reject a duplicate request.
        starting?.let { it.add(done); return }
        val waiters = mutableListOf(done)
        starting = waiters
        fun reply(r: Result<Hotspot>) {
            if (starting !== waiters) return
            starting = null
            waiters.forEach { it(r) }
        }
        val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        try {
            wifi.startLocalOnlyHotspot(object : WifiManager.LocalOnlyHotspotCallback() {
                override fun onStarted(res: WifiManager.LocalOnlyHotspotReservation) {
                    val info = describe(res)
                    if (info == null) {
                        res.close()
                        reply(Result.failure(IllegalStateException("no_config")))
                        return
                    }
                    reservation = res
                    hotspot = info
                    reply(Result.success(info))
                }

                override fun onStopped() {
                    reservation = null
                    hotspot = null
                    onLost?.invoke("hotspot")
                }

                override fun onFailed(reason: Int) {
                    reply(Result.failure(IllegalStateException("failed_$reason")))
                }
            }, main)
        } catch (e: SecurityException) {
            reply(Result.failure(SecurityException("permission")))
        } catch (e: IllegalStateException) {
            // Location is checked in Dart beforehand, so this means another
            // hotspot/tethering session or a pending request is in the way.
            reply(Result.failure(IllegalStateException("busy")))
        }
    }

    @Suppress("DEPRECATION")
    private fun describe(res: WifiManager.LocalOnlyHotspotReservation): Hotspot? {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val c: SoftApConfiguration = res.softApConfiguration
            val ssid = c.ssid ?: return null
            val pass = c.passphrase ?: return null
            val wpa3 = c.securityType == SoftApConfiguration.SECURITY_TYPE_WPA3_SAE
            return Hotspot(ssid.trim('"'), pass, wpa3)
        }
        val c: WifiConfiguration = res.wifiConfiguration ?: return null
        return Hotspot((c.SSID ?: return null).trim('"'), c.preSharedKey?.trim('"') ?: return null, false)
    }

    fun stopHotspot() {
        reservation?.close()
        reservation = null
        hotspot = null
    }

    /**
     * Joins the hotspot and makes it this process's default network, so
     * sockets created afterwards (the Rust core rebinds) use it even if
     * mobile data is on. Succeeds with the network's gateway, which on a
     * hotspot is the phone that created it.
     */
    @SuppressLint("MissingPermission")
    @Suppress("DEPRECATION")
    fun join(context: Context, ssid: String, passphrase: String, wpa3: Boolean, done: (Result<String?>) -> Unit) {
        leave(context)
        val app = context.applicationContext
        var replied = false
        fun reply(r: Result<String?>) {
            if (!replied) {
                replied = true
                main.post { done(r) }
            }
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val spec = WifiNetworkSpecifier.Builder().setSsid(ssid).apply {
                if (wpa3) setWpa3Passphrase(passphrase) else setWpa2Passphrase(passphrase)
            }.build()
            val request = NetworkRequest.Builder()
                .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                .setNetworkSpecifier(spec)
                .build()
            val cm = app.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            val cb = object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) {
                    cm.bindProcessToNetwork(network)
                    reply(Result.success(gateway(cm, network)))
                }

                override fun onUnavailable() {
                    release(cm, this)
                    reply(Result.failure(IllegalStateException("join_failed")))
                }

                override fun onLost(network: Network) {
                    cm.bindProcessToNetwork(null)
                    release(cm, this)
                    main.post { onLost?.invoke("wifi") }
                }
            }
            joinCallback = cb
            try {
                cm.requestNetwork(request, cb, 60_000)
            } catch (e: SecurityException) {
                joinCallback = null
                reply(Result.failure(SecurityException("permission")))
            } catch (e: RuntimeException) {
                joinCallback = null
                reply(Result.failure(IllegalStateException("join_failed")))
            }
        } else {
            // Android 8-9: add the network the classic way; it becomes the Wi-Fi.
            val wifi = app.getSystemService(Context.WIFI_SERVICE) as WifiManager
            val conf = WifiConfiguration().apply {
                SSID = "\"$ssid\""
                preSharedKey = "\"$passphrase\""
            }
            val id = wifi.addNetwork(conf)
            if (id == -1 || !wifi.enableNetwork(id, true)) {
                if (id != -1) wifi.removeNetwork(id)
                reply(Result.failure(IllegalStateException("join_failed")))
            } else {
                legacyNetId = id
                // Give the radio a moment to associate and get an address.
                main.postDelayed({ reply(Result.success(legacyGateway(wifi))) }, 5_000)
            }
        }
    }

    private fun release(cm: ConnectivityManager, cb: ConnectivityManager.NetworkCallback) {
        if (joinCallback === cb) joinCallback = null
        try {
            cm.unregisterNetworkCallback(cb)
        } catch (_: IllegalArgumentException) {
        }
    }

    private fun gateway(cm: ConnectivityManager, network: Network): String? {
        val routes = cm.getLinkProperties(network)?.routes ?: return null
        return routes.firstOrNull { it.isDefaultRoute && it.gateway is Inet4Address }?.gateway?.hostAddress
    }

    @Suppress("DEPRECATION")
    private fun legacyGateway(wifi: WifiManager): String? {
        val g = wifi.dhcpInfo?.gateway ?: return null
        if (g == 0) return null
        return "${g and 0xff}.${g shr 8 and 0xff}.${g shr 16 and 0xff}.${g shr 24 and 0xff}"
    }

    @Suppress("DEPRECATION")
    fun leave(context: Context) {
        val app = context.applicationContext
        joinCallback?.let { cb ->
            val cm = app.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            cm.bindProcessToNetwork(null)
            release(cm, cb)
        }
        legacyNetId?.let { id ->
            val wifi = app.getSystemService(Context.WIFI_SERVICE) as WifiManager
            wifi.disableNetwork(id)
            wifi.removeNetwork(id)
            legacyNetId = null
        }
    }
}
