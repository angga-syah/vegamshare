import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';
import 'package:launch_at_startup/launch_at_startup.dart';
import 'package:open_filex/open_filex.dart';
import 'package:path_provider/path_provider.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:tray_manager/tray_manager.dart' as tm;
import 'package:window_manager/window_manager.dart';

import '../util/l10n.dart';

/// Paths handed to the app from outside: Android "Share to", Explorer
/// "Send to", or command-line arguments.
final StreamController<List<String>> externalPaths = StreamController<List<String>>.broadcast();
List<String> pendingExternalPaths = [];

void _deliver(List<String> paths) {
  final clean = paths.where((p) => p.isNotEmpty).toList();
  if (clean.isEmpty) return;
  if (externalPaths.hasListener) {
    externalPaths.add(clean);
  } else {
    pendingExternalPaths = clean;
  }
}

class Dirs {
  Dirs(this.data, this.downloads);
  final String data;
  final String downloads;
}

Future<Dirs> resolveDirs() async {
  final support = await getApplicationSupportDirectory();
  String downloads;
  if (Platform.isAndroid) {
    downloads = '/storage/emulated/0/Download/VegamShare';
  } else {
    final d = await getDownloadsDirectory();
    downloads = '${(d ?? support).path}${Platform.pathSeparator}VegamShare';
  }
  return Dirs(support.path, downloads);
}

String defaultDeviceName() {
  if (Platform.isWindows) return Platform.environment['COMPUTERNAME'] ?? 'Windows PC';
  if (Platform.isAndroid) return 'Android';
  return Platform.localHostname;
}

String platformName() => Platform.isAndroid ? 'android' : Platform.isWindows ? 'windows' : Platform.operatingSystem;

// ---------------------------------------------------------------- Android

const _native = MethodChannel('vegamshare/native');

/// The device model name, e.g. "Samsung SM-A546E", for a friendlier default.
Future<String?> androidModelName() async {
  try {
    return await _native.invokeMethod<String>('deviceName');
  } catch (_) {
    return null;
  }
}

Future<bool> hasStoragePermission() async {
  if (!Platform.isAndroid) return true;
  return Permission.manageExternalStorage.isGranted;
}

Future<bool> requestStoragePermission() async {
  if (!Platform.isAndroid) return true;
  return (await Permission.manageExternalStorage.request()).isGranted;
}

@pragma('vm:entry-point')
void foregroundCallback() {
  FlutterForegroundTask.setTaskHandler(_KeepAliveHandler());
}

/// The service only exists to keep the process (and the Rust listener in
/// it) alive while the app is in the background; it does no work itself.
class _KeepAliveHandler extends TaskHandler {
  @override
  Future<void> onStart(DateTime timestamp, TaskStarter starter) async {}

  @override
  void onRepeatEvent(DateTime timestamp) {}

  @override
  Future<void> onDestroy(DateTime timestamp, bool isTimeout) async {}
}

Future<void> setBackgroundService(bool on) async {
  if (!Platform.isAndroid) return;
  if (!on) {
    if (await FlutterForegroundTask.isRunningService) {
      await FlutterForegroundTask.stopService();
    }
    return;
  }
  if (await FlutterForegroundTask.checkNotificationPermission() != NotificationPermission.granted) {
    await FlutterForegroundTask.requestNotificationPermission();
  }
  if (!await FlutterForegroundTask.isIgnoringBatteryOptimizations) {
    await FlutterForegroundTask.requestIgnoreBatteryOptimization();
  }
  FlutterForegroundTask.init(
    androidNotificationOptions: AndroidNotificationOptions(
      channelId: 'vegamshare_ready',
      channelName: 'VegamShare',
      channelDescription: tr('ready_notification'),
    ),
    iosNotificationOptions: const IOSNotificationOptions(),
    foregroundTaskOptions: ForegroundTaskOptions(
      eventAction: ForegroundTaskEventAction.nothing(),
      // No locks while idle: the app only waits for a knock. Locks are held
      // during transfers only (see holdTransferLocks).
      allowWakeLock: false,
      allowWifiLock: false,
    ),
  );
  if (!await FlutterForegroundTask.isRunningService) {
    await FlutterForegroundTask.startService(
      serviceTypes: [ForegroundServiceTypes.dataSync],
      notificationTitle: 'VegamShare',
      notificationText: tr('ready_notification'),
      callback: foregroundCallback,
    );
  }
}

/// Files shared to VegamShare from other apps ("Share to"). MainActivity
/// resolves the URIs to readable paths.
/// Keeps CPU and Wi-Fi awake while a transfer runs (Android); released as
/// soon as nothing is moving.
Future<void> holdTransferLocks(bool on) async {
  if (!Platform.isAndroid) return;
  try {
    await _native.invokeMethod('holdLocks', on);
  } catch (_) {}
}

Future<void> initShareIntents() async {
  if (!Platform.isAndroid) return;
  _native.setMethodCallHandler((call) async {
    if (call.method == 'sharedPaths') _deliver(List<String>.from(call.arguments as List));
  });
  try {
    final initial = await _native.invokeListMethod<String>('takeSharedPaths');
    if (initial != null) _deliver(initial);
  } catch (_) {}
}

// ---------------------------------------------------------------- Windows

const _shell = MethodChannel('vegamshare/shell');

class _TrayHandler with tm.TrayListener {
  _TrayHandler(this.onQuit);
  final Future<void> Function() onQuit;

  @override
  void onTrayIconMouseDown() => _showWindow();

  @override
  void onTrayIconRightMouseDown() => tm.trayManager.popUpContextMenu();

  @override
  void onTrayMenuItemClick(tm.MenuItem menuItem) {
    if (menuItem.key == 'show') _showWindow();
    if (menuItem.key == 'quit') onQuit();
  }
}

class _WindowHandler with WindowListener {
  @override
  void onWindowClose() async {
    // Closing the window keeps VegamShare running in the tray so paired
    // devices can still reach it.
    await windowManager.hide();
  }
}

Future<void> _showWindow() async {
  await windowManager.show();
  await windowManager.focus();
}

Future<void> initDesktop(List<String> args, {required Future<void> Function() onQuit}) async {
  if (!Platform.isWindows) return;
  await windowManager.ensureInitialized();
  final startMinimized = args.contains('--minimized');
  await windowManager.waitUntilReadyToShow(
    const WindowOptions(size: Size(1000, 700), minimumSize: Size(420, 560), center: true, title: 'VegamShare'),
    () async {
      if (!startMinimized) await _showWindow();
    },
  );
  await windowManager.setPreventClose(true);
  windowManager.addListener(_WindowHandler());

  await tm.trayManager.setIcon('assets/tray.ico');
  await tm.trayManager.setToolTip('VegamShare');
  await tm.trayManager.setContextMenu(tm.Menu(items: [
    tm.MenuItem(key: 'show', label: tr('show_window')),
    tm.MenuItem.separator(),
    tm.MenuItem(key: 'quit', label: tr('quit')),
  ]));
  tm.trayManager.addListener(_TrayHandler(onQuit));

  launchAtStartup.setup(appName: 'VegamShare', appPath: Platform.resolvedExecutable, args: ['--minimized']);

  // Paths from Explorer "Send to" or a second launch arrive here.
  _shell.setMethodCallHandler((call) async {
    if (call.method == 'openPaths') {
      await _showWindow();
      _deliver(List<String>.from(call.arguments as List));
    }
  });
  _deliver(args.where((a) => !a.startsWith('--')).toList());
}

Future<bool> startupEnabled() async => Platform.isWindows && await launchAtStartup.isEnabled();

Future<void> setStartup(bool on) async {
  if (!Platform.isWindows) return;
  on ? await launchAtStartup.enable() : await launchAtStartup.disable();
}

Future<void> quitDesktop() async {
  if (!Platform.isWindows) return;
  await tm.trayManager.destroy();
  await windowManager.setPreventClose(false);
  await windowManager.destroy();
}

/// Opens a folder (or the folder containing a file) in the system UI.
Future<void> revealPath(String path) async {
  if (Platform.isWindows) {
    final isDir = await FileSystemEntity.isDirectory(path);
    await Process.run('explorer.exe', isDir ? [path] : ['/select,', path]);
  } else if (Platform.isAndroid) {
    if (await FileSystemEntity.isFile(path)) {
      await OpenFilex.open(path);
      return;
    }
    try {
      await _native.invokeMethod('openPath', path);
    } catch (_) {}
  }
}
