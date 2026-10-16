import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../main.dart' show appState;
import '../platform/platform_services.dart';
import '../src/rust/api/vegam.dart' as core;
import '../util/l10n.dart';
import 'common.dart';

/// Pairing code plus the hotspot's credentials. Other VegamShare phones
/// that scan it join the hotspot first, then pair (or just connect, if
/// already paired). The core ignores the extra parameters.
String hotspotQr(HotspotInfo h) =>
    '${core.pairQrCode()}&ws=${Uri.encodeQueryComponent(h.ssid)}&wp=${Uri.encodeQueryComponent(h.passphrase)}&wt=${h.wpa3 ? 3 : 2}';

/// "No Wi-Fi" mode on Android: this phone creates a temporary hotspot (no
/// internet, no mobile data used) that other devices join.
class HotspotPage extends StatefulWidget {
  const HotspotPage({super.key});

  @override
  State<HotspotPage> createState() => _HotspotPageState();
}

class _HotspotPageState extends State<HotspotPage> {
  HotspotInfo? _hotspot;
  String? _qr;
  String? _error;
  bool _starting = true;
  Timer? _keepOpen;
  int _devicesSeen = 0;

  @override
  void initState() {
    super.initState();
    _devicesSeen = appState.devices.length;
    // Visible and accepting tap-to-pair while shown, so a laptop that joined
    // the hotspot can pair from its own Add device screen.
    pairingScreenOpened();
    _keepOpen = Timer.periodic(const Duration(seconds: 90), (_) {
      pairingScreenRenew();
      if (_hotspot != null) setState(() => _qr = hotspotQr(_hotspot!)); // QR tokens expire after 2 min
    });
    appState.addListener(_onState);
    _start();
  }

  @override
  void dispose() {
    _keepOpen?.cancel();
    appState.removeListener(_onState);
    pairingScreenClosed();
    super.dispose();
  }

  void _onState() {
    // A device paired using the QR: its token is spent, show a fresh one.
    if (appState.devices.length != _devicesSeen && _hotspot != null && mounted) {
      _devicesSeen = appState.devices.length;
      setState(() => _qr = hotspotQr(_hotspot!));
    }
    if (appState.hotspot == null && _hotspot != null && mounted) {
      setState(() {
        _hotspot = null;
        _qr = null;
        _error = tr('hotspot_stopped');
      });
    }
  }

  Future<void> _start() async {
    setState(() {
      _starting = true;
      _error = null;
    });
    try {
      final before = appState.hotspot == null ? core.myInfo().addresses.toSet() : null;
      final h = appState.hotspot ?? await startHotspot();
      appState.setHotspot(h);
      // The hotspot interface may get its address a moment after Android
      // reports the hotspot started; wait for it so the QR includes it.
      if (before != null) {
        for (var i = 0; i < 10 && core.myInfo().addresses.toSet().difference(before).isEmpty; i++) {
          await Future<void>.delayed(const Duration(milliseconds: 300));
        }
      }
      if (!mounted) return;
      setState(() {
        _hotspot = h;
        _qr = hotspotQr(h);
      });
    } on WifiLinkException catch (e) {
      if (mounted) setState(() => _error = describeWifiError(e.code));
    } catch (e) {
      if (mounted) setState(() => _error = describeError(e));
    } finally {
      if (mounted) setState(() => _starting = false);
    }
  }

  Future<void> _stop() async {
    await stopHotspot();
    appState.setHotspot(null);
    if (mounted) Navigator.of(context).pop();
  }

  void _copy(String text) {
    Clipboard.setData(ClipboardData(text: text));
    showSnack(context, tr('copied'));
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final h = _hotspot;
    return Scaffold(
      appBar: AppBar(title: Text(tr('no_wifi_title'))),
      body: PageBody(children: [
        if (_starting) const Padding(padding: EdgeInsets.all(48), child: Center(child: CircularProgressIndicator())),
        if (_error != null) ...[
          Card(
            color: theme.colorScheme.errorContainer,
            child: ListTile(leading: const Icon(Icons.wifi_off), title: Text(_error!)),
          ),
          const SizedBox(height: 8),
          FilledButton.icon(onPressed: _start, icon: const Icon(Icons.refresh), label: Text(tr('retry'))),
        ],
        if (h != null && _qr != null) ...[
          Text(tr('hotspot_scan_hint'), style: theme.textTheme.bodyMedium, textAlign: TextAlign.center),
          const SizedBox(height: 16),
          Center(
            child: Container(
              color: Colors.white,
              padding: const EdgeInsets.all(12),
              child: QrImageView(data: _qr!, size: 260, backgroundColor: Colors.white),
            ),
          ),
          const SizedBox(height: 16),
          Card(
            child: Column(children: [
              ListTile(
                leading: const Icon(Icons.laptop_windows),
                title: Text(tr('hotspot_laptop_title')),
                subtitle: Text(tr('hotspot_laptop_hint')),
              ),
              ListTile(
                leading: const Icon(Icons.wifi),
                title: Text(h.ssid, style: const TextStyle(fontWeight: FontWeight.w600)),
                subtitle: Text(tr('hotspot_name')),
                trailing: IconButton(icon: const Icon(Icons.copy), tooltip: tr('copy'), onPressed: () => _copy(h.ssid)),
              ),
              ListTile(
                leading: const Icon(Icons.key),
                title: SelectableText(h.passphrase, style: const TextStyle(fontFamily: 'monospace', fontWeight: FontWeight.w600)),
                subtitle: Text(tr('password')),
                trailing: IconButton(icon: const Icon(Icons.copy), tooltip: tr('copy'), onPressed: () => _copy(h.passphrase)),
              ),
            ]),
          ),
          const SizedBox(height: 8),
          Text(tr('hotspot_note'), style: theme.textTheme.bodySmall?.copyWith(color: theme.colorScheme.onSurfaceVariant)),
          const SizedBox(height: 16),
          OutlinedButton.icon(onPressed: _stop, icon: const Icon(Icons.wifi_tethering_off), label: Text(tr('hotspot_stop'))),
        ],
      ]),
    );
  }
}

String describeWifiError(String code) => switch (code) {
      'permission' => tr('e_wifi_permission'),
      'location_off' => tr('e_wifi_location_off'),
      'unsupported' => tr('e_wifi_unsupported'),
      'join_failed' => tr('e_wifi_join_failed'),
      'busy' => tr('e_wifi_busy'),
      _ => tr('e_wifi_failed', {'code': code}),
    };
