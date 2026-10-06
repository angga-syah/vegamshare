# VegamShare 0.1.0 (pre-release)

Rilis pertama, berstatus **beta**. Sudah diuji di Windows 11 dan emulator Android 16, tapi belum di
banyak HP. Mohon laporkan masalah lewat Issues.

## Fitur
- Kirim file dan **folder utuh** antar Android ↔ Windows di Wi-Fi/LAN yang sama, tanpa internet atau server.
- **Pairing cukup sekali**, dengan pilihan cara: ketuk perangkat di sekitar lalu cocokkan kode 6 digit,
  pindai QR, atau ID 9 digit + password (bisa juga lewat IP). Setelah itu tinggal klik, walaupun IP berubah.
- **Hemat baterai, mirip earphone Bluetooth**: aplikasi diam total sampai Anda mengirim. Tidak ada
  pengumuman ke jaringan dan tidak ada pengecekan di latar belakang.
- Tombol **Siap menerima** On/Off.
- **Resume** otomatis jika koneksi terputus, dan setiap file diverifikasi (BLAKE3).
- **Enkripsi end-to-end** (QUIC/TLS 1.3), pairing dengan SPAKE2, dan identitas perangkat dikunci (pinning).
- Windows: tray, menu Explorer *Kirim ke*, drag & drop. Android: *Bagikan ke VegamShare*, browser file bawaan.

## Unduhan
- `VegamShare-Setup-0.1.0.exe`: installer Windows 10/11 (x64). Installer belum bertanda tangan digital,
  jadi Windows SmartScreen akan menampilkan peringatan. Pilih *More info → Run anyway*.
- `VegamShare-0.1.0.apk`: Android 7.0+ (arm64, armv7, x86_64); hanya diuji di Android 16.

## Catatan
- Pastikan profil Wi-Fi di Windows adalah **Private**. Firewall hanya dibuka untuk jaringan Private.
- Android meminta izin *akses semua file* (untuk mengirim folder) dan notifikasi.
- Model keamanan dan batasannya: [docs/SECURITY.md](SECURITY.md).
