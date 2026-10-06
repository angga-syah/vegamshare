import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../src/rust/api/vegam.dart' as core;
import '../util/l10n.dart';
import 'file_browser_page.dart';

IconData platformIcon(String platform) => switch (platform) {
      'android' => Icons.smartphone,
      'windows' => Icons.laptop_windows,
      _ => Icons.devices_other,
    };

class StatusDot extends StatelessWidget {
  const StatusDot({super.key, required this.online, this.size = 10});
  final bool online;
  final double size;

  @override
  Widget build(BuildContext context) {
    final c = online ? const Color(0xFF2E7D32) : Theme.of(context).colorScheme.outline;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(color: c, shape: BoxShape.circle),
    );
  }
}

void showSnack(BuildContext context, String text, {SnackBarAction? action}) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(text), action: action, behavior: SnackBarBehavior.floating));
}

Future<void> sendToDevice(BuildContext context, core.Device device, List<String> paths) async {
  if (paths.isEmpty) return;
  try {
    await core.sendPaths(fingerprint: device.fingerprint, paths: paths);
    if (context.mounted) showSnack(context, tr('sending_queued', {'name': device.displayName}));
  } catch (e) {
    if (context.mounted) showSnack(context, describeError(e));
  }
}

Future<void> pickAndSend(BuildContext context, core.Device device, {required bool folder}) async {
  final paths = folder ? await pickFolder(context) : await pickFiles(context);
  if (context.mounted) await sendToDevice(context, device, paths);
}

/// Native picker on Windows; on Android an in-app browser that returns
/// real paths (the system picker would copy every file into the cache).
Future<List<String>> pickFiles(BuildContext context) async {
  if (Platform.isAndroid) {
    return await Navigator.of(context).push<List<String>>(MaterialPageRoute(builder: (_) => const FileBrowserPage())) ?? [];
  }
  final r = await FilePicker.platform.pickFiles(allowMultiple: true, lockParentWindow: true);
  return r?.paths.whereType<String>().toList() ?? [];
}

Future<List<String>> pickFolder(BuildContext context) async {
  if (Platform.isAndroid) {
    return await Navigator.of(context).push<List<String>>(MaterialPageRoute(builder: (_) => const FileBrowserPage(foldersOnly: true))) ?? [];
  }
  final d = await FilePicker.platform.getDirectoryPath(lockParentWindow: true);
  return d == null ? [] : [d];
}

class SectionHeader extends StatelessWidget {
  const SectionHeader(this.text, {super.key, this.trailing});
  final String text;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 20, 4, 8),
      child: Row(children: [
        Expanded(
          child: Text(text,
              style: Theme.of(context).textTheme.titleSmall?.copyWith(color: Theme.of(context).colorScheme.primary, fontWeight: FontWeight.w600)),
        ),
        ?trailing,
      ]),
    );
  }
}

class EmptyHint extends StatelessWidget {
  const EmptyHint({super.key, required this.icon, required this.text});
  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    final c = Theme.of(context).colorScheme.onSurfaceVariant;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 24, horizontal: 16),
      child: Row(children: [
        Icon(icon, color: c),
        const SizedBox(width: 16),
        Expanded(child: Text(text, style: TextStyle(color: c))),
      ]),
    );
  }
}

/// Constrains page content on wide windows so it does not stretch.
class PageBody extends StatelessWidget {
  const PageBody({super.key, required this.children});
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.topCenter,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 760),
        child: ListView(padding: const EdgeInsets.fromLTRB(16, 8, 16, 96), children: children),
      ),
    );
  }
}
