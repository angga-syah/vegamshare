package com.vegamshare.vegamshare

import android.content.ContentUris
import android.content.Context
import android.graphics.Bitmap
import android.os.Build
import android.provider.MediaStore
import android.util.Size
import androidx.annotation.RequiresApi
import java.io.ByteArrayOutputStream

/**
 * WhatsApp files from MediaStore (Android 10+): one indexed query instead of a
 * stat per file through the slow storage layer (39k files: ~12 s -> well
 * under a second). Folders WhatsApp marks with .nomedia ("Sent") are only
 * partly indexed, so Dart still walks those few itself.
 */
@RequiresApi(Build.VERSION_CODES.Q)
object WaFiles {
    /** WhatsApp and WhatsApp Business, new location and old one, as RELATIVE_PATH prefixes. */
    private val roots = listOf(
        "Android/media/com.whatsapp/WhatsApp/Media/",
        "WhatsApp/Media/",
        "Android/media/com.whatsapp.w4b/WhatsApp Business/Media/",
        "WhatsApp Business/Media/",
    )

    /** Parallel lists (id, path, size, mtime ms): cheap to pass to Dart. */
    fun list(context: Context): Map<String, Any> {
        val ids = ArrayList<Long>()
        val paths = ArrayList<String>()
        val sizes = ArrayList<Long>()
        val mtimes = ArrayList<Long>()
        val selection = roots.joinToString(" OR ") { "${MediaStore.MediaColumns.RELATIVE_PATH} LIKE ?" }
        @Suppress("DEPRECATION")
        val projection = arrayOf(
            MediaStore.MediaColumns._ID,
            MediaStore.MediaColumns.DATA,
            MediaStore.MediaColumns.SIZE,
            MediaStore.MediaColumns.DATE_MODIFIED,
            "format", // MTP object format; hidden constant in the SDK
        )
        context.contentResolver.query(
            MediaStore.Files.getContentUri("external"),
            projection,
            "($selection)",
            roots.map { "$it%" }.toTypedArray(),
            null,
        )?.use { c ->
            while (c.moveToNext()) {
                if (c.getInt(4) == FORMAT_DIRECTORY) continue
                val path = c.getString(1) ?: continue
                ids += c.getLong(0)
                paths += path
                sizes += c.getLong(2)
                mtimes += c.getLong(3) * 1000
            }
        }
        return mapOf("ids" to ids, "paths" to paths, "sizes" to sizes, "mtimes" to mtimes)
    }

    /** Thumbnails Android already keeps for its gallery; null when the file is not indexed as media. */
    fun thumb(context: Context, id: Long, video: Boolean, width: Int): ByteArray? {
        val base = if (video) MediaStore.Video.Media.EXTERNAL_CONTENT_URI else MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        val w = width.coerceIn(32, 1000)
        val bmp = runCatching { context.contentResolver.loadThumbnail(ContentUris.withAppendedId(base, id), Size(w, w), null) }.getOrNull()
            ?: return null
        val out = ByteArrayOutputStream()
        bmp.compress(Bitmap.CompressFormat.JPEG, 85, out)
        bmp.recycle()
        return out.toByteArray()
    }

    /** MTP object format "association" = a folder row. */
    private const val FORMAT_DIRECTORY = 0x3001

    /** Several at once: thumbnails come from the system cache, not one slow decoder. */
    val executor = java.util.concurrent.Executors.newFixedThreadPool(3)
}
