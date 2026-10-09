import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';

import '../main.dart' show appState;
import '../platform/platform_services.dart';
import '../src/rust/api/vegam.dart' as core;
import '../util/l10n.dart';
import 'common.dart';

class SettingsPage extends StatefulWidget {
  const SettingsPage({super.key});

  @override
  State<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends State<SettingsPage> {
  bool _startup = false;
  String _version = '';

  @override
  void initState() {
    super.initState();
    startupEnabled().then((v) => mounted ? setState(() => _startup = v) : null);
    PackageInfo.fromPlatform().then((p) => mounted ? setState(() => _version = '${p.version}+${p.buildNumber}') : null);
  }

  Future<String?> _ask(String title, String initial, {String? helper, bool obscure = false, int maxLength = 64}) {
    final c = TextEditingController(text: initial);
    return showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(title),
        content: TextField(
          controller: c,
          autofocus: true,
          maxLength: maxLength,
          obscureText: obscure,
          decoration: InputDecoration(helperText: helper),
          onSubmitted: (v) => Navigator.pop(ctx, v),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: Text(tr('cancel'))),
          FilledButton(onPressed: () => Navigator.pop(ctx, c.text), child: Text(tr('save'))),
        ],
      ),
    );
  }

  Future<void> _guard(Future<void> Function() f) async {
    try {
      await f();
      appState.refreshInfo();
    } catch (e) {
      if (mounted) showSnack(context, describeError(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(tr('tab_settings'))),
      body: ListenableBuilder(
        listenable: appState,
        builder: (context, _) {
          final info = appState.info!;
          final scheme = Theme.of(context).colorScheme;
          return PageBody(children: [
            Card(
              child: Column(children: [
                ListTile(
                  leading: const Icon(Icons.badge_outlined),
                  title: Text(tr('device_name')),
                  subtitle: Text(info.name),
                  onTap: () async {
                    final v = await _ask(tr('device_name'), info.name);
                    if (v != null && v.trim().isNotEmpty) await _guard(() => core.setDeviceName(name: v.trim()));
                  },
                ),
                ListTile(
                  leading: const Icon(Icons.key),
                  title: Text(tr('change_password')),
                  subtitle: Text(tr('password_rules')),
                  onTap: () async {
                    final v = await _ask(tr('change_password'), '', helper: tr('password_rules'));
                    if (v != null) await _guard(() => core.setPassword(password: v));
                  },
                ),
                ListTile(
                  leading: const Icon(Icons.folder_outlined),
                  title: Text(tr('download_folder')),
                  subtitle: Text(info.downloadDir),
                  trailing: Row(mainAxisSize: MainAxisSize.min, children: [
                    IconButton(tooltip: tr('open_folder'), icon: const Icon(Icons.open_in_new), onPressed: () => revealPath(info.downloadDir)),
                    TextButton(
                      onPressed: () async {
                        final d = await FilePicker.platform.getDirectoryPath(initialDirectory: info.downloadDir);
                        if (d != null) await _guard(() => core.setDownloadDir(dir: d));
                      },
                      child: Text(tr('choose')),
                    ),
                  ]),
                ),
              ]),
            ),
            const SizedBox(height: 8),
            Card(
              child: Column(children: [
                ListTile(
                  leading: const Icon(Icons.translate),
                  title: Text(tr('language')),
                  trailing: SegmentedButton<Lang>(
                    segments: const [ButtonSegment(value: Lang.id, label: Text('ID')), ButtonSegment(value: Lang.en, label: Text('EN'))],
                    selected: {currentLang},
                    onSelectionChanged: (s) => appState.setLang(s.first),
                  ),
                ),
                ListTile(
                  leading: const Icon(Icons.palette_outlined),
                  title: Text(tr('theme')),
                  trailing: DropdownButton<ThemeMode>(
                    value: appState.themeMode,
                    underline: const SizedBox.shrink(),
                    onChanged: (m) => appState.setTheme(m!),
                    items: [
                      DropdownMenuItem(value: ThemeMode.system, child: Text(tr('theme_system'))),
                      DropdownMenuItem(value: ThemeMode.light, child: Text(tr('theme_light'))),
                      DropdownMenuItem(value: ThemeMode.dark, child: Text(tr('theme_dark'))),
                    ],
                  ),
                ),
                if (Platform.isWindows)
                  SwitchListTile(
                    secondary: const Icon(Icons.power_settings_new),
                    title: Text(tr('start_with_windows')),
                    value: _startup,
                    onChanged: (v) async {
                      await setStartup(v);
                      final now = await startupEnabled();
                      if (mounted) setState(() => _startup = now);
                    },
                  ),
              ]),
            ),
            const SizedBox(height: 8),
            Card(
              child: Column(children: [
                ListTile(
                  leading: const Icon(Icons.fingerprint),
                  title: Text(tr('fingerprint')),
                  subtitle: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    const SizedBox(height: 4),
                    SelectableText(info.fingerprint, style: const TextStyle(fontFamily: 'monospace')),
                    const SizedBox(height: 4),
                    Text(tr('fingerprint_sub'), style: TextStyle(color: scheme.onSurfaceVariant)),
                  ]),
                ),
                ListTile(
                  leading: const Icon(Icons.lan_outlined),
                  title: Text(tr('addresses')),
                  subtitle: SelectableText(info.addresses.isEmpty ? '-' : info.addresses.join('\n')),
                ),
                ListTile(
                  leading: const Icon(Icons.info_outline),
                  title: Text(tr('about')),
                  subtitle: Text('VegamShare $_version · MIT · ${tr('licenses')}'),
                  onTap: () => showLicensePage(context: context, applicationName: 'VegamShare', applicationVersion: _version),
                ),
              ]),
            ),
          ]);
        },
      ),
    );
  }
}
