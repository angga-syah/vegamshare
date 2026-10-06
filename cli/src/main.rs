//! Command-line front end for testing VegamShare between real machines
//! before (or without) the graphical app.
//!
//!     vegamshare-cli [--data DIR] [--downloads DIR] [--name NAME] [--port N] [--no-discovery]
//!
//! Then type `help`.

use std::path::PathBuf;

use vegamshare_core::{Config, Event, Fingerprint, Node};
use tokio::io::{AsyncBufReadExt, BufReader};

const HELP: &str = "\
commands:
  info                         this device's ID, password and addresses
  devices                      paired devices (# = index for other commands)
  nearby                       devices seen on the network
  pair <id> <password> [ip]    pair once with another device
  pairmode on|off              be visible and accept tap-to-pair (2 min)
  pairnear <id> [ip]           tap-to-pair: shows a code to compare
  acceptpair <request>         accept a tap-to-pair request (rejectpair to refuse)
  qr                           print a single-use pairing QR payload
  pairqr <payload>             pair using another device's QR payload
  send <#> <path> [path...]    send files/folders to paired device #
  accept <transfer> | reject <transfer>
  pause <transfer> | resume <transfer> | cancel <transfer>
  transfers                    active transfers
  history                      recent transfers
  password [new]               show or change this device's password
  autoaccept <#> on|off        receive from device # without asking
  forget <#>                   remove a paired device
  quit";

struct Args {
    data: PathBuf,
    downloads: PathBuf,
    name: String,
    port: u16,
    discovery: bool,
}

fn parse_args() -> Result<Args, String> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default();
    let mut a = Args {
        data: home.join(".vegamshare-cli"),
        downloads: home.join("Downloads").join("VegamShare"),
        name: std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "VegamShare CLI".into()),
        port: vegamshare_core::DEFAULT_PORT,
        discovery: true,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--data" => a.data = val()?.into(),
            "--downloads" => a.downloads = val()?.into(),
            "--name" => a.name = val()?,
            "--port" => a.port = val()?.parse().map_err(|_| "bad --port")?,
            "--no-discovery" => a.discovery = false,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(a)
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[u]) }
}

fn print_event(e: &Event) {
    match e {
        Event::DeviceStatus { fingerprint, online } => {
            println!("* device {} is {}", fingerprint.short(), if *online { "online" } else { "offline" })
        }
        Event::IncomingOffer { transfer_id, from_name, file_count, total_bytes, summary, .. } => println!(
            "* {from_name} wants to send \"{summary}\" ({file_count} files, {}). Type: accept {transfer_id}",
            human(*total_bytes)
        ),
        Event::TransferStarted { transfer, resumed } => println!(
            "* {} {} {} ({} files, {}){}",
            transfer.transfer_id,
            if transfer.direction == vegamshare_core::Direction::Send { "sending to" } else { "receiving from" },
            transfer.peer_name,
            transfer.files_total,
            human(transfer.bytes_total),
            if *resumed { " [resumed]" } else { "" }
        ),
        Event::TransferProgress { transfer, speed_bps } => {
            let pct = if transfer.bytes_total == 0 { 100.0 } else { transfer.bytes_done as f64 * 100.0 / transfer.bytes_total as f64 };
            println!(
                "  {} {pct:5.1}%  {}/{} files  {}/s  {}",
                &transfer.transfer_id[..8],
                transfer.files_done,
                transfer.files_total,
                human(*speed_bps),
                transfer.current
            );
        }
        Event::TransferFinished { transfer_id, status, error, location, .. } => {
            print!("* {transfer_id} finished: {status:?}");
            if let Some(e) = error {
                print!(" ({e})");
            }
            if let Some(l) = location {
                print!(" -> {l}");
            }
            println!();
        }
        Event::PairRequest { request_id, from_name, code, .. } => {
            println!("* {from_name} wants to pair, code {code}. Type: acceptpair {request_id}")
        }
        Event::DevicesChanged | Event::NearbyChanged | Event::OfferClosed { .. } | Event::PairRequestClosed { .. } => {}
    }
}

fn device_at(node: &Node, idx: &str) -> Result<Fingerprint, String> {
    let i: usize = idx.parse().map_err(|_| format!("{idx} is not a device number"))?;
    let devices = node.devices().map_err(|e| e.to_string())?;
    devices.get(i).map(|d| d.device.fingerprint).ok_or_else(|| format!("no device #{i}; see `devices`"))
}

async fn run_command(node: &Node, line: &str) -> Result<bool, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let err = |e: vegamshare_core::Error| e.to_string();
    match words.as_slice() {
        [] => {}
        ["help"] => println!("{HELP}"),
        ["quit"] | ["exit"] => return Ok(false),
        ["info"] => {
            let i = node.my_info();
            println!("name:        {}\nID:          {}\npassword:    {}\nfingerprint: {}\nlistening:   {}", i.name, i.device_id_formatted, node.password(), i.fingerprint, i.addresses.join(", "));
            println!("downloads:   {}", node.download_dir().display());
        }
        ["devices"] => {
            for (i, d) in node.devices().map_err(err)?.iter().enumerate() {
                println!(
                    "#{i} {} [{}] ID {} {} auto-accept={}",
                    d.display_name,
                    if d.online { "online" } else { "offline" },
                    d.device.device_id,
                    d.device.last_addr.as_deref().unwrap_or("-"),
                    d.device.auto_accept
                );
            }
        }
        ["nearby"] => {
            for n in node.nearby().map_err(err)? {
                println!("{} ID {} {} {}{}", n.name, n.device_id, n.platform, n.address, if n.paired { " (paired)" } else { "" });
            }
        }
        ["pair", id, password, rest @ ..] => {
            let d = node.pair(id, rest.first().copied(), password).await.map_err(err)?;
            println!("paired with {} (ID {})", d.display_name(), d.device_id);
        }
        ["pairmode", v @ ("on" | "off")] => node.set_pairing_mode(*v == "on"),
        ["pairnear", id, rest @ ..] => {
            let (attempt, code) = node.pair_nearby_start(id, rest.first().copied()).await.map_err(err)?;
            println!("code {code}: confirm it on the other device...");
            let d = node.pair_nearby_finish(&attempt).await.map_err(err)?;
            println!("paired with {} (ID {})", d.display_name(), d.device_id);
        }
        ["acceptpair", id] => node.respond_to_pair_request(id, true).map_err(err)?,
        ["rejectpair", id] => node.respond_to_pair_request(id, false).map_err(err)?,
        ["qr"] => println!("{}", node.pair_qr_code()),
        ["pairqr", payload] => {
            let d = node.pair_with_qr(payload).await.map_err(err)?;
            println!("paired with {} (ID {})", d.display_name(), d.device_id);
        }
        ["send", idx, paths @ ..] if !paths.is_empty() => {
            let fp = device_at(node, idx)?;
            // Rejoin so paths with spaces work when quoted with "".
            let joined = paths.join(" ");
            let list: Vec<PathBuf> = if joined.contains('"') {
                joined.split('"').map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from).collect()
            } else {
                paths.iter().map(PathBuf::from).collect()
            };
            let id = node.send_paths(&fp, list).await.map_err(err)?;
            println!("transfer {id} queued");
        }
        ["accept", id] => node.respond_to_offer(id, true).map_err(err)?,
        ["reject", id] => node.respond_to_offer(id, false).map_err(err)?,
        ["pause", id] => node.pause(id).map_err(err)?,
        ["cancel", id] => node.cancel(id).map_err(err)?,
        ["resume", id] => node.resume(id).await.map_err(err)?,
        ["transfers"] => {
            for t in node.active_transfers() {
                println!("{} {:?} {} {}/{}", t.transfer_id, t.direction, t.peer_name, human(t.bytes_done), human(t.bytes_total));
            }
        }
        ["history"] => {
            for h in node.history(20).map_err(err)? {
                println!("{} {} {} \"{}\" {} files {} {}", h.transfer_id, h.direction, h.peer_name, h.summary, h.file_count, human(h.total_bytes), h.status);
            }
        }
        ["password"] => println!("{}", node.password()),
        ["password", new] => {
            node.set_password(new).map_err(err)?;
            println!("password changed; already paired devices stay paired");
        }
        ["autoaccept", idx, v @ ("on" | "off")] => node.set_auto_accept(&device_at(node, idx)?, *v == "on").map_err(err)?,
        ["forget", idx] => node.forget_device(&device_at(node, idx)?).map_err(err)?,
        _ => println!("unknown command; type `help`"),
    }
    Ok(true)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "vegamshare_core=info".into()))
        .init();

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}");
            }
            eprintln!("usage: vegamshare-cli [--data DIR] [--downloads DIR] [--name NAME] [--port N] [--no-discovery]");
            std::process::exit(2);
        }
    };
    let platform = if cfg!(windows) { "windows" } else if cfg!(target_os = "android") { "android" } else { std::env::consts::OS };
    let mut cfg = Config::new(&args.data, &args.downloads, &args.name, platform);
    cfg.port = args.port;
    if !args.discovery {
        cfg.discovery_port = None;
    }
    let node = match Node::start(cfg).await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("could not start: {e}");
            std::process::exit(1);
        }
    };
    let _ = run_command(&node, "info").await;
    println!("type `help` for commands");

    let mut events = node.subscribe();
    tokio::spawn(async move {
        while let Ok(e) = events.recv().await {
            print_event(&e);
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        match run_command(&node, line.trim()).await {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => println!("error: {e}"),
        }
    }
    node.shutdown().await;
}
