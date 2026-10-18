package com.vegamshare.vegamshare

import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.Process
import android.text.format.Formatter
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView
import android.widget.Toast
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.edit
import androidx.core.net.toUri
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.button.MaterialButton
import com.google.android.material.card.MaterialCardView
import com.google.android.material.color.DynamicColors
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.progressindicator.LinearProgressIndicator
import java.util.concurrent.Executors

class CloneActivity : AppCompatActivity() {

    /** Privileged work runs here, one task at a time. */
    private val work = Executors.newSingleThreadExecutor()
    /** Size measuring can take seconds per game; keep it off the work queue. */
    private val measuring = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private val prefs by lazy { getSharedPreferences("clone", MODE_PRIVATE) }

    private lateinit var statusCard: MaterialCardView
    private lateinit var statusTitle: TextView
    private lateinit var statusBody: TextView
    private lateinit var statusAction: MaterialButton
    private lateinit var targetRow: View
    private lateinit var targetButton: MaterialButton
    private lateinit var message: TextView
    private lateinit var list: RecyclerView
    private val adapter = AppAdapter { onAppTapped(it) }

    /** The user this app runs in: the source of every copy. */
    private val me = Process.myUid() / PER_USER_RANGE
    private var targets: List<Profile> = emptyList()
    private var target: Profile? = null
    private var inTarget: Set<String> = emptySet()
    private var loaded = false
    private var loadGeneration = 0
    /** An app whose clone the user was sent to open once; continue on return. */
    private var pendingCopy: AppItem? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        DynamicColors.applyToActivityIfAvailable(this)
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_clone)
        val root = findViewById<View>(R.id.root)
        ViewCompat.setOnApplyWindowInsetsListener(root) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            v.setPadding(bars.left, bars.top, bars.right, 0)
            list.setPadding(0, 0, 0, bars.bottom)
            insets
        }

        statusCard = findViewById(R.id.statusCard)
        statusTitle = findViewById(R.id.statusTitle)
        statusBody = findViewById(R.id.statusBody)
        statusAction = findViewById(R.id.statusAction)
        targetRow = findViewById(R.id.targetRow)
        targetButton = findViewById(R.id.targetButton)
        message = findViewById(R.id.message)
        list = findViewById(R.id.list)
        list.layoutManager = LinearLayoutManager(this)
        list.adapter = adapter
        targetButton.setOnClickListener { chooseTarget() }

        findViewById<MaterialToolbar>(R.id.toolbar).apply {
            inflateMenu(R.menu.clone)
            setNavigationIcon(androidx.appcompat.R.drawable.abc_ic_ab_back_material)
            setNavigationOnClickListener { finish() }
            setOnMenuItemClickListener {
                when (it.itemId) {
                    R.id.action_refresh -> { loaded = false; render() }
                    R.id.action_choosers -> fixChoosers()
                }
                true
            }
        }

        Priv.onChange = { render() }
    }

    override fun onResume() {
        super.onResume()
        // Coming back from Settings or the clone: profiles or apps may have changed.
        loaded = false
        render()
        pendingCopy?.let { app ->
            pendingCopy = null
            if (Priv.service != null) startCopy(app, confirm = false)
        }
    }

    override fun onDestroy() {
        Priv.onChange = null
        work.shutdownNow()
        measuring.shutdownNow()
        super.onDestroy()
    }

    // ---- Server status -------------------------------------------------------

    private fun render() {
        val state = Priv.state()
        val ready = state == Priv.State.READY
        statusCard.visibility = if (ready) View.GONE else View.VISIBLE
        statusAction.visibility = View.VISIBLE
        when (state) {
            Priv.State.NOT_RUNNING -> status(R.string.sz_stopped_title, R.string.sz_stopped_body, R.string.sz_open) { openSetup() }
            Priv.State.CONNECTING -> {
                statusTitle.setText(R.string.sz_connecting)
                statusBody.text = ""
                statusAction.visibility = View.GONE
            }
            Priv.State.READY -> {}
        }
        if (!ready) {
            targetRow.visibility = View.GONE
            list.visibility = View.GONE
            message.visibility = View.GONE
            loaded = false
        } else if (!loaded) {
            loaded = true
            reload()
        }
    }

    private fun status(title: Int, body: Int, action: Int, onClick: () -> Unit) {
        statusTitle.setText(title)
        statusBody.setText(body)
        statusAction.setText(action)
        statusAction.setOnClickListener { onClick() }
    }

    private fun openSetup() {
        startActivity(Intent(this, EmbeddedSetupActivity::class.java))
    }

    // ---- loading -------------------------------------------------------------

    private fun reload() {
        val generation = ++loadGeneration
        background {
            val profiles = Profile.list().filter { it.id != me }
            val saved = prefs.getInt(KEY_TARGET, -1)
            val chosen = profiles.find { it.id == saved }
                ?: profiles.find { it.kind == Profile.Kind.DUAL_APPS }
                ?: profiles.find { it.kind != Profile.Kind.OTHER_USER }
                ?: profiles.firstOrNull()
            val installed = chosen?.let { Priv.packagesOf(it.id) } ?: emptySet()
            val withData = Priv.appFolders(me, "data") + Priv.appFolders(me, "obb")
            val apps = loadApps(withData)
            main.post {
                if (generation != loadGeneration || isFinishing) return@post
                targets = profiles
                target = chosen
                inTarget = installed
                showTarget()
                adapter.submit(apps, installed)
                list.visibility = if (chosen != null && apps.isNotEmpty()) View.VISIBLE else View.GONE
                message.visibility = if (chosen == null || apps.isEmpty()) View.VISIBLE else View.GONE
                message.text = when {
                    chosen == null -> getString(R.string.no_target) + "\n\n" + Profile.setupHint(this)
                    else -> getString(R.string.games_none)
                }
                measureSizes(apps, generation)
            }
        }
    }

    private fun loadApps(withData: Set<String>): List<AppItem> {
        val pm = packageManager
        return pm.getInstalledApplications(0)
            .asSequence()
            .filter { it.packageName in withData }
            .filter { (it.flags and ApplicationInfo.FLAG_SYSTEM) == 0 || (it.flags and ApplicationInfo.FLAG_UPDATED_SYSTEM_APP) != 0 }
            .filter { it.packageName != packageName && it.packageName != Priv.SHIZUKU_PACKAGE }
            .map {
                AppItem(
                    pkg = it.packageName,
                    label = it.loadLabel(pm).toString(),
                    icon = it.loadIcon(pm),
                    isGame = it.category == ApplicationInfo.CATEGORY_GAME,
                    appId = it.uid % PER_USER_RANGE,
                )
            }
            .sortedWith(compareBy<AppItem> { !it.isGame }.thenBy { it.label.lowercase() })
            .toList()
    }

    private fun measureSizes(apps: List<AppItem>, generation: Int) {
        // Always measured again: a game may have downloaded more since.
        for (app in apps) {
            measuring.execute {
                if (generation != loadGeneration) return@execute
                val bytes = try { Priv.service?.measure(app.pkg, me) } catch (_: Exception) { null } ?: return@execute
                main.post {
                    if (generation != loadGeneration) return@post
                    adapter.setSize(app.pkg, bytes)
                    if (adapter.itemCount == 0) {
                        list.visibility = View.GONE
                        message.visibility = View.VISIBLE
                        message.setText(R.string.games_none)
                    }
                }
            }
        }
    }

    private fun showTarget() {
        val t = target
        targetRow.visibility = if (t == null) View.GONE else View.VISIBLE
        if (t != null) {
            targetButton.text = if (t.running) t.label(this) else getString(R.string.profile_not_running, t.label(this))
        }
    }

    private fun chooseTarget() {
        if (targets.size < 2) return
        val labels = targets.map { if (it.running) it.label(this) else getString(R.string.profile_not_running, it.label(this)) }
        val current = targets.indexOfFirst { it.id == target?.id }
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.choose_target)
            .setSingleChoiceItems(labels.toTypedArray(), current) { d, which ->
                d.dismiss()
                prefs.edit { putInt(KEY_TARGET, targets[which].id) }
                loaded = false
                render()
            }
            .show()
    }

    // ---- per-app actions -----------------------------------------------------

    private fun onAppTapped(app: AppItem) {
        val t = target ?: return
        val cloned = app.pkg in inTarget
        val here = getString(R.string.here, adapter.sizeOf(app.pkg)?.let { size(it) } ?: getString(R.string.size_measuring))
        val there = if (cloned) getString(R.string.there_measuring, t.label(this)) else getString(R.string.there_not_cloned, t.label(this))
        fun text(thereLine: String) = "$here\n$thereLine\n\n${getString(R.string.data_note)}"

        val builder = MaterialAlertDialogBuilder(this)
            .setTitle(app.label)
            .setIcon(app.icon)
            .setMessage(text(there))
            .setNeutralButton(R.string.close, null)
        if (!cloned) {
            builder.setPositiveButton(R.string.act_clone) { _, _ -> clone(app, t) }
        } else {
            builder.setPositiveButton(R.string.act_copy) { _, _ -> startCopy(app, confirm = true) }
            builder.setNegativeButton(R.string.act_remove) { _, _ -> confirmRemove(app, t) }
        }
        val dialog = builder.show()
        if (cloned) {
            measuring.execute {
                val bytes = try { Priv.service?.measure(app.pkg, t.id) } catch (_: Exception) { null } ?: return@execute
                main.post {
                    if (!dialog.isShowing) return@post
                    dialog.setMessage(text(getString(R.string.there_data, t.label(this), size(bytes))))
                    // An existing copy only needs the changed files.
                    if (bytes > 0) dialog.getButton(AlertDialog.BUTTON_POSITIVE)?.setText(R.string.act_sync)
                }
            }
        }
    }

    private fun clone(app: AppItem, t: Profile) {
        background {
            val r = Priv.sh("cmd", "package", "install-existing", "--user", t.id.toString(), app.pkg)
            val ok = r.ok && Priv.packagesOf(t.id).contains(app.pkg)
            main.post {
                if (!ok) {
                    alert(getString(R.string.clone_failed, r.out.trim().ifEmpty { "exit ${r.code}" }))
                    return@post
                }
                inTarget = inTarget + app.pkg
                adapter.setCloned(inTarget)
                MaterialAlertDialogBuilder(this)
                    .setTitle(getString(R.string.cloned_ok, app.label, t.label(this)))
                    .setMessage(R.string.copy_now)
                    .setPositiveButton(R.string.act_copy) { _, _ -> startCopy(app, confirm = false) }
                    .setNegativeButton(R.string.close, null)
                    .show()
            }
        }
    }

    private fun confirmRemove(app: AppItem, t: Profile) {
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.act_remove)
            .setMessage(getString(R.string.remove_confirm, app.label, t.label(this)))
            .setNegativeButton(R.string.cancel, null)
            .setPositiveButton(R.string.act_remove) { _, _ ->
                background {
                    val r = Priv.sh("pm", "uninstall", "--user", t.id.toString(), app.pkg)
                    main.post {
                        if (r.ok) {
                            inTarget = inTarget - app.pkg
                            adapter.setCloned(inTarget)
                            toast(getString(R.string.removed_ok, t.label(this)))
                        } else {
                            alert(getString(R.string.error, r.out.trim()))
                        }
                    }
                }
            }
            .show()
    }

    private fun startCopy(app: AppItem, confirm: Boolean) {
        val t = target ?: return
        if (confirm) {
            val amount = adapter.sizeOf(app.pkg)?.let { size(it) } ?: "?"
            MaterialAlertDialogBuilder(this)
                .setTitle(getString(R.string.copy_title, app.label))
                .setMessage(getString(R.string.copy_confirm, app.label, t.label(this), amount))
                .setNegativeButton(R.string.cancel, null)
                .setPositiveButton(R.string.copy_start) { _, _ -> startCopy(app, confirm = false) }
                .show()
            return
        }
        background {
            // The clone must not write while its files are replaced.
            Priv.sh("am", "force-stop", "--user", t.id.toString(), app.pkg)
            val prepared = Priv.service!!.prepareTarget(app.pkg, t.id, app.appId)
            main.post {
                if (prepared == 2) askToOpenClone(app, t) else runCopy(app, t)
            }
        }
    }

    private fun askToOpenClone(app: AppItem, t: Profile) {
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.need_open_title)
            .setMessage(getString(R.string.need_open_body, app.label, t.label(this)))
            .setNegativeButton(R.string.cancel, null)
            .setNeutralButton(R.string.opened_continue) { _, _ -> startCopy(app, confirm = false) }
            .setPositiveButton(R.string.open_now) { _, _ ->
                background {
                    val launched = launchIn(app.pkg, t.id)
                    main.post {
                        if (launched) pendingCopy = app
                        else alert(getString(R.string.open_manually, app.label, t.label(this)))
                    }
                }
            }
            .show()
    }

    /** Starts the app's launcher activity in another user or profile. */
    private fun launchIn(pkg: String, user: Int): Boolean {
        val r = Priv.sh(
            "cmd", "package", "resolve-activity", "--brief", "--user", user.toString(),
            "-a", Intent.ACTION_MAIN, "-c", Intent.CATEGORY_LAUNCHER, pkg,
        )
        val component = r.out.lineSequence().map { it.trim() }.lastOrNull { it.startsWith("$pkg/") } ?: return false
        val start = Priv.sh("am", "start", "--user", user.toString(), "-n", component)
        return start.ok && !start.out.contains("Error", ignoreCase = true)
    }

    private fun runCopy(app: AppItem, t: Profile) {
        val view = layoutInflater.inflate(R.layout.dialog_progress, FrameLayout(this), false)
        val bar = view.findViewById<LinearProgressIndicator>(R.id.bar)
        val amount = view.findViewById<TextView>(R.id.amount)
        val current = view.findViewById<TextView>(R.id.current)
        amount.setText(R.string.copying)
        val dialog = MaterialAlertDialogBuilder(this)
            .setTitle(getString(R.string.copy_title, app.label))
            .setView(view)
            .setCancelable(false)
            .setNegativeButton(R.string.cancel, null)
            .show()
        dialog.getButton(AlertDialog.BUTTON_NEGATIVE).setOnClickListener {
            it.isEnabled = false
            Thread { try { Priv.service?.cancel() } catch (_: Exception) {} }.start()
        }

        val progress = object : IProgress.Stub() {
            override fun onProgress(doneBytes: Long, totalBytes: Long, name: String?) {
                main.post {
                    if (totalBytes > 0) {
                        if (bar.isIndeterminate) {
                            bar.isIndeterminate = false
                            bar.max = 1000
                        }
                        bar.setProgressCompat((doneBytes * 1000 / totalBytes).toInt(), true)
                    }
                    amount.text = getString(R.string.progress_amount, size(doneBytes), size(totalBytes))
                    current.text = name.orEmpty()
                }
            }
        }

        background {
            val result = try {
                Priv.service!!.copyData(app.pkg, me, t.id, progress)
            } catch (e: Exception) {
                "ERR|${e.message}"
            }
            main.post {
                dialog.dismiss()
                showCopyResult(result)
            }
        }
    }

    private fun showCopyResult(result: String) {
        val p = result.split('|')
        val text = when (p[0]) {
            "OK" -> {
                val base = getString(R.string.copy_done, files(p[1].toInt()), size(p[2].toLong()), files(p[3].toInt()))
                val errors = p[4].toInt()
                if (errors > 0) base + "\n\n" + getString(R.string.copy_done_errors, files(errors), p.getOrElse(5) { "" }) else base
            }
            "CANCELLED" -> getString(R.string.copy_cancelled)
            else -> if (p.getOrNull(1) == "no_target") getString(R.string.copy_no_target)
            else getString(R.string.copy_failed, p.drop(1).joinToString("|"))
        }
        MaterialAlertDialogBuilder(this)
            .setTitle(if (p[0] == "OK") getString(R.string.copy_done_title) else getString(R.string.clone_title))
            .setMessage(text)
            .setPositiveButton(R.string.close, null)
            .show()
    }

    // ---- "Always" in Open with ---------------------------------------------

    /**
     * Apps in the other space that also open links or files make Android ask
     * which space to use, and such a choice cannot be remembered.
     */
    private fun fixChoosers() {
        val t = target ?: return
        if (Priv.service == null) return
        background {
            val queries = listOf(
                arrayOf("-d", "https://example.com"),
                arrayOf("-t", "image/jpeg"),
                arrayOf("-t", "video/mp4"),
                arrayOf("-t", "audio/mpeg"),
                arrayOf("-t", "application/pdf"),
                arrayOf("-t", "text/plain"),
            )
            val found = sortedSetOf<String>()
            for (q in queries) {
                val r = Priv.sh("cmd", "package", "query-activities", "--brief", "--user", t.id.toString(), "-a", Intent.ACTION_VIEW, *q)
                r.out.lineSequence().map { it.trim() }.filter { '/' in it && ' ' !in it }.forEach { found += it.substringBefore('/') }
            }
            val removed = prefs.getStringSet(removedKey(t), emptySet())!!.toSet()
            main.post { showChoosers(t, found.toList(), removed) }
        }
    }

    private fun showChoosers(t: Profile, apps: List<String>, removed: Set<String>) {
        val labels = apps.map { labelOf(it) }.toTypedArray()
        // Preselect what the system put there (a browser, a gallery); apps the
        // user installed and cloned on purpose stay unticked.
        val checked = BooleanArray(apps.size) { isSystemApp(apps[it]) }
        val b = MaterialAlertDialogBuilder(this).setTitle(R.string.choosers_title)
        if (apps.isEmpty()) {
            b.setMessage(getString(R.string.choosers_none, t.label(this)))
            b.setPositiveButton(R.string.close, null)
        } else {
            b.setMultiChoiceItems(labels, checked) { _, which, isChecked -> checked[which] = isChecked }
            b.setPositiveButton(getString(R.string.choosers_remove, t.label(this))) { _, _ ->
                val chosen = apps.filterIndexed { i, _ -> checked[i] }
                background {
                    val done = chosen.filter { Priv.sh("pm", "uninstall", "-k", "--user", t.id.toString(), it).ok }
                    prefs.edit { putStringSet(removedKey(t), removed + done) }
                    main.post { toast(getString(R.string.choosers_done)) }
                }
            }
            b.setNegativeButton(R.string.cancel, null)
        }
        if (removed.isNotEmpty()) {
            b.setNeutralButton(R.string.choosers_restore) { _, _ ->
                background {
                    val back = removed.filter { Priv.sh("cmd", "package", "install-existing", "--user", t.id.toString(), it).ok }
                    prefs.edit { putStringSet(removedKey(t), removed - back.toSet()) }
                    main.post { loaded = false; render() }
                }
            }
        }
        // A list dialog has no room for a message: explain it in the title.
        if (apps.isNotEmpty()) {
            val pad = (24 * resources.displayMetrics.density).toInt()
            val title = layoutInflater.inflate(R.layout.dialog_title, FrameLayout(this), false)
            title.setPadding(pad, pad, pad, pad / 2)
            title.findViewById<TextView>(R.id.dialogTitle).setText(R.string.choosers_title)
            title.findViewById<TextView>(R.id.dialogBody).text = getString(R.string.choosers_body, t.label(this))
            b.setCustomTitle(title)
        }
        b.show()
    }

    private fun removedKey(t: Profile) = "removed_from_${t.id}"

    private fun isSystemApp(pkg: String) = try {
        (packageManager.getApplicationInfo(pkg, 0).flags and ApplicationInfo.FLAG_SYSTEM) != 0
    } catch (_: PackageManager.NameNotFoundException) {
        true
    }

    private fun labelOf(pkg: String) = try {
        packageManager.getApplicationInfo(pkg, 0).loadLabel(packageManager).toString() + "  ($pkg)"
    } catch (_: PackageManager.NameNotFoundException) {
        pkg
    }

    // ---- misc ----------------------------------------------------------------

    private fun background(task: () -> Unit) {
        work.execute {
            try {
                task()
            } catch (e: Exception) {
                main.post { if (!isFinishing) alert(getString(R.string.error, e.message ?: e.javaClass.simpleName)) }
            }
        }
    }

    private fun alert(text: String) {
        MaterialAlertDialogBuilder(this).setMessage(text).setPositiveButton(R.string.close, null).show()
    }

    private fun toast(text: String, long: Boolean = false) {
        Toast.makeText(this, text, if (long) Toast.LENGTH_LONG else Toast.LENGTH_SHORT).show()
    }

    private fun size(bytes: Long) = Formatter.formatShortFileSize(this, bytes)

    private fun files(n: Int) = resources.getQuantityString(R.plurals.n_files, n, n)

    private companion object {
        const val PER_USER_RANGE = 100000
        const val KEY_TARGET = "target_user"
    }
}
