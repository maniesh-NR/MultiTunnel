//! mt-links — share links, the multi-server profile store, subscriptions.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert A"): the public types and function signatures below are stable;
//! the implementation is filled in by the links expert. Policy follows
//! D9/D10: strict where the rest of the app must faithfully represent a
//! link, liberal where v2rayN compatibility only needs parsing; error
//! messages never echo credential values; tests use the synthetic vectors
//! from CONTRACTS-S6.md only.

use serde::{Deserialize, Serialize};

/// Protocol of a proxy profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
}

/// Transport layer under the protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Transport {
    Tcp,
    Ws { host: Option<String>, path: String },
    Grpc { service: String },
    HttpUpgrade { host: Option<String>, path: String },
}

/// Security layer over the transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Security {
    None,
    Tls {
        sni: Option<String>,
        alpn: Vec<String>,
        allow_insecure: bool,
    },
    Reality {
        sni: String,
        fingerprint: String,
        public_key: String,
        short_id: String,
        spider_x: Option<String>,
    },
}

/// One imported server profile (v2rayN "server" equivalent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub tag: String,
    pub protocol: Protocol,
    pub address: String,
    pub port: u16,
    /// UUID (vless/vmess) or password (trojan/ss) — never logged verbatim.
    pub identity: String,
    pub flow: Option<String>,
    pub transport: Transport,
    pub security: Security,
    pub remark: String,
}

/// Parse errors. Messages must never contain credential values.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("not a supported share link: {0}")]
    Unsupported(String),
    #[error("malformed link: {0}")]
    Malformed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Parse a `vless://`, `vmess://`, `ss://` or `trojan://` share link.
pub fn parse_share_link(link: &str) -> Result<Profile, LinkError> {
    let _ = link;
    todo!("Expert A: implement per harness/CONTRACTS-S6.md")
}

/// Render a profile back to a share link (vless/trojan/ss).
pub fn render_share_link(profile: &Profile) -> Result<String, LinkError> {
    let _ = profile;
    todo!("Expert A: implement per harness/CONTRACTS-S6.md")
}

/// Parse a subscription body (base64 or plain lines) into profiles plus
/// human-readable failure lines (never containing credentials).
pub fn parse_subscription(body: &str) -> (Vec<Profile>, Vec<String>) {
    let _ = body;
    todo!("Expert A: implement per harness/CONTRACTS-S6.md")
}

/// Migrate a v0.2 `config.json` `server` block into a VLESS+REALITY profile.
pub fn migrate_legacy_server(v: &serde_json::Value) -> Result<Profile, LinkError> {
    let _ = v;
    todo!("Expert A: implement per harness/CONTRACTS-S6.md")
}

/// The multi-server profile store (`profiles.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProfileStore {
    pub version: u32,
    pub active: Option<String>,
    pub profiles: Vec<Profile>,
}

impl ProfileStore {
    /// Load from a path; a missing file yields an empty store.
    pub fn load(path: &std::path::Path) -> Result<Self, LinkError> {
        let _ = path;
        todo!("Expert A: implement per harness/CONTRACTS-S6.md")
    }

    /// Atomically save (tmp + rename; 0600 on POSIX).
    pub fn save(&self, path: &std::path::Path) -> Result<(), LinkError> {
        let _ = path;
        todo!("Expert A: implement per harness/CONTRACTS-S6.md")
    }

    /// Insert or update (dedupe by protocol+address+port+identity).
    /// Returns the effective profile id.
    pub fn upsert(&mut self, profile: Profile) -> String {
        let _ = profile;
        todo!("Expert A: implement per harness/CONTRACTS-S6.md")
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let _ = id;
        todo!("Expert A: implement per harness/CONTRACTS-S6.md")
    }

    pub fn set_active(&mut self, id: Option<&str>) -> bool {
        let _ = id;
        todo!("Expert A: implement per harness/CONTRACTS-S6.md")
    }
}
