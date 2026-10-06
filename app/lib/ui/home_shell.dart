import 'dart:async';

import 'package:flutter/material.dart';

import '../main.dart' show appState;
import '../platform/platform_services.dart';
import '../src/rust/api/vegam.dart' as core;
import '../state/app_state.dart';
import '../util/format.dart';
import '../util/l10n.dart';
import 'add_device_page.dart' show formatCode;
import 'common.dart';
import 'devices_page.dart';
import 'history_page.dart';
import 'settings_page.dart';
import 'transfers_page.dart';

class HomeShell extends StatefulWidget {
  const HomeShell({super.key});

  @override
  State<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends State<HomeShell> {
  int _tab = 0;
  final List<StreamSubscription> _subs = [];
  final Map<String, BuildContext> _openOffers = {};
  final _messengerKey = GlobalKey<ScaffoldMessengerState>();

  @override
  void initState() {
    super.initState();
    _subs.add(appState.offers.listen(_showOffer));
    _subs.add(appState.offerClosed.listen((id) {
      // The sender gave up or another decision path won: close the dialog.
      final ctx = _openOffers.remove(id);
      if (ctx != null && ctx.mounted) Navigator.pop(ctx);
    }));
    _subs.add(appState.finished.listen(_onFinished));
    _subs.add(appState.pairRequests.listen(_showPairRequest));
    _subs.add(appState.pairRequestClosed.listen((id) {
      final ctx = _openOffers.remove(id);
      if (ctx != null && ctx.mounted) Navigator.pop(ctx);
    }));
    _subs.add(externalPaths.stream.listen(_chooseDeviceFor));
    if (pendingExternalPaths.isNotEmpty) {
      final p = pendingExternalPaths;
      pendingExternalPaths = [];
      WidgetsBinding.instance.addPostFrameCallback((_) => _chooseDeviceFor(p));
    }
  }

  @override
  void dispose() {
    for (final s in _subs) {
      s.cancel();
    }
    super.dispose();
  }

  void _showOffer(IncomingOffer o) {
    showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (ctx) {
        _openOffers[o.id] = ctx;
        return AlertDialog(
        icon: const Icon(Icons.move_to_inbox),
        title: Text(tr('incoming_title')),
        content: Text(tr('incoming_body', {
          'name': o.fromName,
          'summary': o.summary,
          'count': o.count,
          'size': formatBytes(o.size),
        })),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: Text(tr('decline'))),
          FilledButton(onPressed: () => Navigator.pop(ctx, true), child: Text(tr('accept'))),
        ],
      );
      },
    ).then((accept) {
      if (_openOffers.remove(o.id) == null || accept == null) return;
      core.respondToOffer(transferId: o.id, accept: accept).catchError((_) {});
    });
  }

  /// Someone tapped this device on their "Add device" screen.
  void _showPairRequest(PairRequest r) {
    showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (ctx) {
        _openOffers[r.id] = ctx;
        final theme = Theme.of(ctx);
        return AlertDialog(
          icon: Icon(platformIcon(r.platform)),
          title: Text(tr('pair_request_title')),
          content: Column(mainAxisSize: MainAxisSize.min, children: [
            Text(tr('pair_request_body', {'name': r.fromName}), textAlign: TextAlign.center),
            const SizedBox(height: 16),
            Text(formatCode(r.code),
                style: theme.textTheme.displaySmall?.copyWith(fontWeight: FontWeight.w700, letterSpacing: 4)),
          ]),
          actions: [
            TextButton(onPressed: () => Navigator.pop(ctx, false), child: Text(tr('decline'))),
            FilledButton(onPressed: () => Navigator.pop(ctx, true), child: Text(tr('accept'))),
          ],
        );
      },
    ).then((accept) {
      if (_openOffers.remove(r.id) == null || accept == null) return;
      core.respondToPairRequest(requestId: r.id, accept: accept).catchError((_) {});
    });
  }

  void _onFinished(FinishedNotice n) {
    final messenger = _messengerKey.currentState;
    if (messenger == null) return;
    String text;
    SnackBarAction? action;
    if (n.status == 'completed') {
      text = tr('transfer_done');
      if (!n.outgoing && n.location != null) {
        text = tr('received_saved', {'path': n.location!});
        action = SnackBarAction(label: tr('open_folder'), onPressed: () => revealPath(n.location!));
      }
    } else {
      text = statusLabel(n.status);
      if (n.error != null && n.status == 'failed') text = '$text: ${n.error}';
    }
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(text), action: action, behavior: SnackBarBehavior.floating));
  }

  Future<void> _chooseDeviceFor(List<String> paths) async {
    if (!mounted) return;
    final device = await showModalBottomSheet<core.Device>(
      context: context,
      showDragHandle: true,
      builder: (ctx) => SafeArea(
        child: ListView(shrinkWrap: true, children: [
          ListTile(title: Text(tr('choose_device'), style: Theme.of(ctx).textTheme.titleMedium)),
          if (appState.devices.isEmpty) EmptyHint(icon: Icons.devices, text: tr('no_paired')),
          for (final d in appState.devices)
            ListTile(
              leading: Icon(platformIcon(d.platform)),
              title: Text(d.displayName),
              trailing: StatusDot(online: d.online),
              onTap: () => Navigator.pop(ctx, d),
            ),
        ]),
      ),
    );
    if (device != null && mounted) {
      await sendToDevice(_messengerKey.currentContext ?? context, device, paths);
    }
  }

  @override
  Widget build(BuildContext context) {
    const pages = [DevicesPage(), TransfersPage(), HistoryPage(), SettingsPage()];
    final destinations = [
      (Icons.devices_outlined, Icons.devices, tr('tab_devices')),
      (Icons.swap_vert_outlined, Icons.swap_vert, tr('tab_transfers')),
      (Icons.history_outlined, Icons.history, tr('tab_history')),
      (Icons.settings_outlined, Icons.settings, tr('tab_settings')),
    ];
    Widget badge(int i, IconData icon) {
      final active = appState.transfers.length;
      return i == 1 && active > 0 ? Badge(label: Text('$active'), child: Icon(icon)) : Icon(icon);
    }

    return ScaffoldMessenger(
      key: _messengerKey,
      child: ListenableBuilder(listenable: appState, builder: (context, _) => LayoutBuilder(builder: (context, c) {
        final wide = c.maxWidth >= 720;
        final body = IndexedStack(index: _tab, children: pages);
        if (wide) {
          return Scaffold(
            body: Row(children: [
              NavigationRail(
                selectedIndex: _tab,
                onDestinationSelected: (i) => setState(() => _tab = i),
                labelType: NavigationRailLabelType.all,
                leading: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 16),
                  child: Icon(Icons.bolt, size: 32, color: Theme.of(context).colorScheme.primary),
                ),
                destinations: [
                  for (var i = 0; i < destinations.length; i++)
                    NavigationRailDestination(
                      icon: badge(i, destinations[i].$1),
                      selectedIcon: badge(i, destinations[i].$2),
                      label: Text(destinations[i].$3),
                    ),
                ],
              ),
              const VerticalDivider(width: 1),
              Expanded(child: body),
            ]),
          );
        }
        return Scaffold(
          body: SafeArea(child: body),
          bottomNavigationBar: NavigationBar(
            selectedIndex: _tab,
            onDestinationSelected: (i) => setState(() => _tab = i),
            destinations: [
              for (var i = 0; i < destinations.length; i++)
                NavigationDestination(
                  icon: badge(i, destinations[i].$1),
                  selectedIcon: badge(i, destinations[i].$2),
                  label: destinations[i].$3,
                ),
            ],
          ),
        );
      })),
    );
  }
}
