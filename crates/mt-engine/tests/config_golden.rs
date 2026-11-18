//! Golden config tests for `generate_config` (pure; run on every target).

use std::net::SocketAddr;

use mt_engine::{generate_config, EngineSpec, VlessRealityEndpoint};

// Shared synthetic vectors from harness/CONTRACTS-S6.md (safe, no real
// credentials).
const UUID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const PBK: &str = "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";

fn spec() -> EngineSpec {
    EngineSpec {
        tag: "golden".into(),
        endpoint: VlessRealityEndpoint {
            address: "203.0.113.10".into(),
            port: 443,
            uuid: UUID.into(),
            sni: "tunnel.example.org".into(),
            public_key: PBK.into(),
            short_id: "0123abcd".into(),
            fingerprint: "chrome".into(),
            spider_x: None,
        },
        send_through: None,
    }
}

fn listen() -> SocketAddr {
    "127.0.0.1:34567".parse().unwrap()
}

#[test]
fn golden_config_matches_contract() {
    let cfg = generate_config(&spec(), listen());

    // log level warning.
    assert_eq!(cfg["log"]["loglevel"], "warning");

    // ONE loopback socks inbound: noauth, udp false.
    let inbounds = cfg["inbounds"].as_array().expect("inbounds array");
    assert_eq!(inbounds.len(), 1, "exactly one inbound");
    let inbound = &inbounds[0];
    assert_eq!(inbound["listen"], "127.0.0.1", "loopback only");
    assert_eq!(inbound["port"], 34567);
    assert_eq!(inbound["protocol"], "socks");
    assert_eq!(inbound["settings"]["auth"], "noauth");
    assert_eq!(inbound["settings"]["udp"], false, "udp must be false");

    // ONE vless outbound: mux off, reality stream settings.
    let outbounds = cfg["outbounds"].as_array().expect("outbounds array");
    assert_eq!(outbounds.len(), 1, "exactly one outbound");
    let out = &outbounds[0];
    assert_eq!(out["protocol"], "vless");
    // CRITICAL CONTRACT: mux must stay off (see lib.rs module docs).
    assert!(out["mux"].is_object(), "mux object must be present");
    assert_eq!(out["mux"]["enabled"], false);
    let ss = &out["streamSettings"];
    assert_eq!(ss["network"], "tcp");
    assert_eq!(ss["security"], "reality");
    let rs = &ss["realitySettings"];
    assert_eq!(rs["serverName"], "tunnel.example.org");
    assert_eq!(rs["fingerprint"], "chrome");
    assert_eq!(rs["publicKey"], PBK);
    assert_eq!(rs["shortId"], "0123abcd");
    assert_eq!(rs["spiderX"], "/", "spider_x None defaults to /");
    let vnext = &out["settings"]["vnext"][0];
    assert_eq!(vnext["address"], "203.0.113.10");
    assert_eq!(vnext["port"], 443);
    let user = &vnext["users"][0];
    assert_eq!(user["id"], UUID);
    assert_eq!(user["encryption"], "none");

    // No freedom/direct/blackhole outbounds, no routing rules — mt-net's
    // router decides what reaches the engine.
    assert!(cfg.get("routing").is_none(), "no routing block");
    let rendered = serde_json::to_string(&cfg).unwrap();
    assert!(!rendered.contains("freedom"), "no freedom outbound");
    assert!(!rendered.contains("blackhole"), "no blackhole outbound");
    // Unbound engine: no sendThrough key at all (v0.3 golden shape).
    assert!(
        out.get("sendThrough").is_none(),
        "sendThrough absent when unbound"
    );

    // The engine consumes camelCase wire names; assert every contract
    // field survives a JSON round trip.
    for key in [
        "loglevel",
        "streamSettings",
        "realitySettings",
        "serverName",
        "fingerprint",
        "publicKey",
        "shortId",
        "spiderX",
        "vnext",
        "encryption",
        "mux",
    ] {
        assert!(rendered.contains(key), "missing wire key {key}");
    }
}

#[test]
fn spider_x_override_and_listen_are_honored() {
    let mut spec = spec();
    spec.endpoint.spider_x = Some("/spider-path".into());
    let cfg = generate_config(&spec, "127.0.0.1:1".parse().unwrap());
    assert_eq!(
        cfg["outbounds"][0]["streamSettings"]["realitySettings"]["spiderX"],
        "/spider-path"
    );
    assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
    assert_eq!(cfg["inbounds"][0]["port"], 1);
}

#[test]
fn send_through_binds_the_outbound_to_a_local_address() {
    // S7 adapter bonding: a bound engine writes its adapter's IPv4 as the
    // outbound `sendThrough` (top-level field, NOT inside settings).
    let mut bound = spec();
    bound.send_through = Some("192.168.1.42".into());
    let cfg = generate_config(&bound, listen());
    let out = &cfg["outbounds"][0];
    assert_eq!(out["sendThrough"], "192.168.1.42");
    // Everything else is untouched by the bind.
    assert_eq!(out["mux"]["enabled"], false, "mux still off when bound");
    assert_eq!(out["streamSettings"]["security"], "reality");

    // Whitespace-only bind is treated as unbound (tolerant input).
    let mut blank = spec();
    blank.send_through = Some("   ".into());
    let cfg2 = generate_config(&blank, listen());
    assert!(cfg2["outbounds"][0].get("sendThrough").is_none());

    // Old serialized specs (no sendThrough key) still deserialize.
    let legacy = serde_json::json!({
        "tag": "legacy",
        "endpoint": {
            "address": "203.0.113.10", "port": 443, "uuid": UUID,
            "sni": "tunnel.example.org", "public_key": PBK,
            "short_id": "0123abcd", "fingerprint": "chrome"
        }
    });
    let parsed: EngineSpec = serde_json::from_value(legacy).expect("legacy spec deserializes");
    assert_eq!(parsed.send_through, None);
}
