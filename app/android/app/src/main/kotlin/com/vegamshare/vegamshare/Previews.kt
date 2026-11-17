package com.vegamshare.vegamshare

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.pdf.PdfRenderer
import android.media.MediaMetadataRetriever
import android.os.ParcelFileDescriptor
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * Previews for the WhatsApp files screen: PDF pages and video frames, made
 * with Android's own PdfRenderer / MediaMetadataRetriever (no extra library).
 * Called only while that screen asks for them; nothing runs in the background.
 */
object Previews {
    /** One thread: PdfRenderer is not thread-safe and the list asks many at once. */
    val executor = java.util.concurrent.Executors.newSingleThreadExecutor()

    private fun <T> withPdf(path: String, block: (PdfRenderer) -> T): T =
        ParcelFileDescriptor.open(File(path), ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
            PdfRenderer(fd).use(block)
        }

    fun pdfPageCount(path: String): Int = withPdf(path) { it.pageCount }

    /** Page [index] as JPEG, [width] px wide. Encrypted or broken PDFs throw. */
    fun pdfPage(path: String, index: Int, width: Int): ByteArray = withPdf(path) { pdf ->
        pdf.openPage(index).use { page ->
            val w = width.coerceIn(64, 2000)
            val h = (w.toLong() * page.height / page.width.coerceAtLeast(1)).toInt().coerceIn(1, 4000)
            val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
            bmp.eraseColor(Color.WHITE) // PDFs assume paper behind transparent areas
            page.render(bmp, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            jpeg(bmp)
        }
    }

    /** A frame near the start of the video, scaled to [width] px wide. */
    fun videoFrame(path: String, width: Int): ByteArray? {
        val r = MediaMetadataRetriever()
        try {
            r.setDataSource(path)
            val frame = r.getFrameAtTime(1_000_000, MediaMetadataRetriever.OPTION_CLOSEST_SYNC) ?: return null
            val w = width.coerceIn(32, 1000)
            val h = (w.toLong() * frame.height / frame.width.coerceAtLeast(1)).toInt().coerceAtLeast(1)
            val small = Bitmap.createScaledBitmap(frame, w, h, true)
            if (small !== frame) frame.recycle()
            return jpeg(small)
        } finally {
            runCatching { r.release() }
        }
    }

    private fun jpeg(bmp: Bitmap): ByteArray {
        val out = ByteArrayOutputStream()
        bmp.compress(Bitmap.CompressFormat.JPEG, 85, out)
        bmp.recycle()
        return out.toByteArray()
    }
}
