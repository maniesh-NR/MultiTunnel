//! Xray client config generation.
//!
//! Design notes — do not change these without understanding why:
//!
//! - mux is explicitly DISABLED: net-merge distributes individual TCP
//!   connections across uplinks. Mux would collapse all traffic into a single
//!   stream, and one stream always rides one link — bonding would be defeated.
//! - the core exposes only loopback SOCKS/HTTP inbounds (no TUN inbound):
//!   net-merge's wintun adapter is the single system-wide routing layer.
//! - LAN/private traffic bypasses the tunnel (geoip:private -> direct).

use crate::config::Config;
use crate::error::Result;
use serde_json::{json, Value};

/// Build the Xray client configuration as a JSON tree.
pub fn generate(cfg: &Config) -> Value {
    let socks_in = json!({
        "tag": "socks-in",
        "listen": cfg.local.listen,
        "port": cfg.local.socks_port,
        "protocol": "socks",
        "settings": { "auth": "noauth", "udp": true }
    });
    let http_in = json!({
        "tag": "http-in",
        "listen": cfg.local.listen,
        "port": cfg.local.http_port,
        "protocol": "http",
        "settings": {}
    });
    let proxy_out = json!({
        "tag": "proxy",
        "protocol": "vless",
        "settings": {
            "vnext": [{
                "address": cfg.server.address,
                "port": cfg.server.port,
                "users": [{
                    "id": cfg.server.uuid,
                    "encryption": "none",
                    "flow": "xtls-rprx-vision",
                    "level": 0
                }]
            }]
        },
        "streamSettings": {
            "network": "tcp",
            "security": "reality",
            "realitySettings": {
                "serverName": cfg.server.sni,
                "fingerprint": "chrome",
                "publicKey": cfg.server.public_key,
                "shortId": cfg.server.short_id,
                "spiderX": "/"
            }
        },
        // Must stay off — see module docs.
        "mux": { "enabled": false }
    });
    let direct_out = json!({ "tag": "direct", "protocol": "freedom" });
    let block_out = json!({ "tag": "block", "protocol": "blackhole" });
    let routing = json!({
        "domainStrategy": "AsIs",
        "rules": [
            { "type": "field", "ip": ["geoip:private"], "outboundTag": "direct" }
        ]
    });

    json!({
        "log": { "loglevel": "warning" },
        "inbounds": [socks_in, http_in],
        "outbounds": [proxy_out, direct_out, block_out],
        "routing": routing
    })
}

/// Render the generated configuration as pretty JSON (2-space indent).
pub fn render(cfg: &Config) -> Result<String> {
    Ok(serde_json::to_string_pretty(&generate(cfg))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Behavior, Local, NetMerge, Paths, Server};

    fn test_config() -> Config {
        Config {
            server: Server {
                address: "203.0.113.10".into(),
                port: 8443,
                uuid: "11111111-2222-3333-4444-555555555555".into(),
                sni: "tunnel.example.org".into(),
                public_key: "pubkeyabc123".into(),
                short_id: "0123abcd".into(),
            },
            local: Local::default(),
            paths: Paths::default(),
            net_merge: NetMerge::default(),
            behavior: Behavior::default(),
        }
    }

    #[test]
    fn proxy_outbound_is_vless_reality_with_mux_off() {
        let v = generate(&test_config());
        let proxy = &v["outbounds"][0];
        assert_eq!(proxy["tag"], "proxy");
        assert_eq!(proxy["protocol"], "vless");
        // CRITICAL CONTRACT: mux must stay off. net-merge bonds by spreading
        // individual TCP connections; a muxed tunnel is one stream on one link.
        assert!(proxy["mux"].is_object(), "mux object must be present");
        assert_eq!(proxy["mux"]["enabled"], false);
        let ss = &proxy["streamSettings"];
        assert_eq!(ss["network"], "tcp");
        assert_eq!(ss["security"], "reality");
        let rs = &ss["realitySettings"];
        assert_eq!(rs["serverName"], "tunnel.example.org");
        assert_eq!(rs["publicKey"], "pubkeyabc123");
        assert_eq!(rs["shortId"], "0123abcd");
        // Camouflage fingerprint and probe path (reviewer S1-REV-B #6).
        assert_eq!(rs["fingerprint"], "chrome");
        assert_eq!(rs["spiderX"], "/");
        let vnext = &proxy["settings"]["vnext"][0];
        assert_eq!(vnext["address"], "203.0.113.10");
        assert_eq!(vnext["port"].as_u64(), Some(8443));
        let users = vnext["users"].as_array().expect("users array");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0]["id"], "11111111-2222-3333-4444-555555555555");
        assert_eq!(users[0]["flow"], "xtls-rprx-vision");
        assert_eq!(users[0]["encryption"], "none");
    }

    #[test]
    fn inbounds_are_two_loopback_listeners_from_config() {
        let cfg = test_config();
        let v = generate(&cfg);
        let ins = v["inbounds"].as_array().expect("inbounds array");
        assert_eq!(ins.len(), 2);
        for inbound in ins {
            assert_eq!(inbound["listen"], cfg.local.listen);
            assert_eq!(inbound["listen"], "127.0.0.1");
        }
        assert_eq!(ins[0]["tag"], "socks-in");
        assert_eq!(ins[0]["protocol"], "socks");
        assert_eq!(ins[0]["port"].as_u64(), Some(cfg.local.socks_port as u64));
        assert_eq!(ins[0]["port"].as_u64(), Some(10808));
        assert_eq!(ins[0]["settings"]["auth"], "noauth");
        assert_eq!(ins[0]["settings"]["udp"], true);
        assert_eq!(ins[1]["tag"], "http-in");
        assert_eq!(ins[1]["protocol"], "http");
        assert_eq!(ins[1]["port"].as_u64(), Some(cfg.local.http_port as u64));
        assert_eq!(ins[1]["port"].as_u64(), Some(10809));
    }

    #[test]
    fn routing_sends_private_ips_direct_with_asis_strategy() {
        let v = generate(&test_config());
        let routing = &v["routing"];
        assert_eq!(routing["domainStrategy"], "AsIs");
        let rules = routing["rules"].as_array().expect("rules array");
        let hit = rules.iter().any(|r| {
            r["type"] == "field"
                && r["outboundTag"] == "direct"
                && r["ip"]
                    .as_array()
                    .map_or(false, |ips| ips.iter().any(|ip| ip.as_str() == Some("geoip:private")))
        });
        assert!(hit, "no rule maps geoip:private -> direct; rules: {rules:?}");
    }

    #[test]
    fn outbounds_are_proxy_direct_block() {
        let v = generate(&test_config());
        let outs = v["outbounds"].as_array().expect("outbounds array");
        assert_eq!(outs.len(), 3);
        let find = |tag: &str| {
            outs.iter()
                .find(|o| o["tag"].as_str() == Some(tag))
                .unwrap_or_else(|| panic!("missing outbound {tag}"))
        };
        assert_eq!(find("proxy")["protocol"], "vless");
        assert_eq!(find("direct")["protocol"], "freedom");
        assert_eq!(find("block")["protocol"], "blackhole");
    }

    #[test]
    fn render_output_is_valid_json_matching_generate() {
        let cfg = test_config();
        let text = render(&cfg).expect("render");
        let parsed: Value =
            serde_json::from_str(&text).expect("render output must be valid JSON");
        assert_eq!(parsed, generate(&cfg));
    }
}
