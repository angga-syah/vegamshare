package com.vegamshare.vegamshare

import android.graphics.drawable.Drawable
import android.text.format.Formatter
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView

class AppItem(
    val pkg: String,
    val label: String,
    val icon: Drawable,
    val isGame: Boolean,
    /** UID without the user part; the same app has it in every user. */
    val appId: Int,
)

class AppAdapter(private val onTap: (AppItem) -> Unit) : RecyclerView.Adapter<AppAdapter.Holder>() {

    var items: List<AppItem> = emptyList()
        private set
    private var cloned: Set<String> = emptySet()
    private val sizes = HashMap<String, Long>()

    fun submit(items: List<AppItem>, cloned: Set<String>) {
        this.items = items
        this.cloned = cloned
        @Suppress("NotifyDataSetChanged") // the whole list is replaced
        notifyDataSetChanged()
    }

    fun setCloned(cloned: Set<String>) {
        this.cloned = cloned
        notifyItemRangeChanged(0, items.size)
    }

    /** An app with nothing to copy (only logs, cache, settings) is dropped from the list. */
    fun setSize(pkg: String, bytes: Long) {
        sizes[pkg] = bytes
        val i = items.indexOfFirst { it.pkg == pkg }
        if (i < 0) return
        if (bytes == 0L) {
            items = items.toMutableList().also { it.removeAt(i) }
            notifyItemRemoved(i)
        } else {
            notifyItemChanged(i)
        }
    }

    fun sizeOf(pkg: String): Long? = sizes[pkg]

    override fun getItemCount() = items.size

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
        Holder(LayoutInflater.from(parent.context).inflate(R.layout.item_app, parent, false))

    override fun onBindViewHolder(h: Holder, position: Int) {
        val item = items[position]
        val ctx = h.itemView.context
        h.icon.setImageDrawable(item.icon)
        h.title.text = item.label
        val size = sizes[item.pkg]?.let { Formatter.formatShortFileSize(ctx, it) } ?: ctx.getString(R.string.size_measuring)
        h.subtitle.text = if (item.isGame) "${ctx.getString(R.string.game)} · $size" else size
        val isCloned = item.pkg in cloned
        h.badge.text = ctx.getString(if (isCloned) R.string.badge_cloned else R.string.badge_not_cloned)
        h.badge.alpha = if (isCloned) 1f else 0.6f
        h.itemView.setOnClickListener { onTap(item) }
    }

    class Holder(v: View) : RecyclerView.ViewHolder(v) {
        val icon: ImageView = v.findViewById(R.id.icon)
        val title: TextView = v.findViewById(R.id.title)
        val subtitle: TextView = v.findViewById(R.id.subtitle)
        val badge: TextView = v.findViewById(R.id.badge)
    }
}
