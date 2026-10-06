import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'platform/platform_services.dart';
import 'src/rust/api/vegam.dart' as core;
import 'src/rust/frb_generated.dart';
import 'state/app_state.dart';
import 'ui/home_shell.dart';
import 'util/l10n.dart';

final appState = AppState();

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  // Rust crates are not known to Flutter's license registry; add their
  // notices so the in-app licenses page is complete.
  LicenseRegistry.addLicense(() async* {
    yield LicenseEntryWithLineBreaks(['VegamShare (Rust core dependencies)'], await rootBundle.loadString('assets/THIRD_PARTY_LICENSES.txt'));
  });
  await RustLib.init();
  await appState.loadPrefs();
  await initDesktop(args, onQuit: _quit);
  runApp(const VegamShareApp());
}

Future<void> _quit() async {
  await core.shutdown();
  await quitDesktop();
  exit(0);
}

class VegamShareApp extends StatelessWidget {
  const VegamShareApp({super.key});

  ThemeData _theme(Brightness b) {
    final scheme = ColorScheme.fromSeed(seedColor: const Color(0xFF1565C0), brightness: b);
    return ThemeData(
      colorScheme: scheme,
      useMaterial3: true,
      cardTheme: CardThemeData(
        elevation: 0,
        color: scheme.surfaceContainerLow,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      ),
      listTileTheme: const ListTileThemeData(shape: RoundedRectangleBorder(borderRadius: BorderRadius.all(Radius.circular(12)))),
      inputDecorationTheme: const InputDecorationTheme(border: OutlineInputBorder()),
    );
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: appState,
      builder: (context, _) => MaterialApp(
        title: 'VegamShare',
        debugShowCheckedModeBanner: false,
        theme: _theme(Brightness.light),
        darkTheme: _theme(Brightness.dark),
        themeMode: appState.themeMode,
        home: const StartupGate(),
      ),
    );
  }
}

/// Starts the Rust service (after Android permissions) and shows errors
/// with a retry instead of a blank screen.
class StartupGate extends StatefulWidget {
  const StartupGate({super.key});

  @override
  State<StartupGate> createState() => _StartupGateState();
}

class _StartupGateState extends State<StartupGate> {
  Object? _error;
  bool _ready = false;
  bool _needsPermission = false;

  @override
  void initState() {
    super.initState();
    _start();
  }

  Future<void> _start() async {
    setState(() {
      _error = null;
      _needsPermission = false;
    });
    try {
      if (!await hasStoragePermission()) {
        setState(() => _needsPermission = true);
        return;
      }
      final dirs = await resolveDirs();
      var name = defaultDeviceName();
      if (Platform.isAndroid) name = await androidModelName() ?? name;
      final info = await core.start(
        config: core.StartConfig(dataDir: dirs.data, downloadDir: dirs.downloads, deviceName: name, platform: platformName()),
      );
      await appState.attach(info);
      await setBackgroundService(info.receiving);
      await initShareIntents();
      if (mounted) setState(() => _ready = true);
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_ready) return const HomeShell();
    final theme = Theme.of(context);
    Widget body;
    if (_needsPermission) {
      body = Column(mainAxisSize: MainAxisSize.min, children: [
        Icon(Icons.folder_shared_outlined, size: 56, color: theme.colorScheme.primary),
        const SizedBox(height: 16),
        Text(tr('storage_permission'), textAlign: TextAlign.center),
        const SizedBox(height: 24),
        FilledButton(
          onPressed: () async {
            await requestStoragePermission();
            _start();
          },
          child: Text(tr('grant')),
        ),
      ]);
    } else if (_error != null) {
      body = Column(mainAxisSize: MainAxisSize.min, children: [
        Icon(Icons.error_outline, size: 56, color: theme.colorScheme.error),
        const SizedBox(height: 16),
        Text(tr('start_failed'), style: theme.textTheme.titleMedium),
        const SizedBox(height: 8),
        Text(describeError(_error!), textAlign: TextAlign.center),
        const SizedBox(height: 24),
        FilledButton(onPressed: _start, child: Text(tr('retry'))),
      ]);
    } else {
      body = Column(mainAxisSize: MainAxisSize.min, children: [
        const CircularProgressIndicator(),
        const SizedBox(height: 16),
        Text(tr('starting')),
      ]);
    }
    return Scaffold(body: Center(child: Padding(padding: const EdgeInsets.all(32), child: body)));
  }
}
