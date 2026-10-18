package com.vegamshare.vegamshare

import java.io.File
import java.io.FileInputStream
import java.io.InputStream
import java.io.OutputStream

/**
 * Minimal tar (ustar + GNU long names) for streaming an app folder. Only regular
 * files are written; sizes come from a fixed plan so the stream length is known
 * before it is produced, and producing it twice gives the same bytes.
 */
object Tar {
    private const val BLOCK = 512
    private const val LONG_NAME = "././@LongLink"

    class Item(val file: File, val name: String, val size: Long, val mtimeMs: Long)

    private fun padded(n: Long) = (n + BLOCK - 1) / BLOCK * BLOCK

    fun length(items: List<Item>): Long = items.sumOf { item ->
        val nameLen = item.name.toByteArray().size
        val long = if (nameLen > 100) BLOCK + padded(nameLen + 1L) else 0L
        long + BLOCK + padded(item.size)
    } + 2L * BLOCK

    /** Writes exactly [length] bytes; a file that changed since planning is cut or zero-padded. */
    fun write(items: List<Item>, out: OutputStream) {
        val buf = ByteArray(1 shl 20)
        for (item in items) {
            val nameBytes = item.name.toByteArray()
            if (nameBytes.size > 100) {
                out.write(header(LONG_NAME.toByteArray(), nameBytes.size + 1L, 0, 'L'))
                out.write(nameBytes)
                out.write(ByteArray((padded(nameBytes.size + 1L) - nameBytes.size).toInt()))
            }
            out.write(header(nameBytes.copyOf(minOf(nameBytes.size, 100)), item.size, item.mtimeMs / 1000, '0'))
            var left = item.size
            runCatching {
                FileInputStream(item.file).use { input ->
                    while (left > 0) {
                        val n = input.read(buf, 0, minOf(left, buf.size.toLong()).toInt())
                        if (n < 0) break
                        out.write(buf, 0, n)
                        left -= n
                    }
                }
            }
            while (left > 0) {
                val n = minOf(left, buf.size.toLong()).toInt()
                out.write(ByteArray(n))
                left -= n
            }
            out.write(ByteArray((padded(item.size) - item.size).toInt()))
        }
        out.write(ByteArray(2 * BLOCK))
        out.flush()
    }

    private fun header(name: ByteArray, size: Long, mtime: Long, type: Char): ByteArray {
        val h = ByteArray(BLOCK)
        name.copyInto(h, 0)
        octal(h, 100, 8, 420) // 0644
        octal(h, 108, 8, 0)
        octal(h, 116, 8, 0)
        if (size < 8589934592L) octal(h, 124, 12, size) else {
            h[124] = 0x80.toByte()
            for (i in 0 until 8) h[135 - i] = (size ushr (8 * i)).toByte()
        }
        octal(h, 136, 12, mtime)
        h[156] = type.code.toByte()
        "ustar\u0000".toByteArray().copyInto(h, 257)
        "00".toByteArray().copyInto(h, 263)
        for (i in 148 until 156) h[i] = ' '.code.toByte()
        val sum = h.sumOf { it.toInt() and 0xff }
        octal(h, 148, 7, sum.toLong())
        h[155] = ' '.code.toByte()
        return h
    }

    private fun octal(h: ByteArray, at: Int, len: Int, value: Long) {
        val s = java.lang.Long.toOctalString(value).padStart(len - 1, '0')
        s.toByteArray().copyInto(h, at)
        h[at + len - 1] = 0
    }

    /** Calls [onFile] with each regular file's relative name, mtime (ms) and content stream. */
    fun read(input: InputStream, onFile: (name: String, mtimeMs: Long, data: InputStream, size: Long) -> Unit) {
        val h = ByteArray(BLOCK)
        var longName: String? = null
        while (true) {
            if (!readFully(input, h)) return
            if (h.all { it.toInt() == 0 }) return
            val size = parseSize(h)
            val type = h[156].toInt().toChar()
            val name = longName ?: String(h, 0, 100).substringBefore('\u0000')
            longName = null
            when (type) {
                'L' -> {
                    val bytes = ByteArray(size.toInt())
                    readFully(input, bytes)
                    skip(input, padded(size) - size)
                    longName = String(bytes).substringBefore('\u0000')
                }
                '0', '\u0000' -> {
                    val mtime = String(h, 136, 12).trim { it <= ' ' || it == '\u0000' }.toLongOrNull(8) ?: 0L
                    val bounded = Bounded(input, size)
                    onFile(name, mtime * 1000, bounded, size)
                    skip(input, bounded.left)
                    skip(input, padded(size) - size)
                }
                else -> skip(input, padded(size))
            }
        }
    }

    private fun parseSize(h: ByteArray): Long {
        if (h[124].toInt() and 0x80 != 0) {
            var v = 0L
            for (i in 128 until 136) v = (v shl 8) or (h[i].toLong() and 0xff)
            return v
        }
        return String(h, 124, 12).trim { it <= ' ' || it == '\u0000' }.toLongOrNull(8) ?: 0L
    }

    private fun readFully(input: InputStream, b: ByteArray): Boolean {
        var off = 0
        while (off < b.size) {
            val n = input.read(b, off, b.size - off)
            if (n < 0) return false
            off += n
        }
        return true
    }

    private fun skip(input: InputStream, count: Long) {
        var left = count
        val buf = ByteArray(8192)
        while (left > 0) {
            val n = input.read(buf, 0, minOf(left, buf.size.toLong()).toInt())
            if (n < 0) return
            left -= n
        }
    }

    /** At most [left] bytes of the underlying stream. */
    private class Bounded(private val input: InputStream, var left: Long) : InputStream() {
        override fun read(): Int {
            if (left <= 0) return -1
            val b = input.read()
            if (b >= 0) left--
            return b
        }

        override fun read(b: ByteArray, off: Int, len: Int): Int {
            if (left <= 0) return -1
            val n = input.read(b, off, minOf(len.toLong(), left).toInt())
            if (n > 0) left -= n
            return n
        }
    }
}
