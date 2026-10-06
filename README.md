# VegamShare

Kirim file dan folder antar HP dan komputer (Android dan Windows, ke arah mana saja) di Wi-Fi, LAN,
atau hotspot yang sama.

- **Pairing cukup sekali**, dengan cara apa saja: **ketuk perangkat di sekitar** lalu cocokkan kode
  6 digit (seperti pairing Bluetooth), **pindai QR**, atau **ID + password** (bisa juga lewat IP).
  Setelah itu perangkat tersimpan dan untuk mengirim tinggal klik, meskipun IP-nya berubah.
- Mendukung kirim **folder utuh** (subfolder kosong ikut), **resume** jika terputus, **enkripsi
  end-to-end**, auto-accept per perangkat, dan riwayat.
- Tidak ada server, dan data tidak pernah keluar dari jaringan lokal.

Detail desain ada di [docs/PROTOCOL.md](docs/PROTOCOL.md) dan [docs/SECURITY.md](docs/SECURITY.md).

## Struktur

| Folder | Isi |
|---|---|
| `core/` | Rust: identitas, discovery, pairing SPAKE2, QUIC/TLS dengan pinning, mesin transfer, SQLite |
| `cli/` | `vegamshare-cli`, untuk uji antar mesin tanpa UI |
| `app/` | Flutter (Android + Windows), bridge `app/rust` via flutter_rust_bridge |
| `installer/` | Skrip Inno Setup untuk Windows |
| `dist/` | Hasil build lokal (tidak di-commit; diunggah ke GitHub Releases) |

## Instalasi

- **Windows:** jalankan `dist/VegamShare-Setup-x.y.z.exe`. Installer menambahkan aturan firewall khusus
  jaringan *Private* dan menu Explorer **Kirim ke → VegamShare**. Pastikan profil Wi-Fi Anda *Private*.
- **Android:** pasang `dist/VegamShare-x.y.z.apk`, lalu beri izin *akses semua file* (untuk mengirim folder
  dan menyimpan ke `Download/VegamShare`) dan izin notifikasi (agar tetap siap menerima di latar belakang).

## Build

Prasyarat: Rust (stable), Flutter (stable), Android SDK + NDK 28.2, Visual Studio (Desktop C++),
dan **Developer Mode Windows aktif** (Flutter butuh symlink).

```bash
cargo test                                    # unit + end-to-end + fuzz (proptest)
cargo audit                                   # cek kerentanan dependensi
cd app && flutter build windows --release
cd app && flutter build apk --release
"C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\vegamshare.iss
```

Setelah mengubah API di `app/rust/src/api/`, jalankan `flutter_rust_bridge_codegen generate` di `app/`.

Catatan build (sudah diterapkan):

- `app/android/gradle.properties`: `kotlin.incremental=false`, karena project dan pub cache berada di
  drive berbeda. `kotlin.jvm.target.validation.mode=warning`, karena beberapa plugin lama memakai
  target JVM yang tidak seragam.
- Versi plugin dipin: `file_picker ^10.3` dan `permission_handler ^12`. Versi lebih baru menuntut
  compileSdk 37 atau Kotlin bawaan AGP 9 yang dimatikan oleh template Flutter.
- Build APK release **menolak jalan tanpa kunci tanda tangan** (lihat di bawah). Untuk build uji lokal
  saja: `flutter build apk --release -Pvegamshare.debugSigning=true`.

## Kunci tanda tangan APK (sekali saja)

Kunci ini identitas aplikasi Anda selamanya. Kalau hilang, Anda tidak bisa merilis update; kalau bocor,
orang lain bisa membuat APK palsu atas nama Anda. Simpan cadangannya di tempat aman (bukan di repo).

```bash
"C:\Program Files\Android\Android Studio\jbr\bin\keytool.exe" -genkeypair -v -keystore D:\kunci\vegamshare-release.jks -alias vegamshare -keyalg RSA -keysize 4096 -validity 10000
```

Lalu buat `app/android/key.properties` (sudah di-.gitignore):

```properties
storeFile=D:/kunci/vegamshare-release.jks
storePassword=<password keystore>
keyAlias=vegamshare
keyPassword=<password kunci>
```

## Uji manual dengan CLI

```bash
cargo run -p vegamshare-cli -- --name "PC-Uji"
```

Ketik `help` untuk daftar perintah: `pair <id> <password> [ip]`, `send <#> <path>`, `nearby`, `history`, dan lainnya.

## Lisensi

VegamShare dirilis di bawah lisensi [MIT](LICENSE). Pustaka pihak ketiga tetap memakai lisensinya masing-masing.
Untuk komponen Rust, daftarnya ada di [THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) (dibuat dengan
`cargo about generate -m app/rust/Cargo.toml about.hbs -o THIRD_PARTY_LICENSES.txt`). Untuk paket Flutter,
lihat menu Pengaturan → Tentang di aplikasi.
