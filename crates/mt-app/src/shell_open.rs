//! S23: open a folder in the OS file manager — the honest way, per platform.
//!
//! ## Why this module exists (the v0.15.0 "os error 50" regression)
//!
//! v0.15.0 spawned `explorer.exe <dir>` via `std::process::Command` with
//! DEFAULT (inherited) stdio. The app is a Windows GUI-subsystem binary —
//! it has no console, so its standard handles are NULL. Rust's spawn
//! passes the inherited handles with `STARTF_USESTDHANDLES`; the runtime
//! duplicates the null standard handles into the child, and
//! `CreateProcessW` then fails with `ERROR_NOT_SUPPORTED` — Windows
//! error 50, "The request is not supported". Console-subsystem parents
//! never hit this, which is exactly why every manual/CI probe passed
//! while the real GUI app failed on the user's machine.
//!
//! The fix is the canonical one every "reveal in file manager" feature
//! uses: **`ShellExecuteW`** — the shell's own open-verb path, which
//! never goes through the caller's handle table at all. The old
//! explorer spawn is kept as a defense-in-depth FALLBACK and now passes
//! [`Stdio::null`] for all three streams, which sidesteps the
//! inheritance trap even if the shell path is somehow unavailable.
//!
//! Failures are NEVER silent: the error String names every mechanism
//! that was tried and what each returned (the GUI shows it in the
//! Settings status line and the Logs tab).

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Open `dir` in the platform file manager.
///
/// Windows: `ShellExecuteW` (verb "open") — falls back to an
/// `explorer.exe` spawn with NUL stdio. macOS: `open(1)`. Everything
/// else: `xdg-open(1)`.
pub fn open_folder(dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        match win::shell_open(dir) {
            Ok(()) => Ok(()),
            Err(shell_err) => match spawn_file_manager(dir) {
                Ok(()) => Ok(()),
                Err(spawn_err) => Err(format!(
                    "could not open the log folder: {shell_err}; fallback: {spawn_err}"
                )),
            },
        }
    }
    #[cfg(not(windows))]
    {
        spawn_file_manager(dir).map_err(|e| format!("could not open the log folder: {e}"))
    }
}

/// The per-platform file-manager command with NUL stdio on ALL THREE
/// streams (never inherited: see the module docs for the GUI-subsystem
/// inheritance trap that produced os error 50).
///
/// The command is spawned fire-and-forget and only the SPAWN result is
/// inspected: `explorer.exe` exits non-zero even on success, and
/// `xdg-open` detaches from the real opener, so exit statuses carry no
/// signal on either platform.
pub fn file_manager_command(dir: &Path) -> Command {
    #[cfg(windows)]
    let mut c = {
        let mut c = Command::new("explorer.exe");
        c.arg(dir);
        c
    };
    #[cfg(target_os = "macos")]
    let mut c = {
        let mut c = Command::new("open");
        c.arg(dir);
        c
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut c = {
        let mut c = Command::new("xdg-open");
        c.arg(dir);
        c
    };
    // NUL stdio: inherited handles are the os-error-50 trap (module
    // docs) and the child's output is never read anyway.
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

fn spawn_file_manager(dir: &Path) -> Result<(), String> {
    let prog = file_manager_command(dir)
        .get_program()
        .to_string_lossy()
        .into_owned();
    match file_manager_command(dir).spawn() {
        Ok(_) => Ok(()),
        Err(e) => Err(describe_spawn_failure(&prog, &e, dir)),
    }
}

/// User-facing spawn-failure text. Pure (tested on every platform).
pub fn describe_spawn_failure(prog: &str, e: &io::Error, dir: &Path) -> String {
    if e.kind() == io::ErrorKind::NotFound {
        format!("{prog} is not installed (tried to open {})", dir.display())
    } else {
        format!("{prog}: {e} (opening {})", dir.display())
    }
}

/// Map a `ShellExecuteW` return code (≤ 32) to its documented meaning
/// (learn.microsoft.com, ShellExecuteW "Return value" table). Pure —
/// runs (and is tested) on every platform.
pub fn shell_execute_error_text(rc: isize) -> &'static str {
    match rc {
        0 => "the operating system is out of memory or resources",
        2 => "the specified file was not found",
        3 => "the specified path was not found",
        5 => "access was denied",
        8 => "not enough memory to complete the operation",
        11 => "the .exe file is invalid (non-Win32 .exe or error in .exe image)",
        26 => "a sharing violation occurred",
        27 => "the filename association is incomplete or invalid",
        28 => "the DDE transaction timed out",
        29 => "the DDE transaction failed",
        30 => "the DDE transaction is busy",
        31 => "there is no application associated with the given filename extension",
        32 => "the specified dynamic-link library was not found",
        _ => "unknown ShellExecuteW error",
    }
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    /// `ShellExecuteW(null, "open", <dir>, null, null, SW_SHOWNORMAL)`.
    /// Ok when the shell accepted the request (return value > 32).
    pub(super) fn shell_open(dir: &Path) -> Result<(), String> {
        let file: Vec<u16> = dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: [u16; 5] = [
            u16::from(b'o'),
            u16::from(b'p'),
            u16::from(b'e'),
            u16::from(b'n'),
            0,
        ];
        // SAFETY: every non-null pointer argument is a NUL-terminated
        // wide string that lives for the duration of the call (Win32
        // ShellExecuteW takes copies of what it needs before returning;
        // it has no lifetime requirements past the call itself).
        let rc = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            )
        } as isize;
        if rc > 32 {
            Ok(())
        } else {
            Err(format!(
                "ShellExecuteW failed: {}",
                super::shell_execute_error_text(rc)
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The platform file-manager command targets explorer.exe / open /
    /// xdg-open with the folder as its single argument.
    #[test]
    fn file_manager_command_targets_the_os_file_manager() {
        let c = file_manager_command(Path::new("/tmp/logs"));
        let prog = c.get_program().to_string_lossy().to_string();
        let expected = if cfg!(windows) {
            "explorer.exe"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        assert!(prog.ends_with(expected), "program {prog:?}");
        let args: Vec<_> = c.get_args().collect();
        assert_eq!(args, [std::ffi::OsStr::new("/tmp/logs")]);
    }

    /// Spawn failures name the program, the underlying error and the
    /// folder — never a bare os error code the user cannot act on.
    #[test]
    fn describe_spawn_failure_is_actionable() {
        let missing = io::Error::new(io::ErrorKind::NotFound, "no such program");
        let t = describe_spawn_failure("xdg-open", &missing, Path::new("/tmp/logs"));
        assert!(t.contains("xdg-open"), "{t}");
        assert!(t.contains("not installed"), "{t}");
        assert!(t.contains("/tmp/logs"), "{t}");

        let denied = io::Error::from_raw_os_error(13);
        let t = describe_spawn_failure("xdg-open", &denied, Path::new("/tmp/logs"));
        assert!(t.contains("xdg-open"), "{t}");
        assert!(t.contains("opening /tmp/logs"), "{t}");
    }

    /// The ShellExecuteW code table covers the documented return codes.
    #[test]
    fn shell_execute_error_table_is_documented() {
        assert!(shell_execute_error_text(0).contains("memory"));
        assert!(shell_execute_error_text(2).contains("file"));
        assert!(shell_execute_error_text(3).contains("path"));
        assert!(shell_execute_error_text(31).contains("no application"));
        assert!(shell_execute_error_text(7).contains("unknown"));
    }

    /// A spawnable file-manager command resolves to Ok on this host
    /// (Linux CI + the Xvfb rig have xdg-open; a missing opener reports
    /// a failure naming the folder instead of a bare os error).
    #[test]
    fn open_folder_never_panics() {
        // A directory we just created: xdg-open may be absent (rig) —
        // both outcomes are fine, only a panic is not.
        let dir = std::env::temp_dir().join("mt-shell-open-smoke");
        let _ = std::fs::create_dir_all(&dir);
        let _ = open_folder(&dir);
    }
}
