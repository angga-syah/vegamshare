//! Flutter-facing API. Thin wrappers over `vegamshare_core::Node`.
//!
//! Errors reach Dart as exceptions whose message is `"<code>|<detail>"`,
//! so the UI can translate the code and still show the detail.

use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;

use flutter_rust_bridge::frb;
use vegamshare_core::{Config, Error, Fingerprint, Node};

use crate::frb_generated::StreamSink;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static NODE: OnceLock<Node> = OnceLock::new();

fn rt() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("vegamshare")
            .build()
            .expect("tokio runtime")
    })
}

/// Runs `f` on the core's runtime (the node's background tasks live there).
async fn on_rt<T: Send + 'static>(f: impl Future<Output = T> + Send + 'static) -> anyhow::Result<T> {
    rt().spawn(f).await.map_err(|e| anyhow::anyhow!("internal|{e}"))
}

fn node() -> anyhow::Result<&'static Node> {
    NODE.get().ok_or_else(|| anyhow::anyhow!("not_started|service not started"))
}

fn code(e: &Error) -> &'static str {
    match e {
        Error::WrongPassword => "wrong_password",
        Error::LockedOut(_) => "locked_out",
        Error::NotFound => "not_found",
        Error::Unreachable => "unreachable",
        Error::NotTrusted => "not_trusted",
        Error::IdentityMismatch => "identity_mismatch",
        Error::Rejected => "rejected",
        Error::InvalidArgument(_) => "invalid_argument",
        Error::InvalidPath(_) => "invalid_path",
        Error::Timeout => "timeout",
        Error::Io(_) => "io",
        Error::UnknownTransfer => "unknown_transfer",
        _ => "internal",
    }
}

fn err(e: Error) -> anyhow::Error {
    let extra = match &e {
        Error::LockedOut(s) => s.to_string(),
        _ => e.to_string(),
    };
    anyhow::anyhow!("{}|{}", code(&e), extra)
}

fn fp(s: &str) -> anyhow::Result<Fingerprint> {
    s.parse().map_err(err)
}

#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
    // Trace-level output from rustls/quinn would flood logcat and slow
    // transfers; keep our own info-level messages only.
    log::set_max_level(log::LevelFilter::Info);
}

pub struct StartConfig {
    pub data_dir: String,
    pub download_dir: String,
    pub device_name: String,
    pub platform: String,
}

pub struct MyInfo {
    pub device_id: String,
    pub device_id_formatted: String,
    pub name: String,
    pub platform: String,
    pub fingerprint: String,
    pub port: u16,
    pub addresses: Vec<String>,
    pub password: String,
    pub download_dir: String,
    pub receiving: bool,
    pub visible: bool,
}

pub struct Device {
    pub fingerprint: String,
    pub device_id: String,
    pub name: String,
    pub display_name: String,
    pub platform: String,
    pub auto_accept: bool,
    pub online: bool,
    pub last_addr: Option<String>,
    pub last_seen: i64,
}

pub struct Nearby {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub address: String,
    pub paired: bool,
}

pub struct Transfer {
    pub transfer_id: String,
    pub outgoing: bool,
    pub peer: String,
    pub peer_name: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current: String,
}

pub struct HistoryItem {
    pub transfer_id: String,
    pub outgoing: bool,
    pub peer: String,
    pub peer_name: String,
    pub summary: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub status: String,
    pub location: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

fn device_dto(d: vegamshare_core::DeviceView) -> Device {
    Device {
        fingerprint: d.device.fingerprint.to_string(),
        device_id: d.device.device_id,
        name: d.device.name,
        display_name: d.display_name,
        platform: d.device.platform,
        auto_accept: d.device.auto_accept,
        online: d.online,
        last_addr: d.device.last_addr,
        last_seen: d.device.last_seen,
    }
}

fn my_info_dto(n: &Node) -> MyInfo {
    let i = n.my_info();
    MyInfo {
        device_id: i.device_id,
        device_id_formatted: i.device_id_formatted,
        name: i.name,
        platform: i.platform,
        fingerprint: i.fingerprint,
        port: i.port,
        addresses: i.addresses,
        password: n.password(),
        download_dir: n.download_dir().to_string_lossy().into_owned(),
        receiving: n.receiving(),
        visible: n.visible(),
    }
}

/// Starts the background service once; later calls return the same info.
pub async fn start(config: StartConfig) -> anyhow::Result<MyInfo> {
    if let Some(n) = NODE.get() {
        return Ok(my_info_dto(n));
    }
    let started = on_rt(async move {
        let cfg = Config::new(
            PathBuf::from(config.data_dir),
            PathBuf::from(config.download_dir),
            config.device_name,
            config.platform,
        );
        Node::start(cfg).await
    })
    .await?
    .map_err(err)?;
    let _ = NODE.set(started);
    Ok(my_info_dto(node()?))
}

/// JSON-encoded `vegamshare_core::Event`s, for the lifetime of the app.
pub fn events(sink: StreamSink<String>) -> anyhow::Result<()> {
    let mut rx = node()?.subscribe();
    rt().spawn(async move {
        loop {
            match rx.recv().await {
                Ok(e) => {
                    let json = serde_json::to_string(&e).unwrap_or_default();
                    if sink.add(json).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });
    Ok(())
}

#[frb(sync)]
pub fn my_info() -> anyhow::Result<MyInfo> {
    Ok(my_info_dto(node()?))
}

pub fn set_password(password: String) -> anyhow::Result<()> {
    node()?.set_password(&password).map_err(err)
}

pub fn regenerate_password() -> anyhow::Result<String> {
    node()?.regenerate_password().map_err(err)
}

pub fn set_device_name(name: String) -> anyhow::Result<()> {
    node()?.set_device_name(&name).map_err(err)
}

pub fn set_receiving(on: bool) -> anyhow::Result<()> {
    node()?.set_receiving(on).map_err(err)
}

pub fn set_visible(on: bool) -> anyhow::Result<()> {
    node()?.set_visible(on).map_err(err)
}

pub fn set_download_dir(dir: String) -> anyhow::Result<()> {
    node()?.set_download_dir(PathBuf::from(dir)).map_err(err)
}

pub fn devices() -> anyhow::Result<Vec<Device>> {
    Ok(node()?.devices().map_err(err)?.into_iter().map(device_dto).collect())
}

pub fn nearby() -> anyhow::Result<Vec<Nearby>> {
    Ok(node()?
        .nearby()
        .map_err(err)?
        .into_iter()
        .map(|n| Nearby { device_id: n.device_id, name: n.name, platform: n.platform, address: n.address, paired: n.paired })
        .collect())
}

pub async fn pair(device_id: String, address: Option<String>, password: String) -> anyhow::Result<Device> {
    let n = node()?.clone();
    let d = on_rt(async move { n.pair(&device_id, address.as_deref(), &password).await }).await?.map_err(err)?;
    let fp = d.fingerprint;
    node()?
        .devices()
        .map_err(err)?
        .into_iter()
        .find(|v| v.device.fingerprint == fp)
        .map(device_dto)
        .ok_or_else(|| anyhow::anyhow!("internal|paired device missing"))
}

pub fn forget_device(fingerprint: String) -> anyhow::Result<()> {
    node()?.forget_device(&fp(&fingerprint)?).map_err(err)
}

pub fn rename_device(fingerprint: String, alias: Option<String>) -> anyhow::Result<()> {
    node()?.rename_device(&fp(&fingerprint)?, alias.as_deref().filter(|a| !a.trim().is_empty())).map_err(err)
}

pub fn set_auto_accept(fingerprint: String, on: bool) -> anyhow::Result<()> {
    node()?.set_auto_accept(&fp(&fingerprint)?, on).map_err(err)
}

pub async fn ping(fingerprint: String) -> anyhow::Result<()> {
    let n = node()?.clone();
    let f = fp(&fingerprint)?;
    on_rt(async move { n.ping(&f).await }).await?.map_err(err)
}

pub async fn send_paths(fingerprint: String, paths: Vec<String>) -> anyhow::Result<String> {
    let n = node()?.clone();
    let f = fp(&fingerprint)?;
    let paths = paths.into_iter().map(PathBuf::from).collect();
    on_rt(async move { n.send_paths(&f, paths).await }).await?.map_err(err)
}

pub fn respond_to_offer(transfer_id: String, accept: bool) -> anyhow::Result<()> {
    node()?.respond_to_offer(&transfer_id, accept).map_err(err)
}

pub fn pause(transfer_id: String) -> anyhow::Result<()> {
    node()?.pause(&transfer_id).map_err(err)
}

pub fn cancel(transfer_id: String) -> anyhow::Result<()> {
    node()?.cancel(&transfer_id).map_err(err)
}

pub async fn resume(transfer_id: String) -> anyhow::Result<()> {
    let n = node()?.clone();
    on_rt(async move { n.resume(&transfer_id).await }).await?.map_err(err)
}

pub fn active_transfers() -> anyhow::Result<Vec<Transfer>> {
    Ok(node()?
        .active_transfers()
        .into_iter()
        .map(|t| Transfer {
            transfer_id: t.transfer_id,
            outgoing: t.direction == vegamshare_core::Direction::Send,
            peer: t.peer.to_string(),
            peer_name: t.peer_name,
            bytes_done: t.bytes_done,
            bytes_total: t.bytes_total,
            files_done: t.files_done,
            files_total: t.files_total,
            current: t.current,
        })
        .collect())
}

pub fn history(limit: u32) -> anyhow::Result<Vec<HistoryItem>> {
    Ok(node()?
        .history(limit)
        .map_err(err)?
        .into_iter()
        .map(|h| HistoryItem {
            transfer_id: h.transfer_id,
            outgoing: h.direction == "send",
            peer: h.peer_fp.to_string(),
            peer_name: h.peer_name,
            summary: h.summary,
            file_count: h.file_count,
            total_bytes: h.total_bytes,
            status: h.status,
            location: h.location,
            started_at: h.started_at,
            finished_at: h.finished_at,
        })
        .collect())
}

pub fn clear_history() -> anyhow::Result<()> {
    node()?.clear_history().map_err(err)
}

pub async fn shutdown() -> anyhow::Result<()> {
    if let Some(n) = NODE.get() {
        let n = n.clone();
        on_rt(async move { n.shutdown().await }).await?;
    }
    Ok(())
}
