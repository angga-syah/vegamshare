# VegamShare 0.2.3

Fixes "Send app" from 0.2.2 and adds "Clone apps". Please update from 0.2.2.

## Fixes
- **0.2.2 could not be installed next to the Shizuku app** (or other apps embedding it):
  Android refused with "conflicting provider". VegamShare no longer contains Shizuku.
- **App data access never started in 0.2.2.** The embedded Shizuku server needs the separate Shizuku
  app and quit without it. VegamShare now starts its own small helper through Wireless debugging.
- **Pairing failed on Android 16** (blocked internal TLS API); it now uses the public API.
- **Send app did not work end to end in 0.2.2:** game data was never sent, the APKs landed loose in
  `Download/VegamShare`, and the received app was not installed. Now:
  - an app arrives as one folder (`apk/`, `data/`, `obb/`);
  - `Android/data` and OBB are streamed straight from the sender through the helper, so a big game
    (e.g. MLBB with ~11 GB in `Android/data`) needs no extra free space on the sending phone; caches are
    left out;
  - the receiver installs the app (base + split APKs) through the helper without extra prompts, then
    moves the data into `Android/data` / `Android/obb`, and removes the received folder when done;
  - OBB files go to `Android/obb/<app>` (0.2.2 used a wrong folder).

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
- **Sending without app data access** warns first (Setup, or send the APK only) instead of quietly
  sending only the APK.
- **Progress and control:** preparing an app shows a dialog with Cancel, then the Transfer tab shows
  progress, speed and time left with Pause and Stop. Installing a received app shows each stage
  (install, data, OBB) with a progress bar and Cancel; a cancelled restore keeps the received files.
- **Readable install errors:** newer version already installed, install via USB blocked (Xiaomi),
  different signature, unsupported processor, not enough space.
- **Floating number pad** for the pairing code (optional "Display over other apps"): the code stays
  visible in Settings while typing; drag it out of the way if needed.
- **Clone apps:** on phones that need the clone opened once (e.g. ColorOS), VegamShare now opens it,
  waits for its data folder, closes it and copies on its own; files a game keeps locked (such as chat
  databases) are skipped instead of reported as errors.
- **Setup without mDNS:** if the phone does not announce the wireless debugging port, VegamShare finds
  it on its own.

## Upgrading
- **Android:** install over 0.2.x; your data is kept. If 0.2.2 refused to install, this version installs.
- **Windows:** no changes; keep using 0.2.1.

## Downloads
- `VegamShare-0.2.3.apk`: Android 7.0+ (arm64, armv7, x86_64). Send app with data and Clone apps need
  Android 11+.
- `SHA256SUMS.txt`: checksums.

See `RELEASE_NOTES_0.2.2.md` for the send app feature.
