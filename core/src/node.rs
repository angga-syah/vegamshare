//! The running service: listener, presence tracking and the public API
//! used by the UI.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use serde::Serialize;
use tokio::sync::{broadcast, oneshot};
use tokio_util::sync::CancellationToken;

use crate::discovery::{self, Announce, Discovery};
use crate::error::{Error, Result};
use crate::identity::{self, CERT_SUBJECT, Fingerprint, Identity};
use crate::pairing::{self, Lockout, ServerOutcome};
use crate::quickpair::{self, QrPayload};
use crate::proto::{self, ErrorCode, MAX_FRAME, MAX_OFFER_FRAME, PeerInfo, Request, Response, close};
use crate::secret;
use crate::store::{self, HistoryEntry, Store, TrustedDevice};
use crate::tls;
use crate::transfer::{ActiveTransfer, Direction, Source, TransferStatus, TransferView};

pub const DEFAULT_PORT: u16 = 47901;
pub const DEFAULT_DISCOVERY_PORT: u16 = 47900;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a device counts as "online" after the last verified contact.
/// Nothing is polled: the status only changes when the user acts.
const ONLINE_GRACE: Duration = Duration::from_secs(120);
const PROGRESS_TICK: Duration = Duration::from_millis(300);

const KEY_NAME: &str = "device_name";
const KEY_PASSWORD: &str = "password";
const KEY_DOWNLOAD_DIR: &str = "download_dir";
const KEY_RECEIVING: &str = "receiving";
const KEY_VISIBLE: &str = "visible";

/// Opens a host-specific source handle (e.g. an Android content URI).
pub type ContentOpener = Arc<dyn Fn(&str) -> std::io::Result<std::fs::File> + Send + Sync>;

pub struct Config {
    /// Private folder for the database and identity key.
    pub data_dir: PathBuf,
    pub default_device_name: String,
    /// "windows", "android", ...
    pub platform: String,
    /// QUIC listen port; falls back to a random port if taken. 0 = random.
    pub port: u16,
    /// `None` disables LAN discovery (tests, or manual-IP-only setups).
    pub discovery_port: Option<u16>,
    pub default_download_dir: PathBuf,
    pub content_opener: Option<ContentOpener>,
}

impl Config {
    pub fn new(data_dir: impl Into<PathBuf>, download_dir: impl Into<PathBuf>, device_name: impl Into<String>, platform: impl Into<String>) -> Self {
        Config {
            data_dir: data_dir.into(),
            default_device_name: device_name.into(),
            platform: platform.into(),
            port: DEFAULT_PORT,
            discovery_port: Some(DEFAULT_DISCOVERY_PORT),
            default_download_dir: download_dir.into(),
            content_opener: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    DeviceStatus { fingerprint: Fingerprint, online: bool },
    /// Someone nearby wants to pair; show `code` and ask the user.
    PairRequest { request_id: String, from_name: String, from_platform: String, code: String },
    PairRequestClosed { request_id: String },
    DevicesChanged,
    NearbyChanged,
    IncomingOffer { transfer_id: String, from: Fingerprint, from_name: String, file_count: u64, total_bytes: u64, summary: String },
    OfferClosed { transfer_id: String },
    TransferStarted { transfer: TransferView, resumed: bool },
    TransferProgress { transfer: TransferView, speed_bps: u64 },
    TransferFinished { transfer_id: String, direction: Direction, peer: Fingerprint, status: TransferStatus, error: Option<String>, location: Option<String> },
}

#[derive(Clone, Debug, Serialize)]
pub struct MyInfo {
    pub device_id: String,
    pub device_id_formatted: String,
    pub name: String,
    pub platform: String,
    pub fingerprint: String,
    pub port: u16,
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceView {
    #[serde(flatten)]
    pub device: TrustedDevice,
    pub display_name: String,
    pub online: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NearbyView {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub address: String,
    /// Heuristic from the announced key prefix; confirmed only on connect.
    pub paired: bool,
}

#[derive(Default)]
struct Presence {
    online: bool,
    verified_at: Option<Instant>,
}

pub(crate) struct Inner {
    pub cfg: Config,
    pub identity: Identity,
    pub store: Store,
    pub endpoint: quinn::Endpoint,
    pub events: broadcast::Sender<Event>,
    name: RwLock<String>,
    password: RwLock<String>,
    download_dir: RwLock<PathBuf>,
    lockout: std::sync::Mutex<Lockout>,
    presence: Mutex<HashMap<Fingerprint, Presence>>,
    discovery: RwLock<Option<Arc<Discovery>>>,
    pub transfers: Mutex<HashMap<String, Arc<ActiveTransfer>>>,
    pub pending_offers: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    pub finalize_lock: Mutex<()>,
    pub shutdown: CancellationToken,
    /// Woken when a transfer starts, so idle loops can sleep.
    pub activity: tokio::sync::Notify,
    receiving: std::sync::atomic::AtomicBool,
    /// "Add device" screen open until this moment (accepts PairAsk).
    pairing_until: Mutex<Option<Instant>>,
    /// The one pairing request currently shown to the user.
    pending_pair: Mutex<Option<(String, oneshot::Sender<bool>)>>,
    qr_tokens: Mutex<quickpair::Tokens>,
    /// Outgoing nearby pairings waiting for the other user to accept.
    attempts: Mutex<HashMap<String, PairAttempt>>,
}

struct PairAttempt {
    conn: quinn::Connection,
    recv: quinn::RecvStream,
    expected_id: String,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn read<T: Clone>(l: &RwLock<T>) -> T {
    l.read().unwrap_or_else(|e| e.into_inner()).clone()
}

fn transport() -> Arc<quinn::TransportConfig> {
    let mut t = quinn::TransportConfig::default();
    t.max_concurrent_bidi_streams(256u32.into());
    t.max_concurrent_uni_streams(0u32.into());
    t.stream_receive_window((8u32 << 20).into());
    t.receive_window((32u32 << 20).into());
    t.send_window(32 << 20);
    t.keep_alive_interval(Some(Duration::from_secs(4)));
    t.max_idle_timeout(Some(Duration::from_secs(20).try_into().expect("20s fits in a VarInt")));
    Arc::new(t)
}

fn make_endpoint(id: &Identity, port: u16) -> Result<quinn::Endpoint> {
    let crypto = QuicServerConfig::try_from(tls::server_config(id)?).map_err(|e| Error::protocol(format!("QUIC TLS setup: {e}")))?;
    let mut server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    server.transport_config(transport());
    let bind = |p: u16| quinn::Endpoint::server(server.clone(), SocketAddr::from(([0, 0, 0, 0], p)));
    match bind(port) {
        Ok(e) => Ok(e),
        Err(e) if port != 0 && e.kind() == std::io::ErrorKind::AddrInUse => {
            tracing::warn!("port {port} is busy, using a random port");
            Ok(bind(0)?)
        }
        Err(e) => Err(e.into()),
    }
}

impl Inner {
    pub(crate) fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }

    pub(crate) fn my_info(&self) -> PeerInfo {
        PeerInfo {
            device_id: self.identity.device_id.clone(),
            name: read(&self.name),
            platform: self.cfg.platform.clone(),
            version: proto::PROTOCOL_VERSION,
        }
    }

    pub(crate) fn download_dir(&self) -> PathBuf {
        read(&self.download_dir)
    }

    fn discovery(&self) -> Option<Arc<Discovery>> {
        read(&self.discovery)
    }

    fn client_config(&self, expected: Option<Fingerprint>) -> Result<quinn::ClientConfig> {
        let crypto = QuicClientConfig::try_from(tls::client_config(&self.identity, expected)?)
            .map_err(|e| Error::protocol(format!("QUIC TLS setup: {e}")))?;
        let mut c = quinn::ClientConfig::new(Arc::new(crypto));
        c.transport_config(transport());
        Ok(c)
    }

    async fn dial(&self, addr: SocketAddr, expected: Option<Fingerprint>) -> Result<quinn::Connection> {
        let connecting = self.endpoint.connect_with(self.client_config(expected)?, addr, CERT_SUBJECT)?;
        let conn = tokio::time::timeout(CONNECT_TIMEOUT, connecting).await??;
        if let Some(exp) = expected {
            // The verifier already enforced this; checked again so a future
            // change to the TLS setup cannot silently drop pinning.
            match tls::peer_fingerprint(&conn) {
                Some(fp) if fp.ct_eq(&exp) => {}
                _ => {
                    conn.close(close::NORMAL.into(), b"identity mismatch");
                    return Err(Error::IdentityMismatch);
                }
            }
        }
        Ok(conn)
    }

    fn candidates(&self, fp: &Fingerprint) -> Vec<SocketAddr> {
        let mut v = Vec::new();
        if let Some(seen) = self.discovery().and_then(|d| d.lookup_fp(&fp.short())) {
            v.push(seen.addr);
        }
        if let Ok(Some(dev)) = self.store.device(fp)
            && let Some(addr) = dev.last_addr.and_then(|a| a.parse().ok())
            && !v.contains(&addr)
        {
            v.push(addr);
        }
        v
    }

    /// Connects to a paired device, verifying its pinned certificate.
    pub(crate) async fn connect_trusted(self: &Arc<Self>, fp: &Fingerprint) -> Result<quinn::Connection> {
        if !self.store.is_trusted(fp)? {
            return Err(Error::NotTrusted);
        }
        // Knock at the last known address first; only if that fails, ask the
        // network where this device ID went (its IP may have changed).
        let mut tried: Vec<SocketAddr> = Vec::new();
        for addr in self.candidates(fp) {
            tried.push(addr);
            if let Some(conn) = self.try_dial(fp, addr).await {
                return Ok(conn);
            }
        }
        let device_id = self.store.device(fp)?.map(|d| d.device_id);
        if let (Some(d), Some(id)) = (self.discovery(), device_id) {
            for seen in d.find(&id).await {
                if !tried.contains(&seen.addr)
                    && let Some(conn) = self.try_dial(fp, seen.addr).await
                {
                    return Ok(conn);
                }
            }
        }
        Err(Error::Unreachable)
    }

    async fn try_dial(self: &Arc<Self>, fp: &Fingerprint, addr: SocketAddr) -> Option<quinn::Connection> {
        match self.dial(addr, Some(*fp)).await {
            Ok(conn) => {
                self.mark_seen(fp, conn.remote_address(), None);
                Some(conn)
            }
            Err(e) => {
                tracing::debug!("dial {addr} for {fp:?} failed: {e}");
                None
            }
        }
    }

    /// Records that `fp` was just reached with a verified handshake.
    pub(crate) fn mark_seen(self: &Arc<Self>, fp: &Fingerprint, addr: SocketAddr, name: Option<&str>) {
        if let Err(e) = self.store.touch_device(fp, &addr.to_string(), name) {
            tracing::warn!("could not update device address: {e}");
        }
        lock(&self.presence).entry(*fp).or_default().verified_at = Some(Instant::now());
        self.set_online(fp, true);
        self.schedule_offline(*fp);
    }

    /// Flips the status back to offline once the grace period passes without
    /// new contact. One sleeping timer per contact; nothing polls.
    fn schedule_offline(self: &Arc<Self>, fp: Fingerprint) {
        let me = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = me.shutdown.cancelled() => return,
                _ = tokio::time::sleep(ONLINE_GRACE) => {}
            }
            let busy = lock(&me.transfers).values().any(|t| t.peer == fp && t.conn.close_reason().is_none());
            if busy {
                me.schedule_offline(fp);
                return;
            }
            let stale = lock(&me.presence).get(&fp).and_then(|p| p.verified_at).is_none_or(|t| t.elapsed() >= ONLINE_GRACE);
            if stale {
                me.set_online(&fp, false);
            }
        });
    }

    fn set_online(self: &Arc<Self>, fp: &Fingerprint, online: bool) {
        let changed = {
            let mut p = lock(&self.presence);
            let e = p.entry(*fp).or_default();
            let changed = e.online != online;
            e.online = online;
            changed
        };
        if changed {
            self.emit(Event::DeviceStatus { fingerprint: *fp, online });
            if online {
                let me = self.clone();
                let fp = *fp;
                tokio::spawn(async move { me.auto_resume(fp).await });
            }
        }
    }

    fn is_online(&self, fp: &Fingerprint) -> bool {
        lock(&self.presence).get(fp).is_some_and(|p| p.online)
    }

    /// Restarts interrupted uploads to a device that just came back.
    async fn auto_resume(self: Arc<Self>, fp: Fingerprint) {
        let Ok(ids) = self.store.outgoing_for_peer(&fp) else { return };
        let history = self.store.history(10_000).unwrap_or_default();
        for id in ids {
            let interrupted = history.iter().any(|h| h.transfer_id == id && h.status == TransferStatus::Interrupted.as_str());
            if interrupted && !lock(&self.transfers).contains_key(&id) {
                tracing::info!("auto-resuming transfer {id}");
                if let Err(e) = self.resume_send(&id).await {
                    tracing::warn!("auto-resume of {id} failed: {e}");
                }
            }
        }
    }

    fn trust(self: &Arc<Self>, fingerprint: Fingerprint, info: PeerInfo, addr: SocketAddr) -> Result<TrustedDevice> {
        let now = store::now();
        let device = TrustedDevice {
            fingerprint,
            device_id: info.device_id,
            name: info.name,
            platform: info.platform,
            alias: None,
            auto_accept: true,
            last_addr: Some(addr.to_string()),
            last_seen: now,
            paired_at: now,
        };
        self.store.upsert_device(&device)?;
        self.mark_seen(&fingerprint, addr, None);
        self.emit(Event::DevicesChanged);
        Ok(self.store.device(&fingerprint)?.unwrap_or(device))
    }

    // ---- listener ------------------------------------------------------

    async fn accept_loop(self: Arc<Self>) {
        loop {
            let incoming = tokio::select! {
                _ = self.shutdown.cancelled() => break,
                i = self.endpoint.accept() => match i { Some(i) => i, None => break },
            };
            if !self.receiving.load(std::sync::atomic::Ordering::SeqCst) {
                // "Ready to receive" is off: behave as if nobody is home.
                incoming.ignore();
                continue;
            }
            let me = self.clone();
            tokio::spawn(async move {
                if let Err(e) = me.handle_connection(incoming).await {
                    tracing::debug!("incoming connection ended: {e}");
                }
            });
        }
    }

    async fn handle_connection(self: Arc<Self>, incoming: quinn::Incoming) -> Result<()> {
        let conn = tokio::time::timeout(CONNECT_TIMEOUT, incoming.accept()?).await??;
        let peer = tls::peer_fingerprint(&conn).ok_or_else(|| Error::protocol("client sent no certificate"))?;
        if self.store.is_trusted(&peer)? {
            self.mark_seen(&peer, conn.remote_address(), None);
        }
        loop {
            let (send, recv) = tokio::select! {
                _ = self.shutdown.cancelled() => break,
                s = conn.accept_bi() => match s { Ok(s) => s, Err(_) => break },
            };
            let me = self.clone();
            let conn = conn.clone();
            tokio::spawn(async move {
                if let Err(e) = me.handle_stream(conn, peer, send, recv).await {
                    tracing::debug!("request from {peer:?} failed: {e}");
                }
            });
        }
        Ok(())
    }

    async fn handle_stream(self: Arc<Self>, conn: quinn::Connection, peer: Fingerprint, mut send: quinn::SendStream, mut recv: quinn::RecvStream) -> Result<()> {
        // Re-checked per request so "forget device" takes effect immediately.
        let trusted = self.store.is_trusted(&peer)?;
        let max = if trusted { MAX_OFFER_FRAME } else { MAX_FRAME };
        let req: Request = tokio::time::timeout(REQUEST_TIMEOUT, proto::read_frame(&mut recv, max)).await??;

        let deny = |mut send: quinn::SendStream| async move {
            proto::write_frame(&mut send, &Response::error(ErrorCode::NotTrusted, "not paired")).await?;
            let _ = send.finish();
            Ok::<(), Error>(())
        };

        match req {
            Request::Ping { info } => {
                if trusted {
                    self.mark_seen(&peer, conn.remote_address(), Some(&info.sanitized().name));
                }
                proto::write_frame(&mut send, &Response::Pong { info: self.my_info(), trusted }).await?;
                let _ = send.finish();
            }
            Request::PairStart { info, spake } => {
                let password = read(&self.password);
                let me = pairing::Listener {
                    lockout: &self.lockout,
                    password: &password,
                    my_info: self.my_info(),
                    my_fp: self.identity.fingerprint,
                };
                let outcome = pairing::pair_as_server(&conn, &mut send, &mut recv, me, info, &spake).await?;
                if let ServerOutcome::Paired { info, fingerprint } = outcome {
                    tracing::info!("paired with {} ({})", info.name, info.device_id);
                    self.trust(fingerprint, info, conn.remote_address())?;
                }
            }
            Request::PairAsk { info } => {
                self.handle_pair_ask(&conn, peer, info.sanitized(), send).await?;
            }
            Request::PairToken { info, token } => {
                self.handle_pair_token(&conn, peer, info.sanitized(), &token, send).await?;
            }
            Request::Offer { info, offer } => {
                if !trusted {
                    return deny(send).await;
                }
                self.handle_offer(conn, peer, info.sanitized(), offer, send).await?;
            }
            Request::File { transfer_id, index, offset } => {
                if !trusted {
                    return deny(send).await;
                }
                self.handle_file(peer, &transfer_id, index, offset, send, recv).await?;
            }
            Request::Resume { transfer_id } => {
                if !trusted {
                    return deny(send).await;
                }
                let ok = match self.store.outgoing(&transfer_id)? {
                    Some(o) if o.peer_fp.ct_eq(&peer) => {
                        let me = self.clone();
                        tokio::spawn(async move {
                            if let Err(e) = me.resume_send(&transfer_id).await {
                                tracing::warn!("resume requested by peer failed: {e}");
                            }
                        });
                        true
                    }
                    _ => false,
                };
                let resp = if ok { Response::Ok } else { Response::error(ErrorCode::UnknownTransfer, "no such transfer") };
                proto::write_frame(&mut send, &resp).await?;
                let _ = send.finish();
            }
        }
        Ok(())
    }

    // ---- password-free pairing (listener side) ------------------------

    fn pairing_open(&self) -> bool {
        lock(&self.pairing_until).is_some_and(|t| Instant::now() < t)
    }

    async fn handle_pair_ask(self: &Arc<Self>, conn: &quinn::Connection, peer: Fingerprint, info: PeerInfo, mut send: quinn::SendStream) -> Result<()> {
        if !self.pairing_open() {
            proto::write_frame(&mut send, &Response::error(ErrorCode::PairingClosed, "not pairing")).await?;
            let _ = send.finish();
            return Ok(());
        }
        let code = quickpair::sas_code(conn)?;
        let request_id = crate::transfer::new_transfer_id();
        let (tx, rx) = oneshot::channel();
        let busy = {
            let mut pending = lock(&self.pending_pair);
            if pending.is_some() {
                true
            } else {
                *pending = Some((request_id.clone(), tx));
                false
            }
        };
        if busy {
            proto::write_frame(&mut send, &Response::error(ErrorCode::Busy, "another request is pending")).await?;
            let _ = send.finish();
            return Ok(());
        }
        self.emit(Event::PairRequest {
            request_id: request_id.clone(),
            from_name: info.name.clone(),
            from_platform: info.platform.clone(),
            code,
        });
        let accepted = tokio::select! {
            r = tokio::time::timeout(Duration::from_secs(60), rx) => matches!(r, Ok(Ok(true))),
            _ = conn.closed() => false,
        };
        {
            let mut pending = lock(&self.pending_pair);
            if pending.as_ref().is_some_and(|(id, _)| *id == request_id) {
                *pending = None;
            }
        }
        self.emit(Event::PairRequestClosed { request_id });
        if accepted {
            tracing::info!("paired with {} ({}) by code", info.name, info.device_id);
            self.trust(peer, info, conn.remote_address())?;
            proto::write_frame(&mut send, &Response::PairOk { info: self.my_info() }).await?;
        } else {
            proto::write_frame(&mut send, &Response::Rejected).await?;
        }
        let _ = send.finish();
        Ok(())
    }

    async fn handle_pair_token(self: &Arc<Self>, conn: &quinn::Connection, peer: Fingerprint, info: PeerInfo, token: &str, mut send: quinn::SendStream) -> Result<()> {
        // Rate-limited like passwords, although a 128-bit token cannot be guessed.
        let gate = self.lockout.lock().unwrap_or_else(|e| e.into_inner()).try_attempt(Instant::now());
        if let Err(wait) = gate {
            let resp = Response::Error { code: ErrorCode::LockedOut, message: "too many attempts".into(), retry_after: Some(wait) };
            proto::write_frame(&mut send, &resp).await?;
            let _ = send.finish();
            return Ok(());
        }
        let valid = token.len() == 32 && hex::decode(token).is_ok_and(|t| lock(&self.qr_tokens).consume(&t));
        if !valid {
            proto::write_frame(&mut send, &Response::error(ErrorCode::InvalidCode, "invalid or expired QR code")).await?;
            let _ = send.finish();
            return Ok(());
        }
        self.lockout.lock().unwrap_or_else(|e| e.into_inner()).success();
        tracing::info!("paired with {} ({}) by QR code", info.name, info.device_id);
        self.trust(peer, info, conn.remote_address())?;
        proto::write_frame(&mut send, &Response::PairOk { info: self.my_info() }).await?;
        let _ = send.finish();
        Ok(())
    }

    /// Where to reach `device_id` for a first pairing: the address the user
    /// typed (used once, never relied on later), a device seen while
    /// pairing mode is on, or an ID lookup.
    async fn pair_addrs(&self, device_id: &str, address: Option<&str>) -> Result<Vec<SocketAddr>> {
        if let Some(a) = address.map(str::trim).filter(|a| !a.is_empty()) {
            let addr = a
                .parse::<SocketAddr>()
                .or_else(|_| a.parse::<std::net::IpAddr>().map(|ip| SocketAddr::new(ip, DEFAULT_PORT)))
                .map_err(|_| Error::InvalidArgument(format!("{a} is not an IP address")))?;
            return Ok(vec![addr]);
        }
        let Some(d) = self.discovery() else { return Err(Error::NotFound) };
        let mut addrs: Vec<SocketAddr> = d.lookup_device_id(device_id).into_iter().map(|s| s.addr).collect();
        if addrs.is_empty() {
            addrs = d.find(device_id).await.into_iter().map(|s| s.addr).collect();
        }
        if addrs.is_empty() { Err(Error::NotFound) } else { Ok(addrs) }
    }


    // ---- background loops ---------------------------------------------

    async fn progress_loop(self: Arc<Self>) {
        let mut last: HashMap<String, (u64, Instant, f64)> = HashMap::new();
        loop {
            // Sleep until a transfer exists; tick only while one is running.
            let idle = lock(&self.transfers).is_empty();
            tokio::select! {
                _ = self.shutdown.cancelled() => break,
                _ = self.activity.notified(), if idle => continue,
                _ = tokio::time::sleep(PROGRESS_TICK), if !idle => {}
            }
            let active: Vec<_> = lock(&self.transfers).values().cloned().collect();
            last.retain(|id, _| active.iter().any(|t| &t.id == id));
            for t in active {
                let view = t.view();
                let now = Instant::now();
                let speed = match last.get(&t.id) {
                    Some((bytes, at, ema)) => {
                        let dt = now.duration_since(*at).as_secs_f64().max(0.001);
                        let inst = view.bytes_done.saturating_sub(*bytes) as f64 / dt;
                        if *ema == 0.0 { inst } else { ema * 0.7 + inst * 0.3 }
                    }
                    None => 0.0,
                };
                last.insert(t.id.clone(), (view.bytes_done, now, speed));
                self.emit(Event::TransferProgress { transfer: view, speed_bps: speed as u64 });
            }
        }
    }

    pub(crate) async fn ping(self: &Arc<Self>, fp: &Fingerprint) -> Result<PeerInfo> {
        let conn = self.connect_trusted(fp).await?;
        let result = async {
            let (mut send, mut recv) = conn.open_bi().await?;
            proto::write_frame(&mut send, &Request::Ping { info: self.my_info() }).await?;
            let _ = send.finish();
            match tokio::time::timeout(CONNECT_TIMEOUT, proto::read_frame(&mut recv, MAX_FRAME)).await?? {
                Response::Pong { info, .. } => Ok(info.sanitized()),
                other => Err(other.into_error()),
            }
        }
        .await;
        conn.close(close::NORMAL.into(), b"");
        let info = result?;
        let _ = self.store.touch_device(fp, &conn.remote_address().to_string(), Some(&info.name));
        Ok(info)
    }

    // ---- transfer bookkeeping -----------------------------------------

    /// Ends a transfer exactly once: history, journals, event.
    pub(crate) fn finish_transfer(&self, id: &str, direction: Direction, peer: Fingerprint, status: TransferStatus, error: Option<String>) {
        let active = lock(&self.transfers).remove(id);
        if let Err(e) = self.store.set_history_status(id, status.as_str(), true) {
            tracing::warn!("history update failed: {e}");
        }
        match direction {
            Direction::Send => {
                if !status.is_resumable() {
                    let _ = self.store.delete_outgoing(id);
                }
            }
            Direction::Receive => match status {
                TransferStatus::Completed => {
                    let _ = self.store.delete_recv_parts(id);
                }
                TransferStatus::Cancelled | TransferStatus::Rejected => {
                    self.discard_partial_files(id, &peer);
                }
                _ => {}
            },
        }
        let location = self
            .store
            .history(10_000)
            .ok()
            .and_then(|h| h.into_iter().find(|h| h.transfer_id == id))
            .and_then(|h| h.location);
        tracing::info!("transfer {id} ({}) finished: {}{}", direction.as_str(), status.as_str(), error.as_deref().map(|e| format!(" ({e})")).unwrap_or_default());
        drop(active);
        self.emit(Event::TransferFinished { transfer_id: id.to_string(), direction, peer, status, error, location });
    }

    /// Status of a transfer whose connection has closed.
    pub(crate) fn status_after_close(t: &ActiveTransfer) -> TransferStatus {
        use crate::transfer::StopRequest;
        match t.stop_request() {
            StopRequest::Pause => return TransferStatus::Paused,
            StopRequest::Cancel => return TransferStatus::Cancelled,
            StopRequest::None => {}
        }
        match t.conn.close_reason() {
            Some(quinn::ConnectionError::ApplicationClosed(c)) => match u32::try_from(c.error_code.into_inner()).unwrap_or(0) {
                close::PAUSED => TransferStatus::Paused,
                close::CANCELLED => TransferStatus::Cancelled,
                close::REJECTED => TransferStatus::Rejected,
                _ => TransferStatus::Interrupted,
            },
            _ => TransferStatus::Interrupted,
        }
    }
}

/// Handle to the running service. Cheap to clone.
#[derive(Clone)]
pub struct Node {
    inner: Arc<Inner>,
}

impl Node {
    pub async fn start(cfg: Config) -> Result<Node> {
        std::fs::create_dir_all(&cfg.data_dir)?;
        let store = Store::open(&cfg.data_dir.join("vegamshare.db"))?;
        let identity = Identity::load_or_create(&cfg.data_dir, &store)?;

        let name = store.get_string(KEY_NAME)?.unwrap_or_else(|| proto::clean_label(&cfg.default_device_name, 64));
        let stored = store
            .get_setting(KEY_PASSWORD)?
            .and_then(|s| secret::unprotect(&s).ok())
            .and_then(|p| String::from_utf8(p).ok());
        let password = match stored {
            Some(p) => p,
            None => {
                let p = identity::random_password(8);
                store.set_setting(KEY_PASSWORD, &secret::protect(p.as_bytes())?)?;
                p
            }
        };
        let download_dir = store.get_string(KEY_DOWNLOAD_DIR)?.map(PathBuf::from).unwrap_or_else(|| cfg.default_download_dir.clone());
        let receiving = store.get_string(KEY_RECEIVING)?.as_deref() != Some("0");
        let visible = store.get_string(KEY_VISIBLE)?.as_deref() == Some("1");

        let endpoint = make_endpoint(&identity, cfg.port)?;
        let (events, _) = broadcast::channel(1024);

        let inner = Arc::new(Inner {
            identity,
            store,
            endpoint,
            events,
            name: RwLock::new(name),
            password: RwLock::new(password),
            download_dir: RwLock::new(download_dir),
            lockout: std::sync::Mutex::new(Lockout::default()),
            presence: Mutex::new(HashMap::new()),
            discovery: RwLock::new(None),
            transfers: Mutex::new(HashMap::new()),
            pending_offers: Mutex::new(HashMap::new()),
            finalize_lock: Mutex::new(()),
            shutdown: CancellationToken::new(),
            activity: tokio::sync::Notify::new(),
            receiving: std::sync::atomic::AtomicBool::new(receiving),
            pairing_until: Mutex::new(None),
            pending_pair: Mutex::new(None),
            qr_tokens: Mutex::new(quickpair::Tokens::default()),
            attempts: Mutex::new(HashMap::new()),
            cfg,
        });

        if let Some(port) = inner.cfg.discovery_port {
            let me = Announce {
                device_id: inner.identity.device_id.clone(),
                name: read(&inner.name),
                platform: inner.cfg.platform.clone(),
                port: inner.endpoint.local_addr()?.port(),
                fp_short: inner.identity.fingerprint.short(),
            };
            let events = inner.events.clone();
            match Discovery::bind(port, me, Box::new(move || drop(events.send(Event::NearbyChanged)))) {
                Ok(d) => {
                    d.set_visible(visible);
                    d.set_answering(receiving);
                    d.spawn(inner.shutdown.clone());
                    *inner.discovery.write().unwrap_or_else(|e| e.into_inner()) = Some(d);
                }
                Err(e) => tracing::warn!("LAN discovery disabled: {e}"),
            }
        }

        tokio::spawn(inner.clone().accept_loop());
        tokio::spawn(inner.clone().progress_loop());
        Ok(Node { inner })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.inner.identity.fingerprint
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.endpoint.local_addr()?)
    }

    pub fn my_info(&self) -> MyInfo {
        let id = &self.inner.identity.device_id;
        let port = self.inner.endpoint.local_addr().map(|a| a.port()).unwrap_or(0);
        MyInfo {
            device_id: id.clone(),
            device_id_formatted: identity::format_device_id(id),
            name: read(&self.inner.name),
            platform: self.inner.cfg.platform.clone(),
            fingerprint: self.inner.identity.fingerprint.pretty(),
            port,
            addresses: discovery::local_addresses().into_iter().map(|ip| format!("{ip}:{port}")).collect(),
        }
    }

    // ---- settings -------------------------------------------------------

    pub fn password(&self) -> String {
        read(&self.inner.password)
    }

    pub fn set_password(&self, password: &str) -> Result<()> {
        pairing::validate_password(password)?;
        self.inner.store.set_setting(KEY_PASSWORD, &secret::protect(password.as_bytes())?)?;
        *self.inner.password.write().unwrap_or_else(|e| e.into_inner()) = password.to_string();
        Ok(())
    }

    pub fn regenerate_password(&self) -> Result<String> {
        let p = identity::random_password(8);
        self.set_password(&p)?;
        Ok(p)
    }

    pub fn set_device_name(&self, name: &str) -> Result<()> {
        let name = proto::clean_label(name, 64);
        self.inner.store.set_setting(KEY_NAME, name.as_bytes())?;
        *self.inner.name.write().unwrap_or_else(|e| e.into_inner()) = name.clone();
        if let Some(d) = self.inner.discovery() {
            d.update_me(|a| a.name = name);
        }
        Ok(())
    }

    /// "Ready to receive". When off, incoming connections and ID queries are
    /// ignored entirely, so the device looks absent. Sending still works.
    pub fn receiving(&self) -> bool {
        self.inner.receiving.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn set_receiving(&self, on: bool) -> Result<()> {
        self.inner.store.set_setting(KEY_RECEIVING, if on { b"1" } else { b"0" })?;
        self.inner.receiving.store(on, std::sync::atomic::Ordering::SeqCst);
        if let Some(d) = self.inner.discovery() {
            d.set_answering(on);
        }
        Ok(())
    }

    /// "Visible on the network": periodic announcements for the nearby list.
    /// Off by default; pairing by ID works without it.
    pub fn visible(&self) -> bool {
        self.inner.discovery().is_some_and(|d| d.visible_setting())
    }

    pub fn set_visible(&self, on: bool) -> Result<()> {
        self.inner.store.set_setting(KEY_VISIBLE, if on { b"1" } else { b"0" })?;
        if let Some(d) = self.inner.discovery() {
            d.set_visible(on);
        }
        Ok(())
    }

    pub fn download_dir(&self) -> PathBuf {
        self.inner.download_dir()
    }

    pub fn set_download_dir(&self, dir: PathBuf) -> Result<()> {
        std::fs::create_dir_all(&dir)?;
        self.inner.store.set_setting(KEY_DOWNLOAD_DIR, dir.to_string_lossy().as_bytes())?;
        *self.inner.download_dir.write().unwrap_or_else(|e| e.into_inner()) = dir;
        Ok(())
    }

    // ---- devices --------------------------------------------------------

    pub fn devices(&self) -> Result<Vec<DeviceView>> {
        Ok(self
            .inner
            .store
            .devices()?
            .into_iter()
            .map(|d| DeviceView { online: self.inner.is_online(&d.fingerprint), display_name: d.display_name().to_string(), device: d })
            .collect())
    }

    pub fn nearby(&self) -> Result<Vec<NearbyView>> {
        let Some(d) = self.inner.discovery() else { return Ok(Vec::new()) };
        let paired: Vec<String> = self.inner.store.devices()?.iter().map(|d| d.fingerprint.short()).collect();
        Ok(d.nearby()
            .into_iter()
            .map(|s| NearbyView {
                paired: paired.contains(&s.fp_short),
                device_id: s.device_id,
                name: s.name,
                platform: s.platform,
                address: s.addr.to_string(),
            })
            .collect())
    }

    /// Pairs with the device that has `device_id`, found on the LAN or at
    /// `address` (IP or IP:port). Only needed once per device.
    pub async fn pair(&self, device_id: &str, address: Option<&str>, password: &str) -> Result<TrustedDevice> {
        let inner = &self.inner;
        let device_id = identity::normalize_device_id(device_id).ok_or_else(|| Error::InvalidArgument("device ID must be 9 digits".into()))?;
        if device_id == inner.identity.device_id {
            return Err(Error::InvalidArgument("that is this device's own ID".into()));
        }

        let addrs = inner.pair_addrs(&device_id, address).await?;
        let mut last_err = Error::NotFound;
        for addr in addrs {
            let conn = match inner.dial(addr, None).await {
                Ok(c) => c,
                Err(e) => {
                    last_err = e;
                    continue;
                }
            };
            let result = pairing::pair_as_client(&conn, inner.my_info(), inner.identity.fingerprint, password).await;
            match result {
                Ok((info, fp)) => {
                    conn.close(close::NORMAL.into(), b"");
                    if info.device_id != device_id {
                        // Right password but a different ID: the user typed
                        // the address of another device. Refuse to pin it.
                        return Err(Error::InvalidArgument(format!("the device at {addr} has ID {}", identity::format_device_id(&info.device_id))));
                    }
                    return inner.trust(fp, info, conn.remote_address());
                }
                Err(e @ (Error::WrongPassword | Error::LockedOut(_))) => {
                    conn.close(close::NORMAL.into(), b"");
                    return Err(e);
                }
                Err(e) => {
                    conn.close(close::NORMAL.into(), b"");
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    /// "Add device" screen open/closed. While open (at most two minutes per
    /// call) this device is visible and accepts code-confirmed pairing.
    pub fn set_pairing_mode(&self, on: bool) {
        let inner = &self.inner;
        if on {
            *lock(&inner.pairing_until) = Some(Instant::now() + quickpair::QR_TTL);
            if let Some(d) = inner.discovery() {
                d.set_temp_visible(true);
            }
            let me = inner.clone();
            tokio::spawn(async move {
                tokio::time::sleep(quickpair::QR_TTL + Duration::from_millis(50)).await;
                let expired = lock(&me.pairing_until).is_none_or(|t| Instant::now() >= t);
                if expired {
                    *lock(&me.pairing_until) = None;
                    if let Some(d) = me.discovery() {
                        d.set_temp_visible(false);
                    }
                }
            });
        } else {
            *lock(&inner.pairing_until) = None;
            lock(&inner.qr_tokens).revoke_all();
            if let Some(d) = inner.discovery() {
                d.set_temp_visible(false);
            }
        }
    }

    pub fn respond_to_pair_request(&self, request_id: &str, accept: bool) -> Result<()> {
        let mut pending = lock(&self.inner.pending_pair);
        match pending.take() {
            Some((id, tx)) if id == request_id => {
                let _ = tx.send(accept);
                Ok(())
            }
            other => {
                *pending = other;
                Err(Error::UnknownTransfer)
            }
        }
    }

    /// Step 1 of tap-to-pair: connects and returns `(attempt_id, code)`.
    /// Show the code; the other user accepts if it matches theirs.
    pub async fn pair_nearby_start(&self, device_id: &str, address: Option<&str>) -> Result<(String, String)> {
        let inner = &self.inner;
        let device_id = identity::normalize_device_id(device_id).ok_or_else(|| Error::InvalidArgument("device ID must be 9 digits".into()))?;
        if device_id == inner.identity.device_id {
            return Err(Error::InvalidArgument("that is this device's own ID".into()));
        }
        let mut last_err = Error::NotFound;
        for addr in inner.pair_addrs(&device_id, address).await? {
            let conn = match inner.dial(addr, None).await {
                Ok(c) => c,
                Err(e) => {
                    last_err = e;
                    continue;
                }
            };
            let code = quickpair::sas_code(&conn)?;
            let (mut send, recv) = conn.open_bi().await?;
            proto::write_frame(&mut send, &Request::PairAsk { info: inner.my_info() }).await?;
            let _ = send.finish();
            let attempt_id = crate::transfer::new_transfer_id();
            lock(&inner.attempts).insert(attempt_id.clone(), PairAttempt { conn, recv, expected_id: device_id.clone() });
            return Ok((attempt_id, code));
        }
        Err(last_err)
    }

    /// Step 2: waits (up to 90 s) for the other user's decision.
    pub async fn pair_nearby_finish(&self, attempt_id: &str) -> Result<TrustedDevice> {
        let inner = &self.inner;
        let attempt = lock(&inner.attempts).remove(attempt_id).ok_or(Error::UnknownTransfer)?;
        let PairAttempt { conn, mut recv, expected_id } = attempt;
        let result = async {
            match tokio::time::timeout(Duration::from_secs(90), proto::read_frame(&mut recv, MAX_FRAME)).await?? {
                Response::PairOk { info } => {
                    let info = info.sanitized();
                    if info.device_id != expected_id {
                        return Err(Error::InvalidArgument("a different device answered".into()));
                    }
                    let fp = tls::peer_fingerprint(&conn).ok_or_else(|| Error::protocol("peer has no certificate"))?;
                    inner.trust(fp, info, conn.remote_address())
                }
                other => Err(other.into_error()),
            }
        }
        .await;
        conn.close(close::NORMAL.into(), b"");
        result
    }

    pub fn pair_nearby_cancel(&self, attempt_id: &str) {
        let attempt = lock(&self.inner.attempts).remove(attempt_id);
        if let Some(a) = attempt {
            a.conn.close(close::CANCELLED.into(), b"cancelled");
        }
    }

    /// A fresh single-use QR pairing code (valid two minutes).
    pub fn pair_qr_code(&self) -> String {
        let inner = &self.inner;
        let port = inner.endpoint.local_addr().map(|a| a.port()).unwrap_or(DEFAULT_PORT);
        let token = lock(&inner.qr_tokens).issue();
        QrPayload {
            device_id: inner.identity.device_id.clone(),
            fingerprint: inner.identity.fingerprint,
            addrs: discovery::local_addresses().into_iter().map(|ip| SocketAddr::from((ip, port))).collect(),
            token,
        }
        .encode()
    }

    /// Pairs with the device whose QR code was scanned. No password, no prompt.
    pub async fn pair_with_qr(&self, scanned: &str) -> Result<TrustedDevice> {
        let inner = &self.inner;
        let qr = QrPayload::decode(scanned)?;
        if qr.device_id == inner.identity.device_id {
            return Err(Error::InvalidArgument("that is this device's own code".into()));
        }
        let mut conn = None;
        for addr in &qr.addrs {
            if let Ok(c) = inner.dial(*addr, Some(qr.fingerprint)).await {
                conn = Some(c);
                break;
            }
        }
        if conn.is_none()
            && let Some(d) = inner.discovery()
        {
            for seen in d.find(&qr.device_id).await {
                if let Ok(c) = inner.dial(seen.addr, Some(qr.fingerprint)).await {
                    conn = Some(c);
                    break;
                }
            }
        }
        let conn = conn.ok_or(Error::Unreachable)?;
        let result = async {
            let (mut send, mut recv) = conn.open_bi().await?;
            proto::write_frame(&mut send, &Request::PairToken { info: inner.my_info(), token: hex::encode(qr.token) }).await?;
            let _ = send.finish();
            match tokio::time::timeout(REQUEST_TIMEOUT, proto::read_frame(&mut recv, MAX_FRAME)).await?? {
                Response::PairOk { info } => {
                    let info = info.sanitized();
                    if info.device_id != qr.device_id {
                        return Err(Error::InvalidArgument("a different device answered".into()));
                    }
                    inner.trust(qr.fingerprint, info, conn.remote_address())
                }
                other => Err(other.into_error()),
            }
        }
        .await;
        conn.close(close::NORMAL.into(), b"");
        result
    }


    pub fn forget_device(&self, fp: &Fingerprint) -> Result<()> {
        self.inner.store.remove_device(fp)?;
        lock(&self.inner.presence).remove(fp);
        // Drop live connections so the peer cannot keep using one.
        for t in lock(&self.inner.transfers).values().filter(|t| t.peer == *fp) {
            t.request_stop(crate::transfer::StopRequest::Cancel);
            t.conn.close(close::CANCELLED.into(), b"device removed");
        }
        self.inner.emit(Event::DevicesChanged);
        Ok(())
    }

    pub fn rename_device(&self, fp: &Fingerprint, alias: Option<&str>) -> Result<()> {
        let alias = alias.map(|a| proto::clean_label(a, 64)).filter(|a| a != "Unknown");
        self.inner.store.set_device_alias(fp, alias.as_deref())?;
        self.inner.emit(Event::DevicesChanged);
        Ok(())
    }

    pub fn set_auto_accept(&self, fp: &Fingerprint, on: bool) -> Result<()> {
        self.inner.store.set_device_auto_accept(fp, on)?;
        self.inner.emit(Event::DevicesChanged);
        Ok(())
    }

    /// Checks reachability right now (also refreshes the stored address).
    pub async fn ping(&self, fp: &Fingerprint) -> Result<()> {
        self.inner.ping(fp).await.map(|_| ())
    }

    // ---- transfers ------------------------------------------------------

    /// Sends files and folders; returns the transfer ID immediately.
    pub async fn send_paths(&self, to: &Fingerprint, paths: Vec<PathBuf>) -> Result<String> {
        let items = tokio::task::spawn_blocking(move || crate::transfer::manifest::build(&paths))
            .await
            .map_err(|e| Error::protocol(format!("manifest task failed: {e}")))??;
        self.inner.start_send(*to, items).await
    }

    /// Sends host-provided entries (e.g. Android content URIs).
    pub async fn send_entries(&self, to: &Fingerprint, items: Vec<(proto::Entry, Source)>) -> Result<String> {
        self.inner.start_send(*to, items).await
    }

    pub fn respond_to_offer(&self, transfer_id: &str, accept: bool) -> Result<()> {
        let tx = lock(&self.inner.pending_offers).remove(transfer_id).ok_or(Error::UnknownTransfer)?;
        let _ = tx.send(accept);
        Ok(())
    }

    pub fn pause(&self, transfer_id: &str) -> Result<()> {
        self.stop(transfer_id, crate::transfer::StopRequest::Pause, close::PAUSED)
    }

    pub fn cancel(&self, transfer_id: &str) -> Result<()> {
        if self.stop(transfer_id, crate::transfer::StopRequest::Cancel, close::CANCELLED).is_ok() {
            return Ok(());
        }
        // Not active: drop whatever resumable state is left.
        let inner = &self.inner;
        if let Some(o) = inner.store.outgoing(transfer_id)? {
            inner.finish_transfer(transfer_id, Direction::Send, o.peer_fp, TransferStatus::Cancelled, None);
            return Ok(());
        }
        if let Some(peer) = inner.store.recv_peer(transfer_id)? {
            inner.finish_transfer(transfer_id, Direction::Receive, peer, TransferStatus::Cancelled, None);
            return Ok(());
        }
        Err(Error::UnknownTransfer)
    }

    fn stop(&self, transfer_id: &str, req: crate::transfer::StopRequest, code: u32) -> Result<()> {
        let t = lock(&self.inner.transfers).get(transfer_id).cloned().ok_or(Error::UnknownTransfer)?;
        t.request_stop(req);
        t.conn.close(code.into(), b"");
        Ok(())
    }

    /// Continues a paused or interrupted transfer, from either side.
    pub async fn resume(&self, transfer_id: &str) -> Result<()> {
        let inner = &self.inner;
        if lock(&inner.transfers).contains_key(transfer_id) {
            return Ok(());
        }
        if inner.store.outgoing(transfer_id)?.is_some() {
            return inner.resume_send(transfer_id).await;
        }
        let peer = inner.store.recv_peer(transfer_id)?.ok_or(Error::UnknownTransfer)?;
        let conn = inner.connect_trusted(&peer).await?;
        let result = async {
            let (mut send, mut recv) = conn.open_bi().await?;
            proto::write_frame(&mut send, &Request::Resume { transfer_id: transfer_id.to_string() }).await?;
            let _ = send.finish();
            match tokio::time::timeout(REQUEST_TIMEOUT, proto::read_frame(&mut recv, MAX_FRAME)).await?? {
                Response::Ok => Ok(()),
                other => Err(other.into_error()),
            }
        }
        .await;
        conn.close(close::NORMAL.into(), b"");
        result
    }

    pub fn active_transfers(&self) -> Vec<TransferView> {
        lock(&self.inner.transfers).values().map(|t| t.view()).collect()
    }

    pub fn history(&self, limit: u32) -> Result<Vec<HistoryEntry>> {
        self.inner.store.history(limit)
    }

    pub fn clear_history(&self) -> Result<()> {
        self.inner.store.clear_history()
    }

    /// Stops everything. Active transfers become resumable.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        for (_, tx) in lock(&self.inner.pending_offers).drain() {
            let _ = tx.send(false);
        }
        self.inner.endpoint.close(close::SHUTDOWN.into(), b"shutdown");
        let _ = tokio::time::timeout(Duration::from_secs(2), self.inner.endpoint.wait_idle()).await;
    }
}
