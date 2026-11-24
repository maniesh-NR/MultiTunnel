//! mt-net — the native Rust data plane.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert B"): SOCKS5 + HTTP inbounds, a routing rule table, and
//! net-merge-style link aggregation (failover / round-robin /
//! lowest-latency with health probes and quarantine) — all in-process,
//! replacing what the external net-merge binary did for connection
//! spreading. The `Outbound` trait is the boundary every link kind
//! implements (direct, socks, managed engine).
//!
//! Architecture contract carried over from the root crate: mux stays the
//! engine's problem (off); this crate spreads individual TCP connections
//! across links, so it must never assume a single shared upstream stream.

use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Target of an outbound connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Host {
    Domain(String),
    Ip(IpAddr),
}

/// A proxy target address (host + port).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetAddr {
    pub host: Host,
    pub port: u16,
}

/// Anything that is both AsyncRead and AsyncWrite and can cross awaits.
pub trait AsyncStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}

impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> AsyncStream for T {}

/// A boxed bidirectional stream to a proxy target.
pub type ProxyStream = Box<dyn AsyncStream>;

/// One outbound link. Implemented by direct connections, SOCKS5 forwards
/// and managed engine children (mt-engine).
#[async_trait::async_trait]
pub trait Outbound: Send + Sync {
    /// Stable link name for events, health tables and logs.
    fn name(&self) -> &str;

    /// Open a tunneled connection to `target`.
    async fn connect(&self, target: &TargetAddr) -> anyhow::Result<ProxyStream>;

    /// Measure a full round trip through this link to `target`.
    async fn probe(&self, target: &TargetAddr) -> anyhow::Result<Duration>;
}

/// Aggregation policy for spreading new connections across links.
///
/// S11: `Balanced` (byte-load-aware spreading) and `Priority` (user
/// priority order with saturation spill) join the original three. The
/// per-connection picker consumes the live LOAD fields of
/// [`LinkHealth`] (`rate_bps`, `active_conns`, `capacity_bps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MergePolicy {
    /// Always the first healthy link; others are hot spares.
    #[default]
    Failover,
    /// Spread new connections across healthy links in order.
    RoundRobin,
    /// Prefer the link with the best (EWMA) latency.
    LowestLatency,
    /// S11: spread by live BYTE LOAD — each new connection goes to the
    /// link carrying the least traffic right now (rate + a per-connection
    /// reservation), so fat flows stop skewing the spread.
    Balanced,
    /// S11: user priority order (link order = the Adapters-screen order).
    /// The first link takes everything until its measured rate reaches its
    /// learned capacity (saturation); the excess spills to the next link,
    /// and so on. When every link is saturated the least-loaded one wins.
    Priority,
}

/// Live health of one link, updated by the background prober.
/// (Runtime-only: not serialized — the GUI service maps this to its own
/// UI-shaped status types.)
#[derive(Debug, Clone)]
pub struct LinkHealth {
    pub ok: bool,
    pub latency: Option<Duration>,
    pub ewma_ms: f32,
    pub consecutive_failures: u32,
    pub quarantined_until: Option<Instant>,
    /// S11 load tracking (per-connection scheduling): live connections
    /// placed on this link (incremented at establish, decremented when the
    /// connection ends).
    pub active_conns: u32,
    /// S11: EWMA of this link's total (up+down) throughput in bytes/s,
    /// refreshed once per server tick from the data-plane counters.
    pub rate_bps: u64,
    /// S11: learned capacity estimate (peak observed rate, slowly decaying
    /// toward the current rate while idle). Below [`merge::CAPACITY_FLOOR_BPS`]
    /// it counts as "unknown" — the link is assumed to have headroom.
    pub capacity_bps: u64,
    /// S11: consecutive ticks without a meaningful capacity rise (the
    /// RAMP→PLATEAU detector behind saturation; see [`merge::update_load`]).
    pub plateau_ticks: u32,
    /// S11: saturation verdict — rate pinned at a KNOWN capacity that has
    /// stopped rising. Recomputed by [`merge::update_load`] every tick.
    pub saturated: bool,
}

/// Routing rule matcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuleMatcher {
    DomainSuffix { suffix: String },
    DomainKeyword { keyword: String },
    IpCidr { cidr: String },
    Any,
}

/// Routing rule action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Proxy,
    Direct,
    Reject,
}

/// One ordered routing rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub matcher: RuleMatcher,
    pub action: Action,
}

/// Events emitted by a running [`ProxyServer`] (names are stable for the
/// GUI service contract; payloads are human-shaped strings).
/// (Runtime-only: not serialized — consumed via tokio broadcast.)
#[derive(Debug, Clone)]
pub enum ProxyEvent {
    Started,
    Stopped,
    ConnOpened {
        target: String,
        link: String,
    },
    ConnClosed {
        link: String,
        up: u64,
        down: u64,
        dur_ms: u64,
    },
    LinkHealth {
        link: String,
        health: LinkHealth,
    },
    Tick {
        up_bps: u64,
        down_bps: u64,
    },
}

// ---------------------------------------------------------------------------
// S6-B implementation module map (everything below implements the frozen
// contract above; public types/signatures are unchanged):
//
// - `error`    — thiserror enum (`NetError`) for library-internal errors.
// - `outbound` — `DirectOutbound` (plain TCP, `Action::Direct`).
// - `socks5`   — `Socks5Outbound` client (wire discipline ported from the
//                root crate's `socks_probe`) + RFC 1928 server handshake.
// - `http`     — HTTP proxy inbound: CONNECT + absolute-form, 431/400/502.
// - `router`   — ordered first-match rule table (label-boundary suffixes).
// - `merge`    — pure `pick()`, `Aggregator` (tokio-Mutex healths), EWMA +
//                quarantine + per-link prober task.
// - `server`   — `ProxyServer`: listeners, per-connection flow, transparent
//                retry, live byte counters, broadcast events, graceful
//                shutdown. The library NEVER creates its own runtime.
// ---------------------------------------------------------------------------

mod error;
mod http;
mod merge;
mod outbound;
mod qos;
mod router;
mod server;
mod shutdown;
mod socks5;
#[cfg(test)]
mod test_support;

pub use error::NetError;
pub use merge::{
    pick, pressure_of, relative_load_of, update_load, Aggregator, QuarantinePolicy,
    CAPACITY_FLOOR_BPS, CONN_RESERVE_BPS, SATURATION_FRACTION,
};
pub use outbound::DirectOutbound;
pub use qos::QosScheduler;
pub use router::Router;
pub use server::ProxyServer;
pub use socks5::Socks5Outbound;

impl std::fmt::Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Host::Domain(d) => f.write_str(d),
            Host::Ip(ip) => write!(f, "{ip}"),
        }
    }
}

impl std::fmt::Display for TargetAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}
