import 'dart:io';
import 'dart:isolate';

import 'package:flutter/material.dart';
import 'package:open_filex/open_filex.dart';

import '../platform/platform_services.dart';
import '../util/format.dart';
import '../util/l10n.dart';
import 'common.dart';

/// Files WhatsApp saved on this phone, in two tabs as in WhatsApp itself:
/// documents (with audio) and photos & videos, newest first. Returns the
/// chosen paths. Needs all-files access: WhatsApp keeps them in Android/media.
class WhatsAppPage extends StatefulWidget {
  const WhatsAppPage({super.key});

  @override
  State<WhatsAppPage> createState() => _WhatsAppPageState();
}

enum WaTab { docs, media }

class WaFile {
  WaFile(this.path, this.size, this.modified, this.tab, this.sent);
  final String path;
  final int size;
  final int modified; // ms since epoch
  final WaTab tab;
  final bool sent;
  String get name => path.substring(path.lastIndexOf('/') + 1);
  String get ext => name.contains('.') ? name.substring(name.lastIndexOf('.') + 1).toLowerCase() : '';
  bool get isImage => _imageExt.contains(ext);
}

const _storage = '/storage/emulated/0';

/// WhatsApp and WhatsApp Business, new location (Android 11+) and old one.
const _roots = [
  '$_storage/Android/media/com.whatsapp/WhatsApp/Media',
  '$_storage/WhatsApp/Media',
  '$_storage/Android/media/com.whatsapp.w4b/WhatsApp Business/Media',
  '$_storage/WhatsApp Business/Media',
];

const _imageExt = {'jpg', 'jpeg', 'png', 'webp', 'gif', 'heic'};

WaTab? _tabOf(String folder) {
  final f = folder.replaceFirst('WhatsApp Business ', '').replaceFirst('WhatsApp ', '');
  return switch (f) {
    'Documents' || 'Audio' || 'Voice Notes' => WaTab.docs,
    'Images' || 'Video' || 'Animated Gifs' => WaTab.media,
    _ => null, // stickers, profile photos, statuses, backups
  };
}

/// Runs in a background isolate: WhatsApp folders can hold thousands of files.
List<WaFile> _scan(List<String> roots) {
  final out = <WaFile>[];
  final seen = <String>{};
  for (final root in roots) {
    final dir = Directory(root);
    if (!dir.existsSync() || !seen.add(dir.resolveSymbolicLinksSync())) continue;
    for (final top in dir.listSync(followLinks: false)) {
      if (top is! Directory) continue;
      final tab = _tabOf(top.path.substring(top.path.lastIndexOf('/') + 1));
      if (tab == null) continue;
      try {
        for (final e in top.listSync(recursive: true, followLinks: false)) {
          if (e is! File) continue;
          final rel = e.path.substring(top.path.length + 1);
          final name = rel.substring(rel.lastIndexOf('/') + 1);
          if (name.startsWith('.')) continue; // .nomedia and unfinished downloads
          final first = rel.split('/').first;
          if (first == 'Private') continue;
          try {
            final st = e.statSync();
            out.add(WaFile(e.path, st.size, st.modified.millisecondsSinceEpoch, tab, first == 'Sent'));
          } catch (_) {}
        }
      } catch (_) {}
    }
  }
  out.sort((a, b) => b.modified.compareTo(a.modified));
  return out;
}

String _dayLabel(int ms) {
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  final now = DateTime.now();
  final diff = DateTime(now.year, now.month, now.day).difference(DateTime(d.year, d.month, d.day)).inDays;
  if (diff == 0) return tr('today');
  if (diff == 1) return tr('yesterday');
  return '${d.day.toString().padLeft(2, '0')}-${d.month.toString().padLeft(2, '0')}-${d.year}';
}

class _WhatsAppPageState extends State<WhatsAppPage> {
  List<WaFile>? _all;
  bool _permission = true;
  bool _showSent = false;
  String _query = '';
  bool _searching = false;
  final Set<String> _selected = {};

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    if (!await hasStoragePermission()) {
      setState(() => _permission = false);
      return;
    }
    final files = await Isolate.run(() => _scan(_roots));
    if (mounted) {
      setState(() {
        _permission = true;
        _all = files;
      });
    }
  }

  Future<void> _grant() async {
    if (await requestStoragePermission()) _load();
  }

  List<WaFile> _visible(WaTab tab) {
    final q = _query.toLowerCase();
    return (_all ?? const <WaFile>[])
        .where((f) => f.tab == tab && (_showSent || !f.sent) && (q.isEmpty || f.name.toLowerCase().contains(q)))
        .toList();
  }

  void _toggle(WaFile f) => setState(() => _selected.contains(f.path) ? _selected.remove(f.path) : _selected.add(f.path));

  void _tap(WaFile f) {
    if (_selected.isNotEmpty) return _toggle(f);
    if (!f.isImage) {
      OpenFilex.open(f.path); // documents, audio, video: the phone's own viewer
      return;
    }
    Navigator.of(context).push(MaterialPageRoute(
      builder: (_) => _ImageView(
        file: f,
        onSend: () {
          Navigator.of(context)
            ..pop()
            ..pop([f.path]);
        },
        onSelect: () => _toggle(f),
      ),
    ));
  }

  @override
  Widget build(BuildContext context) {
    return DefaultTabController(
      length: 2,
      child: Scaffold(
        appBar: AppBar(
          title: _searching
              ? TextField(
                  autofocus: true,
                  decoration: InputDecoration(hintText: tr('wa_search'), border: InputBorder.none),
                  onChanged: (v) => setState(() => _query = v),
                )
              : Text(tr('wa_title')),
          actions: [
            IconButton(
              tooltip: tr('wa_search'),
              icon: Icon(_searching ? Icons.close : Icons.search),
              onPressed: () => setState(() {
                _searching = !_searching;
                _query = '';
              }),
            ),
            if (_selected.isNotEmpty) TextButton(onPressed: () => setState(_selected.clear), child: Text(tr('cancel'))),
          ],
          bottom: TabBar(tabs: [
            Tab(icon: const Icon(Icons.description_outlined), text: tr('wa_docs')),
            Tab(icon: const Icon(Icons.photo_library_outlined), text: tr('wa_media')),
          ]),
        ),
        body: !_permission
            ? Center(
                child: Padding(
                  padding: const EdgeInsets.all(24),
                  child: Column(mainAxisSize: MainAxisSize.min, children: [
                    Text(tr('storage_permission'), textAlign: TextAlign.center),
                    const SizedBox(height: 16),
                    FilledButton(onPressed: _grant, child: Text(tr('grant'))),
                  ]),
                ),
              )
            : Column(children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(12, 8, 12, 4),
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: FilterChip(label: Text(tr('wa_sent')), selected: _showSent, onSelected: (v) => setState(() => _showSent = v)),
                  ),
                ),
                Expanded(
                  child: _all == null
                      ? const Center(child: CircularProgressIndicator())
                      : TabBarView(children: [_docs(context), _media(context)]),
                ),
              ]),
        floatingActionButton: _selected.isEmpty
            ? null
            : FloatingActionButton.extended(
                onPressed: () => Navigator.of(context).pop(_selected.toList()),
                icon: const Icon(Icons.send),
                label: Text('${tr('send')} (${_selected.length})'),
              ),
      ),
    );
  }

  Widget _empty() => EmptyHint(icon: Icons.chat_outlined, text: tr(_all!.isEmpty ? 'wa_none' : 'wa_none_here'));

  /// Documents and audio: a list with day headers, like WhatsApp's Docs tab.
  Widget _docs(BuildContext context) {
    final files = _visible(WaTab.docs);
    if (files.isEmpty) return _empty();
    final rows = <Object>[];
    String? lastDay;
    for (final f in files) {
      final day = _dayLabel(f.modified);
      if (day != lastDay) rows.add(lastDay = day);
      rows.add(f);
    }
    final scheme = Theme.of(context).colorScheme;
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView.builder(
        padding: const EdgeInsets.only(bottom: 96),
        itemCount: rows.length,
        itemBuilder: (context, i) {
          final r = rows[i];
          if (r is String) {
            return Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
              child: Text(r, style: Theme.of(context).textTheme.labelLarge?.copyWith(color: scheme.primary)),
            );
          }
          final f = r as WaFile;
          final t = DateTime.fromMillisecondsSinceEpoch(f.modified);
          final selected = _selected.contains(f.path);
          return ListTile(
            leading: SizedBox(width: 40, child: Icon(_docIcon(f.ext), size: 30, color: scheme.primary)),
            title: Text(f.name, maxLines: 2, overflow: TextOverflow.ellipsis),
            subtitle: Text([
              formatBytes(f.size),
              if (f.ext.isNotEmpty) f.ext.toUpperCase(),
              '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}',
              if (f.sent) tr('wa_sent_tag'),
            ].join(' · ')),
            trailing: Checkbox(value: selected, onChanged: (_) => _toggle(f)),
            selected: selected,
            onTap: () => _tap(f),
            onLongPress: () => _toggle(f),
          );
        },
      ),
    );
  }

  /// Photos and videos: a thumbnail grid, like WhatsApp's Media tab.
  Widget _media(BuildContext context) {
    final files = _visible(WaTab.media);
    if (files.isEmpty) return _empty();
    final scheme = Theme.of(context).colorScheme;
    final dpr = MediaQuery.devicePixelRatioOf(context);
    return RefreshIndicator(
      onRefresh: _load,
      child: GridView.builder(
        padding: const EdgeInsets.fromLTRB(2, 2, 2, 96),
        gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(maxCrossAxisExtent: 130, mainAxisSpacing: 2, crossAxisSpacing: 2),
        itemCount: files.length,
        itemBuilder: (context, i) {
          final f = files[i];
          final selected = _selected.contains(f.path);
          return GestureDetector(
            onTap: () => _tap(f),
            onLongPress: () => _toggle(f),
            child: Stack(fit: StackFit.expand, children: [
              if (f.isImage)
                Image.file(
                  File(f.path),
                  fit: BoxFit.cover,
                  cacheWidth: (130 * dpr).round(),
                  errorBuilder: (_, _, _) => ColoredBox(color: scheme.surfaceContainerHighest, child: const Icon(Icons.broken_image_outlined)),
                )
              else
                ColoredBox(
                  color: scheme.surfaceContainerHighest,
                  child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                    Icon(Icons.play_circle_outline, size: 36, color: scheme.onSurfaceVariant),
                    const SizedBox(height: 4),
                    Text(formatBytes(f.size), style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant)),
                  ]),
                ),
              if (selected) ColoredBox(color: scheme.primary.withValues(alpha: 0.35)),
              Positioned(
                right: 4,
                top: 4,
                child: GestureDetector(
                  onTap: () => _toggle(f),
                  child: Icon(selected ? Icons.check_circle : Icons.radio_button_unchecked,
                      color: selected ? scheme.primary : Colors.white, shadows: const [Shadow(blurRadius: 3)]),
                ),
              ),
            ]),
          );
        },
      ),
    );
  }

  static IconData _docIcon(String ext) => switch (ext) {
        'pdf' => Icons.picture_as_pdf_outlined,
        'doc' || 'docx' || 'odt' || 'txt' || 'rtf' => Icons.description_outlined,
        'xls' || 'xlsx' || 'csv' || 'ods' => Icons.table_chart_outlined,
        'ppt' || 'pptx' => Icons.slideshow_outlined,
        'zip' || 'rar' || '7z' => Icons.folder_zip_outlined,
        'apk' => Icons.android,
        'mp3' || 'm4a' || 'aac' || 'ogg' || 'opus' || 'wav' || 'amr' => Icons.audiotrack_outlined,
        _ => Icons.insert_drive_file_outlined,
      };
}

/// Full-screen photo with zoom; send it right from here.
class _ImageView extends StatelessWidget {
  const _ImageView({required this.file, required this.onSend, required this.onSelect});
  final WaFile file;
  final VoidCallback onSend;
  final VoidCallback onSelect;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.black,
      appBar: AppBar(
        backgroundColor: Colors.black,
        foregroundColor: Colors.white,
        title: Text(file.name, style: const TextStyle(fontSize: 14)),
        actions: [
          IconButton(tooltip: tr('wa_open_with'), icon: const Icon(Icons.open_in_new), onPressed: () => OpenFilex.open(file.path)),
          IconButton(
            tooltip: tr('wa_select'),
            icon: const Icon(Icons.check_box_outlined),
            onPressed: () {
              onSelect();
              Navigator.of(context).pop();
            },
          ),
        ],
      ),
      body: InteractiveViewer(maxScale: 6, child: Center(child: Image.file(File(file.path)))),
      floatingActionButton: FloatingActionButton.extended(onPressed: onSend, icon: const Icon(Icons.send), label: Text(tr('send'))),
    );
  }
}
