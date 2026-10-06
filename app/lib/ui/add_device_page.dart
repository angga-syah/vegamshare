import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../main.dart' show appState;
import '../src/rust/api/vegam.dart' as core;
import '../util/l10n.dart';
import 'common.dart';
import 'devices_page.dart' show AddDeviceDialog;

/// "Add device": every way to pair, best first. While this screen is open
/// the device is visible to others and accepts code-confirmed requests;
/// leaving it makes the device silent again.
class AddDevicePage extends StatefulWidget {
  const AddDevicePage({super.key});

  @override
  State<AddDevicePage> createState() => _AddDevicePageState();
}

class _AddDevicePageState extends State<AddDevicePage> {
  Timer? _keepOpen;
  int _devicesAtStart = 0;

  @override
  void initState() {
    super.initState();
    _devicesAtStart = appState.devices.length;
    core.setPairingMode(on_: true);
    // Pairing mode lasts two minutes per call; renew while the screen is open.
    _keepOpen = Timer.periodic(const Duration(seconds: 90), (_) => core.setPairingMode(on_: true));
    appState.addListener(_closeWhenPaired);
  }

  @override
  void dispose() {
    _keepOpen?.cancel();
    appState.removeListener(_closeWhenPaired);
    core.setPairingMode(on_: false);
    super.dispose();
  }

  /// The other side may complete pairing (QR scan, accepted request) while
  /// we just watch; leave as soon as a device was added.
  void _closeWhenPaired() {
    if (appState.devices.length > _devicesAtStart && mounted) {
      final newest = appState.devices.last;
      final messenger = ScaffoldMessenger.of(context);
      Navigator.of(context).popUntil((r) => r.isFirst);
      messenger.showSnackBar(SnackBar(content: Text(tr('paired_ok', {'name': newest.displayName})), behavior: SnackBarBehavior.floating));
    }
  }

  Future<void> _pairNearby(core.Nearby n) async {
    await showDialog<void>(context: context, barrierDismissible: false, builder: (_) => _NearbyPairDialog(target: n));
  }

  Future<void> _scan() async {
    final scanned = await Navigator.of(context).push<String>(MaterialPageRoute(builder: (_) => const _ScanPage()));
    if (scanned == null || !mounted) return;
    showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => const AlertDialog(content: Row(children: [CircularProgressIndicator(), SizedBox(width: 20), Expanded(child: Text('…'))])),
    );
    try {
      await core.pairWithQr(scanned: scanned);
      await appState.refreshDevices();
    } catch (e) {
      if (mounted) {
        Navigator.of(context).pop();
        showSnack(context, describeError(e));
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(tr('add_device_title'))),
      body: ListenableBuilder(
        listenable: appState,
        builder: (context, _) {
          final nearby = appState.nearby.where((n) => !n.paired).toList();
          return PageBody(children: [
            Card(
              color: theme.colorScheme.secondaryContainer,
              child: ListTile(
                leading: const Icon(Icons.info_outline),
                title: Text(tr('pair_hint_title')),
                subtitle: Text(tr('pair_hint')),
              ),
            ),
            SectionHeader(
              tr('nearby'),
              trailing: const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)),
            ),
            if (nearby.isEmpty) EmptyHint(icon: Icons.wifi_find, text: tr('searching_nearby')),
            for (final n in nearby)
              Card(
                child: ListTile(
                  leading: CircleAvatar(child: Icon(platformIcon(n.platform))),
                  title: Text(n.name, style: const TextStyle(fontWeight: FontWeight.w600)),
                  subtitle: Text('ID ${n.deviceId}'),
                  trailing: FilledButton.tonal(onPressed: () => _pairNearby(n), child: Text(tr('connect'))),
                  onTap: () => _pairNearby(n),
                ),
              ),
            SectionHeader(tr('other_ways')),
            Card(
              child: Column(children: [
                ListTile(
                  leading: const Icon(Icons.qr_code_2),
                  title: Text(tr('show_my_qr')),
                  subtitle: Text(tr('show_my_qr_sub')),
                  onTap: () => showDialog<void>(context: context, builder: (_) => const _MyQrDialog()),
                ),
                if (Platform.isAndroid)
                  ListTile(
                    leading: const Icon(Icons.qr_code_scanner),
                    title: Text(tr('scan_qr')),
                    subtitle: Text(tr('scan_qr_sub')),
                    onTap: _scan,
                  ),
                ListTile(
                  leading: const Icon(Icons.key),
                  title: Text(tr('pair_with_password')),
                  subtitle: Text(tr('pair_with_password_sub')),
                  onTap: () => showDialog<void>(context: context, builder: (_) => const AddDeviceDialog()),
                ),
              ]),
            ),
          ]);
        },
      ),
    );
  }
}

/// Asks a nearby device to pair and shows the code to compare.
class _NearbyPairDialog extends StatefulWidget {
  const _NearbyPairDialog({required this.target});
  final core.Nearby target;

  @override
  State<_NearbyPairDialog> createState() => _NearbyPairDialogState();
}

class _NearbyPairDialogState extends State<_NearbyPairDialog> {
  String? _attempt;
  String? _code;
  String? _error;

  @override
  void initState() {
    super.initState();
    _run();
  }

  Future<void> _run() async {
    try {
      final start = await core.pairNearbyStart(deviceId: widget.target.deviceId, address: widget.target.address);
      if (!mounted) {
        core.pairNearbyCancel(attemptId: start.attemptId);
        return;
      }
      setState(() {
        _attempt = start.attemptId;
        _code = start.code;
      });
      await core.pairNearbyFinish(attemptId: start.attemptId);
      await appState.refreshDevices(); // the page closes itself on success
    } catch (e) {
      if (mounted) setState(() => _error = describeError(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    Widget body;
    if (_error != null) {
      body = Text(_error!, style: TextStyle(color: theme.colorScheme.error));
    } else if (_code == null) {
      body = const Center(child: Padding(padding: EdgeInsets.all(16), child: CircularProgressIndicator()));
    } else {
      body = Column(mainAxisSize: MainAxisSize.min, children: [
        Text(tr('compare_code', {'name': widget.target.name}), textAlign: TextAlign.center),
        const SizedBox(height: 16),
        Text(formatCode(_code!),
            style: theme.textTheme.displaySmall?.copyWith(fontWeight: FontWeight.w700, letterSpacing: 4, fontFeatures: const [FontFeature.tabularFigures()])),
        const SizedBox(height: 16),
        Row(mainAxisAlignment: MainAxisAlignment.center, children: [
          const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)),
          const SizedBox(width: 12),
          Flexible(child: Text(tr('waiting_accept', {'name': widget.target.name}))),
        ]),
      ]);
    }
    return AlertDialog(
      icon: const Icon(Icons.link),
      title: Text(widget.target.name),
      content: SizedBox(width: 360, child: body),
      actions: [
        TextButton(
          onPressed: () {
            if (_attempt != null && _error == null) core.pairNearbyCancel(attemptId: _attempt!);
            Navigator.pop(context);
          },
          child: Text(_error != null ? tr('close') : tr('cancel')),
        ),
      ],
    );
  }
}

String formatCode(String code) => code.length == 6 ? '${code.substring(0, 3)} ${code.substring(3)}' : code;

/// This device's single-use pairing QR, renewed before it expires.
class _MyQrDialog extends StatefulWidget {
  const _MyQrDialog();

  @override
  State<_MyQrDialog> createState() => _MyQrDialogState();
}

class _MyQrDialogState extends State<_MyQrDialog> {
  late String _data = core.pairQrCode();
  Timer? _renew;

  @override
  void initState() {
    super.initState();
    _renew = Timer.periodic(const Duration(seconds: 100), (_) => setState(() => _data = core.pairQrCode()));
  }

  @override
  void dispose() {
    _renew?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(tr('show_my_qr')),
      content: SizedBox(
        width: 300,
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Container(
            color: Colors.white,
            padding: const EdgeInsets.all(12),
            child: QrImageView(data: _data, size: 260, backgroundColor: Colors.white),
          ),
          const SizedBox(height: 12),
          Text(tr('qr_hint'), textAlign: TextAlign.center, style: Theme.of(context).textTheme.bodySmall),
        ]),
      ),
      actions: [TextButton(onPressed: () => Navigator.pop(context), child: Text(tr('close')))],
    );
  }
}

class _ScanPage extends StatefulWidget {
  const _ScanPage();

  @override
  State<_ScanPage> createState() => _ScanPageState();
}

class _ScanPageState extends State<_ScanPage> {
  bool _done = false;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(tr('scan_qr'))),
      body: Stack(children: [
        MobileScanner(
          onDetect: (capture) {
            if (_done) return;
            for (final b in capture.barcodes) {
              final v = b.rawValue;
              if (v != null && v.startsWith('vegamshare://pair')) {
                _done = true;
                Navigator.of(context).pop(v);
                return;
              }
            }
          },
        ),
        Align(
          alignment: Alignment.bottomCenter,
          child: Container(
            margin: const EdgeInsets.all(24),
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
            decoration: BoxDecoration(color: Colors.black54, borderRadius: BorderRadius.circular(12)),
            child: Text(tr('scan_qr_sub'), style: const TextStyle(color: Colors.white), textAlign: TextAlign.center),
          ),
        ),
      ]),
    );
  }
}
