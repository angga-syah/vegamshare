package com.vegamshare.vegamshare

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import java.io.File
import java.io.FileInputStream

/**
 * Receiver side of "send app": the bundle folder holds apk/, data/, obb/.
 * With the helper the app is installed first and only then its data moved in,
 * so Android hands the new Android/data folder to the installed app.
 */
object AppReceiveManager {

    private const val TAG = "AppReceiveManager"
    const val ACTION_INSTALL = "com.vegamshare.vegamshare.INSTALL_COMPLETE"

    /** Returns "installed|<data files>|<obb files>|<errors>" or "confirm" (user confirms the install). */
    fun restore(context: Context, pkg: String, bundle: File): String {
        val apks = File(bundle, "apk").listFiles { f -> f.isFile && f.name.endsWith(".apk") }
            ?.sortedBy { if (it.name == "base.apk") 0 else 1 }.orEmpty()
        require(apks.isNotEmpty()) { "no APK in ${bundle.name}" }

        if (Priv.service == null) {
            installWithInstaller(context, apks)
            return "confirm"
        }
        installWithShell(apks)

        var errors = 0
        val appUid = context.packageManager.getApplicationInfo(pkg, 0).uid
        val data = File(bundle, "data")
        var dataMoved = 0
        if (data.isDirectory) {
            prepareDataFolder(pkg, appUid)
            dataMoved = moveAll(data, pkg, "data").also { errors += it.second }.first
        }
        val obbMoved = moveAll(File(bundle, "obb"), pkg, "obb").also { errors += it.second }.first
        if (errors == 0) bundle.deleteRecursively()
        return "installed|$dataMoved|$obbMoved|$errors"
    }

    /** Silent install as the shell user; throws with the package manager's message on failure. */
    private fun installWithShell(apks: List<File>) {
        val total = apks.sumOf { it.length() }
        val create = Priv.sh("cmd", "package", "install-create", "-r", "-S", total.toString())
        val id = Regex("\\[(\\d+)]").find(create.out)?.groupValues?.get(1) ?: error(create.out.trim())
        apks.forEachIndexed { i, f ->
            // The system server cannot open files on shared storage itself, so the shell streams them in.
            val w = Priv.sh(
                "sh", "-c", "cat \"\$1\" | cmd package install-write -S \"\$2\" \"\$3\" \"\$4\" -",
                "sh", f.path, f.length().toString(), id, "${i}_${f.name}",
            )
            if (!w.ok || !w.out.contains("Success")) {
                Priv.sh("cmd", "package", "install-abandon", id)
                error(w.out.trim())
            }
        }
        val commit = Priv.sh("cmd", "package", "install-commit", id)
        if (!commit.out.contains("Success")) error(commit.out.trim())
    }

    /**
     * A fresh install has no Android/data/<pkg> yet, and on some phones a folder made by the
     * shell is not handed to the app. Starting the app once lets it create its own folder;
     * apps without a launcher get a folder made by the helper and opened to them instead.
     */
    private fun prepareDataFolder(pkg: String, appUid: Int) {
        val helper = Priv.service ?: return
        if (helper.prepareTarget(pkg, 0, appUid % 100000) != 2) return
        val start = Priv.sh("monkey", "-p", pkg, "-c", "android.intent.category.LAUNCHER", "1")
        if (start.out.contains("No activities")) return
        val dir = "/storage/emulated/0/Android/data/$pkg"
        repeat(20) {
            if (Priv.sh("stat", "-c", "%u", dir).out.trim() == appUid.toString()) {
                Priv.sh("am", "force-stop", pkg)
                return
            }
            Thread.sleep(500)
        }
        Priv.sh("am", "force-stop", pkg)
    }

    /** Returns moved files and errors. */
    private fun moveAll(dir: File, pkg: String, kind: String): Pair<Int, Int> {
        if (!dir.isDirectory) return 0 to 0
        val helper = Priv.service ?: return 0 to 1
        var moved = 0
        var errors = 0
        dir.walkTopDown().filter { it.isFile }.forEach { f ->
            val err = helper.moveIntoApp(f.path, pkg, kind, f.relativeTo(dir).invariantSeparatorsPath)
            if (err.isEmpty()) moved++ else {
                errors++
                Log.w(TAG, "move ${f.name}: $err")
            }
        }
        return moved to errors
    }

    /** Without the helper: the system installer asks the user to confirm. */
    private fun installWithInstaller(context: Context, apks: List<File>) {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setInstallReason(PackageManager.INSTALL_REASON_USER)
        }
        val sessionId = installer.createSession(params)
        installer.openSession(sessionId).use { session ->
            for (apk in apks) {
                session.openWrite(apk.name, 0, apk.length()).use { out ->
                    FileInputStream(apk).use { it.copyTo(out) }
                    session.fsync(out)
                }
            }
            val intent = Intent(context, InstallResultReceiver::class.java).setAction(ACTION_INSTALL)
            val pending = PendingIntent.getBroadcast(context, sessionId, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE)
            session.commit(pending.intentSender)
        }
    }
}

/** Shows the system install confirmation when the installer asks for it. */
class InstallResultReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        if (status != PackageInstaller.STATUS_PENDING_USER_ACTION) {
            Log.i("AppReceiveManager", "install status $status: ${intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)}")
            return
        }
        @Suppress("DEPRECATION")
        val confirm = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
        else intent.getParcelableExtra(Intent.EXTRA_INTENT)
        confirm?.let { context.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }
}