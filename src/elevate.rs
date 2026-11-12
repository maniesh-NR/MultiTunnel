//! Administrator-rights detection and elevation (stage S2 — implementer
//! agent S2-B).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `is_elevated() -> bool`: OpenProcessToken(GetCurrentProcess,
//!   TOKEN_QUERY) + GetTokenInformation(TokenElevation). Non-Windows: true.
//! - `relaunch_elevated(args: &[String]) -> Result<()>`: ShellExecuteW with
//!   verb "runas", file = current_exe, params = args joined; returns after
//!   spawning. Non-Windows: Ok.
//! - No unit tests possible for the real UAC path; test the args-joining
//!   helper if you factor one out (pure function).
pub fn is_elevated() -> bool {
    true
}
