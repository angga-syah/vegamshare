package com.vegamshare.vegamshare

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
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
import android.provider.Settings
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

    /** Text of the ongoing "hotspot on" notification, localized by Dart. */
    var notifyTitle: String = "VegamShare hotspot: {name}"
    var notifyText: String = ""

    fun wifiEnabled(context: Context): Boolean =
        (context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager).isWifiEnabled

    /**
     * Turns Wi-Fi on where Android still lets an app do it: directly up to
     * Android 9, through the privileged helper when it runs. False means the
     * user has to switch it on (Dart then opens the Wi-Fi panel).
     */
    @Suppress("DEPRECATION")
    fun enableWifi(context: Context): Boolean {
        val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        if (wifi.isWifiEnabled) return true
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q && wifi.setWifiEnabled(true)) return true
        if (Priv.service == null) return false
        return try {
            Priv.exec("svc", "wifi", "enable").ok
        } catch (_: Exception) {
            false
        }
    }

    /** The quick Wi-Fi switch above the app (Android 10+), else Wi-Fi settings. */
    fun openWifiPanel(context: Context) {
        val action = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) Settings.Panel.ACTION_WIFI else Settings.ACTION_WIFI_SETTINGS
        open(context, Intent(action))
    }

    /**
     * The normal (tethering) hotspot is on. Android cannot run it next to
     * our local-only hotspot, and apps are not allowed to switch it off.
     */
    fun tetheringOn(context: Context): Boolean {
        val sticky = try {
            context.applicationContext.registerReceiver(null, IntentFilter("android.net.wifi.WIFI_AP_STATE_CHANGED"))
        } catch (_: Exception) {
            null
        } ?: return false
        // WifiManager.WIFI_AP_STATE_ENABLING = 12, WIFI_AP_STATE_ENABLED = 13
        return sticky.getIntExtra("wifi_state", 0) in 12..13
    }

    fun openTetherSettings(context: Context) {
        val tether = Intent().setClassName("com.android.settings", "com.android.settings.TetherSettings")
        if (!open(context, tether)) open(context, Intent(Settings.ACTION_WIRELESS_SETTINGS))
    }

    fun openLocationSettings(context: Context) {
        open(context, Intent(Settings.ACTION_LOCATION_SOURCE_SETTINGS))
    }

    private fun open(context: Context, intent: Intent): Boolean = try {
        context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        true
    } catch (_: Exception) {
        false
    }

    /**
     * A local-only hotspot does not show up as "Hotspot" in quick settings,
     * so an ongoing notification tells the user it is on.
     */
    private fun showNotification(context: Context, h: Hotspot) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Hotspot", NotificationManager.IMPORTANCE_LOW))
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
        val tap = launch?.let { PendingIntent.getActivity(context, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }
        val n = Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(notifyTitle.replace("{name}", h.ssid))
            .setContentText(notifyText)
            .setOngoing(true)
            .setContentIntent(tap)
            .build()
        try {
            nm.notify(NOTIFY_ID, n)
        } catch (_: SecurityException) {
            // Notifications not allowed: the banner in the app still shows it.
        }
    }

    private fun hideNotification(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        context.getSystemService(NotificationManager::class.java).cancel(NOTIFY_ID)
    }

    private const val CHANNEL = "hotspot"
    private const val NOTIFY_ID = 4710
    /** WifiManager.LocalOnlyHotspotCallback.ERROR_INCOMPATIBLE_MODE */
    private const val ERROR_INCOMPATIBLE_MODE = 3

    @SuppressLint("MissingPermission") // runtime permission is requested from Dart first
    fun startHotspot(context: Context, done: (Result<Hotspot>) -> Unit) {
        hotspot?.let { done(Result.success(it)); return }
        if (!hotspotSupported) {
            done(Result.failure(IllegalStateException("unsupported")))
            return
        }
        val app = context.applicationContext
        if (starting == null && tetheringOn(app)) {
            done(Result.failure(IllegalStateException("tethering_on")))
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
                    showNotification(app, info)
                    reply(Result.success(info))
                }

                override fun onStopped() {
                    reservation = null
                    hotspot = null
                    hideNotification(app)
                    onLost?.invoke("hotspot")
                }

                override fun onFailed(reason: Int) {
                    val code = if (reason == ERROR_INCOMPATIBLE_MODE || tetheringOn(app)) "tethering_on" else "failed_$reason"
                    reply(Result.failure(IllegalStateException(code)))
                }
            }, main)
        } catch (e: SecurityException) {
            reply(Result.failure(SecurityException("permission")))
        } catch (e: IllegalStateException) {
            // Location is checked in Dart beforehand, so this means another
            // hotspot/tethering session or a pending request is in the way.
            reply(Result.failure(IllegalStateException(if (tetheringOn(app)) "tethering_on" else "busy")))
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

    fun stopHotspot(context: Context) {
        hideNotification(context.applicationContext)
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
        // One phone cannot run our hotspot and join another one.
        if (hotspot != null) stopHotspot(context)
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
