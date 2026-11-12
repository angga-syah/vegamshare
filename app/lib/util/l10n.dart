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
  'send_app': ['Kirim app', 'Send app'],
  'send_app_title': ['Pilih app untuk dikirim', 'Choose app to send'],
  'send_app_search': ['Cari app…', 'Search apps…'],
  'send_app_queued': ['Mengirim {name} ke {device}…', 'Sending {name} to {device}…'],
  'send_app_failed': ['Gagal menyiapkan app: {error}', 'Failed to prepare app: {error}'],
  'shizuku_needed': ['Aktifkan akses privileged agar data Android/data ikut terkirim.', 'Enable privileged access so Android/data is included.'],
  'shizuku_setup': ['Setup', 'Setup'],
  'clone_apps': ['Gandakan app', 'Clone apps'],
  'restore_install': ['Memasang aplikasi…', 'Installing the app…'],
  'restore_prepare': ['Menyiapkan folder data…', 'Preparing the data folder…'],
  'restore_data': ['Memulihkan data game…', 'Restoring game data…'],
  'restore_obb': ['Memindahkan file OBB…', 'Moving OBB files…'],
  'restore_cancelling': ['Membatalkan…', 'Cancelling…'],
  'restore_cancelled': ['Dibatalkan. File kiriman tetap disimpan, bisa dicoba lagi.', 'Cancelled. The received files are kept so you can try again.'],
  'send_app_preparing': ['Menyiapkan {name}', 'Preparing {name}'],
  'send_app_no_data_title': ['Data game tidak akan ikut', 'Game data will not be included'],
  'send_app_no_data_body': ['Akses data aplikasi di HP ini belum aktif, jadi hanya APK {name} yang terkirim dan game harus mengunduh datanya lagi. Setup dulu supaya data ikut terkirim.', 'App data access is not on for this phone, so only the APK of {name} would be sent and the game would download its data again. Set it up first to include the data.'],
  'send_app_apk_only': ['Kirim APK saja', 'Send APK only'],
  'send_app_login_note': ['Yang terkirim: APK dan data unduhan game (Android/data, OBB). Data login biasanya tidak ikut, jadi login lagi dengan akun game di HP tujuan.', 'Sent: the APK and the game\'s downloaded data (Android/data, OBB). Logins usually stay behind, so sign in with the game account on the other phone.'],
  'send_app_preparing_body': ['Mendata APK dan data game. Untuk game besar bisa sampai 1 menit. Setelah itu progres kirim tampil di tab Transfer, bisa dijeda atau dihentikan.', 'Listing the APKs and game data. A big game can take up to a minute. Then the progress shows in the Transfer tab, where it can be paused or stopped.'],
  'app_data_access': ['Akses data aplikasi', 'App data access'],
  'app_data_on': ['Aktif: data game (Android/data) ikut terkirim dan dipulihkan.', 'On: game data (Android/data) is sent and restored.'],
  'app_data_off': ['Belum aktif. Perlu setup sekali lewat Debug nirkabel; setelah restart HP cukup nyalakan Debug nirkabel.', 'Off. Needs a one-time setup via Wireless debugging; after a restart just turn Wireless debugging on.'],
  'app_data_setup_needed': ['Data game {label} hanya bisa dipulihkan setelah akses data aplikasi aktif. Setup sekarang? Dialog ini muncul lagi setelah setup selesai.', 'Game data for {label} can only be restored once app data access is on. Set it up now? This dialog comes back when setup is done.'],
  'install_without_data': ['Install tanpa data', 'Install without data'],
  'no_apps': ['Tidak ada app.', 'No apps found.'],
  'app_received_title': ['App diterima', 'App received'],
  'app_received_body': ['Terima {label} ({pkg})?', 'Install {label} ({pkg})?'],
  'install': ['Install', 'Install'],
  'app_installing': ['Menginstall {label}…', 'Installing {label}…'],
  'app_install_failed': ['Gagal install: {error}', 'Install failed: {error}'],
  'app_installed': ['{label} terpasang, {files} file data dipulihkan.', '{label} installed, {files} data files restored.'],
  'app_installed_errors': ['{label} terpasang, {files} file data dipulihkan, {errors} gagal.', '{label} installed, {files} data files restored, {errors} failed.'],
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
  'no_wifi_title': ['Tidak ada Wi-Fi? Buat hotspot', 'No Wi-Fi? Create a hotspot'],
  'no_wifi_sub': [
    'HP ini membuat hotspot sementara tanpa internet (kuota tidak terpakai). Perangkat lain bergabung dengan memindai QR.',
    'This phone creates a temporary hotspot without internet (no mobile data used). Others join by scanning the QR code.'
  ],
  'no_wifi_windows': [
    'Di HP Android: Tambah → "Tidak ada Wi-Fi? Buat hotspot". Sambungkan laptop ke hotspot itu lewat menu Wi-Fi Windows, lalu buka Tambah perangkat di sini.',
    'On the Android phone: Add → "No Wi-Fi? Create a hotspot". Connect this laptop to that hotspot from the Windows Wi-Fi menu, then open Add device here.'
  ],
  'hotspot_scan_hint': [
    'Di HP lain: buka VegamShare → Tambah → Pindai QR. HP itu akan bergabung ke hotspot dan langsung tersambung.',
    'On the other phone: open VegamShare → Add → Scan QR code. It joins the hotspot and connects right away.'
  ],
  'hotspot_laptop_title': ['Untuk laptop', 'For a laptop'],
  'hotspot_laptop_hint': [
    'Sambungkan Wi-Fi laptop ke jaringan di bawah, ubah jaringannya menjadi Privat, lalu buka Tambah perangkat di laptop dan ketuk HP ini.',
    'Connect the laptop to the network below, set it to Private, then open Add device on the laptop and tap this phone.'
  ],
  'hotspot_name': ['Nama hotspot', 'Hotspot name'],
  'hotspot_note': [
    'Nama dan password berganti setiap kali hotspot dinyalakan (aturan Android). Perangkat yang sudah dipairing tidak perlu pairing ulang, cukup bergabung lagi.',
    'Name and password change every time the hotspot starts (an Android rule). Paired devices do not need to pair again, they just join again.'
  ],
  'hotspot_stop': ['Matikan hotspot', 'Turn off hotspot'],
  'hotspot_stop_short': ['Matikan', 'Turn off'],
  'hotspot_stopped': ['Hotspot berhenti.', 'The hotspot stopped.'],
  'hotspot_active': ['Hotspot aktif: {name}', 'Hotspot on: {name}'],
  'hotspot_active_sub': ['Ketuk untuk menampilkan QR', 'Tap to show the QR code'],
  'joining_hotspot': ['Bergabung ke hotspot {name}…', 'Joining hotspot {name}…'],
  'pairing_progress': ['Menyambungkan…', 'Connecting…'],
  'joined_ready': ['Tersambung ke {name}. Siap mengirim.', 'Connected to {name}. Ready to send.'],
  'joined_hotspot': ['Tersambung ke hotspot {name}', 'Connected to hotspot {name}'],
  'joined_lost': ['Koneksi ke hotspot terputus.', 'The hotspot connection dropped.'],
  'disconnect': ['Putuskan', 'Disconnect'],
  'public_network_title': ['Jaringan ini "Publik"', 'This network is "Public"'],
  'public_network_body': [
    'Windows memblokir perangkat lain untuk menghubungi PC ini di jaringan Publik. Ubah jaringan menjadi Privat.',
    'Windows blocks other devices from reaching this PC on Public networks. Switch the network to Private.'
  ],
  'open_settings': ['Buka pengaturan', 'Open settings'],
  'replace_title': ['Ganti perangkat lama?', 'Replace the old device?'],
  'replace_body': [
    '{name} baru saja ditambahkan, dan ada {name} lain yang tersimpan (ID {ids}). Biasanya itu perangkat yang sama sebelum aplikasinya diinstal ulang. Hapus entri lama?',
    '{name} was just added, and another {name} is saved (ID {ids}). Usually that is the same device before the app was reinstalled. Remove the old entry?'
  ],
  'keep_both': ['Simpan keduanya', 'Keep both'],
  'remove_old': ['Hapus yang lama', 'Remove old'],
  'e_wifi_permission': ['Izin Wi-Fi/lokasi diperlukan untuk hotspot. Izinkan di pengaturan aplikasi.', 'Wi-Fi/location permission is needed for the hotspot. Allow it in the app settings.'],
  'e_wifi_location_off': ['Nyalakan Lokasi (GPS) dulu, Android memerlukannya untuk hotspot.', 'Turn on Location first; Android requires it for the hotspot.'],
  'e_wifi_unsupported': ['HP ini tidak mendukung hotspot otomatis.', 'This phone does not support the automatic hotspot.'],
  'e_wifi_busy': [
    'Hotspot tidak bisa dibuat karena hotspot/tethering lain sedang aktif. Matikan dulu, lalu coba lagi.',
    'Cannot create the hotspot while another hotspot/tethering is on. Turn it off and try again.'
  ],
  'e_wifi_join_failed': ['Gagal bergabung ke hotspot. Dekatkan ke HP pembuat hotspot dan pastikan hotspotnya masih aktif, lalu pindai lagi.', 'Could not join the hotspot. Move closer to the hotspot phone, make sure its hotspot is still on, and scan again.'],
  'e_wifi_failed': [
    'Hotspot tidak bisa dibuat ({code}). Matikan hotspot/tethering lain yang sedang aktif, lalu coba lagi.',
    'Could not create the hotspot ({code}). Turn off any other hotspot/tethering and try again.'
  ],
  'e_wifi_tethering_on': [
    'Hotspot biasa (tethering) HP ini sedang menyala. Android tidak bisa menjalankannya bersamaan dengan hotspot VegamShare. Matikan di pengaturan hotspot; VegamShare mencoba lagi sendiri saat Anda kembali.',
    'This phone\'s normal hotspot (tethering) is on. Android cannot run it together with the VegamShare hotspot. Turn it off in the hotspot settings; VegamShare tries again by itself when you come back.'
  ],
  'e_wifi_off': ['Wi-Fi belum menyala. Nyalakan Wi-Fi, lalu pindai QR lagi.', 'Wi-Fi is still off. Turn it on and scan the QR code again.'],
  'open_hotspot_settings': ['Buka pengaturan hotspot', 'Open hotspot settings'],
  'open_location_settings': ['Buka pengaturan lokasi', 'Open location settings'],
  'wifi_turning_on': ['Menyalakan Wi-Fi…', 'Turning on Wi-Fi…'],
  'wifi_turn_on_hint': ['Nyalakan Wi-Fi di panel yang muncul. Setelah itu tersambung sendiri.', 'Turn on Wi-Fi in the panel that opened. It connects by itself afterwards.'],
  'hotspot_notice_title': ['Hotspot VegamShare aktif: {name}', 'VegamShare hotspot on: {name}'],
  'hotspot_notice_text': [
    'Hotspot khusus VegamShare, tanpa internet. Tidak tampil sebagai Hotspot di menu cepat.',
    'VegamShare-only hotspot, no internet. It does not show as Hotspot in quick settings.'
  ],
  // WhatsApp files
  'wa_title': ['File WhatsApp', 'WhatsApp files'],
  'wa_button': ['File WhatsApp', 'WhatsApp files'],
  'wa_search': ['Cari nama file', 'Search file name'],
  'wa_docs': ['Dokumen', 'Documents'],
  'wa_media': ['Foto & Video', 'Photos & videos'],
  'wa_sent': ['Termasuk yang saya kirim', 'Include sent by me'],
  'wa_sent_tag': ['terkirim', 'sent'],
  'wa_none': [
    'Tidak ada file WhatsApp di HP ini. File baru muncul di sini setelah diunduh di WhatsApp.',
    'No WhatsApp files on this phone. Files show up here once they are downloaded in WhatsApp.'
  ],
  'wa_none_here': ['Tidak ada file yang cocok.', 'No matching files.'],
  'wa_open_with': ['Buka dengan aplikasi lain', 'Open with another app'],
  'wa_select': ['Pilih', 'Select'],
  'send': ['Kirim', 'Send'],
  'today': ['Hari ini', 'Today'],
  'yesterday': ['Kemarin', 'Yesterday'],
  // App updates
  'update_check': ['Cek pembaruan', 'Check for updates'],
  'update_checking': ['Memeriksa…', 'Checking…'],
  'update_installed': ['Versi terpasang {v}', 'Installed version {v}'],
  'update_up_to_date': ['Sudah versi terbaru ({v})', 'Up to date ({v})'],
  'update_failed_short': ['Gagal memeriksa. Periksa koneksi internet.', 'Could not check. Check the internet connection.'],
  'update_available_short': ['Versi {v} tersedia. Ketuk untuk memperbarui.', 'Version {v} is available. Tap to update.'],
  'update_auto': ['Cek pembaruan otomatis', 'Check for updates automatically'],
  'update_auto_sub': [
    'Mati: cek hanya saat Anda menekan "Cek pembaruan". Nyala: sekali sehari saat aplikasi dibuka, hanya ke GitHub, tidak pernah di latar belakang.',
    'Off: only when you tap "Check for updates". On: once a day when the app is opened, only to GitHub, never in the background.'
  ],
  'update_available_title': ['Versi baru {v}', 'New version {v}'],
  'update_available_body': ['Terpasang {old}. Ukuran unduhan {size}.', 'Installed {old}. Download size {size}.'],
  'update_now': ['Perbarui', 'Update'],
  'update_downloading': ['Mengunduh {v}…', 'Downloading {v}…'],
  'update_permission_title': ['Izinkan pemasangan', 'Allow installing'],
  'update_permission_body': [
    'Android perlu izin "Instal aplikasi tidak dikenal" untuk VegamShare (sekali saja). Aktifkan di layar yang terbuka, lalu kembali: pemasangan berlanjut sendiri.',
    'Android needs "Install unknown apps" allowed for VegamShare (only once). Turn it on in the screen that opened, then come back: the install continues by itself.'
  ],
  'update_install': ['Pasang', 'Install'],
  'update_install_again': ['Pasang lagi', 'Install again'],
  'update_installing_title': ['Memasang {v}', 'Installing {v}'],
  'update_installing_body': [
    'Ikuti petunjuk penginstal sistem. VegamShare tertutup sebentar selama pembaruan; data dan perangkat tersimpan tetap ada.',
    'Follow the system installer. VegamShare closes briefly during the update; your data and saved devices stay.'
  ],
  'update_failed_title': ['Pembaruan gagal', 'Update failed'],
  'update_failed_check': ['Tidak bisa menghubungi GitHub. Periksa koneksi internet, atau unduh manual dari {url}', 'Could not reach GitHub. Check the internet connection, or download it from {url}'],
  'update_failed_download': ['Unduhan gagal atau rusak. Coba lagi, atau unduh manual dari {url}', 'The download failed or is damaged. Try again, or download it from {url}'],
  'update_failed_install': ['Penginstal tidak bisa dibuka. Unduh manual dari {url}', 'Could not open the installer. Download it from {url}'],
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
