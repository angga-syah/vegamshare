# VegamShare 0.2.1

Bug fixes for 0.2.0.

## Fixes
- **Joining a hotspot from the QR code crashed** with "PanicException: there is no reactor running"
  right after the phone joined, so it never connected (tapping the device afterwards ended in
  "Timed out"). Joining now continues straight to pairing as intended.
- **Pairing timed out when "Ready to receive" was off on the other device.** A device on its
  **Add device** screen is visible, so it now also accepts pairing there. Incoming files are still
  refused while "Ready to receive" is off.
- **Re-joining a hotspot with an already paired phone** now also tells the hotspot phone where it is,
  so that phone can send right away too (before, only the joining phone could start a transfer).

## Upgrading
- **Windows:** run the new installer over the old version. Your data is kept.
- **Android:** install over 0.2.0 / 0.1.x from this GitHub release; your data is kept.

## Downloads
- `VegamShare-Setup-0.2.1.exe`: Windows 10/11 (x64). Not code-signed yet, so SmartScreen may warn:
  *More info → Run anyway*.
- `VegamShare-0.2.1.apk`: Android 7.0+ (arm64, armv7, x86_64). The no-Wi-Fi hotspot needs Android 8.0+.
- `SHA256SUMS.txt`: checksums.

See `RELEASE_NOTES_0.2.0.md` for everything new in 0.2.
