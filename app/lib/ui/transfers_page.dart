import 'package:flutter/material.dart';

import '../main.dart' show appState;
import '../src/rust/api/vegam.dart' as core;
import '../state/app_state.dart';
import '../util/format.dart';
import '../util/l10n.dart';
import 'common.dart';

class TransfersPage extends StatelessWidget {
  const TransfersPage({super.key});

  Future<void> _run(BuildContext context, Future<void> Function() f) async {
    try {
      await f();
    } catch (e) {
      if (context.mounted) showSnack(context, describeError(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(tr('tab_transfers'))),
      body: ListenableBuilder(
        listenable: appState,
        builder: (context, _) {
          final active = appState.transfers.values.toList();
          final resumable = appState.resumable;
          return PageBody(children: [
            if (active.isEmpty && resumable.isEmpty) EmptyHint(icon: Icons.swap_vert, text: tr('no_transfers')),
            for (final t in active)
              _ActiveCard(
                t: t,
                onPause: () => _run(context, () => core.pause(transferId: t.id)),
                onStop: () => _run(context, () => core.cancel(transferId: t.id)),
              ),
            if (resumable.isNotEmpty) SectionHeader(tr('resumable')),
            for (final h in resumable)
              Card(
                child: ListTile(
                  leading: Icon(h.outgoing ? Icons.north_east : Icons.south_west),
                  title: Text(h.summary, maxLines: 1, overflow: TextOverflow.ellipsis),
                  subtitle: Text(
                      '${h.outgoing ? tr('to') : tr('from')} ${h.peerName} · ${formatBytes(h.totalBytes.toInt())} · ${statusLabel(h.status)}'),
                  trailing: Row(mainAxisSize: MainAxisSize.min, children: [
                    IconButton(
                      tooltip: tr('resume'),
                      icon: const Icon(Icons.play_arrow),
                      onPressed: () => _run(context, () => core.resume(transferId: h.transferId)),
                    ),
                    IconButton(
                      tooltip: tr('stop'),
                      icon: const Icon(Icons.close),
                      onPressed: () => _run(context, () async {
                        await core.cancel(transferId: h.transferId);
                        await appState.refreshHistory();
                      }),
                    ),
                  ]),
                ),
              ),
          ]);
        },
      ),
    );
  }
}

class _ActiveCard extends StatelessWidget {
  const _ActiveCard({required this.t, required this.onPause, required this.onStop});
  final TransferUi t;
  final VoidCallback onPause;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final eta = formatEta(t.bytesTotal - t.bytesDone, t.speed);
    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 8, 16),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Icon(t.outgoing ? Icons.north_east : Icons.south_west, color: theme.colorScheme.primary),
            const SizedBox(width: 8),
            Expanded(
              child: Text('${t.outgoing ? tr('to') : tr('from')} ${t.peerName}',
                  style: theme.textTheme.titleMedium, overflow: TextOverflow.ellipsis),
            ),
            IconButton(tooltip: tr('pause'), onPressed: onPause, icon: const Icon(Icons.pause)),
            IconButton(tooltip: tr('stop'), onPressed: onStop, icon: const Icon(Icons.close)),
          ]),
          const SizedBox(height: 8),
          Padding(
            padding: const EdgeInsets.only(right: 8),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(4),
              child: LinearProgressIndicator(value: t.fraction.clamp(0, 1).toDouble(), minHeight: 8),
            ),
          ),
          const SizedBox(height: 8),
          Text(
            '${(t.fraction * 100).toStringAsFixed(1)}%  ·  ${formatBytes(t.bytesDone)} / ${formatBytes(t.bytesTotal)}'
            '  ·  ${t.filesDone}/${t.filesTotal} ${tr('files')}'
            '${t.speed > 0 ? '  ·  ${formatSpeed(t.speed)}' : ''}${eta.isNotEmpty ? '  ·  $eta' : ''}',
            style: theme.textTheme.bodySmall,
          ),
          if (t.current.isNotEmpty)
            Text(t.current, maxLines: 1, overflow: TextOverflow.ellipsis, style: theme.textTheme.bodySmall?.copyWith(color: theme.colorScheme.onSurfaceVariant)),
        ]),
      ),
    );
  }
}
