package com.vegamshare.vegamshare

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.net.Uri
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import android.view.View
import android.widget.TextView
import androidx.annotation.RequiresApi
import androidx.appcompat.app.AppCompatActivity
import com.google.android.material.button.MaterialButton
import com.google.android.material.progressindicator.CircularProgressIndicator
import com.google.android.material.textfield.TextInputEditText
import moe.shizuku.manager.adb.AdbMdns

@RequiresApi(Build.VERSION_CODES.R)
class EmbeddedSetupActivity : AppCompatActivity() {

    private lateinit var step0: View
    private lateinit var step1: View
    private lateinit var step2: View

    private lateinit var mdnsStatus: TextView
    private lateinit var portField: TextInputEditText
    private lateinit var codeField: TextInputEditText
    private lateinit var pairButton: MaterialButton

    private lateinit var setupProgress: CircularProgressIndicator
    private lateinit var setupStatus: TextView
    private lateinit var doneButton: MaterialButton
    private lateinit var retryButton: MaterialButton

    private var pairingMdns: AdbMdns? = null
    private var discoveredHost: String = "127.0.0.1"

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_embedded_setup)

        step0 = findViewById(R.id.step0)
        step1 = findViewById(R.id.step1)
        step2 = findViewById(R.id.step2)

        mdnsStatus = findViewById(R.id.mdnsStatus)
        portField = findViewById(R.id.portField)
        codeField = findViewById(R.id.codeField)
        pairButton = findViewById(R.id.pairButton)

        setupProgress = findViewById(R.id.setupProgress)
        setupStatus = findViewById(R.id.setupStatus)
        doneButton = findViewById(R.id.doneButton)
        retryButton = findViewById(R.id.retryButton)

        findViewById<MaterialButton>(R.id.step0Next).setOnClickListener {
            if (Build.VERSION.SDK_INT >= 33 &&
                checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) {
                requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQUEST_NOTIFY)
            } else {
                beginPairing()
            }
        }
        findViewById<MaterialButton>(R.id.step0Manual).setOnClickListener {
            showStep(1)
        }

        pairButton.setOnClickListener {
            startPairing()
        }

        doneButton.setOnClickListener {
            setResult(RESULT_OK)
            finish()
        }

        retryButton.setOnClickListener {
            showStep(0)
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQUEST_NOTIFY) {
            if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) beginPairing() else showStep(1)
        }
    }

    override fun onResume() {
        super.onResume()
        if (startAfterOverlayPrompt) {
            startAfterOverlayPrompt = false
            startNotificationPairing()
            return
        }
        if (Priv.service != null && step2.visibility != View.VISIBLE) {
            showStep(2)
            setupProgress.visibility = View.GONE
            setupStatus.text = getString(R.string.setup_status_done)
            doneButton.visibility = View.VISIBLE
            retryButton.visibility = View.GONE
        }
    }

    private var startAfterOverlayPrompt = false

    /**
     * The floating number pad needs "Display over other apps"; without it the code is typed
     * into the notification instead, so the user may skip this.
     */
    private fun beginPairing() {
        if (Settings.canDrawOverlays(this)) {
            startNotificationPairing()
            return
        }
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.overlay_title)
            .setMessage(R.string.overlay_body)
            .setNegativeButton(R.string.overlay_skip) { _, _ -> startNotificationPairing() }
            .setPositiveButton(R.string.overlay_allow) { _, _ ->
                startAfterOverlayPrompt = true
                startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:$packageName")))
            }
            .show()
    }

    /** Pairing code goes into a notification; Settings opens on the Wireless debugging page. */
    private fun startNotificationPairing() {
        PairingService.start(this)
        val wireless = Intent().setClassName("com.android.settings", "com.android.settings.SubSettings")
            .putExtra(":settings:show_fragment", "com.android.settings.development.WirelessDebuggingFragment")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            startActivity(wireless)
        } catch (_: Exception) {
            startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }

    override fun onStart() {
        super.onStart()
        // Start mDNS discovery when on pairing step
        if (step1.visibility == View.VISIBLE) {
            startMdnsDiscovery()
        }
    }

    override fun onStop() {
        super.onStop()
        pairingMdns?.stop()
        pairingMdns = null
    }

    private fun showStep(n: Int) {
        step0.visibility = if (n == 0) View.VISIBLE else View.GONE
        step1.visibility = if (n == 1) View.VISIBLE else View.GONE
        step2.visibility = if (n == 2) View.VISIBLE else View.GONE

        if (n == 1) {
            startMdnsDiscovery()
        }
    }

    private fun startMdnsDiscovery() {
        pairingMdns?.stop()
        mdnsStatus.setText(R.string.setup_scanning)
        portField.text?.clear()

        val mdns = AdbMdns(this, AdbMdns.TLS_PAIRING) { host, port ->
            if (port != -1) {
                discoveredHost = host
                runOnUiThread {
                    portField.setText(port.toString())
                    mdnsStatus.text = getString(R.string.setup_port_detected, port)
                }
            }
        }
        mdns.start()
        pairingMdns = mdns
    }

    private fun startPairing() {
        // Accepts "port" or "IP:port" as shown on the pairing screen.
        val portStr = portField.text?.toString()?.trim() ?: ""
        val port = portStr.substringAfterLast(':').toIntOrNull()
        if (portStr.contains(':')) portStr.substringBeforeLast(':').takeIf { it.isNotBlank() }?.let { discoveredHost = it }
        if (port == null || port <= 0 || port > 65535) {
            mdnsStatus.text = getString(R.string.setup_enter_port)
            return
        }

        val code = codeField.text?.toString()?.trim() ?: ""
        if (code.isEmpty()) {
            codeField.error = getString(R.string.setup_enter_code)
            return
        }

        pairingMdns?.stop()
        pairingMdns = null

        showStep(2)
        setupProgress.visibility = View.VISIBLE
        doneButton.visibility = View.GONE
        retryButton.visibility = View.GONE

        val host = discoveredHost

        Thread {
            try {
                updateStatus(getString(R.string.setup_status_pairing))
                EmbeddedServer.pair(this, host, port, code)

                updateStatus(getString(R.string.setup_status_connecting))
                EmbeddedServer.connectAndStart(this, host)

                runOnUiThread {
                    setupProgress.visibility = View.GONE
                    setupStatus.text = getString(R.string.setup_status_done)
                    doneButton.visibility = View.VISIBLE
                }
            } catch (e: Exception) {
                runOnUiThread {
                    setupProgress.visibility = View.GONE
                    setupStatus.text = getString(R.string.setup_status_error, e.message ?: e.javaClass.simpleName)
                    retryButton.visibility = View.VISIBLE
                }
            }
        }.start()
    }

    private fun updateStatus(text: String) {
        runOnUiThread { setupStatus.text = text }
    }

    private companion object {
        const val REQUEST_NOTIFY = 1
    }
}
