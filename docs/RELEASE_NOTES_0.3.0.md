# VegamShare 0.3.0

Send installed apps **with their game data** to another phone, and clone games into the phone's second
space. Replaces 0.2.2, whose "Send app" did not work; please update.

## What's new
- **Send app with data (Android):** pick an installed app on a device's page. VegamShare sends the APK
  (with split APKs), the game data in `Android/data` and the OBB files, so a big game does not download
  again. Example: Mobile Legends with ~11 GB of data in about 13 minutes over home Wi-Fi.
  - Nothing is copied first on the sending phone: data is streamed straight from where it is.
  - The receiving phone installs the app and puts the data back in place with one tap (**Install**),
    then removes the received files.
- **Clone apps (Android):** tap **Clone apps** on the Devices screen to clone a game into the phone's
  second space (Dual apps, App cloner, work profile, Secure Folder, Private space) and copy its
  downloaded data, so the clone does not download gigabytes again. Replaces the separate VegamClone app.
- **App data access without extra apps:** VegamShare starts its own small helper through *Wireless
  debugging* (Android 11+). No PC, no root, no Shizuku app. Settings → **App data access** → Setup:
  the pairing port is found automatically and the 6-digit code is typed into the VegamShare
  notification or an optional floating number pad. Only once per phone: after a restart, turning
  Wireless debugging on is enough.
- **Progress and control:** a dialog while an app is prepared (with Cancel), then the Transfer tab
  shows progress, speed and time left with Pause and Stop. Installing a received app shows each stage
  with a progress bar and Cancel; a cancelled restore keeps the received files for another try.
- **Clear messages:** a warning before sending when the game data cannot be included, and readable
  install errors (newer version already installed, install via USB blocked on Xiaomi, different
  signature, unsupported processor, not enough space).

## Good to know
- **Logins usually stay behind.** Most apps keep their login in private storage that only the system
  or root can read. Sign in with the game account (Moonton, Google Play Games, …) on the other phone;
  the downloaded data is what VegamShare saves you from fetching again.
- Caches and files a game keeps locked (such as chat databases) are left out on purpose.
- The receiving phone needs about twice the game data size free while restoring.
- Send app and Clone apps need Android 11+ and Wireless debugging (Developer options). Apps that
  refuse to run with Developer options on (some banking apps) work again once you turn them off.
- Tested on Xiaomi/POCO (HyperOS, Android 16) and realme (ColorOS, Android 15).

## Fixed since 0.2.2
- 0.2.2 could not be installed next to the Shizuku app ("conflicting provider"), and its app data
  access never started without that app. VegamShare no longer contains Shizuku.
- Send app never sent game data, left APKs loose in `Download/VegamShare` and did not install the
  app. An app now arrives as one folder (`apk/`, `data.tar`, `obb/`) and is restored completely.
- OBB files went to a wrong folder.
- Pairing failed on Android 16.

## Upgrading
- **Android:** install over 0.2.x; your data is kept. If 0.2.2 refused to install, this version installs.
- **Windows:** no changes; keep using 0.2.1.

## Downloads
- `VegamShare-0.3.0.apk`: Android 7.0+ (arm64, armv7, x86_64). Send app with data and Clone apps need
  Android 11+.
- `SHA256SUMS.txt`: checksums.
