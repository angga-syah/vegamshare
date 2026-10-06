//! Password-free pairing, in the style of Bluetooth.
//!
//! * **Nearby + code**: the asking device connects without pinning, and both
//!   sides derive a 6-digit code from the TLS session (exporter). The user
//!   on the receiving side accepts only if the codes on both screens match.
//!   A man in the middle terminates two TLS sessions with different
//!   exporters, so the codes cannot match (numeric comparison, as in
//!   Bluetooth Secure Simple Pairing).
//! * **QR code**: the QR carries the full certificate fingerprint and a
//!   single-use 128-bit token valid for two minutes. The scanner dials with
//!   the fingerprint pinned (authenticating the shower) and presents the
//!   token (proving it saw the screen).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use subtle::ConstantTimeEq;

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, is_valid_device_id};

const SAS_LABEL: &[u8] = b"EXPORTER-vegamshare-sas";
pub(crate) const QR_TTL: Duration = Duration::from_secs(120);
const QR_SCHEME: &str = "vegamshare://pair";
const MAX_TOKENS: usize = 4;
const MAX_QR_ADDRS: usize = 6;

/// Six-digit code both sides display, e.g. "482915".
pub(crate) fn sas_code(conn: &quinn::Connection) -> Result<String> {
    let mut out = [0u8; 8];
    conn.export_keying_material(&mut out, SAS_LABEL, b"")
        .map_err(|_| Error::protocol("TLS exporter unavailable"))?;
    Ok(format!("{:06}", u64::from_be_bytes(out) % 1_000_000))
}

#[derive(Clone, Debug, PartialEq)]
pub struct QrPayload {
    pub device_id: String,
    pub fingerprint: Fingerprint,
    pub addrs: Vec<SocketAddr>,
    pub token: [u8; 16],
}

impl QrPayload {
    pub fn encode(&self) -> String {
        let addrs: Vec<String> = self.addrs.iter().map(|a| a.to_string()).collect();
        format!(
            "{QR_SCHEME}?v=1&id={}&fp={}&a={}&t={}",
            self.device_id,
            self.fingerprint,
            addrs.join(","),
            hex::encode(self.token)
        )
    }

    /// Parses untrusted scanner input strictly.
    pub fn decode(s: &str) -> Result<Self> {
        let bad = || Error::InvalidArgument("not a VegamShare pairing code".into());
        let query = s.trim().strip_prefix(QR_SCHEME).and_then(|r| r.strip_prefix('?')).ok_or_else(bad)?;
        if query.len() > 1024 {
            return Err(bad());
        }
        let (mut v, mut id, mut fp, mut addrs, mut token) = (None, None, None, None, None);
        for pair in query.split('&') {
            let (k, val) = pair.split_once('=').ok_or_else(bad)?;
            match k {
                "v" => v = Some(val),
                "id" => id = Some(val),
                "fp" => fp = Some(val),
                "a" => addrs = Some(val),
                "t" => token = Some(val),
                _ => {}
            }
        }
        if v != Some("1") {
            return Err(Error::InvalidArgument("pairing code from a newer VegamShare version".into()));
        }
        let device_id = id.filter(|i| is_valid_device_id(i)).ok_or_else(bad)?.to_string();
        let fingerprint: Fingerprint = fp.ok_or_else(bad)?.parse().map_err(|_| bad())?;
        let token: [u8; 16] = hex::decode(token.ok_or_else(bad)?).ok().and_then(|t| t.try_into().ok()).ok_or_else(bad)?;
        let addrs: Vec<SocketAddr> = addrs
            .unwrap_or("")
            .split(',')
            .filter(|a| !a.is_empty())
            .take(MAX_QR_ADDRS)
            .filter_map(|a| a.parse().ok())
            .collect();
        Ok(QrPayload { device_id, fingerprint, addrs, token })
    }
}

/// Outstanding QR tokens of this device.
#[derive(Default)]
pub(crate) struct Tokens {
    live: Vec<([u8; 16], Instant)>,
}

impl Tokens {
    pub fn issue(&mut self) -> [u8; 16] {
        let mut t = [0u8; 16];
        getrandom::fill(&mut t).expect("OS random generator failed");
        self.live.retain(|(_, at)| at.elapsed() < QR_TTL);
        if self.live.len() >= MAX_TOKENS {
            self.live.remove(0);
        }
        self.live.push((t, Instant::now()));
        t
    }

    /// Valid tokens work exactly once. Compared in constant time.
    pub fn consume(&mut self, presented: &[u8]) -> bool {
        self.live.retain(|(_, at)| at.elapsed() < QR_TTL);
        let hit = self.live.iter().position(|(t, _)| bool::from(t.as_slice().ct_eq(presented)));
        match hit {
            Some(i) => {
                self.live.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn revoke_all(&mut self) {
        self.live.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_roundtrip_and_rejects_garbage() {
        let p = QrPayload {
            device_id: "123456789".into(),
            fingerprint: Fingerprint([7; 32]),
            addrs: vec!["192.168.1.5:47901".parse().unwrap(), "10.0.0.2:47901".parse().unwrap()],
            token: [9; 16],
        };
        assert_eq!(QrPayload::decode(&p.encode()).unwrap(), p);
        for bad in [
            "",
            "https://evil.example/?id=1",
            "vegamshare://pair?v=1&id=12&fp=00&t=00",
            "vegamshare://pair?v=2&id=123456789",
            &p.encode().replace("t=09", "t=zz"),
        ] {
            assert!(QrPayload::decode(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn tokens_are_single_use() {
        let mut t = Tokens::default();
        let a = t.issue();
        assert!(!t.consume(&[0; 16]));
        assert!(t.consume(&a));
        assert!(!t.consume(&a), "second use must fail");
        let b = t.issue();
        t.revoke_all();
        assert!(!t.consume(&b));
    }
}
