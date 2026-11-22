import 'dart:async';
import 'dart:io';
import 'dart:isolate';

import 'package:flutter/foundation.dart';
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
  WaFile(this.path, this.size, this.modified, this.tab, this.sent, [this.id = -1]);
  final String path;
  final int id; // MediaStore row, -1 when found by walking the folders
  final int size;
  final int modified; // ms since epoch
  final WaTab tab;
  final bool sent;
  String get name => path.substring(path.lastIndexOf('/') + 1);
  String get ext => name.contains('.') ? name.substring(name.lastIndexOf('.') + 1).toLowerCase() : '';
  bool get isImage => _imageExt.contains(ext);
  bool get isPdf => ext == 'pdf';
  bool get isVideo => _videoExt.contains(ext);
}

const _videoExt = {'mp4', '3gp', 'mkv', 'webm', 'mov'};

/// Thumbnails made natively, kept for this app run (bounded).
final _thumbs = <String, Future<Uint8List?>>{};

String _thumbKey(WaFile f, int width) => '${f.path}|$width|${f.modified}';

Future<Uint8List?> _thumb(WaFile f, int width) {
  final key = _thumbKey(f, width);
  if (_thumbs.length > 500) _thumbs.clear();
  return _thumbs[key] ??= () async {
    if (f.isPdf) return pdfPage(f.path, 0, width);
    // The gallery's cached thumbnail is far cheaper than decoding the file.
    if (f.id >= 0) {
      final t = await mediaThumb(f.id, f.isVideo, width);
      if (t != null) return t;
    }
    return f.isVideo ? videoFrame(f.path, width) : null;
  }();
}

/// PDF page, photo or video thumbnail. [loading] until it is there; when
/// there is none, [fallback] (defaults to [loading]).
/// Asks only once the tile stayed on screen for a moment: a fast fling would
/// otherwise queue hundreds of thumbnails the user never sees.
class _NativeThumb extends StatefulWidget {
  const _NativeThumb({required this.file, required this.width, required this.loading, this.fallback});
  final WaFile file;
  final int width;
  final Widget loading;
  final Widget? fallback;

  @override
  State<_NativeThumb> createState() => _NativeThumbState();
}

class _NativeThumbState extends State<_NativeThumb> {
  Future<Uint8List?>? _future;
  Timer? _wait;

  @override
  void initState() {
    super.initState();
    _request();
  }

  @override
  void didUpdateWidget(_NativeThumb old) {
    super.didUpdateWidget(old);
    if (_thumbKey(old.file, old.width) != _thumbKey(widget.file, widget.width)) _request();
  }

  void _request() {
    _wait?.cancel();
    final cached = _thumbs[_thumbKey(widget.file, widget.width)];
    if (cached != null) {
      _future = cached;
      return;
    }
    _future = null;
    _wait = Timer(const Duration(milliseconds: 150), () {
      if (mounted) setState(() => _future = _thumb(widget.file, widget.width));
    });
  }

  @override
  void dispose() {
    _wait?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final fallback = widget.fallback;
    final loading = widget.loading;
    if (_future == null) return loading;
    return FutureBuilder<Uint8List?>(
      future: _future,
      builder: (context, snap) => snap.data != null
          ? Image.memory(snap.data!, fit: BoxFit.cover, alignment: Alignment.topCenter, gaplessPlayback: true)
          : snap.connectionState == ConnectionState.done
          ? fallback ?? loading
          : loading,
    );
  }
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

/// Tab and "sent" flag of a file under a WhatsApp root, null for files the
/// page leaves out (stickers, private, hidden, unfinished downloads).
(WaTab, bool)? _classify(String path) {
  for (final root in _roots) {
    if (!path.startsWith('$root/')) continue;
    final parts = path.substring(root.length + 1).split('/');
    if (parts.length < 2 || parts.last.startsWith('.')) return null;
    final tab = _tabOf(parts.first);
    if (tab == null) return null;
    final sub = parts.length > 2 ? parts[1] : '';
    if (sub == 'Private') return null;
    return (tab, sub == 'Sent');
  }
  return null;
}

/// MediaStore rows (parallel lists from [waFilesIndexed]) as page entries.
List<WaFile> _fromIndex(Map<Object?, Object?> m) {
  final ids = (m['ids'] as List).cast<int>();
  final paths = (m['paths'] as List).cast<String>();
  final sizes = (m['sizes'] as List).cast<int>();
  final mtimes = (m['mtimes'] as List).cast<int>();
  final out = <WaFile>[];
  for (var i = 0; i < paths.length; i++) {
    final c = _classify(paths[i]);
    if (c != null) out.add(WaFile(paths[i], sizes[i], mtimes[i], c.$1, c.$2, ids[i]));
  }
  return out;
}

/// Runs in a background isolate: WhatsApp folders can hold thousands of files,
/// and each stat goes through Android's slow storage layer. Sends batches as
/// it goes so the page fills in right away. With sentOnly it walks just the
/// "Sent" folders, which MediaStore indexes only in part (WhatsApp hides them
/// with .nomedia).
void _scan((SendPort, bool) args) {
  final (port, sentOnly) = args;
  var batch = <WaFile>[];
  var last = DateTime.now();
  void flush() {
    if (batch.isEmpty) return;
    port.send(batch);
    batch = <WaFile>[];
    last = DateTime.now();
  }

  final seen = <String>{};
  final roots = _roots;
  for (final root in roots) {
    final dir = Directory(root);
    final List<FileSystemEntity> tops;
    try {
      if (!dir.existsSync() || !seen.add(dir.resolveSymbolicLinksSync())) continue;
      tops = dir.listSync(followLinks: false);
    } catch (_) {
      continue;
    }
    for (final top in tops) {
      if (top is! Directory) continue;
      final tab = _tabOf(top.path.substring(top.path.lastIndexOf('/') + 1));
      if (tab == null) continue;
      final walk = sentOnly ? Directory('${top.path}/Sent') : top;
      try {
        if (sentOnly && !walk.existsSync()) continue;
        for (final e in walk.listSync(recursive: true, followLinks: false)) {
          if (e is! File) continue;
          final rel = e.path.substring(top.path.length + 1);
          final name = rel.substring(rel.lastIndexOf('/') + 1);
          if (name.startsWith('.')) continue; // .nomedia and unfinished downloads
          final first = rel.split('/').first;
          if (first == 'Private') continue;
          try {
            final st = e.statSync();
            batch.add(WaFile(e.path, st.size, st.modified.millisecondsSinceEpoch, tab, first == 'Sent'));
            if (DateTime.now().difference(last).inMilliseconds > 250) flush();
          } catch (_) {}
        }
      } catch (_) {}
    }
  }
  flush();
}

/// Last scan, kept for this app run so reopening the page is instant.
List<WaFile>? _cache;

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

  bool _scanning = false;

  Future<void> _load() async {
    if (_scanning) return;
    if (!await hasStoragePermission()) {
      if (mounted) setState(() => _permission = false);
      return;
    }
    // Show the previous scan at once; the fresh one replaces it when done.
    setState(() {
      _permission = true;
      _scanning = true;
      _all = _cache;
    });
    final sw = Stopwatch()..start();
    // Android 10+: MediaStore has nearly all of it indexed; only "Sent" is walked.
    final index = await waFilesIndexed();
    final fresh = index == null ? <WaFile>[] : await compute(_fromIndex, index);
    if (index != null) {
      debugPrint('WhatsApp index: ${fresh.length} files in ${sw.elapsedMilliseconds} ms');
      if (mounted) setState(() => _all = _sorted(fresh));
    }
    final known = {for (final f in fresh) f.path};
    final port = ReceivePort();
    // onExit sends null, also when the scan dies on an error.
    await Isolate.spawn(_scan, (port.sendPort, index != null), onExit: port.sendPort);
    await for (final msg in port) {
      if (msg == null) break;
      fresh.addAll((msg as List<WaFile>).where((f) => known.add(f.path)));
      if (_cache == null && index == null && mounted) setState(() => _all = _sorted(fresh));
    }
    port.close();
    debugPrint('WhatsApp files: ${fresh.length} in ${sw.elapsedMilliseconds} ms');
    _cache = _sorted(fresh);
    if (mounted) {
      setState(() {
        _all = _cache;
        _scanning = false;
      });
    }
  }

  static List<WaFile> _sorted(List<WaFile> files) => [...files]..sort((a, b) => b.modified.compareTo(a.modified));

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
    if (f.isPdf) {
      Navigator.of(context).push(
        MaterialPageRoute(
          builder: (_) => _PdfView(
            file: f,
            onSend: () {
              Navigator.of(context)
                ..pop()
                ..pop([f.path]);
            },
            onSelect: () => _toggle(f),
          ),
        ),
      );
      return;
    }
    if (!f.isImage) {
      OpenFilex.open(f.path); // documents, audio, video: the phone's own viewer
      return;
    }
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => _ImageView(
          file: f,
          onSend: () {
            Navigator.of(context)
              ..pop()
              ..pop([f.path]);
          },
          onSelect: () => _toggle(f),
        ),
      ),
    );
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
          bottom: TabBar(
            tabs: [
              Tab(icon: const Icon(Icons.description_outlined), text: tr('wa_docs')),
              Tab(icon: const Icon(Icons.photo_library_outlined), text: tr('wa_media')),
            ],
          ),
        ),
        body: !_permission
            ? Center(
                child: Padding(
                  padding: const EdgeInsets.all(24),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(tr('storage_permission'), textAlign: TextAlign.center),
                      const SizedBox(height: 16),
                      FilledButton(onPressed: _grant, child: Text(tr('grant'))),
                    ],
                  ),
                ),
              )
            : Column(
                children: [
                  if (_scanning && _all != null) const LinearProgressIndicator(minHeight: 2),
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
                ],
              ),
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

  Widget _empty() => _scanning
      ? const Center(child: CircularProgressIndicator())
      : EmptyHint(icon: Icons.chat_outlined, text: tr(_all!.isEmpty ? 'wa_none' : 'wa_none_here'));

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
    final dpr = MediaQuery.devicePixelRatioOf(context);
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
            leading: SizedBox(width: 48, height: 48, child: _docLeading(f, scheme, dpr)),
            title: Text(f.name, maxLines: 2, overflow: TextOverflow.ellipsis),
            subtitle: Text(
              [
                formatBytes(f.size),
                if (f.ext.isNotEmpty) f.ext.toUpperCase(),
                '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}',
                if (f.sent) tr('wa_sent_tag'),
              ].join(' · '),
            ),
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
            child: Stack(
              fit: StackFit.expand,
              children: [
                if (f.isImage)
                  _NativeThumb(
                    file: f,
                    width: (130 * dpr).round(),
                    loading: ColoredBox(color: scheme.surfaceContainerHighest),
                    fallback: Image.file(
                      File(f.path),
                      fit: BoxFit.cover,
                      cacheWidth: (130 * dpr).round(),
                      errorBuilder: (_, _, _) =>
                          ColoredBox(color: scheme.surfaceContainerHighest, child: const Icon(Icons.broken_image_outlined)),
                    ),
                  )
                else ...[
                  _NativeThumb(
                    file: f,
                    width: (130 * dpr).round(),
                    loading: ColoredBox(color: scheme.surfaceContainerHighest),
                  ),
                  Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      const Icon(Icons.play_circle_outline, size: 36, color: Colors.white, shadows: [Shadow(blurRadius: 4)]),
                      const SizedBox(height: 4),
                      Text(
                        formatBytes(f.size),
                        style: const TextStyle(fontSize: 11, color: Colors.white, shadows: [Shadow(blurRadius: 3)]),
                      ),
                    ],
                  ),
                ],
                if (selected) ColoredBox(color: scheme.primary.withValues(alpha: 0.35)),
                Positioned(
                  right: 4,
                  top: 4,
                  child: GestureDetector(
                    onTap: () => _toggle(f),
                    child: Icon(
                      selected ? Icons.check_circle : Icons.radio_button_unchecked,
                      color: selected ? scheme.primary : Colors.white,
                      shadows: const [Shadow(blurRadius: 3)],
                    ),
                  ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }

  /// Photos and PDFs show a small preview, everything else its type icon.
  Widget _docLeading(WaFile f, ColorScheme scheme, double dpr) {
    final icon = Center(child: Icon(_docIcon(f.ext), size: 30, color: scheme.primary));
    if (!f.isImage && !f.isPdf) return icon;
    return ClipRRect(
      borderRadius: BorderRadius.circular(4),
      child: f.isImage
          ? _NativeThumb(
              file: f,
              width: (48 * dpr).round(),
              loading: icon,
              fallback: Image.file(File(f.path), fit: BoxFit.cover, cacheWidth: (48 * dpr).round(), errorBuilder: (_, _, _) => icon),
            )
          : _NativeThumb(file: f, width: (48 * dpr).round(), loading: icon),
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

/// Full PDF preview: pages rendered one by one as they scroll into view,
/// each can be pinch-zoomed. Send or select it right from here.
class _PdfView extends StatefulWidget {
  const _PdfView({required this.file, required this.onSend, required this.onSelect});
  final WaFile file;
  final VoidCallback onSend;
  final VoidCallback onSelect;

  @override
  State<_PdfView> createState() => _PdfViewState();
}

class _PdfViewState extends State<_PdfView> {
  late final Future<int?> _count = pdfPageCount(widget.file.path);
  final _pages = <int, Future<Uint8List?>>{};

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final width = (MediaQuery.sizeOf(context).width * MediaQuery.devicePixelRatioOf(context)).round().clamp(320, 1600);
    return Scaffold(
      backgroundColor: scheme.surfaceContainerHighest,
      appBar: AppBar(
        title: FutureBuilder<int?>(
          future: _count,
          builder: (context, snap) => Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(widget.file.name, style: const TextStyle(fontSize: 14), maxLines: 1, overflow: TextOverflow.ellipsis),
              Text(
                [
                  formatBytes(widget.file.size),
                  if (snap.data != null) tr('wa_pages', {'n': snap.data!}),
                ].join(' · '),
                style: const TextStyle(fontSize: 12),
              ),
            ],
          ),
        ),
        actions: [
          IconButton(tooltip: tr('wa_open_with'), icon: const Icon(Icons.open_in_new), onPressed: () => OpenFilex.open(widget.file.path)),
          IconButton(
            tooltip: tr('wa_select'),
            icon: const Icon(Icons.check_box_outlined),
            onPressed: () {
              widget.onSelect();
              Navigator.of(context).pop();
            },
          ),
        ],
      ),
      body: FutureBuilder<int?>(
        future: _count,
        builder: (context, snap) {
          if (snap.connectionState != ConnectionState.done) return const Center(child: CircularProgressIndicator());
          final n = snap.data ?? 0;
          if (n == 0) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(tr('wa_no_preview'), textAlign: TextAlign.center),
                    const SizedBox(height: 16),
                    FilledButton(onPressed: () => OpenFilex.open(widget.file.path), child: Text(tr('wa_open_with'))),
                  ],
                ),
              ),
            );
          }
          return ListView.builder(
            padding: const EdgeInsets.fromLTRB(8, 8, 8, 96),
            itemCount: n,
            itemBuilder: (context, i) => Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: FutureBuilder<Uint8List?>(
                future: _pages[i] ??= pdfPage(widget.file.path, i, width),
                builder: (context, page) => page.data == null
                    ? AspectRatio(
                        aspectRatio: 1 / 1.414, // A4 until the page is drawn
                        child: ColoredBox(
                          color: Colors.white,
                          child: page.connectionState == ConnectionState.done
                              ? const Icon(Icons.broken_image_outlined)
                              : const Center(child: CircularProgressIndicator()),
                        ),
                      )
                    : InteractiveViewer(maxScale: 5, child: Image.memory(page.data!, gaplessPlayback: true)),
              ),
            ),
          );
        },
      ),
      floatingActionButton: FloatingActionButton.extended(onPressed: widget.onSend, icon: const Icon(Icons.send), label: Text(tr('send'))),
    );
  }
}
