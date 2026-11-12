import 'package:flutter/material.dart';

import '../util/format.dart';
import '../util/l10n.dart';
import '../util/updater.dart';

bool _open = false;

/// Shows the update dialog (once, even if offered again while open). It
/// follows the updater through download and install.
Future<void> showUpdateDialog(BuildContext context) async {
  if (_open) return;
  _open = true;
  try {
    await showDialog<void>(context: context, barrierDismissible: false, builder: (_) => const _UpdateDialog());
  } finally {
    _open = false;
  }
}

class _UpdateDialog extends StatelessWidget {
  const _UpdateDialog();

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: updater,
      builder: (context, _) {
        final u = updater;
        final i = u.info;
        void close() {
          u.dismiss();
          Navigator.of(context).pop();
        }

        switch (u.stage) {
          case UpdateStage.available when i != null:
            return AlertDialog(
              icon: const Icon(Icons.system_update),
              title: Text(tr('update_available_title', {'v': i.version})),
              content: SizedBox(
                width: 420,
                child: SingleChildScrollView(
                  child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(tr('update_available_body', {'old': u.installed, 'v': i.version, 'size': i.size > 0 ? formatBytes(i.size) : '?'})),
                    if (i.notes.isNotEmpty) ...[
                      const SizedBox(height: 12),
                      Text(_plain(i.notes), style: Theme.of(context).textTheme.bodySmall),
                    ],
                  ]),
                ),
              ),
              actions: [
                TextButton(
                  onPressed: () {
                    u.later();
                    Navigator.of(context).pop();
                  },
                  child: Text(tr('later')),
                ),
                FilledButton(onPressed: u.download, child: Text(tr('update_now'))),
              ],
            );
          case UpdateStage.downloading:
            return AlertDialog(
              title: Text(tr('update_downloading', {'v': i?.version ?? ''})),
              content: Column(mainAxisSize: MainAxisSize.min, children: [
                LinearProgressIndicator(value: u.progress > 0 ? u.progress : null),
                const SizedBox(height: 8),
                Align(alignment: Alignment.centerRight, child: Text('${(u.progress * 100).round()}%')),
              ]),
              actions: [TextButton(onPressed: u.cancelDownload, child: Text(tr('cancel')))],
            );
          case UpdateStage.needsPermission:
            return AlertDialog(
              icon: const Icon(Icons.security),
              title: Text(tr('update_permission_title')),
              content: Text(tr('update_permission_body')),
              actions: [
                TextButton(onPressed: close, child: Text(tr('later'))),
                FilledButton(onPressed: u.install, child: Text(tr('update_install'))),
              ],
            );
          case UpdateStage.installing:
            return AlertDialog(
              icon: const Icon(Icons.install_mobile),
              title: Text(tr('update_installing_title', {'v': i?.version ?? ''})),
              content: Text(tr('update_installing_body')),
              actions: [
                TextButton(onPressed: u.install, child: Text(tr('update_install_again'))),
                FilledButton(onPressed: close, child: Text(tr('close'))),
              ],
            );
          case UpdateStage.failed:
            return AlertDialog(
              icon: const Icon(Icons.error_outline),
              title: Text(tr('update_failed_title')),
              content: Text(tr('update_failed_${u.error ?? 'check'}', {'url': releasesPage})),
              actions: [FilledButton(onPressed: close, child: Text(tr('close')))],
            );
          default:
            // Nothing to show any more (e.g. cancelled elsewhere).
            WidgetsBinding.instance.addPostFrameCallback((_) {
              if (context.mounted && ModalRoute.of(context)?.isCurrent == true) Navigator.of(context).pop();
            });
            return const SizedBox.shrink();
        }
      },
    );
  }
}

/// Release notes are Markdown with hard-wrapped lines; show them as plain
/// text: headings and list items on their own lines, wrapped lines joined.
String _plain(String md) {
  final out = StringBuffer();
  var inParagraph = false;
  for (final raw in md.replaceAll('\r\n', '\n').split('\n')) {
    final line = raw.trim().replaceAll('**', '').replaceAll('`', '').replaceAll('*', '');
    if (line.isEmpty) {
      if (out.isNotEmpty) out.write('\n\n');
      inParagraph = false;
      continue;
    }
    final heading = RegExp(r'^#+\s*').firstMatch(line);
    final item = RegExp(r'^[-]\s+').firstMatch(line);
    final indent = raw.length - raw.trimLeft().length;
    if (heading != null || item != null) {
      if (inParagraph) out.write('\n');
      if (item != null) out.write(indent >= 2 ? '    – ' : '• ');
      out.write(line.substring((heading ?? item)!.end));
      inParagraph = item != null; // a heading stands alone
      if (heading != null) out.write('\n');
    } else {
      if (inParagraph) out.write(' ');
      out.write(line);
      inParagraph = true;
    }
  }
  return out.toString().replaceAll(RegExp(r'\n{3,}'), '\n\n').trim();
}
