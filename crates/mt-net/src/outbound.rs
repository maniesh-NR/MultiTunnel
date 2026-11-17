//! Direct outbound — plain TCP connects with no tunneling.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"): `DirectOutbound` is the
//! no-tunnel link used for `Action::Direct` routing and as the baseline
//! [`Outbound`] implementation. `connect` opens a plain TCP connection to
//! the target; `probe` measures a full TCP connect round trip. Happy
//! eyeballs is explicitly NOT required by the contract and NOT implemented.
//!
//! IMPLEMENTATION NOTES (S6-B decisions, "pick one and document it"):
//! - One total timeout (10 s) wraps the whole connect/probe, mirroring the
//!   root crate's deadline discipline (`socks_probe`, S2-REV-C P1-1):
//!   per-operation timeouts alone let a byte-dripping peer stall forever.
//! - Domain targets are resolved through tokio's `ToSocketAddrs`
//!   (getaddrinfo on the blocking pool); no explicit happy-eyeballs race.
//! - `probe` = full connect + immediate drop of the stream: the cheapest
//!   honest "can this link carry a fresh TCP connection" signal.

use std::time::{Duration, Instant};

use tokio::net::TcpStream;

use crate::{Host, Outbound, ProxyStream, TargetAddr};

/// Total budget for one direct connect (see module docs).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Plain-TCP outbound (`Action::Direct`); no tunneling.
#[derive(Debug, Clone, Copy, Default)]
pub struct DirectOutbound;

impl DirectOutbound {
    /// Create the direct outbound.
    pub fn new() -> Self {
        Self
    }
}

async fn tcp_connect(host: &Host, port: u16) -> std::io::Result<TcpStream> {
    match host {
        Host::Domain(d) => TcpStream::connect((d.as_str(), port)).await,
        Host::Ip(ip) => TcpStream::connect(std::net::SocketAddr::new(*ip, port)).await,
    }
}

#[async_trait::async_trait]
impl Outbound for DirectOutbound {
    fn name(&self) -> &str {
        "direct"
    }

    async fn connect(&self, target: &TargetAddr) -> anyhow::Result<ProxyStream> {
        match tokio::time::timeout(CONNECT_TIMEOUT, tcp_connect(&target.host, target.port)).await {
            Ok(Ok(s)) => Ok(Box::new(s)),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(anyhow::anyhow!("direct connect to {target} timed out")),
        }
    }

    async fn probe(&self, target: &TargetAddr) -> anyhow::Result<Duration> {
        let started = Instant::now();
        match tokio::time::timeout(CONNECT_TIMEOUT, tcp_connect(&target.host, target.port)).await {
            Ok(Ok(s)) => {
                drop(s);
                Ok(started.elapsed())
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(anyhow::anyhow!("direct probe of {target} timed out")),
        }
    }
}
