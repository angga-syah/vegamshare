package com.vegamshare.vegamshare

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import java.io.File

/**
 * Sender side of "send app" feature.
 *
 * Lists installed user apps and collects the paths that need to be transferred:
 *   - base APK (and any split APKs)
 *   - OBB directory under Android/obb/<package>
 *   - data directory under Android/data/<package> (read via Shizuku)
 */
object AppSendManager {

    private const val TAG = "AppSendManager"

    data class InstalledApp(
        val packageName: String,
        val label: String,
        val apkPath: String,
        val splitApkPaths: List<String>,
    )

    data class AppTransferPaths(
        val packageName: String,
        val apkPaths: List<String>,
        /** Absolute paths to OBB files; empty if none. */
        val obbFiles: List<String>,
        /** Absolute paths to Android/data files; empty if none. */
        val dataFiles: List<String>,
    )

    /** Returns all user-installed (non-system) apps. */
    fun listUserApps(context: Context): List<InstalledApp> {
        val pm = context.packageManager
        @Suppress("DEPRECATION")
        val packages = pm.getInstalledPackages(PackageManager.GET_META_DATA)
        return packages
            .filter { pi -> pi.applicationInfo?.flags?.and(ApplicationInfo.FLAG_SYSTEM) == 0 }
            .map { pi ->
                val ai = pi.applicationInfo!!
                InstalledApp(
                    packageName = pi.packageName,
                    label = pm.getApplicationLabel(ai).toString(),
                    apkPath = ai.publicSourceDir ?: ai.sourceDir,
                    splitApkPaths = ai.splitPublicSourceDirs?.toList() ?: emptyList(),
                )
            }
            .sortedBy { it.label.lowercase() }
    }

    /**
     * Collects all paths for a package.
     * OBB files are read via normal file access; Android/data files via Shizuku.
     * Must be called from a background thread.
     */
    fun collectPaths(context: Context, packageName: String): AppTransferPaths {
        val pm = context.packageManager
        @Suppress("DEPRECATION")
        val pi = pm.getPackageInfo(packageName, 0)
        val ai = pi.applicationInfo!!

        val apkPaths = mutableListOf(ai.publicSourceDir ?: ai.sourceDir)
        ai.splitPublicSourceDirs?.forEach { apkPaths.add(it) }

        // OBB: accessible without Shizuku on most devices
        val extDir = context.externalCacheDirs.firstOrNull()?.parentFile?.parentFile
        val obbDir = extDir?.let { File(it, "obb/$packageName") }
        val obbFiles = if (obbDir?.exists() == true) collectFilesNormal(obbDir) else emptyList()

        // Android/data: requires Shizuku service
        val dataFiles = when (Priv.state()) {
            Priv.State.READY -> {
                try {
                    Priv.listFiles("/storage/emulated/0/Android/data/$packageName")
                } catch (e: Exception) {
                    Log.e(TAG, "Shizuku listFiles failed for $packageName", e)
                    emptyList()
                }
            }
            else -> {
                Log.w(TAG, "Shizuku not ready — skipping Android/data for $packageName")
                emptyList()
            }
        }

        return AppTransferPaths(
            packageName = packageName,
            apkPaths = apkPaths,
            obbFiles = obbFiles,
            dataFiles = dataFiles,
        )
    }

    /**
     * Prepares an app for transfer. Only Android/data is copied to [stagingDir]
     * (requires Shizuku); APK and OBB files are returned as direct source paths
     * so they can be sent without copying — important for large games (OBB 1–4 GB).
     *
     * Returns a map:
     *   "stagingDir"   — path to staging dir (contains app_info.json + data/)
     *   "directPaths"  — list of APK + OBB paths to send directly
     *
     * Throws on fatal error; missing Shizuku is a warning (data/ will be empty).
     */
    fun prepareForSend(context: Context, packageName: String, stagingDir: File): Map<String, Any> {
        val pm = context.packageManager
        @Suppress("DEPRECATION")
        val pi = pm.getPackageInfo(packageName, 0)
        val ai = pi.applicationInfo!!
        val label = pm.getApplicationLabel(ai).toString()

        stagingDir.mkdirs()
        val dataDir = File(stagingDir, "data").apply { mkdirs() }

        // APK paths (accessible directly — no staging needed)
        val directPaths = mutableListOf(ai.publicSourceDir ?: ai.sourceDir)
        ai.splitPublicSourceDirs?.forEach { directPaths.add(it) }

        // OBB paths (accessible directly)
        val extDir = context.externalCacheDirs.firstOrNull()?.parentFile?.parentFile
        val obbSrcDir = extDir?.let { File(it, "obb/$packageName") }
        if (obbSrcDir?.exists() == true) {
            obbSrcDir.walkTopDown().forEach { f ->
                if (f.isFile) directPaths.add(f.absolutePath)
            }
        }

        // Android/data: only this needs Shizuku staging (protected path)
        if (Priv.state() == Priv.State.READY) {
            val dataSrcBase = "/storage/emulated/0/Android/data/$packageName"
            val files = try { Priv.listFiles(dataSrcBase) } catch (e: Exception) {
                Log.e(TAG, "listFiles failed for $packageName", e)
                emptyList()
            }
            for (absPath in files) {
                val rel = absPath.removePrefix("$dataSrcBase/")
                val dst = File(dataDir, rel)
                dst.parentFile?.mkdirs()
                val err = Priv.copyFile(absPath, dst.absolutePath)
                if (err.isNotEmpty()) Log.w(TAG, "Stage copy failed for $rel: $err")
            }
        } else {
            Log.w(TAG, "Shizuku not ready — Android/data skipped for $packageName")
        }

        // Metadata: receiver uses this to identify APKs to install and where OBB goes
        val hasObb = obbSrcDir?.exists() == true
        File(stagingDir, "app_info.json").writeText(
            """{"packageName":"$packageName","label":"${label.replace("\"","\\\"")}" ,"hasObb":$hasObb}"""
        )

        Log.d(TAG, "Prepared $packageName: ${directPaths.size} direct paths, staging=$stagingDir")
        return mapOf("stagingDir" to stagingDir.absolutePath, "directPaths" to directPaths)
    }

    private fun collectFilesNormal(dir: File): List<String> {
        val result = mutableListOf<String>()
        dir.walkTopDown().forEach { f -> if (f.isFile) result.add(f.absolutePath) }
        return result
    }
}
