package com.vegamshare.vegamshare

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.RemoteInput
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.annotation.RequiresApi
import moe.shizuku.manager.adb.AdbMdns

/**
 * Lets the user type the pairing code into a notification, so the Wireless
 * debugging pairing dialog (which closes when Settings is left) stays open.
 */
@RequiresApi(Build.VERSION_CODES.R)
class PairingService : Service() {

    private var mdns: AdbMdns? = null
    @Volatile private var host = "127.0.0.1"
    @Volatile private var port = -1
    @Volatile private var busy = false
    @Volatile private var stopped = false
    private val main = Handler(Looper.getMainLooper())
    private var keypad: PairingKeypad? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_REPLY -> onReply(intent)
            ACTION_STOP -> stopAll()
            else -> start()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        keypad?.hide()
        mdns?.stop()
        super.onDestroy()
    }

    private fun start() {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.pair_channel), NotificationManager.IMPORTANCE_HIGH)
        )
        val first = if (EmbeddedServer.wasPaired(this)) R.string.pair_reconnect else R.string.pair_searching
        startForeground(ID, build(getString(first), withInput = false), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        if (mdns == null && EmbeddedServer.wasPaired(this)) reconnectLoop()
        if (mdns == null) {
            mdns = AdbMdns(this, AdbMdns.TLS_PAIRING) { h, p ->
                if (p != -1 && !busy) {
                    host = h
                    port = p
                    notify(build(getString(R.string.pair_found), withInput = true))
                    main.post { showKeypad() }
                }
            }.also { it.start() }
        }
    }

    /** Already paired: as soon as Wireless debugging is on, start without a code. */
    private fun reconnectLoop() {
        Thread {
            val until = System.currentTimeMillis() + RECONNECT_FOR_MS
            while (System.currentTimeMillis() < until && Priv.service == null && !stopped) {
                if (busy) { Thread.sleep(1000); continue }
                try {
                    EmbeddedServer.connectAndStart(this, "127.0.0.1")
                    finishOk()
                    return@Thread
                } catch (_: Exception) {
                    Thread.sleep(2000)
                }
            }
        }.start()
    }

    /** The floating number pad, when "Display over other apps" is allowed. */
    private fun showKeypad() {
        val pad = keypad ?: PairingKeypad(this, onCode = { submitCode(it) }, onClose = {}).also { keypad = it }
        if (!busy) pad.show()
    }

    private fun keypadStatus(text: String, error: Boolean) = main.post { keypad?.status(text, error) }

    private fun finishOk() {
        main.post { keypad?.hide() }
        mdns?.stop()
        stopForeground(STOP_FOREGROUND_DETACH)
        notify(build(getString(R.string.pair_done), withInput = false, ongoing = false))
        // Apps may not open activities from the background, but the helper (shell) may:
        // brings the user back from Settings.
        runCatching { Priv.sh("am", "start", "-n", "$packageName/${MainActivity::class.java.name}") }
        stopSelf()
    }

    private fun onReply(intent: Intent) {
        submitCode(RemoteInput.getResultsFromIntent(intent)?.getCharSequence(KEY_CODE)?.toString()?.trim().orEmpty())
    }

    private fun submitCode(code: String) {
        if (code.isEmpty() || port == -1 || busy) return
        busy = true
        notify(build(getString(R.string.setup_status_pairing), withInput = false))
        keypadStatus(getString(R.string.setup_status_pairing), error = false)
        Thread {
            try {
                EmbeddedServer.pair(this, host, port, code)
                notify(build(getString(R.string.setup_status_connecting), withInput = false))
                keypadStatus(getString(R.string.setup_status_connecting), error = false)
                EmbeddedServer.connectAndStart(this, host)
                finishOk()
            } catch (e: Exception) {
                busy = false
                val msg = getString(R.string.setup_status_error, e.message ?: e.javaClass.simpleName)
                notify(build(msg, withInput = true))
                keypadStatus(msg, error = true)
            }
        }.start()
    }

    private fun stopAll() {
        main.post { keypad?.hide() }
        stopped = true
        mdns?.stop()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun build(text: String, withInput: Boolean, ongoing: Boolean = true): Notification {
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val b = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(getString(R.string.setup_title))
            .setContentText(text)
            .setStyle(Notification.BigTextStyle().bigText(text))
            .setContentIntent(open)
            .setOngoing(ongoing)
            .setOnlyAlertOnce(false)
        if (withInput) {
            val reply = PendingIntent.getService(
                this, 1, Intent(this, PairingService::class.java).setAction(ACTION_REPLY),
                PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            val input = RemoteInput.Builder(KEY_CODE).setLabel(getString(R.string.setup_pairing_code_hint)).build()
            b.addAction(
                Notification.Action.Builder(null, getString(R.string.pair_enter_code), reply).addRemoteInput(input).build()
            )
        }
        if (ongoing) {
            val stop = PendingIntent.getService(
                this, 2, Intent(this, PairingService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE,
            )
            b.addAction(Notification.Action.Builder(null, getString(R.string.cancel), stop).build())
        }
        return b.build()
    }

    private fun notify(n: Notification) = getSystemService(NotificationManager::class.java).notify(ID, n)

    companion object {
        private const val CHANNEL = "pairing"
        private const val ID = 1
        private const val KEY_CODE = "code"
        private const val ACTION_REPLY = "reply"
        private const val ACTION_STOP = "stop"
        private const val RECONNECT_FOR_MS = 3 * 60_000L

        fun start(context: Context) {
            context.startForegroundService(Intent(context, PairingService::class.java))
        }
    }
}
