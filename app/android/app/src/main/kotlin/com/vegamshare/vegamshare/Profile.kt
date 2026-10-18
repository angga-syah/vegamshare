package com.vegamshare.vegamshare

import android.content.Context
import android.os.Build

/** An Android user or profile that apps can be cloned into. */
data class Profile(val id: Int, val name: String, val type: String, val running: Boolean) {

    enum class Kind { DUAL_APPS, CLONE, WORK, SECURE_FOLDER, PRIVATE_SPACE, OTHER_USER }

    val kind: Kind
        get() = when {
            // Xiaomi/POCO/Redmi "Dual apps" and Oppo/Realme/OnePlus "App cloner".
            id == 999 -> Kind.DUAL_APPS
            type.contains("profile.CLONE", ignoreCase = true) -> Kind.CLONE
            type.contains("profile.PRIVATE", ignoreCase = true) -> Kind.PRIVATE_SPACE
            name.contains("Secure Folder", ignoreCase = true) || id == 150 -> Kind.SECURE_FOLDER
            type.contains("profile.MANAGED", ignoreCase = true) -> Kind.WORK
            else -> Kind.OTHER_USER
        }

    fun label(ctx: Context): String = when (kind) {
        Kind.DUAL_APPS -> ctx.getString(R.string.profile_dual)
        Kind.CLONE -> ctx.getString(R.string.profile_clone)
        Kind.WORK -> ctx.getString(R.string.profile_work)
        Kind.SECURE_FOLDER -> ctx.getString(R.string.profile_secure_folder)
        Kind.PRIVATE_SPACE -> ctx.getString(R.string.profile_private)
        Kind.OTHER_USER -> name.ifBlank { ctx.getString(R.string.profile_user, id) }
    }

    companion object {
        /** All users and profiles, from `cmd user list -v` (or `pm list users`). */
        fun list(): List<Profile> {
            val verbose = Priv.sh("cmd", "user", "list", "-v")
            if (verbose.ok) {
                val parsed = verbose.out.lineSequence().mapNotNull { line ->
                    VERBOSE.find(line)?.let { m ->
                        Profile(
                            id = m.groupValues[1].toInt(),
                            name = m.groupValues[2].trim(),
                            type = m.groupValues[3].trim(),
                            running = line.contains("(running)"),
                        )
                    }
                }.toList()
                if (parsed.isNotEmpty()) return parsed
            }
            return Priv.sh("pm", "list", "users").out.lineSequence().mapNotNull { line ->
                SHORT.find(line)?.let { m ->
                    Profile(m.groupValues[1].toInt(), m.groupValues[2].trim(), "", line.trimEnd().endsWith("running"))
                }
            }.toList()
        }

        /** What to tell the user when there is no profile to clone into yet. */
        fun setupHint(ctx: Context): String {
            val brand = (Build.MANUFACTURER + " " + Build.BRAND).lowercase()
            return when {
                listOf("xiaomi", "redmi", "poco").any { it in brand } -> ctx.getString(R.string.hint_xiaomi)
                listOf("oppo", "realme", "oneplus").any { it in brand } -> ctx.getString(R.string.hint_oppo)
                "vivo" in brand || "iqoo" in brand -> ctx.getString(R.string.hint_vivo)
                "samsung" in brand -> ctx.getString(R.string.hint_samsung)
                listOf("huawei", "honor").any { it in brand } -> ctx.getString(R.string.hint_huawei)
                else -> ctx.getString(R.string.hint_generic)
            }
        }

        private val VERBOSE = Regex("""id=(\d+), name=([^,]*), type=([^,]+),""")
        private val SHORT = Regex("""UserInfo\{(\d+):([^:]*):[0-9a-fA-F]+\}""")
    }
}
