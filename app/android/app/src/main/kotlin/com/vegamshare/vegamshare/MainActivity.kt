package com.vegamshare.vegamshare

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

class MainActivity : FlutterActivity() {
    private var multicastLock: WifiManager.MulticastLock? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var resumed = false
    private var transferring = false
    private var channel: MethodChannel? = null
    private var wifiLostForwarder: ((String) -> Unit)? = null
    private var dartReady = false
    private val pendingShared = mutableListOf<String>()
    private val main = Handler(Looper.getMainLooper())
    /** Opens files for a running app transfer, one binder call at a time. */
    private val opener = java.util.concurrent.Executors.newSingleThreadExecutor()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        // Android drops multicast/broadcast packets unless an app holds this
        // lock. Needed only to answer "where is ID x?" lookups, so it is held
        // while the app is open or a transfer runs, never while idle.
        multicastLock = wifi.createMulticastLock("vegamshare-discovery").apply { setReferenceCounted(false) }
        @Suppress("DEPRECATION")
        val wifiMode = if (Build.VERSION.SDK_INT >= 29) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
        wifiLock = wifi.createWifiLock(wifiMode, "vegamshare-transfer").apply { setReferenceCounted(false) }
        val power = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "vegamshare:transfer").apply { setReferenceCounted(false) }
        if (savedInstanceState == null) handleShare(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleShare(intent)
    }

    override fun onResume() {
        super.onResume()
        resumed = true
        syncLocks()
    }

    override fun onPause() {
        resumed = false
        syncLocks()
        super.onPause()
    }

    override fun onDestroy() {
        // WifiLink outlives activities; do not keep this one alive through it.
        if (WifiLink.onLost === wifiLostForwarder) WifiLink.onLost = null
        transferring = false
        resumed = false
        syncLocks()
        super.onDestroy()
    }

    private fun syncLocks() {
        multicastLock?.let { if (resumed || transferring) { if (!it.isHeld) it.acquire() } else if (it.isHeld) it.release() }
        wifiLock?.let { if (transferring) { if (!it.isHeld) it.acquire() } else if (it.isHeld) it.release() }
        // Safety cap: even if a transfer never reports completion, the CPU
        // lock cannot outlive two hours.
        wakeLock?.let { if (transferring) { if (!it.isHeld) it.acquire(2 * 60 * 60 * 1000L) } else if (it.isHeld) it.release() }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // Tell Dart when the hotspot stops or the joined network drops.
        val forward: (String) -> Unit = { what -> channel?.invokeMethod("wifiLost", what) }
        wifiLostForwarder = forward
        WifiLink.onLost = forward
        channel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "vegamshare/native").apply {
            setMethodCallHandler { call, result ->
                when (call.method) {
                    "deviceName" -> {
                        val maker = Build.MANUFACTURER.replaceFirstChar { it.uppercase() }
                        val model = Build.MODEL
                        result.success(if (model.startsWith(maker, ignoreCase = true)) model else "$maker $model")
                    }
                    "openPath" -> result.success(openFolder(call.arguments as String))
                    "hotspotSupported" -> result.success(WifiLink.hotspotSupported)
                    "sdkInt" -> result.success(Build.VERSION.SDK_INT)
                    "startHotspot" -> WifiLink.startHotspot(this@MainActivity) { r ->
                        r.fold(
                            { h -> result.success(mapOf("ssid" to h.ssid, "passphrase" to h.passphrase, "wpa3" to h.wpa3)) },
                            { e -> result.error(e.message ?: "failed", e.message, null) },
                        )
                    }
                    "stopHotspot" -> {
                        WifiLink.stopHotspot()
                        result.success(null)
                    }
                    "joinWifi" -> {
                        val ssid = call.argument<String>("ssid")
                        val pass = call.argument<String>("passphrase")
                        if (ssid.isNullOrEmpty() || pass.isNullOrEmpty()) {
                            result.error("bad_args", "ssid and passphrase are required", null)
                        } else {
                            WifiLink.join(this@MainActivity, ssid, pass, call.argument<Boolean>("wpa3") == true) { r ->
                                r.fold({ gw -> result.success(mapOf("gateway" to gw)) }, { e -> result.error(e.message ?: "join_failed", e.message, null) })
                            }
                        }
                    }
                    "leaveWifi" -> {
                        WifiLink.leave(this@MainActivity)
                        result.success(null)
                    }
                    "wifiState" -> {
                        val h = WifiLink.currentHotspot()
                        result.success(
                            mapOf(
                                "hotspotSsid" to h?.ssid,
                                "hotspotPassphrase" to h?.passphrase,
                                "hotspotWpa3" to h?.wpa3,
                                "joined" to WifiLink.joined,
                            ),
                        )
                    }
                    "holdLocks" -> {
                        transferring = call.arguments as Boolean
                        syncLocks()
                        result.success(null)
                    }
                    "takeSharedPaths" -> {
                        dartReady = true
                        result.success(pendingShared.toList())
                        pendingShared.clear()
                    }
                    // ---- Privileged helper (Wireless debugging) --------------------------
                    "shizukuAvailable" -> result.success(EmbeddedServer.isAvailable())
                    "shizukuRunning" -> result.success(Priv.state() == Priv.State.READY)
                    "openClone" -> {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                            startActivity(Intent(this@MainActivity, CloneActivity::class.java))
                            result.success(null)
                        } else {
                            result.error("unsupported", "Requires Android 11+", null)
                        }
                    }
                    "openShizukuSetup" -> {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                            startActivity(Intent(this@MainActivity, EmbeddedSetupActivity::class.java))
                            result.success(null)
                        } else {
                            result.error("unsupported", "Requires Android 11+", null)
                        }
                    }
                    // ---- Send app: one folder, data streamed through the helper --------
                    "prepareAppBundle" -> {
                        val pkg = call.argument<String>("packageName")
                        if (pkg.isNullOrEmpty()) {
                            result.error("bad_args", "packageName required", null)
                        } else {
                            Thread {
                                try {
                                    val res = AppSendManager.prepareBundle(this@MainActivity, pkg)
                                    main.post { result.success(res) }
                                } catch (e: Exception) {
                                    main.post { result.error("stage_failed", e.message, null) }
                                }
                            }.start()
                        }
                    }
                    // "<pkg>/<data|obb>/<relative path>" -> a file descriptor the Rust core then owns.
                    "openAppFile" -> {
                        val spec = call.arguments as? String
                        opener.execute {
                            val fd = try {
                                val parts = spec!!.split('/', limit = 3)
                                val helper = Priv.service!!
                                val pfd = if (parts[2] == AppSendManager.ARCHIVE) helper.openAppArchive(parts[0], parts[1])
                                else helper.openAppFile(parts[0], parts[1], parts[2])
                                pfd.detachFd()
                            } catch (e: Exception) {
                                android.util.Log.w("VegamShare", "openAppFile $spec: ${e.message}")
                                -1
                            }
                            main.post { result.success(fd) }
                        }
                    }
                    // ---- Install received app (bundle folder: apk/, data/, obb/) ---------
                    "writeReceivedApp" -> {
                        val pkg = call.argument<String>("packageName")
                        val receivedDir = call.argument<String>("receivedDir")
                        if (pkg.isNullOrEmpty() || receivedDir.isNullOrEmpty()) {
                            result.error("bad_args", "packageName and receivedDir required", null)
                        } else {
                            Thread {
                                try {
                                    val summary = AppReceiveManager.restore(this@MainActivity, pkg, java.io.File(receivedDir))
                                    android.util.Log.i("AppReceiveManager", "restore $pkg: $summary")
                                    main.post { result.success(summary) }
                                } catch (e: Exception) {
                                    android.util.Log.w("AppReceiveManager", "restore $pkg failed", e)
                                    main.post { result.error("install_failed", e.message, null) }
                                }
                            }.start()
                        }
                    }
                    "listUserApps" -> Thread {
                        val apps = AppSendManager.listUserApps(this@MainActivity)
                        val list = apps.map { app ->
                            mapOf(
                                "packageName" to app.packageName,
                                "label" to app.label,
                                "apkPath" to app.apkPath,
                                "splitApkPaths" to app.splitApkPaths,
                            )
                        }
                        main.post { result.success(list) }
                    }.start()
                    else -> result.notImplemented()
                }
            }
        }
    }

    // ---- "Share to VegamShare" ------------------------------------------

    @Suppress("DEPRECATION")
    private fun sharedUris(intent: Intent?): List<Uri> = when (intent?.action) {
        Intent.ACTION_SEND -> listOfNotNull(
            if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
            else intent.getParcelableExtra(Intent.EXTRA_STREAM)
        )
        Intent.ACTION_SEND_MULTIPLE -> (
            if (Build.VERSION.SDK_INT >= 33) intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
            else intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM)
        ) ?: emptyList()
        else -> emptyList()
    }

    private fun handleShare(intent: Intent?) {
        val uris = sharedUris(intent)
        if (uris.isEmpty()) return
        // Resolving may copy data, so keep it off the UI thread.
        Thread {
            val paths = uris.mapNotNull { runCatching { resolve(it) }.getOrNull() }
            main.post { deliver(paths) }
        }.start()
    }

    private fun deliver(paths: List<String>) {
        if (paths.isEmpty()) return
        val ch = channel
        if (dartReady && ch != null) ch.invokeMethod("sharedPaths", paths) else pendingShared.addAll(paths)
    }

    /** Real file path when the app can read it directly, else a cache copy. */
    @Suppress("DEPRECATION")
    private fun resolve(uri: Uri): String? {
        if (uri.scheme == "file") return uri.path
        try {
            contentResolver.query(uri, arrayOf(MediaStore.MediaColumns.DATA), null, null, null)?.use { c ->
                val i = c.getColumnIndex(MediaStore.MediaColumns.DATA)
                if (i >= 0 && c.moveToFirst()) {
                    val p = c.getString(i)
                    if (p != null && File(p).canRead()) return p
                }
            }
        } catch (_: Exception) {
        }
        var name = "shared"
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            val i = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (i >= 0 && c.moveToFirst()) c.getString(i)?.let { name = it }
        }
        val dir = File(cacheDir, "shared/${System.nanoTime()}").apply { mkdirs() }
        val out = File(dir, name.replace('/', '_'))
        val input = contentResolver.openInputStream(uri) ?: return null
        input.use { inp -> out.outputStream().use { inp.copyTo(it) } }
        return out.absolutePath
    }

    // ---- open received folder -------------------------------------------

    /** Opens a folder on shared storage in the system Files app. */
    private fun openFolder(path: String): Boolean {
        val root = Environment.getExternalStorageDirectory().absolutePath
        if (!path.startsWith(root)) return false
        val rel = path.removePrefix(root).trimStart('/')
        val uri: Uri = DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", "primary:$rel")
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, DocumentsContract.Document.MIME_TYPE_DIR)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        return try {
            startActivity(intent)
            true
        } catch (e: ActivityNotFoundException) {
            false
        }
    }
}
