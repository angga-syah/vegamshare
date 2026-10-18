package com.vegamshare.vegamshare

import android.content.AttributionSource
import android.os.Binder
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.os.Looper
import android.util.Log
import java.io.File
import kotlin.system.exitProcess

/**
 * Entry point of the privileged helper. Started over ADB with
 * `app_process` from this app's own APK, so it runs as the shell user.
 * It hands a [PrivilegedService] binder to the app through [PrivProvider]
 * whenever the app process (re)starts, and exits when the app is removed.
 */
object PrivServer {
    private const val TAG = "VegamSharePriv"
    private const val POLL_MS = 2000L

    @JvmStatic
    fun main(args: Array<String>) {
        val appUid = args.getOrNull(0)?.toIntOrNull() ?: exitProcess(2)
        val apk = System.getProperty("java.class.path") ?: exitProcess(2)
        Looper.prepareMainLooper()
        val service = PrivilegedService(appUid)
        Thread({ deliverLoop(service, apk) }, "deliver").start()
        Log.i(TAG, "started, uid=${android.os.Process.myUid()}, app uid=$appUid")
        Looper.loop()
    }

    private fun deliverLoop(service: IBinder, apk: String) {
        var deliveredPid = -1
        while (true) {
            if (!File(apk).exists()) {
                // Updated or removed: the running code is stale either way.
                restartFromNewApk()
                exitProcess(0)
            }
            val pid = appPid()
            if (pid > 0 && pid != deliveredPid) {
                if (send(service)) deliveredPid = pid
            }
            Thread.sleep(POLL_MS)
        }
    }

    private fun restartFromNewApk() {
        val pkg = BuildConfig.APPLICATION_ID
        val path = shellLines("pm", "path", pkg)
            .firstOrNull { it.endsWith("base.apk") }?.removePrefix("package:") ?: return
        val uid = shellLines("pm", "list", "packages", "-U", pkg)
            .firstOrNull { it.startsWith("package:$pkg ") }
            ?.substringAfter("uid:")?.trim()?.toIntOrNull() ?: return
        Log.i(TAG, "app updated, restarting from $path")
        ProcessBuilder("sh", "-c", startCommand(path, uid)).start().waitFor()
    }

    private fun shellLines(vararg argv: String): List<String> = runCatching {
        ProcessBuilder(*argv).redirectErrorStream(true).start().inputStream.bufferedReader().readLines()
    }.getOrDefault(emptyList())

    /**
     * Shell command that (re)starts the helper in the background. ADB shell
     * sessions may use a terminal whose hangup kills background jobs, hence
     * the ignored SIGHUP and the pause before the session ends.
     */
    fun startCommand(apk: String, appUid: Int): String =
        "trap '' HUP; for p in \$(pidof $PROCESS_NAME); do kill \$p; done; " +
            "CLASSPATH='$apk' setsid app_process /system/bin --nice-name=$PROCESS_NAME " +
            "${PrivServer::class.java.name} $appUid </dev/null >/dev/null 2>&1 & sleep 1"

    const val PROCESS_NAME = "vegamshare_priv"

    private fun appPid(): Int {
        val target = BuildConfig.APPLICATION_ID
        File("/proc").listFiles()?.forEach { d ->
            val pid = d.name.toIntOrNull() ?: return@forEach
            val cmd = runCatching { File(d, "cmdline").readText() }.getOrNull() ?: return@forEach
            if (cmd.substringBefore('\u0000') == target) return pid
        }
        return -1
    }

    /** Calls PrivProvider in the app, the same way `content call` does. */
    private fun send(service: IBinder): Boolean {
        val authority = PrivProvider.AUTHORITY
        val token = Binder()
        return try {
            val am = Class.forName("android.app.ActivityManager").getMethod("getService").invoke(null)!!
            val get = am.javaClass.getMethod(
                "getContentProviderExternal",
                String::class.java, Int::class.javaPrimitiveType, IBinder::class.java, String::class.java,
            )
            val holder = get.invoke(am, authority, 0, token, authority) ?: return false
            try {
                val provider = holder.javaClass.getField("provider").get(holder)!!
                val extras = Bundle().apply { putBinder(PrivProvider.KEY_BINDER, service) }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    val call = provider.javaClass.methods.first {
                        it.name == "call" && it.parameterTypes.size == 5 && it.parameterTypes[0] == AttributionSource::class.java
                    }
                    val source = AttributionSource.Builder(android.os.Process.myUid()).setPackageName("com.android.shell").build()
                    call.invoke(provider, source, authority, PrivProvider.METHOD_BINDER, null, extras)
                } else {
                    val call = provider.javaClass.methods.first { it.name == "call" && it.parameterTypes.size == 6 }
                    call.invoke(provider, "com.android.shell", null, authority, PrivProvider.METHOD_BINDER, null, extras)
                }
                Log.i(TAG, "binder sent")
                true
            } finally {
                runCatching {
                    am.javaClass.methods.first { it.name == "removeContentProviderExternalAsUser" }
                        .invoke(am, authority, token, 0)
                }.recoverCatching {
                    am.javaClass.getMethod("removeContentProviderExternal", String::class.java, IBinder::class.java)
                        .invoke(am, authority, token)
                }
            }
        } catch (e: Throwable) {
            Log.w(TAG, "send failed", e)
            false
        }
    }
}
