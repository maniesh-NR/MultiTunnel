//! mt-engine — the managed Xray engine.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert E"): one Xray child process per outbound link, driven by a
//! generated config JSON, exposed to mt-net as an [`mt_net::Outbound`].
//! Lifecycle discipline is ported from the root crate's `core_proc`
//! (waiter semantics, log pumping, CREATE_NO_WINDOW) — async here.
//!
//! MUX STAYS OFF (root-crate architecture contract, copied verbatim from
//! `src/xrayconf.rs`): "net-merge distributes individual TCP connections
//! across uplinks. Mux would collapse all traffic into a single stream,
//! and one stream always rides one link — bonding would be defeated."
//! The Rust router in mt-net decides what reaches the engine and spreads
//! individual TCP connections across links; a muxed tunnel is one stream
//! on one link and would defeat aggregation. `generate_config` pins
//! `"mux": {"enabled": false}` and a test asserts it.
//!
//! IMPLEMENTATION NOTES (S6-E decisions, "pick one and document it"):
//! - Child model: `std::process::Child` (NOT `tokio::process`) in a
//!   poison-tolerant `std::sync::Mutex` slot — the exact `core_proc`
//!   discipline. This makes kill-on-drop a plain sync `Drop` (`kill()`
//!   needs no runtime), reuses the proven waiter/pump thread design, and
//!   keeps every await point free of held-mutex-over-await hazards. The
//!   async layer only does binary resolution (locator), port waits,
//!   backoff sleeps and the SOCKS5 handshake (via mt-net).
//! - Config file: `state_dir/engine-<tag>.json`, written to a `0600`
//!   tmp file then renamed (it embeds the UUID). On Windows there are no
//!   POSIX perms; the profile ACLs apply. The file path or contents must
//!   NEVER appear in the engine log (contract test) — the spawn header
//!   records tag + port only.
//! - Errors never echo credential values (uuid/publicKey); they name
//!   tag, port and binary path at most (D9/D10).
//! - The frozen `VlessRealityEndpoint` has no `flow` field; the outbound
//!   hardcodes `"flow": "xtls-rprx-vision"` matching the root crate's
//!   proven REALITY config. FLAGGED to the orchestrator: if a non-vision
//!   server is ever needed, extend the endpoint struct (wave 3).
//!
//! Module map: `config` stays inline below (`generate_config`), engine
//! machinery lives in [`engine`] (`ManagedEngine`, `EngineCommand`,
//! `EnginePolicy`, `EngineError`).

use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

pub mod engine;

pub use engine::{EngineCommand, EngineError, EnginePolicy, ManagedEngine};

/// A VLESS+REALITY server endpoint (mirror of the root `config::Server`
/// semantics; field names are the engine's own).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VlessRealityEndpoint {
    pub address: String,
    pub port: u16,
    pub uuid: String,
    pub sni: String,
    pub public_key: String,
    pub short_id: String,
    pub fingerprint: String,
    pub spider_x: Option<String>,
}

/// One managed engine: tag + endpoint (+ optional local bind).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineSpec {
    pub tag: String,
    pub endpoint: VlessRealityEndpoint,
    /// S7 (adapter bonding): local IPv4 to bind the engine's outgoing
    /// socket to (Xray `sendThrough`). `None` = the OS routing table
    /// decides (v0.3 behavior). Binding to an adapter's address forces
    /// the VLESS connection to egress through that adapter — one engine
    /// per bonded adapter, the aggregator spreads connections across
    /// them. `#[serde(default)]` keeps old serialized specs loading.
    #[serde(default)]
    pub send_through: Option<String>,
}

/// Where the xray binary comes from. Implemented by mt-platform
/// (env override → exe-dir → verified cache → embedded → download).
#[async_trait::async_trait]
pub trait EngineLocator: Send + Sync {
    /// Path already on disk, if any. NEVER downloads.
    fn locate(&self) -> Option<PathBuf>;

    /// Path on disk, downloading if necessary (user-triggered).
    async fn ensure(&self) -> anyhow::Result<PathBuf>;
}

/// Generate the Xray config JSON for one engine: a single loopback SOCKS
/// inbound (noauth, TCP-only) + a single VLESS/REALITY outbound with mux
/// disabled. No freedom/blackhole outbounds, no routing rules — the Rust
/// router in mt-net decides what reaches the engine.
pub fn generate_config(spec: &EngineSpec, socks_listen: SocketAddr) -> serde_json::Value {
    let ep = &spec.endpoint;
    // Xray `sendThrough`: a top-level outbound field — the dialer binds the
    // socket to this local address before connecting (adapter pinning).
    // Tolerant: an empty/whitespace string is treated as "not set".
    let send_through = spec
        .send_through
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mut outbound = json!({
        "tag": "proxy",
        "protocol": "vless",
        "settings": {
            "vnext": [{
                "address": ep.address,
                "port": ep.port,
                "users": [{
                    "id": ep.uuid,
                    // See module docs: flow is not on the frozen
                    // endpoint; the root crate's REALITY config uses
                    // vision, so we pin it here too.
                    "flow": "xtls-rprx-vision",
                    "encryption": "none",
                }],
            }],
        },
        "streamSettings": {
            "network": "tcp",
            "security": "reality",
            "realitySettings": {
                "serverName": ep.sni,
                "fingerprint": ep.fingerprint,
                "publicKey": ep.public_key,
                "shortId": ep.short_id,
                "spiderX": ep.spider_x.clone().unwrap_or_else(|| "/".to_string()),
            },
        },
        // CRITICAL CONTRACT: mux must stay off. net-merge bonds by
        // spreading individual TCP connections; a muxed tunnel is one
        // stream on one link (see module docs).
        "mux": { "enabled": false },
    });
    if let Some(ip) = send_through {
        outbound["sendThrough"] = json!(ip);
    }
    json!({
        "log": { "loglevel": "warning" },
        "inbounds": [{
            "tag": "socks-in",
            "listen": socks_listen.ip().to_string(),
            "port": socks_listen.port(),
            "protocol": "socks",
            "settings": { "auth": "noauth", "udp": false },
        }],
        "outbounds": [outbound],
    })
}
