import 'dart:io';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';

import '../main.dart' show appState;
import '../src/rust/api/vegam.dart' as core;
import '../util/format.dart';
import '../util/l10n.dart';
import 'common.dart';
import 'history_page.dart';

class DevicePage extends StatefulWidget {
  const DevicePage({super.key, required this.fingerprint});
  final String fingerprint;

  @override
  State<DevicePage> createState() => _DevicePageState();
}

class _DevicePageState extends State<DevicePage> {
  bool _dragging = false;
  bool _checking = false;

  Future<void> _rename(core.Device d) async {
    final c = TextEditingController(text: d.displayName);
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(tr('rename')),
        content: TextField(controller: c, autofocus: true, maxLength: 64, onSubmitted: (v) => Navigator.pop(ctx, v)),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: Text(tr('cancel'))),
          FilledButton(onPressed: () => Navigator.pop(ctx, c.text), child: Text(tr('save'))),
        ],
      ),
    );
    if (name == null) return;
    await core.renameDevice(fingerprint: d.fingerprint, alias: name.trim() == d.name ? null : name);
  }

  Future<void> _forget(core.Device d) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        icon: const Icon(Icons.link_off),
        title: Text(tr('forget')),
        content: Text(tr('forget_confirm', {'name': d.displayName})),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: Text(tr('cancel'))),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: Theme.of(ctx).colorScheme.error),
            onPressed: () => Navigator.pop(ctx, true),
            child: Text(tr('remove')),
          ),
        ],
      ),
    );
    if (ok != true) return;
    await core.forgetDevice(fingerprint: d.fingerprint);
    if (mounted) Navigator.pop(context);
  }

  Future<void> _check(core.Device d) async {
    setState(() => _checking = true);
    try {
      await core.ping(fingerprint: d.fingerprint);
      if (mounted) showSnack(context, tr('reachable'));
    } catch (e) {
      if (mounted) showSnack(context, describeError(e));
    } finally {
      if (mounted) setState(() => _checking = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: appState,
      builder: (context, _) {
        final d = appState.deviceByFingerprint(widget.fingerprint);
        if (d == null) return const Scaffold();
        final scheme = Theme.of(context).colorScheme;
        final history = appState.history.where((h) => h.peer == d.fingerprint).take(30).toList();

        final sendArea = Card(
          color: _dragging ? scheme.secondaryContainer : null,
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Column(children: [
              Row(children: [
                Expanded(
                  child: FilledButton.icon(
                    style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(56)),
                    onPressed: () => pickAndSend(context, d, folder: false),
                    icon: const Icon(Icons.upload_file),
                    label: Text(tr('send_files')),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: FilledButton.tonalIcon(
                    style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(56)),
                    onPressed: () => pickAndSend(context, d, folder: true),
                    icon: const Icon(Icons.drive_folder_upload),
                    label: Text(tr('send_folder')),
                  ),
                ),
              ]),
              if (Platform.isWindows) ...[
                const SizedBox(height: 16),
                Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                  Icon(Icons.file_download_outlined, color: scheme.onSurfaceVariant),
                  const SizedBox(width: 8),
                  Flexible(child: Text(tr('drop_here'), style: TextStyle(color: scheme.onSurfaceVariant))),
                ]),
              ],
            ]),
          ),
        );

        final body = PageBody(children: [
          ListTile(
            contentPadding: EdgeInsets.zero,
            leading: CircleAvatar(radius: 26, child: Icon(platformIcon(d.platform), size: 28)),
            title: Text(d.displayName, style: Theme.of(context).textTheme.titleLarge),
            subtitle: Row(children: [
              StatusDot(online: d.online),
              const SizedBox(width: 6),
              Text('${d.online ? tr('online') : tr('offline')}  ·  ID ${d.deviceId}'),
            ]),
          ),
          const SizedBox(height: 8),
          Platform.isWindows
              ? DropTarget(
                  onDragEntered: (_) => setState(() => _dragging = true),
                  onDragExited: (_) => setState(() => _dragging = false),
                  onDragDone: (details) {
                    setState(() => _dragging = false);
                    sendToDevice(context, d, details.files.map((f) => f.path).toList());
                  },
                  child: sendArea,
                )
              : sendArea,
          Card(
            child: Column(children: [
              SwitchListTile(
                title: Text(tr('auto_accept')),
                subtitle: Text(tr('auto_accept_sub')),
                value: d.autoAccept,
                onChanged: (v) => core.setAutoAccept(fingerprint: d.fingerprint, on_: v),
              ),
              ListTile(
                leading: _checking
                    ? const SizedBox(width: 24, height: 24, child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.network_check),
                title: Text(tr('check_connection')),
                subtitle: d.lastAddr == null ? null : Text(d.lastAddr!.split(':').first),
                onTap: _checking ? null : () => _check(d),
              ),
              ListTile(leading: const Icon(Icons.edit_outlined), title: Text(tr('rename')), onTap: () => _rename(d)),
              ListTile(
                leading: Icon(Icons.link_off, color: scheme.error),
                title: Text(tr('forget'), style: TextStyle(color: scheme.error)),
                onTap: () => _forget(d),
              ),
            ]),
          ),
          if (history.isNotEmpty) ...[
            SectionHeader(tr('with_device')),
            for (final h in history) HistoryTile(item: h, showPeer: false),
          ],
          if (d.lastSeen > 0 && !d.online)
            Padding(
              padding: const EdgeInsets.only(top: 12),
              child: Text('${tr('offline')} · ${formatTime(d.lastSeen)}', style: TextStyle(color: scheme.onSurfaceVariant)),
            ),
        ]);

        return Scaffold(appBar: AppBar(), body: body);
      },
    );
  }
}
