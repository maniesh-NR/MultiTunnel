//! mt-platform — platform integration for the all-in-one app.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert D"): app dirs with env overrides; the engine locator that
//! makes the app self-provisioning (locate → verify → embedded →
//! download, never silently — D14); autostart; a read-only update check.
//! sysproxy/elevate stay in the root crate this stage.
//!
//! Module map:
//! * [`dirs`] — config/cache/log/state dirs (hand-rolled, XDG + Windows).
//! * [`engine`] — `DefaultEngineLocator` (`mt_engine::EngineLocator`),
//!   `HttpFetch` (test-injectable HTTP), optional embedded engine zip.
//! * [`autostart`] — launch-on-login (HKCU Run key / XDG autostart).
//! * [`update`] — read-only "newer release available?" check.

mod autostart;
mod dirs;
mod engine;
mod update;

pub use autostart::{
    autostart_disable, autostart_enable, autostart_is_enabled, desktop_entry, run_key_value,
};
pub use dirs::{cache_dir, config_dir, log_dir, state_dir};
pub use engine::{DefaultEngineLocator, HttpFetch, PlatformError};

/// The production HTTP fetcher (only present with the default `net` feature).
#[cfg(feature = "net")]
pub use engine::ReqwestFetch;

/// Engine zip embedded at build time via `MT_EMBED_ENGINE` (feature `embedded`).
#[cfg(feature = "embedded")]
pub use engine::embedded;

pub use update::latest_release;

/// Shared test helpers: serialized env mutation + scratch dirs.
///
/// `cargo test` runs tests in parallel threads; every test that touches
/// process-global env vars must hold [`test_support::env_lock`]. All
/// scratch dirs are unique under `std::env::temp_dir()` so no test ever
/// reads or writes the real `$HOME`.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    static PATH_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Serialize tests that mutate env vars (poison-tolerant lock).
    pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Unique scratch directory (never the real `$HOME`).
    pub(crate) fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mt-platform-test-{label}-{}-{nanos}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Set an env var (test-only). `set_var` is unsafe from edition 2024;
    /// wrapped here for forward compatibility.
    pub(crate) fn set_env(var: &str, value: impl AsRef<Path>) {
        #[allow(deprecated_safe)]
        unsafe {
            std::env::set_var(var, value.as_ref())
        };
    }

    /// Remove an env var (test-only).
    pub(crate) fn unset_env(var: &str) {
        #[allow(deprecated_safe)]
        unsafe {
            std::env::remove_var(var)
        };
    }

    /// An [`crate::engine::HttpFetch`] fixture answering every URL with the
    /// same canned body (used by the update-check tests).
    pub(crate) fn fixture_http(body: Vec<u8>) -> std::sync::Arc<dyn crate::engine::HttpFetch> {
        std::sync::Arc::new(FixtureHttp { body })
    }

    struct FixtureHttp {
        body: Vec<u8>,
    }

    #[async_trait::async_trait]
    impl crate::engine::HttpFetch for FixtureHttp {
        async fn fetch(&self, _url: &str) -> anyhow::Result<Vec<u8>> {
            Ok(self.body.clone())
        }
    }
}
