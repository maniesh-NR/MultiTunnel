//! Real-Xray end-to-end stream bonding (S11 field-failure reproduction rig).
//!
//! The S9 stream-bonding integration tests dial the daemon through FAKE
//! links; the production hop — per-adapter Xray engines over VLESS+REALITY
//! into a real Xray server whose freedom outbound dials the merge daemon —
//! was never exercised, and the field report ("per-connection works, stream
//! does not") is exactly that gap. This test wires the FULL chain on
//! loopback:
//!
//! ```text
//! BondOutbound ─┬─ engine "e2e-a" (Xray client, REALITY) ──▶ Xray server ──┐
//!               └─ engine "e2e-b" (Xray client, REALITY) ──▶ (freedom)   ──┤
//!                                                                           ▼
//!                                                     mt_bond::serve (7443) ──▶ echo
//! ```
//!
//! GATED: never runs in CI. Requires:
//!   MT_E2E_XRAY=1
//!   xray binary: XRAY_BIN=/path/to/xray  (or MULTITUNNEL_ENGINE, or
//!                /tmp/mtrepro/xray)
//! plus outbound internet for the REALITY dest (a real external HTTPS site;
//! see the s_server note below).
//!
//! WHY THE DEST IS A REAL SITE: an `openssl s_server` dest makes EVERY
//! REALITY handshake fail server-side ("processed invalid connection:
//! handshake did not complete successfully") — the REALITY server must
//! borrow a TLS server flight that s_server does not produce the way real
//! endpoints do (bisected 2026-11-19; the field uses a real gateway, so
//! the rig must too). With a real dest the handshakes pass immediately.
//!
//! Run: MT_E2E_XRAY=1 XRAY_BIN=/tmp/mtrepro/xray cargo test --test e2e_real_xray -- --nocapture

#![allow(dead_code)]

use std::net::{IpAddr, SocketAddr, TcpListener as StdListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::watch;

use mt_bond::BondOutbound;
use mt_engine::{EngineSpec, ManagedEngine, VlessRealityEndpoint};
use mt_net::{Host, Outbound, TargetAddr};

// ---------------------------------------------------------------------------
// Minimal tracing subscriber (lockfile-free): prints every event with its
// target so mt_bond/mt_engine/mt_net diagnostics are visible in the rig.
// ---------------------------------------------------------------------------
struct PrintSubscriber {
    next: std::sync::atomic::AtomicU64,
}
impl tracing::Subscriber for PrintSubscriber {
    fn enabled(&self, meta: &tracing::Metadata<'_>) -> bool {
        meta.level() <= &tracing::Level::DEBUG
    }
    fn new_span(&self, _attrs: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(
            self.next
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .max(1),
        )
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        struct V(String);
        impl tracing::field::Visit for V {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                use std::fmt::Write as _;
                let _ = write!(self.0, " {}={:?}", field.name(), value);
            }
        }
        let mut v = V(format!("TRACE [{}] ", event.metadata().target()));
        event.record(&mut v);
        eprintln!("{}", v.0);
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}
fn init_tracing() {
    let _ = tracing::subscriber::set_global_default(PrintSubscriber {
        next: std::sync::atomic::AtomicU64::new(1),
    });
}

// ---------------------------------------------------------------------------
// Synthetic, TEST-ONLY credentials (loopback between our own client and
// server; generated once with `xray x25519` / `xray uuid`; never shipped).
// ---------------------------------------------------------------------------
const TEST_UUID: &str = "f3480e91-ef16-4bf2-86a2-d588c87a121a";
const TEST_PRIV_KEY: &str = "MA4XmOqQdd4ruaAQ1-5nrU13tp0D6XyxJFxWk5pJi3U";
const TEST_PUB_KEY: &str = "P03r_H0mRp6hK1fqA71pHpy3xq_CRYP8VUAl2wx1hgE";
const TEST_SHORT_ID: &str = "0123456789abcdef";
/// REALITY dest/SNI: a REAL external HTTPS endpoint (see module docs — an
/// `openssl s_server` dest silently breaks every handshake).
const DEST_HOST: &str = "www.bing.com";
const DEST_PORT: u16 = 443;
const TEST_SNI: &str = DEST_HOST;

const SERVER_PORT: u16 = 48443; // Xray server (VLESS+REALITY)
const MERGE_PORT: u16 = 47443; // mt_bond::serve

fn xray_path() -> PathBuf {
    let from_env = std::env::var_os("XRAY_BIN")
        .or_else(|| std::env::var_os("MULTITUNNEL_ENGINE"))
        .map(PathBuf::from);
    from_env.filter(|p| p.is_file()).unwrap_or_else(|| {
        let fallback = PathBuf::from("/tmp/mtrepro/xray");
        if fallback.is_file() {
            fallback
        } else {
            panic!(
                "no xray binary — set XRAY_BIN=/path/to/xray (or drop one at \
                 /tmp/mtrepro/xray) and set MT_E2E_XRAY=1"
            );
        }
    })
}

fn gate() -> bool {
    std::env::var("MT_E2E_XRAY").is_ok_and(|v| v == "1")
}

/// Bind an ephemeral IPv4 loopback port, close it, return the port (the
/// child then rebinds it; races are unlikely on loopback and tolerated).
fn free_port() -> u16 {
    let l = StdListener::bind(SocketAddr::new(IpAddr::from([127, 0, 0, 1]), 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Wait until `addr` accepts TCP (or fail after the deadline).
async fn wait_port(addr: SocketAddr, deadline: Duration, what: &str) {
    let started = std::time::Instant::now();
    loop {
        if tokio::time::timeout(
            Duration::from_millis(400),
            tokio::net::TcpStream::connect(addr),
        )
        .await
        .is_ok()
        {
            return;
        }
        if started.elapsed() > deadline {
            panic!("{what} never came up on {addr}");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Tiny echo target: accepts connections forever, echoes everything back.
async fn spawn_echo() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                loop {
                    match sock.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => {
                            if sock.write_all(&buf[..n]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    addr
}

/// The REALITY dest must be a healthy real TLS endpoint; the Xray server
/// borrows its certificate during authenticated handshakes. TCP-reachability
/// is the honest gate (TLS borrow failures surface as REALITY invalid).
async fn check_dest() {
    match tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::TcpStream::connect((DEST_HOST, DEST_PORT)),
    )
    .await
    {
        Ok(Ok(_)) => eprintln!("e2e: REALITY dest {DEST_HOST}:{DEST_PORT} reachable"),
        _ => panic!(
            "REALITY dest {DEST_HOST}:{DEST_PORT} unreachable — this rig needs outbound \
             internet (the field borrows a real gateway's TLS the same way)"
        ),
    }
}

/// The Xray SERVER: VLESS+REALITY inbound on SERVER_PORT (dest = the real
/// external site), a single freedom outbound, no routing — byte-for-byte the
/// shape `server/setup-xray-reality.sh` ships, minus the external listen.
fn spawn_xray_server(work: &Path, xray: &PathBuf, allow_ports: &[u16]) -> (SocketAddr, ChildGuard) {
    let config = work.join("xray-server.json");
    let ports_csv = allow_ports
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        r#"{{
  "log": {{ "loglevel": "debug" }},
  "inbounds": [
    {{
      "tag": "vless-in",
      "listen": "127.0.0.1",
      "port": {SERVER_PORT},
      "protocol": "vless",
      "settings": {{
        "clients": [ {{ "id": "{TEST_UUID}", "flow": "xtls-rprx-vision" }} ],
        "decryption": "none"
      }},
      "streamSettings": {{
        "network": "tcp",
        "security": "reality",
        "realitySettings": {{
          "show": false,
          "dest": "{DEST_HOST}:{DEST_PORT}",
          "xver": 0,
          "serverNames": ["{TEST_SNI}"],
          "privateKey": "{TEST_PRIV_KEY}",
          "shortIds": ["{TEST_SHORT_ID}"]
        }}
      }}
    }}
  ],
  "outbounds": [
    {{
      "tag": "direct",
      "protocol": "freedom",
      "settings": {{
        "finalRules": [
          {{
            "action": "allow",
            "network": "tcp",
            "ip": ["127.0.0.1/32"],
            "port": "{ports_csv}"
          }}
        ]
      }}
    }}
  ]
}}
"#
    );
    std::fs::write(&config, body).expect("write server config");
    let log = std::fs::File::create(work.join("xray-server.log")).expect("server log");
    let child = Command::new(xray)
        .args(["run", "-c", config.to_str().unwrap()])
        .stdout(Stdio::from(log.try_clone().expect("clone log")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("xray server spawn");
    (
        SocketAddr::new(IpAddr::from([127, 0, 0, 1]), SERVER_PORT),
        ChildGuard(child),
    )
}

fn engine_spec(tag: &str, server: SocketAddr) -> EngineSpec {
    EngineSpec {
        tag: tag.to_string(),
        endpoint: VlessRealityEndpoint {
            address: server.ip().to_string(),
            port: server.port(),
            uuid: TEST_UUID.to_string(),
            sni: TEST_SNI.to_string(),
            public_key: TEST_PUB_KEY.to_string(),
            short_id: TEST_SHORT_ID.to_string(),
            fingerprint: "chrome".to_string(),
            spider_x: Some("/".to_string()),
        },
        send_through: None,
        log_level: Some("debug".to_string()),
    }
}

/// One managed engine per spec, state/logs under `work`. The locator is
/// pinned to the SAME xray binary the server runs (MULTITUNNEL_ENGINE) so
/// the rig never depends on the download path.
fn build_engines(work: &Path, tags: &[&str], server: SocketAddr) -> Vec<Arc<ManagedEngine>> {
    let xray = xray_path();
    std::env::set_var("MULTITUNNEL_ENGINE", &xray);
    let locator: Arc<dyn mt_engine::EngineLocator> =
        Arc::new(mt_platform::DefaultEngineLocator::new());
    tags.iter()
        .map(|tag| {
            Arc::new(ManagedEngine::new(
                engine_spec(tag, server),
                Arc::clone(&locator),
                work.to_path_buf(),
                work.to_path_buf(),
            )) as Arc<ManagedEngine>
        })
        .collect()
}

fn ip_target(addr: SocketAddr) -> TargetAddr {
    TargetAddr {
        host: Host::Ip(addr.ip()),
        port: addr.port(),
    }
}

// ---------------------------------------------------------------------------
// The E2E test
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stream_bonding_survives_real_xray() {
    if !gate() {
        eprintln!("skipped: set MT_E2E_XRAY=1 (and XRAY_BIN=/path/to/xray) to run");
        return;
    }
    init_tracing();
    let xray = xray_path();
    let work = std::env::temp_dir().join("mt-e2e-real-xray");
    std::fs::create_dir_all(&work).unwrap();

    // 1. Echo target FIRST (its ephemeral port goes into the server's
    //    finalRules allow list), then the REALITY dest gate + Xray server.
    //    NOTE the field semantics: per-connection targets are PUBLIC and
    //    allowed by freedom's default policy; LOOPBACK targets (the merge
    //    daemon, and here the echo) are BLOCKED by Xray's default safety
    //    policy unless explicitly allowed — that is the v0.6.1 field bug.
    let echo = spawn_echo().await;
    eprintln!("e2e: echo target at {echo}");
    check_dest().await;
    let (server_addr, _server_guard) = spawn_xray_server(&work, &xray, &[echo.port(), MERGE_PORT]);
    wait_port(server_addr, Duration::from_secs(15), "xray server").await;
    eprintln!("e2e: xray server on {server_addr}, dest {DEST_HOST}:{DEST_PORT}");

    // 2. Merge daemon (the production library path, in-process).
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let listen: SocketAddr = format!("127.0.0.1:{MERGE_PORT}").parse().unwrap();
    tokio::spawn(mt_bond::serve(listen, shutdown_rx));
    wait_port(listen, Duration::from_secs(5), "merge daemon").await;
    eprintln!("e2e: merge daemon on {listen}");

    // 3. (echo already spawned above so its port entered finalRules.)

    // 3b. CONTROL (per-connection semantics): ONE engine, ONE session, a
    //     REAL target (the echo) — no bond, no daemon. This must ALWAYS
    //     work; it isolates "REALITY chain broken" (field: per-connection
    //     works) from "bond/daemon broken" (field: stream does not).
    {
        let engines = build_engines(&work, &["e2e-ctl"], server_addr);
        let ctl: Arc<dyn Outbound> = Arc::clone(&engines[0]) as Arc<dyn Outbound>;
        let mut s = tokio::time::timeout(Duration::from_secs(30), ctl.connect(&ip_target(echo)))
            .await
            .expect("control: engine connect must not hang")
            .expect("control: engine connect must succeed (per-connection semantics)");
        // NOTE: Xray's SOCKS inbound replies "connected" BEFORE the upstream
        // (REALITY+VLESS+freedom) is established, so an immediate
        // write+shutdown can race establishment. Read the echo FIRST
        // (which gives the tunnel time to come up), then half-close.
        s.write_all(b"control-ping").await.expect("control write");
        let mut buf = [0u8; 12];
        tokio::time::timeout(Duration::from_secs(20), s.read_exact(&mut buf))
            .await
            .expect("control: echo bytes must arrive (20s)")
            .expect("control read_exact");
        assert_eq!(&buf, b"control-ping", "control echo must round-trip");
        s.shutdown().await.expect("control half-close");
        let mut tail = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), s.read_to_end(&mut tail))
            .await
            .expect("control: EOF must arrive")
            .expect("control read_to_end");
        eprintln!("e2e: CONTROL per-connection echo OK");
    }

    // 4. TWO real engines → bond, exactly the production wiring
    //    (real_service's stream branch: BondOutbound over engine links).
    let engines = build_engines(&work, &["e2e-a", "e2e-b"], server_addr);
    let links: Vec<Arc<dyn Outbound>> = engines
        .into_iter()
        .map(|e| e as Arc<dyn Outbound>)
        .collect();
    let bond = BondOutbound::new(links.clone(), ip_target(listen));

    let report = tokio::time::timeout(
        Duration::from_secs(60),
        bond.ensure_carriers(Duration::from_secs(45)),
    )
    .await
    .expect("carrier establishment must not hang")
    .expect("carrier establishment must succeed over real Xray");
    eprintln!(
        "e2e: carriers live={} report={:?}",
        report.live, report.lines
    );
    assert_eq!(
        report.live,
        links.len(),
        "every engine carrier must come up over real Xray: {:?}",
        report.lines
    );

    // 5. Full-duplex bulk transfer through ONE bonded session (OPEN + striped
    //    DATA + daemon dial + reverse merge + half-close + final close).
    eprintln!("e2e: step5 open session…");
    let stream = tokio::time::timeout(Duration::from_secs(20), bond.connect(&ip_target(echo)))
        .await
        .expect("step5: bond.connect must not hang")
        .expect("bond session");
    eprintln!("e2e: step5 session open, transferring…");
    let payload: Vec<u8> = (0..256 * 1024u32).map(|i| (i % 251) as u8).collect();
    let expected = payload.clone();
    let (mut r, mut w) = tokio::io::split(stream);
    let send = tokio::spawn(async move {
        w.write_all(&payload).await.expect("echo write");
        eprintln!("e2e: step5 write_all done, half-closing…");
        // Half-close the write side: the daemon propagates it to the target,
        // the echo EOFs, the daemon final-closes, read_to_end can finish.
        w.shutdown().await.expect("echo half-close");
        eprintln!("e2e: step5 half-close done");
    });
    let mut echoed = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(30), r.read_to_end(&mut echoed)).await;
    send.await.unwrap();
    eprintln!("e2e: step5 read_to_end done, {} bytes", echoed.len());
    assert_eq!(
        echoed.len(),
        expected.len(),
        "echo must return every byte through the real chain"
    );
    assert!(
        echoed == expected,
        "echo payload must be byte-identical (striping/reorder intact)"
    );
    eprintln!("e2e: 256 KiB round trip OK");

    // 6. Many short sessions (the OPEN/dial/close churn path).
    for i in 0..8 {
        let mut s = bond.connect(&ip_target(echo)).await.expect("short session");
        s.write_all(format!("ping-{i}").as_bytes()).await.unwrap();
        s.shutdown().await.unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, format!("ping-{i}").as_bytes(), "short echo {i}");
    }
    eprintln!("e2e: 8 short sessions OK");

    // 7. Probe through the bond (production connect gate).
    let d = tokio::time::timeout(Duration::from_secs(20), bond.probe(&ip_target(echo)))
        .await
        .expect("probe must not hang")
        .expect("probe must succeed through the bond");
    eprintln!("e2e: bond probe round trip {d:?}");

    let _ = shutdown_tx.send(true);
}

/// The daemon-UNREACHABLE path with real engines: ensure_carriers must FAIL
/// cleanly (never hang) so RealService falls back to per-connection — this
/// is the field symptom when mt-merge-server is absent on the tunnel server.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stream_bonding_falls_back_when_daemon_absent_over_real_xray() {
    if !gate() {
        eprintln!("skipped: set MT_E2E_XRAY=1 (and XRAY_BIN=/path/to/xray) to run");
        return;
    }
    init_tracing();
    let xray = xray_path();
    let work = std::env::temp_dir().join("mt-e2e-real-xray-fb");
    std::fs::create_dir_all(&work).unwrap();

    check_dest().await;
    let (server_addr, _server_guard) = spawn_xray_server(&work, &xray, &[]);
    wait_port(server_addr, Duration::from_secs(15), "xray server").await;

    // A merge port with NOTHING listening on the server.
    let dead_merge: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();

    let engines = build_engines(&work, &["e2e-fb"], server_addr);
    let links: Vec<Arc<dyn Outbound>> = engines
        .into_iter()
        .map(|e| e as Arc<dyn Outbound>)
        .collect();
    let bond = BondOutbound::new(links, ip_target(dead_merge));

    let started = std::time::Instant::now();
    let res = tokio::time::timeout(
        Duration::from_secs(90),
        bond.ensure_carriers(Duration::from_secs(60)),
    )
    .await
    .expect("ensure_carriers must return, not hang");
    eprintln!(
        "e2e: absent-daemon ensure_carriers -> {:?} after {:?}",
        res.as_ref().err().map(|e| e.to_string()),
        started.elapsed()
    );
    assert!(
        res.is_err(),
        "with no daemon the carrier pass must FAIL (fallback prerequisite), got live report"
    );
}
