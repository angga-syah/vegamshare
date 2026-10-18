# VegamShare 0.2.2

Fitur baru: kirim app — transfer aplikasi Android lengkap (APK + data + OBB) antar HP lewat LAN.

## Fitur baru

- **Kirim app** — pilih aplikasi yang terinstall, VegamShare mengirim APK + Android/data + OBB
  ke HP lain di jaringan yang sama. Cocok untuk game besar seperti MLBB yang butuh waktu lama
  jika didownload ulang.
- **Instalasi otomatis** — penerima mendapat notifikasi saat app tiba dan bisa langsung install
  tanpa keluar dari VegamShare (menggunakan PackageInstaller API, tidak perlu root).
- **Shizuku embedded** — akses Android/data (folder yang dilindungi sistem) melalui Wireless
  Debugging. Tidak butuh root atau PC; cukup Android 11+.
- **Efisien storage** — file OBB besar (1–4 GB) dikirim langsung dari sumbernya tanpa disalin
  dulu ke folder sementara, jadi tidak membutuhkan ruang 2× ukuran game.

## Setup Shizuku (untuk fitur kirim app)

1. Aktifkan **Wireless Debugging** di Pengaturan → Opsi developer (Android 11+)
2. Di VegamShare, buka halaman device → tap **Kirim app** → ikuti wizard **Setup Shizuku**
3. Lakukan sekali di HP pengirim maupun penerima

## Upgrading

- **Android:** install di atas versi lama, data tetap tersimpan.

## Downloads

- `VegamShare-0.2.2.apk`: Android 7.0+ (arm64, armv7, x86_64). Fitur kirim app membutuhkan Android 11+.
- `SHA256SUMS.txt`: checksums.

Lihat `RELEASE_NOTES_0.2.1.md` untuk perubahan sebelumnya.
