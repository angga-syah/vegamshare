//! Wire protocol. Documented in `docs/PROTOCOL.md`.
//!
//! Every request uses its own bidirectional QUIC stream. Control messages
//! are length-prefixed JSON frames (u32 big-endian length). A `File`
//! request is followed by raw file bytes and a 32-byte BLAKE3 digest of the
//! whole file.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const ALPN: &[u8] = b"vegamshare/1";
pub const PROTOCOL_VERSION: u32 = 1;

/// Default cap for control frames.
pub const MAX_FRAME: usize = 1 << 20;
/// Offers list every file and may legitimately be large.
pub const MAX_OFFER_FRAME: usize = 64 << 20;
pub const MAX_ENTRIES: usize = 1_000_000;

/// Offset value meaning "receiver already has this file".
pub const OFFSET_DONE: u64 = u64::MAX;

/// QUIC application close codes.
pub mod close {
    pub const NORMAL: u32 = 0;
    pub const CANCELLED: u32 = 1;
    pub const PAUSED: u32 = 2;
    pub const REJECTED: u32 = 3;
    pub const SHUTDOWN: u32 = 4;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PeerInfo {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub version: u32,
}

impl PeerInfo {
    pub(crate) fn sanitized(mut self) -> Self {
        self.name = clean_label(&self.name, 64);
        self.platform = clean_label(&self.platform, 16);
        self.device_id = clean_label(&self.device_id, 16);
        self
    }
}

/// Strips control characters and bounds length of peer-supplied labels
/// before they reach the UI or the database.
pub(crate) fn clean_label(s: &str, max_chars: usize) -> String {
    // Bidi overrides would let a peer make its name render as something else.
    let is_bidi = |c: char| matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
    let cleaned: String = s.chars().filter(|c| !c.is_control() && !is_bidi(*c)).take(max_chars).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() { "Unknown".into() } else { trimmed.into() }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// `/`-separated path relative to the transfer root.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub mtime: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub transfer_id: String,
    pub entries: Vec<Entry>,
}

impl Offer {
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum()
    }

    pub fn file_count(&self) -> u64 {
        self.entries.iter().filter(|e| !e.is_dir).count() as u64
    }

    /// Distinct top-level names, for a short human summary.
    pub fn roots(&self) -> Vec<String> {
        let mut roots: Vec<String> = Vec::new();
        for e in &self.entries {
            let top = e.path.split('/').next().unwrap_or_default();
            if !roots.iter().any(|r| r == top) {
                roots.push(top.to_string());
            }
        }
        roots
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping { info: PeerInfo },
    PairStart { info: PeerInfo, spake: String },
    Offer { info: PeerInfo, offer: Offer },
    File { transfer_id: String, index: u32, offset: u64 },
    /// Receiver asks the sender to continue a paused or interrupted transfer.
    Resume { transfer_id: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PairConfirm {
    pub mac: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotTrusted,
    WrongPassword,
    LockedOut,
    BadRequest,
    Busy,
    HashMismatch,
    UnknownTransfer,
    Internal,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong { info: PeerInfo, trusted: bool },
    PairChallenge { spake: String },
    PairAccepted { mac: String, info: PeerInfo },
    Accepted { offsets: Vec<u64> },
    Rejected,
    Ok,
    Error { code: ErrorCode, message: String, retry_after: Option<u64> },
}

impl Response {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Response::Error { code, message: message.into(), retry_after: None }
    }

    /// Converts an error response into a local error.
    pub fn into_error(self) -> Error {
        match self {
            Response::Error { code, message, retry_after } => match code {
                ErrorCode::NotTrusted => Error::NotTrusted,
                ErrorCode::WrongPassword => Error::WrongPassword,
                ErrorCode::LockedOut => Error::LockedOut(retry_after.unwrap_or(60)),
                ErrorCode::UnknownTransfer => Error::UnknownTransfer,
                ErrorCode::HashMismatch => Error::Integrity(message),
                _ => Error::Protocol(format!("peer error {code:?}: {message}")),
            },
            Response::Rejected => Error::Rejected,
            other => Error::protocol(format!("unexpected response {other:?}")),
        }
    }
}

pub async fn write_frame<T: Serialize>(s: &mut quinn::SendStream, msg: &T) -> Result<()> {
    let body = serde_json::to_vec(msg)?;
    let len = u32::try_from(body.len()).map_err(|_| Error::protocol("frame too large"))?;
    s.write_all(&len.to_be_bytes()).await?;
    s.write_all(&body).await?;
    Ok(())
}

pub async fn read_frame<T: DeserializeOwned>(r: &mut quinn::RecvStream, max: usize) -> Result<T> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(Error::protocol(format!("frame of {len} bytes exceeds limit {max}")));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_json_is_tagged() {
        let r = Request::File { transfer_id: "t".into(), index: 3, offset: 9 };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"type":"file","transfer_id":"t","index":3,"offset":9}"#);
    }

    #[test]
    fn labels_are_cleaned() {
        assert_eq!(clean_label("  Budi\u{202e}\n's PC ", 64), "Budi's PC");
        assert_eq!(clean_label("\n\t", 64), "Unknown");
        assert_eq!(clean_label(&"x".repeat(100), 64).len(), 64);
    }

    #[test]
    fn offer_summary() {
        let e = |p: &str, d: bool, s: u64| Entry { path: p.into(), is_dir: d, size: s, mtime: 0 };
        let o = Offer { transfer_id: "x".into(), entries: vec![e("A", true, 0), e("A/1", false, 5), e("b.txt", false, 7)] };
        assert_eq!(o.total_bytes(), 12);
        assert_eq!(o.file_count(), 2);
        assert_eq!(o.roots(), vec!["A", "b.txt"]);
    }
}
