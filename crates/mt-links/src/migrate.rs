//! v0.2 `config.json` `server` block → VLESS+REALITY profile.
//!
//! The v0.2 generated shape is pinned by D10 (`xrayconf`: VLESS/TCP/REALITY
//! with the xtls-rprx-vision flow, chrome fingerprint, spiderX "/"), so the
//! migrated profile fills exactly those values. `tag` is derived
//! (`vless-<address>:<port>`) and `remark` stays empty because the legacy
//! block carries no name field (lib.rs NOTE #13). Liberal input: the whole
//! config object (with a `server` key) is accepted as well as the bare
//! block. Error messages carry lengths only — the uuid/publicKey are never
//! echoed (D9); tests use synthetic vector values only.

use crate::link::{is_base64url, parse_port, valid_uuid};
use crate::{LinkError, Profile, Protocol, Security, Transport};

/// Migrate a v0.2 `config.json` `server` block into a VLESS+REALITY profile.
pub fn migrate_legacy_server(v: &serde_json::Value) -> Result<Profile, LinkError> {
    let obj = match v.as_object() {
        Some(o) if o.contains_key("address") => o,
        // Whole-config form: descend into the "server" block (NOTE #13).
        Some(o) => o.get("server").and_then(serde_json::Value::as_object).ok_or_else(|| {
            LinkError::Malformed(
                "legacy server block must be a JSON object with address/port/uuid/sni/publicKey/shortId, or a config object holding a 'server' block".into(),
            )
        })?,
        None => {
            return Err(LinkError::Malformed(
                "legacy server block must be a JSON object with address/port/uuid/sni/publicKey/shortId".into(),
            ));
        }
    };

    let address = field_str(obj, "address")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LinkError::Malformed("legacy server block is missing 'address'".into()))?;

    let port_raw = obj
        .get("port")
        .ok_or_else(|| LinkError::Malformed("legacy server block is missing 'port'".into()))?;
    let port = match port_raw {
        serde_json::Value::Number(n) => {
            n.as_u64()
                .and_then(|p| u16::try_from(p).ok())
                .ok_or_else(|| {
                    LinkError::Malformed(
                        "legacy server block 'port' is not an integer in 1-65535".into(),
                    )
                })?
        }
        serde_json::Value::String(s) => parse_port(s.trim())?,
        _ => {
            return Err(LinkError::Malformed(
                "legacy server block 'port' must be a number or a numeric string".into(),
            ));
        }
    };
    if port == 0 {
        return Err(LinkError::Malformed(
            "legacy server block 'port' is not an integer in 1-65535 (port 0)".into(),
        ));
    }

    let Some(uuid) = field_str(obj, "uuid") else {
        return Err(LinkError::Malformed(
            "legacy server block is missing 'uuid'".into(),
        ));
    };
    if !valid_uuid(uuid) {
        // Length only — never the value (D9).
        return Err(LinkError::Malformed(format!(
            "legacy server block 'uuid' is not 8-4-4-4-12 hex ({} characters)",
            uuid.len()
        )));
    }

    let sni = field_str(obj, "sni")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LinkError::Malformed("legacy server block is missing 'sni'".into()))?;

    let Some(public_key) = field_str(obj, "publicKey") else {
        return Err(LinkError::Malformed(
            "legacy server block is missing 'publicKey'".into(),
        ));
    };
    if public_key.len() != 43 || !public_key.bytes().all(is_base64url) {
        return Err(LinkError::Malformed(format!(
            "legacy server block 'publicKey' must be 43 unpadded base64url characters; got {} characters",
            public_key.len()
        )));
    }

    let short_id = field_str(obj, "shortId").unwrap_or_default();
    if !short_id.is_empty()
        && (short_id.len() > 16 || !short_id.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(LinkError::Malformed(format!(
            "legacy server block 'shortId' must be empty or 1-16 hex characters; got {} characters",
            short_id.len()
        )));
    }

    Ok(Profile {
        id: uuid::Uuid::new_v4().to_string(),
        tag: format!("vless-{address}:{port}"),
        protocol: Protocol::Vless,
        address: address.to_string(),
        port,
        identity: uuid.to_string(),
        // D10-pinned v0.2 generated shape.
        flow: Some("xtls-rprx-vision".to_string()),
        transport: Transport::Tcp,
        security: Security::Reality {
            sni: sni.to_string(),
            fingerprint: "chrome".to_string(),
            public_key: public_key.to_string(),
            short_id: short_id.to_string(),
            spider_x: Some("/".to_string()),
        },
        remark: String::new(),
    })
}

fn field_str<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    obj.get(key).and_then(serde_json::Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The v0.2 `config.json` server block, built from the synthetic
    /// CONTRACTS-S6.md vector values only.
    fn legacy_block() -> serde_json::Value {
        serde_json::json!({
            "address": "203.0.113.10",
            "port": 8443,
            "uuid": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
            "sni": "tunnel.example.org",
            "publicKey": "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde",
            "shortId": "0123abcd"
        })
    }

    #[test]
    fn migration_golden() {
        let p = migrate_legacy_server(&legacy_block()).expect("migrate");
        // Everything except the freshly generated id is golden-pinned.
        assert!(uuid::Uuid::parse_str(&p.id).is_ok(), "fresh uuid v4 id");
        assert_eq!(p.tag, "vless-203.0.113.10:8443");
        assert_eq!(p.protocol, Protocol::Vless);
        assert_eq!(p.address, "203.0.113.10");
        assert_eq!(p.port, 8443);
        assert_eq!(p.identity, "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
        assert_eq!(p.flow.as_deref(), Some("xtls-rprx-vision"));
        assert_eq!(p.transport, Transport::Tcp);
        assert_eq!(
            p.security,
            Security::Reality {
                sni: "tunnel.example.org".to_string(),
                fingerprint: "chrome".to_string(),
                public_key: "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde".to_string(),
                short_id: "0123abcd".to_string(),
                spider_x: Some("/".to_string()),
            }
        );
        assert_eq!(p.remark, "");
    }

    #[test]
    fn migration_accepts_the_whole_config_object() {
        let whole =
            serde_json::json!({ "server": legacy_block(), "local": { "socksPort": 10808 } });
        let p = migrate_legacy_server(&whole).expect("migrate whole config");
        assert_eq!(p.address, "203.0.113.10");
        assert_eq!(p.port, 8443);
    }

    #[test]
    fn migration_rejects_bad_fields_without_echo() {
        // Missing address.
        let bad = serde_json::json!({ "port": 8443 });
        let e = migrate_legacy_server(&bad).unwrap_err();
        assert!(e.to_string().contains("address"), "msg: {e}");

        // Bad uuid: length only, never the value.
        let bad = serde_json::json!({
            "address": "203.0.113.10", "port": 8443,
            "uuid": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeeeeeeeeeee",
            "sni": "tunnel.example.org", "publicKey": "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde"
        });
        let e = migrate_legacy_server(&bad).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("44 characters"), "length only: {msg}");
        assert!(
            !msg.contains("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeeeeeeeeeee"),
            "uuid echoed: {msg}"
        );

        // Bad publicKey: length only, never the value.
        let bad = serde_json::json!({
            "address": "203.0.113.10", "port": 8443,
            "uuid": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
            "sni": "tunnel.example.org", "publicKey": "zzz"
        });
        let e = migrate_legacy_server(&bad).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("got 3 characters"), "length only: {msg}");
        assert!(!msg.contains("zzz"), "key echoed: {msg}");

        // Port 0 is not a server port.
        let bad = serde_json::json!({
            "address": "203.0.113.10", "port": 0,
            "uuid": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
            "sni": "tunnel.example.org", "publicKey": "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde"
        });
        let e = migrate_legacy_server(&bad).unwrap_err();
        assert!(e.to_string().contains("port"), "msg: {e}");

        // Non-object input.
        assert!(migrate_legacy_server(&serde_json::json!(42)).is_err());
    }
}
