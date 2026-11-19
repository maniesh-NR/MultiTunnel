//! App directories with env overrides.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert D"; frozen doc comments in
//! `lib.rs` are authoritative):
//!
//! | fn            | Windows                                  | Linux                                  | env override             |
//! |---------------|------------------------------------------|----------------------------------------|--------------------------|
//! | `config_dir`  | `%APPDATA%\MultiTunnel`                  | `$XDG_CONFIG_HOME/multitunnel`         | `MULTITUNNEL_CONFIG_DIR` |
//! | `cache_dir`   | `%LOCALAPPDATA%\MultiTunnel\cache`       | `$XDG_CACHE_HOME/multitunnel`          | `MULTITUNNEL_CACHE_DIR`  |
//! | `log_dir`     | `config_dir/logs`                        | `config_dir/logs`                      | `MULTITUNNEL_LOG_DIR`    |
//! | `state_dir`   | `config_dir/state`                       | `config_dir/state`                     | `MULTITUNNEL_STATE_DIR`  |
//!
//! Hand-rolled: the `dirs` crate is NOT allowed by the contract.
//!
//! IMPLEMENTATION NOTES (picked and documented):
//! * The `*_DIR` overrides take precedence over everything and are created
//!   on demand like the default paths; tests point them at temp dirs so no
//!   test ever touches the real `$HOME`.
//! * On non-Windows the XDG base dir is used only when the env value is
//!   absolute (XDG spec §3); otherwise `$HOME/<leaf>` is used. `$HOME` is
//!   read with `var_os` (no `home` crate). Last-resort fallback when both
//!   XDG var and `$HOME` are missing (rare: bare service contexts):
//!   `std::env::temp_dir()/multitunnel` — a total function beats a panic.
//! * On Windows `%APPDATA%` (config) / `%LOCALAPPDATA%` (cache) fall back
//!   to each other and finally to the temp dir.
//! * "create on demand": `create_dir_all` best-effort; on failure we still
//!   return the path (signatures are infallible) and log a `warn!`.
//! * `autostart` (Linux) resolves its file under the SAME
//!   `$XDG_CONFIG_HOME`/`$HOME/.config` base (XDG autostart spec), NOT
//!   under `config_dir()`'s `multitunnel` subfolder.

use std::path::PathBuf;

/// Env override for [`config_dir`].
pub const ENV_CONFIG_DIR: &str = "MULTITUNNEL_CONFIG_DIR";
/// Env override for [`cache_dir`].
pub const ENV_CACHE_DIR: &str = "MULTITUNNEL_CACHE_DIR";
/// Env override for [`log_dir`].
pub const ENV_LOG_DIR: &str = "MULTITUNNEL_LOG_DIR";
/// Env override for [`state_dir`].
pub const ENV_STATE_DIR: &str = "MULTITUNNEL_STATE_DIR";

#[cfg(not(windows))]
const APP_DIR_UNIX: &str = "multitunnel";
#[cfg(windows)]
const APP_DIR_WINDOWS: &str = "MultiTunnel";

/// App config dir (Windows: `%APPDATA%\MultiTunnel`; Linux:
/// `$XDG_CONFIG_HOME/multitunnel`). Env override: MULTITUNNEL_CONFIG_DIR.
pub fn config_dir() -> PathBuf {
    if let Some(p) = overridden(ENV_CONFIG_DIR) {
        return ensure_created(p);
    }
    #[cfg(windows)]
    {
        let base = known_folder("APPDATA")
            .or_else(|| known_folder("LOCALAPPDATA"))
            .unwrap_or_else(temp_base);
        ensure_created(base.join(APP_DIR_WINDOWS))
    }
    #[cfg(not(windows))]
    {
        ensure_created(xdg_config_home().join(APP_DIR_UNIX))
    }
}

/// App cache dir (Windows: `%LOCALAPPDATA%\MultiTunnel\cache`; Linux:
/// `$XDG_CACHE_HOME/multitunnel`). Env override: MULTITUNNEL_CACHE_DIR.
pub fn cache_dir() -> PathBuf {
    if let Some(p) = overridden(ENV_CACHE_DIR) {
        return ensure_created(p);
    }
    #[cfg(windows)]
    {
        let base = known_folder("LOCALAPPDATA")
            .or_else(|| known_folder("APPDATA"))
            .unwrap_or_else(temp_base);
        ensure_created(base.join(APP_DIR_WINDOWS).join("cache"))
    }
    #[cfg(not(windows))]
    {
        ensure_created(xdg_cache_home().join(APP_DIR_UNIX))
    }
}

/// Log dir (config_dir/logs). Env override: MULTITUNNEL_LOG_DIR.
pub fn log_dir() -> PathBuf {
    if let Some(p) = overridden(ENV_LOG_DIR) {
        return ensure_created(p);
    }
    ensure_created(config_dir().join("logs"))
}

/// State dir (config_dir/state). Env override: MULTITUNNEL_STATE_DIR.
pub fn state_dir() -> PathBuf {
    if let Some(p) = overridden(ENV_STATE_DIR) {
        return ensure_created(p);
    }
    ensure_created(config_dir().join("state"))
}

/// `$XDG_CONFIG_HOME` if absolute, else `$HOME/.config`, else
/// `temp_dir()/config` (last resort).
#[cfg(not(windows))]
pub(crate) fn xdg_config_home() -> PathBuf {
    xdg_base("XDG_CONFIG_HOME", ".config").unwrap_or_else(|| temp_base().join("config"))
}

/// `$XDG_CACHE_HOME` if absolute, else `$HOME/.cache`, else
/// `temp_dir()/cache` (last resort).
#[cfg(not(windows))]
pub(crate) fn xdg_cache_home() -> PathBuf {
    xdg_base("XDG_CACHE_HOME", ".cache").unwrap_or_else(|| temp_base().join("cache"))
}

#[cfg(not(windows))]
fn xdg_base(var: &str, home_leaf: &str) -> Option<PathBuf> {
    let from_xdg = std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    from_xdg.or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|home| home.join(home_leaf))
    })
}

fn overridden(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

#[cfg(windows)]
fn known_folder(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn temp_base() -> PathBuf {
    std::env::temp_dir()
}

fn ensure_created(path: PathBuf) -> PathBuf {
    if let Err(e) = std::fs::create_dir_all(&path) {
        tracing::warn!(dir = %path.display(), error = %e, "could not create app directory");
    }
    path
}

/// Assert the on-demand creation side effect.
#[cfg(test)]
pub(crate) fn exists(p: &std::path::Path) -> bool {
    p.is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env_lock, set_env, temp_root, unset_env};
    use std::fs;

    #[test]
    fn overrides_win_and_dirs_are_created() {
        let _guard = env_lock();
        let base = temp_root("dirs-override");
        let cfg = base.join("cfg");
        let cache = base.join("cache");
        let logs = base.join("logs");
        let state = base.join("state");
        set_env(ENV_CONFIG_DIR, &cfg);
        set_env(ENV_CACHE_DIR, &cache);
        set_env(ENV_LOG_DIR, &logs);
        set_env(ENV_STATE_DIR, &state);

        assert_eq!(config_dir(), cfg);
        assert_eq!(cache_dir(), cache);
        assert_eq!(log_dir(), logs);
        assert_eq!(state_dir(), state);
        assert!(exists(&cfg), "config dir created on demand");
        assert!(exists(&cache));
        assert!(exists(&logs));
        assert!(exists(&state));

        unset_env(ENV_CONFIG_DIR);
        unset_env(ENV_CACHE_DIR);
        unset_env(ENV_LOG_DIR);
        unset_env(ENV_STATE_DIR);
    }

    #[test]
    fn log_and_state_default_under_config() {
        let _guard = env_lock();
        let base = temp_root("dirs-logstate");
        let cfg = base.join("cfg");
        set_env(ENV_CONFIG_DIR, &cfg);
        unset_env(ENV_LOG_DIR);
        unset_env(ENV_STATE_DIR);

        assert_eq!(log_dir(), cfg.join("logs"));
        assert_eq!(state_dir(), cfg.join("state"));
        assert!(exists(&cfg.join("logs")));
        assert!(exists(&cfg.join("state")));

        unset_env(ENV_CONFIG_DIR);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn xdg_defaults_on_linux() {
        let _guard = env_lock();
        let base = temp_root("dirs-xdg");
        let config_home = base.join("config-home");
        let cache_home = base.join("cache-home");
        let home = base.join("home");
        let prev_home = std::env::var_os("HOME");
        set_env("XDG_CONFIG_HOME", &config_home);
        set_env("XDG_CACHE_HOME", &cache_home);
        set_env("HOME", &home);
        for v in [ENV_CONFIG_DIR, ENV_CACHE_DIR, ENV_LOG_DIR, ENV_STATE_DIR] {
            unset_env(v);
        }

        assert_eq!(config_dir(), config_home.join(APP_DIR_UNIX));
        assert_eq!(cache_dir(), cache_home.join(APP_DIR_UNIX));
        // XDG spec: relative XDG values are ignored → fall back to $HOME.
        set_env("XDG_CONFIG_HOME", "relative/path");
        assert_eq!(xdg_config_home(), home.join(".config"));

        unset_env("XDG_CONFIG_HOME");
        unset_env("XDG_CACHE_HOME");
        match prev_home {
            Some(h) => set_env("HOME", std::path::PathBuf::from(h)),
            None => unset_env("HOME"),
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn empty_override_is_ignored() {
        let _guard = env_lock();
        let base = temp_root("dirs-empty");
        set_env(ENV_CONFIG_DIR, &base);
        set_env(ENV_CONFIG_DIR, "");
        assert_ne!(config_dir(), base, "empty override falls back to XDG/HOME");
        unset_env(ENV_CONFIG_DIR);
    }

    #[test]
    fn override_survives_repeated_calls() {
        let _guard = env_lock();
        let base = temp_root("dirs-stable");
        set_env(ENV_STATE_DIR, &base);
        let first = state_dir();
        let _ = fs::create_dir_all(&first);
        assert_eq!(state_dir(), first);
        unset_env(ENV_STATE_DIR);
    }
}
