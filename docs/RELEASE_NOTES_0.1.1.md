# VegamShare 0.1.1

## Changes
- **Pairing is recorded in History.** Entries read "Paired with <device>".
- **Reinstalled devices are recognised.** If a saved device was reinstalled (new identity) and still
  has the same address, VegamShare says so and offers **Pair again**, instead of only showing
  "offline".
- **Clearer "cannot be reached" message**, with what to check.
- **Removed the "Visible on the network" setting.** Devices are only visible while the **Add device**
  screen is open, which is all pairing needs. Anyone who had turned it on in 0.1.0 is switched back
  to silent.
- **Removing a saved device also cancels its unfinished transfers**, so they no longer show as
  resumable.
- Faster failure when a device is simply offline; history lookups no longer scan the whole table.

## Upgrading
- **Windows:** run the new installer over the old version. Your data is kept.
- **Android:** install over 0.1.0 if that came from this GitHub release; your data is kept. Only if
  you had a test build signed with another key, uninstall first (this resets the device ID; pair
  again afterwards).

## Downloads
- `VegamShare-Setup-0.1.1.exe`: Windows 10/11 (x64). The installer is not code-signed yet, so
  SmartScreen may warn: *More info → Run anyway*.
- `VegamShare-0.1.1.apk`: Android 7.0+ (arm64, armv7, x86_64).
- `SHA256SUMS.txt`: checksums.
