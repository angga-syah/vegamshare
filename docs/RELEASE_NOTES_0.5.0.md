# VegamShare 0.5.0

A new Android app id, a much faster WhatsApp screen, app icons when sending apps, a right-click share on Windows, and a helper that starts by itself.

## Important for Android: install it as a new app
Android now asks developers to register their apps ([Android developer verification](https://developer.android.com/developer-verification)). The old id `com.vegamshare.vegamshare` could not be registered, so from 0.5.0 VegamShare is **`id.linuwih.vegamshare`**, signed with a new key.

- Android sees it as a **new app**: it installs next to the old VegamShare, not over it. If *Check for updates* in 0.4.x offers it, it is installed as a second app too.
- The new app starts fresh: new ID and password, no saved devices, and the helper (Wireless debugging) has to be set up once more. Pair your devices again, then uninstall the old VegamShare.
- Files you received stay where they are (`Download/VegamShare`).

## What's new
- **WhatsApp files open in under a second (Android):** the list now comes from Android's media index instead of checking every file one by one (about 34,000 files: from ~12 s to under a second; the *Sent* folders, which Android indexes only in part, fill in right after). Photo and video thumbnails come from the gallery's own cache, and fast scrolling no longer queues thumbnails for tiles you only flew past.
- **App icons when sending apps (Android):** each app shows its own icon instead of the generic Android one.
- **More apps to send (Android):** preinstalled apps that were updated from a store, such as realme's File Manager, are now listed. Whether they install on the other phone depends on that phone.
- **Helper starts by itself (Android 11+):** after the first pairing, VegamShare turns Wireless debugging on and starts the helper whenever sending or installing an app needs it, also after a restart: no Settings, no pairing code. It needs Wi-Fi, as Wireless debugging itself does. The pairing code is still asked once per phone (Android requires it).
- **"Share with VegamShare" in Explorer (Windows):** right-click files or folders and choose *Share with VegamShare*; several selected files become one share. On Windows 11 it is under *Show more options*. The *Send to* shortcut is still offered by the installer, unticked.

## Upgrading
- **Android:** install the new APK (see above), set it up, then uninstall the old VegamShare.
- **Windows:** run the new installer over 0.4.x (or update from inside the app); settings and saved devices are kept. Devices paired with your old Android app need pairing again with the new one.

## Download
| Platform | File | Size | Notes |
|:--|:--|--:|:--|
| **Windows** 10 / 11 | [VegamShare-Setup-0.5.0.exe](https://github.com/angga-syah/vegamshare/releases/download/v0.5.0/VegamShare-Setup-0.5.0.exe) | 12.7 MB | 64-bit installer |
| **Android** 7.0+ | [VegamShare-0.5.0.apk](https://github.com/angga-syah/vegamshare/releases/download/v0.5.0/VegamShare-0.5.0.apk) | 89.5 MB | arm64, armv7, x86_64; new app id |

Checksums: [SHA256SUMS.txt](https://github.com/angga-syah/vegamshare/releases/download/v0.5.0/SHA256SUMS.txt). The Windows installer is not code-signed yet, so SmartScreen may warn: *More info → Run anyway*.
