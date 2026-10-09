# VegamShare security model

## What is protected

| Goal | How |
|---|---|
| Nobody on the network can read transferred files or file names | QUIC/TLS 1.3 with ephemeral ECDHE (forward secrecy). All requests, manifests and file data travel inside it. |
| A device cannot pretend to be one you paired with | The peer's certificate fingerprint (SHA-256) is pinned at pairing time. Every connection runs a full handshake (no session resumption) and must prove possession of the pinned Ed25519 key. The device ID and discovery packets are never used for authentication. |
| The password cannot be sniffed or brute-forced offline | Pairing uses SPAKE2 (a PAKE). The password never crosses the network, and each connection tests at most one guess. |
| Pairing cannot be intercepted (MITM) | The SPAKE2 identity includes a TLS exporter value, and the confirmation MACs cover both fingerprints. A relay running two separate TLS sessions derives mismatching keys and fails. |
| Password-free pairing cannot be hijacked | Tap-to-pair shows a 6-digit code derived from the TLS session on both screens. A man in the middle has two sessions and cannot make the codes match (Bluetooth-style numeric comparison). Requests are accepted only while the "Add device" screen is open, one at a time. QR pairing pins the fingerprint printed in the QR and spends a single-use 128-bit token valid for two minutes. |
| Online guessing is slow | 5 free attempts, then 30 s → 1 h exponential back-off. |
| A received manifest cannot write outside the download folder | Strict path validation on the receiver (`pathsafe.rs`): no `..`, absolute or drive paths, Windows reserved names, illegal characters, trailing dots or spaces, or case-insensitive duplicates. Existing symlinks along the path are refused. |
| Received files never overwrite the user's files | Colliding names get ` (n)` suffixes. The final rename re-checks under a lock. |
| Corruption, or tampering with a partial file, is detected | Every file is verified end-to-end with BLAKE3 over the whole content, including resumed parts, before it is renamed into place. |
| Keys at rest | The identity key and the password are sealed with DPAPI on Windows (current user). On Android they live in the app-private data folder. |
| Unpaired devices can do almost nothing | Before pairing, only `ping` and `pair_start` are served. Offers and files require a pinned fingerprint, which is re-checked on every request. "Forget device" takes effect immediately and closes live connections. |
| Peer-supplied text cannot spoof the UI | Names are stripped of control and bidi-override characters and length-limited. |

## Known limits (by design)

* **Discovery is silent by default.** A device never announces itself. It only
  answers a query naming its exact 9-digit ID, which a stranger would have to
  know already, and they would still need the password. The answer, and the
  announcements sent while the "Add device" screen is open, are plaintext: they
  reveal name, ID and IP to others on the same Wi-Fi.
* **"Online" status can be spoofed** by forged announcements. This is cosmetic
  only: a forged device fails the pinned handshake, so nothing is sent to it.
* **The password is stored reversibly.** Symmetric SPAKE2 needs it on the
  listener, so it is sealed but not hashed. Anyone with access to the user's
  Windows account can read it, but they could also read the files anyway.
* **A LAN attacker can trigger the pairing lockout** (denial of service for
  new pairings). Already paired devices are not affected.
* **Paired devices are trusted.** With auto-accept on, which is the default
  per device, a paired device can fill the download folder. It cannot
  overwrite or read anything. Turn auto-accept off per device to be asked
  first.
* **Password strength.** The generated password is 8 characters from a
  31-symbol alphabet (~40 bits). This is plenty against a rate-limited online
  attack. User-chosen passwords must be 6–64 characters.

## Cryptographic building blocks

Only well-reviewed libraries are used; no custom cryptography:
`rustls` (ring backend) and `quinn` for TLS 1.3/QUIC, `rcgen` for the
self-signed certificate, `spake2` (RustCrypto) for PAKE, `hmac` + `sha2`,
`blake3`, `subtle` for constant-time comparison, and OS randomness via
`getrandom`.

## Verification

* `cargo test`: unit tests, end-to-end tests between real nodes (wrong password,
  lockout, impostor at a known address, forgotten device, decline,
  pause/restart/resume), and property-based fuzzing (`proptest`). The fuzzing
  covers path validation, offer validation and planning (accepted offers never
  write outside the destination and never collide), and all wire-format parsers
  (they never panic on arbitrary bytes). The fuzz tests assert their own
  coverage, so they cannot pass with only rejected inputs.
* `cargo audit`: dependency advisories (RustSec). 0 findings at v0.1.0.
* Release APKs refuse to build without a private signing key.
