//! `parse_share_link` / `render_share_link` — vless, vmess, ss, trojan.
//!
//! Parse policy (lib.rs IMPLEMENTATION NOTES #2/#3, ported from the root
//! crate's `src/vless.rs` D10 discipline): liberal on capability
//! combinations (any security × transport among none/tls/reality ×
//! tcp/ws/grpc/httpupgrade), strict on credential shapes (uuid 8-4-4-4-12
//! hex; pbk 43 unpadded base64url; sid empty or 1-16 hex; port 1-65535).
//! Unknown query parameters are ignored — only their NAMES are logged via
//! `tracing::debug!`, never their values, which may be credentials.
//!
//! Error messages carry lengths/positions only — never uuid/pbk/password
//! values (D9), so a pasted error report cannot leak a real deployment.
//! Tests use the synthetic CONTRACTS-S6.md vectors only (RFC 5737 hosts,
//! dummy credentials).

use crate::b64;
use crate::percent;
use crate::{LinkError, Profile, Protocol, Security, Transport};

/// Port used when a vless/trojan link carries none (the HTTPS port the
/// tunnel borrows; vless.rs heritage). ss has no such default: SIP002
/// requires a port, so a port-less ss link is malformed (NOTE #7).
const DEFAULT_PORT: u16 = 443;

type ParseFn = fn(&str) -> Result<Profile, LinkError>;

/// Scheme table, tried in order. Scheme matching is case-insensitive
/// (RFC 3986 §3.1) — the payload after the scheme is left untouched.
const SCHEMES: [(&str, ParseFn); 4] = [
    ("vless://", parse_vless as ParseFn),
    ("vmess://", parse_vmess as ParseFn),
    ("ss://", parse_ss as ParseFn),
    ("trojan://", parse_trojan as ParseFn),
];

/// Parse a `vless://`, `vmess://`, `ss://` or `trojan://` share link.
pub fn parse_share_link(link: &str) -> Result<Profile, LinkError> {
    let rest = link.trim();
    for (scheme, parser) in SCHEMES {
        if let Some(payload) = strip_scheme(rest, scheme) {
            return parser(payload);
        }
    }
    // The input is deliberately not echoed: share links embed credentials.
    Err(LinkError::Unsupported(
        "expected a scheme of vless://, vmess://, ss:// or trojan://".to_string(),
    ))
}

/// Case-insensitive scheme strip (byte-safe: schemes are ASCII, and a
/// non-ASCII head simply fails the `get` and yields `None`).
fn strip_scheme<'a>(s: &'a str, scheme: &str) -> Option<&'a str> {
    let head = s.get(..scheme.len())?;
    if head.eq_ignore_ascii_case(scheme) {
        Some(&s[scheme.len()..])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// shared link splitting (fragment → query → authority), vless.rs heritage
// ---------------------------------------------------------------------------

/// The userinfo/authority/query/fragment parts of one link, with the
/// percent-encoding policy already applied (NOTE #3): userinfo and fragment
/// decoded; authority untouched (hosts are never percent-decoded).
struct SplitLink {
    userinfo: String,
    authority: String,
    query: String,
    remark: String,
}

/// Fragment FIRST: it may contain '?'-shaped text and the query may contain
/// '#'-shaped percent-escapes (vless.rs heritage). Userinfo is split on the
/// LAST '@' (hostports never contain '@'; a password that does is only
/// expressible escaped, and the decode in the callers un-escapes it).
fn split_link(rest: &str, missing_userinfo: &str) -> Result<SplitLink, LinkError> {
    let (body, remark) = match rest.split_once('#') {
        Some((b, frag)) => (b, percent::decode(frag)),
        None => (rest, String::new()),
    };
    let (authority, query) = body.split_once('?').unwrap_or((body, ""));
    let Some((userinfo, hostport)) = authority.rsplit_once('@') else {
        return Err(LinkError::Malformed(missing_userinfo.to_string()));
    };
    Ok(SplitLink {
        userinfo: percent::decode(userinfo),
        authority: hostport.to_string(),
        query: query.to_string(),
        remark,
    })
}

/// Split `<host>[:<port>]`, with bracketed IPv6 literals supported
/// (faithful port of `src/vless.rs::split_host_port`). Returns `None` for
/// the port when the link carries none; callers pick the protocol default.
fn split_host_port(hostport: &str) -> Result<(String, Option<u16>), LinkError> {
    if let Some(rest) = hostport.strip_prefix('[') {
        let Some(close) = rest.find(']') else {
            return Err(LinkError::Malformed(
                "unterminated [IPv6-literal] in link".into(),
            ));
        };
        let host = &rest[..close];
        if host.is_empty() {
            return Err(LinkError::Malformed("empty [IPv6-literal] in link".into()));
        }
        let after = &rest[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(p) => Some(parse_port(p)?),
            None => {
                if !after.is_empty() {
                    return Err(LinkError::Malformed(format!(
                        "unexpected text after [IPv6-literal]: {after:?}"
                    )));
                }
                None
            }
        };
        Ok((host.to_string(), port))
    } else {
        if hostport.is_empty() {
            return Err(LinkError::Malformed("link has an empty host".into()));
        }
        match hostport.rsplit_once(':') {
            // Unbracketed IPv6 ("2001:db8::1") lands here with a non-numeric
            // "port" and is rejected — same behavior as vless.rs.
            Some((h, p)) => {
                if h.is_empty() {
                    return Err(LinkError::Malformed("link has an empty host".into()));
                }
                Ok((h.to_string(), Some(parse_port(p)?)))
            }
            None => Ok((hostport.to_string(), None)),
        }
    }
}

pub(crate) fn parse_port(p: &str) -> Result<u16, LinkError> {
    let port: u16 = p
        .parse()
        .map_err(|_| LinkError::Malformed(format!("bad port {p:?} (want 1-65535)")))?;
    if port == 0 {
        return Err(LinkError::Malformed(
            "port 0 is not a valid server port".into(),
        ));
    }
    Ok(port)
}

/// 8-4-4-4-12 hex groups, joined by hyphens. Both hex cases are accepted
/// (vless.rs heritage).
pub(crate) fn valid_uuid(s: &str) -> bool {
    let want = [8, 4, 4, 4, 12];
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == want.len()
        && parts
            .iter()
            .zip(want)
            .all(|(p, w)| p.len() == w && p.bytes().all(|c| c.is_ascii_hexdigit()))
}

/// base64url alphabet WITHOUT padding: A-Z a-z 0-9 '-' '_' (vless.rs
/// heritage; used for pbk validation and ss SIP002 round-trips).
pub(crate) fn is_base64url(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}

// ---------------------------------------------------------------------------
// query parameters
// ---------------------------------------------------------------------------

struct Query {
    items: Vec<(String, String)>,
}

impl Query {
    /// Last occurrence wins for duplicated keys (vless.rs heritage:
    /// assignment-overwrite semantics).
    fn get(&self, key: &str) -> Option<&str> {
        self.items
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// `allowInsecure=1|true` (case-insensitive).
    fn flag(&self, key: &str) -> bool {
        self.get(key)
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }

    /// alpn items: split on the RAW comma, then decode each item — this way
    /// a comma inside an item (escaped as %2C by the renderer) cannot
    /// collide with the separator (NOTE #3).
    fn comma_list(&self, key: &str) -> Vec<String> {
        self.get(key)
            .map(|raw| {
                raw.split(',')
                    .map(percent::decode)
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Parameters we consume. Everything else is ignored BY DESIGN (other
/// clients append extras such as headerType, headerType-related options,
/// plugin, obfs…); only the ignored NAMES are logged (values may embed
/// credentials, D9).
const KNOWN_PARAMS: [&str; 13] = [
    "security",
    "sni",
    "pbk",
    "sid",
    "type",
    "flow",
    "fp",
    "spx",
    "host",
    "path",
    "serviceName",
    "alpn",
    "allowInsecure",
];

fn parse_query(query: &str) -> Query {
    let mut items = Vec::new();
    let mut ignored: Vec<&str> = Vec::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if KNOWN_PARAMS.contains(&k) {
            // NOTE #3: query values are percent-decoded (lossy).
            items.push((k.to_string(), percent::decode(v)));
        } else {
            ignored.push(k);
        }
    }
    if !ignored.is_empty() {
        tracing::debug!(ignored = ?ignored, "share link: ignored unknown query parameters");
    }
    Query { items }
}

// ---------------------------------------------------------------------------
// capability mapping (shared by vless and trojan)
// ---------------------------------------------------------------------------

fn transport_from_query(q: &Query) -> Result<Transport, LinkError> {
    match q.get("type") {
        None | Some("") | Some("tcp") => Ok(Transport::Tcp),
        Some("ws") => Ok(Transport::Ws {
            // Present-but-empty stays Some("") (fidelity rule, NOTE #7).
            host: q.get("host").map(str::to_string),
            path: q
                .get("path")
                .filter(|p| !p.is_empty())
                .unwrap_or("/")
                .to_string(),
        }),
        Some("grpc") => Ok(Transport::Grpc {
            service: q.get("serviceName").unwrap_or_default().to_string(),
        }),
        Some("httpupgrade") => Ok(Transport::HttpUpgrade {
            host: q.get("host").map(str::to_string),
            path: q
                .get("path")
                .filter(|p| !p.is_empty())
                .unwrap_or("/")
                .to_string(),
        }),
        Some(other) => Err(LinkError::Unsupported(format!(
            "unsupported transport type={other:?} — supported: tcp, ws, grpc, httpupgrade"
        ))),
    }
}

fn tls_from_query(q: &Query) -> Security {
    Security::Tls {
        sni: q.get("sni").map(str::to_string),
        alpn: q.comma_list("alpn"),
        allow_insecure: q.flag("allowInsecure"),
    }
}

fn reality_from_query(q: &Query) -> Result<Security, LinkError> {
    let public_key = match q.get("pbk") {
        None | Some("") => String::new(),
        Some(p) => {
            if p.len() != 43 || !p.bytes().all(is_base64url) {
                // Length only — never the value (D9).
                return Err(LinkError::Malformed(format!(
                    "pbk= must be 43 unpadded base64url characters (a 32-byte X25519 key); got {} characters",
                    p.len()
                )));
            }
            p.to_string()
        }
    };
    let short_id = match q.get("sid") {
        None | Some("") => String::new(),
        Some(s) => {
            if s.len() > 16 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(LinkError::Malformed(format!(
                    "sid= must be empty or 1-16 hex characters; got {} characters",
                    s.len()
                )));
            }
            s.to_string()
        }
    };
    Ok(Security::Reality {
        sni: q.get("sni").unwrap_or_default().to_string(),
        fingerprint: q.get("fp").unwrap_or_default().to_string(),
        public_key,
        short_id,
        spider_x: q.get("spx").map(str::to_string),
    })
}

/// `default` picks the protocol-implied security when the parameter is
/// absent: none for vless, TLS for trojan (the protocol IS TLS, NOTE #7).
fn security_from_query(q: &Query, default: fn(&Query) -> Security) -> Result<Security, LinkError> {
    match q.get("security") {
        None | Some("") => Ok(default(q)),
        Some("none") => Ok(Security::None),
        Some("tls") => Ok(tls_from_query(q)),
        Some("reality") => reality_from_query(q),
        Some(other) => Err(LinkError::Unsupported(format!(
            "unsupported security={other:?} — supported: none, tls, reality"
        ))),
    }
}

// ---------------------------------------------------------------------------
// profile assembly
// ---------------------------------------------------------------------------

/// Everything parse knows about one link, ready to become a `Profile`.
struct ParsedCore {
    protocol: Protocol,
    address: String,
    port: u16,
    identity: String,
    flow: Option<String>,
    transport: Transport,
    security: Security,
    remark: String,
}

impl ParsedCore {
    /// `remark` = the imported human name; `tag` = the display label (NOTE
    /// #5). The id is a fresh uuid v4; `ProfileStore::upsert` assigns the
    /// stable id on dedupe.
    fn into_profile(self) -> Profile {
        let tag = if self.remark.is_empty() {
            format!("{}-{}:{}", self.protocol.name(), self.address, self.port)
        } else {
            self.remark.clone()
        };
        Profile {
            id: uuid::Uuid::new_v4().to_string(),
            tag,
            protocol: self.protocol,
            address: self.address,
            port: self.port,
            identity: self.identity,
            flow: self.flow,
            transport: self.transport,
            security: self.security,
            remark: self.remark,
        }
    }
}

// ---------------------------------------------------------------------------
// per-protocol parsers
// ---------------------------------------------------------------------------

fn parse_vless(rest: &str) -> Result<Profile, LinkError> {
    let parts = split_link(
        rest,
        "vless link is missing '<uuid>@<host>:<port>' — expected vless://<uuid>@<host>:<port>?...",
    )?;
    if !valid_uuid(&parts.userinfo) {
        // Length only — never the value (D9).
        return Err(LinkError::Malformed(format!(
            "bad uuid in link (want 8-4-4-4-12 hex groups, {} chars total)",
            parts.userinfo.len()
        )));
    }
    let (address, port) = split_host_port(&parts.authority)?;
    let q = parse_query(&parts.query);
    Ok(ParsedCore {
        protocol: Protocol::Vless,
        address,
        port: port.unwrap_or(DEFAULT_PORT),
        identity: parts.userinfo,
        flow: q.get("flow").map(str::to_string),
        transport: transport_from_query(&q)?,
        security: security_from_query(&q, |_| Security::None)?,
        remark: parts.remark,
    }
    .into_profile())
}

fn parse_trojan(rest: &str) -> Result<Profile, LinkError> {
    let parts = split_link(
        rest,
        "trojan link is missing '<password>@<host>:<port>' — expected trojan://<password>@<host>:<port>?...",
    )?;
    if parts.userinfo.is_empty() {
        return Err(LinkError::Malformed(
            "trojan link has an empty password".into(),
        ));
    }
    let (address, port) = split_host_port(&parts.authority)?;
    let q = parse_query(&parts.query);
    Ok(ParsedCore {
        protocol: Protocol::Trojan,
        address,
        port: port.unwrap_or(DEFAULT_PORT),
        identity: parts.userinfo,
        flow: q.get("flow").map(str::to_string),
        transport: transport_from_query(&q)?,
        security: security_from_query(&q, tls_from_query)?,
        remark: parts.remark,
    }
    .into_profile())
}

/// ss userinfo ladder (NOTE #4): a raw `method:password` always contains
/// ':' which no base64 alphabet has, so the forms never collide. The raw
/// form is percent-decoded (escaped passwords), the base64 form is tried in
/// all four shapes, then the percent-decoded raw form once more.
fn decode_ss_userinfo(raw: &str) -> Result<String, LinkError> {
    if raw.contains(':') {
        return Ok(percent::decode(raw));
    }
    if let Some(bytes) = b64::decode_any(raw) {
        let decoded = String::from_utf8_lossy(&bytes).into_owned();
        if decoded.contains(':') {
            return Ok(decoded);
        }
    }
    let decoded_raw = percent::decode(raw);
    if decoded_raw.contains(':') {
        return Ok(decoded_raw);
    }
    Err(LinkError::Malformed(format!(
        "ss userinfo ({} characters) is neither base64(method:password) nor raw 'method:password'",
        raw.len()
    )))
}

fn parse_ss(rest: &str) -> Result<Profile, LinkError> {
    let (body, remark) = match rest.split_once('#') {
        Some((b, frag)) => (b, percent::decode(frag)),
        None => (rest, String::new()),
    };

    if let Some((userinfo_raw, hostport_query)) = body.rsplit_once('@') {
        // SIP002: ss://base64(method:password)@host:port[?extras]#tag
        let (hostport, query) = hostport_query
            .split_once('?')
            .unwrap_or((hostport_query, ""));
        let identity = decode_ss_userinfo(userinfo_raw)?;
        let (address, port) = split_host_port(hostport)?;
        let port = port.ok_or_else(|| LinkError::Malformed("ss link is missing :<port>".into()))?;
        // ss carries its crypto in the method; query extras (plugin=…) are
        // recorded-as-ignored by parse_query and dropped.
        parse_query(query);
        return Ok(ParsedCore {
            protocol: Protocol::Shadowsocks,
            address,
            port,
            identity,
            flow: None,
            transport: Transport::Tcp,
            security: Security::None,
            remark,
        }
        .into_profile());
    }

    // Legacy whole-link base64: ss://base64(method:password@host:port)#tag
    let compact: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let bytes = b64::decode_any(&compact).ok_or_else(|| {
        LinkError::Malformed(format!(
            "ss body ({} characters) is neither SIP002 'base64(method:password)@host:port' nor legacy whole-link base64",
            body.len()
        ))
    })?;
    let decoded = String::from_utf8_lossy(&bytes);
    // Legacy encodings may append `?plugin=…` after the host — ignored.
    let plain = decoded.split('?').next().unwrap_or_default();
    let Some((userinfo, hostport)) = plain.rsplit_once('@') else {
        return Err(LinkError::Malformed(
            "legacy ss body does not decode to 'method:password@host:port'".into(),
        ));
    };
    let identity = percent::decode(userinfo);
    if !identity.contains(':') {
        return Err(LinkError::Malformed(format!(
            "legacy ss body does not decode to a 'method:password' pair ({} characters)",
            identity.len()
        )));
    }
    let (address, port) = split_host_port(hostport)?;
    let port = port.ok_or_else(|| LinkError::Malformed("ss link is missing :<port>".into()))?;
    Ok(ParsedCore {
        protocol: Protocol::Shadowsocks,
        address,
        port,
        identity,
        flow: None,
        transport: Transport::Tcp,
        security: Security::None,
        remark,
    }
    .into_profile())
}

fn parse_vmess(rest: &str) -> Result<Profile, LinkError> {
    // Fragment: rare, some tools append '#name' — used only when `ps` is
    // empty (the JSON `ps` field is the v2rayN remark).
    let (payload, frag) = rest.split_once('#').unwrap_or((rest, ""));
    let compact: String = payload
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let bytes = b64::decode_any(&compact).ok_or_else(|| {
        LinkError::Malformed(format!(
            "vmess:// payload is not valid base64 ({} payload characters)",
            compact.len()
        ))
    })?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if !text.trim_start().starts_with('{') {
        // Some old v2rayN builds double-encode the JSON blob (NOTE #8).
        if let Some(inner) = b64::decode_any(text.trim()) {
            let inner = String::from_utf8_lossy(&inner).into_owned();
            if inner.trim_start().starts_with('{') {
                text = inner;
            }
        }
    }
    if !text.trim_start().starts_with('{') {
        return Err(LinkError::Malformed(format!(
            "vmess:// payload does not contain a v2rayN JSON object ({} bytes after base64)",
            bytes.len()
        )));
    }
    let v: serde_json::Value = serde_json::from_str(&text)?;

    let Some(add) = jstr(&v, "add").filter(|s| !s.is_empty()) else {
        return Err(LinkError::Malformed(
            "vmess JSON is missing 'add' (the server address)".into(),
        ));
    };
    let Some(id_raw) = jstr(&v, "id") else {
        return Err(LinkError::Malformed(
            "vmess JSON is missing 'id' (the uuid)".into(),
        ));
    };
    if !valid_uuid(id_raw) {
        return Err(LinkError::Malformed(format!(
            "vmess JSON 'id' is not an 8-4-4-4-12 hex uuid ({} characters)",
            id_raw.len()
        )));
    }
    let port = vmess_port(&v)?;

    // v2rayN stores the grpc serviceName in the "path" field. JSON values
    // are raw (never percent-escaped) — no decoding here, unlike URI params.
    let json_path = || jstr(&v, "path").filter(|p| !p.is_empty()).unwrap_or("/");
    let transport = match jstr(&v, "net").unwrap_or("tcp") {
        "" | "tcp" => Transport::Tcp,
        "ws" => Transport::Ws {
            host: jstr(&v, "host")
                .filter(|h| !h.is_empty())
                .map(str::to_string),
            path: json_path().to_string(),
        },
        "grpc" => Transport::Grpc {
            service: jstr(&v, "path").unwrap_or_default().to_string(),
        },
        "httpupgrade" => Transport::HttpUpgrade {
            host: jstr(&v, "host")
                .filter(|h| !h.is_empty())
                .map(str::to_string),
            path: json_path().to_string(),
        },
        other => {
            return Err(LinkError::Unsupported(format!(
                "unsupported vmess net={other:?} — supported: tcp, ws, grpc, httpupgrade"
            )));
        }
    };

    let security = match jstr(&v, "tls") {
        Some("tls") => Security::Tls {
            sni: jstr(&v, "sni")
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            alpn: jstr(&v, "alpn")
                .map(|raw| {
                    raw.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            allow_insecure: ["allowInsecure", "allow_insecure"]
                .iter()
                .any(|k| v.get(*k).is_some_and(json_flag)),
        },
        Some("reality") => Security::Reality {
            sni: jstr(&v, "sni").unwrap_or_default().to_string(),
            fingerprint: jstr(&v, "fp").unwrap_or_default().to_string(),
            public_key: jstr(&v, "pbk").unwrap_or_default().to_string(),
            short_id: jstr(&v, "sid").unwrap_or_default().to_string(),
            spider_x: jstr(&v, "spx")
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        },
        _ => Security::None,
    };

    // NOTE #8: `aid`/`scy` have no field in the frozen Profile schema and
    // are dropped (modern vmess is AEAD with aid=0).
    if v.get("aid").is_some_and(|a| !a.is_null()) || jstr(&v, "scy").is_some() {
        tracing::debug!("vmess import: aid/scy dropped (no Profile field; modern vmess is AEAD)");
    }

    let ps = jstr(&v, "ps").unwrap_or_default().to_string();
    let remark = if ps.is_empty() {
        percent::decode(frag)
    } else {
        ps
    };

    Ok(ParsedCore {
        protocol: Protocol::Vmess,
        address: add.to_string(),
        port,
        identity: id_raw.to_string(),
        flow: None,
        transport,
        security,
        remark,
    }
    .into_profile())
}

fn jstr<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(serde_json::Value::as_str)
}

/// vmess `port` accepts a JSON number or a numeric string (NOTE #8).
fn vmess_port(v: &serde_json::Value) -> Result<u16, LinkError> {
    let raw = v
        .get("port")
        .ok_or_else(|| LinkError::Malformed("vmess JSON is missing 'port'".into()))?;
    let port = match raw {
        serde_json::Value::Number(n) => {
            n.as_u64()
                .and_then(|p| u16::try_from(p).ok())
                .ok_or_else(|| {
                    LinkError::Malformed("vmess JSON 'port' is not an integer in 1-65535".into())
                })?
        }
        serde_json::Value::String(s) => parse_port(s.trim())?,
        _ => {
            return Err(LinkError::Malformed(
                "vmess JSON 'port' must be a number or a numeric string".into(),
            ));
        }
    };
    if port == 0 {
        return Err(LinkError::Malformed(
            "port 0 is not a valid server port".into(),
        ));
    }
    Ok(port)
}

fn json_flag(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_u64() == Some(1),
        serde_json::Value::String(s) => s == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// rendering (NOTE #6: structural validation only, fixed canonical order)
// ---------------------------------------------------------------------------

/// Render a profile back to a share link (vless/trojan/ss).
pub fn render_share_link(profile: &Profile) -> Result<String, LinkError> {
    if profile.port == 0 {
        return Err(LinkError::Malformed(
            "cannot render profile: port 0 is not a valid server port".into(),
        ));
    }
    match profile.protocol {
        Protocol::Vless => render_vless(profile),
        Protocol::Trojan => render_trojan(profile),
        Protocol::Shadowsocks => render_ss(profile),
        Protocol::Vmess => Err(LinkError::Unsupported(
            "vmess profiles cannot be rendered as share links (the v2rayN vmess format is a JSON blob; profiles.json holds the data)".into(),
        )),
    }
}

fn param(key: &str, value: &str) -> String {
    format!("{key}={}", percent::encode_query_value(value))
}

fn alpn_param(items: &[String]) -> String {
    let joined = items
        .iter()
        .map(|s| percent::encode_query_value(s))
        .collect::<Vec<_>>()
        .join(",");
    format!("alpn={joined}")
}

fn fragment_of(remark: &str) -> String {
    let encoded = percent::encode_fragment(remark);
    if encoded.is_empty() {
        String::new()
    } else {
        format!("#{encoded}")
    }
}

fn validate_reality(sni: &str, public_key: &str, short_id: &str) -> Result<(), LinkError> {
    if sni.is_empty() {
        return Err(LinkError::Malformed(
            "cannot render REALITY profile: sni is empty".into(),
        ));
    }
    if public_key.len() != 43 || !public_key.bytes().all(is_base64url) {
        return Err(LinkError::Malformed(format!(
            "cannot render REALITY profile: public_key must be 43 unpadded base64url characters (a 32-byte X25519 key); got {} characters",
            public_key.len()
        )));
    }
    if short_id.len() > 16 || !short_id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(LinkError::Malformed(format!(
            "cannot render REALITY profile: short_id must be empty or 1-16 hex characters; got {} characters",
            short_id.len()
        )));
    }
    Ok(())
}

fn transport_params(transport: &Transport, emit_default_type: bool) -> Vec<String> {
    match transport {
        Transport::Tcp => {
            // vless links always carry type=tcp (Xray convention, D10);
            // trojan links conventionally omit it for plain tcp.
            if emit_default_type {
                vec!["type=tcp".to_string()]
            } else {
                Vec::new()
            }
        }
        Transport::Ws { host, path } => {
            let mut v = vec!["type=ws".to_string()];
            if let Some(h) = host {
                v.push(param("host", h));
            }
            v.push(param("path", if path.is_empty() { "/" } else { path }));
            v
        }
        Transport::Grpc { service } => {
            vec!["type=grpc".to_string(), param("serviceName", service)]
        }
        Transport::HttpUpgrade { host, path } => {
            let mut v = vec!["type=httpupgrade".to_string()];
            if let Some(h) = host {
                v.push(param("host", h));
            }
            v.push(param("path", if path.is_empty() { "/" } else { path }));
            v
        }
    }
}

fn render_vless(p: &Profile) -> Result<String, LinkError> {
    if !valid_uuid(&p.identity) {
        return Err(LinkError::Malformed(format!(
            "cannot render vless profile: identity is not an 8-4-4-4-12 hex uuid ({} characters)",
            p.identity.len()
        )));
    }
    let mut q = transport_params(&p.transport, true);
    match &p.security {
        Security::None => q.push("security=none".into()),
        Security::Tls {
            sni,
            alpn,
            allow_insecure,
        } => {
            q.push("security=tls".into());
            if let Some(s) = sni {
                q.push(param("sni", s));
            }
            if !alpn.is_empty() {
                q.push(alpn_param(alpn));
            }
            if *allow_insecure {
                q.push("allowInsecure=1".into());
            }
        }
        Security::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x,
        } => {
            validate_reality(sni, public_key, short_id)?;
            q.push("security=reality".into());
            q.push(param("sni", sni));
            if !fingerprint.is_empty() {
                q.push(param("fp", fingerprint));
            }
            q.push(param("pbk", public_key));
            if !short_id.is_empty() {
                q.push(param("sid", short_id));
            }
            if let Some(spx) = spider_x {
                q.push(param("spx", spx));
            }
        }
    }
    if let Some(flow) = &p.flow {
        q.push(param("flow", flow));
    }
    Ok(format!(
        "vless://{}@{}:{}?{}{}",
        p.identity,
        p.address,
        p.port,
        q.join("&"),
        fragment_of(&p.remark)
    ))
}

fn render_trojan(p: &Profile) -> Result<String, LinkError> {
    if p.identity.is_empty() {
        return Err(LinkError::Malformed(
            "cannot render trojan profile: password (identity) is empty".into(),
        ));
    }
    let mut q = transport_params(&p.transport, false);
    match &p.security {
        Security::None => q.push("security=none".into()),
        Security::Tls {
            sni,
            alpn,
            allow_insecure,
        } => {
            // Trojan implies TLS: emit only the overrides (NOTE #7).
            if let Some(s) = sni {
                q.push(param("sni", s));
            }
            if !alpn.is_empty() {
                q.push(alpn_param(alpn));
            }
            if *allow_insecure {
                q.push("allowInsecure=1".into());
            }
        }
        Security::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x,
        } => {
            validate_reality(sni, public_key, short_id)?;
            q.push("security=reality".into());
            q.push(param("sni", sni));
            if !fingerprint.is_empty() {
                q.push(param("fp", fingerprint));
            }
            q.push(param("pbk", public_key));
            if !short_id.is_empty() {
                q.push(param("sid", short_id));
            }
            if let Some(spx) = spider_x {
                q.push(param("spx", spx));
            }
        }
    }
    if let Some(flow) = &p.flow {
        q.push(param("flow", flow));
    }
    let query = if q.is_empty() {
        String::new()
    } else {
        format!("?{}", q.join("&"))
    };
    Ok(format!(
        "trojan://{}@{}:{}{}{}",
        percent::encode_userinfo(&p.identity),
        p.address,
        p.port,
        query,
        fragment_of(&p.remark)
    ))
}

fn render_ss(p: &Profile) -> Result<String, LinkError> {
    match (&p.transport, &p.security) {
        (Transport::Tcp, Security::None) => {}
        _ => {
            return Err(LinkError::Unsupported(
                "cannot render ss profile: SIP002 has no representation for transport or security layers".into(),
            ));
        }
    }
    let Some((method, _password)) = p.identity.split_once(':') else {
        return Err(LinkError::Malformed(format!(
            "cannot render ss profile: identity must be a 'method:password' pair ({} characters)",
            p.identity.len()
        )));
    };
    if method.is_empty() {
        return Err(LinkError::Malformed(
            "cannot render ss profile: cipher method is empty".into(),
        ));
    }
    Ok(format!(
        "ss://{}@{}:{}{}",
        b64::encode_urlsafe_nopad(p.identity.as_bytes()),
        p.address,
        p.port,
        fragment_of(&p.remark)
    ))
}

// ---------------------------------------------------------------------------
// tests — synthetic CONTRACTS-S6.md vectors only (RFC 5737 / dummy creds)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    const PBK: &str = "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";

    /// Contract vector: vless REALITY over tcp.
    const VLESS: &str = "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=reality&sni=tunnel.example.org&fp=chrome&pbk=0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde&sid=0123abcd&type=tcp&flow=xtls-rprx-vision#synthetic-reality";
    /// Contract vector: ss SIP002 (userinfo = base64url("aes-256-gcm:synthetic")).
    const SS: &str = "ss://YWVzLTI1Ni1nY206c3ludGhldGlj@203.0.113.10:8388#synthetic-ss";
    /// Contract vector: trojan with sni.
    const TROJAN: &str =
        "trojan://synthetic-password@203.0.113.10:443?sni=tunnel.example.org#synthetic-trojan";
    /// Contract vector payload: base64url of the v2rayN JSON blob.
    const VMESS_B64: &str = "eyJ2IjoiMiIsInBzIjoic3ludGhldGljIiwiYWRkIjoiMjAzLjAuMTEzLjEwIiwicG9ydCI6IjQ0MyIsImlkIjoiYWFhYWFhYWEtYmJiYi00Y2NjLThkZGQtZWVlZWVlZWVlZWVlIiwiYWlkIjoiMCIsIm5ldCI6InRjcCIsInR5cGUiOiJub25lIiwiaG9zdCI6IiIsInBhdGgiOiIiLCJ0bHMiOiJ0bHMiLCJzbmkiOiJ0dW5uZWwuZXhhbXBsZS5vcmciLCJzY3kiOiJhdXRvIn0";

    fn vmess_link(payload: &str) -> String {
        format!("vmess://{payload}")
    }

    fn json_to_b64(v: &serde_json::Value) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        URL_SAFE_NO_PAD.encode(serde_json::to_string(v).unwrap())
    }

    /// Profiles compare equal ignoring the freshly generated id.
    fn sans_id(p: &Profile) -> Profile {
        let mut q = p.clone();
        q.id = String::new();
        q
    }

    fn round_trip(link: &str) -> (Profile, Profile) {
        let first = parse_share_link(link).expect("first parse");
        let rendered = render_share_link(&first).expect("render");
        let second = parse_share_link(&rendered).expect("re-parse");
        (first, second)
    }

    // ------------------------------------------------------------------
    // contract vectors
    // ------------------------------------------------------------------

    #[test]
    fn parses_vless_contract_vector() {
        let p = parse_share_link(VLESS).expect("parse");
        assert_eq!(p.protocol, Protocol::Vless);
        assert_eq!(p.address, "203.0.113.10");
        assert_eq!(p.port, 443);
        assert_eq!(p.identity, UUID);
        assert_eq!(p.flow.as_deref(), Some("xtls-rprx-vision"));
        assert_eq!(p.transport, Transport::Tcp);
        assert_eq!(
            p.security,
            Security::Reality {
                sni: "tunnel.example.org".to_string(),
                fingerprint: "chrome".to_string(),
                public_key: PBK.to_string(),
                short_id: "0123abcd".to_string(),
                spider_x: None,
            }
        );
        assert_eq!(p.remark, "synthetic-reality");
        assert_eq!(p.tag, "synthetic-reality");
        assert!(uuid::Uuid::parse_str(&p.id).is_ok(), "fresh uuid v4 id");
    }

    #[test]
    fn vless_reality_round_trips_through_render() {
        let (a, b) = round_trip(VLESS);
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn parses_vmess_contract_vector() {
        let p = parse_share_link(&vmess_link(VMESS_B64)).expect("parse");
        assert_eq!(p.protocol, Protocol::Vmess);
        assert_eq!(p.address, "203.0.113.10");
        assert_eq!(p.port, 443);
        assert_eq!(p.identity, UUID);
        assert_eq!(p.remark, "synthetic");
        assert_eq!(p.tag, "synthetic");
        assert_eq!(p.transport, Transport::Tcp);
        assert_eq!(
            p.security,
            Security::Tls {
                sni: Some("tunnel.example.org".to_string()),
                alpn: Vec::new(),
                allow_insecure: false,
            }
        );
        assert_eq!(p.flow, None);
    }

    #[test]
    fn vmess_accepts_padded_and_unpadded_base64() {
        let padded = format!("{VMESS_B64}=");
        let a = parse_share_link(&vmess_link(&padded)).expect("padded");
        let b = parse_share_link(&vmess_link(VMESS_B64)).expect("unpadded");
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn parses_ss_contract_vector() {
        let p = parse_share_link(SS).expect("parse");
        assert_eq!(p.protocol, Protocol::Shadowsocks);
        assert_eq!(p.address, "203.0.113.10");
        assert_eq!(p.port, 8388);
        // NOTE #4: ss identity carries the whole SIP002 method:password pair.
        assert_eq!(p.identity, "aes-256-gcm:synthetic");
        assert_eq!(p.transport, Transport::Tcp);
        assert_eq!(p.security, Security::None);
        assert_eq!(p.remark, "synthetic-ss");
    }

    #[test]
    fn parses_trojan_contract_vector() {
        let p = parse_share_link(TROJAN).expect("parse");
        assert_eq!(p.protocol, Protocol::Trojan);
        assert_eq!(p.identity, "synthetic-password");
        assert_eq!(p.port, 443);
        // Trojan implies TLS even without security= (NOTE #7).
        assert_eq!(
            p.security,
            Security::Tls {
                sni: Some("tunnel.example.org".to_string()),
                alpn: Vec::new(),
                allow_insecure: false,
            }
        );
        assert_eq!(p.remark, "synthetic-trojan");
    }

    // ------------------------------------------------------------------
    // round-trips through render
    // ------------------------------------------------------------------

    #[test]
    fn ss_round_trips_through_render() {
        let (a, b) = round_trip(SS);
        assert_eq!(sans_id(&a), sans_id(&b));
        // SIP002 render shape: base64url(method:password)@host:port.
        let first = parse_share_link(SS).unwrap();
        let rendered = render_share_link(&first).unwrap();
        assert_eq!(rendered, SS);
    }

    #[test]
    fn trojan_round_trips_through_render() {
        let (a, b) = round_trip(TROJAN);
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn trojan_password_with_url_specials_round_trips() {
        // '@', ' ', ':', '/' in the password must survive the round trip.
        let link = "trojan://synthetic%20pa%40ss%3Awo%2Frd@203.0.113.10:443?sni=t.example#x";
        let (a, b) = round_trip(link);
        assert_eq!(a.identity, "synthetic pa@ss:wo/rd");
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn trojan_security_none_and_reality_round_trip() {
        let none = "trojan://synthetic-password@203.0.113.10:443?security=none#n";
        let (a, b) = round_trip(none);
        assert_eq!(a.security, Security::None);
        assert_eq!(sans_id(&a), sans_id(&b));

        let reality = &format!(
            "trojan://synthetic-password@203.0.113.10:443?security=reality&sni=t.example&fp=chrome&pbk={PBK}&sid=abcd&type=tcp#r"
        );
        let (a, b) = round_trip(reality);
        assert!(matches!(&a.security, Security::Reality { .. }));
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn vless_ws_tls_round_trip_with_percent_encoded_path() {
        let link = &format!(
            "vless://{UUID}@203.0.113.10:443?security=tls&sni=cf.example&allowInsecure=1&type=ws&host=cf.example&path=%2Fws%3Fed%3D2560&alpn=h2,http/1.1#wstls"
        );
        let p = parse_share_link(link).expect("parse");
        assert_eq!(
            p.transport,
            Transport::Ws {
                host: Some("cf.example".to_string()),
                path: "/ws?ed=2560".to_string(),
            }
        );
        assert_eq!(
            p.security,
            Security::Tls {
                sni: Some("cf.example".to_string()),
                alpn: vec!["h2".to_string(), "http/1.1".to_string()],
                allow_insecure: true,
            }
        );
        let (a, b) = round_trip(link);
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn vless_grpc_and_httpupgrade_round_trip() {
        let grpc =
            &format!("vless://{UUID}@203.0.113.10?security=none&type=grpc&serviceName=grpc-svc#a");
        let (a, b) = round_trip(grpc);
        assert_eq!(
            a.transport,
            Transport::Grpc {
                service: "grpc-svc".to_string()
            }
        );
        assert_eq!(a.security, Security::None);
        assert_eq!(sans_id(&a), sans_id(&b));

        let up = &format!(
            "vless://{UUID}@203.0.113.10:443?security=tls&sni=h.example&type=httpupgrade&host=h.example&path=%2Fup#u"
        );
        let (a, b) = round_trip(up);
        assert_eq!(
            a.transport,
            Transport::HttpUpgrade {
                host: Some("h.example".to_string()),
                path: "/up".to_string(),
            }
        );
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn empty_remark_derives_tag_and_renders_without_fragment() {
        let link = &format!(
            "vless://{UUID}@203.0.113.10:8443?security=reality&sni=s.example&fp=chrome&pbk={PBK}&sid=abc&type=tcp"
        );
        let p = parse_share_link(link).expect("parse");
        assert_eq!(p.remark, "");
        assert_eq!(p.tag, "vless-203.0.113.10:8443");
        let rendered = render_share_link(&p).expect("render");
        assert!(!rendered.contains('#'), "no fragment: {rendered}");
        let re = parse_share_link(&rendered).unwrap();
        assert_eq!(sans_id(&p), sans_id(&re));
    }

    #[test]
    fn unicode_remark_round_trips() {
        let link = &format!(
            "vless://{UUID}@203.0.113.10:443?security=reality&sni=s.example&fp=chrome&pbk={PBK}&type=tcp#%E9%9A%A7%E9%81%93-01"
        );
        let (a, b) = round_trip(link);
        assert_eq!(a.remark, "隧道-01");
        assert_eq!(sans_id(&a), sans_id(&b));
    }

    #[test]
    fn uppercase_scheme_and_bracketed_ipv6_parse() {
        let link = &format!(
            "VLESS://{UUID}@[2001:db8::1]:8443?security=reality&sni=s.example&fp=chrome&pbk={PBK}&type=tcp"
        );
        let p = parse_share_link(link).expect("parse");
        assert_eq!(p.address, "2001:db8::1");
        assert_eq!(p.port, 8443);
    }

    #[test]
    fn unknown_params_are_ignored_and_parse_succeeds() {
        let link = &format!(
            "vless://{UUID}@203.0.113.10?headerType=none&obfs=x&allowInsecure=0&security=reality&sni=s.example&fp=chrome&pbk={PBK}&sid=abc&type=tcp"
        );
        let p = parse_share_link(link).expect("unknown extras ignored");
        assert_eq!(p.address, "203.0.113.10");
    }

    // ------------------------------------------------------------------
    // liberal-but-strict parse rejections, redacted (D9)
    // ------------------------------------------------------------------

    #[test]
    fn rejects_unsupported_scheme_without_echo() {
        let input = "hysteria2://203.0.113.10:443/?secret=topsecretvalue";
        let e = parse_share_link(input).unwrap_err();
        let msg = e.to_string();
        assert!(matches!(e, LinkError::Unsupported(_)));
        assert!(!msg.contains("hysteria2"), "scheme echoed: {msg}");
        assert!(!msg.contains("topsecretvalue"), "input echoed: {msg}");
        assert!(msg.contains("trojan://"), "lists supported schemes: {msg}");
    }

    #[test]
    fn rejects_bad_uuid_with_length_only() {
        let link = &format!(
            "vless://11111111-2222-3333@203.0.113.10?security=reality&sni=s.example&fp=chrome&pbk={PBK}&type=tcp"
        );
        let e = parse_share_link(link).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("18 chars"), "length only: {msg}");
        assert!(!msg.contains("11111111-2222-3333"), "uuid echoed: {msg}");
    }

    #[test]
    fn rejects_bad_pbk_with_length_only() {
        for bad in [
            "y".repeat(20),
            "y".repeat(44),
            format!("{PBK}="),
            "a+b/c".to_string(),
        ] {
            let link = &format!(
                "vless://{UUID}@203.0.113.10?security=reality&sni=s.example&pbk={bad}&type=tcp"
            );
            let e = parse_share_link(link).unwrap_err();
            let msg = e.to_string();
            assert!(msg.contains("pbk="), "msg: {msg}");
            assert!(
                !msg.contains("yyy") && !msg.contains("a+b"),
                "pbk echoed: {msg}"
            );
            assert!(msg.contains("characters"), "length present: {msg}");
        }
    }

    #[test]
    fn rejects_bad_sid_with_length_only() {
        for bad in ["zzzz", "0123456789abcdef0"] {
            let link = &format!(
                "vless://{UUID}@203.0.113.10?security=reality&sni=s.example&pbk={PBK}&sid={bad}&type=tcp"
            );
            let e = parse_share_link(link).unwrap_err();
            let msg = e.to_string();
            assert!(msg.contains("sid="), "msg: {msg}");
            assert!(!msg.contains(bad), "sid echoed: {msg}");
        }
    }

    #[test]
    fn rejects_bad_ports_and_empty_host() {
        for hp in [
            ":443",
            "203.0.113.10:0",
            "203.0.113.10:99999",
            "203.0.113.10:abc",
        ] {
            let link =
                &format!("vless://{UUID}@{hp}?security=reality&sni=s.example&pbk={PBK}&type=tcp");
            let e = parse_share_link(link).unwrap_err();
            let msg = e.to_string();
            assert!(
                msg.contains("port") || msg.contains("host"),
                "hp={hp} msg: {msg}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_transport_and_security_values() {
        let kcp = &format!("vless://{UUID}@203.0.113.10?security=tls&sni=s.example&type=kcp");
        let e = parse_share_link(kcp).unwrap_err();
        assert!(e.to_string().contains("type=\"kcp\""), "msg: {e}");

        let foo = &format!("vless://{UUID}@203.0.113.10?security=foo&sni=s.example&type=tcp");
        let e = parse_share_link(foo).unwrap_err();
        assert!(e.to_string().contains("security=\"foo\""), "msg: {e}");
    }

    #[test]
    fn rejects_missing_userinfo() {
        let e = parse_share_link("vless://203.0.113.10:443?security=reality&type=tcp").unwrap_err();
        assert!(e.to_string().contains("missing"), "msg: {e}");
    }

    // ------------------------------------------------------------------
    // vmess specifics
    // ------------------------------------------------------------------

    #[test]
    fn vmess_accepts_port_as_json_number() {
        let v = serde_json::json!({
            "v": "2", "add": "203.0.113.10", "port": 443, "id": UUID,
            "aid": "0", "net": "tcp", "tls": "", "ps": "numport"
        });
        let p = parse_share_link(&vmess_link(&json_to_b64(&v))).expect("parse");
        assert_eq!(p.port, 443);
        assert_eq!(p.security, Security::None);
    }

    #[test]
    fn vmess_parses_ws_transport() {
        let v = serde_json::json!({
            "v": "2", "add": "203.0.113.10", "port": "443", "id": UUID,
            "net": "ws", "host": "cf.example", "path": "/ray",
            "tls": "tls", "sni": "s.example", "ps": "wsv"
        });
        let p = parse_share_link(&vmess_link(&json_to_b64(&v))).expect("parse");
        assert_eq!(
            p.transport,
            Transport::Ws {
                host: Some("cf.example".to_string()),
                path: "/ray".to_string(),
            }
        );
    }

    #[test]
    fn vmess_double_encoded_payload_still_parses() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let inner = &format!("{{\"v\":\"2\",\"add\":\"203.0.113.10\",\"port\":\"443\",\"id\":\"{UUID}\",\"net\":\"tcp\",\"tls\":\"\",\"ps\":\"dbl\"}}");
        let once = URL_SAFE_NO_PAD.encode(inner.as_bytes());
        let twice = URL_SAFE_NO_PAD.encode(once.as_bytes());
        let p = parse_share_link(&vmess_link(&twice)).expect("double-encoded vmess");
        assert_eq!(p.address, "203.0.113.10");
        assert_eq!(p.remark, "dbl");
    }

    #[test]
    fn vmess_rejects_bad_base64_without_echo() {
        let e = parse_share_link("vmess://%%%not-base64%%%").unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("16 payload characters"), "length only: {msg}");
        assert!(!msg.contains("%%%"), "payload echoed: {msg}");
    }

    #[test]
    fn vmess_rejects_missing_or_bad_json_fields() {
        let e = parse_share_link(&vmess_link(&json_to_b64(&serde_json::json!({
            "v": "2", "port": "443", "id": UUID
        }))))
        .unwrap_err();
        assert!(e.to_string().contains("'add'"), "msg: {e}");

        let e = parse_share_link(&vmess_link(&json_to_b64(&serde_json::json!({
            "v": "2", "add": "203.0.113.10", "port": "443", "id": "not-a-uuid"
        }))))
        .unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("'id'"), "msg: {msg}");
        assert!(!msg.contains("not-a-uuid"), "value echoed: {msg}");

        let e = parse_share_link(&vmess_link(&json_to_b64(&serde_json::json!({
            "v": "2", "add": "203.0.113.10", "id": UUID
        }))))
        .unwrap_err();
        assert!(e.to_string().contains("'port'"), "msg: {e}");
    }

    #[test]
    fn vmess_render_is_unsupported() {
        let p = parse_share_link(&vmess_link(VMESS_B64)).expect("parse");
        let e = render_share_link(&p).unwrap_err();
        assert!(matches!(e, LinkError::Unsupported(_)), "msg: {e}");
    }

    // ------------------------------------------------------------------
    // ss specifics
    // ------------------------------------------------------------------

    #[test]
    fn ss_legacy_full_base64_parses() {
        // base64("aes-256-gcm:synthetic@203.0.113.10:8388")
        let link = "ss://YWVzLTI1Ni1nY206c3ludGhldGljQDIwMy4wLjExMy4xMDo4Mzg4#legacy";
        let p = parse_share_link(link).expect("legacy ss");
        assert_eq!(p.identity, "aes-256-gcm:synthetic");
        assert_eq!(p.address, "203.0.113.10");
        assert_eq!(p.port, 8388);
        assert_eq!(p.remark, "legacy");
    }

    #[test]
    fn ss_raw_userinfo_parses() {
        let link = "ss://aes-256-gcm:hunter2dummy@203.0.113.10:8388#raw";
        let p = parse_share_link(link).expect("raw ss userinfo");
        assert_eq!(p.identity, "aes-256-gcm:hunter2dummy");
    }

    #[test]
    fn ss_rejects_userinfo_without_method_password_pair() {
        let link = "ss://justonecolonless@203.0.113.10:8388#x";
        let e = parse_share_link(link).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("16 characters"), "length only: {msg}");
        assert!(!msg.contains("justonecolonless"), "userinfo echoed: {msg}");
    }

    #[test]
    fn ss_requires_a_port() {
        let link = "ss://YWVzLTI1Ni1nY206c3ludGhldGlj@203.0.113.10#noport";
        let e = parse_share_link(link).unwrap_err();
        assert!(e.to_string().contains("port"), "msg: {e}");
    }

    #[test]
    fn render_rejects_ss_identity_without_pair() {
        let mut p = parse_share_link(SS).unwrap();
        p.identity = "no-pair-here".to_string();
        let e = render_share_link(&p).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("method:password"), "msg: {msg}");
        assert!(!msg.contains("no-pair-here"), "identity echoed: {msg}");
    }

    #[test]
    fn render_rejects_ss_with_transport_layer() {
        let mut p = parse_share_link(SS).unwrap();
        p.transport = Transport::Ws {
            host: Some("cf.example".to_string()),
            path: "/ws".to_string(),
        };
        let e = render_share_link(&p).unwrap_err();
        assert!(matches!(e, LinkError::Unsupported(_)), "msg: {e}");
    }

    // ------------------------------------------------------------------
    // render structural validation (NOTE #6)
    // ------------------------------------------------------------------

    fn vless_reality_profile(public_key: &str) -> Profile {
        Profile {
            id: UUID.to_string(),
            tag: "t".to_string(),
            protocol: Protocol::Vless,
            address: "203.0.113.10".to_string(),
            port: 443,
            identity: UUID.to_string(),
            flow: Some("xtls-rprx-vision".to_string()),
            transport: Transport::Tcp,
            security: Security::Reality {
                sni: "s.example".to_string(),
                fingerprint: "chrome".to_string(),
                public_key: public_key.to_string(),
                short_id: String::new(),
                spider_x: None,
            },
            remark: String::new(),
        }
    }

    #[test]
    fn render_rejects_reality_with_bad_public_key() {
        let bad_pk = "y".repeat(20);
        let mut p = vless_reality_profile(&bad_pk);
        let e = render_share_link(&p).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("got 20 characters"), "length only: {msg}");
        assert!(!msg.contains("yyy"), "key echoed: {msg}");

        p.security = Security::Reality {
            sni: String::new(),
            fingerprint: "chrome".to_string(),
            public_key: PBK.to_string(),
            short_id: String::new(),
            spider_x: None,
        };
        let e = render_share_link(&p).unwrap_err();
        assert!(e.to_string().contains("sni is empty"), "msg: {e}");
    }

    #[test]
    fn render_rejects_bad_uuid_and_port_zero() {
        let mut p = vless_reality_profile(PBK);
        p.identity = "not-a-uuid".to_string();
        let e = render_share_link(&p).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("10 characters"), "length only: {msg}");
        assert!(!msg.contains("not-a-uuid"), "identity echoed: {msg}");

        p.identity = UUID.to_string();
        p.port = 0;
        let e = render_share_link(&p).unwrap_err();
        assert!(e.to_string().contains("port 0"), "msg: {e}");
    }
}
