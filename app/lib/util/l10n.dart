/// Minimal two-language string table (Indonesian default, English).
library;

enum Lang { id, en }

Lang currentLang = Lang.id;

const Map<String, List<String>> _t = {
  // key: [Indonesian, English]
  'app_name': ['VegamShare', 'VegamShare'],
  'tab_devices': ['Perangkat', 'Devices'],
  'tab_transfers': ['Transfer', 'Transfers'],
  'tab_history': ['Riwayat', 'History'],
  'tab_settings': ['Pengaturan', 'Settings'],
  'my_device': ['Perangkat ini', 'This device'],
  'my_id': ['ID', 'ID'],
  'password': ['Password', 'Password'],
  'copy': ['Salin', 'Copy'],
  'copied': ['Disalin', 'Copied'],
  'show': ['Tampilkan', 'Show'],
  'hide': ['Sembunyikan', 'Hide'],
  'new_password': ['Buat password baru', 'New random password'],
  'change_password': ['Ganti password', 'Change password'],
  'paired_devices': ['Perangkat tersimpan', 'Saved devices'],
  'no_paired': [
    'Belum ada perangkat. Tekan "Tambah" di kedua perangkat, lalu ketuk perangkat yang muncul. Cukup sekali.',
    'No devices yet. Tap "Add" on both devices, then tap the one that shows up. Only needed once.'
  ],
  'nearby': ['Terdeteksi di jaringan', 'On this network'],
  'no_nearby': ['Tidak ada perangkat VegamShare lain di jaringan ini.', 'No other VegamShare devices on this network.'],
  'online': ['Online', 'Online'],
  'offline': ['Offline', 'Offline'],
  'add_device': ['Tambah', 'Add'],
  'add_device_title': ['Tambah perangkat', 'Add device'],
  'add_device_hint': [
    'Lihat ID dan password di layar utama perangkat tujuan. Setelah tersimpan, berikutnya cukup klik.',
    'Find the ID and password on the other device\'s home screen. After this, one click is enough.'
  ],
  'device_id': ['ID perangkat (9 digit)', 'Device ID (9 digits)'],
  'ip_optional': ['Alamat IP (opsional)', 'IP address (optional)'],
  'ip_help': [
    'Cadangan untuk pairing pertama saja. Setelah tersimpan, perangkat ditemukan otomatis walau IP berubah.',
    'Fallback for the first pairing only. Once saved, the device is found automatically even if its IP changes.'
  ],
  'connect': ['Hubungkan', 'Connect'],
  'close': ['Tutup', 'Close'],
  'pair_hint_title': ['Buka layar ini di kedua perangkat', 'Open this screen on both devices'],
  'pair_hint': [
    'Selama layar ini terbuka, perangkat saling terlihat. Pairing cukup sekali; setelah itu tinggal klik untuk mengirim.',
    'While this screen is open, devices can see each other. Pair once; after that, just tap to send.'
  ],
  'searching_nearby': [
    'Mencari… Pastikan perangkat lain di Wi-Fi yang sama juga membuka layar Tambah perangkat.',
    'Searching… Make sure the other device on the same Wi-Fi also has Add device open.'
  ],
  'other_ways': ['Cara lain', 'Other ways'],
  'show_my_qr': ['Tampilkan QR saya', 'Show my QR code'],
  'show_my_qr_sub': ['Pindai dengan HP lain, tersambung tanpa mengetik', 'Scan it with another phone to pair without typing'],
  'scan_qr': ['Pindai QR', 'Scan QR code'],
  'scan_qr_sub': ['Arahkan kamera ke QR di layar perangkat lain', 'Point the camera at the QR on the other device'],
  'qr_hint': ['Sekali pakai, berlaku 2 menit, diperbarui otomatis.', 'Single use, valid 2 minutes, renewed automatically.'],
  'pair_with_password': ['ID + password', 'ID + password'],
  'pair_with_password_sub': ['Kalau perangkat tidak muncul di atas. Bisa juga lewat IP.', 'If the device does not show up above. Works with an IP too.'],
  'compare_code': ['Pastikan kode ini sama dengan yang tampil di {name}:', 'Check that {name} shows the same code:'],
  'waiting_accept': ['Menunggu {name} menekan Terima…', 'Waiting for {name} to accept…'],
  'pair_request_title': ['Permintaan pairing', 'Pairing request'],
  'pair_request_body': [
    '{name} ingin terhubung. Terima hanya jika kode ini sama dengan yang tampil di {name}:',
    '{name} wants to connect. Accept only if {name} shows the same code:'
  ],
  'cancel': ['Batal', 'Cancel'],
  'save': ['Simpan', 'Save'],
  'paired_ok': ['Tersimpan: {name}', 'Saved: {name}'],
  'send_files': ['Kirim file', 'Send files'],
  'send_folder': ['Kirim folder', 'Send folder'],
  'drop_here': ['Lepaskan file atau folder di sini untuk mengirim', 'Drop files or folders here to send'],
  'auto_accept': ['Terima otomatis dari perangkat ini', 'Accept automatically from this device'],
  'auto_accept_sub': ['Jika mati, Anda akan ditanya setiap kali', 'If off, you will be asked every time'],
  'rename': ['Ganti nama', 'Rename'],
  'forget': ['Hapus perangkat', 'Remove device'],
  'forget_confirm': [
    'Hapus {name}? Untuk terhubung lagi perlu ID dan password.',
    'Remove {name}? Connecting again will need the ID and password.'
  ],
  'remove': ['Hapus', 'Remove'],
  'check_connection': ['Cek koneksi', 'Check connection'],
  'reachable': ['Terhubung', 'Reachable'],
  'with_device': ['Riwayat dengan perangkat ini', 'History with this device'],
  'sending_queued': ['Mengirim ke {name}…', 'Sending to {name}…'],
  'incoming_title': ['File masuk', 'Incoming files'],
  'incoming_body': ['{name} ingin mengirim "{summary}" ({count} file, {size}).', '{name} wants to send "{summary}" ({count} files, {size}).'],
  'accept': ['Terima', 'Accept'],
  'decline': ['Tolak', 'Decline'],
  'no_transfers': ['Tidak ada transfer berjalan.', 'No transfers running.'],
  'resumable': ['Bisa dilanjutkan', 'Can be resumed'],
  'pause': ['Jeda', 'Pause'],
  'resume': ['Lanjutkan', 'Resume'],
  'stop': ['Hentikan', 'Stop'],
  'to': ['ke', 'to'],
  'from': ['dari', 'from'],
  'files': ['file', 'files'],
  'open_folder': ['Buka folder', 'Open folder'],
  'clear_history': ['Hapus riwayat', 'Clear history'],
  'no_history': ['Belum ada riwayat.', 'No history yet.'],
  'st_in_progress': ['Berjalan', 'Running'],
  'st_completed': ['Selesai', 'Completed'],
  'st_paused': ['Dijeda', 'Paused'],
  'st_interrupted': ['Terputus', 'Interrupted'],
  'st_cancelled': ['Dibatalkan', 'Cancelled'],
  'st_rejected': ['Ditolak', 'Declined'],
  'st_failed': ['Gagal', 'Failed'],
  'device_name': ['Nama perangkat', 'Device name'],
  'download_folder': ['Folder penyimpanan', 'Download folder'],
  'choose': ['Pilih', 'Choose'],
  'language': ['Bahasa', 'Language'],
  'theme': ['Tema', 'Theme'],
  'theme_system': ['Ikuti sistem', 'System'],
  'theme_light': ['Terang', 'Light'],
  'theme_dark': ['Gelap', 'Dark'],
  'start_with_windows': ['Jalankan saat Windows menyala', 'Start with Windows'],
  'ready': ['Siap menerima', 'Ready to receive'],
  'ready_on': ['Menunggu ketukan dari perangkat tersimpan. Tidak ada aktivitas sampai ada kiriman.', 'Waiting for saved devices to knock. No activity until something is sent.'],
  'ready_off': ['Tidak menerima. Perangkat ini tidak terlihat dan tidak menjawab siapa pun.', 'Not receiving. This device is invisible and answers no one.'],
  'paired_history': ['Dipasangkan dengan {name}', 'Paired with {name}'],
  'identity_changed_title': ['Pairing ulang diperlukan', 'Pair again'],
  'identity_changed_body': [
    'Di alamat terakhir {name} sekarang ada VegamShare dengan identitas lain (ID {id}). Biasanya karena aplikasinya diinstal ulang. Hapus {name} lalu pairing ulang?',
    'At the last address of {name} there is now a VegamShare with another identity (ID {id}), usually because the app was reinstalled. Remove {name} and pair again?'
  ],
  'pair_again': ['Pairing ulang', 'Pair again'],
  'unreachable_hint': [
    '{name} tidak dapat dihubungi. Pastikan aplikasinya terbuka, "Siap menerima" aktif, dan di Wi-Fi yang sama. Kiriman bisa dilanjutkan dari tab Transfer. Kalau {name} baru diinstal ulang, hapus dan pairing ulang.',
    '{name} cannot be reached. Make sure the app is open, "Ready to receive" is on and it is on the same Wi-Fi. Resume from the Transfers tab. If {name} was reinstalled, remove it and pair again.'
  ],
  'last_seen': ['Terakhir terhubung {time}', 'Last connected {time}'],
  'never_connected': ['Belum pernah terhubung', 'Never connected'],
  'fingerprint': ['Sidik jari keamanan', 'Security fingerprint'],
  'fingerprint_sub': [
    'Kunci unik perangkat ini. Tidak pernah berubah dan dipakai untuk memastikan Anda terhubung ke perangkat yang benar.',
    'This device\'s unique key. It never changes and proves you are talking to the right device.'
  ],
  'addresses': ['Alamat di jaringan', 'Network addresses'],
  'about': ['Tentang', 'About'],
  'licenses': ['Lisensi open source', 'Open-source licenses'],
  'password_rules': ['6–64 karakter', '6–64 characters'],
  'ready_notification': ['Siap menerima file', 'Ready to receive files'],
  'storage_permission': [
    'VegamShare perlu izin akses semua file agar bisa mengirim folder dan menyimpan file yang diterima.',
    'VegamShare needs all-files access to send folders and save received files.'
  ],
  'grant': ['Beri izin', 'Grant'],
  'later': ['Nanti', 'Later'],
  'choose_device': ['Kirim ke perangkat', 'Send to device'],
  'show_window': ['Tampilkan', 'Show'],
  'quit': ['Keluar', 'Quit'],
  'starting': ['Memulai…', 'Starting…'],
  'start_failed': ['Gagal memulai layanan', 'Could not start the service'],
  'retry': ['Coba lagi', 'Retry'],
  'received_saved': ['Tersimpan di {path}', 'Saved to {path}'],
  'transfer_done': ['Transfer selesai', 'Transfer complete'],
  // errors
  'e_wrong_password': ['Password salah.', 'Wrong password.'],
  'e_locked_out': ['Terlalu banyak percobaan. Coba lagi dalam {s} detik.', 'Too many attempts. Try again in {s} seconds.'],
  'e_not_found': [
    'Perangkat tidak ditemukan di jaringan. Pastikan terhubung ke Wi-Fi yang sama dan aplikasinya terbuka, atau isi alamat IP.',
    'Device not found on this network. Make sure both are on the same Wi-Fi and the app is open, or enter its IP.'
  ],
  'e_unreachable': [
    'Tidak dapat dihubungi. Pastikan aplikasinya terbuka, "Siap menerima" aktif, dan di Wi-Fi yang sama. Kalau perangkat itu baru diinstal ulang, hapus lalu pairing ulang.',
    'Cannot be reached. Make sure the app is open, "Ready to receive" is on and it is on the same Wi-Fi. If the device was reinstalled, remove it and pair again.'
  ],
  'e_identity_changed': ['Perangkat ini sudah berganti identitas (kemungkinan diinstal ulang). Pairing ulang diperlukan.', 'This device has a new identity (probably reinstalled). Pair again.'],
  'e_not_trusted': ['Perangkat ini tidak lagi mempercayai Anda. Tambahkan ulang.', 'That device no longer trusts you. Add it again.'],
  'e_identity_mismatch': [
    'Identitas perangkat tidak cocok. Kemungkinan perangkat lain menyamar. Koneksi dibatalkan.',
    'Device identity mismatch: possibly an impostor. Connection refused.'
  ],
  'e_rejected': ['Ditolak oleh penerima.', 'Declined by the receiver.'],
  'e_invalid_code': ['Kode QR tidak valid, sudah dipakai, atau kedaluwarsa. Minta QR baru.', 'QR code is invalid, already used or expired. Ask for a new one.'],
  'e_pairing_closed': [
    'Perangkat tujuan tidak sedang membuka layar Tambah perangkat.',
    'The other device is not on its Add device screen.'
  ],
  'e_timeout': ['Waktu habis.', 'Timed out.'],
  'e_generic': ['Terjadi kesalahan: {detail}', 'Something went wrong: {detail}'],
};

String tr(String key, [Map<String, Object> params = const {}]) {
  final entry = _t[key];
  var s = entry == null ? key : entry[currentLang.index];
  params.forEach((k, v) => s = s.replaceAll('{$k}', '$v'));
  return s;
}

/// Turns a bridge exception ("code|detail") into a friendly message.
/// Splits a bridge error into (code, detail); null if it has no code.
(String, String)? errorParts(Object error) {
  var text = error.toString();
  // flutter_rust_bridge wraps anyhow errors in "AnyhowException(...)".
  final m = RegExp(r'AnyhowException\((.*)\)', dotAll: true).firstMatch(text);
  if (m != null) text = m.group(1)!;
  final bar = text.indexOf('|');
  if (bar < 0) return null;
  return (text.substring(0, bar).trim(), text.substring(bar + 1).split('\n').first.trim());
}

String describeError(Object error) {
  final parts = errorParts(error);
  if (parts == null) return tr('e_generic', {'detail': error.toString()});
  final (code, detail) = parts;
  switch (code) {
    case 'locked_out':
      return tr('e_locked_out', {'s': detail});
    case 'wrong_password':
    case 'not_found':
    case 'unreachable':
    case 'not_trusted':
    case 'identity_mismatch':
    case 'rejected':
    case 'pairing_closed':
    case 'identity_changed':
    case 'invalid_code':
    case 'timeout':
      return tr('e_$code');
    default:
      return tr('e_generic', {'detail': detail});
  }
}

String statusLabel(String status) => tr('st_$status');
