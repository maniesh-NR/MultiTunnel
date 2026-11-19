//! `vless://` share-link import (added between S2 rounds, 2026-11-17).
//!
//! MultiTunnel speaks exactly ONE protocol combination — VLESS over TCP with
//! REALITY security and the xtls-rprx-vision flow (see [`crate::xrayconf`]).
//! The importer therefore accepts ONLY links it can faithfully represent and
//! fails with a named-parameter error for everything else, instead of
//! writing a config that silently behaves differently from the link.
//!
//! Policy (DECISIONS D10):
//! - hand-rolled split of `vless://<uuid>@<host>[:<port>]?<query>#<remark>`;
//!   no new dependencies.
//! - NO percent-decoding of fields we store: uuid, host, port, sni,
//!   publicKey and shortId never carry percent-escapes in links emitted by
//!   Xray tooling; the remark fragment is returned as-is and is display-only
//!   (it is NOT written to config.json).
//! - unknown query parameters are ignored (other clients append extras such
//!   as headerType, path, alpn, allowInsecure).
//! - strict where the generated config is fixed: security must be `reality`,
//!   type must be `tcp`, flow must be absent or `xtls-rprx-vision`; pbk must
//!   be a 43-character unpadded base64url string (32-byte X25519 key);
//!   shortId must be empty or 1-16 hex characters; uuid must be 8-4-4-4-12
//!   hex.
//! - error messages never echo credential values (uuid/pbk) so a pasted
//!   error report cannot leak them (D9).
//! - tests use RFC 5737 / dummy values only — never a real deployment's
//!   credentials.

use crate::config::Server;
use crate::err;
use crate::error::Result;

/// Port used when the link carries none (the HTTPS port REALITY borrows).
const DEFAULT_PORT: u16 = 443;

/// A parsed `vless://` link.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLink {
    /// Server block to write into `config.json`.
    pub server: Server,
    /// Fragment (`#name`) of the link; display-only, never stored.
    pub remark: Option<String>,
}

/// Parse a `vless://` share link into a [`Server`] block (+ remark).
pub fn parse(link: &str) -> Result<ParsedLink> {
    let rest = link.trim();
    let Some(rest) = rest.strip_prefix("vless://") else {
        return Err(err!(
            "not a vless:// link (the text must start with 'vless://')"
        ));
    };

    // Fragment = remark. Split it FIRST: it may contain '?'-shaped text, and
    // the query may contain '#'-shaped percent-escapes.
    let (rest, remark) = match rest.split_once('#') {
        Some((body, frag)) => (body, Some(frag.to_string())),
        None => (rest, None),
    };
    let (authority, query) = match rest.split_once('?') {
        Some((a, q)) => (a, q),
        None => (rest, ""),
    };

    let Some((uuid, hostport)) = authority.split_once('@') else {
        return Err(err!(
            "link is missing '<uuid>@' — expected vless://<uuid>@<host>:<port>?..."
        ));
    };
    let uuid = uuid.trim();
    if !valid_uuid(uuid) {
        return Err(err!(
            "bad uuid in link (want 8-4-4-4-12 hex groups, {n} chars total)",
            n = uuid.len()
        ));
    }

    let (host, port) = split_host_port(hostport)?;
    let query = parse_query(query);

    match query.security.as_deref() {
        Some("reality") => {}
        Some(other) => {
            return Err(err!(
                "unsupported security={other:?} — MultiTunnel only speaks VLESS+REALITY (security=reality)"
            ));
        }
        None => {
            return Err(err!(
                "link has no security=reality parameter — MultiTunnel only speaks VLESS+REALITY"
            ));
        }
    }
    match query.net_type.as_deref() {
        Some("tcp") => {}
        Some(other) => {
            return Err(err!(
                "unsupported type={other:?} — the generated config is TCP-only (type=tcp)"
            ));
        }
        None => return Err(err!("link has no type=tcp parameter")),
    }
    match query.flow.as_deref() {
        None | Some("xtls-rprx-vision") => {}
        Some(other) => {
            return Err(err!(
                "unsupported flow={other:?} — the generated config always uses xtls-rprx-vision"
            ));
        }
    }

    let sni = query.sni.unwrap_or_default();
    if sni.is_empty() {
        return Err(err!("link has no sni= parameter (the REALITY serverName)"));
    }
    let public_key = query.pbk.unwrap_or_default();
    if public_key.len() != 43 || !public_key.bytes().all(is_base64url) {
        return Err(err!(
            "pbk= must be 43 unpadded base64url characters (a 32-byte X25519 key); got {n} characters",
            n = public_key.len()
        ));
    }
    let short_id = query.sid.unwrap_or_default();
    if short_id.len() > 16 || !short_id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(err!(
            "sid= must be empty or 1-16 hex characters; got {n} characters",
            n = short_id.len()
        ));
    }

    Ok(ParsedLink {
        server: Server {
            address: host,
            port,
            uuid: uuid.to_string(),
            sni,
            public_key,
            short_id,
        },
        remark,
    })
}

/// 8-4-4-4-12 hex groups, joined by hyphens. Both hex cases are accepted.
fn valid_uuid(s: &str) -> bool {
    let want = [8, 4, 4, 4, 12];
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == want.len()
        && parts
            .iter()
            .zip(want)
            .all(|(p, w)| p.len() == w && p.bytes().all(|c| c.is_ascii_hexdigit()))
}

/// base64url alphabet WITHOUT padding: A-Z a-z 0-9 '-' '_'.
fn is_base64url(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}

/// Split `<host>[:<port>]`, with bracketed IPv6 literals supported.
fn split_host_port(hostport: &str) -> Result<(String, u16)> {
    if let Some(rest) = hostport.strip_prefix('[') {
        let Some(close) = rest.find(']') else {
            return Err(err!("unterminated [IPv6-literal] in link"));
        };
        let host = &rest[..close];
        let after = &rest[close + 1..];
        if host.is_empty() {
            return Err(err!("empty [IPv6-literal] in link"));
        }
        let port = match after.strip_prefix(':') {
            Some(p) => parse_port(p)?,
            None => {
                if !after.is_empty() {
                    return Err(err!("unexpected text after [IPv6-literal]: {after:?}"));
                }
                DEFAULT_PORT
            }
        };
        Ok((host.to_string(), port))
    } else {
        if hostport.is_empty() {
            return Err(err!("link has an empty host"));
        }
        match hostport.rsplit_once(':') {
            Some((h, p)) => {
                if h.is_empty() {
                    return Err(err!("link has an empty host"));
                }
                Ok((h.to_string(), parse_port(p)?))
            }
            None => Ok((hostport.to_string(), DEFAULT_PORT)),
        }
    }
}

fn parse_port(p: &str) -> Result<u16> {
    let port: u16 = p
        .parse()
        .map_err(|_| err!("bad port {p:?} (want 1-65535)"))?;
    if port == 0 {
        return Err(err!("port 0 is not a valid server port"));
    }
    Ok(port)
}

struct Query {
    security: Option<String>,
    sni: Option<String>,
    pbk: Option<String>,
    sid: Option<String>,
    net_type: Option<String>,
    flow: Option<String>,
}

/// Recognized parameters are kept, everything else is ignored by design.
fn parse_query(query: &str) -> Query {
    let mut q = Query {
        security: None,
        sni: None,
        pbk: None,
        sid: None,
        net_type: None,
        flow: None,
    };
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "security" => q.security = Some(v.to_string()),
            "sni" => q.sni = Some(v.to_string()),
            "pbk" => q.pbk = Some(v.to_string()),
            "sid" => q.sid = Some(v.to_string()),
            "type" => q.net_type = Some(v.to_string()),
            "flow" => q.flow = Some(v.to_string()),
            _ => {}
        }
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dummy credential-shaped values (RFC 5737 hosts; NOT a real key/uuid).
    const UUID: &str = "11111111-2222-3333-4444-555555555555";

    fn pbk43() -> String {
        "x".repeat(43)
    }

    fn link(host_port: &str, params: &str, remark: &str) -> String {
        format!("vless://{UUID}@{host_port}?{params}#{remark}")
    }

    fn reality_params() -> String {
        format!(
            "security=reality&sni=tunnel.example.org&fp=chrome&pbk={}&sid=0123abcd&type=tcp&flow=xtls-rprx-vision",
            pbk43()
        )
    }

    #[test]
    fn parses_full_reality_link() {
        let p = parse(&link("203.0.113.10:443", &reality_params(), "my-vps")).unwrap();
        assert_eq!(p.server.address, "203.0.113.10");
        assert_eq!(p.server.port, 443);
        assert_eq!(p.server.uuid, UUID);
        assert_eq!(p.server.sni, "tunnel.example.org");
        assert_eq!(p.server.public_key, pbk43());
        assert_eq!(p.server.short_id, "0123abcd");
        assert_eq!(p.remark.as_deref(), Some("my-vps"));
    }

    #[test]
    fn default_port_is_443_when_link_has_none() {
        let p = parse(&link("203.0.113.10", &reality_params(), "")).unwrap();
        assert_eq!(p.server.port, 443);
        assert_eq!(p.remark.as_deref(), Some(""));
    }

    #[test]
    fn parses_bracketed_ipv6_and_custom_port() {
        let p = parse(&link("[2001:db8::1]:8443", &reality_params(), "v6")).unwrap();
        assert_eq!(p.server.address, "2001:db8::1");
        assert_eq!(p.server.port, 8443);
    }

    #[test]
    fn accepts_missing_flow_and_ignores_unknown_params() {
        let params = format!(
            "headerType=none&alpn=h2&security=reality&sni=tunnel.example.org&pbk={}&sid=&type=tcp&allowInsecure=0",
            pbk43()
        );
        let p = parse(&link("203.0.113.10", &params, "x")).unwrap();
        assert_eq!(p.server.short_id, "");
        assert_eq!(p.server.sni, "tunnel.example.org");
    }

    #[test]
    fn accepts_uppercase_uuid_and_sid() {
        let params = format!(
            "security=reality&sni=tunnel.example.org&pbk={}&sid=ABCDEF01&type=tcp",
            pbk43()
        );
        let l = format!("vless://AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE@203.0.113.10?{params}");
        let p = parse(&l).unwrap();
        assert_eq!(p.server.uuid, "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE");
        assert_eq!(p.server.short_id, "ABCDEF01");
    }

    #[test]
    fn rejects_non_vless_scheme() {
        let e = parse("vmess://AAAA").unwrap_err();
        assert!(e.to_string().contains("not a vless:// link"), "msg: {e}");
    }

    #[test]
    fn rejects_missing_userinfo() {
        let e = parse(&format!(
            "vless://203.0.113.10:443?{}&type=tcp",
            "security=reality"
        ))
        .unwrap_err();
        assert!(e.to_string().contains("missing"), "msg: {e}");
    }

    #[test]
    fn rejects_missing_or_wrong_security() {
        let no_sec = format!("sni=tunnel.example.org&pbk={}&type=tcp", pbk43());
        let e = parse(&link("203.0.113.10", &no_sec, "")).unwrap_err();
        assert!(e.to_string().contains("security=reality"), "msg: {e}");

        let tls = format!(
            "security=tls&sni=tunnel.example.org&pbk={}&type=tcp",
            pbk43()
        );
        let e = parse(&link("203.0.113.10", &tls, "")).unwrap_err();
        assert!(e.to_string().contains("security=\"tls\""), "msg: {e}");
    }

    #[test]
    fn rejects_missing_or_non_tcp_type() {
        let no_type = format!("security=reality&sni=tunnel.example.org&pbk={}", pbk43());
        let e = parse(&link("203.0.113.10", &no_type, "")).unwrap_err();
        assert!(e.to_string().contains("type=tcp"), "msg: {e}");

        let ws = format!(
            "security=reality&sni=tunnel.example.org&pbk={}&type=ws",
            pbk43()
        );
        let e = parse(&link("203.0.113.10", &ws, "")).unwrap_err();
        assert!(e.to_string().contains("type=\"ws\""), "msg: {e}");
    }

    #[test]
    fn rejects_wrong_flow() {
        let params = format!(
            "security=reality&sni=tunnel.example.org&pbk={}&type=tcp&flow=xtls-rprx-direct",
            pbk43()
        );
        let e = parse(&link("203.0.113.10", &params, "")).unwrap_err();
        assert!(e.to_string().contains("flow="), "msg: {e}");
    }

    #[test]
    fn rejects_missing_sni() {
        let params = format!("security=reality&pbk={}&type=tcp", pbk43());
        let e = parse(&link("203.0.113.10", &params, "")).unwrap_err();
        assert!(e.to_string().contains("sni="), "msg: {e}");
    }

    #[test]
    fn rejects_bad_pbk() {
        for bad in [
            "y".repeat(42),          // too short
            "y".repeat(44),          // too long
            format!("{}=", pbk43()), // padded
            "a+b/c".to_string(),     // wrong alphabet
        ] {
            let params = format!("security=reality&sni=s.example&pbk={bad}&type=tcp");
            let e = parse(&link("203.0.113.10", &params, "")).unwrap_err();
            assert!(e.to_string().contains("pbk="), "msg: {e}");
        }
    }

    #[test]
    fn rejects_bad_sid() {
        for bad in ["zzzz", "0123456789abcdef0"] {
            let params = format!(
                "security=reality&sni=s.example&pbk={}&sid={bad}&type=tcp",
                pbk43()
            );
            let e = parse(&link("203.0.113.10", &params, "")).unwrap_err();
            assert!(e.to_string().contains("sid="), "msg: {e}");
        }
    }

    #[test]
    fn rejects_bad_uuid_shape() {
        let params = reality_params();
        let l = format!("vless://11111111-2222-3333@203.0.113.10?{params}");
        let e = parse(&l).unwrap_err();
        assert!(e.to_string().contains("bad uuid"), "msg: {e}");
    }

    #[test]
    fn rejects_bad_ports_and_empty_host() {
        for hp in [
            ":443",
            "203.0.113.10:0",
            "203.0.113.10:99999",
            "203.0.113.10:abc",
        ] {
            let e = parse(&link(hp, &reality_params(), "")).unwrap_err();
            let msg = e.to_string();
            assert!(
                msg.contains("port") || msg.contains("host"),
                "hp={hp} msg: {msg}"
            );
        }
    }

    #[test]
    fn rejects_unterminated_ipv6() {
        let e = parse(&link("[2001:db8::1", &reality_params(), "")).unwrap_err();
        assert!(e.to_string().contains("IPv6"), "msg: {e}");
    }

    #[test]
    fn error_messages_do_not_echo_credentials() {
        // Bad pbk: the error must carry the length, not the value, and never
        // the uuid — so a pasted error report cannot leak a real credential.
        let params = format!(
            "security=reality&sni=s.example&pbk={}&type=tcp",
            "y".repeat(20)
        );
        let e = parse(&link("203.0.113.10", &params, "")).unwrap_err();
        let msg = e.to_string();
        assert!(!msg.contains(UUID), "uuid leaked: {msg}");
        assert!(!msg.contains("yyy"), "pbk leaked: {msg}");
        assert!(msg.contains("20 characters"), "msg: {msg}");
    }

    #[test]
    fn server_block_roundtrips_camel_case_into_config() {
        // Mirror of cmd_import: patch the defaults doc's server block, then
        // prove the merged document deserializes, validates, and re-serializes
        // with the documented camelCase schema (D8).
        let p = parse(&link("203.0.113.10:443", &reality_params(), "my-vps")).unwrap();
        let mut doc = serde_json::to_value(crate::config::Config::defaults()).unwrap();
        doc["server"] = serde_json::to_value(&p.server).unwrap();
        let merged: crate::config::Config = serde_json::from_value(doc).unwrap();
        merged.validate().unwrap();
        let v = serde_json::to_value(&merged).unwrap();
        assert_eq!(v["server"]["publicKey"], pbk43());
        assert_eq!(v["server"]["shortId"], "0123abcd");
        assert_eq!(v["server"]["port"], 443);
        assert!(v.get("public_key").is_none(), "snake_case leaked");
    }
}
