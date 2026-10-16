//! End-to-end tests: real nodes talking QUIC over localhost.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vegamshare_core::{Config, Error, Event, Fingerprint, Node, TransferStatus};
use tokio::sync::broadcast::Receiver;

const WAIT: Duration = Duration::from_secs(120);

/// Test folders live under target/tmp: nodes still running in background
/// tasks keep SQLite files open, so Windows cannot always delete them.
fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

struct TestNode {
    node: Node,
    root: PathBuf,
}

impl TestNode {
    async fn start(root: &Path, name: &str) -> Self {
        let mut cfg = Config::new(root.join("data"), root.join("downloads"), name, "test");
        cfg.port = 0;
        cfg.discovery_port = None;
        TestNode { node: Node::start(cfg).await.expect("node starts"), root: root.to_path_buf() }
    }

    fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.node.local_addr().unwrap().port())
    }

    fn id(&self) -> String {
        self.node.my_info().device_id
    }

    fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }
}

/// Pairs `b` with `a` using `a`'s password.
async fn pair(a: &TestNode, b: &TestNode) {
    b.node.pair(&a.id(), Some(&a.addr()), &a.node.password()).await.expect("pairing succeeds");
}

async fn wait_finished(rx: &mut Receiver<Event>, id: &str) -> (TransferStatus, Option<String>) {
    tokio::time::timeout(WAIT, async {
        loop {
            match rx.recv().await {
                Ok(Event::TransferFinished { transfer_id, status, error, .. }) if transfer_id == id => return (status, error),
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("event channel closed: {e}"),
            }
        }
    })
    .await
    .expect("transfer finishes in time")
}

fn blake3_file(p: &Path) -> blake3::Hash {
    blake3::hash(&std::fs::read(p).unwrap())
}

/// Deterministic pseudo-random content so compression cannot cheat.
fn write_noise(path: &Path, len: usize, seed: u64) {
    let mut x = seed | 1;
    let mut data = Vec::with_capacity(len);
    while data.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        data.extend_from_slice(&x.to_le_bytes());
    }
    data.truncate(len);
    std::fs::write(path, data).unwrap();
}

fn assert_same_tree(a: &Path, b: &Path) {
    let mut left: Vec<_> = walk(a);
    let mut right: Vec<_> = walk(b);
    left.sort_by(|x, y| x.0.cmp(&y.0));
    right.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(left.iter().map(|x| &x.0).collect::<Vec<_>>(), right.iter().map(|x| &x.0).collect::<Vec<_>>());
    assert_eq!(left, right);
}

fn walk(root: &Path) -> Vec<(String, Option<blake3::Hash>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if p.is_dir() {
                out.push((rel, None));
                stack.push(p);
            } else {
                out.push((rel, Some(blake3_file(&p))));
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_once_then_send_folders_both_ways() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "Laptop A").await;
    let b = TestNode::start(&tmp.path().join("b"), "Phone B").await;
    pair(&a, &b).await;

    // Both sides now trust each other.
    assert_eq!(a.node.devices().unwrap().len(), 1);
    assert_eq!(b.node.devices().unwrap()[0].device.name, "Laptop A");
    let fp_a = b.node.devices().unwrap()[0].device.fingerprint;
    let fp_b = a.node.devices().unwrap()[0].device.fingerprint;
    assert_eq!(fp_a, a.node.fingerprint());

    // B -> A: nested folder with an empty subfolder and a larger file.
    let src = tmp.path().join("src/Proyek");
    std::fs::create_dir_all(src.join("sub/deeper")).unwrap();
    std::fs::create_dir_all(src.join("kosong")).unwrap();
    std::fs::write(src.join("readme.txt"), b"halo").unwrap();
    std::fs::write(src.join("sub/empty.bin"), b"").unwrap();
    write_noise(&src.join("sub/deeper/big.bin"), 5 * 1024 * 1024 + 123, 7);

    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&fp_a, vec![src.clone()]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await, (TransferStatus::Completed, None));
    assert_same_tree(&src, &a.downloads().join("Proyek"));

    // A -> B without pairing again.
    let f = tmp.path().join("src/foto.jpg");
    write_noise(&f, 300_000, 3);
    let mut rx = a.node.subscribe();
    let id = a.node.send_paths(&fp_b, vec![f.clone()]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await.0, TransferStatus::Completed);
    assert_eq!(blake3_file(&b.downloads().join("foto.jpg")), blake3_file(&f));

    // Sending the same folder again must not overwrite: it lands in "Proyek (1)".
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&fp_a, vec![src.clone()]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await.0, TransferStatus::Completed);
    assert_same_tree(&src, &a.downloads().join("Proyek (1)"));

    let h = a.node.history(10).unwrap();
    let transfers: Vec<_> = h.iter().filter(|e| e.direction != "pair").collect();
    assert_eq!(transfers.len(), 3, "{h:?}");
    assert_eq!(h.iter().filter(|e| e.direction == "pair").count(), 1, "{h:?}");
    assert!(h.iter().all(|e| e.status == "completed"), "{h:?}");
    // No temporary files left behind.
    assert!(walk(&a.downloads()).iter().all(|(p, _)| !p.ends_with(".vegamshare-part")));
}

#[tokio::test(flavor = "multi_thread")]
async fn many_small_files() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;

    let src = tmp.path().join("src/banyak");
    for d in 0..30 {
        let dir = src.join(format!("dir{d:02}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..100 {
            std::fs::write(dir.join(format!("f{f:03}.txt")), format!("{d}-{f}").repeat(f + 1)).unwrap();
        }
    }
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&a.node.fingerprint(), vec![src.clone()]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await, (TransferStatus::Completed, None));
    assert_same_tree(&src, &a.downloads().join("banyak"));
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_password_is_rejected_and_rate_limited() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;

    for _ in 0..5 {
        let r = b.node.pair(&a.id(), Some(&a.addr()), "salah123").await;
        assert!(matches!(r, Err(Error::WrongPassword)), "{r:?}");
    }
    // Even the right password is refused while locked out.
    let r = b.node.pair(&a.id(), Some(&a.addr()), &a.node.password()).await;
    assert!(matches!(r, Err(Error::LockedOut(s)) if s > 0), "{r:?}");
    assert!(a.node.devices().unwrap().is_empty());
    assert!(b.node.devices().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_checks_the_device_id() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    let wrong_id = if a.id() == "123456789" { "987654321" } else { "123456789" };
    let r = b.node.pair(wrong_id, Some(&a.addr()), &a.node.password()).await;
    assert!(matches!(r, Err(Error::InvalidArgument(_))), "{r:?}");
    assert!(b.node.devices().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn impostor_at_known_address_is_not_trusted() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;
    let fp_a = a.node.fingerprint();
    let port = a.node.local_addr().unwrap().port();

    // A goes away; a different device takes over its address and port.
    a.node.shutdown().await;
    drop(a);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut cfg = Config::new(tmp.path().join("evil/data"), tmp.path().join("evil/dl"), "A", "test");
    cfg.port = port;
    cfg.discovery_port = None;
    let evil = Node::start(cfg).await.unwrap();
    assert_eq!(evil.local_addr().unwrap().port(), port);

    // B must refuse to talk to it as A. It is reported as "identity
    // changed" (indistinguishable from a reinstall); the only way forward
    // is a fresh pairing, which the impostor cannot pass without the code,
    // QR or password.
    assert!(matches!(b.node.ping(&fp_a).await, Err(Error::IdentityChanged { .. })));
    let f = tmp.path().join("secret.txt");
    std::fs::write(&f, b"rahasia").unwrap();
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&fp_a, vec![f]).await.unwrap();
    let (status, _) = wait_finished(&mut rx, &id).await;
    assert_eq!(status, TransferStatus::Failed);
    assert!(!tmp.path().join("evil/dl/secret.txt").exists());
    assert!(evil.devices().unwrap().is_empty(), "the impostor trusts nobody");
}

#[tokio::test(flavor = "multi_thread")]
async fn forgotten_device_cannot_send() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;
    let fp_b: Fingerprint = b.node.fingerprint();
    a.node.forget_device(&fp_b).unwrap();

    let f = tmp.path().join("x.txt");
    std::fs::write(&f, b"x").unwrap();
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&a.node.fingerprint(), vec![f]).await.unwrap();
    let (status, err) = wait_finished(&mut rx, &id).await;
    assert_eq!(status, TransferStatus::Failed, "{err:?}");
    assert!(!a.downloads().join("x.txt").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn receiver_can_decline() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;
    a.node.set_auto_accept(&b.node.fingerprint(), false).unwrap();

    let mut arx = a.node.subscribe();
    let decider = {
        let node = a.node.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(Event::IncomingOffer { transfer_id, file_count, from_name, .. }) = arx.recv().await {
                    assert_eq!(file_count, 1);
                    assert_eq!(from_name, "B");
                    node.respond_to_offer(&transfer_id, false).unwrap();
                    return;
                }
            }
        })
    };
    let f = tmp.path().join("tidak.txt");
    std::fs::write(&f, b"no").unwrap();
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&a.node.fingerprint(), vec![f]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await.0, TransferStatus::Rejected);
    decider.await.unwrap();
    assert!(!a.downloads().join("tidak.txt").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_restart_and_resume() {
    let tmp = tmpdir();
    let a_root = tmp.path().join("a");
    let a = TestNode::start(&a_root, "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;
    let fp_a = a.node.fingerprint();

    let f = tmp.path().join("film.mkv");
    write_noise(&f, 400 * 1024 * 1024, 11);

    // Pause from the sender as soon as bytes flow.
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&fp_a, vec![f.clone()]).await.unwrap();
    loop {
        match tokio::time::timeout(WAIT, rx.recv()).await.unwrap().unwrap() {
            Event::TransferProgress { transfer, .. } if transfer.transfer_id == id && transfer.bytes_done > 0 => break,
            Event::TransferFinished { transfer_id, status, .. } if transfer_id == id => panic!("finished early: {status:?}"),
            _ => {}
        }
    }
    b.node.pause(&id).unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await.0, TransferStatus::Paused);

    let part = a.downloads().join("film.mkv.vegamshare-part");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let partial = std::fs::metadata(&part).expect("partial file kept").len();
    assert!(partial > 0 && partial < 400 * 1024 * 1024, "partial = {partial}");

    // Receiver restarts (new port), then asks the sender to continue.
    a.node.shutdown().await;
    drop(a);
    let a = TestNode::start(&a_root, "A").await;
    assert_eq!(a.node.fingerprint(), fp_a, "identity survives restart");

    let mut arx = a.node.subscribe();
    a.node.resume(&id).await.unwrap();
    let resumed = tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(Event::TransferStarted { transfer, resumed }) = arx.recv().await
                && transfer.transfer_id == id
            {
                return (resumed, transfer.bytes_done);
            }
        }
    })
    .await
    .unwrap();
    assert!(resumed.0, "receiver recognised the resumed transfer");
    assert!(resumed.1 >= partial, "resumed from {} (had {partial})", resumed.1);

    assert_eq!(wait_finished(&mut arx, &id).await, (TransferStatus::Completed, None));
    assert_eq!(blake3_file(&a.downloads().join("film.mkv")), blake3_file(&f));
    assert!(!part.exists());
}

// ---- silent ("standby") discovery ---------------------------------------

async fn start_with_discovery(root: &Path, name: &str, discovery_port: u16) -> TestNode {
    let mut cfg = Config::new(root.join("data"), root.join("downloads"), name, "test");
    cfg.port = 0;
    cfg.discovery_port = Some(discovery_port);
    TestNode { node: Node::start(cfg).await.expect("node starts"), root: root.to_path_buf() }
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_by_id_while_silent_and_find_after_address_change() {
    let tmp = tmpdir();
    let port = 47971;
    let a_root = tmp.path().join("a");
    let a = start_with_discovery(&a_root, "A", port).await;
    let b = start_with_discovery(&tmp.path().join("b"), "B", port).await;

    // Nobody announces: B sees no nearby devices, yet pairing by ID works.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(b.node.nearby().unwrap().is_empty(), "silent devices must not be listed");
    b.node.pair(&a.id(), None, &a.node.password()).await.expect("pair by ID only");
    let fp_a = a.node.fingerprint();

    // A restarts on a different port (like a new IP from DHCP).
    let old_port = a.node.local_addr().unwrap().port();
    a.node.shutdown().await;
    drop(a);
    let a = start_with_discovery(&a_root, "A", port).await;
    assert_ne!(a.node.local_addr().unwrap().port(), old_port);

    // B knocks at the stale address, then finds A by its ID and sends.
    let f = tmp.path().join("pindah.txt");
    std::fs::write(&f, b"alamat baru").unwrap();
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&fp_a, vec![f]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await, (TransferStatus::Completed, None));
    assert_eq!(std::fs::read(a.downloads().join("pindah.txt")).unwrap(), b"alamat baru");
}

#[tokio::test(flavor = "multi_thread")]
async fn not_receiving_means_absent() {
    let tmp = tmpdir();
    let port = 47972;
    let a = start_with_discovery(&tmp.path().join("a"), "A", port).await;
    let b = start_with_discovery(&tmp.path().join("b"), "B", port).await;
    b.node.pair(&a.id(), None, &a.node.password()).await.unwrap();

    a.node.set_receiving(false).unwrap();
    assert!(!a.node.receiving());
    assert!(b.node.ping(&a.node.fingerprint()).await.is_err(), "must not answer when off");
    let c = start_with_discovery(&tmp.path().join("c"), "C", port).await;
    let r = c.node.pair(&a.id(), None, &a.node.password()).await;
    assert!(matches!(r, Err(Error::NotFound)), "{r:?}");

    a.node.set_receiving(true).unwrap();
    b.node.ping(&a.node.fingerprint()).await.expect("reachable again");
}

#[tokio::test(flavor = "multi_thread")]
async fn visible_only_while_add_device_is_open() {
    let tmp = tmpdir();
    let port = 47973;
    let a = start_with_discovery(&tmp.path().join("a"), "A", port).await;
    let b = start_with_discovery(&tmp.path().join("b"), "B", port).await;
    let listed = |b: &TestNode, id: &str| b.node.nearby().unwrap().iter().any(|n| n.device_id == id);

    a.node.set_pairing_mode(true);
    b.node.set_pairing_mode(true); // B's screen asks everyone visible to introduce themselves
    let seen = tokio::time::timeout(Duration::from_secs(15), async {
        while !listed(&b, &a.id()) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await;
    assert!(seen.is_ok(), "A should appear while its Add device screen is open");

    // Leaving the screen makes A silent again; B forgets it within the expiry.
    a.node.set_pairing_mode(false);
    b.node.set_pairing_mode(false);
    tokio::time::sleep(Duration::from_secs(1)).await;
    b.node.set_pairing_mode(true);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!listed(&b, &a.id()), "A must not be listed after closing its screen");
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_is_recorded_in_history() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;
    for (node, other) in [(&a, "B"), (&b, "A")] {
        let h = node.node.history(10).unwrap();
        assert!(h.iter().any(|e| e.direction == "pair" && e.peer_name == other && e.status == "completed"), "{h:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reinstalled_device_is_reported_not_just_offline() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "Phone").await;
    pair(&a, &b).await;
    let fp_b = b.node.fingerprint();
    let port = b.node.local_addr().unwrap().port();

    // The phone's app is reinstalled: same address, brand-new identity.
    b.node.shutdown().await;
    drop(b);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut cfg = Config::new(tmp.path().join("b2/data"), tmp.path().join("b2/dl"), "Phone", "test");
    cfg.port = port;
    cfg.discovery_port = None;
    let reinstalled = Node::start(cfg).await.unwrap();
    let new_id = reinstalled.my_info().device_id;

    match a.node.ping(&fp_b).await {
        Err(Error::IdentityChanged { device_id, name }) => {
            assert_eq!(device_id, new_id);
            assert_eq!(name, "Phone");
        }
        other => panic!("expected IdentityChanged, got {other:?}"),
    }

    // A send fails for good (nothing to resume) and says why.
    let f = tmp.path().join("x.txt");
    std::fs::write(&f, b"x").unwrap();
    let mut rx = a.node.subscribe();
    let id = a.node.send_paths(&fp_b, vec![f]).await.unwrap();
    let kind = tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(Event::TransferFinished { transfer_id, status, error_kind, .. }) = rx.recv().await
                && transfer_id == id
            {
                return (status, error_kind);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(kind, (TransferStatus::Failed, Some("identity_changed".to_string())));
    assert!(!tmp.path().join("b2/dl/x.txt").exists(), "nothing is sent to the new identity");

    // Truly offline (nobody at the address) stays a plain "unreachable".
    reinstalled.shutdown().await;
    drop(reinstalled);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(matches!(a.node.ping(&fp_b).await, Err(Error::Unreachable)));
}

// ---- password-free pairing ----------------------------------------------

async fn next_pair_request(rx: &mut Receiver<Event>) -> (String, String) {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(Event::PairRequest { request_id, code, .. }) = rx.recv().await {
                return (request_id, code);
            }
        }
    })
    .await
    .expect("pair request arrives")
}

#[tokio::test(flavor = "multi_thread")]
async fn tap_to_pair_with_matching_code() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    a.node.set_pairing_mode(true);
    let mut arx = a.node.subscribe();

    let (attempt, code_b) = b.node.pair_nearby_start(&a.id(), Some(&a.addr())).await.unwrap();
    let (request, code_a) = next_pair_request(&mut arx).await;
    assert_eq!(code_a, code_b, "both screens show the same code");
    assert_eq!(code_a.len(), 6);
    a.node.respond_to_pair_request(&request, true).unwrap();
    let d = b.node.pair_nearby_finish(&attempt).await.unwrap();
    assert_eq!(d.fingerprint, a.node.fingerprint());
    assert_eq!(a.node.devices().unwrap()[0].device.fingerprint, b.node.fingerprint());

    // Saved: sending works with no further pairing.
    let f = tmp.path().join("ketuk.txt");
    std::fs::write(&f, b"tanpa password").unwrap();
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&a.node.fingerprint(), vec![f]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await.0, TransferStatus::Completed);
}

#[tokio::test(flavor = "multi_thread")]
async fn tap_to_pair_declined_or_closed() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;

    // Not on the "Add device" screen: refused without asking anyone.
    let (attempt, _) = b.node.pair_nearby_start(&a.id(), Some(&a.addr())).await.unwrap();
    let r = b.node.pair_nearby_finish(&attempt).await;
    assert!(matches!(r, Err(Error::PairingClosed)), "{r:?}");

    a.node.set_pairing_mode(true);
    let mut arx = a.node.subscribe();
    let (attempt, _) = b.node.pair_nearby_start(&a.id(), Some(&a.addr())).await.unwrap();
    let (request, _) = next_pair_request(&mut arx).await;
    a.node.respond_to_pair_request(&request, false).unwrap();
    let r = b.node.pair_nearby_finish(&attempt).await;
    assert!(matches!(r, Err(Error::Rejected)), "{r:?}");
    assert!(a.node.devices().unwrap().is_empty());
    assert!(b.node.devices().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn qr_pairing_is_single_use_and_pinned() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    let c = TestNode::start(&tmp.path().join("c"), "C").await;

    let qr = a.node.pair_qr_code();
    let d = b.node.pair_with_qr(&qr).await.expect("scan pairs");
    assert_eq!(d.fingerprint, a.node.fingerprint());
    assert_eq!(a.node.devices().unwrap().len(), 1);

    // A photo of the same QR is useless afterwards.
    let r = c.node.pair_with_qr(&qr).await;
    assert!(matches!(r, Err(Error::InvalidCode)), "{r:?}");

    // A QR pointing at A's address but with someone else's key is refused.
    let forged = a.node.pair_qr_code().replace(&a.node.fingerprint().to_string(), &c.node.fingerprint().to_string());
    let r = b.node.pair_with_qr(&forged).await;
    assert!(r.is_err(), "{r:?}");
    assert_eq!(a.node.devices().unwrap().len(), 1, "C never got trusted");
}

#[tokio::test(flavor = "multi_thread")]
async fn another_device_at_the_old_address_is_not_a_reinstall() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "Phone").await;
    let c = TestNode::start(&tmp.path().join("c"), "Laptop").await;
    pair(&a, &b).await;
    let fp_b = b.node.fingerprint();
    let port = b.node.local_addr().unwrap().port();
    b.node.shutdown().await;
    drop(b);
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Someone else with a different name now sits at B's old address.
    let mut cfg = Config::new(tmp.path().join("x/data"), tmp.path().join("x/dl"), "Tablet Tetangga", "test");
    cfg.port = port;
    cfg.discovery_port = None;
    let stranger = Node::start(cfg).await.unwrap();
    assert!(matches!(a.node.ping(&fp_b).await, Err(Error::Unreachable)), "different name: just unreachable");
    stranger.shutdown().await;
    drop(stranger);
    tokio::time::sleep(Duration::from_millis(300)).await;

    // One of A's own paired devices got that address (same name even).
    let mut cfg = Config::new(tmp.path().join("y/data"), tmp.path().join("y/dl"), "Phone", "test");
    cfg.port = port;
    cfg.discovery_port = None;
    let other = Node::start(cfg).await.unwrap();
    other.pair(&a.id(), Some(&a.addr()), &a.node.password()).await.unwrap();
    let r = a.node.ping(&fp_b).await;
    assert!(matches!(r, Err(Error::Unreachable)), "a paired device is never a reinstall: {r:?}");
    drop(c);
}

#[tokio::test(flavor = "multi_thread")]
async fn forgetting_a_device_cancels_its_unfinished_transfers() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;
    let fp_b = b.node.fingerprint();
    b.node.shutdown().await;
    drop(b);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let f = tmp.path().join("nanti.txt");
    std::fs::write(&f, b"later").unwrap();
    let mut rx = a.node.subscribe();
    let id = a.node.send_paths(&fp_b, vec![f]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await.0, TransferStatus::Interrupted);

    a.node.forget_device(&fp_b).unwrap();
    let h = a.node.history(10).unwrap();
    let entry = h.iter().find(|e| e.transfer_id == id).unwrap();
    assert_eq!(entry.status, "cancelled", "no longer offered as resumable");
    assert!(matches!(a.node.resume(&id).await, Err(Error::UnknownTransfer)));
}

// ---- 0.2.0: network changes and dead addresses ---------------------------

#[tokio::test(flavor = "multi_thread")]
async fn keeps_working_after_switching_network_socket() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    pair(&a, &b).await;
    let old = b.node.local_addr().unwrap();

    // B "joins a hotspot": its socket is re-created.
    b.node.rebind_network().unwrap();
    let new = b.node.local_addr().unwrap();
    assert_ne!(new.port(), 0);

    // B can still send...
    let f = tmp.path().join("dari-b.txt");
    std::fs::write(&f, b"setelah pindah jaringan").unwrap();
    let mut rx = b.node.subscribe();
    let id = b.node.send_paths(&a.node.fingerprint(), vec![f]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await, (TransferStatus::Completed, None));

    // ...and A learned B's current address from that contact, so A can reach B.
    let g = tmp.path().join("dari-a.txt");
    std::fs::write(&g, b"balasan").unwrap();
    let mut rx = a.node.subscribe();
    let id = a.node.send_paths(&b.node.fingerprint(), vec![g]).await.unwrap();
    assert_eq!(wait_finished(&mut rx, &id).await.0, TransferStatus::Completed, "old {old}, new {new}");
    assert_eq!(std::fs::read(b.downloads().join("dari-a.txt")).unwrap(), b"balasan");
}

#[tokio::test(flavor = "multi_thread")]
async fn qr_with_dead_addresses_still_pairs_quickly() {
    let tmp = tmpdir();
    let a = TestNode::start(&tmp.path().join("a"), "A").await;
    let b = TestNode::start(&tmp.path().join("b"), "B").await;
    // Prepend addresses nobody answers on (like idle 169.254.x.x adapters).
    let qr = a.node.pair_qr_code().replace("&a=", &format!("&a=127.0.0.1:9,127.0.0.1:19,169.254.10.10:47901,{},", a.addr()));
    let started = std::time::Instant::now();
    b.node.pair_with_qr(&qr).await.expect("pairs despite dead addresses");
    assert!(started.elapsed() < Duration::from_secs(3), "took {:?}", started.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn id_lookup_still_works_after_switching_network() {
    let tmp = tmpdir();
    let port = 47974;
    let a = start_with_discovery(&tmp.path().join("a"), "A", port).await;
    let b = start_with_discovery(&tmp.path().join("b"), "B", port).await;
    // A switches network: both its QUIC and discovery sockets are recreated.
    a.node.rebind_network().unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // B finds A by ID alone (through A's new discovery socket and new port).
    b.node.pair(&a.id(), None, &a.node.password()).await.expect("found by ID after rebind");
}
