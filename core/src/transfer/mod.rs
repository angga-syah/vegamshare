//! File transfer engine: shared state for sending and receiving.

pub mod manifest;
pub(crate) mod recv;
pub(crate) mod send;

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use serde::Serialize;

use crate::identity::Fingerprint;

pub use manifest::Source;

/// Size of the buffer used when streaming file contents.
pub(crate) const CHUNK: usize = 256 * 1024;
/// Files sent concurrently within one transfer.
pub(crate) const PARALLEL_FILES: usize = 4;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Send,
    Receive,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Send => "send",
            Direction::Receive => "receive",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    InProgress,
    Completed,
    /// Deliberately paused; resumable by either side.
    Paused,
    /// Connection lost; the sender resumes automatically when the peer is back.
    Interrupted,
    Cancelled,
    Rejected,
    Failed,
}

impl TransferStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TransferStatus::InProgress => "in_progress",
            TransferStatus::Completed => "completed",
            TransferStatus::Paused => "paused",
            TransferStatus::Interrupted => "interrupted",
            TransferStatus::Cancelled => "cancelled",
            TransferStatus::Rejected => "rejected",
            TransferStatus::Failed => "failed",
        }
    }

    pub fn is_resumable(self) -> bool {
        matches!(self, TransferStatus::Paused | TransferStatus::Interrupted)
    }
}

/// What the local user asked an active transfer to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum StopRequest {
    None = 0,
    Pause = 1,
    Cancel = 2,
}

#[derive(Default)]
pub(crate) struct Progress {
    pub bytes_done: AtomicU64,
    pub files_done: AtomicU64,
    pub current: Mutex<String>,
}

impl Progress {
    pub fn set_current(&self, name: &str) {
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = name.to_string();
    }

    pub fn current(&self) -> String {
        self.current.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

pub(crate) struct ActiveTransfer {
    pub id: String,
    pub direction: Direction,
    pub peer: Fingerprint,
    pub peer_name: String,
    pub total_bytes: u64,
    pub total_files: u64,
    pub progress: Progress,
    pub conn: quinn::Connection,
    stop: AtomicU8,
    /// Receiver bookkeeping; `None` on the sending side.
    pub recv: Option<recv::RecvState>,
}

impl ActiveTransfer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        direction: Direction,
        peer: Fingerprint,
        peer_name: String,
        total_bytes: u64,
        total_files: u64,
        conn: quinn::Connection,
        recv: Option<recv::RecvState>,
    ) -> Self {
        ActiveTransfer {
            id,
            direction,
            peer,
            peer_name,
            total_bytes,
            total_files,
            progress: Progress::default(),
            conn,
            stop: AtomicU8::new(StopRequest::None as u8),
            recv,
        }
    }

    pub fn stop_request(&self) -> StopRequest {
        match self.stop.load(Ordering::SeqCst) {
            1 => StopRequest::Pause,
            2 => StopRequest::Cancel,
            _ => StopRequest::None,
        }
    }

    pub fn request_stop(&self, s: StopRequest) {
        self.stop.store(s as u8, Ordering::SeqCst);
    }

    pub fn view(&self) -> TransferView {
        TransferView {
            transfer_id: self.id.clone(),
            direction: self.direction,
            peer: self.peer,
            peer_name: self.peer_name.clone(),
            bytes_done: self.progress.bytes_done.load(Ordering::Relaxed),
            bytes_total: self.total_bytes,
            files_done: self.progress.files_done.load(Ordering::Relaxed),
            files_total: self.total_files,
            current: self.progress.current(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TransferView {
    pub transfer_id: String,
    pub direction: Direction,
    pub peer: Fingerprint,
    pub peer_name: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current: String,
}

pub(crate) fn new_transfer_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS random generator failed");
    hex::encode(b)
}

pub(crate) fn is_valid_transfer_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Short summary for history, e.g. "Photos + 2 more".
pub(crate) fn summarize(roots: &[String]) -> String {
    match roots {
        [] => String::new(),
        [one] => one.clone(),
        [first, rest @ ..] => format!("{first} + {} more", rest.len()),
    }
}
