//! mt-service — the UI service contract.
//!
//! Written by the orchestrator (complete, no expert fills this in): the
//! types and trait the GUI (mt-app) codes against. The root crate provides
//! `RealService` (ProfileStore + mt-net::ProxyServer + mt-engine +
//! mt-platform + legacy sysproxy); mt-app ships a `MockService` for
//! standalone development. All payloads are UI-shaped (Strings, millis) —
//! this crate depends on no other mt-* crate by design.
//!
//! S7 (adapter bonding): [`AdapterView`] + the `adapters` /
//! `set_bonded_adapters` trait methods + `SettingsView::bonded_adapters`.
//! `bonded_adapters` holds adapter IDS (not display names): on Windows the
//! stable adapter GUID, on dev hosts the interface name. Empty = no
//! pinning (one link, the OS routing table decides — v0.3 behavior).

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Connection lifecycle state (mirrors the root supervisor's State).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

/// Point-in-time status for the Status screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub state: ServiceState,
    pub active_profile: Option<String>,
    pub up_since: Option<String>,
    pub restarts: u32,
    pub links: Vec<LinkStatus>,
}

/// One aggregated link's live status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkStatus {
    pub name: String,
    pub ok: bool,
    pub latency_ms: Option<u32>,
    pub ewma_ms: Option<f32>,
    pub quarantined: bool,
    pub bytes_up: u64,
    pub bytes_down: u64,
}

/// A profile as the GUI sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileView {
    pub id: String,
    pub tag: String,
    pub protocol: String,
    pub endpoint: String,
    pub remark: String,
    pub active: bool,
}

/// One network adapter as the GUI's Adapters screen sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterView {
    /// Stable id — matches entries in
    /// [`SettingsView::bonded_adapters`].
    pub id: String,
    /// Friendly display name ("Wi-Fi", "Ethernet").
    pub name: String,
    /// Hardware/driver description ("Intel(R) Wi-Fi 6 AX201").
    pub description: String,
    /// Unicast IPv4 addresses (display; the service binds the engine to
    /// the first one when the adapter is bonded).
    pub ipv4: Vec<String>,
    /// OS reports the interface operationally up.
    pub up: bool,
    /// Currently selected for bonding (derived from settings by the
    /// service; a pure convenience for the GUI draft state).
    pub selected: bool,
}

/// A routing rule as the GUI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleView {
    pub kind: String,
    pub value: String,
    pub action: String,
}

/// App settings as the GUI sees them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsView {
    pub theme_seed: u32,
    pub dark_mode: bool,
    pub system_proxy: bool,
    pub merge_policy: String,
    pub autostart: bool,
    /// Adapter ids to bond the tunnel across (S7). `#[serde(default)]` so
    /// settings.json written by v0.3 loads unchanged (empty = one link,
    /// no adapter pinning).
    #[serde(default)]
    pub bonded_adapters: Vec<String>,
}

/// Live events pushed to the GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceEvent {
    StateChanged(ServiceState),
    LinkUpdated(LinkStatus),
    Throughput { up_bps: u64, down_bps: u64 },
    Latency { profile_id: String, ms: Option<u32> },
    LogLine(String),
}

/// The service the GUI talks to. Implementations: root crate's
/// `RealService`, mt-app's `MockService`.
#[async_trait::async_trait]
pub trait MtService: Send + Sync {
    async fn status(&self) -> StatusSnapshot;
    async fn connect(&self);
    async fn disconnect(&self);

    async fn profiles(&self) -> Vec<ProfileView>;
    /// Parse + store a share link; errors are user-facing Strings that
    /// never contain credential values.
    async fn import_link(&self, link: &str) -> Result<ProfileView, String>;
    async fn remove_profile(&self, id: &str) -> Result<(), String>;
    async fn set_active_profile(&self, id: &str) -> Result<(), String>;
    async fn test_latency(&self, id: &str) -> Result<u32, String>;

    async fn routing_rules(&self) -> Vec<RuleView>;
    async fn set_routing_rules(&self, rules: Vec<RuleView>) -> Result<(), String>;

    async fn settings(&self) -> SettingsView;
    async fn set_settings(&self, settings: SettingsView) -> Result<(), String>;

    /// Enumerate the machine's network adapters with their current
    /// bonding selection (never errors — enumeration fails open to an
    /// empty list, like the rest of the contract's read paths).
    async fn adapters(&self) -> Vec<AdapterView>;

    /// Replace the bonded-adapter selection (ids from [`Self::adapters`]).
    /// Persists immediately; a live session is restarted so the new link
    /// set takes effect. Unknown ids are an `Err` naming the id.
    async fn set_bonded_adapters(&self, ids: Vec<String>) -> Result<(), String>;

    /// Subscribe to live events (state, links, throughput, logs).
    fn events(&self) -> broadcast::Receiver<ServiceEvent>;
}
