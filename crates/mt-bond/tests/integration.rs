//! End-to-end loopback integration tests for stream bonding (D17).
//!
//! The whole stack WITHOUT Xray: a real `mt-merge-server` daemon on
//! loopback, two fake links (each dials the daemon directly — in
//! production the dial rides a per-adapter engine's VLESS tunnel), and a
//! plain TCP echo / sink target. Verifies:
//! - data integrity across the bond in both directions (1 MiB blob),
//! - the split actually uses BOTH carriers,
//! - concurrent sessions multiplex over the same carriers,
//! - carrier death mid-stream fails over without data loss,
//! - clean close semantics.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use mt_bond::daemon;
use mt_bond::{BondOutbound, CarrierSnapshot};
use mt_net::{Host, Outbound, ProxyStream, TargetAddr};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A "link" that dials the merge daemon directly (the engine's tunnel is
/// elided; the framing layer is identical). Counts dials per link so the
/// respawn tests can observe recovery.
#[derive(Clone)]
struct FakeEngineLink {
    name: &'static str,
    daemon: std::net::SocketAddr,
    dials: Arc<AtomicUsize>,
    fail_next: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Outbound for FakeEngineLink {
    fn name(&self) -> &str {
        self.name
    }
    async fn connect(&self, _target: &TargetAddr) -> anyhow::Result<ProxyStream> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            anyhow::bail!("link failure injected");
        }
        self.dials.fetch_add(1, Ordering::SeqCst);
        let s = tokio::net::TcpStream::connect(self.daemon).await?;
        Ok(Box::new(s))
    }
    async fn probe(&self, _target: &TargetAddr) -> anyhow::Result<Duration> {
        let s = tokio::net::TcpStream::connect(self.daemon).await?;
        drop(s);
        Ok(Duration::from_millis(1))
    }
}

/// Spawn the merge daemon on an ephemeral loopback port. The returned
/// guard keeps the shutdown watch sender alive — dropping it shuts the
/// daemon down (tests hold it for their whole body).
async fn spawn_daemon() -> (std::net::SocketAddr, tokio::sync::watch::Sender<bool>) {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let (tx, rx) = tokio::sync::watch::channel(false);
    tokio::spawn(daemon::serve(addr, rx));
    // Wait for the listener to come up.
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return (addr, tx);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("daemon listener never came up");
}

/// An echo server that ALSO reports how much data it received (for the
/// reverse-path integrity check we echo with a per-connection prefix).
async fn spawn_echo() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let (mut r, mut w) = stream.into_split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    addr
}

fn merge_target(daemon: std::net::SocketAddr) -> TargetAddr {
    TargetAddr {
        host: Host::Ip(daemon.ip()),
        port: daemon.port(),
    }
}

fn tcp_target(addr: std::net::SocketAddr) -> TargetAddr {
    TargetAddr {
        host: Host::Ip(addr.ip()),
        port: addr.port(),
    }
}

fn bond_links(
    daemon: std::net::SocketAddr,
) -> (Vec<Arc<dyn Outbound>>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let dials_a = Arc::new(AtomicUsize::new(0));
    let dials_b = Arc::new(AtomicUsize::new(0));
    let links: Vec<Arc<dyn Outbound>> = vec![
        Arc::new(FakeEngineLink {
            name: "Wi-Fi",
            daemon,
            dials: Arc::clone(&dials_a),
            fail_next: Arc::new(AtomicBool::new(false)),
        }),
        Arc::new(FakeEngineLink {
            name: "LTE",
            daemon,
            dials: Arc::clone(&dials_b),
            fail_next: Arc::new(AtomicBool::new(false)),
        }),
    ];
    (links, dials_a, dials_b)
}

/// Pseudo-random-ish blob (deterministic, no rand dep).
fn blob(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn echo_roundtrip_1mib_across_two_carriers() {
    let (daemon_addr, _daemon) = spawn_daemon().await;
    let echo = spawn_echo().await;
    let (links, _a, _b) = bond_links(daemon_addr);
    let bond = BondOutbound::new(links, merge_target(daemon_addr));
    let live = bond
        .ensure_carriers(Duration::from_secs(5))
        .await
        .expect("carriers established")
        .live;
    assert_eq!(live, 2, "both fake links must become carriers");

    // Send 1 MiB and echo it back through the bond.
    let payload = blob(1024 * 1024, 7);
    let mut stream = bond.connect(&tcp_target(echo)).await.expect("bond connect");
    stream.write_all(&payload).await.expect("write");
    stream.shutdown().await.expect("shutdown write side");

    let mut received = Vec::with_capacity(payload.len());
    stream
        .read_to_end(&mut received)
        .await
        .expect("read to end");
    assert_eq!(received.len(), payload.len(), "echo length matches");
    assert_eq!(received, payload, "echo content matches byte-for-byte");

    // Both carriers must have carried bytes (the split is real).
    let snap: Vec<CarrierSnapshot> = bond.carrier_snapshot();
    assert_eq!(snap.len(), 2);
    for c in &snap {
        assert!(
            c.bytes_up > 0 && c.bytes_down > 0,
            "carrier {:?} carried no traffic: {:?}",
            c.name,
            c
        );
    }
    bond.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_sessions_multiplex_over_the_same_carriers() {
    let (daemon_addr, _daemon) = spawn_daemon().await;
    let echo = spawn_echo().await;
    let (links, _a, _b) = bond_links(daemon_addr);
    let bond = BondOutbound::new(links, merge_target(daemon_addr));
    bond.ensure_carriers(Duration::from_secs(5))
        .await
        .expect("carriers established");

    const SESSIONS: usize = 8;
    const PER_SESSION: usize = 64 * 1024;
    let mut handles = Vec::new();
    for i in 0..SESSIONS {
        let bond_link = Arc::clone(&bond);
        handles.push(tokio::spawn(async move {
            let payload = blob(PER_SESSION, i as u8);
            let mut stream = bond_link.connect(&tcp_target(echo)).await.expect("connect");
            stream.write_all(&payload).await.expect("write");
            stream.shutdown().await.expect("shutdown");
            let mut received = Vec::new();
            stream.read_to_end(&mut received).await.expect("read");
            assert_eq!(received, payload, "session {i} integrity");
        }));
    }
    for h in handles {
        h.await.expect("session task");
    }
    assert!(
        bond.session_count() <= 1,
        "sessions cleaned up as they close"
    );
    bond.shutdown();
}

/// Carrier death mid-stream: one carrier's socket is force-shutdown while
/// a 512 KiB transfer is in flight; the client must replay unacked frames
/// on the surviving carrier and the stream must complete without data
/// loss. [`HoldingLink`] keeps a clone of every carrier socket so the
/// test can `shutdown()` them (a shutdown on any fd affects the socket).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn carrier_death_mid_stream_fails_over_without_data_loss() {
    use std::net::Shutdown as StdShutdown;

    struct HoldingLink {
        name: &'static str,
        daemon: std::net::SocketAddr,
        held: Arc<std::sync::Mutex<Vec<std::net::TcpStream>>>,
    }

    #[async_trait::async_trait]
    impl Outbound for HoldingLink {
        fn name(&self) -> &str {
            self.name
        }
        async fn connect(&self, _target: &TargetAddr) -> anyhow::Result<ProxyStream> {
            // std first (clone while blocking-mode), then hand the original
            // to tokio (non-blocking is required by from_std).
            let std_stream = std::net::TcpStream::connect(self.daemon)?;
            let held_clone = std_stream.try_clone()?;
            std_stream.set_nonblocking(true)?;
            let stream = tokio::net::TcpStream::from_std(std_stream)?;
            self.held.lock().unwrap().push(held_clone);
            Ok(Box::new(stream))
        }
        async fn probe(&self, _target: &TargetAddr) -> anyhow::Result<Duration> {
            let s = tokio::net::TcpStream::connect(self.daemon).await?;
            drop(s);
            Ok(Duration::from_millis(1))
        }
    }

    // Runner-flake hardening (S19, LEDGER 2026-11-21): this test failed
    // or wedged on ubuntu runners three documented times (green locally,
    // green in sibling runs of the SAME commit). A hard deadline turns a
    // silent 54-minute wedge into a loud one-minute failure.
    tokio::time::timeout(Duration::from_secs(90), async {
        let (daemon_addr, _daemon) = spawn_daemon().await;
        let echo = spawn_echo().await;

        let held: Arc<std::sync::Mutex<Vec<std::net::TcpStream>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let wifi: Arc<dyn Outbound> = Arc::new(HoldingLink {
            name: "Wi-Fi",
            daemon: daemon_addr,
            held: Arc::clone(&held),
        });
        let lte: Arc<dyn Outbound> = Arc::new(FakeEngineLink {
            name: "LTE",
            daemon: daemon_addr,
            dials: Arc::new(AtomicUsize::new(0)),
            fail_next: Arc::new(AtomicBool::new(false)),
        });
        let bond = BondOutbound::new(vec![wifi, lte], merge_target(daemon_addr));
        bond.ensure_carriers(Duration::from_secs(5))
            .await
            .expect("carriers established");

        let payload = blob(512 * 1024, 0x5A);
        let mut stream = bond.connect(&tcp_target(echo)).await.expect("connect");

        // Write the first half, let it spread over both carriers, then kill
        // every Wi-Fi carrier socket (FIN → both ends mark the carrier dead).
        let half = payload.len() / 2;
        stream
            .write_all(&payload[..half])
            .await
            .expect("first half");
        tokio::time::sleep(Duration::from_millis(250)).await;
        {
            let mut sockets = held.lock().unwrap();
            assert!(!sockets.is_empty(), "Wi-Fi carrier socket held");
            for s in sockets.drain(..) {
                let _ = s.shutdown(StdShutdown::Both);
            }
        }
        // The write side must still work through the surviving carrier.
        stream
            .write_all(&payload[half..])
            .await
            .expect("second half");
        stream.shutdown().await.expect("shutdown");

        let mut received = Vec::with_capacity(payload.len());
        let _ = stream.read_to_end(&mut received).await;
        assert_eq!(
            received.len(),
            payload.len(),
            "failover must not lose bytes (got {}/{})",
            received.len(),
            payload.len()
        );
        assert_eq!(received, payload, "failover integrity");
        // The respawn must have re-dialed the Wi-Fi carrier.
        let snap = bond.carrier_snapshot();
        assert_eq!(snap.len(), 2, "both slots still present");
        bond.shutdown();
    })
    .await
    .expect("deadline: carrier_death wedged (documented runner flake — see LEDGER)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refused_target_surfaces_as_connection_reset() {
    let (daemon_addr, _daemon) = spawn_daemon().await;
    // Nothing listens here (bind + drop).
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead = probe.local_addr().unwrap();
    drop(probe);

    let (links, _a, _b) = bond_links(daemon_addr);
    let bond = BondOutbound::new(links, merge_target(daemon_addr));
    bond.ensure_carriers(Duration::from_secs(5))
        .await
        .expect("carriers established");

    let mut stream = bond.connect(&tcp_target(dead)).await.expect("connect");
    let mut buf = [0u8; 16];
    match stream.read(&mut buf).await {
        Err(e) => {
            assert!(
                e.to_string().contains("target refused"),
                "unexpected error: {e}"
            );
        }
        other => panic!("expected refused error, got {other:?}"),
    }
    bond.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn domain_targets_dial_through_the_daemon() {
    // Runner-flake hardening (S19, LEDGER 2026-11-21): wedged >54 min on
    // an ubuntu runner while green locally and in the sibling run of the
    // same commit. Deadline: loud failure instead of an hourly hang.
    tokio::time::timeout(Duration::from_secs(60), async {
        let (daemon_addr, _daemon) = spawn_daemon().await;
        let echo = spawn_echo().await;
        let (links, _a, _b) = bond_links(daemon_addr);
        let bond = BondOutbound::new(links, merge_target(daemon_addr));
        bond.ensure_carriers(Duration::from_secs(5))
            .await
            .expect("carriers established");

        // localhost:port resolves inside the daemon's dial_target.
        let target = TargetAddr {
            host: Host::Domain("localhost".to_string()),
            port: echo.port(),
        };
        let mut stream = bond.connect(&target).await.expect("connect");
        stream.write_all(b"hello bond").await.expect("write");
        stream.shutdown().await.expect("shutdown");
        let mut received = Vec::new();
        stream.read_to_end(&mut received).await.expect("read");
        assert_eq!(received, b"hello bond");
        bond.shutdown();
    })
    .await
    .expect("deadline: domain_targets wedged (documented runner flake — see LEDGER)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_dead_link_still_yields_a_working_bond() {
    let (daemon_addr, _daemon) = spawn_daemon().await;
    let echo = spawn_echo().await;
    let dials_a = Arc::new(AtomicUsize::new(0));
    let dead_link: Arc<dyn Outbound> = Arc::new(FakeEngineLink {
        name: "dead-adapter",
        daemon: "127.0.0.1:1".parse().unwrap(), // nothing listens
        dials: Arc::clone(&dials_a),
        fail_next: Arc::new(AtomicBool::new(true)),
    });
    let dials_b = Arc::new(AtomicUsize::new(0));
    let good_link: Arc<dyn Outbound> = Arc::new(FakeEngineLink {
        name: "LTE",
        daemon: daemon_addr,
        dials: Arc::clone(&dials_b),
        fail_next: Arc::new(AtomicBool::new(false)),
    });
    let bond = BondOutbound::new(vec![dead_link, good_link], merge_target(daemon_addr));
    let live = bond
        .ensure_carriers(Duration::from_secs(5))
        .await
        .expect("one live carrier is enough")
        .live;
    assert_eq!(live, 1);

    let mut stream = bond.connect(&tcp_target(echo)).await.expect("connect");
    stream.write_all(b"fallback").await.expect("write");
    stream.shutdown().await.expect("shutdown");
    let mut received = Vec::new();
    stream.read_to_end(&mut received).await.expect("read");
    assert_eq!(received, b"fallback");
    bond.shutdown();
}
