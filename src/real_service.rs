//! `RealService` — the production [`mt_service::MtService`] backend (wave 3).
//!
//! # CONTRACT
//!
//! Implements the frozen UI contract [`mt_service::MtService`] EXACTLY
//! (`crates/mt-service/src/lib.rs`): every method, every payload shape; all
//! error strings are user-facing and NEVER contain credential values
//! (uuid/password/pbk) — mt-links' parse errors are already redacted and are
//! passed through verbatim. The GUI (`mt_app::run_app`) talks to this through
//! `Arc<dyn MtService>`. v0.2 CLI behavior (supervisor, connect/doctor/import/
//! generate/tray) is untouched and stays green.
//!
//! # IMPLEMENTATION NOTES (wave-3 "pick one and document it" decisions)
//!
//! 1. **Paths**: profiles live at `<config_dir>/profiles.json`, settings at
//!    `<config_dir>/settings.json`, engine state/logs under the mt-platform
//!    `state_dir()`/`log_dir()`. `RealService::new()` resolves them through
//!    `mt_platform` (env overrides honored); tests use
//!    [`RealService::with`] to inject scratch dirs instead of mutating env.
//! 2. **Local ports**: read from settings.json (`socks_port`/`http_port`,
//!    defaults 10808/10809 — the v2rayN convention this app inherits). Port
//!    `0` means "ephemeral loopback bind" (tests use it; a real system-proxy
//!    write with port 0 would be pointless, and `system_proxy` is simply
//!    skipped when the bind is ephemeral).
//! 3. **Data plane**: one active profile → one link (v0.3 wire-up). The
//!    [`ProxyServer`] owns SOCKS5+HTTP inbounds, the [`Router`] and link
//!    spreading; `Action::Direct` traffic is served by the server's internal
//!    direct outbound, so no extra link is built here. Aggregation with a
//!    single engine link is a hot spare waiting for future multi-profile
//!    support (the router/aggregator code paths are identical).
//! 4. **Engine isolation**: spawning Xray is kept OUT of RealService's test
//!    surface behind the [`LinkFactory`] trait. Production uses
//!    [`EngineLinkFactory`] (locator + [`ManagedEngine`], lazily spawned on
//!    first probe/connect — binary resolution/download is the engine's job);
//!    tests inject a loopback fake. Kill-on-drop is preserved: dropping the
//!    session drops the last `Arc<dyn Outbound>` to the engine, which kills
//!    the child.
//! 5. **Endpoint conversion** ([`endpoint_from_profile`]): only
//!    `Vless` × `Reality` × `Transport::Tcp` is wired for v0.3; everything
//!    else is REJECTED with a polite, credential-free message (never a
//!    silent skip — the user must see why a profile cannot drive the engine).
//!    `flow` must be `xtls-rprx-vision` or unset (the engine config pins
//!    vision; see mt-engine's module docs for why mux/flow are frozen).
//! 6. **State machine** (`Disconnected|Connecting|Connected|Failed`, mirrored
//!    to the GUI through `StateChanged` events): `connect()` — no-op unless
//!    `Disconnected`/`Failed`; sets `Connecting`, builds links, starts the
//!    server, probes the link; `Connected` on the first successful probe,
//!    `Failed` on exhaustion (factory error, bind failure, probe error or
//!    timeout — the server is torn down again before `Failed` is reported).
//!    `disconnect()` stops the server (shutdown watch + sender drop), waits
//!    for the tasks, restores the system proxy and reports `Disconnected`.
//!    Disconnect during `Connecting` sets a request flag the in-flight
//!    connect honors right after its probe. `connect()`/`disconnect()` must
//!    be called from within a tokio runtime (the GUI's iced runtime provides
//!    one; this library never creates its own — global rule 7).
//! 7. **Probe target**: settings `probe_target` (`host:port`, default
//!    `connectivitycheck.gstatic.com:80`). The probe is a full round trip
//!    through the link, so it touches the network by design; tests point it
//!    at loopback.
//! 8. **System proxy**: legacy `crate::sysproxy` (WinINET, snapshot +
//!    restore, no-op on non-Windows) is applied AFTER `Connected`, driven by
//!    `SettingsView.system_proxy`, non-fatal (failure is logged, the tunnel
//!    stays up). The proxy address is the HTTP listener; the override is the
//!    v0.2 full LAN-bypass default (`crate::config::default_proxy_override()`,
//!    S6-REV-C P1-4). `connect_flow` begins with a best-effort stale-snapshot
//!    self-heal (v0.2 connect step 0 / D5). When disabled (or the bind is
//!    ephemeral) nothing is written — no registry touch, no snapshot file.
//! 9. **StatusSnapshot**: state from the state machine; `active_profile` is
//!    the active profile's tag; `up_since` is a UTC timestamp formatted by
//!    [`fmt_utc`] (dependency-free civil-from-days, no chrono); `restarts`
//!    counts successful connect cycles of this service instance (engine
//!    respawns are internal to ManagedEngine and not surfaced); `links` come
//!    from the live aggregator snapshot merged with byte counters accrued
//!    from `ConnClosed` events. Empty link list while disconnected.
//! 10. **Events**: a `ServiceEvent` broadcast re-publishes mapped
//!     `ProxyEvent`s (pump task: `LinkHealth→LinkUpdated`, `Tick→Throughput`,
//!     `Conn*→LogLine`) plus state changes. Buffer 256; slow GUI receivers
//!     lag (drop-oldest) by tokio broadcast semantics — never block the pump.
//! 11. **Id-or-tag lookup**: `set_active_profile`/`remove_profile`/
//!     `test_latency` accept the profile id OR its tag (the CLI speaks tags,
//!     the GUI speaks ids; one lookup path, documented here).
//! 12. **Settings application**: `set_settings` persists first, then applies
//!     the autostart flag through `mt_platform::autostart_*` with the
//!     current executable; an autostart failure is reported as `Err` but the
//!     setting stays persisted (honest UI, no hidden retries). The merge
//!     policy string is mapped leniently (unknown → `Failover` + warn).
//! 13. **Locks**: all shared mutable state sits behind poison-tolerant
//!     `std::sync` locks (see [`lock`]) and is NEVER held across an `.await`;
//!     the session mutex only guards insert/remove of the whole session.
//! 14. **CLI**: `profiles` (see [`crate::profiles`]) and `gui` reuse this
//!     module's paths/settings shapes so the CLI, GUI and service agree on
//!     one on-disk format.
//! 15. **Adapter bonding (S7)**: `SettingsView::bonded_adapters` holds
//!     adapter ids; at connect time they are resolved against the CURRENT
//!     machine enumeration ([`resolve_bonded`]) and the engine factory
//!     builds ONE `ManagedEngine` PER bonded adapter (`sendThrough` binds
//!     its outgoing socket to that adapter's IPv4 — Windows source-address
//!     binding selects the egress interface), each with a distinct tag
//!     (`"profile [adapter]"`) so config/log files never collide. The
//!     aggregator + merge policy then spread individual TCP connections
//!     across the per-adapter links — the net-merge bonding model, mux-off
//!     contract preserved. Unknown ids or IPv4-less adapters are skipped
//!     with a warning (fail-open: a USB modem unplugged between selection
//!     and connect must not black-hole the tunnel); empty selection = one
//!     unbound link, the OS routing table decides (v0.3 behavior).
//!     `set_bonded_adapters` validates ids, persists, and RESTARTS a live
//!     session so the new link set takes effect immediately. `set_settings`
//!     NEVER writes `bonded_adapters` (the Adapters screen owns that field
//!     via `set_bonded_adapters`; a stale GUI settings copy must not
//!     clobber a fresh selection).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mt_bond::BondOutbound;
use mt_links::{parse_share_link, Profile, ProfileStore, Protocol, Security, Transport};
use mt_net::{
    Action, Host, MergePolicy, Outbound, ProxyEvent, ProxyServer, Router, Rule, RuleMatcher,
    TargetAddr,
};
use mt_platform::NetAdapter;
use mt_service::{
    AdapterView, ExitInfo, LinkStatus, MtService, ProfileView, RuleView, ServiceEvent,
    ServiceState, SettingsView, StatusSnapshot,
};
use tokio::sync::broadcast;

use crate::Error;

/// Default local SOCKS5 listen port (v2rayN convention).
pub const DEFAULT_SOCKS_PORT: u16 = 10808;
/// Default local HTTP proxy listen port.
pub const DEFAULT_HTTP_PORT: u16 = 10809;
/// Default full-round-trip probe target (`host:port`).
pub const DEFAULT_PROBE_TARGET: &str = "connectivitycheck.gstatic.com:80";
/// Budget for the connect-time link probe (covers a first-run engine
/// download; the engine downloads only via `locator.ensure()`).
const CONNECT_PROBE_BUDGET: Duration = Duration::from_secs(15);
/// How long `connect` waits for the server listeners to bind.
const BIND_WAIT: Duration = Duration::from_secs(2);
/// Bind-wait poll slice.
const BIND_POLL: Duration = Duration::from_millis(10);
/// `ServiceEvent` broadcast buffer (slow GUI receivers lag, never block).
const EVENT_BUFFER: usize = 256;
/// The flow the engine pins (mt-engine hardcodes vision in its config).
const ENGINE_FLOW: &str = "xtls-rprx-vision";

/// Filesystem locations the service works with (injected in tests).
#[derive(Debug, Clone)]
pub struct ServicePaths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl ServicePaths {
    /// Platform-resolved paths (mt-platform, env overrides honored).
    pub fn from_platform() -> Self {
        Self {
            config_dir: mt_platform::config_dir(),
            state_dir: mt_platform::state_dir(),
            log_dir: mt_platform::log_dir(),
        }
    }

    fn profiles_file(&self) -> PathBuf {
        self.config_dir.join("profiles.json")
    }

    fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
}

/// Builds the outbound links for one profile.
///
/// The seam that keeps the Xray engine out of RealService's test surface:
/// production ([`EngineLinkFactory`]) resolves the locator and constructs a
/// [`ManagedEngine`] per link; tests inject fakes. `bonded` is the resolved
/// adapter selection (S7): empty → one unbound link; one entry per bonded
/// adapter otherwise. `Err` messages must be credential-free (they become
/// UI error strings).
#[async_trait::async_trait]
pub trait LinkFactory: Send + Sync {
    async fn build(
        &self,
        profile: &Profile,
        bonded: &[BondedAdapter],
    ) -> anyhow::Result<Vec<Arc<dyn Outbound>>>;
}

/// One selected adapter resolved to an engine bind address (S7). Built by
/// [`resolve_bonded`] at connect time; consumed by [`EngineLinkFactory`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondedAdapter {
    /// Stable adapter id (as enumerated — Windows GUID / Linux ifname).
    pub id: String,
    /// Friendly name for engine tags and logs ("Wi-Fi").
    pub name: String,
    /// The local IPv4 the engine's `sendThrough` binds to.
    pub bind_ip: String,
}

/// Resolve persisted bonded-adapter ids against `adapters` (pure — the
/// caller passes the CURRENT enumeration so tests can inject fixtures).
/// Fail-open per id: unknown ids and IPv4-less adapters are skipped with a
/// warning (see module note 15); the remaining adapters keep their
/// settings-file order, duplicates collapse.
pub fn resolve_bonded(ids: &[String], adapters: &[NetAdapter]) -> Vec<BondedAdapter> {
    let mut out: Vec<BondedAdapter> = Vec::new();
    for id in ids {
        if out.iter().any(|b| &b.id == id) {
            continue;
        }
        let Some(adapter) = adapters.iter().find(|a| &a.id == id) else {
            tracing::warn!("bonded adapter {id:?} is not present on this machine; skipping");
            continue;
        };
        // IPv4 list is normal-first/link-local-last (mt-platform), so
        // `first` is the right bind address.
        let Some(ip) = adapter.ipv4.first() else {
            tracing::warn!(
                "bonded adapter {:?} has no IPv4 address (down or IPv6-only); skipping",
                adapter.name
            );
            continue;
        };
        out.push(BondedAdapter {
            id: adapter.id.clone(),
            name: adapter.name.clone(),
            bind_ip: ip.to_string(),
        });
    }
    out
}

/// Engine specs for one profile + resolved bonded adapters (pure; the
/// testable seam behind [`EngineLinkFactory`]). Empty `bonded` → exactly
/// one UNBOUND spec (v0.3 behavior). Otherwise one spec per adapter, tag
/// `"<profile-tag> [<name>]"` (deduplicated — two adapters may share a
/// friendly name), `send_through` = the adapter's bind IPv4.
pub fn engine_specs_for(profile: &Profile, bonded: &[BondedAdapter]) -> Vec<mt_engine::EngineSpec> {
    let Ok(endpoint) = endpoint_from_profile(profile) else {
        // Conversion errors are the factory's job to report (it re-runs
        // `endpoint_from_profile`); specs_for only runs on convertible
        // profiles in practice.
        return Vec::new();
    };
    if bonded.is_empty() {
        return vec![mt_engine::EngineSpec {
            tag: profile.tag.clone(),
            endpoint,
            send_through: None,
            log_level: None,
        }];
    }
    let mut used_names: Vec<String> = Vec::new();
    let mut specs = Vec::with_capacity(bonded.len());
    for a in bonded {
        let mut name = a.name.clone();
        if name.trim().is_empty() {
            name = a.id.clone();
        }
        // Distinct adapters can share a friendly name (two "Ethernet");
        // engine tag → config/log filename, so it MUST be unique.
        if used_names.contains(&name) {
            let mut n = 2;
            while used_names.contains(&format!("{name} #{n}")) {
                n += 1;
            }
            name = format!("{name} #{n}");
        }
        used_names.push(name.clone());
        specs.push(mt_engine::EngineSpec {
            tag: format!("{} [{}]", profile.tag, name),
            endpoint: endpoint.clone(),
            send_through: Some(a.bind_ip.clone()),
            log_level: None,
        });
    }
    specs
}

/// Production factory: one [`ManagedEngine`] per VLESS+REALITY+TCP profile.
///
/// The engine is constructed lazily — it spawns its child on the first
/// probe/connect, resolving the binary through the locator (`ensure()` may
/// download; that happens inside the probe budget, not at GUI startup).
pub struct EngineLinkFactory {
    locator: Arc<dyn mt_engine::EngineLocator>,
    state_dir: PathBuf,
    log_dir: PathBuf,
    /// S9: the engine `loglevel` from Advanced settings, SHARED with the
    /// service Core (which refreshes it from settings.json at every
    /// connect; production new() passes the same cell into both).
    log_level: Arc<RwLock<Option<String>>>,
}

impl EngineLinkFactory {
    pub fn new(
        locator: Arc<dyn mt_engine::EngineLocator>,
        state_dir: PathBuf,
        log_dir: PathBuf,
    ) -> Self {
        Self {
            locator,
            state_dir,
            log_dir,
            log_level: Arc::new(RwLock::new(None)),
        }
    }

    /// Share the log-level cell with the service Core (production wiring).
    pub fn shared_log_level(mut self, cell: Arc<RwLock<Option<String>>>) -> Self {
        self.log_level = cell;
        self
    }
}

#[async_trait::async_trait]
impl LinkFactory for EngineLinkFactory {
    async fn build(
        &self,
        profile: &Profile,
        bonded: &[BondedAdapter],
    ) -> anyhow::Result<Vec<Arc<dyn Outbound>>> {
        // Conversion first: a non-convertible profile is a user-facing
        // error (module note 5), not an empty link list.
        endpoint_from_profile(profile).map_err(anyhow::Error::msg)?;
        let mut specs = engine_specs_for(profile, bonded);
        // S9: apply the Advanced-settings log level to every engine spec.
        let level = wlock(&self.log_level).clone();
        for spec in &mut specs {
            spec.log_level = level.clone();
        }
        if specs.is_empty() {
            anyhow::bail!(
                "profile {:?}: no engine specs (endpoint conversion failed)",
                profile.tag
            );
        }
        Ok(specs
            .into_iter()
            .map(|spec| {
                Arc::new(mt_engine::ManagedEngine::new(
                    spec,
                    Arc::clone(&self.locator),
                    self.state_dir.clone(),
                    self.log_dir.clone(),
                )) as Arc<dyn Outbound>
            })
            .collect())
    }
}

/// Convert an mt-links profile into the engine endpoint the v0.3 wire-up
/// supports: `Vless` × `Reality` × `Transport::Tcp` only. Errors are polite,
/// credential-free and name the offending capability (never its values).
pub fn endpoint_from_profile(profile: &Profile) -> Result<mt_engine::VlessRealityEndpoint, Error> {
    if profile.protocol != Protocol::Vless {
        return Err(Error(format!(
            "profile {:?}: protocol '{}' cannot drive the built-in engine (v0.3 wires VLESS); \
             pick a vless profile",
            profile.tag,
            protocol_name(profile.protocol)
        )));
    }
    let Security::Reality {
        sni,
        fingerprint,
        public_key,
        short_id,
        spider_x,
    } = &profile.security
    else {
        return Err(Error(format!(
            "profile {:?}: security '{}' is not wired yet (v0.3 wires REALITY only); \
             re-import a security=reality link",
            profile.tag,
            security_name(&profile.security)
        )));
    };
    if !matches!(profile.transport, Transport::Tcp) {
        return Err(Error(format!(
            "profile {:?}: transport '{}' is not wired yet (v0.3 wires plain TCP only); \
             re-import a type=tcp link",
            profile.tag,
            transport_name(&profile.transport)
        )));
    }
    if let Some(flow) = &profile.flow {
        if flow != ENGINE_FLOW {
            return Err(Error(format!(
                "profile {:?}: the engine pins flow '{ENGINE_FLOW}', the profile requests \
                 a different flow; remove the flow parameter or use a vision server",
                profile.tag
            )));
        }
    }
    Ok(mt_engine::VlessRealityEndpoint {
        address: profile.address.clone(),
        port: profile.port,
        uuid: profile.identity.clone(),
        sni: sni.clone(),
        public_key: public_key.clone(),
        short_id: short_id.clone(),
        fingerprint: fingerprint.clone(),
        spider_x: Some(spider_x.clone().unwrap_or_else(|| "/".to_string())),
    })
}

fn protocol_name(p: Protocol) -> &'static str {
    match p {
        Protocol::Vless => "vless",
        Protocol::Vmess => "vmess",
        Protocol::Trojan => "trojan",
        Protocol::Shadowsocks => "shadowsocks",
    }
}

fn security_name(s: &Security) -> &'static str {
    match s {
        Security::None => "none",
        Security::Tls { .. } => "tls",
        Security::Reality { .. } => "reality",
    }
}

fn transport_name(t: &Transport) -> &'static str {
    match t {
        Transport::Tcp => "tcp",
        Transport::Ws { .. } => "ws",
        Transport::Grpc { .. } => "grpc",
        Transport::HttpUpgrade { .. } => "httpupgrade",
    }
}

/// Lenient `merge_policy` string → policy. Unknown values fall back to
/// `RoundRobin` with a warning — a fail-open default keeps the tunnel up
/// (the D20 rationale). The GUI sends the canonical lowercase hyphenated
/// names (its pick-list), which are accepted verbatim; the un-hyphenated
/// mt-net serde names are accepted too. S11 adds `balanced` (live
/// byte-load spreading — the v0.7.0 default) and `priority` (user
/// adapter order with saturation spill).
pub fn merge_policy_from_str(s: &str) -> MergePolicy {
    match s {
        "roundrobin" | "round-robin" => MergePolicy::RoundRobin,
        "lowestlatency" | "lowest-latency" => MergePolicy::LowestLatency,
        "failover" => MergePolicy::Failover,
        "balanced" => MergePolicy::Balanced,
        "priority" | "priority-order" | "waterfall" => MergePolicy::Priority,
        other => {
            tracing::warn!("unknown merge policy {other:?}; using round-robin");
            MergePolicy::RoundRobin
        }
    }
}

/// Does `s` name a KNOWN merge policy (any accepted spelling)? Used by
/// `set_settings` so a typo'd policy fails loud instead of silently
/// degrading to the fallback.
pub fn is_known_merge_policy(s: &str) -> bool {
    matches!(
        s,
        "roundrobin"
            | "round-robin"
            | "lowestlatency"
            | "lowest-latency"
            | "failover"
            | "balanced"
            | "priority"
            | "priority-order"
            | "waterfall"
    )
}

/// `RuleView` (GUI) → `mt_net::Rule` (data plane). Errors are UI strings.
fn rule_from_view(v: &RuleView) -> Result<Rule, String> {
    let matcher = match v.kind.as_str() {
        "domainSuffix" => RuleMatcher::DomainSuffix {
            suffix: v.value.clone(),
        },
        "domainKeyword" => RuleMatcher::DomainKeyword {
            keyword: v.value.clone(),
        },
        "ipCidr" => RuleMatcher::IpCidr {
            cidr: v.value.clone(),
        },
        "any" => RuleMatcher::Any,
        other => return Err(format!("unknown rule kind {other:?}")),
    };
    let action = match v.action.as_str() {
        "proxy" => Action::Proxy,
        "direct" => Action::Direct,
        "reject" => Action::Reject,
        other => return Err(format!("unknown rule action {other:?}")),
    };
    Ok(Rule { matcher, action })
}

fn rule_to_view(r: &Rule) -> RuleView {
    let (kind, value) = match &r.matcher {
        RuleMatcher::DomainSuffix { suffix } => ("domainSuffix", suffix.clone()),
        RuleMatcher::DomainKeyword { keyword } => ("domainKeyword", keyword.clone()),
        RuleMatcher::IpCidr { cidr } => ("ipCidr", cidr.clone()),
        RuleMatcher::Any => ("any", String::new()),
    };
    let action = match r.action {
        Action::Proxy => "proxy",
        Action::Direct => "direct",
        Action::Reject => "reject",
    };
    RuleView {
        kind: kind.to_string(),
        value,
        action: action.to_string(),
    }
}

/// On-disk settings document (settings.json; no credentials inside).
/// `socks_port`/`http_port` are the local listeners, `probe_target` is
/// `host:port`. Serialized with mt-net's rule shapes verbatim.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct SettingsFile {
    pub settings: SettingsView,
    pub rules: Vec<Rule>,
    pub socks_port: u16,
    pub http_port: u16,
    pub probe_target: String,
    /// Schema version for stock-default migrations. Absent (v0.5.0 and
    /// older) deserializes as 0 and is migrated on load.
    #[serde(default)]
    pub settings_version: u32,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            settings: SettingsView {
                // Default seed: the Material baseline primary #6750A4.
                theme_seed: 0x6750A4,
                dark_mode: false,
                // S18: fresh installs follow the OS appearance (the user's
                // ask); the legacy dark_mode stays false so an old binary
                // reading this file lands on light, matching what a
                // light-mode OS user sees.
                theme_mode: "system".to_string(),
                system_proxy: false,
                // D23: "balanced" spreads new connections across the
                // ticked adapters by LIVE byte load — the v0.7.0 default.
                // Round-robin's connection-count evenness let one fat
                // flow dominate a single adapter (the "one adapter hogs
                // everything" report); failover (v0.5.0's default) left
                // the second adapter idle entirely.
                merge_policy: "balanced".to_string(),
                autostart: false,
                bonded_adapters: Vec::new(),
                bond_mode: "per-connection".to_string(),
                merge_port: 7443,
                socks_port: DEFAULT_SOCKS_PORT,
                http_port: DEFAULT_HTTP_PORT,
                allow_lan: false,
                log_level: "warning".to_string(),
                probe_target: DEFAULT_PROBE_TARGET.to_string(),
                exit_probe_url: "http://ip-api.com/json".to_string(),
                close_to_tray: false,
                always_on_top: false,
            },
            // Stock rules = the LAN/private bypass (S6-REV-C P1-4): a fresh
            // settings.json must carry private-CIDR → direct rules, mirroring
            // v0.2's `geoip:private → direct` invariant, so router admin
            // pages / printers / NAS never ride the tunnel.
            rules: default_lan_rules(),
            socks_port: DEFAULT_SOCKS_PORT,
            http_port: DEFAULT_HTTP_PORT,
            probe_target: DEFAULT_PROBE_TARGET.to_string(),
            settings_version: SETTINGS_VERSION,
        }
    }
}

/// Current settings schema version (D20 = 1: stock failover →
/// round-robin; D23 = 2: stock round-robin → balanced).
const SETTINGS_VERSION: u32 = 2;

impl SettingsFile {
    fn load(path: &Path) -> Self {
        let mut file = match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str::<SettingsFile>(&raw).unwrap_or_else(|e| {
                tracing::warn!("settings.json unreadable ({e}); using defaults");
                Self::default()
            }),
            Err(_) => Self::default(),
        };
        // S9 migration: v0.4 persisted socks/http/probe at the TOP LEVEL
        // (the view did not carry them). Top-level wins on load — it is
        // what v0.4 actually used at runtime, INCLUDING port 0 ("ephemeral
        // bind", the test/embedder mode) — then both copies agree.
        file.settings.socks_port = file.socks_port;
        file.settings.http_port = file.http_port;
        if !file.probe_target.trim().is_empty() {
            file.settings.probe_target = file.probe_target.clone();
        }
        // D20 migration (v0.5.0 → v0.5.1): v0.5.0 wrote the then-stock
        // "failover" merge policy, which sends EVERY connection down the
        // first healthy adapter — a second ticked adapter stayed idle and
        // users read that as broken bonding. Migrate the STOCK value to
        // the new default; an explicit re-choice persists version >= 1.
        //
        // D23 migration (v0.6.x → v0.7.0): the same one-time move for
        // "round-robin" → "balanced". v0.5.1..v0.6.1 shipped round-robin
        // as the STOCK default, so a version<2 file carrying it is
        // indistinguishable from an explicit choice — following the D20
        // precedent, the stock value migrates once; an explicit
        // re-choice of round-robin persists (version 2) verbatim.
        let migrated = file.settings_version < SETTINGS_VERSION;
        if migrated {
            if file.settings_version < 1 && file.settings.merge_policy == "failover" {
                file.settings.merge_policy = "round-robin".to_string();
                tracing::info!("settings migration: stock merge policy failover → round-robin");
            }
            if file.settings_version < 2 && file.settings.merge_policy == "round-robin" {
                file.settings.merge_policy = "balanced".to_string();
                tracing::info!("settings migration: stock merge policy round-robin → balanced");
            }
            file.settings_version = SETTINGS_VERSION;
            // Best-effort persist so the Settings screen shows the
            // migrated value even before the next explicit save.
            if let Err(e) = file.save(path) {
                tracing::warn!("could not persist settings migration: {e}");
            }
        }
        file
    }

    /// Persist the view's advanced fields into the legacy top-level slots
    /// too (v0.4 CLI paths read those; one file, two views, same truth).
    fn sync_legacy(&mut self) {
        self.socks_port = self.settings.socks_port;
        self.http_port = self.settings.http_port;
        self.probe_target = self.settings.probe_target.clone();
    }

    fn save(&self, path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error(format!("create {}: {e}", parent.display())))?;
        }
        let blob = serde_json::to_string_pretty(self)
            .map_err(|e| Error(format!("serialize settings: {e}")))?;
        crate::fsutil::write_private_atomic(path, &(blob + "\n"))
            .map_err(|e| Error(format!("write {}: {e}", path.display())))
    }
}

/// Poison-tolerant std Mutex lock (global rule 5).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Poison-tolerant std RwLock write lock (the state cell is tiny; reads take
/// the write lock too, which keeps transitions atomic).
fn wlock<T>(m: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Supervision cadence (S6-REV-C P1-3c): the event pump doubles as the
/// session supervisor. Every [`SUPERVISE_INTERVAL`] the link health is
/// checked; if EVERY link is down for [`SUPERVISE_STRIKES`] consecutive
/// checks (after [`SUPERVISE_GRACE`]), the D5 fail-open give-up fires:
/// system proxy restored, State::Failed, session stopped — the v0.2
/// supervisor semantics for restart exhaustion.
const SUPERVISE_INTERVAL: Duration = Duration::from_secs(5);
/// Grace after session start before all-down strikes count (lets the
/// probers establish a first health picture; health starts optimistic).
const SUPERVISE_GRACE: Duration = Duration::from_secs(15);
/// Consecutive all-down checks before the give-up fires.
const SUPERVISE_STRIKES: u32 = 3;

/// Stock routing rules: private/LAN CIDRs → Direct (S6-REV-C P1-4 — the
/// v0.2 `geoip:private → direct` invariant). Applied when settings.json
/// carries NO rules (an empty list means "stock"); a user-defined rule
/// list is respected verbatim.
fn default_lan_rules() -> Vec<Rule> {
    [
        "127.0.0.0/8",
        "::1/128",
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
        "169.254.0.0/16",
        "fe80::/10",
        "fc00::/7",
    ]
    .iter()
    .map(|cidr| Rule {
        matcher: RuleMatcher::IpCidr {
            cidr: (*cidr).to_string(),
        },
        action: Action::Direct,
    })
    .collect()
}

/// One live connect session (server + its tasks + lifecycle signals).
struct Session {
    server: Arc<ProxyServer>,
    /// S9: present when stream bonding engaged — the per-carrier status
    /// surface AND the owner of the per-adapter engines.
    bond: Option<Arc<BondOutbound>>,
    shutdown: tokio::sync::watch::Sender<bool>,
    run_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
    pump_handle: tokio::task::JoinHandle<()>,
    started_at: SystemTime,
}

/// Shared service state. Every field is either atomic or behind a
/// poison-tolerant std lock; no lock is ever held across an await.
struct Core {
    paths: ServicePaths,
    factory: Arc<dyn LinkFactory>,
    /// S9: engine loglevel cell shared with the production factory
    /// (dummy cell when a test factory is injected).
    engine_log_level: Arc<RwLock<Option<String>>>,
    state: RwLock<ServiceState>,
    events: broadcast::Sender<ServiceEvent>,
    session: Mutex<Option<Session>>,
    /// Per-link `(up, down)` byte totals accrued from `ConnClosed` events.
    bytes: Mutex<HashMap<String, (u64, u64)>>,
    sysproxy_set: AtomicBool,
    disconnect_requested: AtomicBool,
    restarts: AtomicU32,
}

impl Core {
    fn set_state(&self, s: ServiceState) {
        *wlock(&self.state) = s;
        let _ = self.events.send(ServiceEvent::StateChanged(s));
    }

    fn get_state(&self) -> ServiceState {
        *wlock(&self.state)
    }

    fn emit(&self, ev: ServiceEvent) {
        let _ = self.events.send(ev);
    }

    /// Strict store load: a corrupt profiles.json must NEVER be silently
    /// discarded (the next save would wipe the user's profiles).
    fn load_store(&self) -> Result<ProfileStore, Error> {
        ProfileStore::load(&self.paths.profiles_file())
            .map_err(|e| Error(format!("load profiles.json: {e}")))
    }

    /// Lossy load for infallible read paths (status/profiles views): a
    /// corrupt file degrades to an empty view with a warning, never a panic
    /// and never a write.
    fn load_store_lossy(&self) -> ProfileStore {
        self.load_store().unwrap_or_else(|e| {
            tracing::warn!("{e}");
            ProfileStore::default()
        })
    }

    fn save_store(&self, store: &ProfileStore) -> Result<(), Error> {
        if let Some(parent) = self.paths.config_dir.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error(format!("create {}: {e}", parent.display())))?;
        }
        std::fs::create_dir_all(&self.paths.config_dir)
            .map_err(|e| Error(format!("create {}: {e}", self.paths.config_dir.display())))?;
        store
            .save(&self.paths.profiles_file())
            .map_err(|e| Error(format!("save profiles: {e}")))
    }
}

/// The production [`MtService`]: ProfileStore + ProxyServer + ManagedEngine
/// + mt-platform + legacy sysproxy, glued per the module docs.
#[derive(Clone)]
pub struct RealService {
    core: Arc<Core>,
}

impl RealService {
    /// Production constructor: platform paths + engine-backed links.
    pub fn new() -> Self {
        let paths = ServicePaths::from_platform();
        let cell: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));
        let factory: Arc<dyn LinkFactory> = Arc::new(
            EngineLinkFactory::new(
                Arc::new(mt_platform::DefaultEngineLocator::new()),
                paths.state_dir.clone(),
                paths.log_dir.clone(),
            )
            .shared_log_level(Arc::clone(&cell)),
        );
        let mut svc = Self::with(paths, factory);
        Arc::get_mut(&mut svc.core)
            .expect("RealService::new owns the only Arc")
            .engine_log_level = cell;
        svc
    }

    /// Injected paths + link factory (tests, embedders).
    pub fn with(paths: ServicePaths, factory: Arc<dyn LinkFactory>) -> Self {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Self {
            core: Arc::new(Core {
                paths,
                factory,
                engine_log_level: Arc::new(RwLock::new(None)),
                state: RwLock::new(ServiceState::Disconnected),
                events,
                session: Mutex::new(None),
                bytes: Mutex::new(HashMap::new()),
                sysproxy_set: AtomicBool::new(false),
                disconnect_requested: AtomicBool::new(false),
                restarts: AtomicU32::new(0),
            }),
        }
    }

    /// The bound local listeners of the live session, `(socks, http)`.
    /// Diagnostics hook (CLI/tests); the GUI sees state through the trait.
    pub fn session_addrs(&self) -> (Option<SocketAddr>, Option<SocketAddr>) {
        let guard = lock(&self.core.session);
        match guard.as_ref() {
            Some(s) => (s.server.socks_local_addr(), s.server.http_local_addr()),
            None => (None, None),
        }
    }

    /// Transition into `Connecting` (no-op otherwise); returns whether this
    /// caller owns the connect attempt. A stale `disconnect_requested` flag
    /// from a previous cycle is cleared HERE (S6-REV-C P2-1): a disconnect
    /// landing between the post-probe check and session installation must
    /// not abort the NEXT connect.
    fn begin_connect(&self) -> bool {
        let mut st = wlock(&self.core.state);
        if !matches!(*st, ServiceState::Disconnected | ServiceState::Failed) {
            return false;
        }
        *st = ServiceState::Connecting;
        drop(st);
        self.core
            .disconnect_requested
            .store(false, Ordering::SeqCst);
        self.core
            .emit(ServiceEvent::StateChanged(ServiceState::Connecting));
        true
    }

    /// Shared connect-failure landing: log, emit a LogLine, mark Failed.
    async fn fail_connect(core: &Core, reason: String) {
        tracing::warn!("connect failed: {reason}");
        core.emit(ServiceEvent::LogLine(format!("connect failed: {reason}")));
        core.set_state(ServiceState::Failed);
    }

    /// Build links + server, probe, and finish the state transition.
    async fn connect_flow(&self) {
        // 0. Stale sysproxy self-heal (v0.2 connect step 0 / D5,
        //    S6-REV-C P1-3b): a previous run that died without disconnecting
        //    may have left the registry pointing at a dead local listener.
        //    Best-effort — a failure never blocks connecting.
        let _ = crate::sysproxy::restore(&self.core.paths.state_dir);

        let settings = SettingsFile::load(&self.core.paths.settings_file());
        // S9: refresh the engine log level BEFORE links are built.
        *wlock(&self.core.engine_log_level) = Some(settings.settings.log_level.clone());
        let store = match self.core.load_store() {
            Ok(s) => s,
            Err(e) => {
                Self::fail_connect(&self.core, format!("{e}")).await;
                return;
            }
        };

        let profile = store
            .active
            .as_deref()
            .and_then(|id| store.profiles.iter().find(|p| p.id == id))
            .cloned();
        let Some(profile) = profile else {
            Self::fail_connect(&self.core, "no active profile — import one first".into()).await;
            return;
        };

        let bonded = resolve_bonded(
            &settings.settings.bonded_adapters,
            &mt_platform::enumerate_adapters(),
        );
        // D20 visibility: a ticked adapter that resolve_bonded SKIPPED
        // (absent at connect time, or no IPv4) was silent tracing-only —
        // the user ticks two adapters, gets one link, and reads the idle
        // second NIC as broken bonding. Say it in the GUI log instead.
        for id in &settings.settings.bonded_adapters {
            if !bonded.iter().any(|a| &a.id == id) {
                self.core.emit(ServiceEvent::LogLine(format!(
                    "WARN  bonded adapter {id:?} skipped (not present or no IPv4) — \
                     check the Adapters tab"
                )));
            }
        }
        let links = match self.core.factory.build(&profile, &bonded).await {
            Ok(l) if !l.is_empty() => l,
            Ok(_) => {
                Self::fail_connect(&self.core, "link factory produced no links".into()).await;
                return;
            }
            Err(e) => {
                Self::fail_connect(&self.core, format!("{e}")).await;
                return;
            }
        };

        let policy = merge_policy_from_str(&settings.settings.merge_policy);
        // Empty rule list = stock rules (LAN/private bypass, S6-REV-C P1-4);
        // a user-defined list is respected verbatim.
        let rules = if settings.rules.is_empty() {
            default_lan_rules()
        } else {
            settings.rules.clone()
        };
        let router = Router::new(rules);
        // S9 (v2rayN parity): allow-LAN swaps the listener bind address.
        let bind_ip: [u8; 4] = if settings.settings.allow_lan {
            [0, 0, 0, 0]
        } else {
            [127, 0, 0, 1]
        };
        let socks = SocketAddr::from((bind_ip, settings.settings.socks_port));
        let http = SocketAddr::from((bind_ip, settings.settings.http_port));

        // S9 stream bonding (D17): ONE BondOutbound over the per-adapter
        // engine links — every connection's BYTES are split across the
        // carriers and merged by mt-merge-server on the tunnel server.
        // Carriers ride the engines via VLESS targets of 127.0.0.1:<merge
        // port> (server-side loopback, reached THROUGH each tunnel).
        let mut bond: Option<Arc<BondOutbound>> = None;
        let links: Vec<Arc<dyn Outbound>> =
            if settings.settings.bond_mode == "stream" && !links.is_empty() {
                let merge_target = TargetAddr {
                    host: Host::Ip("127.0.0.1".parse().expect("loopback parses")),
                    port: settings.settings.merge_port,
                };
                // The bond keeps its OWN Arc clones of the engines; the
                // originals stay usable for the per-connection fallback.
                let candidate = BondOutbound::with_policy(links.clone(), merge_target, policy);
                match candidate.ensure_carriers(CONNECT_PROBE_BUDGET).await {
                    Ok(report) => {
                        // D20: per-carrier verdicts in the GUI log — a dead
                        // carrier (e.g. a WiFi engine dial failure) must be
                        // VISIBLE, not just a tracing line in a file.
                        for line in &report.lines {
                            self.core
                                .emit(ServiceEvent::LogLine(format!("INFO  carrier {line}")));
                        }
                        if report.live < links.len() {
                            self.core.emit(ServiceEvent::LogLine(format!(
                                "WARN  stream bonding active with {}/{} carriers — bytes \
                                 ride the live adapter(s); the dead one keeps respawning",
                                report.live,
                                links.len()
                            )));
                        } else {
                            self.core.emit(ServiceEvent::LogLine(format!(
                                "INFO  stream bonding active — {} carriers, frames spread by the \
                                 '{}' merge policy across them",
                                report.live, settings.settings.merge_policy
                            )));
                        }
                        bond = Some(Arc::clone(&candidate));
                        vec![candidate as Arc<dyn Outbound>]
                    }
                    Err(e) => {
                        // Never black-hole (D17 §5): per-connection mode.
                        self.core.emit(ServiceEvent::LogLine(format!(
                            "WARN  stream bonding unavailable ({e}) — per-connection \
                             spreading instead. Likely causes: mt-merge-server not \
                             running on the tunnel server, or the server's Xray \
                             freedom policy blocks the merge port (Xray 26.x blackholes \
                             loopback targets from VLESS by default). Fix on the VM: \
                             sudo bash server/setup-xray-reality.sh (idempotently patches \
                             the Xray finalRules allow) + sudo bash server/setup-merge-daemon.sh"
                        )));
                        links
                    }
                }
            } else {
                links
            };
        let server = Arc::new(ProxyServer::new(socks, http, links.clone(), router, policy));
        let proxy_events = server.subscribe();

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let run_handle = tokio::spawn(Arc::clone(&server).run(shutdown_rx.clone()));
        let pump_handle = tokio::spawn(pump(
            Arc::clone(&self.core),
            Arc::clone(&server),
            proxy_events,
            shutdown_tx.clone(),
            shutdown_rx,
        ));

        // Wait for the listeners to bind (run binds at startup; a bind
        // failure ends the run task).
        let deadline = Instant::now() + BIND_WAIT;
        loop {
            if run_handle.is_finished() {
                let reason = match run_handle.await {
                    Ok(Err(e)) => format!("proxy server failed to start: {e}"),
                    Ok(Ok(())) => "proxy server exited immediately".to_string(),
                    Err(e) => format!("proxy server task panicked: {e}"),
                };
                let _ = shutdown_tx.send(true);
                drop(shutdown_tx);
                drop(server); // last sender Arc — lets the pump drain + exit
                let _ = pump_handle.await;
                Self::fail_connect(&self.core, reason).await;
                return;
            }
            if server.socks_local_addr().is_some() || Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(BIND_POLL).await;
        }

        let Some(proxy_addr) = server.socks_local_addr() else {
            let _ = shutdown_tx.send(true);
            drop(shutdown_tx);
            drop(server); // last sender Arc — lets the pump drain + exit
            let _ = run_handle.await;
            let _ = pump_handle.await;
            Self::fail_connect(&self.core, "proxy listeners did not bind in time".into()).await;
            return;
        };

        // Full round-trip probe through the link (engine spawn/download
        // included; the budget covers a first-run engine fetch).
        let probe_target = parse_probe_target(&settings.probe_target)
            .unwrap_or_else(|| parse_probe_target(DEFAULT_PROBE_TARGET).expect("constant parses"));
        let Some(link) = links.first().cloned() else {
            // Unreachable (an empty link list failed above) — belt and braces.
            self.abort_connect(shutdown_tx, run_handle, pump_handle)
                .await;
            Self::fail_connect(&self.core, "no links".into()).await;
            return;
        };
        tracing::info!("probing link {:?} via {proxy_addr}", link.name());
        let probe = tokio::time::timeout(CONNECT_PROBE_BUDGET, link.probe(&probe_target)).await;
        match probe {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                self.abort_connect(shutdown_tx, run_handle, pump_handle)
                    .await;
                Self::fail_connect(&self.core, format!("link probe failed: {e}")).await;
                return;
            }
            Err(_) => {
                self.abort_connect(shutdown_tx, run_handle, pump_handle)
                    .await;
                Self::fail_connect(&self.core, "link probe timed out".into()).await;
                return;
            }
        }

        // A disconnect request landing mid-connect wins.
        if self.core.disconnect_requested.swap(false, Ordering::SeqCst) {
            self.abort_connect(shutdown_tx, run_handle, pump_handle)
                .await;
            self.core.set_state(ServiceState::Disconnected);
            return;
        }

        let session = Session {
            server,
            bond,
            shutdown: shutdown_tx,
            run_handle,
            pump_handle,
            started_at: SystemTime::now(),
        };
        {
            let mut slot = lock(&self.core.session);
            if slot.is_some() {
                // Raced with another successful connect (should not happen:
                // begin_connect gates); keep the newest, drop the older.
                tracing::warn!("replacing an existing session");
            }
            *slot = Some(session);
        }
        self.core.bytes_reset();
        self.core.restarts.fetch_add(1, Ordering::SeqCst);
        self.core.set_state(ServiceState::Connected);

        // System proxy AFTER Connected, non-fatal (step 5 of the v0.2
        // connect_sequence discipline). Skipped for ephemeral binds (port 0
        // in settings — the tests' mode; a registry write would be pointless).
        if settings.settings.system_proxy && settings.socks_port != 0 && settings.http_port != 0 {
            let http_addr = lock(&self.core.session)
                .as_ref()
                .and_then(|s| s.server.http_local_addr());
            if let Some(http) = http_addr {
                // The v0.2 full LAN-bypass override (S6-REV-C P1-4): LAN and
                // loopback hosts must never ride the tunnel.
                let override_list = crate::config::default_proxy_override();
                match crate::sysproxy::set(
                    &http.to_string(),
                    &override_list,
                    &self.core.paths.state_dir,
                ) {
                    Ok(()) => {
                        self.core.sysproxy_set.store(true, Ordering::SeqCst);
                        self.core
                            .emit(ServiceEvent::LogLine("system proxy set".into()));
                    }
                    Err(e) => {
                        tracing::warn!("system proxy not set (tunnel stays up): {e}");
                        self.core.emit(ServiceEvent::LogLine(format!(
                            "system proxy not set (tunnel stays up): {e}"
                        )));
                    }
                }
            }
        }
        self.core.emit(ServiceEvent::LogLine(format!(
            "connected via {:?} — socks {proxy_addr}",
            links.first().map(|l| l.name()).unwrap_or("?")
        )));
    }

    /// Tear down an in-flight (not yet installed) session.
    async fn abort_connect(
        &self,
        shutdown: tokio::sync::watch::Sender<bool>,
        run_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
        pump_handle: tokio::task::JoinHandle<()>,
    ) {
        let _ = shutdown.send(true);
        drop(shutdown); // run() also exits when the sender drops
        let _ = run_handle.await;
        let _ = pump_handle.await;
    }
}

impl Default for RealService {
    fn default() -> Self {
        Self::new()
    }
}

/// Map one `ProxyEvent` stream into `ServiceEvent`s until shutdown, and
/// SUPERVISE the session (S6-REV-C P1-3c): every [`SUPERVISE_INTERVAL`] the
/// link health is checked; ALL links down for [`SUPERVISE_STRIKES`]
/// consecutive checks (after [`SUPERVISE_GRACE`]) fires the D5 fail-open
/// give-up — system proxy restored, `State::Failed`, session stopped (the
/// v0.2 supervisor semantics for restart exhaustion, which the engine's own
/// respawn budget exhaustion cannot express on its own).
async fn pump(
    core: Arc<Core>,
    server: Arc<ProxyServer>,
    mut events: broadcast::Receiver<ProxyEvent>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let started = tokio::time::Instant::now();
    let mut next_check = started + SUPERVISE_INTERVAL;
    let mut strikes = 0u32;
    loop {
        let ev = tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    // Final drain, then stop (senders live inside the server
                    // we are about to drop — never wait on channel closure).
                    while let Ok(ev) = events.try_recv() {
                        handle_proxy_event(&core, ev).await;
                    }
                    break;
                }
                continue;
            }
            // sleep_until (not sleep) + biased order: a steady event stream
            // can never starve the health check — once the deadline passes
            // this branch is immediately ready and polled before `recv`.
            _ = tokio::time::sleep_until(next_check) => {
                next_check = tokio::time::Instant::now() + SUPERVISE_INTERVAL;
                if started.elapsed() >= SUPERVISE_GRACE {
                    let health = server.health_snapshot().await;
                    if !health.is_empty() && health.iter().all(|(_, h)| !h.ok) {
                        strikes += 1;
                        tracing::warn!(
                            "supervisor: all links down ({strikes}/{SUPERVISE_STRIKES} checks)"
                        );
                        if strikes >= SUPERVISE_STRIKES {
                            give_up(&core, &shutdown_tx).await;
                            break;
                        }
                    } else {
                        strikes = 0;
                    }
                }
                continue;
            }
            ev = events.recv() => match ev {
                Ok(ev) => ev,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("proxy event pump lagged; {n} events dropped");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        };
        handle_proxy_event(&core, ev).await;
    }
}

/// D5 fail-open give-up (v0.2 supervisor semantics for restart exhaustion):
/// restore the system proxy, retire the session, mark `Failed`, and stop
/// the server. Best-effort throughout — the user's browsing keeps working.
async fn give_up(core: &Core, shutdown: &tokio::sync::watch::Sender<bool>) {
    tracing::warn!("supervisor: all tunnel links exhausted — giving up (fail-open)");
    if core.sysproxy_set.swap(false, Ordering::SeqCst) {
        if let Err(e) = crate::sysproxy::restore(&core.paths.state_dir) {
            tracing::warn!("system proxy restore failed: {e}");
        }
    }
    // Retire the session so status/profiles views stop reporting the dead
    // server (the next connect replaces it anyway; dropping it also drops
    // the last server Arc → run() exits).
    drop(lock(&core.session).take());
    core.emit(ServiceEvent::LogLine(
        "all tunnel links down — disconnected (system proxy restored)".into(),
    ));
    core.set_state(ServiceState::Failed);
    let _ = shutdown.send(true);
}

async fn handle_proxy_event(core: &Core, ev: ProxyEvent) {
    match ev {
        ProxyEvent::Started => core.emit(ServiceEvent::LogLine("proxy server started".into())),
        ProxyEvent::Stopped => core.emit(ServiceEvent::LogLine("proxy server stopped".into())),
        ProxyEvent::ConnOpened { target, link } => {
            core.emit(ServiceEvent::LogLine(format!("conn {target} via {link}")))
        }
        ProxyEvent::ConnClosed {
            link,
            up,
            down,
            dur_ms,
        } => {
            core.bytes_add(&link, up, down);
            core.emit(ServiceEvent::LogLine(format!(
                "done {link}: +{up}B -{down}B in {dur_ms}ms"
            )));
            core.emit(ServiceEvent::LinkUpdated(link_status_of(core, &link, None)));
        }
        ProxyEvent::LinkHealth { link, health } => {
            core.emit(ServiceEvent::LinkUpdated(link_status_of(
                core,
                &link,
                Some(&health),
            )));
        }
        ProxyEvent::Tick { up_bps, down_bps } => {
            core.emit(ServiceEvent::Throughput { up_bps, down_bps })
        }
    }
}

fn link_status_of(core: &Core, link: &str, health: Option<&mt_net::LinkHealth>) -> LinkStatus {
    let (up, down) = core.bytes_of(link);
    let (ok, latency_ms, ewma_ms, quarantined) = match health {
        Some(h) => (
            h.ok,
            h.latency
                .map(|d| d.as_millis().min(u128::from(u32::MAX)) as u32),
            (h.ewma_ms > 0.0).then_some(h.ewma_ms),
            h.quarantined_until
                .is_some_and(|until| until > Instant::now()),
        ),
        None => (true, None, None, false),
    };
    LinkStatus {
        name: link.to_string(),
        ok,
        latency_ms,
        ewma_ms,
        quarantined,
        bytes_up: up,
        bytes_down: down,
    }
}

impl Core {
    fn bytes_add(&self, link: &str, up: u64, down: u64) {
        let mut map = lock(&self.bytes);
        let e = map.entry(link.to_string()).or_default();
        e.0 = e.0.saturating_add(up);
        e.1 = e.1.saturating_add(down);
    }

    fn bytes_of(&self, link: &str) -> (u64, u64) {
        *lock(&self.bytes).get(link).unwrap_or(&(0, 0))
    }

    fn bytes_reset(&self) {
        lock(&self.bytes).clear();
    }
}

/// Overall budget for one exit-IP probe (D18).
const EXIT_PROBE_BUDGET: Duration = Duration::from_secs(12);

/// Parse a plain-HTTP probe URL into `(host, port, path)` (D18). Only
/// `http://` URLs are accepted — by design (no TLS stack in the tree).
pub fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        // Beware IPv6 literals — this simple parser rejects them (the
        // stock probe URL is a domain; power users can edit settings).
        Some((h, p)) => (h, p.parse().ok()?),
        None => (authority, 80),
    };
    if host.is_empty() || host.contains('[') {
        return None;
    }
    Some((host.to_string(), port, path))
}

/// `host:port` → [`TargetAddr`] (IP literals become [`Host::Ip`]; brackets
/// for IPv6 are unwrapped by the v0.2 parser we reuse).
pub fn parse_probe_target(spec: &str) -> Option<TargetAddr> {
    let (host, port) = crate::supervisor::parse_probe_target(spec)?;
    let host = match host.parse() {
        Ok(ip) => Host::Ip(ip),
        Err(_) => Host::Domain(host),
    };
    Some(TargetAddr { host, port })
}

/// Dependency-free UTC formatting (`civil_from_days`, Hinnant 2016) for
/// `StatusSnapshot::up_since` — no chrono/time dependency in the root crate.
pub fn fmt_utc(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

#[async_trait::async_trait]
impl MtService for RealService {
    async fn status(&self) -> StatusSnapshot {
        let state = self.core.get_state();
        let store = self.core.load_store_lossy();
        let active_profile = store.active.as_deref().and_then(|id| {
            store
                .profiles
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.tag.clone())
        });
        let (up_since, server, bond) = {
            let guard = lock(&self.core.session);
            match guard.as_ref() {
                Some(s) => (
                    Some(fmt_utc(s.started_at)),
                    Some(Arc::clone(&s.server)),
                    s.bond.clone(),
                ),
                None => (None, None, None),
            }
        }; // guard dropped before the await below (never hold a std lock
           // across .await — real_service decision #13)
           // Stream mode: the Status rows are the BOND CARRIERS (one per
           // bonded adapter, with wire byte counters + ping RTT). Per-
           // connection mode keeps the aggregator's health table.
        let links = if let Some(bond) = bond {
            bond.carrier_snapshot()
                .into_iter()
                .map(|c| LinkStatus {
                    name: c.name,
                    ok: c.alive,
                    latency_ms: c.rtt_ms,
                    ewma_ms: c.rtt_ms.map(|r| r as f32),
                    quarantined: !c.alive,
                    bytes_up: c.bytes_up,
                    bytes_down: c.bytes_down,
                })
                .collect()
        } else {
            match server {
                Some(server) => server
                    .health_snapshot()
                    .await
                    .iter()
                    .map(|(name, h)| link_status_of(&self.core, name, Some(h)))
                    .collect(),
                None => Vec::new(),
            }
        };
        StatusSnapshot {
            state,
            active_profile,
            up_since,
            restarts: self.core.restarts.load(Ordering::SeqCst),
            links,
        }
    }

    async fn connect(&self) {
        if !self.begin_connect() {
            tracing::info!("connect ignored: state is {:?}", self.core.get_state());
            return;
        }
        self.connect_flow().await;
    }

    async fn disconnect(&self) {
        let session = lock(&self.core.session).take();
        let Some(session) = session else {
            // Nothing installed: a connect may still be in flight — flag it
            // so the in-flight flow aborts right after its probe.
            if self.core.get_state() == ServiceState::Connecting {
                self.core.disconnect_requested.store(true, Ordering::SeqCst);
            }
            return;
        };
        // S9: stop the bond's carriers/sweeper before the engines drop.
        if let Some(bond) = &session.bond {
            bond.shutdown();
        }
        let _ = session.shutdown.send(true);
        drop(session.shutdown); // run() also exits when the sender drops
        let _ = session.run_handle.await;
        let _ = session.pump_handle.await;
        if self.core.sysproxy_set.swap(false, Ordering::SeqCst) {
            if let Err(e) = crate::sysproxy::restore(&self.core.paths.state_dir) {
                tracing::warn!("system proxy restore failed: {e}");
            }
        }
        self.core.bytes_reset();
        self.core.set_state(ServiceState::Disconnected);
        self.core.emit(ServiceEvent::LogLine("disconnected".into()));
    }

    async fn profiles(&self) -> Vec<ProfileView> {
        let store = self.core.load_store_lossy();
        store
            .profiles
            .iter()
            .map(|p| ProfileView {
                active: store.active.as_deref() == Some(p.id.as_str()),
                id: p.id.clone(),
                tag: p.tag.clone(),
                protocol: protocol_name(p.protocol).to_string(),
                endpoint: format!("{}:{}", p.address, p.port),
                remark: p.remark.clone(),
            })
            .collect()
    }

    async fn import_link(&self, link: &str) -> Result<ProfileView, String> {
        let profile = parse_share_link(link).map_err(|e| e.to_string())?;
        let mut store = self.core.load_store().map_err(|e| e.to_string())?;
        let id = store.upsert(profile.clone());
        self.core.save_store(&store).map_err(|e| e.to_string())?;
        Ok(ProfileView {
            active: store.active.as_deref() == Some(id.as_str()),
            id,
            tag: profile.tag.clone(),
            protocol: protocol_name(profile.protocol).to_string(),
            endpoint: format!("{}:{}", profile.address, profile.port),
            remark: profile.remark.clone(),
        })
    }

    async fn remove_profile(&self, id: &str) -> Result<(), String> {
        let mut store = self.core.load_store().map_err(|e| e.to_string())?;
        let target = find_profile(&store, id)
            .ok_or_else(|| format!("no profile matches {id:?}"))?
            .id
            .clone();
        if store.remove(&target) {
            self.core.save_store(&store).map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err(format!("no profile matches {id:?}"))
        }
    }

    async fn set_active_profile(&self, id: &str) -> Result<(), String> {
        let mut store = self.core.load_store().map_err(|e| e.to_string())?;
        let target = find_profile(&store, id)
            .ok_or_else(|| format!("no profile matches {id:?}"))?
            .id
            .clone();
        if store.set_active(Some(&target)) {
            self.core.save_store(&store).map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err(format!("no profile matches {id:?}"))
        }
    }

    async fn test_latency(&self, id: &str) -> Result<u32, String> {
        let store = self.core.load_store().map_err(|e| e.to_string())?;
        let profile = find_profile(&store, id)
            .ok_or_else(|| format!("no profile matches {id:?}"))?
            .clone();
        let settings = SettingsFile::load(&self.core.paths.settings_file());
        let bonded = resolve_bonded(
            &settings.settings.bonded_adapters,
            &mt_platform::enumerate_adapters(),
        );
        let mut links = self
            .core
            .factory
            .build(&profile, &bonded)
            .await
            .map_err(|e| e.to_string())?;
        let Some(link) = links.pop() else {
            return Err("link factory produced no links".to_string());
        };
        let target = parse_probe_target(&settings.probe_target)
            .unwrap_or_else(|| parse_probe_target(DEFAULT_PROBE_TARGET).expect("constant parses"));
        let started = Instant::now();
        let probe = tokio::time::timeout(CONNECT_PROBE_BUDGET, link.probe(&target)).await;
        match probe {
            Ok(Ok(_)) => Ok(started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32),
            Ok(Err(e)) => Err(format!("latency probe failed: {e}")),
            Err(_) => Err("latency probe timed out".to_string()),
        }
    }

    async fn routing_rules(&self) -> Vec<RuleView> {
        SettingsFile::load(&self.core.paths.settings_file())
            .rules
            .iter()
            .map(rule_to_view)
            .collect()
    }

    async fn set_routing_rules(&self, rules: Vec<RuleView>) -> Result<(), String> {
        let mut converted = Vec::with_capacity(rules.len());
        for v in &rules {
            converted.push(rule_from_view(v)?);
        }
        let mut file = SettingsFile::load(&self.core.paths.settings_file());
        file.rules = converted;
        file.save(&self.core.paths.settings_file())
            .map_err(|e| e.to_string())
    }

    async fn settings(&self) -> SettingsView {
        SettingsFile::load(&self.core.paths.settings_file()).settings
    }

    async fn set_settings(&self, settings: SettingsView) -> Result<(), String> {
        let mut file = SettingsFile::load(&self.core.paths.settings_file());
        let autostart_changed = file.settings.autostart != settings.autostart;
        // Validate the S9 advanced knobs BEFORE persisting (fail loud, no
        // partial writes). Port 0 = "ephemeral" is legal on BOTH listeners
        // (the documented test/embedder mode — system proxy is simply
        // skipped then); two equal NONZERO ports would collide.
        if settings.socks_port != 0 && settings.socks_port == settings.http_port {
            return Err("SOCKS and HTTP ports must differ".to_string());
        }
        if !matches!(
            settings.log_level.as_str(),
            "error" | "warning" | "info" | "debug"
        ) {
            return Err(format!("unknown log level {:?}", settings.log_level));
        }
        if !matches!(settings.bond_mode.as_str(), "per-connection" | "stream") {
            return Err(format!("unknown bond mode {:?}", settings.bond_mode));
        }
        if !is_known_merge_policy(&settings.merge_policy) {
            return Err(format!(
                "unknown merge policy {:?} — pick one from the Settings screen",
                settings.merge_policy
            ));
        }
        if parse_http_url(&settings.exit_probe_url).is_none() {
            return Err(format!(
                "exit probe URL must be plain http:// ({:?})",
                settings.exit_probe_url
            ));
        }
        if settings.probe_target.split(':').count() != 2 {
            return Err(format!(
                "probe target must be host:port ({:?})",
                settings.probe_target
            ));
        }
        // These change the LIVE data plane — remember for the restart below
        // (S11: the merge policy too — it drives per-connection spreading
        // AND the stream-bond carrier scheduler now).
        let dataplane_changed = file.settings.socks_port != settings.socks_port
            || file.settings.http_port != settings.http_port
            || file.settings.allow_lan != settings.allow_lan
            || file.settings.bond_mode != settings.bond_mode
            || file.settings.merge_port != settings.merge_port
            || file.settings.merge_policy != settings.merge_policy;
        // Module note 15: `bonded_adapters` belongs to the Adapters screen
        // (`set_bonded_adapters`); a possibly-stale GUI settings copy must
        // never clobber a fresh selection — persist the server-side value.
        let mut settings = settings;
        settings.bonded_adapters = file.settings.bonded_adapters.clone();
        file.settings = settings.clone();
        file.sync_legacy();
        file.save(&self.core.paths.settings_file())
            .map_err(|e| e.to_string())?;
        if dataplane_changed && self.core.get_state() == ServiceState::Connected {
            self.core.emit(ServiceEvent::LogLine(
                "data-plane settings changed — reconnecting".into(),
            ));
            self.disconnect().await;
            self.connect().await;
        }
        if autostart_changed {
            let exe = std::env::current_exe()
                .map_err(|e| Error(format!("resolve executable: {e}")))
                .map_err(|e| e.to_string())?;
            if settings.autostart {
                mt_platform::autostart_enable(&exe).map_err(|e| e.to_string())?;
            } else {
                mt_platform::autostart_disable().map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    async fn adapters(&self) -> Vec<AdapterView> {
        let selected = SettingsFile::load(&self.core.paths.settings_file())
            .settings
            .bonded_adapters;
        mt_platform::enumerate_adapters()
            .into_iter()
            .map(|a| AdapterView {
                selected: selected.contains(&a.id),
                id: a.id,
                name: a.name,
                description: a.description,
                ipv4: a.ipv4.iter().map(|ip| ip.to_string()).collect(),
                up: a.up,
            })
            .collect()
    }

    async fn set_bonded_adapters(&self, ids: Vec<String>) -> Result<(), String> {
        // Validate against the CURRENT enumeration: a typo'd or vanished
        // id is a hard error naming the id (the GUI refreshes its list from
        // the same source, so this only fires on races).
        let known = mt_platform::enumerate_adapters();
        for id in &ids {
            if !known.iter().any(|a| &a.id == id) {
                return Err(format!(
                    "unknown adapter {id:?} — refresh the adapter list and pick again"
                ));
            }
        }
        // Dedupe preserving first-seen order.
        let mut deduped: Vec<String> = Vec::with_capacity(ids.len());
        for id in ids {
            if !deduped.contains(&id) {
                deduped.push(id);
            }
        }
        let mut file = SettingsFile::load(&self.core.paths.settings_file());
        file.settings.bonded_adapters = deduped;
        file.save(&self.core.paths.settings_file())
            .map_err(|e| e.to_string())?;
        // A live session rides the OLD link set — restart so the new
        // selection takes effect immediately (module note 15). Connecting
        // sessions finish first (their links are already fixed); the next
        // manual connect picks the selection up.
        match self.core.get_state() {
            ServiceState::Connected => {
                self.core.emit(ServiceEvent::LogLine(
                    "bonded adapters changed — reconnecting".into(),
                ));
                self.disconnect().await;
                self.connect().await;
            }
            ServiceState::Connecting => {
                self.core.emit(ServiceEvent::LogLine(
                    "bonded adapters changed — applies after the pending connect".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    async fn exit_probe(&self) -> Result<ExitInfo, String> {
        // 1. Must ride the LIVE session (the whole point is the tunnel's
        //    egress as the internet sees it, D18).
        let socks_addr = {
            let guard = lock(&self.core.session);
            let Some(s) = guard.as_ref() else {
                return Err("not connected".to_string());
            };
            s.server
                .socks_local_addr()
                .ok_or_else(|| "no local listener".to_string())?
        };
        let store = self.core.load_store_lossy();
        let Some(profile) = store
            .active
            .as_deref()
            .and_then(|id| store.profiles.iter().find(|p| p.id == id))
            .cloned()
        else {
            return Err("no active profile".to_string());
        };
        let settings = SettingsFile::load(&self.core.paths.settings_file());
        let url = settings.settings.exit_probe_url;
        let (host, port, path) =
            parse_http_url(&url).ok_or_else(|| format!("invalid exit probe URL {url:?}"))?;
        // 2. SOCKS CONNECT through the live local listener, then a plain
        //    HTTP/1.1 GET (no TLS stack — zero-dep rule; the client→server
        //    leg is REALITY-encrypted and the payload is only an IP echo).
        let target = TargetAddr {
            host: if let Ok(ip) = host.parse() {
                Host::Ip(ip)
            } else {
                Host::Domain(host.clone())
            },
            port,
        };
        let probe = async {
            let outbound = mt_net::Socks5Outbound::new(socks_addr);
            let mut stream = outbound.connect(&target).await?;
            let req = format!(
                "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: MultiTunnel/0.5\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
            );
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            stream.write_all(req.as_bytes()).await?;
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await?;
            Ok::<Vec<u8>, anyhow::Error>(buf)
        };
        let body = tokio::time::timeout(EXIT_PROBE_BUDGET, probe)
            .await
            .map_err(|_| "exit probe timed out".to_string())?
            .map_err(|e| format!("exit probe failed: {e}"))?;
        // 3. Body = everything after the blank line.
        let text = String::from_utf8_lossy(&body);
        let payload = text
            .split_once("\r\n\r\n")
            .map(|(_, b)| b)
            .unwrap_or(text.as_ref());
        let json: serde_json::Value = serde_json::from_str(payload.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("exit probe response not JSON: {e}"))?;
        let field = |k: &str| {
            json.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let exit_ip = field("query");
        if exit_ip.is_empty() {
            return Err("exit probe response carried no IP".to_string());
        }
        // 4. Direct-exit verdict: the exit IP should BE the profile server.
        //    A domain profile is resolved locally for the comparison.
        let server_endpoint = format!("{}:{}", profile.address, profile.port);
        let direct = exit_ip == profile.address
            || std::net::ToSocketAddrs::to_socket_addrs(&server_endpoint)
                .map(|addrs| addrs.map(|a| a.ip().to_string()).any(|ip| ip == exit_ip))
                .unwrap_or(false);
        Ok(ExitInfo {
            exit_ip,
            country: field("country"),
            city: field("city"),
            isp: field("isp"),
            asn: field("as"),
            server_endpoint,
            direct_exit: direct,
        })
    }

    fn events(&self) -> broadcast::Receiver<ServiceEvent> {
        self.core.events.subscribe()
    }
}

/// Find a profile by id first, then by tag (decision #11).
fn find_profile<'a>(store: &'a ProfileStore, id_or_tag: &str) -> Option<&'a Profile> {
    store
        .profiles
        .iter()
        .find(|p| p.id == id_or_tag)
        .or_else(|| store.profiles.iter().find(|p| p.tag == id_or_tag))
}

// ---------------------------------------------------------------------------
// Tests: the state machine against the REAL crates with a FAKE link. No
// engine binary is ever spawned (the LinkFactory seam keeps ManagedEngine out
// of this surface); all networking stays on 127.0.0.1 ephemeral ports.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mt_net::{Socks5Outbound, TargetAddr};
    use std::net::SocketAddr;

    /// Synthetic REALITY vector from harness/CONTRACTS-S6.md (D9-safe).
    const VLESS_REALITY: &str = "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=reality&sni=tunnel.example.org&fp=chrome&pbk=0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde&sid=0123abcd&type=tcp&flow=xtls-rprx-vision#synthetic-reality";

    fn scratch(label: &str) -> ServicePaths {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base =
            std::env::temp_dir().join(format!("mt-w3-{label}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&base).expect("scratch dir");
        ServicePaths {
            config_dir: base.join("config"),
            state_dir: base.join("state"),
            log_dir: base.join("logs"),
        }
    }

    /// A loopback echo server: every byte received is sent back.
    async fn spawn_echo() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind echo");
        let addr = listener.local_addr().expect("echo addr");
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.into_split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });
        addr
    }

    /// Fake link: always dials the loopback echo server (regardless of the
    /// requested target — this is a data-plane smoke, not routing fidelity).
    struct FakeLink {
        name: &'static str,
        echo: SocketAddr,
    }

    #[async_trait::async_trait]
    impl Outbound for FakeLink {
        fn name(&self) -> &str {
            self.name
        }
        async fn connect(&self, _target: &TargetAddr) -> anyhow::Result<mt_net::ProxyStream> {
            let s = tokio::net::TcpStream::connect(self.echo).await?;
            Ok(Box::new(s))
        }
        async fn probe(&self, _target: &TargetAddr) -> anyhow::Result<Duration> {
            let started = Instant::now();
            tokio::net::TcpStream::connect(self.echo).await?;
            Ok(started.elapsed())
        }
    }

    struct FakeFactory {
        echo: SocketAddr,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl LinkFactory for FakeFactory {
        async fn build(
            &self,
            _profile: &Profile,
            bonded: &[BondedAdapter],
        ) -> anyhow::Result<Vec<Arc<dyn Outbound>>> {
            if self.fail {
                anyhow::bail!("no engine available (synthetic factory failure)");
            }
            // Bonding fidelity: one fake link per bonded adapter, named
            // like the real factory would (empty selection → one link).
            if bonded.is_empty() {
                return Ok(vec![Arc::new(FakeLink {
                    name: "fake-0",
                    echo: self.echo,
                })]);
            }
            Ok(bonded
                .iter()
                .map(|a| {
                    Arc::new(FakeLinkNamed {
                        name: format!("fake [{}]", a.name),
                        echo: self.echo,
                    }) as Arc<dyn Outbound>
                })
                .collect())
        }
    }

    /// FakeLink variant with an owned name (bonded links are named after
    /// adapters, which are runtime strings).
    struct FakeLinkNamed {
        name: String,
        echo: SocketAddr,
    }

    #[async_trait::async_trait]
    impl Outbound for FakeLinkNamed {
        fn name(&self) -> &str {
            &self.name
        }
        async fn connect(&self, _target: &TargetAddr) -> anyhow::Result<mt_net::ProxyStream> {
            let s = tokio::net::TcpStream::connect(self.echo).await?;
            Ok(Box::new(s))
        }
        async fn probe(&self, _target: &TargetAddr) -> anyhow::Result<Duration> {
            let started = Instant::now();
            tokio::net::TcpStream::connect(self.echo).await?;
            Ok(started.elapsed())
        }
    }

    /// settings.json with ephemeral local ports and a loopback probe target.
    /// `settings_version` is CURRENT so the fixture's explicit "failover"
    /// is respected verbatim (the D20 migration only touches version-0
    /// files — exactly the v0.5.0 stock default case).
    fn write_settings(paths: &ServicePaths, echo_port: u16, system_proxy: bool) {
        let file = SettingsFile {
            settings: SettingsView {
                theme_seed: 0x6750A4,
                dark_mode: false,
                // v0.5-era file shape: NO theme_mode key (the S18 legacy
                // migration tests build on files like this one).
                theme_mode: String::new(),
                system_proxy,
                merge_policy: "failover".to_string(),
                autostart: false,
                bonded_adapters: Vec::new(),
                bond_mode: "per-connection".to_string(),
                merge_port: 7443,
                socks_port: DEFAULT_SOCKS_PORT,
                http_port: DEFAULT_HTTP_PORT,
                allow_lan: false,
                log_level: "warning".to_string(),
                probe_target: DEFAULT_PROBE_TARGET.to_string(),
                exit_probe_url: "http://ip-api.com/json".to_string(),
                close_to_tray: false,
                always_on_top: false,
            },
            rules: Vec::new(),
            socks_port: 0,
            http_port: 0,
            probe_target: format!("127.0.0.1:{echo_port}"),
            settings_version: SETTINGS_VERSION,
        };
        std::fs::create_dir_all(&paths.config_dir).expect("config dir");
        file.save(&paths.settings_file()).expect("save settings");
    }

    /// A fake exit-echo HTTP server (D18): answers any GET with a fixed
    /// JSON identity document and closes.
    async fn spawn_exit_echo(query_ip: &'static str) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind exit echo");
        let addr = listener.local_addr().expect("exit addr");
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                    let mut buf = vec![0u8; 2048];
                    let _ = stream.read(&mut buf).await; // drain the request head
                    let body = format!(
                        "{{\"query\":\"{query_ip}\",\"country\":\"Testland\",\"city\":\"Mock City\",\"isp\":\"Example Telecom\",\"as\":\"AS64512 Example Telecom\"}}"
                    );
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });
        addr
    }

    /// A local merge daemon for stream-mode tests (real mt-bond daemon on
    /// loopback; the guard keeps it alive for the test body).
    async fn spawn_merge_daemon() -> (SocketAddr, tokio::sync::watch::Sender<bool>) {
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind daemon probe");
        let addr = probe.local_addr().expect("daemon addr");
        drop(probe);
        let (tx, rx) = tokio::sync::watch::channel(false);
        tokio::spawn(mt_bond::daemon::serve(addr, rx));
        for _ in 0..100 {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                return (addr, tx);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("merge daemon listener never came up");
    }

    #[test]
    fn parse_http_url_accepts_and_rejects() {
        assert_eq!(
            parse_http_url("http://ip-api.com/json"),
            Some(("ip-api.com".to_string(), 80, "/json".to_string()))
        );
        assert_eq!(
            parse_http_url("http://example.org:8080"),
            Some(("example.org".to_string(), 8080, "/".to_string()))
        );
        assert_eq!(
            parse_http_url("http://example.org:8080/a/b?x=1"),
            Some(("example.org".to_string(), 8080, "/a/b?x=1".to_string()))
        );
        assert_eq!(
            parse_http_url("https://ip-api.com/json"),
            None,
            "TLS is out of scope"
        );
        assert_eq!(parse_http_url("ip-api.com/json"), None, "scheme required");
        assert_eq!(parse_http_url("http://"), None);
        assert_eq!(parse_http_url("http://host:notaport/"), None);
    }

    #[tokio::test]
    async fn exit_probe_reports_identity_and_verdict() {
        // The exit-echo server doubles as the fake link's dial target, so
        // the probe rides the whole path: local SOCKS → link → exit.
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("exitprobe");
        write_settings(&paths, exit.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.exit_probe_url = format!("http://127.0.0.1:{}/json", exit.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        // Not connected → refused.
        assert_eq!(svc.exit_probe().await.unwrap_err(), "not connected");

        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);

        // The synthetic profile points at 203.0.113.10 — the exit
        // (127.0.0.1) differs → extra-hop verdict.
        let info = svc.exit_probe().await.expect("probe");
        assert_eq!(info.exit_ip, "127.0.0.1");
        assert_eq!(info.country, "Testland");
        assert_eq!(info.city, "Mock City");
        assert_eq!(info.isp, "Example Telecom");
        assert!(info.asn.contains("AS64512"));
        assert_eq!(info.server_endpoint, "203.0.113.10:443");
        assert!(
            !info.direct_exit,
            "203.0.113.10 profile must NOT match a 127.0.0.1 exit"
        );
        svc.disconnect().await;
    }

    #[tokio::test]
    async fn exit_probe_direct_exit_matches_loopback_profile() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("exitdirect");
        write_settings(&paths, exit.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.exit_probe_url = format!("http://127.0.0.1:{}/json", exit.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        // A profile whose server IS 127.0.0.1 → direct verdict.
        let local_link = VLESS_REALITY.replace("@203.0.113.10:", "@127.0.0.1:");
        let view = svc.import_link(&local_link).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");
        svc.connect().await;
        let info = svc.exit_probe().await.expect("probe");
        assert!(
            info.direct_exit,
            "the loopback profile matches the loopback exit"
        );
        assert_eq!(info.server_endpoint, "127.0.0.1:443");
        svc.disconnect().await;
    }

    #[tokio::test]
    async fn stream_bond_mode_connects_and_carries_traffic() {
        let (daemon, _guard) = spawn_merge_daemon().await;
        let echo = spawn_echo().await;
        let paths = scratch("streambond");
        write_settings(&paths, echo.port(), false);
        // Stream mode: the fake links dial the DAEMON (in production the
        // engines' VLESS connections land there through the tunnel).
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.bond_mode = "stream".to_string();
        file.settings.merge_port = daemon.port();
        file.save(&paths.settings_file()).expect("save settings");

        // NOTE: synthetic bonded-adapter ids are SKIPPED by resolve_bonded
        // (they are not real adapters on this machine), so the factory
        // builds exactly ONE unbound link — the bond therefore rides ONE
        // carrier here. Multi-carrier coverage lives in mt-bond's own
        // integration tests (two fake links).

        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: daemon,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");
        svc.connect().await;
        assert_eq!(
            svc.status().await.state,
            ServiceState::Connected,
            "stream mode must connect when the daemon is reachable"
        );

        // Status rows are BOND CARRIERS now (D19): the fake factory's one
        // link became one carrier named after it.
        let status = svc.status().await;
        assert!(
            status.links.len() == 1,
            "one carrier row expected, got {:?}",
            status.links
        );
        assert_eq!(status.links[0].name, "fake-0");
        assert!(status.links[0].ok, "carrier healthy");
        // Traffic through the bond: SOCKS CONNECT to the echo target.
        let (socks, _) = svc.session_addrs();
        let socks = socks.expect("socks bound");
        let outbound = mt_net::Socks5Outbound::new(socks);
        let target = TargetAddr {
            host: Host::Ip("127.0.0.1".parse().unwrap()),
            port: echo.port(),
        };
        let mut stream = outbound.connect(&target).await.expect("socks connect");
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        stream.write_all(b"bonded hello").await.expect("write");
        stream.shutdown().await.expect("shutdown");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.expect("read");
        assert_eq!(buf, b"bonded hello", "echo must survive the bond");

        svc.disconnect().await;
        assert_eq!(svc.status().await.state, ServiceState::Disconnected);
    }

    #[tokio::test]
    async fn stream_bond_falls_back_when_daemon_unreachable() {
        // merge port with nothing listening: connect must still succeed
        // (per-connection fallback, D17 §5) and log the fallback.
        let echo = spawn_echo().await;
        let paths = scratch("streamfallback");
        write_settings(&paths, echo.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.bond_mode = "stream".to_string();
        file.settings.merge_port = 9; // discard port — nothing listens
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");
        svc.connect().await;
        assert_eq!(
            svc.status().await.state,
            ServiceState::Connected,
            "fallback must never black-hole the tunnel"
        );
        svc.disconnect().await;
    }

    #[tokio::test]
    async fn connect_reaches_connected_and_serves_socks() {
        let echo = spawn_echo().await;
        let paths = scratch("connect");
        write_settings(&paths, echo.port(), false);
        let svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));

        // Seed the store through the service itself (GUI-shaped path).
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        assert_eq!(svc.status().await.state, ServiceState::Disconnected);
        svc.connect().await;

        let snap = svc.status().await;
        assert_eq!(snap.state, ServiceState::Connected, "probe must succeed");
        assert_eq!(snap.restarts, 1);
        assert!(snap.up_since.is_some(), "up_since set when connected");
        assert_eq!(snap.links.len(), 1, "the fake link is reported");
        assert!(snap.links[0].ok);

        // The data plane is real: SOCKS5 CONNECT through the bound listener
        // lands on the echo server through the fake link.
        let (socks, http) = svc.session_addrs();
        let socks = socks.expect("socks listener bound");
        assert!(http.is_some(), "http listener bound");
        let client = Socks5Outbound::new(socks);
        let target = TargetAddr {
            host: Host::Domain("synthetic.example".to_string()),
            port: echo.port(),
        };
        let mut stream = client.connect(&target).await.expect("socks connect");
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        stream.write_all(b"ping").await.expect("write");
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.expect("echo read");
        assert_eq!(&buf, b"ping");
        drop(stream);

        // Disconnect: server stops, addrs clear, state reports Disconnected.
        svc.disconnect().await;
        let snap = svc.status().await;
        assert_eq!(snap.state, ServiceState::Disconnected);
        assert_eq!(svc.session_addrs(), (None, None));
    }

    #[tokio::test]
    async fn factory_failure_reports_failed_with_no_leftover_server() {
        let paths = scratch("fail");
        write_settings(&paths, 9, false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: "127.0.0.1:9".parse().expect("addr"),
                fail: true,
            }),
        );
        let mut events = svc.events();

        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Failed);
        assert!(
            svc.session_addrs().0.is_none(),
            "failed connect must not leave a server running"
        );

        let mut saw_failed = false;
        while let Ok(ev) = events.try_recv() {
            if matches!(ev, ServiceEvent::StateChanged(ServiceState::Failed)) {
                saw_failed = true;
            }
        }
        assert!(saw_failed, "StateChanged(Failed) must be broadcast");
    }

    #[tokio::test]
    async fn settings_round_trip_persists_across_instances() {
        let echo = spawn_echo().await;
        let paths = scratch("settings");
        let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));

        let view = SettingsView {
            theme_seed: 0xEA4335,
            dark_mode: true,
            theme_mode: "dark".to_string(),
            system_proxy: false, // never true in tests: autostart/sysproxy touch the host
            merge_policy: "lowestlatency".to_string(),
            autostart: false,
            bonded_adapters: Vec::new(),
            bond_mode: "per-connection".to_string(),
            merge_port: 7443,
            socks_port: 0, // ephemeral — tests never bind fixed ports
            http_port: 0,
            allow_lan: false,
            log_level: "warning".to_string(),
            probe_target: "connectivitycheck.gstatic.com:80".to_string(),
            exit_probe_url: "http://ip-api.com/json".to_string(),
            close_to_tray: false,
            always_on_top: false,
        };
        svc.set_settings(view.clone()).await.expect("save settings");
        assert_eq!(svc.settings().await, view);

        let rules = vec![
            RuleView {
                kind: "domainSuffix".to_string(),
                value: "example.org".to_string(),
                action: "direct".to_string(),
            },
            RuleView {
                kind: "any".to_string(),
                value: String::new(),
                action: "proxy".to_string(),
            },
        ];
        svc.set_routing_rules(rules.clone())
            .await
            .expect("save rules");
        assert_eq!(svc.routing_rules().await, rules);

        // A second service over the same paths reads the same on-disk state.
        let svc2 = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: "127.0.0.1:9".parse().expect("addr"),
                fail: false,
            }),
        );
        assert_eq!(svc2.settings().await, view);
        assert_eq!(svc2.routing_rules().await, rules);
    }

    #[tokio::test]
    async fn system_proxy_disabled_is_a_no_op_path() {
        let echo = spawn_echo().await;
        let paths = scratch("sysproxy");
        write_settings(&paths, echo.port(), false);
        let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));

        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        svc.disconnect().await;

        // On Windows the sysproxy snapshot file is only created by
        // sysproxy::set; with system_proxy=false it must never appear.
        // (On non-Windows set/restore are no-ops entirely.)
        assert!(
            !paths.state_dir.join("sysproxy.json").exists(),
            "system_proxy=false must not touch the system proxy"
        );
    }

    #[test]
    fn endpoint_from_profile_maps_reality_tcp() {
        let profile = parse_share_link(VLESS_REALITY).expect("parse");
        let ep = endpoint_from_profile(&profile).expect("endpoint");
        assert_eq!(ep.address, "203.0.113.10");
        assert_eq!(ep.port, 443);
        // Synthetic test vector (RFC 5737 + dummy UUID) — safe to assert (D9).
        assert_eq!(ep.uuid, "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
        assert_eq!(ep.sni, "tunnel.example.org");
        assert_eq!(ep.fingerprint, "chrome");
        assert_eq!(ep.public_key, "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde");
        assert_eq!(ep.short_id, "0123abcd");
        assert_eq!(ep.spider_x.as_deref(), Some("/"));
    }

    #[test]
    fn endpoint_from_profile_rejects_unwired_shapes_without_credential_echo() {
        // ws transport (reality × ws is parseable but not wired for v0.3)
        let ws = "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=reality&sni=tunnel.example.org&fp=chrome&pbk=0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde&sid=0123abcd&type=ws&path=%2Fws#ws-one";
        let profile = parse_share_link(ws).expect("parse ws");
        let err = endpoint_from_profile(&profile).expect_err("ws must be rejected");
        assert!(err.to_string().contains("tcp"), "names the limitation");
        assert!(
            !err.to_string().contains("eeeeeeeeeeee"),
            "errors never echo the uuid"
        );

        // non-vless protocol
        let ss = "ss://YWVzLTI1Ni1nY206c3ludGhldGlj@203.0.113.10:8388#synthetic-ss";
        let profile = parse_share_link(ss).expect("parse ss");
        let err = endpoint_from_profile(&profile).expect_err("ss must be rejected");
        assert!(err.to_string().contains("VLESS"));

        // plain TLS (engine is REALITY-only)
        let tls = "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=tls&sni=tunnel.example.org&type=tcp#tls-one";
        let profile = parse_share_link(tls).expect("parse tls");
        let err = endpoint_from_profile(&profile).expect_err("tls must be rejected");
        assert!(err.to_string().contains("REALITY"));
    }

    #[test]
    fn fmt_utc_formats_the_epoch_and_a_known_date() {
        assert_eq!(fmt_utc(UNIX_EPOCH), "1970-01-01 00:00:00 UTC");
        // 2024-02-29 12:34:56 UTC (leap day sanity).
        let secs: i64 = 1709210096;
        let t = UNIX_EPOCH + Duration::from_secs(secs as u64);
        assert_eq!(fmt_utc(t), "2024-02-29 12:34:56 UTC");
    }

    // --- S7: adapter bonding -------------------------------------------------

    fn net_adapter(id: &str, name: &str, ips: &[&str], up: bool) -> NetAdapter {
        NetAdapter {
            id: id.to_string(),
            name: name.to_string(),
            description: format!("desc of {name}"),
            ipv4: ips.iter().map(|s| s.parse().expect("test ip")).collect(),
            up,
        }
    }

    #[test]
    fn resolve_bonded_keeps_order_skips_unknown_and_ipless() {
        let adapters = vec![
            net_adapter("guid-wifi", "Wi-Fi", &["192.168.1.42", "169.254.1.1"], true),
            net_adapter("guid-eth", "Ethernet", &["10.0.0.5"], true),
            net_adapter("guid-dead", "Dead NIC", &[], false),
        ];
        // Settings order is preserved; unknown + IPv4-less skipped.
        let ids = ["guid-eth", "nope", "guid-dead", "guid-wifi"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let bonded = resolve_bonded(&ids, &adapters);
        assert_eq!(
            bonded,
            vec![
                BondedAdapter {
                    id: "guid-eth".into(),
                    name: "Ethernet".into(),
                    bind_ip: "10.0.0.5".into(),
                },
                BondedAdapter {
                    id: "guid-wifi".into(),
                    name: "Wi-Fi".into(),
                    bind_ip: "192.168.1.42".into(),
                },
            ]
        );
        // Duplicates collapse; empty input → empty output.
        let dup = ["guid-wifi", "guid-wifi"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert_eq!(resolve_bonded(&dup, &adapters).len(), 1);
        assert!(resolve_bonded(&[], &adapters).is_empty());
    }

    /// D20: a v0.5.0 settings.json (no `settings_version`, the then-stock
    /// "failover") migrates to round-robin on load and persists version 1;
    /// an EXPLICIT failover on a current file is respected verbatim.
    #[test]
    fn settings_migration_failover_to_round_robin() {
        let paths = scratch("migrate");
        std::fs::create_dir_all(&paths.config_dir).expect("config dir");
        let path = paths.settings_file();

        // (a) v0.5.0-shaped file: no settings_version, stock failover.
        // (All v0.5.0-required fields present, as a real v0.5.0 write.)
        // D23: the migration CHAINS — failover (v0.5.0 stock) →
        // round-robin (v0.5.1 stock) → balanced (v0.7.0 stock).
        let v050_json = concat!(
            r#"{"settings":{"theme_seed":4273518,"dark_mode":false,"system_proxy":false,"#,
            r#""merge_policy":"failover","autostart":false,"bonded_adapters":[],"#,
            r#""bond_mode":"per-connection","merge_port":7443},"rules":[]}"#
        );
        std::fs::write(&path, v050_json).expect("write v0.5.0 file");
        let loaded = SettingsFile::load(&path);
        assert_eq!(
            loaded.settings.merge_policy, "balanced",
            "stock failover migrates through the chain to balanced"
        );
        assert_eq!(loaded.settings_version, SETTINGS_VERSION);
        // The migration persists (a second load is a no-op with the same
        // value — explicit failover from NOW ON is respected).
        let persisted = std::fs::read_to_string(&path).expect("migration persisted");
        assert!(persisted.contains("\"balanced\""));
        assert!(persisted.contains(&format!("\"settings_version\": {SETTINGS_VERSION}")));
        let reloaded = SettingsFile::load(&path);
        assert_eq!(reloaded.settings.merge_policy, "balanced");

        // (b) v0.6.1-shaped file (version 1) with the then-stock
        // round-robin: migrates ONCE to balanced.
        let mut file = SettingsFile::default();
        file.settings.merge_policy = "round-robin".to_string();
        file.settings_version = 1;
        file.save(&path).expect("save v0.6.1 stock");
        let loaded = SettingsFile::load(&path);
        assert_eq!(
            loaded.settings.merge_policy, "balanced",
            "v0.6.x stock round-robin migrates to balanced"
        );

        // (c) CURRENT file with an explicit round-robin: never touched.
        let mut file = SettingsFile::default();
        file.settings.merge_policy = "round-robin".to_string();
        file.settings_version = SETTINGS_VERSION;
        file.save(&path).expect("save explicit round-robin");
        let loaded = SettingsFile::load(&path);
        assert_eq!(
            loaded.settings.merge_policy, "round-robin",
            "explicit round-robin on a current file is respected"
        );

        // (d) CURRENT file with an explicit failover: never touched.
        let mut file = SettingsFile::default();
        file.settings.merge_policy = "failover".to_string();
        file.settings_version = SETTINGS_VERSION;
        file.save(&path).expect("save explicit failover");
        let loaded = SettingsFile::load(&path);
        assert_eq!(
            loaded.settings.merge_policy, "failover",
            "explicit failover on a current file is respected"
        );

        // (e) Fresh default (no file at all) is balanced.
        std::fs::remove_file(&path).expect("remove settings");
        let fresh = SettingsFile::load(&path);
        assert_eq!(fresh.settings.merge_policy, "balanced");
        assert_eq!(fresh.settings_version, SETTINGS_VERSION);
    }

    /// D20: the unknown-policy fallback is round-robin (the new stock).
    #[test]
    fn unknown_merge_policy_falls_back_to_round_robin() {
        assert!(matches!(
            merge_policy_from_str("garbage"),
            MergePolicy::RoundRobin
        ));
        assert!(matches!(
            merge_policy_from_str("failover"),
            MergePolicy::Failover
        ));
        assert!(matches!(
            merge_policy_from_str("roundrobin"),
            MergePolicy::RoundRobin
        ));
        assert!(matches!(
            merge_policy_from_str("lowestlatency"),
            MergePolicy::LowestLatency
        ));
    }

    /// S10 regression: the GUI's pick-list sends the HYPHENATED names —
    /// before this fix "lowest-latency" (and "round-robin") fell into the
    /// unknown-value fallback, so the lowest-latency policy silently
    /// behaved as round-robin. S11: the two new policies parse too.
    #[test]
    fn hyphenated_merge_policy_names_parse() {
        assert!(matches!(
            merge_policy_from_str("round-robin"),
            MergePolicy::RoundRobin
        ));
        assert!(matches!(
            merge_policy_from_str("lowest-latency"),
            MergePolicy::LowestLatency
        ));
        assert!(matches!(
            merge_policy_from_str("balanced"),
            MergePolicy::Balanced
        ));
        assert!(matches!(
            merge_policy_from_str("priority"),
            MergePolicy::Priority
        ));
        assert!(is_known_merge_policy("balanced"));
        assert!(is_known_merge_policy("priority"));
        assert!(!is_known_merge_policy("fastest"));
    }

    #[test]
    fn engine_specs_for_binds_one_engine_per_adapter() {
        let profile = parse_share_link(VLESS_REALITY).expect("parse");
        // No selection → exactly one UNBOUND spec with the plain tag.
        let unbound = engine_specs_for(&profile, &[]);
        assert_eq!(unbound.len(), 1);
        assert_eq!(unbound[0].tag, profile.tag);
        assert_eq!(unbound[0].send_through, None);

        // Two adapters → two specs, tags embed the adapter name,
        // sendThrough pins each adapter's IPv4.
        let bonded = vec![
            BondedAdapter {
                id: "guid-wifi".into(),
                name: "Wi-Fi".into(),
                bind_ip: "192.168.1.42".into(),
            },
            BondedAdapter {
                id: "guid-eth".into(),
                name: "Ethernet".into(),
                bind_ip: "10.0.0.5".into(),
            },
        ];
        let specs = engine_specs_for(&profile, &bonded);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].tag, "synthetic-reality [Wi-Fi]");
        assert_eq!(specs[0].send_through.as_deref(), Some("192.168.1.42"));
        assert_eq!(specs[1].tag, "synthetic-reality [Ethernet]");
        assert_eq!(specs[1].send_through.as_deref(), Some("10.0.0.5"));

        // Duplicate friendly names get distinct tags (config/log filenames
        // derive from the tag and must never collide).
        let twins = vec![
            BondedAdapter {
                id: "a1".into(),
                name: "Ethernet".into(),
                bind_ip: "10.0.0.5".into(),
            },
            BondedAdapter {
                id: "a2".into(),
                name: "Ethernet".into(),
                bind_ip: "10.0.0.6".into(),
            },
        ];
        let specs = engine_specs_for(&profile, &twins);
        assert_ne!(specs[0].tag, specs[1].tag, "twin names must diverge");
    }

    #[tokio::test]
    async fn set_bonded_adapters_validates_persists_and_reconnects() {
        let echo = spawn_echo().await;
        let paths = scratch("bonded");
        write_settings(&paths, echo.port(), false);
        let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("active");

        // Unknown ids are rejected with a message naming the id.
        let err = svc
            .set_bonded_adapters(vec!["definitely-not-an-adapter-id".into()])
            .await
            .expect_err("unknown id must be rejected");
        assert!(err.contains("definitely-not-an-adapter-id"));

        // Machine-dependent part: use a REAL adapter id when the host has
        // one (Linux CI: eth0; Windows runners: several). None at all →
        // only the validation path above is exercisable; skip the rest.
        let adapters = svc.adapters().await;
        let Some(first) = adapters.iter().find(|a| a.up || !a.ipv4.is_empty()) else {
            return;
        };
        let pick = first.id.clone();

        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        let restarts_before = svc.status().await.restarts;

        // Applying a selection while connected restarts the session
        // (FakeFactory builds one link per bonded adapter).
        svc.set_bonded_adapters(vec![pick.clone()])
            .await
            .expect("valid selection");
        let snap = svc.status().await;
        assert_eq!(snap.state, ServiceState::Connected, "reconnected");
        assert!(snap.restarts > restarts_before, "a fresh connect ran");

        // The selection is persisted and reflected by adapters().
        let adapters = svc.adapters().await;
        let selected: Vec<_> = adapters.iter().filter(|a| a.selected).collect();
        assert_eq!(selected.len(), 1, "exactly one adapter selected");
        assert_eq!(selected[0].id, pick);

        // settings() carries it too; set_settings must NOT clobber it.
        let mut s = svc.settings().await;
        s.dark_mode = !s.dark_mode;
        svc.set_settings(s).await.expect("save settings");
        assert_eq!(
            svc.settings().await.bonded_adapters,
            vec![pick.clone()],
            "set_settings preserves the bonded selection"
        );

        // Clearing the selection restores the single unbound link and
        // restarts again.
        svc.set_bonded_adapters(Vec::new()).await.expect("clear");
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        assert!(
            svc.adapters().await.iter().all(|a| !a.selected),
            "selection cleared"
        );
        let snap = svc.status().await;
        // Linux CI: IPv4 discovery is unavailable (mt-platform note), so
        // the resolved bonded list can be EMPTY → the single fake link
        // carries the name "fake-0". With a resolvable adapter the link
        // name embeds the adapter name. Both are correct; assert >=1 link.
        assert!(!snap.links.is_empty());
        svc.disconnect().await;
    }
}
