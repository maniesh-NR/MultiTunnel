//! Atomic private-file writes — S2-REV-C P2-6 fix (staged for S5).
//!
//! Contract (relied on by `main::write_private` and
//! `supervisor::write_private`):
//!
//! 1. `write_private_atomic(path, contents)` never leaves `path` in a
//!    partial state: contents go to a sibling temp file
//!    (`<name>.tmp-<pid>`), are fsynced, then the temp file is renamed over
//!    the destination. `std::fs::rename` replaces an existing destination
//!    atomically on both Linux and same-volume Windows files.
//! 2. Mode 0600 on unix (the file can embed the VLESS credential); on
//!    Windows the default user-profile ACL applies (same as before S5).
//! 3. On ANY failure the temp file is removed and the original destination
//!    is left untouched.
//! 4. The parent directory must already exist (callers create it with
//!    `create_dir_all`); a missing parent surfaces as the open error.
//! 5. A stale same-pid temp file from a crashed earlier run is removed and
//!    retried exactly once.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Write `contents` to `path` atomically (see module contract).
pub fn write_private_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let tmp = temp_sibling(path);
    let result = write_and_rename(&tmp, path, contents.as_bytes());
    if result.is_err() {
        // Contract #3: never leave the temp file behind on failure.
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_and_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = match open_exclusive(tmp) {
        Ok(f) => f,
        // Contract #5: a stale temp from a crashed same-pid run — remove
        // once and retry; a second collision is a real error.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(tmp);
            open_exclusive(tmp)?
        }
        Err(e) => return Err(e),
    };
    f.write_all(bytes)?;
    // Flush to disk BEFORE the rename makes the content visible — the
    // rename is the atomic publication point.
    f.sync_all()?;
    drop(f);
    fs::rename(tmp, path)
}

fn open_exclusive(tmp: &Path) -> io::Result<fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(tmp)
    }
    #[cfg(not(unix))]
    {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(tmp)
    }
}

/// `<dir>/<name>.tmp-<pid>` next to `path` (same volume → atomic rename).
fn temp_sibling(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    name.push_str(".tmp-");
    name.push_str(&std::process::id().to_string());
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "mt-fsutil-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("scratch dir");
        d
    }

    #[test]
    fn writes_exact_contents_and_leaves_no_temp() {
        let d = scratch("write");
        let p = d.join("f.json");
        write_private_atomic(&p, "hello").expect("write");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hello");
        let leftovers: Vec<std::ffi::OsString> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            leftovers,
            vec![std::ffi::OsString::from("f.json")],
            "no temp left behind"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn overwrites_existing_file_atomically() {
        let d = scratch("over");
        let p = d.join("f.json");
        std::fs::write(&p, "OLD-CONTENTS-LONGER-THAN-NEW").unwrap();
        write_private_atomic(&p, "new").expect("rewrite");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn missing_parent_is_an_error_and_touches_nothing() {
        let d = scratch("missing");
        let p = d.join("no-such-dir").join("f.json");
        assert!(write_private_atomic(&p, "x").is_err());
        assert!(!p.exists());
        assert!(std::fs::read_dir(&d).unwrap().next().is_none());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rename_failure_cleans_up_temp_and_keeps_original() {
        let d = scratch("rename-fail");
        // Destination is a DIRECTORY: the temp file is written fine, but the
        // rename fails → the failure/cleanup path must remove the temp.
        let dir_dest = d.join("i-am-a-dir");
        std::fs::create_dir(&dir_dest).unwrap();
        assert!(write_private_atomic(&dir_dest, "x").is_err());
        let leftovers: Vec<std::ffi::OsString> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            leftovers,
            vec![std::ffi::OsString::from("i-am-a-dir")],
            "temp cleaned up"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[cfg(unix)]
    #[test]
    fn unix_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("mode");
        let p = d.join("secret.json");
        write_private_atomic(&p, "credential").expect("write");
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "private file mode");
        std::fs::remove_dir_all(&d).ok();
    }
}
