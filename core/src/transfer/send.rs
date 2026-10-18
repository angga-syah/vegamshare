//! Sending side: offer, parallel file streams, resume.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use super::{ActiveTransfer, CHUNK, Direction, PARALLEL_FILES, Source, StopRequest, TransferStatus, new_transfer_id, summarize};
use crate::error::{Error, Result};
use crate::identity::Fingerprint;
use crate::node::{Event, Inner, lock};
use crate::pathsafe;
use crate::proto::{self, Entry, MAX_FRAME, OFFSET_DONE, Offer, Request, Response, close};
use crate::store::{self, HistoryEntry, Outgoing};

/// The receiver may wait for its user to accept.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(6 * 60);
/// Receiver fsyncs and renames before acknowledging a file.
const ACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

impl Inner {
    pub(crate) async fn start_send(self: &Arc<Self>, peer: Fingerprint, items: Vec<(Entry, Source)>) -> Result<String> {
        if items.is_empty() {
            return Err(Error::InvalidArgument("nothing to send".into()));
        }
        let device = self.store.device(&peer)?.ok_or(Error::NotTrusted)?;
        for (e, s) in &items {
            pathsafe::validate(&e.path)?;
            if e.is_dir != matches!(s, Source::Dir) {
                return Err(Error::InvalidArgument(format!("{}: entry kind does not match its source", e.path)));
            }
        }
        let (entries, sources): (Vec<Entry>, Vec<Source>) = items.into_iter().unzip();
        let offer = Offer { transfer_id: new_transfer_id(), entries };
        crate::transfer::recv::validate_offer(&offer)?;

        let id = offer.transfer_id.clone();
        self.store.insert_outgoing(&Outgoing {
            transfer_id: id.clone(),
            peer_fp: peer,
            manifest: serde_json::to_string(&offer.entries)?,
            sources: serde_json::to_string(&sources)?,
            created_at: store::now(),
        })?;
        self.store.upsert_history(&HistoryEntry {
            transfer_id: id.clone(),
            direction: Direction::Send.as_str().into(),
            peer_fp: peer,
            peer_name: device.display_name().to_string(),
            summary: summarize(&offer.roots()),
            file_count: offer.file_count(),
            total_bytes: offer.total_bytes(),
            status: TransferStatus::InProgress.as_str().into(),
            location: None,
            started_at: store::now(),
            finished_at: None,
        })?;

        let me = self.clone();
        tokio::spawn(async move { me.run_send(peer, offer, sources, false).await });
        Ok(id)
    }

    pub(crate) async fn resume_send(self: &Arc<Self>, transfer_id: &str) -> Result<()> {
        if lock(&self.transfers).contains_key(transfer_id) {
            return Ok(());
        }
        let o = self.store.outgoing(transfer_id)?.ok_or(Error::UnknownTransfer)?;
        let entries: Vec<Entry> = serde_json::from_str(&o.manifest)?;
        let sources: Vec<Source> = serde_json::from_str(&o.sources)?;
        if entries.len() != sources.len() {
            return Err(Error::protocol("corrupt send journal"));
        }
        self.store.set_history_status(transfer_id, TransferStatus::InProgress.as_str(), false)?;
        let me = self.clone();
        let offer = Offer { transfer_id: o.transfer_id, entries };
        tokio::spawn(async move { me.run_send(o.peer_fp, offer, sources, true).await });
        Ok(())
    }

    async fn run_send(self: Arc<Self>, peer: Fingerprint, offer: Offer, sources: Vec<Source>, resumed: bool) {
        let id = offer.transfer_id.clone();
        let (status, error) = match self.send_inner(peer, offer, sources, resumed).await {
            Ok(s) => (s, None),
            Err((s, e)) => (s, Some(crate::node::ErrorInfo::from(&e))),
        };
        self.finish_transfer_with(&id, Direction::Send, peer, status, error);
    }

    async fn send_inner(
        self: &Arc<Self>,
        peer: Fingerprint,
        offer: Offer,
        sources: Vec<Source>,
        resumed: bool,
    ) -> std::result::Result<TransferStatus, (TransferStatus, Error)> {
        let conn = self.connect_trusted(&peer).await.map_err(|e| match e {
            // The saved device no longer exists as such: nothing to resume.
            e @ Error::IdentityChanged { .. } => (TransferStatus::Failed, e),
            e => (TransferStatus::Interrupted, e),
        })?;

        let offsets = match self.offer(&conn, &offer).await {
            Ok(o) => o,
            Err(Error::Rejected) => {
                conn.close(close::NORMAL.into(), b"");
                return Ok(TransferStatus::Rejected);
            }
            Err(e @ Error::Connection(_)) | Err(e @ Error::Timeout) => return Err((TransferStatus::Interrupted, e)),
            Err(e) => {
                conn.close(close::NORMAL.into(), b"");
                return Err((TransferStatus::Failed, e));
            }
        };

        let peer_name = self.store.device(&peer).ok().flatten().map(|d| d.display_name().to_string()).unwrap_or_default();
        let t = Arc::new(ActiveTransfer::new(
            offer.transfer_id.clone(),
            Direction::Send,
            peer,
            peer_name,
            offer.total_bytes(),
            offer.file_count(),
            conn.clone(),
            None,
        ));
        let (mut done_bytes, mut done_files) = (0u64, 0u64);
        for (e, off) in offer.entries.iter().zip(&offsets) {
            if e.is_dir {
                continue;
            }
            if *off == OFFSET_DONE {
                done_bytes += e.size;
                done_files += 1;
            } else {
                done_bytes += off;
            }
        }
        t.progress.bytes_done.store(done_bytes, Ordering::Relaxed);
        t.progress.files_done.store(done_files, Ordering::Relaxed);
        lock(&self.transfers).insert(t.id.clone(), t.clone());
        self.activity.notify_one();
        self.emit(Event::TransferStarted { transfer: t.view(), resumed });

        let sem = Arc::new(Semaphore::new(PARALLEL_FILES));
        let mut tasks: JoinSet<Result<()>> = JoinSet::new();
        let mut failure: Option<Error> = None;
        for (i, ((entry, source), offset)) in offer.entries.into_iter().zip(sources).zip(offsets).enumerate() {
            if entry.is_dir || offset == OFFSET_DONE {
                continue;
            }
            let permit = tokio::select! {
                p = sem.clone().acquire_owned() => p.expect("semaphore is never closed"),
                Some(r) = tasks.join_next() => {
                    if let Some(e) = flatten(r) { failure = Some(e); break; }
                    sem.clone().acquire_owned().await.expect("semaphore is never closed")
                }
            };
            if conn.close_reason().is_some() {
                break;
            }
            let (conn, t, opener) = (conn.clone(), t.clone(), self.cfg.content_opener.clone());
            tasks.spawn(async move {
                let _permit = permit;
                send_file(&conn, &t, i as u32, &entry, &source, offset, opener.as_ref()).await
            });
        }
        while let Some(r) = tasks.join_next().await {
            if let Some(e) = flatten(r)
                && failure.is_none()
            {
                failure = Some(e);
            }
        }

        let closed = conn.close_reason().is_some();
        match failure {
            None if !closed => {
                conn.close(close::NORMAL.into(), b"done");
                Ok(TransferStatus::Completed)
            }
            Some(e) if !closed && !matches!(e, Error::Connection(_)) => {
                // Local problem (source file vanished, unreadable...). Files
                // already delivered stay on the receiver; partial ones are
                // dropped because this transfer cannot be completed as offered.
                t.request_stop(StopRequest::Cancel);
                conn.close(close::CANCELLED.into(), b"sender error");
                Err((TransferStatus::Failed, e))
            }
            failure => match Inner::status_after_close(&t) {
                s @ (TransferStatus::Paused | TransferStatus::Cancelled | TransferStatus::Rejected) => Ok(s),
                s => Err((s, failure.unwrap_or(Error::ConnectionLost))),
            },
        }
    }

    async fn offer(&self, conn: &quinn::Connection, offer: &Offer) -> Result<Vec<u64>> {
        let (mut send, mut recv) = conn.open_bi().await?;
        proto::write_frame(&mut send, &Request::Offer { info: self.my_info(), offer: offer.clone() }).await?;
        let _ = send.finish();
        match tokio::time::timeout(ANSWER_TIMEOUT, proto::read_frame(&mut recv, MAX_FRAME)).await?? {
            Response::Accepted { offsets } => {
                if offsets.len() != offer.entries.len() {
                    return Err(Error::protocol("offset list has the wrong length"));
                }
                for (e, off) in offer.entries.iter().zip(&offsets) {
                    if !e.is_dir && *off != OFFSET_DONE && *off > e.size {
                        return Err(Error::protocol("offset beyond end of file"));
                    }
                }
                Ok(offsets)
            }
            other => Err(other.into_error()),
        }
    }
}

fn flatten(r: std::result::Result<Result<()>, tokio::task::JoinError>) -> Option<Error> {
    match r {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e),
        Err(e) => Some(Error::protocol(format!("send task panicked: {e}"))),
    }
}

async fn open_source(source: &Source, opener: Option<&crate::node::ContentOpener>) -> Result<tokio::fs::File> {
    match source {
        Source::Path(p) => Ok(tokio::fs::File::open(p).await?),
        Source::Uri(u) => {
            let opener = opener.ok_or_else(|| Error::InvalidArgument("no content opener configured".into()))?.clone();
            let u = u.clone();
            let f = tokio::task::spawn_blocking(move || opener(&u))
                .await
                .map_err(|e| Error::protocol(format!("opener task failed: {e}")))??;
            Ok(tokio::fs::File::from_std(f))
        }
        Source::Dir => Err(Error::InvalidArgument("folders have no content".into())),
    }
}

/// Sends one file; retries once from the start if the receiver reports a
/// checksum mismatch (e.g. the source changed while resuming).
async fn send_file(
    conn: &quinn::Connection,
    t: &ActiveTransfer,
    index: u32,
    entry: &Entry,
    source: &Source,
    offset: u64,
    opener: Option<&crate::node::ContentOpener>,
) -> Result<()> {
    match send_file_once(conn, t, index, entry, source, offset, opener).await {
        Err(Error::Integrity(msg)) => {
            tracing::warn!("{msg}; resending {} from the start", entry.path);
            send_file_once(conn, t, index, entry, source, 0, opener).await
        }
        other => other,
    }
}

async fn send_file_once(
    conn: &quinn::Connection,
    t: &ActiveTransfer,
    index: u32,
    entry: &Entry,
    source: &Source,
    offset: u64,
    opener: Option<&crate::node::ContentOpener>,
) -> Result<()> {
    let mut file = open_source(source, opener).await?;
    // Host openers may hand over a stream (a pipe) whose length is not known
    // up front; for those the reads below catch a short source instead.
    let meta = file.metadata().await?;
    if meta.is_file() && meta.len() < entry.size {
        return Err(Error::InvalidArgument(format!("{} changed since it was selected", entry.path)));
    }
    t.progress.set_current(&entry.path);

    let (mut send, mut recv) = conn.open_bi().await?;
    proto::write_frame(&mut send, &Request::File { transfer_id: t.id.clone(), index, offset }).await?;

    let mut buf = vec![0u8; CHUNK];
    let mut hasher = blake3::Hasher::new();
    let mut left = offset;
    while left > 0 {
        let n = file.read(&mut buf[..left.min(CHUNK as u64) as usize]).await?;
        if n == 0 {
            return Err(Error::InvalidArgument(format!("{} shrank while sending", entry.path)));
        }
        hasher.update(&buf[..n]);
        left -= n as u64;
    }
    // The loop above already read up to `offset`; no seek, so streams work too.

    let mut sent = 0u64;
    let result: Result<()> = async {
        let mut remaining = entry.size - offset;
        while remaining > 0 {
            let n = file.read(&mut buf[..remaining.min(CHUNK as u64) as usize]).await?;
            if n == 0 {
                return Err(Error::InvalidArgument(format!("{} shrank while sending", entry.path)));
            }
            hasher.update(&buf[..n]);
            send.write_all(&buf[..n]).await?;
            remaining -= n as u64;
            sent += n as u64;
            t.progress.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
        }
        send.write_all(hasher.finalize().as_bytes()).await?;
        send.finish()?;
        match tokio::time::timeout(ACK_TIMEOUT, proto::read_frame(&mut recv, MAX_FRAME)).await?? {
            Response::Ok => Ok(()),
            other => Err(other.into_error()),
        }
    }
    .await;

    match result {
        Ok(()) => {
            t.progress.files_done.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        Err(e) => {
            // Undo progress: on a mismatch the whole file is resent.
            let undo = if matches!(e, Error::Integrity(_)) { sent + offset } else { sent };
            t.progress.bytes_done.fetch_sub(undo, Ordering::Relaxed);
            Err(e)
        }
    }
}
