package com.vegamshare.vegamshare

import android.os.Binder
import android.os.Parcel
import android.os.ParcelFileDescriptor
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import kotlin.system.exitProcess

/** Runs in [PrivServer], as the shell user. Only [appUid] may call it. */
class PrivilegedService(private val appUid: Int) : IPrivileged.Stub() {

    override fun onTransact(code: Int, data: Parcel, reply: Parcel?, flags: Int): Boolean {
        val caller = Binder.getCallingUid()
        if (caller != appUid && caller != Os.getuid()) throw SecurityException("uid $caller not allowed")
        return super.onTransact(code, data, reply, flags)
    }

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

    // ---- send app: one app's data folders ------------------------------------

    /** The app folder, or an exception for anything outside it. */
    private fun appFolder(pkg: String, kind: String): File {
        checkPackage(pkg)
        require(kind == "data" || kind == "obb") { "bad kind" }
        return root(0, kind, pkg)
    }

    private fun inside(base: File, relPath: String): File {
        require(relPath.isNotEmpty() && !relPath.startsWith("/") && relPath.split('/').none { it == ".." || it.isEmpty() }) { "bad path" }
        return File(base, relPath)
    }

    override fun listAppFiles(pkg: String, kind: String): ParcelFileDescriptor {
        val base = appFolder(pkg, kind)
        val (read, write) = ParcelFileDescriptor.createPipe()
        Thread {
            ParcelFileDescriptor.AutoCloseOutputStream(write).bufferedWriter().use { out ->
                val stack = ArrayDeque<File>()
                stack.add(base)
                while (stack.isNotEmpty()) {
                    for (f in stack.removeLast().listFiles() ?: continue) {
                        if (isLink(f)) continue
                        if (f.isDirectory) stack.add(f)
                        else if (f.isFile && f.canRead()) out.append(f.length().toString()).append('\t').append(f.lastModified().toString())
                            .append('\t').append(f.path.substring(base.path.length + 1)).append('\n')
                    }
                }
            }
        }.start()
        return read
    }

    override fun openAppFile(pkg: String, kind: String, relPath: String): ParcelFileDescriptor {
        val f = inside(appFolder(pkg, kind), relPath)
        require(!isLink(f)) { "link" }
        return ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    /** File lists fixed by [archiveSize], so the stream matches the announced length. */
    private val archivePlans = HashMap<String, List<Tar.Item>>()

    private fun archivePlan(pkg: String, kind: String): List<Tar.Item> {
        val base = appFolder(pkg, kind)
        val items = ArrayList<Tar.Item>()
        val stack = ArrayDeque<File>()
        stack.add(base)
        while (stack.isNotEmpty()) {
            for (f in stack.removeLast().listFiles() ?: continue) {
                if (isLink(f)) continue
                val rel = f.path.substring(base.path.length + 1)
                if (kind == "data" && (rel == "cache" || rel.startsWith("cache/"))) continue
                if (f.isDirectory) stack.add(f) else if (f.isFile && f.canRead()) items += Tar.Item(f, rel, f.length(), f.lastModified())
            }
        }
        items.sortBy { it.name }
        return items
    }

    override fun archiveSize(pkg: String, kind: String): Long {
        val plan = archivePlan(pkg, kind)
        synchronized(archivePlans) { archivePlans["$pkg/$kind"] = plan }
        return if (plan.isEmpty()) 0 else Tar.length(plan)
    }

    override fun openAppArchive(pkg: String, kind: String): ParcelFileDescriptor {
        val plan = synchronized(archivePlans) { archivePlans["$pkg/$kind"] } ?: archivePlan(pkg, kind)
        val (read, write) = ParcelFileDescriptor.createPipe()
        Thread {
            // The reader closing early (a paused transfer) ends this with an IOException.
            runCatching { ParcelFileDescriptor.AutoCloseOutputStream(write).buffered(1 shl 20).use { Tar.write(plan, it) } }
        }.start()
        return read
    }

    override fun extractIntoApp(tarPath: String, pkg: String, kind: String, progress: IProgress?): String = try {
        val tar = File(tarPath)
        require(tar.path.startsWith("/storage/emulated/0/") && !tarPath.contains("/../")) { "bad source" }
        val base = appFolder(pkg, kind)
        cancelled = false
        var files = 0
        var errors = 0
        val total = tar.length()
        var lastReport = 0L
        val raw = FileInputStream(tar)
        raw.buffered(1 shl 20).use { input ->
            Tar.read(input) { name, mtimeMs, data, _ ->
                if (cancelled) throw CancelledException()
                val now = System.currentTimeMillis()
                if (now - lastReport > REPORT_EVERY_MS) {
                    lastReport = now
                    progress?.onProgress(raw.channel.position(), total, name.substringAfterLast('/'))
                }
                try {
                    val dst = inside(base, name)
                    ensureDir(dst.parentFile!!)
                    FileOutputStream(dst).use { data.copyTo(it, 1 shl 20) }
                    if (mtimeMs > 0) dst.setLastModified(mtimeMs)
                    openUp(dst, dir = false)
                    files++
                } catch (_: Exception) {
                    errors++
                }
            }
        }
        progress?.onProgress(total, total, "")
        "$files|$errors"
    } catch (_: CancelledException) {
        "CANCELLED"
    } catch (e: Exception) {
        "ERR|${e.message}"
    }

    private class CancelledException : Exception()

    override fun moveIntoApp(srcPath: String, pkg: String, kind: String, relPath: String): String = try {
        val src = File(srcPath)
        require(src.path.startsWith("/storage/emulated/0/") && !srcPath.contains("/../")) { "bad source" }
        val dst = inside(appFolder(pkg, kind), relPath)
        ensureDir(dst.parentFile!!)
        if (!src.renameTo(dst)) {
            FileInputStream(src).use { i -> FileOutputStream(dst).use { o -> i.copyTo(o, 1 shl 20) } }
            dst.setLastModified(src.lastModified())
            src.delete()
        }
        openUp(dst, dir = false)
        ""
    } catch (e: Exception) {
        e.message ?: e.javaClass.simpleName
    }

    // ---- clone: game data between users ------------------------------------

    @Volatile
    private var cancelled = false

    override fun cancel() {
        cancelled = true
    }

    override fun measure(pkg: String, user: Int): Long {
        checkPackage(pkg)
        return plan(pkg, user).sumOf { it.length() }
    }

    override fun prepareTarget(pkg: String, user: Int, appId: Int): Int {
        checkPackage(pkg)
        val dir = root(user, "data", pkg)
        if (dir.isDirectory) return 0
        // The storage service usually hands a new Android/data/<pkg> folder
        // to its app; if it did not, the app must create it itself.
        if (dir.mkdir()) {
            val owner = try { Os.stat(dir.path).st_uid } catch (_: ErrnoException) { -1 }
            if (owner == user * PER_USER_RANGE + appId) return 1
            dir.delete()
        }
        return 2
    }

    override fun copyData(pkg: String, fromUser: Int, toUser: Int, progress: IProgress?): String {
        checkPackage(pkg)
        require(fromUser != toUser) { "same user" }
        cancelled = false
        if (!root(toUser, "data", pkg).isDirectory) return "ERR|no_target"

        val todo = ArrayList<Pair<File, File>>()
        var same = 0
        for (src in plan(pkg, fromUser)) {
            val dst = target(src, fromUser, toUser)
            if (dst.isFile && dst.length() == src.length() &&
                Math.abs(dst.lastModified() - src.lastModified()) < MTIME_SLACK_MS
            ) {
                same++
            } else {
                todo += src to dst
            }
        }
        val total = todo.sumOf { it.first.length() }
        var done = 0L
        var copied = 0
        var errors = 0
        var firstError = ""
        var lastReport = 0L
        val buf = ByteArray(1 shl 20)

        for ((src, dst) in todo) {
            if (cancelled) return "CANCELLED|$copied|$done|$same|$errors|$firstError"
            try {
                ensureDir(dst.parentFile!!)
                FileInputStream(src).use { input ->
                    FileOutputStream(dst).use { output ->
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            output.write(buf, 0, n)
                            done += n
                            val now = System.currentTimeMillis()
                            if (now - lastReport > REPORT_EVERY_MS) {
                                lastReport = now
                                progress?.onProgress(done, total, src.name)
                            }
                            if (cancelled) break
                        }
                    }
                }
                dst.setLastModified(src.lastModified())
                openUp(dst, dir = false)
                copied++
            } catch (e: Exception) {
                errors++
                if (firstError.isEmpty()) firstError = "${src.name}: ${e.message}"
            }
        }
        progress?.onProgress(total, total, "")
        val status = if (cancelled) "CANCELLED" else "OK"
        return "$status|$copied|$done|$same|$errors|$firstError"
    }

    /**
     * Game data: everything in obb, and the folders in Android/data/<pkg>/files.
     * Left out: the cache folder, loose small files at the top of files/
     * (settings, records, screenshots), and log, crash and cache folders.
     */
    private fun plan(pkg: String, user: Int): List<File> {
        val out = ArrayList<File>()
        val files = File(root(user, "data", pkg), "files")
        files.listFiles()?.forEach { f ->
            when {
                isLink(f) -> {}
                f.isDirectory -> if (!skipFolder(f.name)) walk(f, out)
                f.isFile -> if (f.length() >= LOOSE_FILE_MIN && !isMedia(f.name) && f.canRead()) out += f
            }
        }
        walk(root(user, "obb", pkg), out)
        return out
    }

    private fun walk(top: File, out: MutableList<File>) {
        val stack = ArrayDeque<File>()
        stack.add(top)
        while (stack.isNotEmpty()) {
            val d = stack.removeLast()
            for (f in d.listFiles() ?: continue) {
                if (isLink(f)) continue
                if (f.isDirectory) stack.add(f) else if (f.isFile && f.canRead()) out += f
            }
        }
    }

    private fun skipFolder(name: String): Boolean {
        val n = name.lowercase()
        return n in SKIP_FOLDERS || n.endsWith("cache") || n.startsWith("crash") ||
            name.endsWith("Log") || n.endsWith("_log") || n.endsWith("_logs")
    }

    private fun isMedia(name: String) = name.substringAfterLast('.', "").lowercase() in MEDIA

    private fun isLink(f: File) = try {
        OsConstants.S_ISLNK(Os.lstat(f.path).st_mode)
    } catch (_: ErrnoException) {
        true
    }

    private fun root(user: Int, kind: String, pkg: String) = File("/storage/emulated/$user/Android/$kind/$pkg")

    private fun target(src: File, fromUser: Int, toUser: Int): File {
        val from = "/storage/emulated/$fromUser/"
        check(src.path.startsWith(from))
        return File("/storage/emulated/$toUser/" + src.path.substring(from.length))
    }

    private fun checkPackage(pkg: String) {
        require(PACKAGE.matches(pkg)) { "bad package name" }
    }

    // ---- paths and permissions --------------------------------------------

    private fun ensureDir(dir: File) {
        if (dir.isDirectory) return
        dir.parentFile?.let { ensureDir(it) }
        if (!dir.mkdir() && !dir.isDirectory) throw java.io.IOException("cannot create ${dir.path}")
        openUp(dir, dir = true)
    }

    /**
     * Files we create belong to the shell user, which the app is not. The
     * app's own Android/data/<pkg> folder stays closed to other apps, so
     * opening what is inside it to "others" only lets the app itself in.
     */
    private fun openUp(f: File, dir: Boolean) {
        try {
            val st = Os.stat(f.path)
            if (st.st_uid != Os.getuid()) return
            val extra = if (dir) "007".toInt(8) else "006".toInt(8)
            Os.chmod(f.path, (st.st_mode and "7777".toInt(8)) or extra)
        } catch (_: ErrnoException) {
        }
    }

    private companion object {
        const val PER_USER_RANGE = 100000
        const val MTIME_SLACK_MS = 2000L
        const val REPORT_EVERY_MS = 150L
        const val LOOSE_FILE_MIN = 1L shl 20
        val PACKAGE = Regex("^[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z0-9_]+)+$")
        val SKIP_FOLDERS = setOf(
            "cache", "caches", "log", "logs", "tmp", "temp", "crash", "crashes",
            "recordvideo", "screenshot", "screenshots", "dump", "dumps",
        )
        val MEDIA = setOf("jpg", "jpeg", "png", "gif", "webp", "mp4", "mov", "mkv", "3gp")
    }
}
