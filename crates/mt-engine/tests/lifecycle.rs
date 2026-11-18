//! Lifecycle tests for `ManagedEngine` (unix-gated; spawn `python3` with
//! the shipped fake engine — NO real xray in tests, per contract).
//!
//! Contract list covered here: spawn → port up → connect succeeds →
//! kill child → auto-respawn → budget exhaustion errors; kill-on-drop;
//! 0600 perms on the written config (unix); config file never appears in
//! logs. Tests skip (pass) when python3 is unavailable so windows CI
//! legs never fail on a missing interpreter.

#![cfg(unix)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use mt_engine::{
    EngineCommand, EngineLocator, EnginePolicy, EngineSpec, ManagedEngine, VlessRealityEndpoint,
};
use mt_net::{Host, Outbound, TargetAddr};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake_engine.py");
const UUID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const PBK: &str = "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";

/// Tiny policy so respawn/backoff tests stay fast (defaults are
/// separately asserted by the pure policy tests).
fn tiny_policy() -> EnginePolicy {
    EnginePolicy {
        max_respawns: 5,
        backoff_base: Duration::from_millis(50),
        backoff_cap: Duration::from_millis(200),
        port_wait: Duration::from_secs(5),
    }
}

fn spec(tag: &str) -> EngineSpec {
    EngineSpec {
        tag: tag.into(),
        endpoint: VlessRealityEndpoint {
            address: "203.0.113.10".into(),
            port: 443,
            uuid: UUID.into(),
            sni: "tunnel.example.org".into(),
            public_key: PBK.into(),
            short_id: "0123abcd".into(),
            fingerprint: "chrome".into(),
            spider_x: None,
        },
        send_through: None,
    }
}

/// Locator that has NOTHING on disk (override path must win in tests).
struct EmptyLocator;

#[async_trait::async_trait]
impl EngineLocator for EmptyLocator {
    fn locate(&self) -> Option<PathBuf> {
        None
    }
    async fn ensure(&self) -> anyhow::Result<PathBuf> {
        Err(anyhow::anyhow!("no engine available (test locator)"))
    }
}

fn fake_command() -> EngineCommand {
    EngineCommand {
        program: "python3".into(),
        args: vec![SCRIPT.to_string()],
    }
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "mt-engine-test-{}-{}-{}",
            name,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::fs::create_dir_all(root.join("logs")).unwrap();
        Self { root }
    }

    fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    fn engine(&self, tag: &str) -> ManagedEngine {
        ManagedEngine::new(spec(tag), Arc::new(EmptyLocator), self.state(), self.logs())
            .with_command(fake_command())
            .with_policy(tiny_policy())
    }

    fn config_path(&self, tag: &str) -> PathBuf {
        self.state().join(format!("engine-{tag}.json"))
    }

    fn log_path(&self, tag: &str) -> PathBuf {
        self.logs().join(format!("engine-{tag}.log"))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if std::env::var("MT_ENGINE_KEEP_SANDBOX").is_ok() {
            eprintln!("keeping sandbox {}", self.root.display());
            return;
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn have_python3() -> bool {
    Command::new("python3")
        .arg("-c")
        .arg("import sys")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Loopback echo server standing in for "the internet".
async fn spawn_echo() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut s = stream;
                let mut buf = [0u8; 1024];
                loop {
                    match s.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if s.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    addr
}

async fn port_reachable(addr: SocketAddr) -> bool {
    tokio::net::TcpStream::connect(addr).await.is_ok()
}

async fn wait_port_free(addr: SocketAddr, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if !port_reachable(addr).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Simulate an unexpected crash: the fake engine exits when the exit
/// file (next to its config) appears; we then remove it so the respawn
/// gets a clean start.
async fn kill_via_exit_file(sandbox: &Sandbox, tag: &str, socks: SocketAddr) {
    let exit_file = PathBuf::from(format!("{}.exit", sandbox.config_path(tag).display()));
    std::fs::write(&exit_file, b"exit").unwrap();
    assert!(
        wait_port_free(socks, Duration::from_secs(5)).await,
        "fake engine did not die after exit file"
    );
    std::fs::remove_file(&exit_file).unwrap();
}

async fn roundtrip(engine: &ManagedEngine, target: &TargetAddr) {
    let mut s = engine.connect(target).await.expect("tunnel connect");
    s.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4];
    s.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");
}

#[tokio::test]
async fn lifecycle_spawn_connect_kill_respawn_budget() {
    if !have_python3() {
        eprintln!("python3 unavailable; skipping lifecycle test");
        return;
    }
    let sandbox = Sandbox::new("lifecycle");
    let engine = sandbox.engine("lc");
    let echo = spawn_echo().await;
    let target = TargetAddr {
        host: Host::Ip(echo.ip()),
        port: echo.port(),
    };

    // Spawn → port up: ensure_alive returns the engine's private loopback
    // SOCKS address and the port answers.
    let socks = engine.ensure_alive().await.expect("first spawn");
    eprintln!("[dbg] spawned at {socks}");
    assert!(socks.ip().is_loopback(), "private socks is loopback");
    assert!(port_reachable(socks).await, "port up after spawn");

    // Connect succeeds and carries bytes; probe measures a real RTT.
    eprintln!("[dbg] roundtrip1 begin");
    roundtrip(&engine, &target).await;
    eprintln!("[dbg] roundtrip1 ok");
    let rtt = engine.probe(&target).await.expect("probe");
    assert!(rtt <= Duration::from_secs(5), "probe RTT sane: {rtt:?}");

    // Kill child → auto-respawn on next connect.
    eprintln!("[dbg] kill1 begin");
    kill_via_exit_file(&sandbox, "lc", socks).await;
    eprintln!("[dbg] kill1 done");
    let socks2 = engine.ensure_alive().await.expect("respawn");
    assert!(
        socks2.ip().is_loopback() && socks2.port() != 0,
        "respawn bound a fresh loopback port"
    );
    eprintln!("[dbg] roundtrip2 begin at {socks2}");
    roundtrip(&engine, &target).await;
    eprintln!("[dbg] roundtrip2 ok");

    // Budget exhaustion: a command that always dies must degrade after
    // `max_respawns` attempts, and later connects fail FAST (degraded).
    let t0 = Instant::now();
    let broken = ManagedEngine::new(
        spec("broken"),
        Arc::new(EmptyLocator),
        sandbox.state(),
        sandbox.logs(),
    )
    .with_command(EngineCommand {
        program: "python3".into(),
        args: vec!["-c".into(), "import sys; sys.exit(3)".into()],
    })
    .with_policy(EnginePolicy {
        max_respawns: 2,
        backoff_base: Duration::from_millis(30),
        backoff_cap: Duration::from_millis(60),
        port_wait: Duration::from_secs(3),
    });
    let err = broken
        .connect(&target)
        .await
        .map(|_| ())
        .expect_err("dying child exhausts the budget");
    assert!(
        err.to_string().contains("degraded"),
        "degraded error: {err}"
    );
    // Second connect: degraded fast-path, no new attempts.
    let err2 = broken
        .connect(&target)
        .await
        .map(|_| ())
        .expect_err("degraded engine keeps failing");
    assert!(
        err2.to_string().contains("degraded"),
        "degraded error: {err2}"
    );
    assert!(
        t0.elapsed() < Duration::from_secs(15),
        "budget exhaustion stays bounded, took {:?}",
        t0.elapsed()
    );

    drop(engine);
}

#[tokio::test]
async fn kill_on_drop_terminates_child() {
    if !have_python3() {
        eprintln!("python3 unavailable; skipping kill-on-drop test");
        return;
    }
    let sandbox = Sandbox::new("killdrop");
    let engine = sandbox.engine("kd");
    let socks = engine.ensure_alive().await.expect("spawn");
    assert!(port_reachable(socks).await, "port up before drop");

    // Kill-on-drop (contract): no exit file, no stop API — the Drop impl
    // must take the child down.
    drop(engine);
    assert!(
        wait_port_free(socks, Duration::from_secs(3)).await,
        "child must not survive the engine drop"
    );
}

#[tokio::test]
async fn config_is_0600_and_never_appears_in_logs() {
    if !have_python3() {
        eprintln!("python3 unavailable; skipping perms/logs test");
        return;
    }
    let sandbox = Sandbox::new("perms");
    let engine = sandbox.engine("perm");
    let _socks = engine.ensure_alive().await.expect("spawn");

    // 0600 on the config that embeds the UUID (unix contract gate).
    let cfg_path = sandbox.config_path("perm");
    let meta = std::fs::metadata(&cfg_path).expect("config written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            meta.permissions().mode() & 0o777,
            0o600,
            "config must be 0600"
        );
    }

    // The config file (path or contents) never appears in the log.
    let log = std::fs::read_to_string(sandbox.log_path("perm")).expect("log written");
    assert!(log.contains("spawn engine tag=perm"), "header present");
    assert!(
        !log.contains(&*cfg_path.file_name().unwrap().to_string_lossy()),
        "config file name leaked into the log"
    );
    assert!(!log.contains(&cfg_path.display().to_string()));
    assert!(!log.contains(UUID), "uuid leaked into the log");
    assert!(!log.contains(PBK), "public key leaked into the log");

    // Log dir holds the engine log; state dir holds the config — and the
    // log never grows a stray copy of the config either.
    let logs_dir = std::fs::read_dir(sandbox.logs()).unwrap();
    for entry in logs_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(name.ends_with(".log"), "unexpected file in log dir: {name}");
    }

    drop(engine);
}
