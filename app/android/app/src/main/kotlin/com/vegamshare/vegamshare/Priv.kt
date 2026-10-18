package com.vegamshare.vegamshare

import android.content.ComponentName
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.os.IBinder
import rikka.shizuku.Shizuku

/** Connection to [PrivilegedService] through Shizuku. Call from the main thread. */
object Priv {
    enum class State { NOT_RUNNING, TOO_OLD, NO_PERMISSION, CONNECTING, READY }

    @Volatile
    var service: IPrivileged? = null
        private set

    var onChange: (() -> Unit)? = null

    private var binding = false

    private val args = Shizuku.UserServiceArgs(
        ComponentName(BuildConfig.APPLICATION_ID, PrivilegedService::class.java.name)
    )
        .daemon(false)
        .processNameSuffix("priv")
        .debuggable(BuildConfig.DEBUG)
        .version(BuildConfig.VERSION_CODE)

    private val connection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            binding = false
            if (binder != null && binder.pingBinder()) service = IPrivileged.Stub.asInterface(binder)
            onChange?.invoke()
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            binding = false
            service = null
            onChange?.invoke()
        }
    }

    fun state(): State {
        if (!Shizuku.pingBinder()) return State.NOT_RUNNING
        if (Shizuku.isPreV11()) return State.TOO_OLD
        if (Shizuku.checkSelfPermission() != PackageManager.PERMISSION_GRANTED) return State.NO_PERMISSION
        if (service?.asBinder()?.pingBinder() != true) {
            service = null
            if (!binding) {
                binding = true
                Shizuku.bindUserService(args, connection)
            }
            return State.CONNECTING
        }
        return State.READY
    }

    fun dropped() {
        service = null
        binding = false
    }

    class ShellResult(val code: Int, val out: String) {
        val ok get() = code == 0
    }

    /** Runs a command in the privileged process. Returns exit code + stdout. */
    fun exec(vararg argv: String): ShellResult {
        val s = service ?: throw IllegalStateException("Shizuku service not connected")
        val r = s.exec(arrayOf(*argv))
        val nl = r.indexOf('\n')
        return ShellResult(r.substring(0, nl).toIntOrNull() ?: -1, r.substring(nl + 1))
    }

    /** Lists files under path. Returns list of absolute paths. */
    fun listFiles(path: String): List<String> {
        val s = service ?: throw IllegalStateException("Shizuku service not connected")
        return s.listFiles(path).lines().filter { it.isNotBlank() }
    }

    /** Copies a file in the privileged context. Returns empty string on success. */
    fun copyFile(src: String, dst: String): String {
        val s = service ?: throw IllegalStateException("Shizuku service not connected")
        return s.copyFile(src, dst)
    }

    /** Creates directories in the privileged context. */
    fun mkdirs(path: String): String {
        val s = service ?: throw IllegalStateException("Shizuku service not connected")
        return s.mkdirs(path)
    }
}
