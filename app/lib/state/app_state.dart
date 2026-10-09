import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../platform/platform_services.dart' show holdTransferLocks;
import '../src/rust/api/vegam.dart' as core;
import '../util/l10n.dart';

class TransferUi {
  TransferUi({
    required this.id,
    required this.outgoing,
    required this.peer,
    required this.peerName,
    required this.bytesTotal,
    required this.filesTotal,
  });

  final String id;
  final bool outgoing;
  final String peer;
  final String peerName;
  int bytesTotal;
  int filesTotal;
  int bytesDone = 0;
  int filesDone = 0;
  int speed = 0;
  String current = '';

  double get fraction => bytesTotal == 0 ? (filesDone >= filesTotal ? 1 : 0) : bytesDone / bytesTotal;

  void apply(Map<String, dynamic> t) {
    bytesDone = (t['bytes_done'] as num).toInt();
    bytesTotal = (t['bytes_total'] as num).toInt();
    filesDone = (t['files_done'] as num).toInt();
    filesTotal = (t['files_total'] as num).toInt();
    current = t['current'] as String? ?? '';
  }

  static TransferUi fromJson(Map<String, dynamic> t) => TransferUi(
        id: t['transfer_id'] as String,
        outgoing: t['direction'] == 'send',
        peer: t['peer'] as String,
        peerName: t['peer_name'] as String,
        bytesTotal: (t['bytes_total'] as num).toInt(),
        filesTotal: (t['files_total'] as num).toInt(),
      )..apply(t);
}

class IncomingOffer {
  IncomingOffer(this.id, this.from, this.fromName, this.count, this.size, this.summary);
  final String id;
  final String from;
  final String fromName;
  final int count;
  final int size;
  final String summary;
}

class PairRequest {
  PairRequest(this.id, this.fromName, this.platform, this.code);
  final String id;
  final String fromName;
  final String platform;
  final String code;
}

class FinishedNotice {
  FinishedNotice(this.id, this.outgoing, this.peer, this.status, this.error, this.errorKind, this.errorDetail, this.location);
  final String id;
  final bool outgoing;
  final String peer;
  final String status;
  final String? error;
  final String? errorKind;
  final String? errorDetail;
  final String? location;
}

/// Single source of truth for the UI; fed by the Rust event stream.
class AppState extends ChangeNotifier {
  core.MyInfo? info;
  List<core.Device> devices = [];
  List<core.Nearby> nearby = [];
  List<core.HistoryItem> history = [];
  final Map<String, TransferUi> transfers = {};

  ThemeMode themeMode = ThemeMode.system;
  bool _locksHeld = false;

  final _offers = StreamController<IncomingOffer>.broadcast();
  final _offerClosed = StreamController<String>.broadcast();
  final _finished = StreamController<FinishedNotice>.broadcast();
  final _pairRequests = StreamController<PairRequest>.broadcast();
  final _pairClosed = StreamController<String>.broadcast();
  Stream<PairRequest> get pairRequests => _pairRequests.stream;
  Stream<String> get pairRequestClosed => _pairClosed.stream;
  Stream<IncomingOffer> get offers => _offers.stream;
  Stream<String> get offerClosed => _offerClosed.stream;
  Stream<FinishedNotice> get finished => _finished.stream;

  StreamSubscription<String>? _sub;
  Timer? _refreshDebounce;
  SharedPreferences? _prefs;

  Future<void> loadPrefs() async {
    _prefs = await SharedPreferences.getInstance();
    currentLang = (_prefs!.getString('lang') == 'en') ? Lang.en : Lang.id;
    themeMode = ThemeMode.values.byName(_prefs!.getString('theme') ?? 'system');
  }

  Future<void> attach(core.MyInfo myInfo) async {
    info = myInfo;
    _sub = core.events().listen(_onEvent);
    await refreshAll();
  }

  Future<void> refreshAll() async {
    await Future.wait([refreshDevices(), refreshNearby(), refreshHistory(), _refreshTransfers()]);
  }

  Future<void> refreshDevices() async {
    devices = await core.devices();
    notifyListeners();
  }

  Future<void> refreshNearby() async {
    nearby = await core.nearby();
    notifyListeners();
  }

  Future<void> refreshHistory() async {
    history = await core.history(limit: 300);
    notifyListeners();
  }

  void refreshInfo() {
    info = core.myInfo();
    notifyListeners();
  }

  Future<void> _refreshTransfers() async {
    for (final t in await core.activeTransfers()) {
      transfers.putIfAbsent(
        t.transferId,
        () => TransferUi(
          id: t.transferId,
          outgoing: t.outgoing,
          peer: t.peer,
          peerName: t.peerName,
          bytesTotal: t.bytesTotal.toInt(),
          filesTotal: t.filesTotal.toInt(),
        ),
      );
    }
  }

  void _debouncedRefresh(Future<void> Function() f) {
    _refreshDebounce?.cancel();
    _refreshDebounce = Timer(const Duration(milliseconds: 200), f);
  }

  /// Holds wake/Wi-Fi locks exactly while transfers are running.
  void _syncLocks() {
    final want = transfers.isNotEmpty;
    if (want != _locksHeld) {
      _locksHeld = want;
      holdTransferLocks(want);
    }
  }

  void _onEvent(String raw) {
    final Map<String, dynamic> e = jsonDecode(raw);
    _handle(e);
    _syncLocks();
  }

  void _handle(Map<String, dynamic> e) {
    switch (e['event']) {
      case 'device_status':
        refreshDevices();
      case 'devices_changed':
        // Pairing and removal also write history entries.
        refreshDevices();
        refreshHistory();
        _debouncedRefresh(refreshNearby);
      case 'nearby_changed':
        _debouncedRefresh(refreshNearby);
      case 'incoming_offer':
        _offers.add(IncomingOffer(
          e['transfer_id'],
          e['from'],
          e['from_name'],
          (e['file_count'] as num).toInt(),
          (e['total_bytes'] as num).toInt(),
          e['summary'],
        ));
      case 'pair_request':
        _pairRequests.add(PairRequest(e['request_id'], e['from_name'], e['from_platform'], e['code']));
      case 'pair_request_closed':
        _pairClosed.add(e['request_id']);
      case 'offer_closed':
        _offerClosed.add(e['transfer_id']);
      case 'transfer_started':
        final t = TransferUi.fromJson(e['transfer']);
        transfers[t.id] = t;
        refreshHistory();
      case 'transfer_progress':
        final t = e['transfer'] as Map<String, dynamic>;
        final ui = transfers.putIfAbsent(t['transfer_id'], () => TransferUi.fromJson(t));
        ui.apply(t);
        ui.speed = (e['speed_bps'] as num).toInt();
        notifyListeners();
      case 'transfer_finished':
        final id = e['transfer_id'] as String;
        transfers.remove(id);
        _finished.add(FinishedNotice(id, e['direction'] == 'send', e['peer'], e['status'], e['error'], e['error_kind'], e['error_detail'], e['location']));
        refreshHistory();
    }
  }

  List<core.HistoryItem> get resumable =>
      history.where((h) => (h.status == 'paused' || h.status == 'interrupted') && !transfers.containsKey(h.transferId)).toList();

  core.Device? deviceByFingerprint(String fp) {
    for (final d in devices) {
      if (d.fingerprint == fp) return d;
    }
    return null;
  }

  // ---- preferences ----

  void setLang(Lang l) {
    currentLang = l;
    _prefs?.setString('lang', l.name);
    notifyListeners();
  }

  void setTheme(ThemeMode m) {
    themeMode = m;
    _prefs?.setString('theme', m.name);
    notifyListeners();
  }


  @override
  void dispose() {
    _sub?.cancel();
    _offers.close();
    _offerClosed.close();
    _finished.close();
    super.dispose();
  }
}
