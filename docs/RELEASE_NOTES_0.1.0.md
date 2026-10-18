# VegamShare 0.1.0 (pre-release)

First release, **beta** status. Tested on Windows 11 and Android 16 emulator, but not yet on many
physical devices. Please report issues via the Issues tracker.

## Features
- Send files and **entire folders** between Android ↔ Windows over the same Wi-Fi/LAN, with no internet or server required.
- **Pair once**, with your choice of method: tap a nearby device and match a 6-digit code, scan a QR
  code, or enter a 9-digit ID + password (or IP address). After that, just click — even if the IP changes.
- **Battery-friendly, like Bluetooth earbuds**: the app is completely silent until you send something.
  No network announcements and no background polling.
- **Ready to receive** On/Off toggle.
- **Automatic resume** if the connection drops, with per-file verification (BLAKE3).
- **End-to-end encryption** (QUIC/TLS 1.3), SPAKE2 pairing, and device identity pinning.
- Windows: system tray, Explorer *Send to* menu, drag & drop. Android: *Share to VegamShare*, built-in file browser.

## Downloads
- `VegamShare-Setup-0.1.0.exe`: Windows 10/11 installer (x64). The installer is not yet digitally
  signed, so Windows SmartScreen will show a warning. Choose *More info → Run anyway*.
- `VegamShare-0.1.0.apk`: Android 7.0+ (arm64, armv7, x86_64); tested on Android 16 only.

## Notes
- Make sure your Wi-Fi network profile on Windows is set to **Private**. The firewall rule only opens for Private networks.
- Android requires *Manage all files* permission (for sending folders) and notification permission.
- Security model and limitations: [docs/SECURITY.md](SECURITY.md).
