import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// App updates straight from GitHub Releases (VegamShare is not in a store).
///
/// Read the latest release through GitHub's public API, compare its tag with
/// the installed version, download the asset for this platform and hand it
/// to the system installer. On Android the installer only accepts an APK
/// signed with the same key as the installed app.
///
/// This is the only thing VegamShare ever sends to the internet: one request
/// to api.github.com when the user taps "Check for updates", or at most once
/// a day while the app is open if the automatic check is switched on (off by
/// default). The downloaded installer is deleted once it has been handed
/// over, or on the next start at the latest.
const _latestUrl = 'https://api.github.com/repos/angga-syah/vegamshare/releases/latest';
const releasesPage = 'https://github.com/angga-syah/vegamshare/releases';
const _native = MethodChannel('vegamshare/native');

class UpdateInfo {
  UpdateInfo(this.version, this.notes, this.url, this.name, this.size);
  final String version;
  final String notes;
  final String url;
  final String name;
  final int size;
}

enum UpdateStage { idle, checking, upToDate, available, downloading, needsPermission, installing, failed }

/// One instance for the whole app, so the automatic dialog and the
/// Settings entry share the same state.
class Updater extends ChangeNotifier {
  UpdateStage stage = UpdateStage.idle;
  UpdateInfo? info;
  double progress = 0;
  String? error;
  String installed = '';
  bool autoCheck = false;
  File? _file;
  HttpClient? _http;

  /// Fires when an update should be offered on screen (automatic check).
  final StreamController<UpdateInfo> offers = StreamController<UpdateInfo>.broadcast();

  static bool get supported => Platform.isAndroid || Platform.isWindows;

  Future<void> init() async {
    installed = (await PackageInfo.fromPlatform()).version;
    autoCheck = (await SharedPreferences.getInstance()).getBool(_kAuto) ?? false;
    notifyListeners();
    // Whatever an earlier update left behind (the Windows installer cannot
    // be deleted while it runs; it restarts VegamShare when done).
    try {
      final dir = await _dir();
      if (await dir.exists()) await dir.delete(recursive: true);
    } catch (_) {}
  }

  static Future<Directory> _dir() async =>
      Directory('${(await getTemporaryDirectory()).path}${Platform.pathSeparator}updates');

  Future<void> setAutoCheck(bool on) async {
    autoCheck = on;
    notifyListeners();
    await (await SharedPreferences.getInstance()).setBool(_kAuto, on);
  }

  void _set(UpdateStage s, {String? err}) {
    stage = s;
    error = err;
    notifyListeners();
  }

  /// When the app opens: at most once a day, silent when offline. A version
  /// the user postponed with "Later" is not offered again automatically.
  Future<void> autoCheckIfDue() async {
    if (!supported || !autoCheck || stage != UpdateStage.idle) return;
    final prefs = await SharedPreferences.getInstance();
    final today = DateTime.now().toUtc().millisecondsSinceEpoch ~/ Duration.millisecondsPerDay;
    if (prefs.getInt(_kLastDay) == today) return;
    final UpdateInfo? found;
    try {
      found = await _fetchLatest();
    } catch (_) {
      return; // offline or GitHub unreachable: try again next time the app opens
    }
    await prefs.setInt(_kLastDay, today);
    if (found == null || found.version == prefs.getString(_kSkipped) || stage != UpdateStage.idle) return;
    info = found;
    _set(UpdateStage.available);
    offers.add(found);
  }

  /// "Check for updates" in Settings.
  Future<void> checkNow() async {
    if (stage == UpdateStage.checking || stage == UpdateStage.downloading) return;
    _set(UpdateStage.checking);
    try {
      info = await _fetchLatest();
      _set(info == null ? UpdateStage.upToDate : UpdateStage.available);
      if (info != null) offers.add(info!);
    } catch (e) {
      _set(UpdateStage.failed, err: 'check');
    }
  }

  Future<void> later() async {
    final v = info?.version;
    if (v != null) await (await SharedPreferences.getInstance()).setString(_kSkipped, v);
    if (stage == UpdateStage.available) _set(UpdateStage.idle);
  }

  Future<void> download() async {
    final i = info;
    if (i == null || stage == UpdateStage.downloading) return;
    progress = 0;
    _set(UpdateStage.downloading);
    try {
      final dir = await _dir();
      if (await dir.exists()) await dir.delete(recursive: true);
      await dir.create(recursive: true);
      final file = File('${dir.path}${Platform.pathSeparator}${i.name}');
      final http = HttpClient()..userAgent = 'VegamShare';
      _http = http;
      final res = await (await http.getUrl(Uri.parse(i.url))).close();
      if (res.statusCode != 200) throw HttpException('HTTP ${res.statusCode}');
      final total = i.size > 0 ? i.size : res.contentLength;
      final sink = file.openWrite();
      var done = 0;
      try {
        await for (final chunk in res) {
          sink.add(chunk);
          done += chunk.length;
          if (total > 0) {
            final p = (done / total).clamp(0.0, 1.0);
            if (p - progress >= 0.01 || p == 1) {
              progress = p;
              notifyListeners();
            }
          }
        }
      } finally {
        await sink.close();
      }
      // A file of the wrong size is broken (or not what the release lists).
      if (i.size > 0 && await file.length() != i.size) {
        await file.delete();
        throw const FileSystemException('size mismatch');
      }
      _file = file;
      await install();
    } catch (e) {
      if (stage == UpdateStage.downloading) _set(UpdateStage.failed, err: 'download');
    } finally {
      _http = null;
    }
  }

  void cancelDownload() {
    _http?.close(force: true);
    _http = null;
    _set(info == null ? UpdateStage.idle : UpdateStage.available);
  }

  /// Hands the downloaded file to the system installer.
  Future<void> install() async {
    final f = _file;
    if (f == null) return;
    if (!await f.exists()) {
      // Already handed to the installer and deleted: fetch it again.
      _file = null;
      _set(UpdateStage.available);
      return download();
    }
    try {
      if (Platform.isAndroid) {
        // The installer session keeps its own copy, so the APK is deleted
        // right after it has been handed over ("ok").
        final r = await _native.invokeMethod<String>('installUpdate', f.path);
        if (r == 'ok') {
          try {
            await f.delete();
          } catch (_) {}
        }
        _set(r == 'permission' ? UpdateStage.needsPermission : UpdateStage.installing);
      } else if (Platform.isWindows) {
        // The installer needs admin rights: "start" goes through the shell,
        // which shows the UAC prompt. The installer closes VegamShare
        // itself and starts it again when done.
        await Process.start('cmd.exe', ['/c', 'start', '', f.path], mode: ProcessStartMode.detached);
        _set(UpdateStage.installing);
      }
    } catch (e) {
      _set(UpdateStage.failed, err: 'install');
    }
  }

  /// Back from the "install unknown apps" screen: continue if it was allowed.
  void onResume() {
    if (stage == UpdateStage.needsPermission) install();
  }

  void dismiss() {
    if (stage == UpdateStage.downloading) return;
    if (stage != UpdateStage.available) _set(UpdateStage.idle);
  }

  Future<UpdateInfo?> _fetchLatest() async {
    final http = HttpClient()
      ..userAgent = 'VegamShare'
      ..connectionTimeout = const Duration(seconds: 15);
    try {
      final req = await http.getUrl(Uri.parse(_latestUrl));
      req.headers.set(HttpHeaders.acceptHeader, 'application/vnd.github+json');
      final res = await req.close().timeout(const Duration(seconds: 20));
      if (res.statusCode == 404) return null; // no published release yet
      if (res.statusCode != 200) throw HttpException('HTTP ${res.statusCode}');
      final body = await res.transform(utf8.decoder).join().timeout(const Duration(seconds: 20));
      final release = jsonDecode(body) as Map<String, dynamic>;
      final tag = (release['tag_name'] as String? ?? '').replaceFirst(RegExp('^[vV]'), '');
      if (tag.isEmpty || compareVersions(tag, installed) <= 0) return null;
      for (final a in (release['assets'] as List? ?? const []).cast<Map<String, dynamic>>()) {
        final name = a['name'] as String? ?? '';
        if (_isForThisPlatform(name)) {
          return UpdateInfo(tag, (release['body'] as String? ?? '').trim(), a['browser_download_url'] as String, name, (a['size'] as num?)?.toInt() ?? -1);
        }
      }
      return null; // this release has nothing for this platform (e.g. Android-only)
    } finally {
      http.close(force: true);
    }
  }

  static bool _isForThisPlatform(String name) {
    final n = name.toLowerCase();
    if (Platform.isAndroid) return n.endsWith('.apk');
    if (Platform.isWindows) return n.endsWith('.exe') && n.contains('setup');
    return false;
  }

  static const _kAuto = 'update_auto_check';
  static const _kLastDay = 'update_last_check_day';
  static const _kSkipped = 'update_skipped_version';
}

/// "1.2.10" vs "1.2.9", number by number. Positive means a is newer.
int compareVersions(String a, String b) {
  List<int> parts(String v) => v.split(RegExp(r'[.+-]')).map((p) => int.tryParse(p) ?? 0).toList();
  final pa = parts(a), pb = parts(b);
  for (var i = 0; i < (pa.length > pb.length ? pa.length : pb.length); i++) {
    final d = (i < pa.length ? pa[i] : 0) - (i < pb.length ? pb[i] : 0);
    if (d != 0) return d;
  }
  return 0;
}

final updater = Updater();
