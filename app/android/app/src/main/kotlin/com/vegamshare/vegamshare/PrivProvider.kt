package com.vegamshare.vegamshare

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri
import android.os.Binder
import android.os.Bundle

/** Receives the [PrivilegedService] binder from [PrivServer]. */
class PrivProvider : ContentProvider() {

    override fun call(method: String, arg: String?, extras: Bundle?): Bundle? {
        val uid = Binder.getCallingUid()
        if (uid != SHELL_UID && uid != ROOT_UID) throw SecurityException("not allowed")
        if (method == METHOD_BINDER) extras?.getBinder(KEY_BINDER)?.let { Priv.attach(it) }
        return null
    }

    override fun onCreate() = true
    override fun query(uri: Uri, p: Array<out String>?, s: String?, a: Array<out String>?, o: String?): Cursor? = null
    override fun getType(uri: Uri): String? = null
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, s: String?, a: Array<out String>?) = 0
    override fun update(uri: Uri, v: ContentValues?, s: String?, a: Array<out String>?) = 0

    companion object {
        const val AUTHORITY = "${BuildConfig.APPLICATION_ID}.priv"
        const val METHOD_BINDER = "binder"
        const val KEY_BINDER = "binder"
        private const val SHELL_UID = 2000
        private const val ROOT_UID = 0
    }
}
