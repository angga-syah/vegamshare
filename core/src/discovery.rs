//! LAN discovery via UDP multicast and broadcast.
//!
//! By default a device is silent: it never announces itself and only
//! answers a query that names its own device ID ("is 123456789 here?").
//! Strangers on the network therefore cannot list VegamShare devices; only
//! someone who already knows the ID gets an answer, and they still need the
//! password (pairing) or the pinned key (paired devices) to connect.
//! "Visible" mode re-enables periodic announcements for the nearby list.
//!
//! Announcements are unauthenticated hints ("device 123456789 with key
//! prefix ab12... listens on 192.168.1.5:47901"). They are only used to find
//! an address to dial; the TLS handshake against the pinned certificate
//! decides whether the device really is who it claims to be.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, SockRef, Socket, Type};
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use crate::identity::is_valid_device_id;
use crate::proto::clean_label;

const APP: &str = "vegamshare";
const VERSION: u32 = 1;
pub const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 77);
const ANNOUNCE_EVERY: Duration = Duration::from_secs(5);
/// Housekeeping cadence while silent (interface changes, expiry).
const IDLE_TICK: Duration = Duration::from_secs(30);
const FIND_TIMEOUT: Duration = Duration::from_millis(1500);
pub(crate) const EXPIRE_AFTER: Duration = Duration::from_secs(16);
const REPLY_MIN_INTERVAL: Duration = Duration::from_secs(1);
const MAX_PACKET: usize = 1024;
const MAX_TRACKED: usize = 256;

#[cfg(test)]
mod rank_tests {
    use super::*;

    #[test]
    fn hotspot_and_lan_addresses_come_first() {
        let mut v: Vec<Ipv4Addr> = ["192.0.0.4", "10.12.3.4", "169.254.1.2", "100.70.1.1", "192.168.43.1", "172.20.0.5", "8.8.8.8"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        v.sort_by_key(|ip| address_rank(*ip));
        let s: Vec<String> = v.iter().map(|i| i.to_string()).collect();
        assert_eq!(s, ["192.168.43.1", "172.20.0.5", "10.12.3.4", "8.8.8.8", "100.70.1.1", "169.254.1.2", "192.0.0.4"]);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Announce,
    Query,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Packet {
    app: String,
    v: u32,
    kind: Kind,
    id: String,
    name: String,
    platform: String,
    port: u16,
    fp: String,
    /// For queries: only the device with this ID should answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    want: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Announce {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub port: u16,
    pub fp_short: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub fp_short: String,
    pub addr: SocketAddr,
    pub at: Instant,
}

pub(crate) struct Discovery {
    sock: UdpSocket,
    port: u16,
    me: RwLock<Announce>,
    seen: Mutex<HashMap<String, Seen>>,
    replied: Mutex<HashMap<IpAddr, Instant>>,
    joined: Mutex<HashSet<Ipv4Addr>>,
    on_change: Box<dyn Fn() + Send + Sync>,
    /// Answer queries for our own ID (off while not receiving).
    answering: AtomicBool,
    /// Temporarily visible while the "Add device" screen is open.
    temp_visible: AtomicBool,
    /// Wakes the housekeeping loop when visibility changes.
    wake: tokio::sync::Notify,
}

/// IPv4 interfaces, routable LAN addresses first. Link-local addresses
/// (169.254.x.x) come last: they are what two computers joined by a plain
/// network cable get, but Windows also keeps them on idle adapters.
fn local_ipv4() -> Vec<if_addrs::Ifv4Addr> {
    let mut v: Vec<if_addrs::Ifv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(a) if !a.ip.is_loopback() && !a.ip.is_unspecified() => Some(a),
            _ => None,
        })
        .collect();
    v.sort_by_key(|a| address_rank(a.ip));
    v
}

/// Lower is better. Home routers and Android hotspots use 192.168.x.x;
/// 10.x and 100.64/10 are typical of mobile data; 192.0.0.x is the 464XLAT
/// shim on phones; link-local only matters for a direct cable.
fn address_rank(ip: Ipv4Addr) -> u8 {
    let o = ip.octets();
    match o {
        [192, 0, 0, _] => 6,
        [192, 168, ..] => 0,
        [172, b, ..] if (16..32).contains(&b) => 1,
        [10, ..] => 2,
        [100, b, ..] if (64..128).contains(&b) => 4,
        [169, 254, ..] => 5,
        _ => 3,
    }
}

/// Addresses of this machine on the LAN, for display ("connect by IP").
pub fn local_addresses() -> Vec<Ipv4Addr> {
    local_ipv4().into_iter().map(|a| a.ip).collect()
}

impl Discovery {
    pub(crate) fn bind(port: u16, me: Announce, on_change: Box<dyn Fn() + Send + Sync>) -> std::io::Result<Arc<Self>> {
        let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        s.set_reuse_address(true)?;
        #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
        s.set_reuse_port(true)?;
        s.set_broadcast(true)?;
        s.set_multicast_loop_v4(true)?;
        s.set_nonblocking(true)?;
        s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())?;
        let sock = UdpSocket::from_std(s.into())?;
        let d = Arc::new(Discovery {
            sock,
            port,
            me: RwLock::new(me),
            seen: Mutex::new(HashMap::new()),
            replied: Mutex::new(HashMap::new()),
            joined: Mutex::new(HashSet::new()),
            on_change,
            answering: AtomicBool::new(true),
            temp_visible: AtomicBool::new(false),
            wake: tokio::sync::Notify::new(),
        });
        d.join_groups();
        Ok(d)
    }

    pub(crate) fn spawn(self: &Arc<Self>, cancel: CancellationToken) {
        let me = self.clone();
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = c.cancelled() => {}
                _ = me.recv_loop() => {}
            }
        });
        let me = self.clone();
        tokio::spawn(async move {
            let mut last_join = Instant::now();
            loop {
                let visible = me.is_visible();
                if visible {
                    me.send_packet_all(Kind::Announce, None).await;
                }
                let pause = if visible { ANNOUNCE_EVERY } else { IDLE_TICK };
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(pause) => {}
                    _ = me.wake.notified() => {}
                }
                // Wi-Fi can come and go; pick up new interfaces periodically.
                if last_join.elapsed() >= IDLE_TICK {
                    me.join_groups();
                    last_join = Instant::now();
                }
                me.prune();
            }
        });
    }

    /// Visible only while the "Add device" screen is open.
    pub(crate) fn is_visible(&self) -> bool {
        self.temp_visible.load(Ordering::SeqCst)
    }

    pub(crate) fn set_temp_visible(self: &Arc<Self>, on: bool) {
        let was = self.temp_visible.swap(on, Ordering::SeqCst);
        if was != on {
            self.wake.notify_one();
        }
        if !on && was {
            // Leaving the screen: forget who we saw, it goes stale anyway.
            self.seen.lock().unwrap_or_else(|e| e.into_inner()).clear();
            (self.on_change)();
        }
        if on && !was {
            // Ask everyone visible to introduce themselves right away.
            let me = self.clone();
            tokio::spawn(async move { me.send_packet_all(Kind::Query, None).await });
        }
    }

    pub(crate) fn set_answering(&self, on: bool) {
        self.answering.store(on, Ordering::SeqCst);
    }

    fn join_groups(&self) {
        let sock = SockRef::from(&self.sock);
        let mut joined = self.joined.lock().unwrap_or_else(|e| e.into_inner());
        for iface in local_ipv4() {
            if joined.contains(&iface.ip) {
                continue;
            }
            match sock.join_multicast_v4(&MULTICAST_GROUP, &iface.ip) {
                Ok(()) => {
                    joined.insert(iface.ip);
                }
                Err(e) => tracing::debug!("multicast join on {} failed: {e}", iface.ip),
            }
        }
    }

    pub(crate) fn update_me(&self, f: impl FnOnce(&mut Announce)) {
        f(&mut self.me.write().unwrap_or_else(|e| e.into_inner()));
    }

    fn packet(&self, kind: Kind, want: Option<&str>) -> Vec<u8> {
        let me = self.me.read().unwrap_or_else(|e| e.into_inner()).clone();
        let p = Packet {
            app: APP.into(),
            v: VERSION,
            kind,
            id: me.device_id,
            name: me.name,
            platform: me.platform,
            port: me.port,
            fp: me.fp_short,
            want: want.map(str::to_string),
        };
        serde_json::to_vec(&p).expect("packet serializes")
    }

    /// Multicast on every interface plus limited and directed broadcast;
    /// some routers and Android hotspots drop one kind but not the other.
    async fn send_packet_all(&self, kind: Kind, want: Option<&str>) {
        let data = self.packet(kind, want);
        let sock = SockRef::from(&self.sock);
        let mut targets: Vec<SocketAddr> = vec![SocketAddr::from((Ipv4Addr::BROADCAST, self.port))];
        for iface in local_ipv4() {
            if sock.set_multicast_if_v4(&iface.ip).is_ok() {
                let _ = self.sock.send_to(&data, SocketAddrV4::new(MULTICAST_GROUP, self.port)).await;
            }
            let bcast = iface.broadcast.unwrap_or_else(|| Ipv4Addr::from(u32::from(iface.ip) | !u32::from(iface.netmask)));
            targets.push(SocketAddr::from((bcast, self.port)));
        }
        targets.dedup();
        for t in targets {
            if let Err(e) = self.sock.send_to(&data, t).await {
                tracing::trace!("discovery send to {t} failed: {e}");
            }
        }
    }

    /// Asks the network where the device with `device_id` is. Only that
    /// device answers. Returns what was heard within ~1.5 s.
    pub(crate) async fn find(&self, device_id: &str) -> Vec<Seen> {
        let started = Instant::now();
        self.send_packet_all(Kind::Query, Some(device_id)).await;
        let mut resent = false;
        loop {
            let found: Vec<Seen> = self.lookup_device_id(device_id).into_iter().filter(|s| s.at >= started).collect();
            if !found.is_empty() || started.elapsed() >= FIND_TIMEOUT {
                return found;
            }
            // Packets get lost on busy Wi-Fi; ask once more.
            if !resent && started.elapsed() >= FIND_TIMEOUT / 3 {
                self.send_packet_all(Kind::Query, Some(device_id)).await;
                resent = true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn recv_loop(&self) {
        let mut buf = [0u8; MAX_PACKET];
        loop {
            let (n, src) = match self.sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    // Windows reports ICMP port-unreachable as a recv error.
                    tracing::trace!("discovery recv error: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            };
            if let Ok(p) = serde_json::from_slice::<Packet>(&buf[..n]) {
                self.handle(p, src).await;
            }
        }
    }

    async fn handle(&self, p: Packet, src: SocketAddr) {
        if p.app != APP || p.v != VERSION || p.port == 0 || !is_valid_device_id(&p.id) {
            return;
        }
        if p.fp.len() != 16 || !p.fp.bytes().all(|b| b.is_ascii_hexdigit()) {
            return;
        }
        if p.fp == self.me.read().unwrap_or_else(|e| e.into_inner()).fp_short {
            return;
        }
        let me_id = self.me.read().unwrap_or_else(|e| e.into_inner()).device_id.clone();
        if p.kind == Kind::Query {
            let for_me = p.want.as_deref() == Some(me_id.as_str());
            let anonymous = p.want.is_none();
            let answer = (for_me && self.answering.load(Ordering::SeqCst)) || (anonymous && self.is_visible());
            if answer && self.may_reply(src.ip()) {
                let data = self.packet(Kind::Announce, None);
                let _ = self.sock.send_to(&data, SocketAddr::new(src.ip(), self.port)).await;
                // Also via multicast/broadcast: when several instances share
                // the discovery port on one host, unicast reaches only one.
                if for_me {
                    self.send_packet_all(Kind::Announce, None).await;
                }
            }
            return;
        }

        let seen = Seen {
            device_id: p.id,
            name: clean_label(&p.name, 64),
            platform: clean_label(&p.platform, 16),
            fp_short: p.fp.to_ascii_lowercase(),
            addr: SocketAddr::new(src.ip(), p.port),
            at: Instant::now(),
        };
        let changed = {
            let mut map = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            let changed = match map.get(&seen.fp_short) {
                Some(old) => old.addr != seen.addr || old.name != seen.name,
                None => true,
            };
            if map.len() < MAX_TRACKED || map.contains_key(&seen.fp_short) {
                map.insert(seen.fp_short.clone(), seen);
            }
            changed
        };
        if changed {
            (self.on_change)();
        }
        // Visible devices greet newcomers directly in case our own
        // multicast/broadcast does not reach them.
        if changed && self.is_visible() && self.may_reply(src.ip()) {
            let data = self.packet(Kind::Announce, None);
            let _ = self.sock.send_to(&data, SocketAddr::new(src.ip(), self.port)).await;
        }
    }

    fn may_reply(&self, ip: IpAddr) -> bool {
        let mut r = self.replied.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if r.get(&ip).is_some_and(|t| now.duration_since(*t) < REPLY_MIN_INTERVAL) {
            return false;
        }
        if r.len() > MAX_TRACKED {
            r.retain(|_, t| now.duration_since(*t) < REPLY_MIN_INTERVAL);
        }
        r.insert(ip, now);
        true
    }

    fn prune(&self) {
        let removed = {
            let mut map = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            let before = map.len();
            map.retain(|_, s| s.at.elapsed() < EXPIRE_AFTER);
            map.len() != before
        };
        if removed {
            (self.on_change)();
        }
    }

    pub(crate) fn nearby(&self) -> Vec<Seen> {
        let mut v: Vec<Seen> = self.seen.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect();
        v.sort_by_key(|a| a.name.to_lowercase());
        v
    }

    pub(crate) fn lookup_fp(&self, fp_short: &str) -> Option<Seen> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).get(fp_short).filter(|s| s.at.elapsed() < EXPIRE_AFTER).cloned()
    }

    pub(crate) fn lookup_device_id(&self, id: &str) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|s| s.device_id == id && s.at.elapsed() < EXPIRE_AFTER)
            .cloned()
            .collect()
    }
}
