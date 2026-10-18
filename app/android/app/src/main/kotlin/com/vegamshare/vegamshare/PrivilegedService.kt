package com.vegamshare.vegamshare

import android.content.Context
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import androidx.annotation.Keep
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import kotlin.system.exitProcess

/**
 * Runs in a separate process started by Shizuku (shell/ADB rights).
 * Only VegamShare can bind to it.
 */
class PrivilegedService : IPrivileged.Stub {

    @Keep
    constructor() : super()

    @Keep
    @Suppress("UNUSED_PARAMETER")
    constructor(context: Context) : super()

    override fun destroy() {
        exitProcess(0)
    }

    override fun exec(argv: Array<String>): String {
        val p = ProcessBuilder(argv.toList()).redirectErrorStream(true).start()
        val out = p.inputStream.bufferedReader().readText()
        return "${p.waitFor()}\n$out"
    }

    override fun listFiles(path: String): String {
        val sb = StringBuilder()
        File(path).walkTopDown().forEach { f ->
            if (f.isFile) sb.appendLine(f.absolutePath)
        }
        return sb.toString()
    }

    override fun copyFile(srcPath: String, dstPath: String): String {
        return try {
            val dst = File(dstPath)
            dst.parentFile?.let { ensureDir(it) }
            FileInputStream(srcPath).use { inp ->
                FileOutputStream(dst).use { out -> inp.copyTo(out) }
            }
            dst.setLastModified(File(srcPath).lastModified())
            openUp(dst, dir = false)
            ""
        } catch (e: Exception) {
            e.message ?: e.javaClass.simpleName
        }
    }

    override fun mkdirs(path: String): String {
        return try {
            ensureDir(File(path))
            ""
        } catch (e: Exception) {
            e.message ?: e.javaClass.simpleName
        }
    }

    private fun ensureDir(dir: File) {
        if (dir.isDirectory) return
        dir.parentFile?.let { ensureDir(it) }
        if (!dir.mkdir() && !dir.isDirectory) throw java.io.IOException("cannot create ${dir.path}")
        openUp(dir, dir = true)
    }

    private fun openUp(f: File, dir: Boolean) {
        try {
            val st = Os.stat(f.path)
            if (st.st_uid != Os.getuid()) return
            val extra = if (dir) "007".toInt(8) else "006".toInt(8)
            Os.chmod(f.path, (st.st_mode and "7777".toInt(8)) or extra)
        } catch (_: ErrnoException) {
        }
    }
}
