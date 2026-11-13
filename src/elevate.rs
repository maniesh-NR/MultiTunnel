//! Administrator-rights detection and elevation (stage S2 — implementer
//! agent S2-B).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `is_elevated() -> bool`: OpenProcessToken(GetCurrentProcess,
//!   TOKEN_QUERY) + GetTokenInformation(TokenElevation). Non-Windows: true.
//! - `relaunch_elevated(args: &[String]) -> Result<()>`: ShellExecuteW with
//!   verb "runas", file = current_exe, params = args joined; returns after
//!   spawning. Non-Windows: Ok.
//! - No unit tests possible for the real UAC path; the pure helpers
//!   [`join_args`] and [`to_wide`] are factored out and tested portably.

/// Join command-line arguments into a single ShellExecuteW parameter string:
/// arguments that are empty or contain whitespace are wrapped in double
/// quotes (Windows command-line style). Pure helper, portable tests.
#[cfg(any(windows, test))]
pub(crate) fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.is_empty() || a.chars().any(char::is_whitespace) {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Encode `s` as a null-terminated UTF-16 wide string — the representation
/// all Win32 `*W` string APIs expect (astral chars become surrogate pairs).
/// Pure helper, portable tests.
#[cfg(any(windows, test))]
pub(crate) fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // SAFETY: GetCurrentProcess returns a pseudo-handle for the current
    // process and cannot fail.
    let process: HANDLE = unsafe { GetCurrentProcess() };
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `process` is a valid (pseudo-)handle; `token` is a properly
    // aligned out-slot for the newly opened handle.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut elevation: TOKEN_ELEVATION = unsafe { std::mem::zeroed() };
    let mut returned: u32 = 0;
    // SAFETY: `token` is a valid handle from OpenProcessToken; the buffer is
    // a writable TOKEN_ELEVATION of exactly the declared size.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut TOKEN_ELEVATION as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    let elevated = ok != 0 && elevation.TokenIsElevated != 0;
    // SAFETY: `token` was opened successfully above; close it on this path.
    unsafe { CloseHandle(token) };
    elevated
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    true
}

#[cfg(windows)]
pub fn relaunch_elevated(args: &[String]) -> crate::error::Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe()
        .map_err(|e| crate::err!("failed to locate current executable: {}", e))?;
    let verb = to_wide("runas");
    let file = to_wide(&exe.to_string_lossy());
    let params = to_wide(&join_args(args));
    // SAFETY: every string argument is a null-terminated UTF-16 buffer we own
    // (or NULL for the unused directory); ShellExecuteW only reads them and
    // hands control to the shell, which shows the UAC prompt.
    let hinst = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(), // no owner window
            verb.as_ptr(),        // "runas" -> UAC elevation prompt
            file.as_ptr(),        // this executable
            params.as_ptr(),      // joined arguments
            std::ptr::null(),     // default working directory
            SW_SHOWNORMAL,        // show the new process normally
        )
    };
    // ShellExecuteW contract: any return value <= 32 signals failure.
    if (hinst as isize) <= 32 {
        return Err(crate::err!(
            "ShellExecuteW(\"runas\") failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn relaunch_elevated(args: &[String]) -> crate::error::Result<()> {
    let _ = args;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{join_args, to_wide};

    #[test]
    fn to_wide_null_terminates() {
        assert_eq!(to_wide(""), vec![0u16]);
        assert_eq!(
            to_wide("runas"),
            vec!['r' as u16, 'u' as u16, 'n' as u16, 'a' as u16, 's' as u16, 0]
        );
    }

    #[test]
    fn to_wide_handles_utf16_surrogate_pair() {
        // U+1F600 is outside the BMP: it must encode as a UTF-16 surrogate
        // pair (high 0xD83D, low 0xDE00), followed by the terminator.
        assert_eq!(to_wide("\u{1F600}"), vec![0xD83D, 0xDE00, 0]);
    }

    #[test]
    fn join_args_plain_and_quoted() {
        assert_eq!(join_args(&[]), "");
        assert_eq!(join_args(&["run".to_string()]), "run");
        assert_eq!(
            join_args(&["--config".to_string(), "my file.json".to_string()]),
            "--config \"my file.json\""
        );
        // Empty argument stays a distinct argument via quotes.
        assert_eq!(join_args(&["".to_string()]), "\"\"");
        // Any whitespace (not just spaces) triggers quoting.
        assert_eq!(
            join_args(&["a\tb".to_string(), "c".to_string()]),
            "\"a\tb\" c"
        );
    }
}
