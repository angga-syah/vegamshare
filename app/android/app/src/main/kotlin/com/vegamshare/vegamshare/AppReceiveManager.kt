package com.vegamshare.vegamshare

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.util.Log
import java.io.File
import java.io.FileInputStream

/**
 * Receiver side of "send app" feature.
 *
 * Installs a received APK via PackageInstaller API and writes received
 * Android/data files via the Shizuku privileged service.
 */
object AppReceiveManager {

    private const val TAG = "AppReceiveManager"

    /**
     * Install one or more APK files (base + splits) using PackageInstaller.
     * Must be called from a background thread.
     */
    fun installApk(context: Context, apkFiles: List<File>): Boolean {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setInstallReason(PackageManager.INSTALL_REASON_USER)
        }

        val sessionId = installer.createSession(params)
        installer.openSession(sessionId).use { session ->
            for (apk in apkFiles) {
                session.openWrite(apk.name, 0, apk.length()).use { out ->
                    FileInputStream(apk).use { inp -> inp.copyTo(out) }
                    session.fsync(out)
                }
            }
            val intentSender = createInstallIntentSender(context, sessionId)
            session.commit(intentSender)
        }

        Log.d(TAG, "APK install session committed for ${apkFiles.map { it.name }}")
        return true
    }

    private fun createInstallIntentSender(context: Context, sessionId: Int): android.content.IntentSender {
        val intent = Intent("com.vegamshare.vegamshare.INSTALL_COMPLETE").apply {
            setPackage(context.packageName)
        }
        val pending = PendingIntent.getBroadcast(
            context,
            sessionId,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
        )
        return pending.intentSender
    }

    /**
     * Write received Android/data files using the Shizuku privileged service.
     * [dataFiles] maps relative path (relative to Android/data/<package>/) to local File.
     * Must be called from a background thread.
     */
    fun writeDataViaPriv(packageName: String, dataFiles: Map<String, File>): Int {
        if (Priv.state() != Priv.State.READY) {
            Log.w(TAG, "Shizuku not ready — cannot write Android/data")
            return 0
        }

        val baseDst = "/storage/emulated/0/Android/data/$packageName"
        Priv.mkdirs(baseDst)

        var written = 0
        for ((relPath, srcFile) in dataFiles) {
            val dstPath = "$baseDst/$relPath"
            val err = Priv.copyFile(srcFile.absolutePath, dstPath)
            if (err.isEmpty()) {
                written++
            } else {
                Log.e(TAG, "copyFile failed for $relPath: $err")
            }
        }

        Log.d(TAG, "Wrote $written/${dataFiles.size} files to Android/data/$packageName")
        return written
    }

    /**
     * Write received OBB files to Android/obb/<packageName>.
     * OBB dir is writable without Shizuku on most devices.
     */
    fun writeObbFiles(context: Context, packageName: String, obbFiles: Map<String, File>): Int {
        val extDir = context.externalCacheDirs.firstOrNull()?.parentFile?.parentFile
            ?: return 0
        val obbDir = File(extDir, "obb/$packageName")
        obbDir.mkdirs()

        var written = 0
        for ((relPath, srcFile) in obbFiles) {
            try {
                val dstFile = File(obbDir, relPath)
                dstFile.parentFile?.mkdirs()
                srcFile.copyTo(dstFile, overwrite = true)
                written++
            } catch (e: Exception) {
                Log.e(TAG, "Failed to write OBB $relPath", e)
            }
        }
        return written
    }
}
