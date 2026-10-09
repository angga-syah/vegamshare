# VegamShare protocol v1

All traffic stays on the local network. There is no server.

## Transport

* QUIC (RFC 9000) over UDP, default port **47901**, ALPN `vegamshare/1`.
* TLS 1.3 only, Ed25519 certificates only, mutual authentication.
  Each device has one long-lived self-signed certificate. Its SHA-256 is the
  device **fingerprint**. No CA validation is performed; identity is decided
  by fingerprint pinning (see `SECURITY.md`).
* Every request opens its own bidirectional stream. Control messages are
  frames: `u32 big-endian length` + JSON body. Max 1 MiB, or 64 MiB for
  offers from a paired device.

## Discovery (UDP port 47900)

Devices are **silent by default**: nothing is announced and nothing is polled.
JSON datagrams (max 1 KiB) are used only on demand:

```json
{"app":"vegamshare","v":1,"kind":"query","id":"<asker>","name":"...","platform":"...","port":47901,"fp":"<8 bytes hex>","want":"123456789"}
```

* `query` with `want` is sent (multicast `239.255.77.77`, `255.255.255.255`,
  directed broadcast; once more after 0.5 s) when pairing by ID, or when a saved
  device does not answer at its last known address. Only the device whose ID
  equals `want` replies with an `announce`, and only while "ready to receive" is
  on. The reply goes unicast to the asker and via multicast/broadcast. It is
  rate-limited to one per second per asker.
* `announce` packets are sent every 5 s only while the device's **Add device**
  screen is open (pairing mode, at most 2 minutes per renewal); `query` without
  `want` is answered only in that state. There is no permanent "visible" mode.

Announcements are hints only and are never trusted for identity.

## Presence

There is no background polling. A device counts as online for 2 minutes after a
verified connection in either direction (a transfer, pairing, or the user
pressing "check connection"). A single timer per contact flips it back. With
"ready to receive" off, incoming connections and ID queries are ignored, so
the device looks absent.

When a saved device cannot be reached, the client checks whether something
answered at its last address (a TLS failure rather than a timeout). If so, it
pings that address without pinning. If a different key answers there under the
saved device's name, and that key is not another saved device, the user is told
the device was probably reinstalled and is offered to pair again. Otherwise the
device is reported as unreachable. The pinned connection itself is never
weakened: nothing is sent to the other key.

## Requests

| Request | Allowed from | Response |
|---|---|---|
| `{"type":"ping","info":PeerInfo}` | anyone | `pong {info, trusted}` |
| `{"type":"pair_start","info":PeerInfo,"spake":hex}` | anyone | see Pairing |
| `{"type":"offer","info":PeerInfo,"offer":Offer}` | paired | `accepted {offsets}` / `rejected` / `error` |
| `{"type":"file","transfer_id","index","offset"}` + bytes + 32-byte BLAKE3 | paired | `ok` / `error` |
| `{"type":"resume","transfer_id"}` | paired | `ok` / `error` |
| `{"type":"pair_ask","info":PeerInfo}` | anyone, only while the listener is on its "Add device" screen | `pair_ok {info}` / `rejected` / `error pairing_closed` |
| `{"type":"pair_token","info":PeerInfo,"token":hex}` | anyone holding a valid QR token | `pair_ok {info}` / `error invalid_code` |

`PeerInfo = {device_id, name, platform, version}`.
Errors: `{"type":"error","code":"not_trusted|wrong_password|locked_out|bad_request|busy|hash_mismatch|unknown_transfer|internal","message","retry_after"}`.

## Pairing

1. Client dials without pinning and sends `pair_start` with its SPAKE2
   message.
2. Both sides run symmetric SPAKE2 (Ed25519 group) over the password with
   identity `"vegamshare-pair-v1:" || exporter`, where `exporter` is 32
   bytes of TLS keying material (label `EXPORTER-vegamshare-pairing`).
3. Server answers `pair_challenge {spake}`.
4. Client sends `{"mac": HMAC-SHA256(K, "client" || fp_client || fp_server)}`.
5. Server verifies it and answers `pair_accepted {mac: HMAC(K, "server" || ...), info}`,
   or `error wrong_password`.
6. Both store the other's fingerprint. Future connections pin it, and no
   password is needed any more.

The listener allows 5 attempts, then requires 30 s between attempts,
doubling up to 1 h. It resets after success or 15 quiet minutes.

## Password-free pairing

**Tap + code.** The listener's "Add device" screen turns on pairing mode for
two minutes at a time (renewed while open). During pairing mode the device is
temporarily visible (announces). The asker dials without pinning and sends
`pair_ask`. Both sides derive a 6-digit code from TLS keying material (label
`EXPORTER-vegamshare-sas`, 8 bytes, big-endian, mod 10^6). The listener shows
the code and asks its user; the asker shows the same code. On acceptance the
listener answers `pair_ok` and both pin each other's fingerprint. Only one
request is pending at a time; it expires after 60 s.

**QR code.** `vegamshare://pair?v=1&id=<device id>&fp=<64 hex>&a=<ip:port,...>&t=<32 hex>`.
The token is random, single-use and valid for 2 minutes (at most 4 live). The
scanner dials the listed addresses with `fp` pinned (falling back to an ID
lookup) and sends `pair_token`. The listener checks the token in constant time
under the same rate limit as passwords, then answers `pair_ok`.

Every method is needed only once per pair of devices.

## Transfers

* The sender opens a new connection per transfer and sends an `Offer`:
  `{transfer_id: 32 lowercase hex, entries: [{path, is_dir, size, mtime}]}`.
  Paths are `/`-separated and relative. Folders are listed before their
  contents. Empty folders are included.
* The receiver validates every path (no `..`, no absolute or drive paths,
  no Windows reserved names or illegal characters, no case-insensitive
  duplicates, no file that is also a parent folder). It then asks the user,
  unless auto-accept is on for that device.
* `accepted.offsets[i]` is the byte offset at which to continue file `i`
  (`2^64-1` = already complete). A new transfer has all offsets 0.
* Up to 4 files stream in parallel. Each `file` stream carries exactly
  `size - offset` bytes, then the BLAKE3 hash of the **whole** file. The
  receiver writes to `<name>.vegamshare-part`, verifies, fsyncs and renames.
  On a mismatch the part is deleted and the sender retries once from 0.
* Top-level names that already exist at the receiver get ` (n)` suffixes.
  Nothing is overwritten.
* Connection close codes: `0` normal, `1` cancelled, `2` paused,
  `3` rejected, `4` shutdown. Paused or interrupted transfers keep their
  journals on both sides. Resending the offer with the same `transfer_id`
  and the same manifest resumes from the offsets on disk. The sender resumes
  interrupted transfers automatically when the peer comes back online. The
  receiver can ask for a resume with a `resume` request.
