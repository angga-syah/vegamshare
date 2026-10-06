//! Long-lived device identity.
//!
//! Every installation owns an Ed25519 key and a self-signed certificate that
//! never changes. Peers remember the SHA-256 of that certificate after
//! pairing (the "fingerprint"), so later connections are authenticated by
//! TLS alone and need no password. The 9-digit device ID is only a
//! human-friendly handle used to find the device on the network; it is never
//! trusted for authentication.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::{Error, Result};
use crate::secret;
use crate::store::Store;

const KEY_FILE: &str = "identity.key";
const CERT_FILE: &str = "identity.crt";
const DEVICE_ID_KEY: &str = "device_id";
pub(crate) const CERT_SUBJECT: &str = "vegamshare";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint(pub [u8; 32]);

impl Fingerprint {
    pub fn of_cert(cert_der: &[u8]) -> Self {
        Fingerprint(Sha256::digest(cert_der).into())
    }

    pub fn from_slice(b: &[u8]) -> Option<Self> {
        b.try_into().ok().map(Fingerprint)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn ct_eq(&self, other: &Fingerprint) -> bool {
        self.0.ct_eq(&other.0).into()
    }

    /// Short prefix sent in discovery packets; only a lookup hint.
    pub fn short(&self) -> String {
        hex::encode(&self.0[..8])
    }

    /// Human-comparable form, e.g. `3F2A 9C01 ...` (first 16 bytes).
    pub fn pretty(&self) -> String {
        hex::encode_upper(&self.0[..16])
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).expect("hex is ascii"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.short())
    }
}

impl FromStr for Fingerprint {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(|_| Error::InvalidArgument("fingerprint is not hex".into()))?;
        Fingerprint::from_slice(&bytes).ok_or_else(|| Error::InvalidArgument("fingerprint must be 32 bytes".into()))
    }
}

impl Serialize for Fingerprint {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

pub struct Identity {
    pub device_id: String,
    pub cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
    pub fingerprint: Fingerprint,
}

impl Identity {
    pub fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(self.key.clone_key())
    }

    /// Loads the identity from `dir`, creating one on first run.
    pub fn load_or_create(dir: &Path, store: &Store) -> Result<Self> {
        let key_path = dir.join(KEY_FILE);
        let cert_path = dir.join(CERT_FILE);

        let loaded = match (std::fs::read(&key_path), std::fs::read(&cert_path)) {
            (Ok(sealed), Ok(cert)) => match secret::unprotect(&sealed) {
                Ok(key) => Some((key, cert)),
                Err(e) => {
                    // Happens if the data folder was copied to another user
                    // account. The old key is unusable; start fresh.
                    tracing::warn!("identity key cannot be unsealed ({e}); generating a new identity");
                    None
                }
            },
            _ => None,
        };

        let (key, cert) = match loaded {
            Some(v) => v,
            None => {
                let (key, cert) = generate()?;
                secret::write_atomic(&key_path, &secret::protect(&key)?)?;
                secret::write_atomic(&cert_path, &cert)?;
                (key, cert)
            }
        };

        let device_id = match store.get_string(DEVICE_ID_KEY)? {
            Some(id) if is_valid_device_id(&id) => id,
            _ => {
                let id = random_device_id();
                store.set_setting(DEVICE_ID_KEY, id.as_bytes())?;
                id
            }
        };

        Ok(Identity {
            device_id,
            fingerprint: Fingerprint::of_cert(&cert),
            cert: CertificateDer::from(cert),
            key: PrivatePkcs8KeyDer::from(key),
        })
    }

    /// Ephemeral identity for tests.
    pub fn generate_ephemeral() -> Result<Self> {
        let (key, cert) = generate()?;
        Ok(Identity {
            device_id: random_device_id(),
            fingerprint: Fingerprint::of_cert(&cert),
            cert: CertificateDer::from(cert),
            key: PrivatePkcs8KeyDer::from(key),
        })
    }
}

fn generate() -> Result<(Vec<u8>, Vec<u8>)> {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)?;
    let params = rcgen::CertificateParams::new(vec![CERT_SUBJECT.to_string()])?;
    let cert = params.self_signed(&key)?;
    Ok((key.serialize_der(), cert.der().to_vec()))
}

pub fn is_valid_device_id(id: &str) -> bool {
    id.len() == 9 && id.bytes().all(|b| b.is_ascii_digit()) && !id.starts_with('0')
}

/// Accepts user input like "123 456 789" or "123-456-789".
pub fn normalize_device_id(input: &str) -> Option<String> {
    let id: String = input.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    is_valid_device_id(&id).then_some(id)
}

pub fn format_device_id(id: &str) -> String {
    if id.len() == 9 { format!("{} {} {}", &id[0..3], &id[3..6], &id[6..9]) } else { id.to_string() }
}

fn random_u32() -> u32 {
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).expect("OS random generator failed");
    u32::from_le_bytes(b)
}

fn random_device_id() -> String {
    const SPAN: u32 = 900_000_000;
    const LIMIT: u32 = SPAN * 4; // largest multiple of SPAN below u32::MAX, avoids modulo bias
    loop {
        let n = random_u32();
        if n < LIMIT {
            return (100_000_000 + n % SPAN).to_string();
        }
    }
}

/// Random password without look-alike characters (0/O, 1/l/I).
pub fn random_password(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    const LIMIT: u8 = (256 / ALPHABET.len() * ALPHABET.len()) as u8;
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut b = [0u8; 32];
        getrandom::fill(&mut b).expect("OS random generator failed");
        for x in b {
            if x < LIMIT && out.len() < len {
                out.push(ALPHABET[(x as usize) % ALPHABET.len()] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_stable_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("db.sqlite")).unwrap();
        let a = Identity::load_or_create(dir.path(), &store).unwrap();
        let b = Identity::load_or_create(dir.path(), &store).unwrap();
        assert_eq!(a.device_id, b.device_id);
        assert_eq!(a.fingerprint, b.fingerprint);
        assert!(is_valid_device_id(&a.device_id));
    }

    #[test]
    fn device_id_formats() {
        for _ in 0..1000 {
            assert!(is_valid_device_id(&random_device_id()));
        }
        assert_eq!(normalize_device_id(" 123-456 789 ").as_deref(), Some("123456789"));
        assert_eq!(normalize_device_id("023456789"), None);
        assert_eq!(normalize_device_id("12345678"), None);
        assert_eq!(format_device_id("123456789"), "123 456 789");
    }

    #[test]
    fn fingerprint_roundtrip() {
        let fp = Fingerprint([0xab; 32]);
        assert_eq!(fp.to_string().parse::<Fingerprint>().unwrap(), fp);
        let json = serde_json::to_string(&fp).unwrap();
        assert_eq!(serde_json::from_str::<Fingerprint>(&json).unwrap(), fp);
        assert_eq!(fp.pretty(), "ABAB ABAB ABAB ABAB ABAB ABAB ABAB ABAB");
    }

    #[test]
    fn passwords() {
        let p = random_password(8);
        assert_eq!(p.len(), 8);
        assert!(!p.contains(['0', 'o', '1', 'l', 'i']));
    }
}
