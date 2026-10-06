//! Persistent state: settings, paired devices, transfer history and the
//! journals that make interrupted transfers resumable.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::error::Result;
use crate::identity::Fingerprint;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS devices (
    fingerprint BLOB PRIMARY KEY,
    device_id   TEXT NOT NULL,
    name        TEXT NOT NULL,
    platform    TEXT NOT NULL,
    alias       TEXT,
    auto_accept INTEGER NOT NULL DEFAULT 1,
    last_addr   TEXT,
    last_seen   INTEGER NOT NULL DEFAULT 0,
    paired_at   INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS history (
    transfer_id TEXT PRIMARY KEY,
    direction   TEXT NOT NULL,
    peer_fp     BLOB NOT NULL,
    peer_name   TEXT NOT NULL,
    summary     TEXT NOT NULL,
    file_count  INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL,
    status      TEXT NOT NULL,
    location    TEXT,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER
);
CREATE TABLE IF NOT EXISTS recv_parts (
    transfer_id TEXT NOT NULL,
    idx         INTEGER NOT NULL,
    peer_fp     BLOB NOT NULL,
    rel_path    TEXT NOT NULL,
    is_dir      INTEGER NOT NULL,
    size        INTEGER NOT NULL,
    mtime       INTEGER NOT NULL,
    part_path   TEXT NOT NULL,
    final_path  TEXT NOT NULL,
    done        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (transfer_id, idx)
);
CREATE TABLE IF NOT EXISTS outgoing (
    transfer_id TEXT PRIMARY KEY,
    peer_fp     BLOB NOT NULL,
    manifest    TEXT NOT NULL,
    sources     TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);
"#;

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TrustedDevice {
    pub fingerprint: Fingerprint,
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub alias: Option<String>,
    pub auto_accept: bool,
    pub last_addr: Option<String>,
    pub last_seen: i64,
    pub paired_at: i64,
}

impl TrustedDevice {
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryEntry {
    pub transfer_id: String,
    pub direction: String,
    pub peer_fp: Fingerprint,
    pub peer_name: String,
    pub summary: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub status: String,
    pub location: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct RecvPart {
    pub idx: u32,
    pub rel_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    pub part_path: String,
    pub final_path: String,
    pub done: bool,
}

#[derive(Clone, Debug)]
pub struct Outgoing {
    pub transfer_id: String,
    pub peer_fp: Fingerprint,
    pub manifest: String,
    pub sources: String,
    pub created_at: i64,
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn fp_from_row(bytes: Vec<u8>) -> rusqlite::Result<Fingerprint> {
    Fingerprint::from_slice(&bytes).ok_or_else(|| rusqlite::Error::InvalidColumnType(0, "fingerprint".into(), rusqlite::types::Type::Blob))
}

fn device_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrustedDevice> {
    Ok(TrustedDevice {
        fingerprint: fp_from_row(r.get(0)?)?,
        device_id: r.get(1)?,
        name: r.get(2)?,
        platform: r.get(3)?,
        alias: r.get(4)?,
        auto_accept: r.get(5)?,
        last_addr: r.get(6)?,
        last_seen: r.get(7)?,
        paired_at: r.get(8)?,
    })
}

const DEVICE_COLS: &str = "fingerprint, device_id, name, platform, alias, auto_accept, last_addr, last_seen, paired_at";

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot leave SQLite inconsistent
        // (each statement is atomic), so recovering the guard is safe.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- settings -------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .db()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &[u8]) -> Result<()> {
        self.db().execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_string(&self, key: &str) -> Result<Option<String>> {
        Ok(self.get_setting(key)?.and_then(|v| String::from_utf8(v).ok()))
    }

    // ---- devices --------------------------------------------------------

    pub fn upsert_device(&self, d: &TrustedDevice) -> Result<()> {
        self.db().execute(
            "INSERT INTO devices(fingerprint, device_id, name, platform, alias, auto_accept, last_addr, last_seen, paired_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(fingerprint) DO UPDATE SET
                device_id = excluded.device_id, name = excluded.name, platform = excluded.platform,
                last_addr = COALESCE(excluded.last_addr, devices.last_addr),
                last_seen = MAX(excluded.last_seen, devices.last_seen)",
            params![
                d.fingerprint.as_bytes().as_slice(),
                d.device_id,
                d.name,
                d.platform,
                d.alias,
                d.auto_accept,
                d.last_addr,
                d.last_seen,
                d.paired_at
            ],
        )?;
        Ok(())
    }

    pub fn device(&self, fp: &Fingerprint) -> Result<Option<TrustedDevice>> {
        Ok(self
            .db()
            .query_row(&format!("SELECT {DEVICE_COLS} FROM devices WHERE fingerprint = ?1"), [fp.as_bytes().as_slice()], device_from_row)
            .optional()?)
    }

    pub fn devices(&self) -> Result<Vec<TrustedDevice>> {
        let db = self.db();
        let mut stmt = db.prepare(&format!("SELECT {DEVICE_COLS} FROM devices ORDER BY paired_at"))?;
        let rows = stmt.query_map([], device_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn is_trusted(&self, fp: &Fingerprint) -> Result<bool> {
        Ok(self.device(fp)?.is_some())
    }

    pub fn remove_device(&self, fp: &Fingerprint) -> Result<bool> {
        Ok(self.db().execute("DELETE FROM devices WHERE fingerprint = ?1", [fp.as_bytes().as_slice()])? > 0)
    }

    pub fn set_device_alias(&self, fp: &Fingerprint, alias: Option<&str>) -> Result<()> {
        self.db().execute("UPDATE devices SET alias = ?2 WHERE fingerprint = ?1", params![fp.as_bytes().as_slice(), alias])?;
        Ok(())
    }

    pub fn set_device_auto_accept(&self, fp: &Fingerprint, on: bool) -> Result<()> {
        self.db().execute("UPDATE devices SET auto_accept = ?2 WHERE fingerprint = ?1", params![fp.as_bytes().as_slice(), on])?;
        Ok(())
    }

    pub fn touch_device(&self, fp: &Fingerprint, addr: &str, name: Option<&str>) -> Result<()> {
        self.db().execute(
            "UPDATE devices SET last_addr = ?2, last_seen = ?3, name = COALESCE(?4, name) WHERE fingerprint = ?1",
            params![fp.as_bytes().as_slice(), addr, now(), name],
        )?;
        Ok(())
    }

    // ---- history --------------------------------------------------------

    pub fn upsert_history(&self, h: &HistoryEntry) -> Result<()> {
        self.db().execute(
            "INSERT INTO history(transfer_id, direction, peer_fp, peer_name, summary, file_count, total_bytes, status, location, started_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(transfer_id) DO UPDATE SET status = excluded.status, finished_at = excluded.finished_at,
                location = COALESCE(excluded.location, history.location)",
            params![
                h.transfer_id,
                h.direction,
                h.peer_fp.as_bytes().as_slice(),
                h.peer_name,
                h.summary,
                h.file_count as i64,
                h.total_bytes as i64,
                h.status,
                h.location,
                h.started_at,
                h.finished_at
            ],
        )?;
        Ok(())
    }

    pub fn set_history_status(&self, transfer_id: &str, status: &str, finished: bool) -> Result<()> {
        let finished_at = finished.then(now);
        self.db().execute(
            "UPDATE history SET status = ?2, finished_at = ?3 WHERE transfer_id = ?1",
            params![transfer_id, status, finished_at],
        )?;
        Ok(())
    }

    pub fn history(&self, limit: u32) -> Result<Vec<HistoryEntry>> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT transfer_id, direction, peer_fp, peer_name, summary, file_count, total_bytes, status, location, started_at, finished_at
             FROM history ORDER BY started_at DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(HistoryEntry {
                transfer_id: r.get(0)?,
                direction: r.get(1)?,
                peer_fp: fp_from_row(r.get(2)?)?,
                peer_name: r.get(3)?,
                summary: r.get(4)?,
                file_count: r.get::<_, i64>(5)? as u64,
                total_bytes: r.get::<_, i64>(6)? as u64,
                status: r.get(7)?,
                location: r.get(8)?,
                started_at: r.get(9)?,
                finished_at: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn clear_history(&self) -> Result<()> {
        self.db().execute("DELETE FROM history WHERE finished_at IS NOT NULL", [])?;
        Ok(())
    }

    // ---- receive journal ------------------------------------------------

    pub fn insert_recv_parts(&self, transfer_id: &str, peer: &Fingerprint, parts: &[RecvPart]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO recv_parts(transfer_id, idx, peer_fp, rel_path, is_dir, size, mtime, part_path, final_path, done)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for p in parts {
                stmt.execute(params![
                    transfer_id,
                    p.idx,
                    peer.as_bytes().as_slice(),
                    p.rel_path,
                    p.is_dir,
                    p.size as i64,
                    p.mtime,
                    p.part_path,
                    p.final_path,
                    p.done
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn recv_parts(&self, transfer_id: &str, peer: &Fingerprint) -> Result<Vec<RecvPart>> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT idx, rel_path, is_dir, size, mtime, part_path, final_path, done FROM recv_parts
             WHERE transfer_id = ?1 AND peer_fp = ?2 ORDER BY idx",
        )?;
        let rows = stmt.query_map(params![transfer_id, peer.as_bytes().as_slice()], |r| {
            Ok(RecvPart {
                idx: r.get(0)?,
                rel_path: r.get(1)?,
                is_dir: r.get(2)?,
                size: r.get::<_, i64>(3)? as u64,
                mtime: r.get(4)?,
                part_path: r.get(5)?,
                final_path: r.get(6)?,
                done: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn recv_peer(&self, transfer_id: &str) -> Result<Option<Fingerprint>> {
        let bytes: Option<Vec<u8>> = self
            .db()
            .query_row("SELECT peer_fp FROM recv_parts WHERE transfer_id = ?1 LIMIT 1", [transfer_id], |r| r.get(0))
            .optional()?;
        Ok(bytes.and_then(|b| Fingerprint::from_slice(&b)))
    }

    pub fn mark_part_done(&self, transfer_id: &str, idx: u32, final_path: &str) -> Result<()> {
        self.db().execute(
            "UPDATE recv_parts SET done = 1, final_path = ?3 WHERE transfer_id = ?1 AND idx = ?2",
            params![transfer_id, idx, final_path],
        )?;
        Ok(())
    }

    pub fn delete_recv_parts(&self, transfer_id: &str) -> Result<()> {
        self.db().execute("DELETE FROM recv_parts WHERE transfer_id = ?1", [transfer_id])?;
        Ok(())
    }

    // ---- send journal ---------------------------------------------------

    pub fn insert_outgoing(&self, o: &Outgoing) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO outgoing(transfer_id, peer_fp, manifest, sources, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![o.transfer_id, o.peer_fp.as_bytes().as_slice(), o.manifest, o.sources, o.created_at],
        )?;
        Ok(())
    }

    pub fn outgoing(&self, transfer_id: &str) -> Result<Option<Outgoing>> {
        Ok(self
            .db()
            .query_row(
                "SELECT transfer_id, peer_fp, manifest, sources, created_at FROM outgoing WHERE transfer_id = ?1",
                [transfer_id],
                |r| {
                    Ok(Outgoing {
                        transfer_id: r.get(0)?,
                        peer_fp: fp_from_row(r.get(1)?)?,
                        manifest: r.get(2)?,
                        sources: r.get(3)?,
                        created_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn outgoing_for_peer(&self, peer: &Fingerprint) -> Result<Vec<String>> {
        let db = self.db();
        let mut stmt = db.prepare("SELECT transfer_id FROM outgoing WHERE peer_fp = ?1 ORDER BY created_at")?;
        let rows = stmt.query_map([peer.as_bytes().as_slice()], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_outgoing(&self, transfer_id: &str) -> Result<()> {
        self.db().execute("DELETE FROM outgoing WHERE transfer_id = ?1", [transfer_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(b: u8) -> TrustedDevice {
        TrustedDevice {
            fingerprint: Fingerprint([b; 32]),
            device_id: "123456789".into(),
            name: "Laptop".into(),
            platform: "windows".into(),
            alias: None,
            auto_accept: true,
            last_addr: Some("192.168.1.2:47901".into()),
            last_seen: 10,
            paired_at: 5,
        }
    }

    #[test]
    fn device_crud() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_device(&dev(1)).unwrap();
        assert!(s.is_trusted(&Fingerprint([1; 32])).unwrap());
        assert!(!s.is_trusted(&Fingerprint([2; 32])).unwrap());

        s.set_device_alias(&Fingerprint([1; 32]), Some("Kantor")).unwrap();
        s.set_device_auto_accept(&Fingerprint([1; 32]), false).unwrap();
        // Re-pairing must not wipe user preferences.
        s.upsert_device(&TrustedDevice { name: "Laptop 2".into(), ..dev(1) }).unwrap();
        let d = s.device(&Fingerprint([1; 32])).unwrap().unwrap();
        assert_eq!(d.display_name(), "Kantor");
        assert_eq!(d.name, "Laptop 2");
        assert!(!d.auto_accept);

        assert!(s.remove_device(&Fingerprint([1; 32])).unwrap());
        assert!(s.devices().unwrap().is_empty());
    }

    #[test]
    fn settings_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.get_setting("x").unwrap().is_none());
        s.set_setting("x", b"1").unwrap();
        s.set_setting("x", b"2").unwrap();
        assert_eq!(s.get_string("x").unwrap().as_deref(), Some("2"));
    }
}
