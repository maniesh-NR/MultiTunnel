//! JSON configuration: load, defaults, path resolution, validation.
//!
//! JSON schema (camelCase — kept identical to the docs in README.md):
//!
//! ```json
//! {
//!   "server":  { "address": "203.0.113.10", "port": 8443, "uuid": "...",
//!                "sni": "tunnel.example.org", "publicKey": "...", "shortId": "..." },
//!   "local":   { "listen": "127.0.0.1", "socksPort": 10808, "httpPort": 10809 },
//!   "paths":   { "xray": "bin/xray.exe", "netMerge": "net-merge",
//!                "xrayConfig": "xray-client.json", "logDir": "logs", "stateDir": "state" },
//!   "netMerge": { "enable": true },
//!   "behavior": { "systemProxy": true, "proxyOverride": "...", "probeTarget": "example.com:80",
//!                 "probeIntervalSec": 60, "restartMax": 5 }
//! }
//! ```
//!
//! Conventions (reviewer S1-REV-C #6): numeric fields set to 0 and strings
//! left empty fall back to the documented defaults on load — so
//! `server.port: 0` silently becomes 8443. Paths that are absolute or rooted
//! (`C:\tools\x`, `\tools\x`) are used as-is; purely relative paths are
//! joined onto the executable's directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::err;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Server {
    pub address: String,
    pub port: u16,
    pub uuid: String,
    pub sni: String,
    pub public_key: String,
    pub short_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Local {
    pub listen: String,
    pub socks_port: u16,
    pub http_port: u16,
}

impl Default for Local {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1".into(),
            socks_port: 10808,
            http_port: 10809,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Paths {
    pub xray: PathBuf,
    pub net_merge: PathBuf,
    pub xray_config: PathBuf,
    pub log_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            xray: PathBuf::from("bin/xray.exe"),
            net_merge: PathBuf::from("net-merge"),
            xray_config: PathBuf::from("xray-client.json"),
            log_dir: PathBuf::from("logs"),
            state_dir: PathBuf::from("state"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NetMerge {
    pub enable: bool,
}

impl Default for NetMerge {
    fn default() -> Self {
        Self { enable: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Behavior {
    pub system_proxy: bool,
    pub proxy_override: String,
    pub probe_target: String,
    pub probe_interval_sec: u32,
    pub restart_max: u32,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            system_proxy: true,
            proxy_override: default_proxy_override(),
            probe_target: "example.com:80".into(),
            probe_interval_sec: 60,
            restart_max: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub server: Server,
    pub local: Local,
    pub paths: Paths,
    pub net_merge: NetMerge,
    pub behavior: Behavior,
}

fn default_proxy_override() -> String {
    let mut s = String::from("localhost;127.*;10.*;192.168.*");
    for i in 16..=31 {
        s.push_str(&format!(";172.{i}.*"));
    }
    s.push_str(";<local>");
    s
}

impl Config {
    /// Built-in defaults. The server address defaults to the VM this tool was
    /// built for; change it if you reserve a different static IP.
    pub fn defaults() -> Self {
        Self {
            server: Server {
                address: "35.208.99.240".into(),
                port: 8443,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Write a fresh template config (first run).
    pub fn save_template(path: &Path) -> Result<()> {
        let mut cfg = Self::defaults();
        cfg.apply_fallback_defaults();
        let pretty = serde_json::to_string_pretty(&cfg)?;
        std::fs::write(path, pretty + "\n")?;
        Ok(())
    }

    /// Load the config, apply fallback defaults for anything left unset,
    /// resolve relative paths against the executable's directory, validate.
    pub fn load(path: &Path) -> Result<Self> {
        let raw =
            std::fs::read_to_string(path).map_err(|e| err!("read {}: {e}", path.display()))?;
        let mut cfg: Self =
            serde_json::from_str(&raw).map_err(|e| err!("parse {}: {e}", path.display()))?;
        cfg.apply_fallback_defaults();
        let base = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        cfg.resolve(base.as_deref());
        cfg.validate()?;
        Ok(cfg)
    }

    fn apply_fallback_defaults(&mut self) {
        let d = Self::defaults();
        if self.local.listen.is_empty() {
            self.local.listen = d.local.listen.clone();
        }
        if self.local.socks_port == 0 {
            self.local.socks_port = d.local.socks_port;
        }
        if self.local.http_port == 0 {
            self.local.http_port = d.local.http_port;
        }
        if self.paths.xray.as_os_str().is_empty() {
            self.paths.xray = d.paths.xray.clone();
        }
        if self.paths.net_merge.as_os_str().is_empty() {
            self.paths.net_merge = d.paths.net_merge.clone();
        }
        if self.paths.xray_config.as_os_str().is_empty() {
            self.paths.xray_config = d.paths.xray_config.clone();
        }
        if self.paths.log_dir.as_os_str().is_empty() {
            self.paths.log_dir = d.paths.log_dir.clone();
        }
        if self.paths.state_dir.as_os_str().is_empty() {
            self.paths.state_dir = d.paths.state_dir.clone();
        }
        if self.behavior.proxy_override.is_empty() {
            self.behavior.proxy_override = default_proxy_override();
        }
        if self.behavior.probe_target.is_empty() {
            self.behavior.probe_target = d.behavior.probe_target.clone();
        }
        if self.behavior.probe_interval_sec == 0 {
            self.behavior.probe_interval_sec = d.behavior.probe_interval_sec;
        }
        if self.server.port == 0 {
            self.server.port = d.server.port;
        }
    }

    fn resolve(&mut self, base: Option<&Path>) {
        let Some(base) = base else { return };
        for p in [
            &mut self.paths.xray,
            &mut self.paths.net_merge,
            &mut self.paths.xray_config,
            &mut self.paths.log_dir,
            &mut self.paths.state_dir,
        ] {
            // Rooted-but-relative paths (`\tools\x` on Windows) are treated as
            // resolved: joining them onto the exe dir would re-root them
            // (reviewer S1-REV-C #7).
            if p.is_relative() && !p.has_root() {
                *p = base.join(&*p);
            }
        }
    }

    /// Validate that the connection can work at all; report exactly which
    /// server fields still need to be filled in.
    pub fn validate(&self) -> Result<()> {
        let mut missing = Vec::new();
        if self.server.address.is_empty() {
            missing.push("server.address");
        }
        if self.server.uuid.is_empty() {
            missing.push("server.uuid");
        }
        if self.server.sni.is_empty() {
            missing.push("server.sni");
        }
        if self.server.public_key.is_empty() {
            missing.push("server.publicKey");
        }
        if !missing.is_empty() {
            return Err(err!(
                "incomplete config — missing: {}.\n\
                 Run server/setup-xray-reality.sh on your VM; the block it prints at the end\n\
                 contains every value listed here. (server.shortId may stay empty.)",
                missing.join(", ")
            ));
        }
        if self.local.socks_port == self.local.http_port {
            return Err(err!(
                "local.socksPort and local.httpPort must differ (both are {})",
                self.local.socks_port
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique temp path so parallel test runs never collide.
    fn temp_config_path(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "multitunnel-s1a-{tag}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    /// Server credentials as `server/setup-xray-reality.sh` would print them.
    fn with_credentials(doc: &mut serde_json::Value) {
        let server = doc.get_mut("server").expect("server object");
        server["uuid"] = serde_json::json!("11111111-2222-3333-4444-555555555555");
        server["sni"] = serde_json::json!("tunnel.example.org");
        server["publicKey"] = serde_json::json!("pubkey-abc");
    }

    #[test]
    fn defaults_are_documented() {
        let c = Config::defaults();
        assert_eq!(c.local.listen, "127.0.0.1");
        assert_eq!(c.local.socks_port, 10808);
        assert_eq!(c.local.http_port, 10809);
        assert_eq!(c.server.address, "35.208.99.240");
        assert_eq!(c.server.port, 8443);
        assert!(c.net_merge.enable);
        assert_eq!(c.behavior.restart_max, 5);
        assert_eq!(c.behavior.probe_interval_sec, 60);
        assert!(c.behavior.system_proxy);
        assert_eq!(c.behavior.probe_target, "example.com:80");
        assert_eq!(c.paths.xray, PathBuf::from("bin/xray.exe"));
        assert_eq!(c.paths.net_merge, PathBuf::from("net-merge"));
        assert_eq!(c.paths.xray_config, PathBuf::from("xray-client.json"));
        assert_eq!(c.paths.log_dir, PathBuf::from("logs"));
        assert_eq!(c.paths.state_dir, PathBuf::from("state"));
    }

    #[test]
    fn save_template_then_load_roundtrip() {
        let path = temp_config_path("roundtrip");
        Config::save_template(&path).expect("save_template");
        let raw = std::fs::read_to_string(&path).expect("read template");
        let mut doc: serde_json::Value =
            serde_json::from_str(&raw).expect("template is valid JSON");
        // The template itself must use the documented camelCase schema.
        assert_eq!(doc["local"]["socksPort"].as_u64(), Some(10808));
        assert_eq!(doc["paths"]["xrayConfig"], "xray-client.json");
        // Simulate the user pasting the server block printed by
        // server/setup-xray-reality.sh into the template.
        with_credentials(&mut doc);
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        let loaded = Config::load(&path);
        let _ = std::fs::remove_file(&path); // delete even if asserts below fail
        let cfg = loaded.expect("load patched template");
        assert_eq!(cfg.server.address, "35.208.99.240");
        assert_eq!(cfg.server.port, 8443);
        assert_eq!(cfg.server.uuid, "11111111-2222-3333-4444-555555555555");
        assert_eq!(cfg.server.sni, "tunnel.example.org");
        assert_eq!(cfg.server.public_key, "pubkey-abc");
        assert_eq!(cfg.local.listen, "127.0.0.1");
        assert_eq!(cfg.local.socks_port, 10808);
        assert_eq!(cfg.local.http_port, 10809);
        assert!(cfg.net_merge.enable);
        assert_eq!(cfg.behavior.restart_max, 5);
        assert_eq!(cfg.behavior.probe_interval_sec, 60);
        // Relative paths survive the round trip (resolved against the exe dir).
        let base = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert_eq!(cfg.paths.xray, base.join("bin/xray.exe"));
        assert_eq!(cfg.paths.log_dir, base.join("logs"));
    }

    #[test]
    fn load_applies_fallback_defaults_and_keeps_explicit_values() {
        let path = temp_config_path("fallback");
        let doc = serde_json::json!({
            "server": {
                "address": "198.51.100.7",
                "port": 0,
                "uuid": "uuid-1",
                "sni": "sni.example.org",
                "publicKey": "key-1"
            },
            "local": { "listen": "", "socksPort": 0, "httpPort": 0 },
            "paths": {
                "xray": "/custom/xray",
                "netMerge": "",
                "xrayConfig": "",
                "logDir": "/custom/logs",
                "stateDir": ""
            },
            "behavior": {
                "systemProxy": false,
                "proxyOverride": "",
                "probeTarget": "9.9.9.9:53",
                "probeIntervalSec": 0
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        let loaded = Config::load(&path);
        let _ = std::fs::remove_file(&path);
        let c = loaded.expect("load with fallbacks");

        // Empty strings / zero ports come back as the documented defaults.
        assert_eq!(c.local.listen, "127.0.0.1");
        assert_eq!(c.local.socks_port, 10808);
        assert_eq!(c.local.http_port, 10809);
        assert_eq!(c.server.port, 8443);
        // Empty path fields fall back to the documented relative defaults,
        // which load then resolves against the executable's directory.
        let base = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert_eq!(c.paths.net_merge, base.join("net-merge"));
        assert_eq!(c.paths.xray_config, base.join("xray-client.json"));
        assert_eq!(c.paths.state_dir, base.join("state"));
        assert_eq!(c.behavior.probe_interval_sec, 60);
        let po = &c.behavior.proxy_override;
        assert!(
            po.starts_with("localhost;127.*;10.*;192.168.*")
                && po.contains(";172.16.*")
                && po.contains(";172.31.*")
                && po.ends_with(";<local>"),
            "proxyOverride default, got: {po}"
        );

        // Explicit non-empty values are preserved.
        assert_eq!(c.server.address, "198.51.100.7");
        assert_eq!(c.server.uuid, "uuid-1");
        assert_eq!(c.server.sni, "sni.example.org");
        assert_eq!(c.server.public_key, "key-1");
        assert_eq!(c.paths.xray, PathBuf::from("/custom/xray"));
        assert_eq!(c.paths.log_dir, PathBuf::from("/custom/logs"));
        assert_eq!(c.behavior.probe_target, "9.9.9.9:53");
        assert!(!c.behavior.system_proxy);
    }

    #[test]
    fn load_resolves_relative_paths_against_exe_dir() {
        let path = temp_config_path("resolve");
        let doc = serde_json::json!({
            "server": {
                "address": "203.0.113.10",
                "port": 8443,
                "uuid": "u",
                "sni": "s",
                "publicKey": "k"
            },
            "paths": {
                "xray": "bin/xray.exe",
                "netMerge": "net-merge",
                "xrayConfig": "xray-client.json",
                "logDir": "logs",
                "stateDir": "state"
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        let loaded = Config::load(&path);
        let _ = std::fs::remove_file(&path);
        let c = loaded.expect("load with relative paths");
        let base = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        for (got, rel) in [
            (&c.paths.xray, "bin/xray.exe"),
            (&c.paths.net_merge, "net-merge"),
            (&c.paths.xray_config, "xray-client.json"),
            (&c.paths.log_dir, "logs"),
            (&c.paths.state_dir, "state"),
        ] {
            assert!(got.starts_with(&base), "{got:?} must start with {base:?}");
            assert_eq!(got, &base.join(rel));
        }
    }

    #[test]
    fn validate_names_missing_camelcase_fields_and_setup_script() {
        let c = Config::defaults();
        let e = c.validate().expect_err("defaults lack credentials");
        let msg = e.to_string();
        assert!(msg.contains("server.uuid"), "msg: {msg}");
        assert!(msg.contains("server.sni"), "msg: {msg}");
        assert!(msg.contains("server.publicKey"), "msg: {msg}");
        assert!(msg.contains("server/setup-xray-reality.sh"), "msg: {msg}");
        // The missing list is exactly the unfilled camelCase fields; address is
        // defaulted (never listed) and shortId may stay empty (only hinted at).
        assert!(
            msg.contains("missing: server.uuid, server.sni, server.publicKey"),
            "msg: {msg}"
        );
        assert!(!msg.contains("server.address"), "msg: {msg}");
    }

    #[test]
    fn validate_rejects_equal_socks_and_http_ports() {
        let mut c = Config::defaults();
        c.server.uuid = "u".into();
        c.server.sni = "s".into();
        c.server.public_key = "k".into();
        c.local.socks_port = 10808;
        c.local.http_port = 10808;
        let e = c.validate().expect_err("equal ports must be rejected");
        let msg = e.to_string();
        assert!(msg.contains("must differ"), "msg: {msg}");
        assert!(msg.contains("10808"), "msg: {msg}");
    }

    #[test]
    fn serialized_schema_is_camel_case() {
        let v = serde_json::to_value(Config::defaults()).unwrap();
        let top = v.as_object().expect("top-level object");
        let mut top_keys: Vec<&str> = top.keys().map(String::as_str).collect();
        top_keys.sort_unstable();
        assert_eq!(
            top_keys,
            ["behavior", "local", "netMerge", "paths", "server"]
        );

        let server = v["server"].as_object().unwrap();
        for key in ["publicKey", "shortId", "address", "port", "uuid", "sni"] {
            assert!(server.contains_key(key), "server.{key} missing");
        }
        assert!(!server.contains_key("public_key") && !server.contains_key("short_id"));

        let local = v["local"].as_object().unwrap();
        for key in ["socksPort", "httpPort", "listen"] {
            assert!(local.contains_key(key), "local.{key} missing");
        }
        assert!(!local.contains_key("socks_port") && !local.contains_key("http_port"));

        let paths = v["paths"].as_object().unwrap();
        for key in ["xrayConfig", "logDir", "stateDir", "xray", "netMerge"] {
            assert!(paths.contains_key(key), "paths.{key} missing");
        }
        assert!(!paths.contains_key("xray_config") && !paths.contains_key("log_dir"));

        assert!(v.get("net_merge").is_none(), "snake_case net_merge leaked");
        assert_eq!(v["netMerge"]["enable"], true);

        let behavior = v["behavior"].as_object().unwrap();
        for key in [
            "systemProxy",
            "proxyOverride",
            "probeTarget",
            "probeIntervalSec",
            "restartMax",
        ] {
            assert!(behavior.contains_key(key), "behavior.{key} missing");
        }
        assert!(
            !behavior.contains_key("system_proxy") && !behavior.contains_key("probe_interval_sec")
        );
        assert_eq!(v["behavior"]["restartMax"].as_u64(), Some(5));
        assert_eq!(v["behavior"]["probeIntervalSec"].as_u64(), Some(60));
    }

    #[test]
    fn proxy_override_default_is_the_full_documented_string() {
        // Golden string — reviewer S1-REV-B #6: pin the exact value so the
        // generated config and the docs cannot drift apart silently.
        let mut want = String::from("localhost;127.*;10.*;192.168.*");
        for i in 16..=31 {
            want.push_str(&format!(";172.{i}.*"));
        }
        want.push_str(";<local>");
        assert_eq!(Config::defaults().behavior.proxy_override, want);
    }

    #[test]
    fn config_example_json_matches_defaults() {
        // config.example.json must stay consistent with what the binary's
        // template writer produces (reviewer S1-REV-B #6).
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let raw = std::fs::read_to_string(manifest.join("config.example.json"))
            .expect("config.example.json must exist next to Cargo.toml");
        let example: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        let defaults = serde_json::to_value(Config::defaults()).expect("serialize defaults");
        assert_eq!(
            example, defaults,
            "config.example.json drifted from defaults()"
        );
    }
}
