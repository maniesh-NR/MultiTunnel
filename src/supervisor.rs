//! Connection lifecycle orchestration (stage S2 — implementer agent S2-A).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `State`: Disconnected / Connecting / Connected / Failed (Serialize).
//! - `Snapshot`: state, error, bond_error, sys_error, probe_error, merging,
//!   links, raw_status, system_proxy, restarts (Serialize).
//! - `Supervisor::new(Config)`, `connect(&self) -> Result<()>`,
//!   `disconnect(&self) -> Result<()>`, `snapshot(&self) -> Snapshot`,
//!   accessors log_dir()/state_dir()/net_merge_enabled().
//! - CONNECT ORDER (must not change):
//!   1 write xrayconf::render to paths.xray_config (0600 via main-style
//!     private write — reuse core_proc helper or OpenOptions)
//!   2 core start + wait_for_port socks (15s)
//!   3 socks_probe (fail fast → stop core, restore proxy, State::Failed)
//!   4 netmerge::ensure_running + netmerge::on (NON-FATAL: bond_error set)
//!   5 sysproxy::set(http_addr) if behavior.system_proxy (NON-FATAL)
//! - DISCONNECT: reverse 5→2 (sysproxy::restore, netmerge::off if merging,
//!   core stop).
//! - Fail-open: core death → restart w/ backoff 1s,2s,... cap 30s up to
//!   behavior.restart_max (0 = never); exhausted → restore proxy,
//!   State::Failed. Background thread polls every 5s (netmerge status →
//!   snapshot) and probes every behavior.probe_interval_sec (15s timeout).
//! - connect() when already connected/connecting = no-op Ok.
//! - Stale sysproxy self-heal: connect() starts with sysproxy::restore()
//!   best-effort (covers previous abrupt Ctrl+C exits).
//! - Tests: backoff sequence math; probe-stub-driven connect failure path
//!   (bad server → Failed, no system proxy change); snapshot serde.
pub enum State {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}
