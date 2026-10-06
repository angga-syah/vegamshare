import 'dart:io';

import 'package:flutter/material.dart';

import '../util/format.dart';
import '../util/l10n.dart';

/// Android file browser with multi-select of files and folders. Returns
/// the selected absolute paths. Requires all-files access.
class FileBrowserPage extends StatefulWidget {
  const FileBrowserPage({super.key, this.foldersOnly = false});
  final bool foldersOnly;

  @override
  State<FileBrowserPage> createState() => _FileBrowserPageState();
}

const _root = '/storage/emulated/0';

class _FileBrowserPageState extends State<FileBrowserPage> {
  String _dir = _root;
  List<FileSystemEntity> _items = [];
  final Set<String> _selected = {};
  String? _error;

  static const _shortcuts = {
    'Download': Icons.download,
    'DCIM': Icons.photo_camera,
    'Pictures': Icons.image,
    'Movies': Icons.movie,
    'Music': Icons.music_note,
    'Documents': Icons.description,
  };

  @override
  void initState() {
    super.initState();
    _load(_root);
  }

  Future<void> _load(String dir) async {
    try {
      final list = await Directory(dir).list(followLinks: false).toList();
      list.removeWhere((e) => e.path.split('/').last.startsWith('.'));
      if (widget.foldersOnly) list.removeWhere((e) => e is! Directory);
      list.sort((a, b) {
        final ad = a is Directory, bd = b is Directory;
        if (ad != bd) return ad ? -1 : 1;
        return a.path.toLowerCase().compareTo(b.path.toLowerCase());
      });
      setState(() {
        _dir = dir;
        _items = list;
        _error = null;
      });
    } catch (e) {
      setState(() => _error = e.toString());
    }
  }

  void _toggle(String path) => setState(() => _selected.contains(path) ? _selected.remove(path) : _selected.add(path));

  @override
  Widget build(BuildContext context) {
    final rel = _dir == _root ? '' : _dir.substring(_root.length + 1);
    final crumbs = rel.isEmpty ? <String>[] : rel.split('/');
    return PopScope(
      canPop: _dir == _root,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _load(Directory(_dir).parent.path);
      },
      child: Scaffold(
        appBar: AppBar(
          title: Text(widget.foldersOnly ? tr('send_folder') : tr('send_files')),
          actions: [
            if (_selected.isNotEmpty)
              TextButton(onPressed: () => setState(_selected.clear), child: Text(tr('cancel'))),
          ],
        ),
        body: Column(children: [
          SizedBox(
            height: 48,
            child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 8), children: [
              TextButton.icon(onPressed: () => _load(_root), icon: const Icon(Icons.phone_android, size: 18), label: const Text('Internal')),
              for (var i = 0; i < crumbs.length; i++) ...[
                const Icon(Icons.chevron_right, size: 18),
                TextButton(onPressed: () => _load('$_root/${crumbs.sublist(0, i + 1).join('/')}'), child: Text(crumbs[i])),
              ],
            ]),
          ),
          if (_dir == _root)
            SizedBox(
              height: 44,
              child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 8), children: [
                for (final s in _shortcuts.entries)
                  Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: ActionChip(avatar: Icon(s.value, size: 18), label: Text(s.key), onPressed: () => _load('$_root/${s.key}')),
                  ),
              ]),
            ),
          const Divider(height: 1),
          Expanded(
            child: _error != null
                ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(_error!)))
                : ListView.builder(
                    itemCount: _items.length,
                    itemBuilder: (context, i) {
                      final e = _items[i];
                      final name = e.path.split('/').last;
                      final isDir = e is Directory;
                      final selected = _selected.contains(e.path);
                      String sub = '';
                      if (!isDir) {
                        try {
                          sub = formatBytes((e as File).lengthSync());
                        } catch (_) {}
                      }
                      return ListTile(
                        leading: Icon(isDir ? Icons.folder : Icons.insert_drive_file_outlined,
                            color: isDir ? Theme.of(context).colorScheme.primary : null),
                        title: Text(name, maxLines: 1, overflow: TextOverflow.ellipsis),
                        subtitle: sub.isEmpty ? null : Text(sub),
                        trailing: Checkbox(value: selected, onChanged: (_) => _toggle(e.path)),
                        selected: selected,
                        onTap: () => isDir ? _load(e.path) : _toggle(e.path),
                        onLongPress: () => _toggle(e.path),
                      );
                    },
                  ),
          ),
        ]),
        floatingActionButton: _selected.isEmpty
            ? null
            : FloatingActionButton.extended(
                onPressed: () => Navigator.of(context).pop(_selected.toList()),
                icon: const Icon(Icons.send),
                label: Text('${tr('choose')} (${_selected.length})'),
              ),
      ),
    );
  }
}
