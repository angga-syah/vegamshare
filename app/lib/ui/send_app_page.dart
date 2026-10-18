import 'dart:io';

import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';

import '../platform/platform_services.dart';
import '../src/rust/api/vegam.dart' as core;
import '../util/l10n.dart';
import 'common.dart';

/// Shows a list of installed apps; user picks one, app is staged and sent.
class SendAppPage extends StatefulWidget {
  const SendAppPage({super.key, required this.device});
  final core.Device device;

  @override
  State<SendAppPage> createState() => _SendAppPageState();
}

class _SendAppPageState extends State<SendAppPage> {
  List<InstalledAppInfo> _apps = [];
  List<InstalledAppInfo> _filtered = [];
  bool _loading = true;
  bool _shizukuOk = false;
  String _query = '';
  String? _stagingPkg;

  @override
  void initState() {
    super.initState();
    _init();
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
    try {
      final cache = await getTemporaryDirectory();
      final stagingDir = Directory('${cache.path}/app_send/${app.packageName}_${DateTime.now().millisecondsSinceEpoch}');
      await stagingDir.create(recursive: true);

      final staged = await stageAppFiles(app.packageName, stagingDir.path);
      // Send stagingDir (has app_info.json + data/) plus APK+OBB directly
      await core.sendPaths(
        fingerprint: widget.device.fingerprint,
        paths: [staged.stagingDir, ...staged.directPaths],
      );

      if (mounted) {
        showSnack(context, tr('app_send_queued', {'name': app.label, 'device': widget.device.displayName}));
        Navigator.of(context).pop();
      }
    } catch (e) {
      if (mounted) {
        setState(() => _stagingPkg = null);
        showSnack(context, tr('app_send_failed', {'error': e.toString()}));
      }
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
