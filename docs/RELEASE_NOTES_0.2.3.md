# VegamShare 0.2.3

Fixes "Send app" from 0.2.2 and adds "Clone apps". Please update from 0.2.2.

## Fixes
- **0.2.2 could not be installed next to the Shizuku app** (or other apps embedding it):
  Android refused with "conflicting provider". VegamShare no longer contains Shizuku.
- **App data access never started in 0.2.2.** The embedded Shizuku server needs the separate Shizuku
  app and quit without it. VegamShare now starts its own small helper through Wireless debugging.
- **Pairing failed on Android 16** (blocked internal TLS API); it now uses the public API.

## What's new
- **Clone apps (Android):** tap **Clone apps** on the Devices screen to clone a game into the phone's
  second space (Dual apps, App cloner, work profile, Secure Folder, Private space) and copy its
  downloaded data, so the clone does not download gigabytes again. Replaces the separate VegamClone app.
- **Easier setup of app data access:** Settings → App data access → Setup. The pairing port is found
  automatically; type the 6-digit code into the VegamShare notification while the code stays on
  screen, and VegamShare comes back by itself. After a restart only Wireless debugging has to be
  turned on; no code is needed.
- **Receiving an app without the helper running** now offers the setup first and then continues the
  install, instead of silently leaving out the game data.

## Upgrading
- **Android:** install over 0.2.x; your data is kept. If 0.2.2 refused to install, this version installs.
- **Windows:** no changes; keep using 0.2.1.

## Downloads
- `VegamShare-0.2.3.apk`: Android 7.0+ (arm64, armv7, x86_64). Send app with data and Clone apps need
  Android 11+.
- `SHA256SUMS.txt`: checksums.

See `RELEASE_NOTES_0.2.2.md` for the send app feature.
