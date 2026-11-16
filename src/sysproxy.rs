//! WinINET system proxy control (stage S2 — implementer agent S2-B).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `set(proxy_addr: &str, proxy_override: &str, state_dir: &std::path::Path) -> crate::error::Result<()>`
//!   Windows (winreg, HKCU\Software\Microsoft\Windows\CurrentVersion\Internet
//!   Settings): save snapshot {ProxyEnable, ProxyServer, ProxyOverride,
//!   AutoConfigURL} to state_dir/sysproxy.json (0600-ish) if absent; then set
//!   ProxyEnable=1, ProxyServer=proxy_addr, ProxyOverride=override — the
//!   override is written VERBATIM, an empty string meaning "no local
//!   exceptions" (WinINET semantics; S2-REV-C P2-8/10 — it is never
//!   skipped); notify via InternetSetOptionW(NULL,
//!   INTERNET_OPTION_SETTINGS_CHANGED/39) and (INTERNET_OPTION_REFRESH/37).
//! - `restore(state_dir)` restores snapshot values (deletes fields that were
//!   absent pre-change), removes the snapshot file, refresh. Without
//!   snapshot: ProxyEnable=0. Values snapshotted as Opaque (present
//!   pre-change with an unexpected registry type) are NEVER deleted or
//!   overwritten (S2-REV-C P2-5 — see [`SnapValue`]).
//! - Non-Windows: both are no-op Ok.
//! - Unit tests MUST NOT touch the real HKCU settings: all registry-free
//!   logic is factored into the pure helpers below ([`Snapshot`] serde, the
//!   [`SnapValue`] tri-state, the [`str_action`]/[`enable_action`] restore
//!   mappings, the [`set_string_plan`] verbatim-override plan, and the
//!   snapshot-file save/load/remove trio) and every test uses a fake
//!   temporary state_dir. The registry itself is only ever exercised by the
//!   real `set`/`restore` code paths, never from tests.

use std::path::Path;

/// File the pre-change proxy state is snapshotted into, inside `state_dir`.
#[cfg(any(windows, test))]
const SNAPSHOT_FILE: &str = "sysproxy.json";

/// Tri-state of one snapshotted registry value (S2-REV-C P2-5). A plain
/// `Option` could not tell "absent" from "present but with an unexpected
/// type": a REG_EXPAND_SZ or REG_BINARY `ProxyServer` used to snapshot as
/// ABSENT (the typed read simply failed), and `restore` then DELETED the
/// user's value. The three states:
///
/// - [`Absent`](SnapValue::Absent) — the value does not exist;
///   `restore` deletes it (strings) / writes 0 (`ProxyEnable`), exactly as
///   before S5.
/// - [`Value`](SnapValue::Value) — present with the expected type
///   (REG_SZ / REG_DWORD) and a decodable payload; `restore` writes the
///   snapshotted data back.
/// - [`Opaque`](SnapValue::Opaque) — present with an UNEXPECTED type (or
///   unreadable); `restore` must leave the user's data completely
///   untouched — never deleted, never overwritten. The registry kind is
///   kept for diagnostics.
///
/// JSON shape (backward compatible with pre-S5 snapshots): `null`, the
/// plain value, or `{"opaqueKind": "REG_BINARY"}`.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum SnapValue<T> {
    #[default]
    Absent,
    Value(T),
    Opaque {
        #[serde(rename = "opaqueKind")]
        kind: String,
    },
}

/// The saved pre-change proxy state. Serde keys are `proxyEnable`,
/// `proxyServer`, `proxyOverride`, `autoConfigURL` (note the exact
/// `autoConfigURL` casing WinINET uses). Every field is a tri-state
/// [`SnapValue`]; `Absent`, `Value("")` and `Opaque` stay distinct through
/// the JSON round-trip. Missing JSON fields deserialize to `Absent` so a
/// hand-edited snapshot still restores.
///
/// Mid-set half-applied window: `set` writes `ProxyEnable=1` BEFORE
/// `ProxyServer`/`ProxyOverride`, and registry writes are not
/// transactional — a crash between them leaves the proxy "enabled" with the
/// previous server value. The snapshot saved just before the first write
/// makes the next `restore()` (a manual disconnect, or the stale-snapshot
/// self-heal at the start of the next `connect()`) roll that half-applied
/// state back to the true pre-change values.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Snapshot {
    #[serde(default, rename = "proxyEnable")]
    proxy_enable: SnapValue<u32>,
    #[serde(default, rename = "proxyServer")]
    proxy_server: SnapValue<String>,
    #[serde(default, rename = "proxyOverride")]
    proxy_override: SnapValue<String>,
    #[serde(default, rename = "autoConfigURL")]
    autoconfig_url: SnapValue<String>,
}

/// Build a snapshot from (tri-state) registry reads. Pure mapping:
/// testable on in-memory values only, never the real registry.
#[cfg(any(windows, test))]
fn snapshot_from(
    proxy_enable: SnapValue<u32>,
    proxy_server: SnapValue<String>,
    proxy_override: SnapValue<String>,
    autoconfig_url: SnapValue<String>,
) -> Snapshot {
    Snapshot {
        proxy_enable,
        proxy_server,
        proxy_override,
        autoconfig_url,
    }
}

/// What `restore` does with one snapshotted REG_SZ value (pure mapping).
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq)]
enum StrAction {
    /// `Value` → write the snapshotted string back.
    Write(String),
    /// `Absent` → delete the value (it was absent before the change).
    Delete,
    /// `Opaque` → present with an unexpected type: NEVER delete or
    /// overwrite — leave the user's data untouched (S2-REV-C P2-5).
    Leave,
}

/// Tri-state → restore action for one REG_SZ value (pure, host-testable).
#[cfg(any(windows, test))]
fn str_action(v: &SnapValue<String>) -> StrAction {
    match v {
        SnapValue::Absent => StrAction::Delete,
        SnapValue::Value(s) => StrAction::Write(s.clone()),
        SnapValue::Opaque { .. } => StrAction::Leave,
    }
}

/// What `restore` does with the snapshotted `ProxyEnable` DWORD (pure
/// mapping). There is no delete: `Absent` writes 0 exactly as before S5
/// (WinINET treats a missing enable as "disabled", and deleting the value
/// was never the pre-S5 behavior).
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq)]
enum EnableAction {
    /// Write this DWORD (`Value(n)` → n; `Absent` → 0). `Opaque` ALSO
    /// writes 0 (S5-REV-C P1a): a type-mismatched ProxyEnable was already
    /// overwritten by `set`'s unconditional DWORD write — the user's
    /// original bytes are unrecoverable at restore time, so "disabled"
    /// is the only fail-safe choice (v0.1.0 behavior, kept).
    Write(u32),
}

/// Tri-state → restore action for `ProxyEnable` (pure, host-testable).
#[cfg(any(windows, test))]
fn enable_action(v: &SnapValue<u32>) -> EnableAction {
    match v {
        SnapValue::Absent => EnableAction::Write(0),
        SnapValue::Value(n) => EnableAction::Write(*n),
        // S5-REV-C P1a: never Leave — leaving OUR DWORD 1 in place after
        // disconnect would silently blackhole every WinINET consumer
        // through the now-dead local inbound.
        SnapValue::Opaque { .. } => EnableAction::Write(0),
    }
}

/// Write-back plan for the three REG_SZ values of a snapshot (pure,
/// host-testable): name → [`StrAction`] per tri-state.
#[cfg(any(windows, test))]
fn string_write_plan(s: &Snapshot) -> [(&'static str, StrAction); 3] {
    [
        ("ProxyServer", str_action(&s.proxy_server)),
        ("ProxyOverride", str_action(&s.proxy_override)),
        ("AutoConfigURL", str_action(&s.autoconfig_url)),
    ]
}

/// The REG_SZ writes `set` performs (pure, host-testable): `ProxyServer`
/// and — ALWAYS — `ProxyOverride`. The override is used VERBATIM: an empty
/// string is a legal WinINET state meaning "no local exceptions"
/// (S2-REV-C P2-8/10; skipping an empty override would leave the previous
/// session's exceptions in force) and matches the config side, which also
/// uses the value verbatim.
#[cfg(any(windows, test))]
fn set_string_plan(proxy_addr: &str, proxy_override: &str) -> [(&'static str, String); 2] {
    [
        ("ProxyServer", proxy_addr.to_owned()),
        ("ProxyOverride", proxy_override.to_owned()),
    ]
}

/// Names of the REG_SZ values `set` unconditionally overwrites that are
/// `Opaque` in `snap` (present with an unexpected registry type). `set`
/// REFUSES to run when this is non-empty (S5-REV-C P1b): writing our
/// REG_SZ would destroy the user's data and a later restore could not
/// recover it (restore only LEAVES Opaque values — it cannot re-create
/// them). `AutoConfigURL` is deliberately not listed: `set` never writes
/// it, so an Opaque AutoConfigURL is safe to snapshot and leave.
#[cfg(any(windows, test))]
fn opaque_overwritten_string_names(s: &Snapshot) -> Vec<&'static str> {
    [
        ("ProxyServer", &s.proxy_server),
        ("ProxyOverride", &s.proxy_override),
    ]
    .into_iter()
    .filter(|(_, v)| matches!(v, SnapValue::Opaque { .. }))
    .map(|(name, _)| name)
    .collect()
}

#[cfg(any(windows, test))]
fn snapshot_path(state_dir: &Path) -> std::path::PathBuf {
    state_dir.join(SNAPSHOT_FILE)
}

/// Serialize + write the snapshot to `state_dir/sysproxy.json` with
/// keep-first semantics: an existing snapshot is NEVER overwritten (the
/// first saved state is the one `restore` must return to, e.g. after a
/// crashed run). The write itself is atomic — temp + fsync + rename
/// (S5-REV-C P2-2): a crash can no longer leave a truncated snapshot,
/// which would make every future restore a silent no-op. Mode is 0600
/// on unix; on Windows the file lands in the user profile with default
/// ACLs. The exists-check + rename pair is safe under the supervisor's
/// op_lock single-writer invariant.
#[cfg(any(windows, test))]
fn save_snapshot(state_dir: &Path, snap: &Snapshot) -> crate::error::Result<()> {
    let path = snapshot_path(state_dir);
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(state_dir)?;
    let body = serde_json::to_string_pretty(snap)?;
    crate::fsutil::write_private_atomic(&path, &body)
        .map_err(|e| crate::err!("failed to write proxy snapshot {}: {}", path.display(), e))
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

/// Human-readable kind of a raw registry value, kept inside
/// [`SnapValue::Opaque`] for diagnostics (e.g. "REG_EXPAND_SZ").
#[cfg(windows)]
fn kind_of(raw: &winreg::RegValue<'_>) -> String {
    format!("{:?}", raw.vtype)
}

/// Read one registry string value into its tri-state (Windows; expected
/// type REG_SZ): `NotFound` → `Absent`; REG_SZ + decodable → `Value`;
/// ANYTHING else — an unexpected type such as REG_EXPAND_SZ or REG_BINARY,
/// an undecodable payload, or an unreadable value — → `Opaque` with the
/// kind kept for logging. A present-but-unreadable value must never
/// snapshot as absent: restore would delete the user's data
/// (S2-REV-C P2-5). Note winreg's `String::from_reg_value` also accepts
/// REG_EXPAND_SZ/REG_MULTI_SZ, so the type is gated explicitly.
#[cfg(windows)]
fn snap_string(key: &RegKey, name: &str) -> SnapValue<String> {
    use winreg::types::FromRegValue as _;
    match key.get_raw_value(name) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SnapValue::Absent,
        Err(e) => SnapValue::Opaque {
            kind: format!("read error: {e}"),
        },
        Ok(raw) => {
            if raw.vtype == winreg::enums::REG_SZ {
                match String::from_reg_value(&raw) {
                    Ok(s) => SnapValue::Value(s),
                    Err(_) => SnapValue::Opaque {
                        kind: kind_of(&raw),
                    },
                }
            } else {
                SnapValue::Opaque {
                    kind: kind_of(&raw),
                }
            }
        }
    }
}

/// Read one registry DWORD value into its tri-state (Windows; expected
/// type REG_DWORD). Same rules as [`snap_string`].
#[cfg(windows)]
fn snap_dword(key: &RegKey, name: &str) -> SnapValue<u32> {
    use winreg::types::FromRegValue as _;
    match key.get_raw_value(name) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SnapValue::Absent,
        Err(e) => SnapValue::Opaque {
            kind: format!("read error: {e}"),
        },
        Ok(raw) => {
            if raw.vtype == winreg::enums::REG_DWORD {
                match u32::from_reg_value(&raw) {
                    Ok(v) => SnapValue::Value(v),
                    Err(_) => SnapValue::Opaque {
                        kind: kind_of(&raw),
                    },
                }
            } else {
                SnapValue::Opaque {
                    kind: kind_of(&raw),
                }
            }
        }
    }
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
/// `proxy_override` is written VERBATIM — including when empty, which
/// means "no local exceptions" in WinINET semantics (S2-REV-C P2-8/10);
/// it is never skipped, so a previous session's exceptions cannot linger.
pub fn set(proxy_addr: &str, proxy_override: &str, state_dir: &Path) -> crate::error::Result<()> {
    #[cfg(windows)]
    {
        let key = open_inet_settings(KEY_QUERY_VALUE | KEY_SET_VALUE)?;
        let current = snapshot_from(
            snap_dword(&key, "ProxyEnable"),
            snap_string(&key, "ProxyServer"),
            snap_string(&key, "ProxyOverride"),
            snap_string(&key, "AutoConfigURL"),
        );
        // S5-REV-C P1b: refuse rather than destroy exotic-typed user data
        // (nothing has been written yet — no snapshot is saved either).
        let opaque = opaque_overwritten_string_names(&current);
        if !opaque.is_empty() {
            return Err(crate::err!(
                "registry value(s) {} present with an unexpected type — refusing \
                 to overwrite user data; fix them via Internet Options > \
                 Connections > LAN settings and retry",
                opaque.join(", ")
            ));
        }
        // Save the pre-change state once; never clobber an existing snapshot.
        save_snapshot(state_dir, &current)?;

        // Mid-set half-applied window (see the Snapshot docs):
        // ProxyEnable=1 is written BEFORE ProxyServer/ProxyOverride and the
        // writes are not transactional — a crash in between leaves the proxy
        // "enabled" with the previous server value. The snapshot saved just
        // above lets the next restore() (disconnect, or the next connect()'s
        // stale-snapshot self-heal) roll that state back.
        reg_set_dword(&key, "ProxyEnable", 1)?;
        for (name, value) in set_string_plan(proxy_addr, proxy_override) {
            reg_set_string(&key, name, &value)?;
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
/// Values snapshotted as Opaque are left completely untouched — never
/// deleted, never overwritten (S2-REV-C P2-5).
pub fn restore(state_dir: &Path) -> crate::error::Result<()> {
    #[cfg(windows)]
    {
        match load_snapshot(state_dir)? {
            Some(saved) => {
                let key = open_inet_settings(KEY_QUERY_VALUE | KEY_SET_VALUE)?;
                // Opaque or not, restore ALWAYS writes a DWORD here
                // (S5-REV-C P1a): the pre-S5 fail-safe "disabled" unless a
                // real value was captured.
                let EnableAction::Write(enable) = enable_action(&saved.proxy_enable);
                reg_set_dword(&key, "ProxyEnable", enable)?;
                for (name, action) in string_write_plan(&saved) {
                    match action {
                        StrAction::Write(v) => reg_set_string(&key, name, &v)?,
                        StrAction::Delete => reg_delete_string(&key, name)?,
                        // Opaque (present, unexpected type pre-change):
                        // NEVER delete or overwrite (S2-REV-C P2-5).
                        StrAction::Leave => {}
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
    use super::{
        enable_action, load_snapshot, opaque_overwritten_string_names, remove_snapshot,
        save_snapshot, set_string_plan, snapshot_from, snapshot_path, str_action,
        string_write_plan, EnableAction, SnapValue, Snapshot, StrAction,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
            proxy_enable: SnapValue::Value(1),
            proxy_server: SnapValue::Value("127.0.0.1:10808".to_string()),
            proxy_override: SnapValue::Value("localhost;*.local;192.168.*".to_string()),
            autoconfig_url: SnapValue::Value("http://127.0.0.1:8081/proxy.pac".to_string()),
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
        assert_eq!(s.proxy_enable, SnapValue::Absent);
        assert_eq!(s.proxy_server, SnapValue::Absent);
        assert_eq!(s.proxy_override, SnapValue::Absent);
        assert_eq!(s.autoconfig_url, SnapValue::Absent);

        let partial: Snapshot =
            serde_json::from_str(r#"{"proxyEnable":1,"proxyServer":"p:80"}"#).unwrap();
        assert_eq!(partial.proxy_enable, SnapValue::Value(1));
        assert_eq!(partial.proxy_server, SnapValue::Value("p:80".to_string()));
        assert_eq!(partial.proxy_override, SnapValue::Absent);
        assert_eq!(partial.autoconfig_url, SnapValue::Absent);
    }

    #[test]
    fn snapshot_absent_vs_empty_distinct() {
        let absent: Snapshot = serde_json::from_str(r#"{"proxyEnable":0}"#).unwrap();
        let empty: Snapshot =
            serde_json::from_str(r#"{"proxyEnable":0,"proxyServer":""}"#).unwrap();
        assert_eq!(absent.proxy_server, SnapValue::Absent);
        assert_eq!(empty.proxy_server, SnapValue::Value(String::new()));
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
            proxy_enable: SnapValue::Value(7),
            proxy_server: SnapValue::Value("9.9.9.9:1".to_string()),
            proxy_override: SnapValue::Absent,
            autoconfig_url: SnapValue::Absent,
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

    /// Tri-state serde (S2-REV-C P2-5): the three states map to distinct
    /// JSON shapes, round-trip, and stay distinct from each other; a
    /// snapshot written by a pre-S5 build (plain nulls/strings) parses
    /// into the tri-states unchanged.
    #[test]
    fn snapvalue_serde_tri_state_and_legacy_format() {
        let absent: SnapValue<String> = serde_json::from_str("null").unwrap();
        assert_eq!(absent, SnapValue::Absent);
        let value: SnapValue<String> = serde_json::from_str("\"p:80\"").unwrap();
        assert_eq!(value, SnapValue::Value("p:80".to_string()));
        let opaque: SnapValue<String> =
            serde_json::from_str(r#"{"opaqueKind":"REG_BINARY"}"#).unwrap();
        assert_eq!(
            opaque,
            SnapValue::Opaque {
                kind: "REG_BINARY".to_string()
            }
        );
        assert_ne!(absent, value);
        assert_ne!(value, opaque);
        assert_ne!(absent, opaque);

        // Serialization: Absent -> null (legacy shape), Value -> plain,
        // Opaque -> marker object with the kind detail.
        assert_eq!(
            serde_json::to_string(&SnapValue::<String>::Absent).unwrap(),
            "null"
        );
        assert_eq!(serde_json::to_string(&value).unwrap(), r#""p:80""#);
        assert_eq!(
            serde_json::to_string(&opaque).unwrap(),
            r#"{"opaqueKind":"REG_BINARY"}"#
        );

        // DWORD tri-state: number -> Value.
        let v: SnapValue<u32> = serde_json::from_str("7").unwrap();
        assert_eq!(v, SnapValue::Value(7));

        // Malformed marker objects / wrong types are errors, never a
        // silent Absent (that silence is exactly the P2-5 bug).
        assert!(
            serde_json::from_str::<SnapValue<String>>(r#"{"nonsense":1}"#).is_err(),
            "unknown object must not parse as some state"
        );
        assert!(serde_json::from_str::<SnapValue<u32>>("true").is_err());

        // Legacy pre-S5 snapshot file: nulls and plain strings everywhere.
        let legacy: Snapshot = serde_json::from_str(
            r#"{"proxyEnable":0,"proxyServer":null,"proxyOverride":"localhost","autoConfigURL":null}"#,
        )
        .unwrap();
        assert_eq!(legacy.proxy_enable, SnapValue::Value(0));
        assert_eq!(legacy.proxy_server, SnapValue::Absent);
        assert_eq!(
            legacy.proxy_override,
            SnapValue::Value("localhost".to_string())
        );
        assert_eq!(legacy.autoconfig_url, SnapValue::Absent);
    }

    /// Restore semantics per tri-state (S2-REV-C P2-5, pure): Absent →
    /// delete, Value → write back, Opaque → leave untouched — and the
    /// DWORD variant (Absent → 0, Value → n, Opaque → leave).
    #[test]
    fn restore_actions_per_tri_state() {
        // REG_SZ values.
        assert_eq!(str_action(&SnapValue::Absent), StrAction::Delete);
        assert_eq!(
            str_action(&SnapValue::Value("127.0.0.1:1080".to_string())),
            StrAction::Write("127.0.0.1:1080".to_string())
        );
        assert_eq!(
            str_action(&SnapValue::Value(String::new())),
            StrAction::Write(String::new())
        );
        for kind in ["REG_EXPAND_SZ", "REG_BINARY", "read error: denied"] {
            assert_eq!(
                str_action(&SnapValue::Opaque {
                    kind: kind.to_string()
                }),
                StrAction::Leave,
                "opaque ({kind}) must be left untouched"
            );
        }

        // ProxyEnable DWORD: Absent keeps the pre-S5 "write 0" behavior,
        // and Opaque ALSO writes 0 — the P1a fail-safe (set already
        // overwrote the exotic-typed value with our DWORD; "disabled" is
        // the only safe restoration, v0.1.0 behavior kept).
        assert_eq!(enable_action(&SnapValue::Absent), EnableAction::Write(0));
        assert_eq!(enable_action(&SnapValue::Value(1)), EnableAction::Write(1));
        assert_eq!(
            enable_action(&SnapValue::Opaque {
                kind: "REG_SZ".to_string()
            }),
            EnableAction::Write(0)
        );
    }

    /// The full string write-back plan maps each field independently:
    /// one of each state per name (pure, host-testable).
    #[test]
    fn string_write_plan_maps_each_tri_state_independently() {
        let s = snapshot_from(
            SnapValue::Value(1),
            SnapValue::Value("proxy.example:3128".to_string()),
            SnapValue::Absent,
            SnapValue::Opaque {
                kind: "REG_BINARY".to_string(),
            },
        );
        let plan = string_write_plan(&s);
        assert_eq!(
            plan[0],
            (
                "ProxyServer",
                StrAction::Write("proxy.example:3128".to_string())
            )
        );
        assert_eq!(plan[1], ("ProxyOverride", StrAction::Delete));
        assert_eq!(
            plan[2],
            (
                "AutoConfigURL",
                StrAction::Leave // Opaque: never deleted nor overwritten
            )
        );
    }

    /// The P2-5 regression itself: an all-opaque snapshot (every value
    /// present with an unexpected type) produces an all-Leave plan — no
    /// delete, no overwrite — and the Opaque markers survive the snapshot
    /// file round-trip so restore keeps its promise after a restart.
    #[test]
    fn all_opaque_snapshot_never_deletes_or_overwrites() {
        let opaque_str = |kind: &str| SnapValue::Opaque {
            kind: kind.to_string(),
        };
        let s = snapshot_from(
            SnapValue::Opaque {
                kind: "REG_SZ".to_string(),
            },
            opaque_str("REG_EXPAND_SZ"),
            opaque_str("REG_BINARY"),
            opaque_str("REG_QWORD"),
        );
        for (_, action) in string_write_plan(&s) {
            assert_eq!(action, StrAction::Leave, "opaque must stay untouched");
        }
        assert_eq!(enable_action(&s.proxy_enable), EnableAction::Write(0));

        // File round-trip keeps the Opaque markers (kind detail included).
        let dir = temp_state_dir("opaque-roundtrip");
        save_snapshot(&dir, &s).unwrap();
        let back = load_snapshot(&dir).unwrap().expect("snapshot saved");
        assert_eq!(back, s);
        assert_eq!(
            back.proxy_server,
            SnapValue::Opaque {
                kind: "REG_EXPAND_SZ".to_string()
            }
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// S5-REV-C P1b: `set` must refuse when a REG_SZ value it overwrites
    /// (ProxyServer / ProxyOverride) is Opaque — the refusal decision is
    /// the pure helper `opaque_overwritten_string_names`.
    #[test]
    fn set_refusal_only_triggers_for_overwritten_opaque_values() {
        let opaque = |kind: &str| SnapValue::Opaque {
            kind: kind.to_string(),
        };
        // Clean machine: nothing refused.
        let clean = snapshot_from(
            SnapValue::Value(0),
            SnapValue::Value("proxy:8080".into()),
            SnapValue::Absent,
            SnapValue::Absent,
        );
        assert!(opaque_overwritten_string_names(&clean).is_empty());

        // ProxyServer exotic-typed: refused, named.
        let mut s = clean.clone();
        s.proxy_server = opaque("REG_BINARY");
        assert_eq!(opaque_overwritten_string_names(&s), vec!["ProxyServer"]);

        // ProxyOverride exotic-typed too: both named, ProxyServer first.
        s.proxy_override = opaque("REG_EXPAND_SZ");
        assert_eq!(
            opaque_overwritten_string_names(&s),
            vec!["ProxyServer", "ProxyOverride"]
        );

        // AutoConfigURL Opaque alone does NOT refuse (set never writes it)
        // and ProxyEnable Opaque alone does NOT refuse (set writes a DWORD
        // over it; restore fail-safes to 0 — P1a).
        let mut only_ac = clean.clone();
        only_ac.autoconfig_url = opaque("REG_QWORD");
        assert!(opaque_overwritten_string_names(&only_ac).is_empty());
        let mut only_enable = clean;
        only_enable.proxy_enable = SnapValue::Opaque {
            kind: "REG_SZ".to_string(),
        };
        assert!(opaque_overwritten_string_names(&only_enable).is_empty());
    }

    /// S5-REV-C P2-2: save_snapshot is atomic — no `.tmp-*` residue next to
    /// a saved snapshot, and the content is complete.
    #[test]
    fn save_snapshot_leaves_no_temp_residue() {
        let dir = temp_state_dir("atomic-save");
        let s = snapshot_from(
            SnapValue::Value(1),
            SnapValue::Value("p".into()),
            SnapValue::Absent,
            SnapValue::Absent,
        );
        save_snapshot(&dir, &s).unwrap();
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["sysproxy.json".to_string()], "no temp residue");
        assert!(load_snapshot(&dir).unwrap().is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    /// `set` writes ProxyOverride VERBATIM (S2-REV-C P2-8/10): the plan
    /// always contains the override — an empty string stays an explicit
    /// write meaning "no local exceptions", and is never skipped (which
    /// would leave the previous session's exceptions in force).
    #[test]
    fn set_plan_writes_override_verbatim_even_when_empty() {
        let plan = set_string_plan("127.0.0.1:10809", "");
        assert_eq!(plan[0], ("ProxyServer", "127.0.0.1:10809".to_string()));
        assert_eq!(plan[1], ("ProxyOverride", String::new()));

        let plan = set_string_plan("127.0.0.1:10809", "localhost;192.168.*");
        assert_eq!(
            plan[1],
            ("ProxyOverride", "localhost;192.168.*".to_string())
        );
    }
}
