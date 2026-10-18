import 'dart:async';

import 'package:flutter/material.dart';

import '../platform/platform_services.dart';
import '../util/format.dart';
import '../util/l10n.dart';

/// Shows the stages of installing a received app and lets the user stop the
/// data restore. Pops with the native result string, or the error.
class RestoreDialog extends StatefulWidget {
  const RestoreDialog({super.key, required this.label, required this.work});
  final String label;
  final Future<String> work;

  @override
  State<RestoreDialog> createState() => _RestoreDialogState();
}

class _RestoreDialogState extends State<RestoreDialog> {
  RestoreProgress? _p;
  bool _cancelling = false;
  StreamSubscription<RestoreProgress>? _sub;

  @override
  void initState() {
    super.initState();
    _sub = restoreProgress.stream.listen((p) => mounted ? setState(() => _p = p) : null);
    widget.work.then(
      (r) => mounted ? Navigator.pop(context, r) : null,
      onError: (Object e) => mounted ? Navigator.pop(context, e) : null,
    );
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = _p;
    final stage = p?.stage ?? 'install';
    final fraction = (p != null && p.total > 0 && (stage == 'data' || stage == 'obb')) ? p.done / p.total : null;
    final detail = switch (stage) {
      'data' when p != null && p.total > 0 => '${formatBytes(p.done)} / ${formatBytes(p.total)}',
      'obb' when p != null && p.total > 0 => '${p.done} / ${p.total}',
      _ => '',
    };
    final canCancel = (stage == 'data' || stage == 'obb') && !_cancelling;
    return PopScope(
      canPop: false,
      child: AlertDialog(
        title: Text(widget.label),
        content: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(tr(_cancelling ? 'restore_cancelling' : 'restore_$stage')),
          const SizedBox(height: 16),
          LinearProgressIndicator(value: fraction?.clamp(0, 1).toDouble()),
          if (detail.isNotEmpty) ...[const SizedBox(height: 8), Text(detail, style: Theme.of(context).textTheme.bodySmall)],
        ]),
        actions: [
          TextButton(
            onPressed: canCancel
                ? () {
                    setState(() => _cancelling = true);
                    cancelRestore();
                  }
                : null,
            child: Text(tr('cancel')),
          ),
        ],
      ),
    );
  }
}
