# VegamShare 0.2.0

## New
- **Works without Wi-Fi.** On an Android phone (8.0+) tap **Add → No Wi-Fi? Create a hotspot**. The
  phone starts a temporary hotspot without internet (no mobile data used) and shows a QR code. Scan
  it from VegamShare on the other phone: it joins the hotspot and connects by itself, pairing first
  if needed. A laptop can join the same hotspot from the Windows Wi-Fi menu; the name and password
  are shown under the QR code.
- **Direct LAN cable.** Two PCs connected with a cable (no router) now find each other on their
  automatic 169.254.x.x addresses.
- **Windows "Public" network warning.** Windows blocks incoming connections on Public networks. The
  home screen now says so, with a button to the network settings to switch to Private.
- **Reinstalled devices are cleaned up.** When a newly paired device has the same name and platform
  as a saved one (typically the same phone after reinstalling), VegamShare offers to remove the old
  entry.

## Improvements
- QR codes list the best addresses first (home Wi-Fi and hotspot before mobile data) and pairing
  tries all of them at once, so a QR with stale addresses no longer slows pairing down.
- Joining or leaving a hotspot moves VegamShare to the new network immediately; saved devices stay
  reachable without restarting the app.

## Notes
- Android changes the hotspot name and password each time it starts. Paired devices do not need to
  pair again; they just join the new hotspot (scan the QR again).
- Android asks for the *Nearby devices* permission (Android 13+) or *Location* (Android 8-12, with
  Location turned on) for the hotspot. This is an Android rule; VegamShare does not use location.
- The hotspot QR contains the hotspot password. Only show it to devices you want to connect.

## Upgrading
- **Windows:** run the new installer over the old version. Your data is kept.
- **Android:** install over 0.1.x from this GitHub release; your data is kept.

## Downloads
- `VegamShare-Setup-0.2.0.exe`: Windows 10/11 (x64). The installer is not code-signed yet, so
  SmartScreen may warn: *More info → Run anyway*.
- `VegamShare-0.2.0.apk`: Android 7.0+ (arm64, armv7, x86_64). The no-Wi-Fi hotspot needs Android 8.0+.
- `SHA256SUMS.txt`: checksums.
