//! WinINET system proxy control (stage S2 — implementer agent S2-B).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `set(proxy_addr: &str, proxy_override: &str, state_dir: &std::path::Path) -> crate::error::Result<()>`
//!   Windows (winreg, HKCU\Software\Microsoft\Windows\CurrentVersion\Internet
//!   Settings): save snapshot {ProxyEnable, ProxyServer, ProxyOverride,
//!   AutoConfigURL} to state_dir/sysproxy.json (0600-ish) if absent; then set
//!   ProxyEnable=1, ProxyServer=proxy_addr, ProxyOverride=override; notify via
//!   InternetSetOptionW(NULL, INTERNET_OPTION_SETTINGS_CHANGED/39) and
//!   (INTERNET_OPTION_REFRESH/37).
//! - `restore(state_dir)` restores snapshot values (deletes leftover fields),
//!   removes the snapshot file, refresh. Without snapshot: ProxyEnable=0.
//! - Non-Windows: both are no-op Ok.
//! - Unit tests MUST NOT touch the real HKCU settings: all registry-free
//!   logic is factored into the pure helpers below ([`Snapshot`] serde,
//!   [`string_write_plan`] / [`snapshot_from`] field mapping, and the
//!   snapshot-file save/load/remove trio) and every test uses a fake
//!   temporary state_dir. The registry itself is only ever exercised by the
//!   real `set`/`restore` code paths, never from tests.

#[cfg(any(windows, test))]
use std::io::Write as _;
use std::path::Path;

/// File the pre-change proxy state is snapshotted into, inside `state_dir`.
#[cfg(any(windows, test))]
const SNAPSHOT_FILE: &str = "sysproxy.json";

/// The saved pre-change proxy state. Serde keys are `proxyEnable`,
/// `proxyServer`, `proxyOverride`, `autoConfigURL` (note the exact
/// `autoConfigURL` casing WinINET uses). `None` string fields mean "the
/// registry value was absent before the change"; `Some("")` and `None` stay
/// distinct through the JSON round-trip. Missing JSON fields deserialize to
/// their defaults (`0` / `None`) so a hand-edited snapshot still restores.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Snapshot {
    #[serde(default, rename = "proxyEnable")]
    proxy_enable: u32,
    #[serde(default, rename = "proxyServer")]
    proxy_server: Option<String>,
    #[serde(default, rename = "proxyOverride")]
    proxy_override: Option<String>,
    #[serde(default, rename = "autoConfigURL")]
    autoconfig_url: Option<String>,
}

/// Build a snapshot from (possibly absent) registry reads. Pure mapping:
/// testable on in-memory values only, never the real registry.
#[cfg(windows)]
fn snapshot_from(
    proxy_enable: Option<u32>,
    proxy_server: Option<String>,
    proxy_override: Option<String>,
    autoconfig_url: Option<String>,
) -> Snapshot {
    Snapshot {
        proxy_enable: proxy_enable.unwrap_or(0),
        proxy_server,
        proxy_override,
        autoconfig_url,
    }
}

/// Write-back plan for the three REG_SZ values of a snapshot: `Some(value)`
/// sets the registry value, `None` deletes it (it was absent before the
/// proxy was enabled). Pure mapping: testable on in-memory values only.
#[cfg(windows)]
fn string_write_plan(s: &Snapshot) -> [(&'static str, Option<String>); 3] {
    [
        ("ProxyServer", s.proxy_server.clone()),
        ("ProxyOverride", s.proxy_override.clone()),
        ("AutoConfigURL", s.autoconfig_url.clone()),
    ]
}

#[cfg(any(windows, test))]
fn snapshot_path(state_dir: &Path) -> std::path::PathBuf {
    state_dir.join(SNAPSHOT_FILE)
}

/// Serialize + write the snapshot to `state_dir/sysproxy.json` with
/// create-new semantics: an existing snapshot is NEVER overwritten (the
/// first saved state is the one `restore` must return to, e.g. after a
/// crashed run). Mode is 0600 on unix; on Windows the file lands in the
/// user profile with default ACLs.
#[cfg(any(windows, test))]
fn save_snapshot(state_dir: &Path, snap: &Snapshot) -> crate::error::Result<()> {
    std::fs::create_dir_all(state_dir)?;
    let body = serde_json::to_string_pretty(snap)?;
    let path = snapshot_path(state_dir);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    match opts.open(&path) {
        Ok(mut f) => f
            .write_all(body.as_bytes())
            .map_err(|e| crate::err!("failed to write proxy snapshot {}: {}", path.display(), e)),
        // Snapshot already present (previous run not restored): keep it.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(crate::err!(
            "failed to create proxy snapshot {}: {}",
            path.display(),
            e
        )),
    }
}

/// Load the snapshot; `Ok(None)` when no snapshot file exists. A corrupt
/// file is an error, never a silent skip (restore must not guess).
#[cfg(any(windows, test))]
fn load_snapshot(state_dir: &Path) -> crate::error::Result<Option<Snapshot>> {
    let path = snapshot_path(state_dir);
    match std::fs::read_to_string(&path) {
        Ok(body) => serde_json::from_str(&body)
            .map(Some)
            .map_err(|e| crate::err!("corrupt proxy snapshot {}: {}", path.display(), e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(crate::err!("failed to read {}: {}", path.display(), e)),
    }
}

/// Remove the snapshot file. Missing file is fine (idempotent).
#[cfg(any(windows, test))]
fn remove_snapshot(state_dir: &Path) -> crate::error::Result<()> {
    match std::fs::remove_file(snapshot_path(state_dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(crate::err!(
            "failed to remove {}: {}",
            snapshot_path(state_dir).display(),
            e
        )),
    }
}

#[cfg(windows)]
use winreg::enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE};
#[cfg(windows)]
use winreg::RegKey;

/// HKCU subkey holding the WinINET proxy settings.
#[cfg(windows)]
const INET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

#[cfg(windows)]
fn open_inet_settings(rights: u32) -> crate::error::Result<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(INET_SETTINGS, rights)
        .map_err(|e| crate::err!("failed to open HKCU\\{}: {}", INET_SETTINGS, e))
}

#[cfg(windows)]
fn reg_set_dword(key: &RegKey, name: &str, value: u32) -> crate::error::Result<()> {
    key.set_value(name, &value)
        .map_err(|e| crate::err!("failed to write HKCU\\{}\\{}: {}", INET_SETTINGS, name, e))
}

#[cfg(windows)]
fn reg_set_string(key: &RegKey, name: &str, value: &str) -> crate::error::Result<()> {
    key.set_value(name, &value)
        .map_err(|e| crate::err!("failed to write HKCU\\{}\\{}: {}", INET_SETTINGS, name, e))
}

/// Delete a registry string value, tolerating "already absent" so restore
/// stays idempotent.
#[cfg(windows)]
fn reg_delete_string(key: &RegKey, name: &str) -> crate::error::Result<()> {
    if key.get_raw_value(name).is_ok() {
        key.delete_value(name).map_err(|e| {
            crate::err!("failed to delete HKCU\\{}\\{}: {}", INET_SETTINGS, name, e)
        })?;
    }
    Ok(())
}

/// Tell WinINET (and running shells/browsers) that the proxy settings
/// changed and must be re-read.
#[cfg(windows)]
fn refresh_internet_settings() -> crate::error::Result<()> {
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };
    // SAFETY: no buffer is involved (NULL buffer, zero length); the call only
    // broadcasts INTERNET_OPTION_SETTINGS_CHANGED and does not dereference
    // the NULL HINTERNET.
    let changed = unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        )
    };
    if changed == 0 {
        return Err(crate::err!(
            "InternetSetOptionW(INTERNET_OPTION_SETTINGS_CHANGED) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: same as above — NULL handle/buffer, zero length, pure notify.
    let refreshed = unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        )
    };
    if refreshed == 0 {
        return Err(crate::err!(
            "InternetSetOptionW(INTERNET_OPTION_REFRESH) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Enable the system proxy pointing at `proxy_addr` (see module contract).
pub fn set(proxy_addr: &str, proxy_override: &str, state_dir: &Path) -> crate::error::Result<()> {
    #[cfg(windows)]
    {
        let key = open_inet_settings(KEY_QUERY_VALUE | KEY_SET_VALUE)?;
        let current = snapshot_from(
            key.get_value::<u32, _>("ProxyEnable").ok(),
            key.get_value::<String, _>("ProxyServer").ok(),
            key.get_value::<String, _>("ProxyOverride").ok(),
            key.get_value::<String, _>("AutoConfigURL").ok(),
        );
        // Save the pre-change state once; never clobber an existing snapshot.
        save_snapshot(state_dir, &current)?;

        reg_set_dword(&key, "ProxyEnable", 1)?;
        reg_set_string(&key, "ProxyServer", proxy_addr)?;
        if !proxy_override.is_empty() {
            reg_set_string(&key, "ProxyOverride", proxy_override)?;
        }
        refresh_internet_settings()?;
    }
    #[cfg(not(windows))]
    {
        let _ = (proxy_addr, proxy_override, state_dir);
    }
    Ok(())
}

/// Restore the pre-proxy system state (see module contract). Idempotent.
pub fn restore(state_dir: &Path) -> crate::error::Result<()> {
    #[cfg(windows)]
    {
        match load_snapshot(state_dir)? {
            Some(saved) => {
                let key = open_inet_settings(KEY_QUERY_VALUE | KEY_SET_VALUE)?;
                reg_set_dword(&key, "ProxyEnable", saved.proxy_enable)?;
                for (name, value) in string_write_plan(&saved) {
                    match value {
                        Some(v) => reg_set_string(&key, name, &v)?,
                        None => reg_delete_string(&key, name)?,
                    }
                }
                remove_snapshot(state_dir)?;
                refresh_internet_settings()?;
            }
            None => {
                let key = open_inet_settings(KEY_SET_VALUE)?;
                reg_set_dword(&key, "ProxyEnable", 0)?;
                refresh_internet_settings()?;
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = state_dir;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_snapshot, remove_snapshot, save_snapshot, snapshot_path, Snapshot};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(windows)]
    use super::{snapshot_from, string_write_plan};

    /// Unique throwaway state_dir per test. Tests are registry-free: they
    /// only exercise the pure serde/file helpers, never HKCU.
    fn temp_state_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "mt-sysproxy-test-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> Snapshot {
        Snapshot {
            proxy_enable: 1,
            proxy_server: Some("127.0.0.1:10808".to_string()),
            proxy_override: Some("localhost;*.local;192.168.*".to_string()),
            autoconfig_url: Some("http://127.0.0.1:8081/proxy.pac".to_string()),
        }
    }

    #[test]
    fn snapshot_serde_roundtrip_incl_autoconfigurl_casing() {
        let s = sample();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"autoConfigURL\""), "casing lost: {}", json);
        assert!(json.contains("\"proxyEnable\""));
        assert!(json.contains("\"proxyServer\""));
        assert!(json.contains("\"proxyOverride\""));
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn snapshot_missing_fields_default() {
        let s: Snapshot = serde_json::from_str("{}").unwrap();
        assert_eq!(s.proxy_enable, 0);
        assert_eq!(s.proxy_server, None);
        assert_eq!(s.proxy_override, None);
        assert_eq!(s.autoconfig_url, None);

        let partial: Snapshot =
            serde_json::from_str(r#"{"proxyEnable":1,"proxyServer":"p:80"}"#).unwrap();
        assert_eq!(partial.proxy_enable, 1);
        assert_eq!(partial.proxy_server.as_deref(), Some("p:80"));
        assert_eq!(partial.proxy_override, None);
        assert_eq!(partial.autoconfig_url, None);
    }

    #[test]
    fn snapshot_absent_vs_empty_distinct() {
        let absent: Snapshot = serde_json::from_str(r#"{"proxyEnable":0}"#).unwrap();
        let empty: Snapshot =
            serde_json::from_str(r#"{"proxyEnable":0,"proxyServer":""}"#).unwrap();
        assert_eq!(absent.proxy_server, None);
        assert_eq!(empty.proxy_server, Some(String::new()));
        assert_ne!(absent, empty);
        // The distinction survives a round-trip.
        let back: Snapshot = serde_json::from_str(&serde_json::to_string(&empty).unwrap()).unwrap();
        assert_eq!(back, empty);
    }

    #[test]
    fn snapshot_file_save_load_remove_roundtrip() {
        let dir = temp_state_dir("save-load-remove");
        let s = sample();

        assert!(load_snapshot(&dir).unwrap().is_none(), "no snapshot yet");

        save_snapshot(&dir, &s).unwrap();
        let loaded = load_snapshot(&dir).unwrap().expect("snapshot saved");
        assert_eq!(loaded, s);

        // 0600-ish permissions on unix. (Binding lives inside the cfg block
        // so the windows-target test build has no unused variable.)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = snapshot_path(&dir);
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "snapshot file must be 0600");
        }

        remove_snapshot(&dir).unwrap();
        assert!(load_snapshot(&dir).unwrap().is_none());
        // Removing again is fine (idempotent).
        remove_snapshot(&dir).unwrap();

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_file_save_never_overwrites() {
        let dir = temp_state_dir("never-overwrite");
        let first = sample();
        save_snapshot(&dir, &first).unwrap();

        let second = Snapshot {
            proxy_enable: 7,
            proxy_server: Some("9.9.9.9:1".to_string()),
            proxy_override: None,
            autoconfig_url: None,
        };
        save_snapshot(&dir, &second).unwrap(); // Ok, but must keep `first`

        assert_eq!(load_snapshot(&dir).unwrap().unwrap(), first);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_snapshot_is_err() {
        let dir = temp_state_dir("corrupt");
        let path = snapshot_path(&dir);
        fs::write(&path, "{definitely not json").unwrap();
        assert!(load_snapshot(&dir).is_err(), "corrupt snapshot must Err");
        let _ = fs::remove_dir_all(&dir);
    }

    // Windows-only mapping tests: pure in-memory values, never the registry.
    #[test]
    #[cfg(windows)]
    fn snapshot_from_maps_registry_reads() {
        let s = snapshot_from(
            None,
            Some("10.0.0.2:8080".to_string()),
            None,
            Some("http://pac/proxy.pac".to_string()),
        );
        assert_eq!(s.proxy_enable, 0);
        assert_eq!(s.proxy_server.as_deref(), Some("10.0.0.2:8080"));
        assert_eq!(s.proxy_override, None);
        assert_eq!(s.autoconfig_url.as_deref(), Some("http://pac/proxy.pac"));
    }

    #[test]
    #[cfg(windows)]
    fn string_write_plan_maps_set_and_delete() {
        let s = Snapshot {
            proxy_enable: 3,
            proxy_server: Some("127.0.0.1:10808".to_string()),
            proxy_override: None,
            autoconfig_url: Some("http://pac/proxy.pac".to_string()),
        };
        let plan = string_write_plan(&s);
        assert_eq!(
            plan[0],
            ("ProxyServer", Some("127.0.0.1:10808".to_string()))
        );
        assert_eq!(plan[1], ("ProxyOverride", None));
        assert_eq!(
            plan[2],
            ("AutoConfigURL", Some("http://pac/proxy.pac".to_string()))
        );
    }
}
