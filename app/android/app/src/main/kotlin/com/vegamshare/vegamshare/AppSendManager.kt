package com.vegamshare.vegamshare

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.ParcelFileDescriptor
import org.json.JSONObject
import java.io.File

/**
 * Sender side of "send app": an app goes out as one folder
 *   app_info.json, apk/<base and splits>, data/<Android/data/pkg>, obb/<Android/obb/pkg>
 * Nothing is copied first: APKs are read directly, data and obb through the helper.
 */
object AppSendManager {

    /** Path marker for "the whole folder as a tar stream" in helper file specs. */
    const val ARCHIVE = "@archive"
    private const val HELPER_WAIT_MS = 5_000L

    data class InstalledApp(
        val packageName: String,
        val label: String,
        val apkPath: String,
        val splitApkPaths: List<String>,
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
     * Files of the bundle as maps for Dart (relPath, path, size, mtime, viaHelper).
     * Without the helper only the APKs go out. Call off the main thread.
     */
    fun prepareBundle(context: Context, pkg: String): Map<String, Any> {
        val pm = context.packageManager
        val ai = pm.getApplicationInfo(pkg, 0)
        val label = pm.getApplicationLabel(ai).toString()
        val files = ArrayList<Map<String, Any>>()

        val apks = listOf(ai.publicSourceDir ?: ai.sourceDir) + (ai.splitPublicSourceDirs?.toList() ?: emptyList())
        for (p in apks) {
            val f = File(p)
            files += mapOf("relPath" to "apk/${f.name}", "path" to f.path, "size" to f.length(), "mtime" to f.lastModified() / 1000, "viaHelper" to false)
        }

        var dataFiles = 0
        var obbFiles = 0
        // Right after the app (re)starts the helper needs a moment to hand over its binder.
        val until = System.currentTimeMillis() + HELPER_WAIT_MS
        while (Priv.service == null && System.currentTimeMillis() < until) Thread.sleep(200)
        val helper = Priv.service
        if (helper != null) {
            // Android/data: often 100k+ small files, so one tar stream instead of file by file.
            val tarSize = helper.archiveSize(pkg, "data")
            if (tarSize > 0) {
                files += mapOf("relPath" to "data.tar", "path" to "$pkg/data/$ARCHIVE", "size" to tarSize, "mtime" to System.currentTimeMillis() / 1000, "viaHelper" to true)
                dataFiles = 1
            }
            for (kind in listOf("obb")) {
                ParcelFileDescriptor.AutoCloseInputStream(helper.listAppFiles(pkg, kind)).bufferedReader().useLines { lines ->
                    for (line in lines) {
                        val parts = line.split('\t', limit = 3)
                        if (parts.size < 3) continue
                        val rel = parts[2]
                        // Caches are rebuilt by the app; leave them out.
                        if (kind == "data" && rel.startsWith("cache/")) continue
                        files += mapOf(
                            "relPath" to "$kind/$rel",
                            "path" to "$pkg/$kind/$rel",
                            "size" to parts[0].toLong(),
                            "mtime" to parts[1].toLong() / 1000,
                            "viaHelper" to true,
                        )
                        if (kind == "data") dataFiles++ else obbFiles++
                    }
                }
            }
        }

        val info = File(context.cacheDir, "app_info_$pkg.json")
        info.writeText(
            JSONObject()
                .put("packageName", pkg)
                .put("label", label)
                .put("dataFiles", dataFiles)
                .put("obbFiles", obbFiles)
                .toString()
        )
        files += mapOf("relPath" to "app_info.json", "path" to info.path, "size" to info.length(), "mtime" to info.lastModified() / 1000, "viaHelper" to false)

        return mapOf("root" to label.ifBlank { pkg }, "files" to files, "withData" to (helper != null))
    }
}
