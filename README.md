# VegamShare

**Fast, private file sharing between Android and Windows over your local Wi-Fi.**
Send files and whole folders phone ↔ PC, phone ↔ phone or PC ↔ PC, with no internet, no cloud
and no account. Pair a device once, then just tap to send.

**[⬇ Download the latest release](https://github.com/angga-syah/vegamshare/releases/latest)**:
Windows installer (`.exe`) and Android app (`.apk`).

## Features

- **Any direction:** Android → Windows, Windows → Android, Android → Android, Windows → Windows.
- **Whole folders:** subfolders (including empty ones) arrive exactly as they were sent.
- **Pair once, then one tap:** saved devices are found again automatically, even when their IP address
  changes.
- **Several ways to pair:** tap a nearby device and compare a 6-digit code, scan a QR code, or enter
  the device ID and password (optionally with an IP address).
- **Resumes interrupted transfers:** if Wi-Fi drops, the transfer continues where it stopped. Every file
  is checked end to end with BLAKE3.
- **No Wi-Fi needed:** an Android phone can create a temporary hotspot (no internet, no mobile data
  used); the other phone scans the QR code and joins it automatically. A direct LAN cable between two
  PCs works too.
- **Encrypted:** QUIC / TLS 1.3. Devices verify each other's keys, and no data ever leaves your network.
- **Battery friendly:** completely idle until you send something. It does no background scanning and
  makes no announcements on the network.
- **Convenient:** drag & drop and Explorer *Send to* on Windows; *Share to VegamShare* from any Android
  app; per-device auto-accept; transfer history.

## How to use

1. Install VegamShare on both devices and connect them to the same Wi-Fi, LAN or hotspot. No Wi-Fi?
   On an Android phone tap **Add → No Wi-Fi? Create a hotspot** and scan its QR code from the other
   phone; a laptop connects to the hotspot shown there.
2. On both devices tap **Add**. The other device appears in the list: tap it and confirm that the same
   6-digit code shows on both screens. You can also scan a QR code or use ID + password.
3. Done. From now on, pick the saved device and send files or folders.

On Windows, keep your Wi-Fi network profile set to **Private**; the installer opens the firewall for
private networks only, and the app warns when the current network is Public. On Android, allow *all files access* (to send folders and save received files to
`Download/VegamShare`) and notifications (to stay ready in the background).

The Windows installer is not code-signed yet, so SmartScreen may warn on first run: choose
*More info → Run anyway*.

## Security

VegamShare uses well-reviewed libraries only: `rustls`/`quinn` (TLS 1.3 over QUIC), SPAKE2 for password
pairing, numeric-comparison codes for tap-to-pair, certificate pinning and BLAKE3. Details and known
limitations are in [docs/SECURITY.md](docs/SECURITY.md); the wire format is in
[docs/PROTOCOL.md](docs/PROTOCOL.md).

## Building from source

| Folder | Contents |
|---|---|
| `core/` | Rust core: identity, discovery, pairing, QUIC/TLS with pinning, transfer engine, SQLite |
| `cli/` | `vegamshare-cli`, a command-line client for testing |
| `app/` | Flutter app (Android + Windows); Rust bridge in `app/rust` via flutter_rust_bridge |
| `installer/` | Inno Setup script for the Windows installer |

Requirements: Rust (stable), Flutter (stable), Android SDK + NDK 28.2, Visual Studio with Desktop C++,
and Windows **Developer Mode** enabled (Flutter needs symlinks).

```bash
cargo test                                   # unit, end-to-end and property-based tests
cargo audit                                  # dependency advisories
cd app && flutter build windows --release
cd app && flutter build apk --release
"C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\vegamshare.iss
```

After changing the API in `app/rust/src/api/`, run `flutter_rust_bridge_codegen generate` in `app/`.

Build notes:

- `app/android/gradle.properties` sets `kotlin.incremental=false` (project and pub cache on different
  drives) and `kotlin.jvm.target.validation.mode=warning` (some plugins mix JVM targets).
- `file_picker ^10.3` and `permission_handler ^12` are pinned: newer versions need compileSdk 37 or
  AGP 9 built-in Kotlin, which the Flutter template disables.
- Release APKs refuse to build without a signing key. For a local test build only:
  `flutter build apk --release -Pvegamshare.debugSigning=true`.
- Bump `version` in `app/pubspec.yaml` (e.g. `0.2.1+4`) for every build you share, so installed
  versions can be told apart and Android refuses to downgrade.

### Signing key (once)

To build your own release APK you need **your own** signing key. It identifies the app for good: lose it
and you can no longer ship updates; leak it and others can sign fake APKs in your name. Keep a backup
outside the repository, and never create a second one for the same app.

```bash
keytool -genkeypair -v -keystore <path>/vegamshare-release.jks -alias vegamshare -keyalg RSA -keysize 4096 -validity 10000
```

`keytool` ships with Java / Android Studio (e.g. `<Android Studio>\jbr\bin\keytool.exe`). Then create
`app/android/key.properties` (git-ignored):

```properties
storeFile=<path>/vegamshare-release.jks
storePassword=<keystore password>
keyAlias=vegamshare
keyPassword=<key password>
```

### Command-line client

```bash
cargo run -p vegamshare-cli -- --name "Test PC"
```

Type `help` for commands, e.g. `pair <id> <password> [ip]`, `pairnear <id>`, `qr`, `send <#> <path>`,
`history`.

## License

VegamShare is released under the [MIT License](LICENSE). Third-party components keep their own licenses:
Rust dependencies are listed in [THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) (generated with
`cargo about generate -m app/rust/Cargo.toml about.hbs -o THIRD_PARTY_LICENSES.txt`); Flutter packages
are listed in the app under Settings → About.
