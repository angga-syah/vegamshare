import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart' show PlatformInt64Util;
import 'package:flutter_foreground_task/flutter_foreground_task.dart';
import 'package:launch_at_startup/launch_at_startup.dart';
import 'package:open_filex/open_filex.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:tray_manager/tray_manager.dart' as tm;
import 'package:window_manager/window_manager.dart';

import '../src/rust/api/vegam.dart' as core;
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
  // Ask about battery optimisation once; if the user declines, respect it
  // instead of asking on every launch.
  final prefs = await SharedPreferences.getInstance();
  if (!(prefs.getBool('battery_asked') ?? false) && !await FlutterForegroundTask.isIgnoringBatteryOptimizations) {
    await prefs.setBool('battery_asked', true);
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
    if (call.method == 'wifiLost') wifiLost.add(call.arguments as String);
    if (call.method == 'restoreProgress') {
      final m = call.arguments as Map;
      restoreProgress.add(RestoreProgress(m['stage'] as String, m['done'] as int, m['total'] as int));
    }
  });
  try {
    final initial = await _native.invokeListMethod<String>('takeSharedPaths');
    if (initial != null) _deliver(initial);
  } catch (_) {}
}

// ------------------------------------------------------- "No Wi-Fi" mode (Android)

/// "hotspot" when our hotspot stopped, "wifi" when the joined one dropped.
final StreamController<String> wifiLost = StreamController<String>.broadcast();

class HotspotInfo {
  HotspotInfo(this.ssid, this.passphrase, this.wpa3);
  final String ssid;
  final String passphrase;
  final bool wpa3;
}

/// Thrown with a code the UI translates: permission, location_off,
/// unsupported, failed_N, join_failed.
class WifiLinkException implements Exception {
  WifiLinkException(this.code);
  final String code;
  @override
  String toString() => 'WifiLinkException($code)';
}

Future<bool> hotspotSupported() async {
  if (!Platform.isAndroid) return false;
  try {
    return await _native.invokeMethod<bool>('hotspotSupported') ?? false;
  } catch (_) {
    return false;
  }
}

/// Wi-Fi permissions differ by Android version: location (8-12) or
/// "nearby Wi-Fi devices" (13+). Location services must also be on for 8-12.
Future<void> _ensureWifiPermission() async {
  final sdk = await _native.invokeMethod<int>('sdkInt') ?? 0;
  if (sdk >= 33) {
    if (!(await Permission.nearbyWifiDevices.request()).isGranted) throw WifiLinkException('permission');
  } else {
    if (!(await Permission.locationWhenInUse.request()).isGranted) throw WifiLinkException('permission');
    if (!await Permission.locationWhenInUse.serviceStatus.isEnabled) throw WifiLinkException('location_off');
  }
}

Future<HotspotInfo> startHotspot() async {
  await _ensureWifiPermission();
  try {
    final m = await _native.invokeMapMethod<String, dynamic>('startHotspot');
    return HotspotInfo(m!['ssid'] as String, m['passphrase'] as String, m['wpa3'] as bool? ?? false);
  } on PlatformException catch (e) {
    throw WifiLinkException(e.code);
  }
}

Future<void> stopHotspot() async {
  if (Platform.isAndroid) await _native.invokeMethod('stopHotspot');
}

/// Joins the hotspot; returns its gateway address, which on a hotspot is
/// the phone that created it (null if Android did not report one).
Future<String?> joinWifi(String ssid, String passphrase, bool wpa3) async {
  await _ensureWifiPermission();
  try {
    final m = await _native.invokeMapMethod<String, dynamic>('joinWifi', {'ssid': ssid, 'passphrase': passphrase, 'wpa3': wpa3});
    return m?['gateway'] as String?;
  } on PlatformException catch (e) {
    throw WifiLinkException(e.code);
  }
}

Future<void> leaveWifi() async {
  if (Platform.isAndroid) await _native.invokeMethod('leaveWifi');
}

/// The hotspot we run and whether we joined one (survives activity restarts).
Future<(HotspotInfo?, bool)> wifiState() async {
  if (!Platform.isAndroid) return (null, false);
  try {
    final m = await _native.invokeMapMethod<String, dynamic>('wifiState');
    final ssid = m?['hotspotSsid'] as String?;
    final hotspot = ssid == null ? null : HotspotInfo(ssid, m!['hotspotPassphrase'] as String, m['hotspotWpa3'] as bool? ?? false);
    return (hotspot, m?['joined'] as bool? ?? false);
  } catch (_) {
    return (null, false);
  }
}

// ---------------------------------------------------------------- Windows

/// True if Windows treats a connected network as "Public". The installer only
/// opens the firewall for private networks, so other devices cannot reach us.
/// Only adapters that are up and have a default gateway count: VPN, VM
/// host-only and other virtual adapters are often "Public" but irrelevant.
Future<bool> windowsOnPublicNetwork() async {
  if (!Platform.isWindows) return false;
  try {
    final r = await Process.run('powershell.exe', [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      r"(Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway -and $_.NetAdapter.Status -eq 'Up' } | ForEach-Object { $_.NetProfile.NetworkCategory }) -join ','",
    ]);
    return (r.stdout as String).contains('Public');
  } catch (_) {
    return false;
  }
}

Future<void> openWindowsNetworkSettings() async {
  if (Platform.isWindows) await Process.run('explorer.exe', ['ms-settings:network-status']);
}


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

// ---------------------------------------------------------------- Shizuku / Send App (Android)

class InstalledAppInfo {
  InstalledAppInfo({required this.packageName, required this.label, required this.apkPath, required this.splitApkPaths});
  final String packageName;
  final String label;
  final String apkPath;
  final List<String> splitApkPaths;
}

/// Whether Wireless Debugging (Android 11+) is available on this device.
Future<bool> shizukuAvailable() async {
  if (!Platform.isAndroid) return false;
  try {
    return await _native.invokeMethod<bool>('shizukuAvailable') ?? false;
  } catch (_) { return false; }
}

/// Whether the Shizuku privileged service is currently running.
Future<bool> shizukuRunning() async {
  if (!Platform.isAndroid) return false;
  try {
    return await _native.invokeMethod<bool>('shizukuRunning') ?? false;
  } catch (_) { return false; }
}

/// Opens the EmbeddedSetupActivity (pair + start Shizuku via Wireless Debugging).
Future<void> openShizukuSetup() async {
  if (!Platform.isAndroid) return;
  await _native.invokeMethod('openShizukuSetup');
}

/// Opens the native "clone apps into the second space" screen (Android 11+).
Future<void> openClone() async {
  if (!Platform.isAndroid) return;
  await _native.invokeMethod('openClone');
}

/// Lists all user-installed apps on this device.
Future<List<InstalledAppInfo>> listUserApps() async {
  if (!Platform.isAndroid) return [];
  try {
    final raw = await _native.invokeListMethod<Map>('listUserApps') ?? [];
    return raw.map((m) {
      return InstalledAppInfo(
        packageName: m['packageName'] as String,
        label: m['label'] as String,
        apkPath: m['apkPath'] as String,
        splitApkPaths: (m['splitApkPaths'] as List?)?.cast<String>() ?? [],
      );
    }).toList();
  } catch (_) { return []; }
}

class AppBundle {
  AppBundle({required this.root, required this.files, required this.withData});
  /// Folder name on the receiving side (the app's label).
  final String root;
  final List<core.BundleFile> files;
  /// False when the helper is not running: only the APKs go out.
  final bool withData;
}

/// Lists an app's APKs, Android/data and obb files as one bundle; nothing is copied.
Future<AppBundle> prepareAppBundle(String packageName) async {
  final raw = await _native.invokeMapMethod<String, dynamic>('prepareAppBundle', {'packageName': packageName});
  final files = ((raw?['files'] as List?) ?? []).cast<Map>().map((m) => core.BundleFile(
        relPath: m['relPath'] as String,
        path: m['path'] as String,
        size: BigInt.from(m['size'] as int),
        mtime: PlatformInt64Util.from(m['mtime'] as int),
        viaHelper: m['viaHelper'] as bool,
      )).toList();
  return AppBundle(root: raw?['root'] as String? ?? packageName, files: files, withData: raw?['withData'] as bool? ?? false);
}

/// Lets the transfer engine read Android/data and obb files through the helper.
Future<void> registerAppFileOpener() async {
  if (!Platform.isAndroid) return;
  await core.setPrivOpener(open: (spec) async => await _native.invokeMethod<int>('openAppFile', spec) ?? -1);
}

/// A stage of installing a received app: install, prepare, data (bytes), obb (files).
class RestoreProgress {
  RestoreProgress(this.stage, this.done, this.total);
  final String stage;
  final int done;
  final int total;
}

final StreamController<RestoreProgress> restoreProgress = StreamController<RestoreProgress>.broadcast();

/// Stops restoring data; the received folder is kept so it can be tried again.
Future<void> cancelRestore() async {
  if (Platform.isAndroid) await _native.invokeMethod('cancelRestore');
}

/// On the receiver: installs the app from a received bundle folder, then moves its data in.
/// Returns `installed|<data files>|<obb files>|<errors>` or `confirm`.
Future<String> writeReceivedApp(String packageName, String receivedDir) async {
  if (!Platform.isAndroid) return '';
  return await _native.invokeMethod<String>('writeReceivedApp', {
        'packageName': packageName,
        'receivedDir': receivedDir,
      }) ??
      '';
}
// ---------------------------------------------------------------- Paths

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
