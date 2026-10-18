package com.vegamshare.vegamshare

import android.content.Context
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.provider.Settings
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/**
 * A small number pad floating at the top of the screen, above Settings. The pairing
 * code stays visible below it, so nothing has to be remembered and no keyboard covers
 * it. Needs "Display over other apps"; call on the main thread.
 */
class PairingKeypad(
    private val context: Context,
    private val onCode: (String) -> Unit,
    private val onClose: () -> Unit,
) {
    private val wm = context.getSystemService(WindowManager::class.java)
    private var root: View? = null
    private lateinit var digits: TextView
    private lateinit var status: TextView
    private val code = StringBuilder()
    private var locked = false

    val showing get() = root != null

    fun show(): Boolean {
        if (root != null) return true
        if (!Settings.canDrawOverlays(context)) return false
        root = build()
        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP
            y = dp(28)
        }
        return runCatching { wm.addView(root, params) }.isSuccess.also { if (!it) root = null }
    }

    fun hide() {
        root?.let { runCatching { wm.removeView(it) } }
        root = null
    }

    /** Shows progress or an error; an error clears the code for another try. */
    fun status(text: String, error: Boolean) {
        if (root == null) return
        status.text = text
        locked = !error
        if (error) {
            code.clear()
            render()
        }
    }

    private fun build(): View {
        val pad = dp(12)
        val box = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
            background = GradientDrawable().apply {
                cornerRadius = dp(20).toFloat()
                setColor(0xF0202124.toInt())
            }
        }
        val outer = LinearLayout(context).apply {
            setPadding(dp(12), 0, dp(12), 0)
            addView(box, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        }
        status = text(context.getString(R.string.keypad_hint), 14f)
        digits = text("", 28f).apply { letterSpacing = 0.3f; gravity = Gravity.CENTER }
        box.addView(status)
        box.addView(digits)
        for (row in listOf("12345", "67890")) {
            box.addView(LinearLayout(context).apply { row.forEach { c -> addView(key(c.toString()) { press(c) }, weight()) } })
        }
        box.addView(LinearLayout(context).apply {
            addView(key("⌫") { if (!locked && code.isNotEmpty()) { code.setLength(code.length - 1); render() } }, weight())
            addView(key(context.getString(R.string.cancel)) { hide(); onClose() }, weight())
        })
        render()
        return outer
    }

    private fun press(c: Char) {
        if (locked || code.length >= 6) return
        code.append(c)
        render()
        if (code.length == 6) {
            locked = true
            onCode(code.toString())
        }
    }

    private fun render() {
        digits.text = (0 until 6).joinToString(" ") { if (it < code.length) code[it].toString() else "_" }
    }

    private fun text(s: String, sp: Float) = TextView(context).apply {
        text = s
        setTextColor(0xFFFFFFFF.toInt())
        setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
        setPadding(dp(4), dp(2), dp(4), dp(6))
    }

    private fun key(label: String, onClick: () -> Unit) = Button(context).apply {
        text = label
        isAllCaps = false
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
        setOnClickListener { onClick() }
    }

    private fun weight() = LinearLayout.LayoutParams(0, dp(52), 1f)

    private fun dp(v: Int) = (v * context.resources.displayMetrics.density).toInt()
}
