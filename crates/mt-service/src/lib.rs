//! mt-service — the UI service contract.
//!
//! Written by the orchestrator (complete, no expert fills this in): the
//! types and trait the GUI (mt-app) codes against. The root crate provides
//! `RealService` (ProfileStore + mt-net::ProxyServer + mt-engine +
//! mt-platform + legacy sysproxy); mt-app ships a `MockService` for
//! standalone development. All payloads are UI-shaped (Strings, millis) —
//! this crate depends on no other mt-* crate by design.

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

    /// Subscribe to live events (state, links, throughput, logs).
    fn events(&self) -> broadcast::Receiver<ServiceEvent>;
}
