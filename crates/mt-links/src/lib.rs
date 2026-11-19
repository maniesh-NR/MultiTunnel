//! mt-links — share links, the multi-server profile store, subscriptions.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert A"): the public types and function signatures below are stable;
//! the implementation is filled in by the links expert. Policy follows
//! D9/D10: strict where the rest of the app must faithfully represent a
//! link, liberal where v2rayN compatibility only needs parsing; error
//! messages never echo credential values; tests use the synthetic vectors
//! from CONTRACTS-S6.md only.
//!
//! # IMPLEMENTATION NOTES (Expert A — every "pick one and document it")
//!
//! 1. **Module layout**: parsing/rendering lives in [`link`], subscription
//!    handling in [`subscription`], the store in [`store`], migration in
//!    [`migrate`], and codec helpers in [`b64`]/[`percent`]. Everything is
//!    re-exported at the crate root, so the frozen public paths
//!    (`mt_links::Profile`, `mt_links::parse_share_link`, …) are unchanged.
//!
//! 2. **Parse policy — D10 ported and widened** (D10 = v0.2 `src/vless.rs`):
//!    *liberal on capability combinations* — any `security` × `type` combo
//!    among none/tls/reality × tcp/ws/grpc/httpupgrade is accepted and
//!    recorded, unknown query parameters are ignored (their NAMES are
//!    recorded via `tracing::debug!`, never their values, which may be
//!    credentials); *strict on credential shapes* — uuid must be 8-4-4-4-12
//!    hex, `pbk` (when present) must be 43 unpadded base64url characters,
//!    `sid` empty or 1-16 hex, port 1-65535. A truncated paste should fail
//!    loudly, not become a half-profile. v0.2's REALITY-only restriction is
//!    deliberately dropped: mt-links is multi-protocol by contract.
//!
//! 3. **Percent-encoding policy**: parameter VALUES that carry user data
//!    (fragment/remark, userinfo/identity, ws/httpupgrade `host`+`path`,
//!    grpc `serviceName`, `sni`, `alpn` items, `flow`, `fp`, `spx`) are
//!    percent-DECODED on parse (lossy) and percent-ENCODED on render, so
//!    `parse(render(p)) == p` is a fixed point for any UTF-8 payload — this
//!    matches v2rayN/Android `Uri.getQueryParameter` behavior in the wild.
//!    Structural fields (address/host, port, `pbk`, `sid`) are never
//!    decoded/encoded (v0.2 D10 heritage): their alphabets cannot contain
//!    escapes (pbk/sid are alphabet-validated on parse), and percent-
//!    decoding a hostname would be wrong. Real-world links that percent-
//!    encode remarks (`%E9%9A%A7…`) and ws paths (`path=%2Fws`) therefore
//!    import correctly, and remarks render back escaped.
//!
//! 4. **SS `identity` carries `method:password`** (SIP002 userinfo before
//!    base64). The frozen `Profile` schema has no separate `method` field,
//!    and `render_share_link` must round-trip SIP002
//!    (`ss://base64url(method:password)@host:port`), so the method+password
//!    pair is stored as one credential string. Deduping by
//!    `(protocol, address, port, identity)` stays correct (same endpoint +
//!    same method+password = same server). Report to the orchestrator: if a
//!    later wave splits the method out, this is the one stored-value change.
//!
//! 5. **`tag` vs `remark` semantics**: `remark` is the human name imported
//!    from the source (link fragment for vless/trojan/ss, `ps` for vmess,
//!    empty when the source had none). `tag` is the display label: the
//!    source name when non-empty, otherwise a derived
//!    `"{protocol}-{address}:{port}"`. Render uses `remark` as the fragment
//!    so round-trips are exact.
//!
//! 6. **Render strictness**: `render_share_link` validates only STRUCTURAL
//!    integrity (uuid shape; REALITY profiles need non-empty sni, 43-char
//!    base64url `public_key`, hex `short_id` ≤ 16; port != 0; ss identity
//!    must be `method:password`; ss may not carry transport/security layers
//!    — SIP002 cannot represent them). It does NOT restrict capability
//!    combinations: any recorded profile must be exportable faithfully
//!    (export strictness is the engine's job — D10 belongs to the v0.2
//!    importer, which stays REALITY-only). Parameters are emitted in a
//!    fixed canonical order for deterministic output.
//!
//! 7. **Defaults picked when a link omits data**: vless/trojan without a
//!    port → 443 (the HTTPS port REALITY/TLS borrows, vless.rs heritage);
//!    ss REQUIRES a port (SIP002 has no canonical default — truncated links
//!    should fail); ws/httpupgrade without `path` → `"/"` (URL-path default,
//!    v2rayN convention); grpc without `serviceName` → `""` (Xray default);
//!    trojan without `security` → TLS (the protocol IS TLS; explicit
//!    `security=none` is honored); vless without `security` → none.
//!
//! 8. **vmess specifics**: v2rayN base64 JSON (all four base64 shapes are
//!    accepted; some old builds double-encode — a second decode is attempted
//!    when the first result is not a JSON object). `port` accepts a JSON
//!    number or string; `aid`/`scy` have NO field in the frozen `Profile`
//!    schema and are dropped (modern vmess is AEAD with aid=0) — logged at
//!    debug, flagged to the orchestrator as a known data-loss. vmess has no
//!    render (v2rayN's vmess format is a JSON blob, not a URI; the contract
//!    lists render only for vless/trojan/ss).
//!
//! 9. **Subscription bodies**: whole-body base64 is detected by the absence
//!    of `"://"` in the trimmed body (a base64 string can never contain the
//!    `:` of a scheme, plain lines always can); all ASCII whitespace is
//!    stripped before the decode attempt because providers hard-wrap blobs.
//!    Failure lines carry `line N:` positions of the DECODED text plus the
//!    (credential-safe by construction) parse error. An empty body yields a
//!    single failure line, not an empty success.
//!
//! 10. **`fetch_subscription` is async** (global rule 7: tokio, and a
//!     library must not create its own runtime — `reqwest::blocking` would
//!     spin one internally). The frozen-looking synchronous signature in
//!     the contract is interpreted as async for that reason. A 20 s total
//!     timeout is applied; reqwest errors are stripped of their URL
//!     (`Error::without_url`) before being wrapped in `LinkError::Io`,
//!     because subscription URLs can embed access tokens. `LinkError` is
//!     frozen (no `Fetch` variant), so network failures map to `Io`.
//!
//! 11. **`ProfileStore` semantics**: `save` normalizes `version == 0` to 1
//!     (the frozen `#[derive(Default)]` yields 0, but a version-0 file on
//!     disk would be meaningless); `upsert` keeps the EXISTING profile id
//!     on dedupe so id references (including `active`) stay stable and
//!     returns that id; `remove` clears `active` when it pointed at the
//!     removed id; `set_active(Some(id))` returns false and does nothing
//!     unless the id exists, `set_active(None)` always succeeds; `list()`
//!     is added here (the contract TEXT lists it, the skeleton omitted it).
//!
//! 12. **Atomic save**: a faithful port of the root crate's
//!     `src/fsutil.rs::write_private_atomic` (staged S2-REV-C P2-6) —
//!     sibling temp file `<name>.tmp-<pid>` created exclusive, fsynced,
//!     renamed, temp removed on any failure, stale same-pid temp retried
//!     once, 0600 on POSIX. Re-implemented locally because mt-links is
//!     standalone and must not depend on the root crate. The parent
//!     directory must already exist (caller creates it — same contract as
//!     fsutil #4). profiles.json embeds credentials, hence 0600 (D9).
//!
//! 13. **Migration**: `migrate_legacy_server` accepts the v0.2 `server`
//!     block (`address/port/uuid/sni/publicKey/shortId`, camelCase) and
//!     tolerantly descends into `{"server": {...}}` if handed the whole
//!     config. The derived profile pins the v0.2 generated shape (D10):
//!     flow `xtls-rprx-vision`, fingerprint `chrome`, spiderX `"/"`, tag
//!     `vless-<address>:<port>`, empty remark (the source block has no
//!     name field).

use serde::{Deserialize, Serialize};

mod b64;
mod link;
mod migrate;
mod percent;
mod store;
mod subscription;

#[cfg(feature = "fetch")]
pub use subscription::fetch_subscription;

pub use link::{parse_share_link, render_share_link};
pub use migrate::migrate_legacy_server;
pub use subscription::parse_subscription;

/// Protocol of a proxy profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
}

impl Protocol {
    /// Short name used in derived tags (`vless-203.0.113.10:443`).
    pub(crate) fn name(self) -> &'static str {
        match self {
            Protocol::Vless => "vless",
            Protocol::Vmess => "vmess",
            Protocol::Trojan => "trojan",
            Protocol::Shadowsocks => "ss",
        }
    }
}

/// Transport layer under the protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Transport {
    Tcp,
    Ws { host: Option<String>, path: String },
    Grpc { service: String },
    HttpUpgrade { host: Option<String>, path: String },
}

/// Security layer over the transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Security {
    None,
    Tls {
        sni: Option<String>,
        alpn: Vec<String>,
        allow_insecure: bool,
    },
    Reality {
        sni: String,
        fingerprint: String,
        public_key: String,
        short_id: String,
        spider_x: Option<String>,
    },
}

/// One imported server profile (v2rayN "server" equivalent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub tag: String,
    pub protocol: Protocol,
    pub address: String,
    pub port: u16,
    /// UUID (vless/vmess) or password (trojan/ss) — never logged verbatim.
    pub identity: String,
    pub flow: Option<String>,
    pub transport: Transport,
    pub security: Security,
    pub remark: String,
}

impl Profile {
    /// The dedupe key of the profile: `(protocol, address, port, identity)`.
    pub(crate) fn dedupe_key_parts(&self) -> (Protocol, &str, u16, &str) {
        (
            self.protocol,
            self.address.as_str(),
            self.port,
            self.identity.as_str(),
        )
    }
}

/// Parse errors. Messages must never contain credential values.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("not a supported share link: {0}")]
    Unsupported(String),
    #[error("malformed link: {0}")]
    Malformed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// The multi-server profile store (`profiles.json`).
///
/// JSON shape (frozen by the contract): `{ "version": 1, "active": id|null,
/// "profiles": [Profile] }`. See IMPLEMENTATION NOTES #11/#12 for the
/// upsert/remove/atomic-save semantics.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProfileStore {
    pub version: u32,
    pub active: Option<String>,
    pub profiles: Vec<Profile>,
}
