//! `profiles` CLI — thin command surface over the mt-links [`ProfileStore`].
//!
//! # CONTRACT
//!
//! Backs `multitunnel profiles list|add <share-link>|use <tag-or-id>|
//! remove <tag-or-id>|export [tag]` (see `src/main.rs`). The store lives at
//! `<config_dir>/profiles.json` (same file [`crate::real_service::RealService`]
//! uses, so CLI and GUI share one profile book). Credential discipline (D9):
//! * parse/store errors are passed through from mt-links verbatim (already
//!   redacted);
//! * nothing here ever prints `identity` (uuid/password) — `list` shows
//!   protocol/address/port/tag only;
//! * `export` DOES print a full share link — that is its purpose (the user
//!   owns the profile and asked for the link; same disclosure the v0.2
//!   `import` preview made for uuid/sni/pbk).
//!
//! # IMPLEMENTATION NOTES (picked and documented)
//!
//! * Functions RETURN printable lines (`Vec<String>`); `main.rs` prints them.
//!   This keeps the module println-free (global rule 5) and unit-testable.
//! * Lookup is id-first, then tag — matches the service's rule
//!   (real_service decision #11).
//! * `add` previews protocol/address/port/tag (NO identity), then upserts:
//!   an identical `(protocol, address, port, identity)` updates the existing
//!   profile in place (mt-links dedupe semantics) — the report says which.
//! * `export` with no argument exports the ACTIVE profile; without an active
//!   profile it fails with a hint. vmess profiles cannot be exported (v2rayN
//!   vmess is a base64 JSON blob, not a URI — mt-links has no vmess render;
//!   the error says so).
//! * The store file is created on first save (atomic, 0600 POSIX) by
//!   mt-links; the config dir is created here first (mt-links requires the
//!   parent to exist).

use std::path::Path;

use mt_links::{parse_share_link, render_share_link, Profile, ProfileStore, Protocol};

use crate::Error;

/// Entry point: `args` is everything after the `profiles` keyword.
pub fn run(config_dir: &Path, args: &[String]) -> Result<Vec<String>, Error> {
    let Some(sub) = args.first().map(String::as_str) else {
        return Err(Error(usage_text()));
    };
    let mut store = load_store(config_dir)?;
    match sub {
        "list" => list(&store),
        "add" => {
            let Some(link) = args.get(1) else {
                return Err(Error("usage: multitunnel profiles add <share-link>".into()));
            };
            add(config_dir, &mut store, link)
        }
        "use" => {
            let Some(key) = args.get(1) else {
                return Err(Error("usage: multitunnel profiles use <tag-or-id>".into()));
            };
            mutate(
                config_dir,
                &mut store,
                |store, profile| {
                    store.set_active(Some(&profile.id));
                    Ok(vec![format!("active profile: {}", profile.tag)])
                },
                key,
            )
        }
        "remove" => {
            let Some(key) = args.get(1) else {
                return Err(Error(
                    "usage: multitunnel profiles remove <tag-or-id>".into(),
                ));
            };
            mutate(
                config_dir,
                &mut store,
                |store, profile| {
                    let tag = profile.tag.clone();
                    if store.remove(&profile.id) {
                        Ok(vec![format!("removed: {tag}")])
                    } else {
                        Err(Error(format!("no profile matches {key:?}")))
                    }
                },
                key,
            )
        }
        "export" => {
            let key = args.get(1).map(String::as_str);
            export(&store, key)
        }
        other => Err(Error(format!(
            "unknown profiles subcommand {other:?}\n{}",
            usage_text()
        ))),
    }
}

fn usage_text() -> String {
    "usage: multitunnel profiles <command>\n\
     \x20 list               show stored profiles\n\
     \x20 add <share-link>   parse + store a profile (shows a preview)\n\
     \x20 use <tag-or-id>    set the active profile\n\
     \x20 remove <tag-or-id> delete a profile\n\
     \x20 export [tag]       print the share link of a profile (default: active)"
        .to_string()
}

fn load_store(config_dir: &Path) -> Result<ProfileStore, Error> {
    ProfileStore::load(&config_dir.join("profiles.json"))
        .map_err(|e| Error(format!("load profiles.json: {e}")))
}

fn save_store(config_dir: &Path, store: &ProfileStore) -> Result<(), Error> {
    std::fs::create_dir_all(config_dir)
        .map_err(|e| Error(format!("create {}: {e}", config_dir.display())))?;
    store
        .save(&config_dir.join("profiles.json"))
        .map_err(|e| Error(format!("save profiles: {e}")))
}

/// `list` output — no identity column (credential discipline).
fn list(store: &ProfileStore) -> Result<Vec<String>, Error> {
    if store.profiles.is_empty() {
        return Ok(vec![
            "no profiles stored".to_string(),
            "add one: multitunnel profiles add <share-link>".to_string(),
        ]);
    }
    let mut lines = Vec::with_capacity(store.profiles.len() + 1);
    lines.push(format!("{} profile(s):", store.profiles.len()));
    for p in &store.profiles {
        let active = if store.active.as_deref() == Some(p.id.as_str()) {
            "*"
        } else {
            " "
        };
        lines.push(format!(
            "{active} {}  {:9} {}:{}  [{}]",
            p.tag,
            protocol_name(p.protocol),
            p.address,
            p.port,
            short_id(&p.id),
        ));
    }
    Ok(lines)
}

fn add(config_dir: &Path, store: &mut ProfileStore, link: &str) -> Result<Vec<String>, Error> {
    let profile = parse_share_link(link).map_err(|e| Error(format!("{e}")))?;
    let preview = preview_line(&profile);
    let dedupes = store
        .profiles
        .iter()
        .any(|p| p.id == profile.id || p.tag == profile.tag || same_endpoint(p, &profile));
    let id = store.upsert(profile.clone());
    save_store(config_dir, store)?;
    let mut lines = vec![preview];
    if dedupes {
        lines.push(format!(
            "updated existing profile {} (same endpoint/identity)",
            short_id(&id)
        ));
    } else {
        lines.push(format!("saved {} [{}]", profile.tag, short_id(&id)));
    }
    lines.push("set active: multitunnel profiles use <tag>".to_string());
    Ok(lines)
}

fn export(store: &ProfileStore, key: Option<&str>) -> Result<Vec<String>, Error> {
    let profile = match key {
        Some(k) => find(store, k)
            .ok_or_else(|| Error(format!("no profile matches {k:?}")))?
            .clone(),
        None => store
            .active
            .as_deref()
            .and_then(|id| store.profiles.iter().find(|p| p.id == id))
            .cloned()
            .ok_or_else(|| Error("no active profile — pass a tag: profiles export <tag>".into()))?,
    };
    if profile.protocol == Protocol::Vmess {
        return Err(Error(
            "vmess profiles cannot be exported as a URI (v2rayN vmess is a \
             base64 JSON blob with no canonical URI form)"
                .into(),
        ));
    }
    let link = render_share_link(&profile).map_err(|e| Error(format!("{e}")))?;
    Ok(vec![link])
}

/// Lookup by id first, then tag; apply `op` with the store, then save.
fn mutate(
    config_dir: &Path,
    store: &mut ProfileStore,
    op: impl FnOnce(&mut ProfileStore, &Profile) -> Result<Vec<String>, Error>,
    key: &str,
) -> Result<Vec<String>, Error> {
    let profile = find(store, key)
        .ok_or_else(|| Error(format!("no profile matches {key:?}")))?
        .clone();
    let lines = op(store, &profile)?;
    save_store(config_dir, store)?;
    Ok(lines)
}

fn find<'a>(store: &'a ProfileStore, key: &str) -> Option<&'a Profile> {
    store
        .profiles
        .iter()
        .find(|p| p.id == key)
        .or_else(|| store.profiles.iter().find(|p| p.tag == key))
}

/// Preview shown by `add` — protocol/address/port/tag only, NO identity.
fn preview_line(p: &Profile) -> String {
    let mut line = format!(
        "parsed: {} {}:{} tag {:?}",
        protocol_name(p.protocol),
        p.address,
        p.port,
        p.tag
    );
    let security = match &p.security {
        mt_links::Security::None => "none",
        mt_links::Security::Tls { .. } => "tls",
        mt_links::Security::Reality { .. } => "reality",
    };
    line.push_str(&format!(" security {security}"));
    if !matches!(p.transport, mt_links::Transport::Tcp) {
        line.push_str(&format!(" transport {}", transport_name(&p.transport)));
    }
    line
}

fn same_endpoint(a: &Profile, b: &Profile) -> bool {
    a.protocol == b.protocol
        && a.address == b.address
        && a.port == b.port
        && a.identity == b.identity
}

fn protocol_name(p: Protocol) -> &'static str {
    match p {
        Protocol::Vless => "vless",
        Protocol::Vmess => "vmess",
        Protocol::Trojan => "trojan",
        Protocol::Shadowsocks => "ss",
    }
}

fn transport_name(t: &mt_links::Transport) -> &'static str {
    match t {
        mt_links::Transport::Tcp => "tcp",
        mt_links::Transport::Ws { .. } => "ws",
        mt_links::Transport::Grpc { .. } => "grpc",
        mt_links::Transport::HttpUpgrade { .. } => "httpupgrade",
    }
}

/// First 8 uuid chars for display (an id fragment is not a credential).
fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic vectors from harness/CONTRACTS-S6.md (D9-safe).
    const VLESS_REALITY: &str = "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=reality&sni=tunnel.example.org&fp=chrome&pbk=0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde&sid=0123abcd&type=tcp&flow=xtls-rprx-vision#synthetic-reality";
    const SS: &str = "ss://YWVzLTI1Ni1nY206c3ludGhldGlj@203.0.113.10:8388#synthetic-ss";

    fn scratch(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("mt-w3-cli-{label}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn run(dir: &Path, args: &[&str]) -> Result<Vec<String>, Error> {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        super::run(dir, &owned)
    }

    #[test]
    fn add_use_export_remove_roundtrip() {
        let dir = scratch("ops");

        // add: preview + saved, never echoing the uuid
        let out = run(&dir, &["add", VLESS_REALITY]).expect("add");
        assert!(out[0].contains("parsed: vless 203.0.113.10:443"), "{out:?}");
        assert!(out[0].contains("synthetic-reality"), "tag in preview");
        assert!(
            !out.iter().any(|l| l.contains("eeeeeeeeeeee")),
            "no uuid echo: {out:?}"
        );

        // list shows the profile, not active yet
        let out = run(&dir, &["list"]).expect("list");
        assert!(out.iter().any(|l| l.contains("synthetic-reality")));
        assert!(!out.iter().any(|l| l.starts_with("* ")), "not active yet");

        // use by tag
        let out = run(&dir, &["use", "synthetic-reality"]).expect("use");
        assert!(out.iter().any(|l| l.contains("active profile")));
        let out = run(&dir, &["list"]).expect("list");
        assert!(
            out.iter().any(|l| l.starts_with("* ")),
            "active marker shown"
        );

        // export (active) prints the share link
        let out = run(&dir, &["export"]).expect("export active");
        assert!(
            out.iter().any(|l| l.starts_with("vless://")),
            "share link printed: {out:?}"
        );

        // remove by tag, then list is empty again
        let out = run(&dir, &["remove", "synthetic-reality"]).expect("remove");
        assert!(out.iter().any(|l| l.contains("removed")));
        let out = run(&dir, &["list"]).expect("list");
        assert!(out.iter().any(|l| l.contains("no profiles stored")));
    }

    #[test]
    fn add_dedupes_by_endpoint_and_identity() {
        let dir = scratch("dedupe");
        run(&dir, &["add", VLESS_REALITY]).expect("first add");
        let out = run(&dir, &["add", VLESS_REALITY]).expect("second add");
        assert!(
            out.iter().any(|l| l.contains("updated existing profile")),
            "second add must update in place: {out:?}"
        );
        let store = load_store(&dir).expect("store loads");
        assert_eq!(store.profiles.len(), 1, "still one profile");
    }

    #[test]
    fn errors_are_helpful_and_credential_free() {
        let dir = scratch("errors");
        // add garbage
        let err = run(&dir, &["add", "https://example.org/not-a-link"]).expect_err("bad link");
        assert!(!err.to_string().contains("eeeeee"));

        // unknown tag for use/export/remove
        for cmd in ["use", "export", "remove"] {
            let err = run(&dir, &[cmd, "missing-tag"]).expect_err("missing profile");
            assert!(err.to_string().contains("no profile matches"), "{err}");
        }

        // vmess cannot export: build the CONTRACTS-S6 vmess vector inline
        run(&dir, &["add", SS]).expect("ss add");
        let vmess_json = r#"{"v":"2","ps":"synthetic","add":"203.0.113.10","port":"443","id":"aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee","aid":"0","net":"tcp","type":"none","host":"","path":"","tls":"tls","sni":"tunnel.example.org","scy":"auto"}"#;
        let vmess_link = format!("vmess://{}", base64_of(vmess_json.as_bytes()));
        let out = run(&dir, &["add", &vmess_link]).expect("vmess add");
        assert!(out[0].contains("parsed: vmess"), "{out:?}");
        let err = run(&dir, &["export", "synthetic"]).expect_err("vmess export");
        assert!(err.to_string().contains("vmess"), "{err}");
    }

    /// Minimal standard base64 (pad) — keeps this test dependency-free.
    fn base64_of(data: &[u8]) -> String {
        const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            out.push(ALPHA[(n >> 18) as usize & 63] as char);
            out.push(ALPHA[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                ALPHA[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHA[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }
}
