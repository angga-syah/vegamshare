//! Receiving side: validating offers, writing files, resuming.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::oneshot;

use super::{ActiveTransfer, CHUNK, Direction, TransferStatus, is_valid_transfer_id, summarize};
use crate::error::{Error, Result};
use crate::identity::Fingerprint;
use crate::node::{Event, Inner, lock};
use crate::pathsafe;
use crate::proto::{self, ErrorCode, MAX_ENTRIES, OFFSET_DONE, Offer, PeerInfo, Response, close};
use crate::store::{self, HistoryEntry, RecvPart};

pub(crate) const PART_SUFFIX: &str = ".vegamshare-part";
const DECISION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub(crate) struct RecvState {
    parts: Mutex<Vec<RecvPart>>,
    remaining: AtomicU64,
}

/// Rejects anything that could write outside the destination or collide.
pub(crate) fn validate_offer(offer: &Offer) -> Result<()> {
    if !is_valid_transfer_id(&offer.transfer_id) {
        return Err(Error::protocol("bad transfer id"));
    }
    if offer.entries.is_empty() || offer.entries.len() > MAX_ENTRIES {
        return Err(Error::protocol("bad entry count"));
    }
    let mut seen: HashSet<String> = HashSet::with_capacity(offer.entries.len());
    let mut files: HashSet<String> = HashSet::new();
    let mut total: u64 = 0;
    for e in &offer.entries {
        pathsafe::validate(&e.path)?;
        // Windows and Android storage are case-insensitive.
        let key = e.path.to_lowercase();
        if !seen.insert(key.clone()) {
            return Err(Error::InvalidPath(format!("{} appears twice", e.path)));
        }
        if e.is_dir {
            if e.size != 0 {
                return Err(Error::protocol("folder with a size"));
            }
        } else {
            total = total.checked_add(e.size).ok_or_else(|| Error::protocol("size overflow"))?;
            files.insert(key);
        }
    }
    // A file cannot also be the parent folder of another entry.
    for e in &offer.entries {
        let lower = e.path.to_lowercase();
        let mut idx = 0;
        while let Some(pos) = lower[idx..].find('/') {
            let ancestor = &lower[..idx + pos];
            if files.contains(ancestor) {
                return Err(Error::InvalidPath(format!("{ancestor} is both a file and a folder")));
            }
            idx += pos + 1;
        }
    }
    Ok(())
}

fn same_manifest(parts: &[RecvPart], offer: &Offer) -> bool {
    parts.len() == offer.entries.len()
        && parts.iter().zip(&offer.entries).enumerate().all(|(i, (p, e))| {
            p.idx as usize == i && p.rel_path == e.path && p.is_dir == e.is_dir && p.size == e.size && p.mtime == e.mtime
        })
}

fn part_len(path: &str) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

/// Chooses on-disk names for a new offer. Top-level items that already
/// exist get a " (n)" suffix so nothing the user owns is overwritten.
fn plan(dest: &Path, offer: &Offer) -> Result<Vec<RecvPart>> {
    std::fs::create_dir_all(dest)?;
    let roots = offer.roots();
    let mut root_map: HashMap<String, String> = HashMap::new();
    for root in &roots {
        let prefix = format!("{root}/");
        let is_dir = offer.entries.iter().any(|e| (e.is_dir && &e.path == root) || e.path.starts_with(&prefix));
        let mapped = if is_dir {
            // create_dir (not _all) fails if it exists, which makes the
            // reservation atomic against concurrent transfers.
            let mut taken: HashSet<String> = HashSet::new();
            loop {
                let candidate = pathsafe::unique_name(root, true, |n| taken.contains(n) || exists(&dest.join(n)));
                match std::fs::create_dir(dest.join(&candidate)) {
                    Ok(()) => break candidate,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        taken.insert(candidate);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        } else {
            pathsafe::unique_name(root, false, |n| exists(&dest.join(n)) || exists(&dest.join(format!("{n}{PART_SUFFIX}"))))
        };
        root_map.insert(root.clone(), mapped);
    }

    let mut parts = Vec::with_capacity(offer.entries.len());
    for (i, e) in offer.entries.iter().enumerate() {
        let mut comps = pathsafe::validate(&e.path)?;
        let mapped = root_map.get(comps[0]).expect("every root was mapped");
        comps[0] = mapped;
        let final_path = pathsafe::join_under(dest, &comps)?;
        if e.is_dir {
            std::fs::create_dir_all(&final_path)?;
        } else if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part_path = if e.is_dir { String::new() } else { format!("{}{PART_SUFFIX}", final_path.to_string_lossy()) };
        parts.push(RecvPart {
            idx: i as u32,
            rel_path: e.path.clone(),
            is_dir: e.is_dir,
            size: e.size,
            mtime: e.mtime,
            part_path,
            final_path: final_path.to_string_lossy().into_owned(),
            done: e.is_dir,
        });
    }
    Ok(parts)
}

/// Picks a free name if something appeared at `target` meanwhile.
fn free_target(target: &Path) -> PathBuf {
    if !exists(target) {
        return target.to_path_buf();
    }
    let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    parent.join(pathsafe::unique_name(&name, false, |n| exists(&parent.join(n))))
}

fn set_mtime(file: &std::fs::File, mtime: i64) {
    if mtime > 0 {
        let t = std::time::UNIX_EPOCH + Duration::from_secs(mtime as u64);
        if let Err(e) = file.set_modified(t) {
            tracing::debug!("could not set modification time: {e}");
        }
    }
}

impl Inner {
    pub(crate) async fn handle_offer(
        self: &Arc<Self>,
        conn: quinn::Connection,
        peer: Fingerprint,
        info: PeerInfo,
        offer: Offer,
        mut send: quinn::SendStream,
    ) -> Result<()> {
        if let Err(e) = validate_offer(&offer) {
            tracing::warn!("rejecting malformed offer from {}: {e}", info.name);
            proto::write_frame(&mut send, &Response::error(ErrorCode::BadRequest, e.to_string())).await?;
            let _ = send.finish();
            return Ok(());
        }
        let id = offer.transfer_id.clone();
        if lock(&self.transfers).contains_key(&id) {
            proto::write_frame(&mut send, &Response::error(ErrorCode::Busy, "transfer already running")).await?;
            let _ = send.finish();
            return Ok(());
        }
        let device = self.store.device(&peer)?.ok_or(Error::NotTrusted)?;
        let peer_name = device.display_name().to_string();

        let existing = self.store.recv_parts(&id, &peer)?;
        let resumed = !existing.is_empty() && same_manifest(&existing, &offer);
        let parts = if resumed {
            existing
        } else {
            if !existing.is_empty() {
                self.discard_partial_files(&id, &peer);
            }
            if !device.auto_accept && !self.ask_user(&conn, &peer, &peer_name, &offer).await {
                proto::write_frame(&mut send, &Response::Rejected).await?;
                let _ = send.finish();
                return Ok(());
            }
            let dest = self.download_dir();
            let offer2 = offer.clone();
            let parts = tokio::task::spawn_blocking(move || plan(&dest, &offer2))
                .await
                .map_err(|e| Error::protocol(format!("planning task failed: {e}")))??;
            self.store.insert_recv_parts(&id, &peer, &parts)?;
            parts
        };

        // Offsets: resume from whatever is on disk.
        let mut offsets = Vec::with_capacity(parts.len());
        let mut bytes_done = 0u64;
        let mut files_done = 0u64;
        let mut remaining = 0u64;
        for p in &parts {
            if p.is_dir {
                offsets.push(0);
            } else if p.done {
                offsets.push(OFFSET_DONE);
                bytes_done += p.size;
                files_done += 1;
            } else {
                let len = part_len(&p.part_path);
                let off = if len <= p.size { len } else { 0 };
                offsets.push(off);
                bytes_done += off;
                remaining += 1;
            }
        }

        let roots = offer.roots();
        let location = parts.iter().find(|p| !p.rel_path.contains('/')).map(|p| p.final_path.clone());
        self.store.upsert_history(&HistoryEntry {
            transfer_id: id.clone(),
            direction: Direction::Receive.as_str().into(),
            peer_fp: peer,
            peer_name: peer_name.clone(),
            summary: summarize(&roots),
            file_count: offer.file_count(),
            total_bytes: offer.total_bytes(),
            status: TransferStatus::InProgress.as_str().into(),
            location,
            started_at: store::now(),
            finished_at: None,
        })?;
        self.store.set_history_status(&id, TransferStatus::InProgress.as_str(), false)?;

        let t = Arc::new(ActiveTransfer::new(
            id.clone(),
            Direction::Receive,
            peer,
            peer_name,
            offer.total_bytes(),
            offer.file_count(),
            conn.clone(),
            Some(RecvState { parts: Mutex::new(parts), remaining: AtomicU64::new(remaining) }),
        ));
        t.progress.bytes_done.store(bytes_done, Ordering::Relaxed);
        t.progress.files_done.store(files_done, Ordering::Relaxed);
        lock(&self.transfers).insert(id.clone(), t.clone());
        self.activity.notify_one();
        self.emit(Event::TransferStarted { transfer: t.view(), resumed });

        proto::write_frame(&mut send, &Response::Accepted { offsets }).await?;
        let _ = send.finish();

        if remaining == 0 {
            self.finish_transfer(&id, Direction::Receive, peer, TransferStatus::Completed, None);
            return Ok(());
        }

        // Watch the connection: if it ends before all files arrived, the
        // transfer is paused, cancelled or interrupted.
        let me = self.clone();
        tokio::spawn(async move {
            conn.closed().await;
            if lock(&me.transfers).get(&t.id).is_some_and(|a| Arc::ptr_eq(a, &t)) {
                let status = Inner::status_after_close(&t);
                me.finish_transfer(&t.id, Direction::Receive, peer, status, None);
            }
        });
        Ok(())
    }

    /// Shows the offer to the user and waits for a decision.
    async fn ask_user(&self, conn: &quinn::Connection, peer: &Fingerprint, peer_name: &str, offer: &Offer) -> bool {
        let (tx, rx) = oneshot::channel();
        lock(&self.pending_offers).insert(offer.transfer_id.clone(), tx);
        self.emit(Event::IncomingOffer {
            transfer_id: offer.transfer_id.clone(),
            from: *peer,
            from_name: peer_name.to_string(),
            file_count: offer.file_count(),
            total_bytes: offer.total_bytes(),
            summary: summarize(&offer.roots()),
        });
        let accepted = tokio::select! {
            r = tokio::time::timeout(DECISION_TIMEOUT, rx) => matches!(r, Ok(Ok(true))),
            _ = conn.closed() => false,
        };
        lock(&self.pending_offers).remove(&offer.transfer_id);
        self.emit(Event::OfferClosed { transfer_id: offer.transfer_id.clone() });
        if !accepted {
            let _ = self.store.upsert_history(&HistoryEntry {
                transfer_id: offer.transfer_id.clone(),
                direction: Direction::Receive.as_str().into(),
                peer_fp: *peer,
                peer_name: peer_name.to_string(),
                summary: summarize(&offer.roots()),
                file_count: offer.file_count(),
                total_bytes: offer.total_bytes(),
                status: TransferStatus::Rejected.as_str().into(),
                location: None,
                started_at: store::now(),
                finished_at: Some(store::now()),
            });
        }
        accepted
    }

    pub(crate) async fn handle_file(
        self: &Arc<Self>,
        peer: Fingerprint,
        transfer_id: &str,
        index: u32,
        offset: u64,
        mut send: quinn::SendStream,
        mut recv: quinn::RecvStream,
    ) -> Result<()> {
        let t = lock(&self.transfers).get(transfer_id).cloned();
        let Some(t) = t.filter(|t| t.direction == Direction::Receive && t.peer == peer) else {
            proto::write_frame(&mut send, &Response::error(ErrorCode::UnknownTransfer, "no such transfer")).await?;
            let _ = send.finish();
            return Ok(());
        };
        let state = t.recv.as_ref().expect("receive transfers carry state");
        let part = lock(&state.parts).get(index as usize).cloned();
        let Some(part) = part.filter(|p| !p.is_dir && !p.done) else {
            proto::write_frame(&mut send, &Response::error(ErrorCode::BadRequest, "bad file index")).await?;
            let _ = send.finish();
            return Ok(());
        };

        match self.receive_file(&t, &part, offset, &mut recv).await {
            Ok(final_path) => {
                let final_str = final_path.to_string_lossy().into_owned();
                self.store.mark_part_done(transfer_id, index, &final_str)?;
                if let Some(p) = lock(&state.parts).get_mut(index as usize) {
                    p.done = true;
                    p.final_path = final_str;
                }
                t.progress.files_done.fetch_add(1, Ordering::Relaxed);
                proto::write_frame(&mut send, &Response::Ok).await?;
                let _ = send.finish();
                if state.remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
                    self.finish_transfer(transfer_id, Direction::Receive, peer, TransferStatus::Completed, None);
                }
                Ok(())
            }
            Err(Error::Integrity(msg)) => {
                tracing::warn!("{}: {msg}", part.rel_path);
                proto::write_frame(&mut send, &Response::error(ErrorCode::HashMismatch, msg)).await?;
                let _ = send.finish();
                Ok(())
            }
            Err(e) => {
                // Local failure (disk full, permissions): stop the transfer
                // but keep what we have so it can be resumed.
                if !matches!(e, Error::Connection(_)) {
                    tracing::error!("receiving {} failed: {e}", part.rel_path);
                    let _ = proto::write_frame(&mut send, &Response::error(ErrorCode::Internal, e.to_string())).await;
                    let _ = send.finish();
                    // The sender sees PAUSED and keeps its journal, so the
                    // user can resume after fixing the problem.
                    t.conn.close(close::PAUSED.into(), b"receiver error");
                    self.finish_transfer(transfer_id, Direction::Receive, peer, TransferStatus::Failed, Some(e.to_string()));
                }
                Err(e)
            }
        }
    }

    async fn receive_file(&self, t: &ActiveTransfer, part: &RecvPart, offset: u64, recv: &mut quinn::RecvStream) -> Result<PathBuf> {
        let part_path = PathBuf::from(&part.part_path);
        if offset > part.size || (offset != 0 && offset != part_len(&part.part_path)) {
            return Err(Error::protocol(format!("unexpected offset {offset} for {}", part.rel_path)));
        }
        let mut file = if offset == 0 {
            tokio::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&part_path).await?
        } else {
            tokio::fs::OpenOptions::new().read(true).write(true).open(&part_path).await?
        };

        t.progress.set_current(&part.rel_path);
        let mut buf = vec![0u8; CHUNK];
        let mut hasher = blake3::Hasher::new();
        if offset > 0 {
            let mut left = offset;
            while left > 0 {
                let n = file.read(&mut buf[..left.min(CHUNK as u64) as usize]).await?;
                if n == 0 {
                    return Err(Error::protocol("partial file shrank"));
                }
                hasher.update(&buf[..n]);
                left -= n as u64;
            }
            file.seek(std::io::SeekFrom::Start(offset)).await?;
        }

        // On any failure below, undo this file's progress contribution.
        let rollback = |received: u64| {
            t.progress.bytes_done.fetch_sub(received, Ordering::Relaxed);
        };
        let mut remaining = part.size - offset;
        let mut received = 0u64;
        while remaining > 0 {
            let want = remaining.min(CHUNK as u64) as usize;
            let n = match recv.read(&mut buf[..want]).await {
                Ok(Some(n)) => n,
                Ok(None) => {
                    rollback(received);
                    return Err(Error::protocol("sender ended the file early"));
                }
                Err(e) => {
                    rollback(received);
                    return Err(e.into());
                }
            };
            if let Err(e) = file.write_all(&buf[..n]).await {
                rollback(received);
                return Err(e.into());
            }
            hasher.update(&buf[..n]);
            remaining -= n as u64;
            received += n as u64;
            t.progress.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
        }

        let mut digest = [0u8; 32];
        if let Err(e) = recv.read_exact(&mut digest).await {
            rollback(received);
            return Err(e.into());
        }
        if hasher.finalize() != blake3::Hash::from_bytes(digest) {
            drop(file);
            let _ = tokio::fs::remove_file(&part_path).await;
            t.progress.bytes_done.fetch_sub(received + offset, Ordering::Relaxed);
            return Err(Error::Integrity(format!("checksum mismatch for {}", part.rel_path)));
        }

        file.flush().await?;
        let std_file = file.into_std().await;
        let mtime = part.mtime;
        let final_path = PathBuf::from(&part.final_path);
        let target = tokio::task::spawn_blocking(move || -> std::io::Result<PathBuf> {
            std_file.sync_all()?;
            set_mtime(&std_file, mtime);
            drop(std_file);
            Ok(final_path)
        })
        .await
        .map_err(|e| Error::protocol(format!("finalize task failed: {e}")))??;

        // Check-and-rename under a lock so two transfers cannot pick the same name.
        let target = {
            let _g = lock(&self.finalize_lock);
            let target = free_target(&target);
            std::fs::rename(&part_path, &target)?;
            target
        };
        Ok(target)
    }

    /// Removes unfinished `.vegamshare-part` files and the journal.
    /// Completed files are kept: the user already has them.
    pub(crate) fn discard_partial_files(&self, transfer_id: &str, peer: &Fingerprint) {
        let parts = self.store.recv_parts(transfer_id, peer).unwrap_or_default();
        let _ = self.store.delete_recv_parts(transfer_id);
        let leftovers: Vec<String> = parts.into_iter().filter(|p| !p.is_dir && !p.done).map(|p| p.part_path).collect();
        if leftovers.is_empty() {
            return;
        }
        // File handles of in-flight streams may still be open (Windows
        // refuses to delete open files), so retry briefly in the background.
        std::thread::spawn(move || {
            for attempt in 0..10 {
                let mut left = false;
                for p in &leftovers {
                    match std::fs::remove_file(p) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(_) => left = true,
                    }
                }
                if !left {
                    return;
                }
                std::thread::sleep(Duration::from_millis(200 * (attempt + 1)));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::Entry;

    fn offer(paths: &[(&str, bool)]) -> Offer {
        Offer {
            transfer_id: "0123456789abcdef0123456789abcdef".into(),
            entries: paths.iter().map(|(p, d)| Entry { path: (*p).into(), is_dir: *d, size: if *d { 0 } else { 3 }, mtime: 0 }).collect(),
        }
    }

    #[test]
    fn validates_offers() {
        assert!(validate_offer(&offer(&[("a", true), ("a/b.txt", false)])).is_ok());
        assert!(validate_offer(&offer(&[("../x", false)])).is_err());
        assert!(validate_offer(&offer(&[("a.txt", false), ("A.TXT", false)])).is_err());
        assert!(validate_offer(&offer(&[("a", false), ("a/b", false)])).is_err());
        assert!(validate_offer(&offer(&[("x/y/z", false), ("X/Y", false)])).is_err());
        assert!(validate_offer(&offer(&[])).is_err());
        let mut bad_id = offer(&[("a", false)]);
        bad_id.transfer_id = "../../etc".into();
        assert!(validate_offer(&bad_id).is_err());
    }

    mod fuzz {
        use super::*;
        use proptest::prelude::*;
        use proptest::test_runner::{Config, TestRunner};

        fn component() -> impl Strategy<Value = String> {
            // Mostly plausible names, sometimes hostile ones.
            prop_oneof![
                12 => r"[a-z]{1,4}( [a-z]{1,3})?(\.[a-z]{2,3})?",
                1 => Just("..".to_string()),
                1 => Just("A".to_string()),
                1 => Just("a".to_string()),
                1 => Just("con".to_string()),
                1 => Just("x:y".to_string()),
                1 => Just("trailing.".to_string()),
            ]
        }

        fn offer() -> impl Strategy<Value = Offer> {
            let entry = (prop::collection::vec(component(), 1..4), prop::bool::weighted(0.3), 0u64..100).prop_map(|(c, is_dir, size)| Entry {
                path: c.join("/"),
                is_dir,
                size: if is_dir { 0 } else { size },
                mtime: 0,
            });
            prop::collection::vec(entry, 1..8)
                .prop_map(|entries| Offer { transfer_id: "0123456789abcdef0123456789abcdef".into(), entries })
        }

        /// Returns whether the offer was accepted; panics if an accepted
        /// offer would write outside the destination or collide.
        fn check(o: &Offer) -> bool {
            if validate_offer(o).is_err() {
                return false;
            }
            let d = tempfile::tempdir().unwrap();
            let dest = d.path().join("dl");
            let parts = plan(&dest, o).unwrap();
            let mut finals = std::collections::HashSet::new();
            for p in &parts {
                assert!(Path::new(&p.final_path).starts_with(&dest), "{} escapes {dest:?}", p.final_path);
                assert!(finals.insert(p.final_path.to_lowercase()), "duplicate target {}", p.final_path);
                if !p.is_dir {
                    assert!(Path::new(&p.part_path).starts_with(&dest));
                }
            }
            let outside: Vec<_> = std::fs::read_dir(d.path()).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
            assert_eq!(outside, vec![std::ffi::OsString::from("dl")], "something was created outside the destination");
            true
        }

        /// Any offer the validator accepts is planned strictly inside the
        /// destination. Also asserts enough generated offers are valid,
        /// so the property cannot pass vacuously.
        #[test]
        fn accepted_offers_stay_inside_destination() {
            let mut runner = TestRunner::new(Config { cases: 2000, ..Config::default() });
            let accepted = std::cell::Cell::new(0u32);
            runner
                .run(&offer(), |o| {
                    if check(&o) {
                        accepted.set(accepted.get() + 1);
                    }
                    Ok(())
                })
                .unwrap();
            assert!(accepted.get() >= 300, "only {} of 2000 generated offers were valid", accepted.get());
        }
    }

    #[test]
    fn plan_renames_existing_roots_and_keeps_structure() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("Foto")).unwrap();
        std::fs::write(d.path().join("a.txt"), b"mine").unwrap();
        let o = offer(&[("Foto", true), ("Foto/2024", true), ("Foto/2024/x.jpg", false), ("a.txt", false), ("Lain/y.txt", false)]);
        let parts = plan(d.path(), &o).unwrap();
        let rel = |p: &RecvPart| Path::new(&p.final_path).strip_prefix(d.path()).unwrap().to_string_lossy().replace('\\', "/");
        assert_eq!(rel(&parts[0]), "Foto (1)");
        assert_eq!(rel(&parts[2]), "Foto (1)/2024/x.jpg");
        assert_eq!(rel(&parts[3]), "a (1).txt");
        assert_eq!(rel(&parts[4]), "Lain/y.txt");
        assert!(d.path().join("Foto (1)/2024").is_dir());
        assert_eq!(std::fs::read(d.path().join("a.txt")).unwrap(), b"mine");
    }
}
