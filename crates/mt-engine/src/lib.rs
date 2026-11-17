//! mt-engine — the managed Xray engine.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert E"): one Xray child process per outbound link, driven by a
//! generated config JSON, exposed to mt-net as an [`mt_net::Outbound`].
//! Lifecycle discipline is ported from the root crate's `core_proc`
//! (waiter semantics, log pumping, CREATE_NO_WINDOW) — async here.
//!
//! MUX STAYS OFF (root-crate architecture contract): the Rust router in
//! mt-net decides what reaches the engine and spreads individual TCP
//! connections across links; a muxed tunnel is one stream on one link and
//! would defeat aggregation. `generate_config` pins `"mux": {"enabled":
//! false}` and a test asserts it.

use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

/// One managed engine: tag + endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineSpec {
    pub tag: String,
    pub endpoint: VlessRealityEndpoint,
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
    let _ = (spec, socks_listen);
    todo!("Expert E: implement per harness/CONTRACTS-S6.md")
}

/// A supervised Xray child bound to one link. Kill-on-drop; self-heals
/// with backoff; connects through its private loopback SOCKS port.
pub struct ManagedEngine {
    _private: (),
}

impl ManagedEngine {
    /// Production constructor: resolves the binary through `locator`.
    pub fn new(
        spec: EngineSpec,
        locator: std::sync::Arc<dyn EngineLocator>,
        state_dir: PathBuf,
        log_dir: PathBuf,
    ) -> Self {
        let _ = (spec, locator, state_dir, log_dir);
        todo!("Expert E: implement per harness/CONTRACTS-S6.md")
    }
}
