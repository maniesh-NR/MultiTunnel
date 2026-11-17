//! mt-platform — platform integration for the all-in-one app.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert D"): app dirs with env overrides; the engine locator that
//! makes the app self-provisioning (locate → verify → embedded →
//! download, never silently — D14); autostart; a read-only update check.
//! sysproxy/elevate stay in the root crate this stage.

use std::path::PathBuf;

/// App config dir (Windows: %APPDATA%\MultiTunnel; Linux:
/// $XDG_CONFIG_HOME/multitunnel). Env override: MULTITUNNEL_CONFIG_DIR.
pub fn config_dir() -> PathBuf {
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}

/// App cache dir (Windows: %LOCALAPPDATA%\MultiTunnel\cache; Linux:
/// $XDG_CACHE_HOME/multitunnel). Env override: MULTITUNNEL_CACHE_DIR.
pub fn cache_dir() -> PathBuf {
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}

/// Log dir (config_dir/logs). Env override: MULTITUNNEL_LOG_DIR.
pub fn log_dir() -> PathBuf {
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}

/// State dir (config_dir/state). Env override: MULTITUNNEL_STATE_DIR.
pub fn state_dir() -> PathBuf {
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}

/// The default [`mt_engine::EngineLocator`] (D14 resolution order).
pub struct DefaultEngineLocator {
    _private: (),
}

impl DefaultEngineLocator {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for DefaultEngineLocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Enable launch-on-login for the app at `app_path`.
pub fn autostart_enable(app_path: &std::path::Path) -> anyhow::Result<()> {
    let _ = app_path;
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}

/// Disable launch-on-login.
pub fn autostart_disable() -> anyhow::Result<()> {
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}

/// Is launch-on-login currently enabled?
pub fn autostart_is_enabled() -> bool {
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}

/// Latest release tag from GitHub, if reachable. Read-only; no auto-apply.
pub async fn latest_release() -> anyhow::Result<Option<String>> {
    todo!("Expert D: implement per harness/CONTRACTS-S6.md")
}
