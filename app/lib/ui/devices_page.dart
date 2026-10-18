import 'dart:async';
import 'dart:io';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../main.dart' show appState;
import '../platform/platform_services.dart';
import '../src/rust/api/vegam.dart' as core;
import '../util/format.dart';
import '../util/l10n.dart';
import 'add_device_page.dart';
import 'hotspot_page.dart';
import 'common.dart';
import 'device_page.dart';

class DevicesPage extends StatelessWidget {
  const DevicesPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(tr('app_name')),
        actions: [
          if (Platform.isAndroid)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: TextButton.icon(
                onPressed: openClone,
                icon: const Icon(Icons.copy_all),
                label: Text(tr('clone_apps')),
              ),
            ),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => const AddDevicePage())),
        icon: const Icon(Icons.add_link),
        label: Text(tr('add_device')),
      ),
      body: ListenableBuilder(
        listenable: appState,
        builder: (context, _) {
          final paired = appState.devices;
          // Only devices that chose to be visible show up here.
          final others = appState.nearby.where((n) => !n.paired).toList();
          return RefreshIndicator(
            onRefresh: appState.refreshAll,
            child: PageBody(children: [
              const MyDeviceCard(),
              const NetworkBanners(),
              SectionHeader(tr('paired_devices')),
              if (paired.isEmpty) EmptyHint(icon: Icons.devices, text: tr('no_paired')),
              for (final d in paired) DeviceTile(device: d),
              if (others.isNotEmpty) SectionHeader(tr('nearby')),
              for (final n in others)
                Card(
                  child: ListTile(
                    leading: Icon(platformIcon(n.platform)),
                    title: Text(n.name),
                    subtitle: Text('ID ${n.deviceId}  ·  ${n.address.split(':').first}'),
                    trailing: FilledButton.tonal(
                      onPressed: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => const AddDevicePage())),
                      child: Text(tr('connect')),
                    ),
                  ),
                ),
            ]),
          );
        },
      ),
    );
  }
}

/// Network situations the user should know about: Windows "Public"
/// network (we are unreachable), our hotspot running, a joined hotspot.
class NetworkBanners extends StatelessWidget {
  const NetworkBanners({super.key});

  // Listens itself: as a const child it is skipped when the parent rebuilds.
  @override
  Widget build(BuildContext context) => ListenableBuilder(listenable: appState, builder: (context, _) => _banners(context));

  Widget _banners(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final banners = <Widget>[];
    if (appState.publicNetwork) {
      banners.add(Card(
        color: scheme.errorContainer,
        child: ListTile(
          leading: const Icon(Icons.public),
          title: Text(tr('public_network_title')),
          subtitle: Text(tr('public_network_body')),
          trailing: TextButton(onPressed: openWindowsNetworkSettings, child: Text(tr('open_settings'))),
        ),
      ));
    }
    final h = appState.hotspot;
    if (h != null) {
      banners.add(Card(
        color: scheme.tertiaryContainer,
        child: ListTile(
          leading: const Icon(Icons.wifi_tethering),
          title: Text(tr('hotspot_active', {'name': h.ssid})),
          subtitle: Text(tr('hotspot_active_sub')),
          onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => const HotspotPage())),
          trailing: TextButton(
            onPressed: () async {
              await stopHotspot();
              appState.setHotspot(null);
            },
            child: Text(tr('hotspot_stop_short')),
          ),
        ),
      ));
    }
    final joined = appState.joinedSsid;
    if (joined != null) {
      banners.add(Card(
        color: scheme.secondaryContainer,
        child: ListTile(
          leading: const Icon(Icons.wifi),
          title: Text(tr('joined_hotspot', {'name': joined})),
          trailing: TextButton(
            onPressed: () async {
              await leaveWifi();
              appState.setJoined(null);
              await core.rebindNetwork();
            },
            child: Text(tr('disconnect')),
          ),
        ),
      ));
    }
    if (banners.isEmpty) return const SizedBox.shrink();
    return Padding(padding: const EdgeInsets.only(top: 8), child: Column(children: banners));
  }
}

class MyDeviceCard extends StatefulWidget {
  const MyDeviceCard({super.key});

  @override
  State<MyDeviceCard> createState() => _MyDeviceCardState();
}

class _MyDeviceCardState extends State<MyDeviceCard> {
  bool _showPassword = false;

  void _copy(String text) {
    Clipboard.setData(ClipboardData(text: text));
    showSnack(context, tr('copied'));
  }

  // Listens itself: as a const child it is skipped when the parent rebuilds.
  @override
  Widget build(BuildContext context) => ListenableBuilder(listenable: appState, builder: (context, _) => _card(context));

  Widget _card(BuildContext context) {
    final info = appState.info;
    if (info == null) return const SizedBox.shrink();
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    return Card(
      color: scheme.primaryContainer,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Icon(platformIcon(info.platform), color: scheme.onPrimaryContainer),
            const SizedBox(width: 8),
            Expanded(
              child: Text('${tr('my_device')} · ${info.name}',
                  style: theme.textTheme.titleSmall?.copyWith(color: scheme.onPrimaryContainer), overflow: TextOverflow.ellipsis),
            ),
          ]),
          const SizedBox(height: 16),
          Text(tr('my_id'), style: theme.textTheme.labelMedium?.copyWith(color: scheme.onPrimaryContainer)),
          Row(children: [
            Expanded(
              child: SelectableText(info.deviceIdFormatted,
                  style: theme.textTheme.displaySmall?.copyWith(
                      color: scheme.onPrimaryContainer, fontWeight: FontWeight.w600, letterSpacing: 2, fontFeatures: const [FontFeature.tabularFigures()])),
            ),
            IconButton(tooltip: tr('copy'), onPressed: () => _copy(info.deviceId), icon: const Icon(Icons.copy)),
          ]),
          const SizedBox(height: 8),
          Text(tr('password'), style: theme.textTheme.labelMedium?.copyWith(color: scheme.onPrimaryContainer)),
          Row(children: [
            Expanded(
              child: SelectableText(_showPassword ? info.password : '•' * info.password.length,
                  style: theme.textTheme.titleLarge?.copyWith(color: scheme.onPrimaryContainer, fontFamily: 'monospace')),
            ),
            IconButton(
              tooltip: _showPassword ? tr('hide') : tr('show'),
              onPressed: () => setState(() => _showPassword = !_showPassword),
              icon: Icon(_showPassword ? Icons.visibility_off : Icons.visibility),
            ),
            IconButton(tooltip: tr('copy'), onPressed: () => _copy(info.password), icon: const Icon(Icons.copy)),
            IconButton(
              tooltip: tr('new_password'),
              onPressed: () async {
                await core.regeneratePassword();
                appState.refreshInfo();
                setState(() => _showPassword = true);
              },
              icon: const Icon(Icons.refresh),
            ),
          ]),
          const Divider(height: 24),
          Row(children: [
            Icon(info.receiving ? Icons.sensors : Icons.sensors_off, color: scheme.onPrimaryContainer),
            const SizedBox(width: 12),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(tr('ready'), style: theme.textTheme.titleSmall?.copyWith(color: scheme.onPrimaryContainer)),
                Text(info.receiving ? tr('ready_on') : tr('ready_off'),
                    style: theme.textTheme.bodySmall?.copyWith(color: scheme.onPrimaryContainer)),
              ]),
            ),
            Switch(
              value: info.receiving,
              onChanged: (v) async {
                await core.setReceiving(on_: v);
                appState.refreshInfo();
                // Starting/stopping the Android service can take a while;
                // the switch must not wait for it.
                unawaited(setBackgroundService(v).catchError((_) {}));
              },
            ),
          ]),
        ]),
      ),
    );
  }
}

class DeviceTile extends StatefulWidget {
  const DeviceTile({super.key, required this.device});
  final core.Device device;

  @override
  State<DeviceTile> createState() => _DeviceTileState();
}

class _DeviceTileState extends State<DeviceTile> {
  bool _dragging = false;

  @override
  Widget build(BuildContext context) {
    final d = widget.device;
    final scheme = Theme.of(context).colorScheme;
    final tile = Card(
      color: _dragging ? scheme.secondaryContainer : null,
      child: ListTile(
        contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
        leading: Stack(clipBehavior: Clip.none, children: [
          CircleAvatar(child: Icon(platformIcon(d.platform))),
          Positioned(right: -1, bottom: -1, child: StatusDot(online: d.online, size: 12)),
        ]),
        title: Text(d.displayName, style: const TextStyle(fontWeight: FontWeight.w600)),
        subtitle: Text('${d.online ? tr('online') : d.lastSeen > 0 ? tr('last_seen', {'time': formatTime(d.lastSeen)}) : tr('never_connected')}  ·  ID ${d.deviceId}'),
        trailing: Row(mainAxisSize: MainAxisSize.min, children: [
          IconButton(
            tooltip: tr('send_files'),
            onPressed: () => pickAndSend(context, d, folder: false),
            icon: const Icon(Icons.upload_file),
          ),
          IconButton(
            tooltip: tr('send_folder'),
            onPressed: () => pickAndSend(context, d, folder: true),
            icon: const Icon(Icons.drive_folder_upload),
          ),
        ]),
        onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => DevicePage(fingerprint: d.fingerprint))),
      ),
    );
    if (!Platform.isWindows) return tile;
    // Drop files straight onto a device to send them.
    return DropTarget(
      onDragEntered: (_) => setState(() => _dragging = true),
      onDragExited: (_) => setState(() => _dragging = false),
      onDragDone: (details) {
        setState(() => _dragging = false);
        sendToDevice(context, d, details.files.map((f) => f.path).toList());
      },
      child: tile,
    );
  }
}

class AddDeviceDialog extends StatefulWidget {
  const AddDeviceDialog({super.key, this.deviceId});
  final String? deviceId;

  @override
  State<AddDeviceDialog> createState() => _AddDeviceDialogState();
}

class _AddDeviceDialogState extends State<AddDeviceDialog> {
  late final _id = TextEditingController(text: widget.deviceId ?? '');
  final _password = TextEditingController();
  final _ip = TextEditingController();
  bool _busy = false;
  bool _showIp = false;
  bool _obscure = true;
  String? _error;

  @override
  void dispose() {
    _id.dispose();
    _password.dispose();
    _ip.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final d = await core.pair(
        deviceId: _id.text,
        address: _ip.text.trim().isEmpty ? null : _ip.text.trim(),
        password: _password.text,
      );
      await appState.refreshDevices();
      if (!mounted) return;
      // The "Add device" page closes itself (and this dialog) as soon as it
      // sees the new device. Popping again would remove the home screen.
      final route = ModalRoute.of(context);
      if (route == null || !route.isActive || !route.isCurrent) return;
      final messenger = ScaffoldMessenger.of(context);
      Navigator.pop(context);
      messenger.showSnackBar(SnackBar(content: Text(tr('paired_ok', {'name': d.displayName})), behavior: SnackBarBehavior.floating));
    } catch (e) {
      if (mounted) setState(() => _error = describeError(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      icon: const Icon(Icons.add_link),
      title: Text(tr('add_device_title')),
      content: SizedBox(
        width: 400,
        child: SingleChildScrollView(
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text(tr('add_device_hint'), style: Theme.of(context).textTheme.bodySmall),
            const SizedBox(height: 16),
            TextField(
              controller: _id,
              autofocus: widget.deviceId == null,
              keyboardType: TextInputType.number,
              inputFormatters: [FilteringTextInputFormatter.allow(RegExp(r'[0-9 ]')), LengthLimitingTextInputFormatter(11)],
              decoration: InputDecoration(labelText: tr('device_id'), prefixIcon: const Icon(Icons.tag)),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _password,
              autofocus: widget.deviceId != null,
              obscureText: _obscure,
              onSubmitted: (_) => _busy ? null : _submit(),
              decoration: InputDecoration(
                labelText: tr('password'),
                prefixIcon: const Icon(Icons.key),
                suffixIcon: IconButton(
                  icon: Icon(_obscure ? Icons.visibility : Icons.visibility_off),
                  onPressed: () => setState(() => _obscure = !_obscure),
                ),
              ),
            ),
            if (_showIp) ...[
              const SizedBox(height: 12),
              TextField(
                controller: _ip,
                keyboardType: TextInputType.url,
                decoration: InputDecoration(labelText: tr('ip_optional'), helperText: tr('ip_help'), helperMaxLines: 3, prefixIcon: const Icon(Icons.lan)),
              ),
            ] else
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton(onPressed: () => setState(() => _showIp = true), child: Text(tr('ip_optional'))),
              ),
            if (_error != null) ...[
              const SizedBox(height: 8),
              Text(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
            ],
          ]),
        ),
      ),
      actions: [
        TextButton(onPressed: _busy ? null : () => Navigator.pop(context), child: Text(tr('cancel'))),
        FilledButton(
          onPressed: _busy ? null : _submit,
          child: _busy ? const SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)) : Text(tr('connect')),
        ),
      ],
    );
  }
}
