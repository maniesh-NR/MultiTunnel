//! `ProfileStore` — profiles.json: load/save/upsert/remove/set_active/list.
//!
//! Semantics per lib.rs NOTES #11/#12:
//! - missing file → empty store with `version: 1`;
//! - `save` normalizes `version == 0` → 1 and pins the exact frozen JSON
//!   shape (`version` / `active` / `profiles`);
//! - `upsert` dedupes by `(protocol, address, port, identity)` and keeps the
//!   EXISTING id so references stay stable; returns the effective id;
//! - `update` (S19) replaces one profile BY ID in place — the id, the
//!   list position and the `active` pointer are all preserved, and NO
//!   dedupe runs (an explicit edit may legitimately collide with another
//!   entry; merging is the user's call, not ours);
//! - `remove` clears `active` when it pointed at the removed id;
//! - `set_active(Some(id))` is rejected (false, no change) for unknown ids,
//!   `set_active(None)` always succeeds.
//!
//! Save is atomic: a faithful port of the root crate's
//! `src/fsutil.rs::write_private_atomic` (S2-REV-C P2-6) — sibling temp
//! file `<name>.tmp-<pid>` created exclusive (0600 POSIX), fsynced, then
//! renamed over the destination; the temp is removed on ANY failure; a
//! stale same-pid temp from a crashed earlier run is removed and retried
//! exactly once. The parent directory must already exist (caller creates
//! it — same contract as fsutil #4). profiles.json embeds credentials, so
//! it is written 0600 (D9); on Windows the user-profile ACL applies.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::{LinkError, Profile, ProfileStore};

impl ProfileStore {
    /// Load from a path; a missing file yields an empty store.
    pub fn load(path: &Path) -> Result<Self, LinkError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    version: 1,
                    active: None,
                    profiles: Vec::new(),
                });
            }
            Err(e) => return Err(e.into()),
        };
        let mut store: Self = serde_json::from_str(&text)?;
        if store.version == 0 {
            // The frozen `#[derive(Default)]` yields version 0; a version-0
            // file on disk is meaningless — normalize either way.
            store.version = 1;
        }
        Ok(store)
    }

    /// Atomically save (tmp + rename; 0600 on POSIX).
    pub fn save(&self, path: &Path) -> Result<(), LinkError> {
        let version = if self.version == 0 { 1 } else { self.version };
        // json! pins the exact frozen shape and key order.
        let doc = serde_json::json!({
            "version": version,
            "active": self.active,
            "profiles": self.profiles,
        });
        let text = serde_json::to_string_pretty(&doc)?;
        write_private_atomic(path, &text)?;
        tracing::debug!(count = self.profiles.len(), "profiles.json saved");
        Ok(())
    }

    /// Insert or update (dedupe by protocol+address+port+identity).
    /// Returns the effective profile id.
    pub fn upsert(&mut self, profile: Profile) -> String {
        let (protocol, address, port, identity) = profile.dedupe_key_parts();
        let existing = self.profiles.iter_mut().find(|p| {
            p.protocol == protocol
                && p.address == address
                && p.port == port
                && p.identity == identity
        });
        if let Some(existing) = existing {
            // Same server, fresh metadata: keep the EXISTING id (NOTE #11)
            // so `active` and any GUI selection stay valid.
            existing.tag = profile.tag;
            existing.flow = profile.flow;
            existing.transport = profile.transport;
            existing.security = profile.security;
            existing.remark = profile.remark;
            return existing.id.clone();
        }
        let id = profile.id.clone();
        self.profiles.push(profile);
        id
    }

    /// Remove by id; clears `active` when it referenced the removed id.
    /// Returns whether a profile was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        let removed = self.profiles.len() < before;
        if removed && self.active.as_deref() == Some(id) {
            self.active = None;
        }
        removed
    }

    /// S19: replace one profile by id, IN PLACE (same list position, same
    /// id, `active` untouched). The replacement's `id` field is ignored —
    /// the stored id wins, exactly like `upsert` keeps the existing id on
    /// a metadata refresh. Returns false (no change) for an unknown id.
    /// No dedupe: an explicit edit that collides with another profile's
    /// `(protocol, address, port, identity)` is the user's choice; a later
    /// re-import of the same link still merges via `upsert`.
    pub fn update(&mut self, id: &str, mut replacement: Profile) -> bool {
        let Some(slot) = self.profiles.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        replacement.id = id.to_string();
        *slot = replacement;
        true
    }

    /// Set the active profile by id (or clear it). An unknown id is
    /// rejected: returns false and changes nothing.
    pub fn set_active(&mut self, id: Option<&str>) -> bool {
        match id {
            None => {
                self.active = None;
                true
            }
            Some(id) => {
                if self.profiles.iter().any(|p| p.id == id) {
                    self.active = Some(id.to_string());
                    true
                } else {
                    false
                }
            }
        }
    }

    /// All stored profiles (contract text lists `list`; NOTE #11).
    pub fn list(&self) -> &[Profile] {
        &self.profiles
    }
}

// ---------------------------------------------------------------------------
// atomic private write — port of src/fsutil.rs (mt-links is standalone)
// ---------------------------------------------------------------------------

/// Write `contents` to `path` atomically (see module doc).
fn write_private_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let tmp = temp_sibling(path);
    let result = write_and_rename(&tmp, path, contents.as_bytes());
    if result.is_err() {
        // Never leave the temp file behind on failure (fsutil contract #3).
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_and_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = match open_exclusive(tmp) {
        Ok(f) => f,
        // Stale temp from a crashed same-pid run — remove once and retry
        // (fsutil contract #5); a second collision is a real error.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(tmp);
            open_exclusive(tmp)?
        }
        Err(e) => return Err(e),
    };
    f.write_all(bytes)?;
    // Flush to disk BEFORE the rename makes the content visible — the
    // rename is the atomic publication point (fsutil contract #1).
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
    use crate::{Protocol, Security, Transport};

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "mt-links-store-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&d).expect("scratch dir");
        d
    }

    /// Dummy profiles built from the synthetic vectors' values only.
    fn profile(id: &str, tag: &str, identity: &str) -> Profile {
        Profile {
            id: id.to_string(),
            tag: tag.to_string(),
            protocol: Protocol::Vless,
            address: "203.0.113.10".to_string(),
            port: 443,
            identity: identity.to_string(),
            flow: Some("xtls-rprx-vision".to_string()),
            transport: Transport::Tcp,
            security: Security::Reality {
                sni: "tunnel.example.org".to_string(),
                fingerprint: "chrome".to_string(),
                public_key: "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde".to_string(),
                short_id: "0123abcd".to_string(),
                spider_x: None,
            },
            remark: String::new(),
        }
    }

    fn temp_blocked_sibling(file: &Path) -> PathBuf {
        let mut name = file.file_name().unwrap().to_os_string();
        name.push(format!(".tmp-{}", std::process::id()));
        file.with_file_name(name)
    }

    #[test]
    fn missing_file_yields_empty_store_version_1() {
        let d = scratch("missing");
        let store = ProfileStore::load(&d.join("profiles.json")).expect("load");
        assert_eq!(store.version, 1);
        assert!(store.active.is_none());
        assert!(store.list().is_empty());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn save_load_roundtrip_preserves_profiles_and_active() {
        let d = scratch("roundtrip");
        let path = d.join("profiles.json");
        let mut store = ProfileStore::default();
        let id_a = store.upsert(profile(
            "id-a",
            "tag-a",
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        ));
        let id_b = store.upsert(profile(
            "id-b",
            "tag-b",
            "11111111-2222-3333-4444-555555555555",
        ));
        assert!(store.set_active(Some(&id_b)));
        store.save(&path).expect("save");
        let loaded = ProfileStore::load(&path).expect("load");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.active.as_deref(), Some(id_b.as_str()));
        assert_eq!(loaded.list().len(), 2);
        assert_eq!(loaded.list()[0].id, id_a);
        assert_eq!(loaded.list()[1].id, id_b);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn saved_json_has_the_exact_frozen_shape() {
        let d = scratch("shape");
        let path = d.join("profiles.json");
        let mut store = ProfileStore::default();
        let id = store.upsert(profile(
            "id-a",
            "tag-a",
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        ));
        store.set_active(Some(&id));
        store.save(&path).expect("save");
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["active", "profiles", "version"], "exact key set");
        assert_eq!(v["version"], 1);
        assert_eq!(v["active"], id.as_str());
        assert_eq!(v["profiles"].as_array().unwrap().len(), 1);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn upsert_dedupes_by_protocol_address_port_identity() {
        let mut store = ProfileStore::default();
        let first = store.upsert(profile(
            "first",
            "old tag",
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        ));
        // Same endpoint+identity, new tag: must UPDATE, not append, and keep
        // the existing id so references stay stable.
        let again = store.upsert(profile(
            "fresh-uuid",
            "new tag",
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        ));
        assert_eq!(first, again, "existing id returned");
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].tag, "new tag");
        assert_eq!(store.list()[0].id, first);
        // Different identity → different server → new entry.
        store.upsert(profile(
            "other",
            "other",
            "11111111-2222-3333-4444-555555555555",
        ));
        assert_eq!(store.list().len(), 2);
    }

    #[test]
    fn remove_clears_active_and_reports() {
        let mut store = ProfileStore::default();
        let id = store.upsert(profile("a", "a", "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"));
        store.upsert(profile("b", "b", "11111111-2222-3333-4444-555555555555"));
        assert!(store.set_active(Some(&id)));
        assert!(!store.remove("no-such-id"));
        assert!(store.remove(&id));
        assert!(store.active.is_none(), "active cleared with the profile");
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn update_replaces_in_place_keeping_id_position_and_active() {
        let mut store = ProfileStore::default();
        let id_a = store.upsert(profile(
            "id-a",
            "tag-a",
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        ));
        store.upsert(profile(
            "id-b",
            "tag-b",
            "11111111-2222-3333-4444-555555555555",
        ));
        assert!(store.set_active(Some(&id_a)), "set active");

        // Edit A: new address + tag; a FRESH uuid in the id field must be
        // ignored (the stored id wins).
        let mut edited = profile(
            "fresh-uuid",
            "edited-a",
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        );
        edited.address = "198.51.100.7".to_string();
        edited.port = 8443;
        assert!(store.update(&id_a, edited));

        assert_eq!(store.list().len(), 2, "no append on edit");
        assert_eq!(store.list()[0].id, id_a, "id preserved");
        assert_eq!(store.list()[0].tag, "edited-a");
        assert_eq!(store.list()[0].address, "198.51.100.7");
        assert_eq!(store.list()[0].port, 8443);
        assert_eq!(store.active.as_deref(), Some(id_a.as_str()), "active kept");

        // Unknown id: rejected, nothing changes.
        assert!(!store.update("no-such-id", profile("x", "x", "x")));
        assert_eq!(store.list().len(), 2);
    }

    #[test]
    fn update_colliding_with_another_profile_is_kept() {
        // An explicit edit may duplicate another entry's dedupe key — the
        // store keeps BOTH (merging is the user's call, not ours).
        let mut store = ProfileStore::default();
        let id_a = store.upsert(profile(
            "id-a",
            "tag-a",
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        ));
        store.upsert(profile(
            "id-b",
            "tag-b",
            "11111111-2222-3333-4444-555555555555",
        ));
        let mut edited = profile(
            "id-a",
            "tag-a-clone",
            "11111111-2222-3333-4444-555555555555",
        );
        edited.address = "203.0.113.10".to_string();
        assert!(store.update(&id_a, edited));
        assert_eq!(store.list().len(), 2, "no silent merge on explicit edit");
        assert_eq!(
            store.list()[0].identity,
            "11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn set_active_unknown_id_is_rejected_without_change() {
        let mut store = ProfileStore::default();
        let id = store.upsert(profile("a", "a", "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"));
        assert!(!store.set_active(Some("bogus")));
        assert!(store.active.is_none());
        assert!(store.set_active(Some(&id)));
        assert_eq!(store.active.as_deref(), Some(id.as_str()));
        assert!(store.set_active(None));
        assert!(store.active.is_none());
    }

    #[test]
    fn atomic_save_overwrites_and_leaves_no_temp() {
        let d = scratch("atomic");
        let path = d.join("profiles.json");
        fs::write(&path, "OLD-CONTENTS").unwrap();
        let mut store = ProfileStore::default();
        store.upsert(profile("a", "a", "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"));
        store.save(&path).expect("save");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"profiles\""), "rewritten: {text}");
        let leftovers: Vec<_> = fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["profiles.json".to_string()], "no temp left");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn stale_temp_from_a_crashed_run_is_removed_and_retried() {
        let d = scratch("stale");
        let path = d.join("profiles.json");
        let stale = temp_blocked_sibling(&path);
        fs::write(&stale, "half-written crash debris").unwrap();
        let mut store = ProfileStore::default();
        store.upsert(profile("a", "a", "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"));
        store.save(&path).expect("save retries past the stale temp");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"profiles\""));
        assert!(!stale.exists(), "stale temp consumed");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rename_failure_cleans_the_temp_up() {
        let d = scratch("rename-fail");
        // Destination is a DIRECTORY: the temp is written fine, but the
        // rename fails → failure path must remove the temp (fsutil test
        // heritage).
        let dir_dest = d.join("profiles.json");
        fs::create_dir(&dir_dest).unwrap();
        let store = ProfileStore::default();
        assert!(store.save(&dir_dest).is_err());
        let leftovers: Vec<_> = fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["profiles.json".to_string()], "temp cleaned");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn failed_save_leaves_the_existing_file_untouched() {
        let d = scratch("blocked");
        let path = d.join("profiles.json");
        fs::write(&path, "OLD-CONTENTS").unwrap();
        // Block the temp path with a DIRECTORY: create_new fails
        // AlreadyExists, the remove-retry also fails (it is a dir), and the
        // whole save errors out — the real file must never be touched.
        fs::create_dir(temp_blocked_sibling(&path)).unwrap();
        let mut store = ProfileStore::default();
        store.upsert(profile("a", "a", "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"));
        assert!(store.save(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "OLD-CONTENTS");
        fs::remove_dir_all(&d).ok();
    }

    #[cfg(unix)]
    #[test]
    fn unix_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("mode");
        let path = d.join("profiles.json");
        let mut store = ProfileStore::default();
        store.upsert(profile("a", "a", "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"));
        store.save(&path).expect("save");
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "private file mode (D9)");
        fs::remove_dir_all(&d).ok();
    }
}
