# VegamShare 0.4.0

Send WhatsApp files in a few taps, update the app from inside it, and a "No Wi-Fi" hotspot that
no longer needs you to switch Wi-Fi or tethering yourself.

## What's new
- **WhatsApp files (Android):** a new screen lists what WhatsApp saved on the phone, newest first,
  in two tabs as in WhatsApp: **Documents** (with audio and voice notes) and **Photos & videos**
  (thumbnail grid). Tap to view (photos open inside VegamShare), tick several and send. Open it
  from the chat icon on the Devices screen or **WhatsApp files** on a device's page. Files you sent
  yourself are hidden unless you turn on *Include sent by me*. WhatsApp Business is included.
- **Updates inside the app (Android and Windows):** Settings → **Check for updates** looks for a
  new release on GitHub and shows its release notes. **Update** downloads it and opens the system
  installer; Android only accepts it when it is signed with the same key as the installed app.
  The downloaded APK/installer is deleted afterwards, so it takes no storage. An automatic daily
  check can be switched on in Settings (off by default).
- **"No Wi-Fi" hotspot, without fiddling:**
  - Joining phone: if Wi-Fi is off, VegamShare turns it on (directly with the app data helper,
    otherwise via the Wi-Fi panel) and then joins the hotspot by itself. Just scan.
  - Hotspot phone: if the normal hotspot (tethering) is on, VegamShare says so and opens the
    hotspot settings; when you come back it starts its own hotspot by itself. Same for Location
    on Android 8-12.
  - A notification shows while the VegamShare hotspot is on. It is a hotspot only for VegamShare
    (no internet), so Android does not show it as "Hotspot" in quick settings.

## Good to know
- Checking for updates is the only time VegamShare goes to the internet: one request to
  api.github.com when you tap it (or once a day while the app is open, if you switch that on).
- The first update from inside the app asks once to allow *Install unknown apps* for VegamShare.

## Upgrading
- **Android:** install over 0.3.0; your data is kept. From now on updates arrive in the app.
- **Windows:** run the new installer over 0.2.1; settings and saved devices are kept. From now on
  updates arrive in the app.

## Downloads
- `VegamShare-Setup-0.4.0.exe`: Windows 10/11 (x64). Not code-signed yet, so SmartScreen may warn:
  *More info → Run anyway*.
- `VegamShare-0.4.0.apk`: Android 7.0+ (arm64, armv7, x86_64).
- `SHA256SUMS.txt`: checksums.
