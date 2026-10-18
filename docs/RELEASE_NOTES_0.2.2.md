# VegamShare 0.2.2

New feature: send apps — transfer a complete Android app (APK + data + OBB) to another phone over LAN.

## What's new

- **Send app** — select an installed app, and VegamShare sends the APK + Android/data + OBB to another
  phone on the same network. Useful for large games like MLBB that take a long time to re-download.
- **Auto-install** — the receiver gets a notification when the app arrives and can install it directly
  from VegamShare without leaving the app (uses the PackageInstaller API; no root required).
- **Shizuku embedded** — access Android/data (system-protected folder) via Wireless Debugging. No root
  or PC needed; requires Android 11+.
- **Storage-efficient** — large OBB files (1–4 GB) are sent directly from their source without being
  copied to a temp location first, so no 2× storage overhead.

## Shizuku setup (required for send app)

1. Enable **Wireless Debugging** in Settings → Developer options (Android 11+)
2. In VegamShare, open a device page → tap **Send app** → follow the **Setup Shizuku** wizard
3. Do this once on both the sending and receiving phone

## Upgrading

- **Android:** install over the previous version; your data is preserved.

## Downloads

- `VegamShare-0.2.2.apk`: Android 7.0+ (arm64, armv7, x86_64). The send app feature requires Android 11+.
- `SHA256SUMS.txt`: checksums.

See `RELEASE_NOTES_0.2.1.md` for previous changes.
