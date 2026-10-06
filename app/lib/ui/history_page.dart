import 'package:flutter/material.dart';

import '../main.dart' show appState;
import '../platform/platform_services.dart';
import '../src/rust/api/vegam.dart' as core;
import '../util/format.dart';
import '../util/l10n.dart';
import 'common.dart';

class HistoryPage extends StatelessWidget {
  const HistoryPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(tr('tab_history')),
        actions: [
          IconButton(
            tooltip: tr('clear_history'),
            icon: const Icon(Icons.delete_sweep_outlined),
            onPressed: () async {
              await core.clearHistory();
              await appState.refreshHistory();
            },
          ),
        ],
      ),
      body: ListenableBuilder(
        listenable: appState,
        builder: (context, _) {
          final items = appState.history.where((h) => h.status != 'in_progress').toList();
          return RefreshIndicator(
            onRefresh: appState.refreshHistory,
            child: PageBody(children: [
              if (items.isEmpty) EmptyHint(icon: Icons.history, text: tr('no_history')),
              for (final h in items) HistoryTile(item: h),
            ]),
          );
        },
      ),
    );
  }
}

class HistoryTile extends StatelessWidget {
  const HistoryTile({super.key, required this.item, this.showPeer = true});
  final core.HistoryItem item;
  final bool showPeer;

  Color _statusColor(BuildContext context) {
    final s = Theme.of(context).colorScheme;
    return switch (item.status) {
      'completed' => const Color(0xFF2E7D32),
      'failed' || 'cancelled' || 'rejected' => s.error,
      _ => s.tertiary,
    };
  }

  @override
  Widget build(BuildContext context) {
    final h = item;
    final where = showPeer ? '${h.outgoing ? tr('to') : tr('from')} ${h.peerName} · ' : '';
    final canOpen = !h.outgoing && h.location != null && h.status == 'completed';
    return Card(
      child: ListTile(
        leading: CircleAvatar(
          backgroundColor: Theme.of(context).colorScheme.secondaryContainer,
          child: Icon(h.outgoing ? Icons.north_east : Icons.south_west),
        ),
        title: Text(h.summary, maxLines: 1, overflow: TextOverflow.ellipsis),
        subtitle: Text('$where${h.fileCount} ${tr('files')} · ${formatBytes(h.totalBytes.toInt())} · ${formatTime(h.startedAt)}'),
        trailing: Text(statusLabel(h.status), style: TextStyle(color: _statusColor(context), fontWeight: FontWeight.w600)),
        onTap: canOpen ? () => revealPath(h.location!) : null,
      ),
    );
  }
}
