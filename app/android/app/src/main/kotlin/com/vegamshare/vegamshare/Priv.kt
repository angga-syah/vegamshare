package com.vegamshare.vegamshare

import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.Process
import android.os.SystemClock

/** Connection to [PrivilegedService], delivered by [PrivServer]. */
object Priv {
    const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"

    enum class State { NOT_RUNNING, CONNECTING, READY }

    /** A running helper delivers its binder within a couple of seconds of process start. */
    private const val WAIT_MS = 4000L
    private val main = Handler(Looper.getMainLooper())

    @Volatile
    var service: IPrivileged? = null
        private set

    /** Called on the main thread whenever [state] may have changed. */
    var onChange: (() -> Unit)? = null

    fun attach(binder: IBinder) {
        val s = IPrivileged.Stub.asInterface(binder)
        try {
            binder.linkToDeath({
                if (service === s) service = null
                main.post { onChange?.invoke() }
            }, 0)
        } catch (_: Exception) {
            return
        }
        service = s
        main.post { onChange?.invoke() }
    }

    fun state(): State {
        if (service?.asBinder()?.pingBinder() == true) return State.READY
        service = null
        val waited = SystemClock.elapsedRealtime() - Process.getStartElapsedRealtime()
        if (waited < WAIT_MS) {
            main.postDelayed({ onChange?.invoke() }, WAIT_MS - waited)
            return State.CONNECTING
        }
        return State.NOT_RUNNING
    }

    class ShellResult(val code: Int, val out: String) {
        val ok get() = code == 0
    }

    private fun connected() = service ?: throw IllegalStateException("privileged helper not running")

    /** Runs a command in the privileged process. Returns exit code + stdout. */
    fun exec(vararg argv: String): ShellResult {
        val r = connected().exec(arrayOf(*argv))
        val nl = r.indexOf('\n')
        return ShellResult(r.substring(0, nl).toIntOrNull() ?: -1, r.substring(nl + 1))
    }

    fun sh(vararg argv: String): ShellResult = exec(*argv)

    /** Lists files under path. Returns list of absolute paths. */
    fun listFiles(path: String): List<String> = connected().listFiles(path).lines().filter { it.isNotBlank() }

    /** Copies a file in the privileged context. Returns empty string on success. */
    fun copyFile(src: String, dst: String): String = connected().copyFile(src, dst)

    /** Creates directories in the privileged context. */
    fun mkdirs(path: String): String = connected().mkdirs(path)

    /** Package names visible in a user. */
    fun packagesOf(user: Int): Set<String> =
        sh("pm", "list", "packages", "--user", user.toString()).out.lineSequence()
            .map { it.trim().removePrefix("package:") }
            .filter { it.isNotEmpty() }
            .toSet()

    /** Folder names in /storage/emulated/<user>/Android/<kind>. */
    fun appFolders(user: Int, kind: String): Set<String> =
        sh("ls", "/storage/emulated/$user/Android/$kind").out.lineSequence()
            .map { it.trim() }
            .filter { it.isNotEmpty() && !it.contains(' ') && !it.contains(':') }
            .toSet()
}
