//! WinINET system proxy control (stage S2 — implementer agent S2-B).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `set(proxy_addr: &str, proxy_override: &str, state_dir: &Path) -> Result<()>`
//!   Windows (winreg, HKCU\Software\Microsoft\Windows\CurrentVersion\Internet
//!   Settings): save snapshot {ProxyEnable, ProxyServer, ProxyOverride,
//!   AutoConfigURL} to state_dir/sysproxy.json (0600-ish) if absent; then set
//!   ProxyEnable=1, ProxyServer=proxy_addr, ProxyOverride=override; notify via
//!   InternetSetOptionW(NULL, INTERNET_OPTION_SETTINGS_CHANGED/39) and
//!   (INTERNET_OPTION_REFRESH/37).
//! - `restore(state_dir)` restores snapshot values (deletes leftover fields),
//!   removes the snapshot file, refresh. Without snapshot: ProxyEnable=0.
//! - Non-Windows: both are no-op Ok.
//! - Windows-only unit tests (cfg(windows)): snapshot round-trip with a temp
//!   dir; they MUST NOT touch the real HKCU settings — use a fake state_dir
//!   and only test the snapshot encode/decode + field mapping logic you
//!   factor out, never write the real registry from tests.
pub fn set(_proxy_addr: &str, _proxy_override: &str, _state_dir: &std::path::Path) -> crate::error::Result<()> {
    Ok(())
}
