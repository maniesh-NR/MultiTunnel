//! Shared test scaffolding (compiled only under `cfg(test)`): a loopback
//! echo TCP server and fake outbounds with scripted behavior.
//!
//! Everything here is loopback-only (127.0.0.1 ephemeral ports) and makes
//! no network calls — the contract's no-network / synthetic-vector test
//! rules. `unwrap`/`expect` are fine in test-only code.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::{Outbound, ProxyStream, TargetAddr};

/// Bind an echo server on 127.0.0.1:0 and serve it in the background;
/// returns its address. Every accepted connection echoes bytes until EOF.
pub(crate) async fn spawn_echo() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind echo listener");
    let addr = listener.local_addr().expect("echo local_addr");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let _ = echo_conn(stream).await;
            });
        }
    });
    addr
}

async fn echo_conn(mut s: TcpStream) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    loop {
        let n = s.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        s.write_all(&buf[..n]).await?;
    }
}

/// Scripted behavior for [`FakeOutbound`].
pub(crate) enum FakeMode {
    /// Tunnel to the fixed echo server (ignores the requested target).
    Echo(SocketAddr),
    /// Dial a fixed address (ignores the requested target) — used to put a
    /// real origin server behind a "proxy link".
    Dial(SocketAddr),
    /// `probe` reports healthy but every `connect` fails — forces the
    /// server's transparent retry path deterministically.
    ConnectFail,
    /// Both `connect` and `probe` fail.
    AlwaysFail,
}

/// A scripted [`Outbound`] for aggregator/server tests.
pub(crate) struct FakeOutbound {
    name: String,
    mode: FakeMode,
}

impl FakeOutbound {
    pub(crate) fn echo(name: &str, echo: SocketAddr) -> Self {
        Self {
            name: name.into(),
            mode: FakeMode::Echo(echo),
        }
    }

    pub(crate) fn dial(name: &str, addr: SocketAddr) -> Self {
        Self {
            name: name.into(),
            mode: FakeMode::Dial(addr),
        }
    }

    pub(crate) fn connect_fail(name: &str) -> Self {
        Self {
            name: name.into(),
            mode: FakeMode::ConnectFail,
        }
    }

    pub(crate) fn always_fail(name: &str) -> Self {
        Self {
            name: name.into(),
            mode: FakeMode::AlwaysFail,
        }
    }
}

#[async_trait::async_trait]
impl Outbound for FakeOutbound {
    fn name(&self) -> &str {
        &self.name
    }

    async fn connect(&self, _target: &TargetAddr) -> anyhow::Result<ProxyStream> {
        match &self.mode {
            FakeMode::Echo(a) | FakeMode::Dial(a) => Ok(Box::new(TcpStream::connect(*a).await?)),
            FakeMode::ConnectFail => Err(anyhow::anyhow!(
                "synthetic connect failure (fake outbound '{}')",
                self.name
            )),
            FakeMode::AlwaysFail => Err(anyhow::anyhow!(
                "synthetic connect failure (fake outbound '{}')",
                self.name
            )),
        }
    }

    async fn probe(&self, _target: &TargetAddr) -> anyhow::Result<Duration> {
        match &self.mode {
            FakeMode::Echo(_) | FakeMode::Dial(_) | FakeMode::ConnectFail => {
                Ok(Duration::from_millis(1))
            }
            FakeMode::AlwaysFail => Err(anyhow::anyhow!(
                "synthetic probe failure (fake outbound '{}')",
                self.name
            )),
        }
    }
}

/// Poll a server-side `Option<SocketAddr>` getter (ephemeral :0 binds are
/// only known once the server has bound) until it reports, or panic.
pub(crate) async fn wait_for_addr(get: impl Fn() -> Option<SocketAddr>) -> SocketAddr {
    for _ in 0..300 {
        if let Some(a) = get() {
            return a;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("server did not report its bound address within 3 s");
}
