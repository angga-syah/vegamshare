//! First-time pairing with a password (SPAKE2, bound to the TLS session).
//!
//! The password never crosses the wire and a network observer cannot run an
//! offline dictionary attack: each connection attempt tests at most one
//! password guess, and the listener rate-limits attempts.
//!
//! Both sides feed a TLS exporter value into the SPAKE2 identity string.
//! A man in the middle terminates two different TLS sessions, so the two
//! exporters differ, the derived keys differ, and the confirmation MACs
//! fail. The confirmation MACs also cover both certificate fingerprints, so
//! a successful run proves that the peer holding the password is the one
//! whose certificate we are about to pin.

use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity as SpakeIdentity, Password, Spake2};

use crate::error::{Error, Result};
use crate::identity::Fingerprint;
use crate::proto::{self, ErrorCode, PairConfirm, PeerInfo, Request, Response};

const EXPORTER_LABEL: &[u8] = b"EXPORTER-vegamshare-pairing";
const SPAKE_ID_PREFIX: &[u8] = b"vegamshare-pair-v1:";
const STEP_TIMEOUT: Duration = Duration::from_secs(20);

pub const MIN_PASSWORD_LEN: usize = 6;
pub const MAX_PASSWORD_LEN: usize = 64;

pub fn validate_password(p: &str) -> Result<()> {
    let n = p.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&n) {
        return Err(Error::InvalidArgument(format!(
            "password must be {MIN_PASSWORD_LEN}-{MAX_PASSWORD_LEN} characters"
        )));
    }
    if p.chars().any(char::is_control) {
        return Err(Error::InvalidArgument("password contains control characters".into()));
    }
    Ok(())
}

fn channel_binding(conn: &quinn::Connection) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    conn.export_keying_material(&mut out, EXPORTER_LABEL, b"")
        .map_err(|_| Error::protocol("TLS exporter unavailable"))?;
    Ok(out)
}

fn start(password: &str, binding: &[u8; 32]) -> (Spake2<Ed25519Group>, Vec<u8>) {
    let mut id = SPAKE_ID_PREFIX.to_vec();
    id.extend_from_slice(binding);
    Spake2::<Ed25519Group>::start_symmetric(&Password::new(password.as_bytes()), &SpakeIdentity::new(&id))
}

fn mac(key: &[u8], role: &[u8], client: &Fingerprint, server: &Fingerprint) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    m.update(role);
    m.update(client.as_bytes());
    m.update(server.as_bytes());
    m
}

fn decode_hex(s: &str, max: usize) -> Result<Vec<u8>> {
    if s.len() > max * 2 {
        return Err(Error::protocol("field too long"));
    }
    hex::decode(s).map_err(|_| Error::protocol("invalid hex"))
}

/// Brute-force protection for the listener. Five attempts are free; after
/// that each attempt must wait 30 s, doubling up to one hour. The counter
/// resets after a successful pairing or 15 quiet minutes.
#[derive(Debug, Default)]
pub struct Lockout {
    attempts: u32,
    last_attempt: Option<Instant>,
}

impl Lockout {
    const FREE_ATTEMPTS: u32 = 5;
    const QUIET_RESET: Duration = Duration::from_secs(15 * 60);

    fn delay_after(attempts: u32) -> Duration {
        if attempts < Self::FREE_ATTEMPTS {
            return Duration::ZERO;
        }
        let exp = (attempts - Self::FREE_ATTEMPTS).min(7);
        Duration::from_secs((30u64 << exp).min(3600))
    }

    /// Registers an attempt, or returns how many seconds the caller must wait.
    pub fn try_attempt(&mut self, now: Instant) -> std::result::Result<(), u64> {
        if let Some(last) = self.last_attempt {
            let since = now.saturating_duration_since(last);
            if since >= Self::QUIET_RESET {
                self.attempts = 0;
            } else {
                let wait = Self::delay_after(self.attempts);
                if since < wait {
                    return Err((wait - since).as_secs().max(1));
                }
            }
        }
        self.attempts += 1;
        self.last_attempt = Some(now);
        Ok(())
    }

    pub fn success(&mut self) {
        self.attempts = 0;
        self.last_attempt = None;
    }
}

/// Runs pairing on a connection whose server certificate was not pinned.
/// Returns the peer's info and certificate fingerprint to be trusted.
pub(crate) async fn pair_as_client(
    conn: &quinn::Connection,
    my_info: PeerInfo,
    my_fp: Fingerprint,
    password: &str,
) -> Result<(PeerInfo, Fingerprint)> {
    let server_fp = crate::tls::peer_fingerprint(conn).ok_or_else(|| Error::protocol("peer has no certificate"))?;
    let binding = channel_binding(conn)?;
    let (spake, msg) = start(password, &binding);

    let (mut send, mut recv) = conn.open_bi().await?;
    proto::write_frame(&mut send, &Request::PairStart { info: my_info, spake: hex::encode(msg) }).await?;

    let their_msg = match tokio::time::timeout(STEP_TIMEOUT, proto::read_frame(&mut recv, proto::MAX_FRAME)).await?? {
        Response::PairChallenge { spake } => decode_hex(&spake, 64)?,
        other => return Err(other.into_error()),
    };
    let key = spake.finish(&their_msg).map_err(|_| Error::protocol("invalid SPAKE2 message"))?;

    let client_mac = mac(&key, b"client", &my_fp, &server_fp).finalize().into_bytes();
    proto::write_frame(&mut send, &PairConfirm { mac: hex::encode(client_mac) }).await?;

    let (server_mac, info) = match tokio::time::timeout(STEP_TIMEOUT, proto::read_frame(&mut recv, proto::MAX_FRAME)).await?? {
        Response::PairAccepted { mac, info } => (decode_hex(&mac, 32)?, info),
        other => return Err(other.into_error()),
    };
    mac(&key, b"server", &my_fp, &server_fp)
        .verify_slice(&server_mac)
        .map_err(|_| Error::IdentityMismatch)?;
    let _ = send.finish();
    Ok((info.sanitized(), server_fp))
}

pub(crate) enum ServerOutcome {
    Paired { info: PeerInfo, fingerprint: Fingerprint },
    Refused,
}

/// What the listener needs to answer a pairing request.
pub(crate) struct Listener<'a> {
    pub lockout: &'a std::sync::Mutex<Lockout>,
    pub password: &'a str,
    pub my_info: PeerInfo,
    pub my_fp: Fingerprint,
}

/// Listener side, after the router has read a `PairStart` request.
pub(crate) async fn pair_as_server(
    conn: &quinn::Connection,
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    me: Listener<'_>,
    client_info: PeerInfo,
    client_msg: &str,
) -> Result<ServerOutcome> {
    let Listener { lockout, password, my_info, my_fp } = me;
    let client_fp = crate::tls::peer_fingerprint(conn).ok_or_else(|| Error::protocol("peer has no certificate"))?;

    let gate = lockout.lock().unwrap_or_else(|e| e.into_inner()).try_attempt(Instant::now());
    if let Err(wait) = gate {
        let resp = Response::Error {
            code: ErrorCode::LockedOut,
            message: "too many attempts".into(),
            retry_after: Some(wait),
        };
        proto::write_frame(send, &resp).await?;
        return Ok(ServerOutcome::Refused);
    }

    let client_msg = decode_hex(client_msg, 64)?;
    let binding = channel_binding(conn)?;
    let (spake, msg) = start(password, &binding);
    let key = match spake.finish(&client_msg) {
        Ok(k) => k,
        Err(_) => {
            proto::write_frame(send, &Response::error(ErrorCode::BadRequest, "invalid SPAKE2 message")).await?;
            return Ok(ServerOutcome::Refused);
        }
    };
    proto::write_frame(send, &Response::PairChallenge { spake: hex::encode(msg) }).await?;

    let confirm: PairConfirm = tokio::time::timeout(STEP_TIMEOUT, proto::read_frame(recv, proto::MAX_FRAME)).await??;
    let client_mac = decode_hex(&confirm.mac, 32)?;
    if mac(&key, b"client", &client_fp, &my_fp).verify_slice(&client_mac).is_err() {
        proto::write_frame(send, &Response::error(ErrorCode::WrongPassword, "wrong password")).await?;
        return Ok(ServerOutcome::Refused);
    }

    lockout.lock().unwrap_or_else(|e| e.into_inner()).success();
    let server_mac = mac(&key, b"server", &client_fp, &my_fp).finalize().into_bytes();
    proto::write_frame(send, &Response::PairAccepted { mac: hex::encode(server_mac), info: my_info }).await?;
    let _ = send.finish();
    Ok(ServerOutcome::Paired { info: client_info.sanitized(), fingerprint: client_fp })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockout_schedule() {
        let t0 = Instant::now();
        let mut l = Lockout::default();
        for i in 0..5 {
            assert!(l.try_attempt(t0 + Duration::from_millis(i)).is_ok(), "attempt {i} should be free");
        }
        let wait = l.try_attempt(t0 + Duration::from_secs(1)).unwrap_err();
        assert!((28..=30).contains(&wait), "wait {wait}");
        assert!(l.try_attempt(t0 + Duration::from_secs(31)).is_ok());
        // Next one needs 60 s.
        assert!(l.try_attempt(t0 + Duration::from_secs(61)).is_err());
        assert!(l.try_attempt(t0 + Duration::from_secs(92)).is_ok());
        // Quiet period resets everything.
        assert!(l.try_attempt(t0 + Duration::from_secs(92 + 15 * 60)).is_ok());
        l.success();
        assert!(l.try_attempt(t0 + Duration::from_secs(2000)).is_ok());
    }

    #[test]
    fn delay_is_capped() {
        assert_eq!(Lockout::delay_after(100), Duration::from_secs(3600));
    }

    #[test]
    fn password_rules() {
        assert!(validate_password("abc12").is_err());
        assert!(validate_password("abc123").is_ok());
        assert!(validate_password("kata sandi panjang").is_ok());
        assert!(validate_password("abc\n123").is_err());
    }
}
