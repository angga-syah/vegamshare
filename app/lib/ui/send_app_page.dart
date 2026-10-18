import 'package:flutter/material.dart';

import '../platform/platform_services.dart';
import '../src/rust/api/vegam.dart' as core;
import '../util/l10n.dart';
import 'common.dart';
import 'home_shell.dart';

/// Shows a list of installed apps; user picks one, app is staged and sent.
class SendAppPage extends StatefulWidget {
  const SendAppPage({super.key, required this.device});
  final core.Device device;

  @override
  State<SendAppPage> createState() => _SendAppPageState();
}

class _SendAppPageState extends State<SendAppPage> with WidgetsBindingObserver {
  List<InstalledAppInfo> _apps = [];
  List<InstalledAppInfo> _filtered = [];
  bool _loading = true;
  bool _shizukuOk = false;
  String _query = '';
  String? _stagingPkg;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _init();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  // Back from the setup screen or Settings: the helper may be running now.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) return;
    shizukuRunning().then((ok) {
      if (mounted && ok != _shizukuOk) setState(() => _shizukuOk = ok);
    });
  }

  Future<void> _init() async {
    final running = await shizukuRunning();
    final apps = await listUserApps();
    if (mounted) {
      setState(() {
        _shizukuOk = running;
        _apps = apps;
        _filtered = apps;
        _loading = false;
      });
    }
  }

  void _filter(String q) {
    _query = q.toLowerCase();
    setState(() {
      _filtered = _apps.where((a) =>
          a.label.toLowerCase().contains(_query) ||
          a.packageName.toLowerCase().contains(_query)).toList();
    });
  }

  Future<void> _send(InstalledAppInfo app) async {
    setState(() => _stagingPkg = app.packageName);
    var cancelled = false;
    BuildContext? dialogContext;
    showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (dctx) {
        dialogContext = dctx;
        return AlertDialog(
          title: Text(tr('send_app_preparing', {'name': app.label})),
          content: Column(mainAxisSize: MainAxisSize.min, children: [
            Text(tr('send_app_preparing_body')),
            const SizedBox(height: 16),
            const LinearProgressIndicator(),
          ]),
          actions: [
            TextButton(
              onPressed: () {
                cancelled = true;
                Navigator.pop(dctx);
              },
              child: Text(tr('cancel')),
            ),
          ],
        );
      },
    );
    try {
      // One folder (apk/, data.tar, obb/); data is read through the helper while sending.
      final bundle = await prepareAppBundle(app.packageName);
      if (cancelled) return;
      await core.sendBundle(fingerprint: widget.device.fingerprint, root: bundle.root, files: bundle.files);
      if (!mounted) return;
      showSnack(context, tr('send_app_queued', {'name': app.label, 'device': widget.device.displayName}));
      // Progress, pause and stop live in the Transfer tab.
      Navigator.of(context).popUntil((r) => r.isFirst);
      HomeShell.tabRequest.value = HomeShell.transfersTab;
    } catch (e) {
      if (!mounted || cancelled) return;
      final d = dialogContext;
      if (d != null && d.mounted) Navigator.pop(d);
      showSnack(context, tr('send_app_failed', {'error': e.toString()}));
    } finally {
      if (mounted) setState(() => _stagingPkg = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(tr('send_app_title')),
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(56),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
            child: SearchBar(
              hintText: tr('send_app_search'),
              onChanged: _filter,
              leading: const Icon(Icons.search),
            ),
          ),
        ),
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : Column(
              children: [
                if (!_shizukuOk)
                  MaterialBanner(
                    padding: const EdgeInsets.fromLTRB(16, 8, 8, 8),
                    content: Text(tr('shizuku_needed')),
                    actions: [
                      TextButton(
                        onPressed: openShizukuSetup,
                        child: Text(tr('shizuku_setup')),
                      ),
                    ],
                  ),
                Expanded(
                  child: _filtered.isEmpty
                      ? Center(child: Text(tr('no_apps')))
                      : ListView.builder(
                          itemCount: _filtered.length,
                          itemBuilder: (ctx, i) {
                            final app = _filtered[i];
                            final sending = _stagingPkg == app.packageName;
                            return ListTile(
                              leading: const Icon(Icons.android),
                              title: Text(app.label),
                              subtitle: Text(app.packageName, style: const TextStyle(fontSize: 12)),
                              trailing: sending
                                  ? const SizedBox(width: 24, height: 24, child: CircularProgressIndicator(strokeWidth: 2))
                                  : const Icon(Icons.send_outlined),
                              onTap: (_stagingPkg != null) ? null : () => _send(app),
                            );
                          },
                        ),
                ),
              ],
            ),
    );
  }
}
