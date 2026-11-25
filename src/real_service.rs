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
    adapter_of_link_name, AdapterView, ExitInfo, LinkStatus, MtService, ProfileDetails,
    ProfileView, RuleView, ServiceEvent, ServiceState, SettingsView, SpeedTestInfo, StatusSnapshot,
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
const BIND_WAIT: Duration = Duration::from_secs(8);
// S21: BIND_WAIT covers the server's own bind-retry budget
// (mt-net BIND_RETRY_BUDGET, 5 s) plus polling margin — a reconnect that
// races lingering-socket teardown waits, it does not fail.
/// Bind-wait poll slice.
const BIND_POLL: Duration = Duration::from_millis(10);
/// `ServiceEvent` broadcast buffer (slow GUI receivers lag, never block).
const EVENT_BUFFER: usize = 256;
/// The flow the engine pins (mt-engine hardcodes vision in its config).
const ENGINE_FLOW: &str = "xtls-rprx-vision";
/// S19: overall budget for one exit-IP probe (unchanged; the temp-tunnel
/// variant also pays engine spawn inside this budget).
const EXIT_PROBE_BUDGET: Duration = Duration::from_secs(12);
/// S19: exit probe body cap (the identity document is tiny).
const EXIT_PROBE_MAX_BYTES: u64 = 64 * 1024;
/// S19: overall budget for one speed test (transfer time only; the
/// temp-tunnel setup also pays inside this window when offline).
const SPEED_TEST_BUDGET: Duration = Duration::from_secs(30);
/// S19: stop reading after this many bytes even if the server has more.
const SPEED_TEST_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// S19: per-read stall limit inside a speed test — a server that trickles
/// slower than this yields the partial measurement instead of hanging.
const SPEED_TEST_STALL: Duration = Duration::from_secs(5);
/// S20 (Copilot round 2): the response-head buffer in a speed test is
/// capped INDEPENDENTLY of the 64 MiB payload limit — until `\r\n\r\n`
/// is found nothing counts toward `total`, so an endpoint streaming a
/// header-less byte flood would otherwise grow the buffer for the whole
/// 30 s budget. A legitimate HTTP head never approaches 64 KiB.
const SPEED_TEST_MAX_HEAD: usize = 64 * 1024;
/// S22: the upload payload size — one fixed POST body pushed through
/// the tunnel (mirrors the download default's 25 MB).
const SPEED_TEST_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;
/// S19: the kill switch's black-hole proxy target — a loopback port
/// nothing legitimately listens on (discard). With the system proxy
/// pointed here, WinINET consumers get instant connection-refused
/// instead of silently going DIRECT after a spontaneous tunnel death
/// (fail-closed). Reuses `sysproxy::set` so the ORIGINAL pre-tunnel
/// snapshot stays intact for the eventual restore. While engaged the
/// app also BINDS the port (see [`spawn_blackhole`]) so a local process
/// cannot pre-bind it and turn the black hole into its own proxy.
const KILL_SWITCH_BLACKHOLE_PROXY: &str = "127.0.0.1:9";
/// S19 (review fix): the port [`KILL_SWITCH_BLACKHOLE_PROXY`] names.
/// Windows does not reserve ports < 1024 for admins, so an unprivileged
/// app can own it; on hosts where the bind fails the dead-port semantics
/// (instant connection-refused) still hold.
const KILL_SWITCH_BLACKHOLE_PORT: u16 = 9;

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
                kill_switch: false,
                diagnostics: false,
                speed_test_url: default_speed_test_url(),
                speed_test_upload_url: default_speed_test_upload_url(),
                qos_fair: false,
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

/// S19: the default speed-test URL (mirrors the mt-service serde default
/// so a fresh SettingsFile and a freshly-parsed one agree).
fn default_speed_test_url() -> String {
    "http://speed.cloudflare.com/__down?bytes=25000000".to_string()
}

/// S22: the default upload-test URL (mirrors the mt-service serde
/// default, same reason).
fn default_speed_test_upload_url() -> String {
    "http://speed.cloudflare.com/__up".to_string()
}

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
    /// S20 (Copilot round 2): the profile this session was BUILT from —
    /// the active profile can be switched without reconnecting, and the
    /// exit probe / speed test must only ride the live listeners when
    /// they actually belong to the target profile (otherwise the old
    /// tunnel's exit gets reported against the new profile's endpoint).
    profile_id: String,
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
    /// S20 (Copilot round 2): serializes every system-proxy hand-off
    /// (give_up's kill-switch engage, both disconnect restore paths, the
    /// kill-switch toggle-off escape hatch, connect step-0 self-heal and
    /// the connect-time arming) so a `give_up` racing an explicit
    /// disconnect can never re-install the black-hole AFTER the disconnect
    /// already restored the user's proxy. Std lock, never held across an
    /// await (the black-hole BIND is hoisted out, see `give_up`).
    proxy_mtx: Mutex<()>,
    /// S21 P0 fix (H3): serializes every profiles.json read-modify-write
    /// (import/remove/set_active/update). The GUI runs iced message tasks
    /// concurrently and the CLI can race the GUI process; without this
    /// lock two interleaved load→mutate→save cycles could lose an update
    /// (or collide on the shared per-PID temp name).
    store_mtx: Mutex<()>,
    /// S19 (review fix): the kill switch's black-hole listener — while
    /// engaged we OWN [`KILL_SWITCH_BLACKHOLE_PORT`] so a local process
    /// cannot pre-bind it and turn the black hole into its own proxy.
    /// Accept loop drops every connection; the watch sender stops it.
    blackhole: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
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
        // S19: every GUI-visible line doubles into the diagnostics file
        // (no-op when the mode is off).
        if let ServiceEvent::LogLine(line) = &ev {
            crate::diagnostics::line("GUI", line);
        }
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
        let svc = Self {
            core: Arc::new(Core {
                paths,
                factory,
                engine_log_level: Arc::new(RwLock::new(None)),
                state: RwLock::new(ServiceState::Disconnected),
                events,
                session: Mutex::new(None),
                bytes: Mutex::new(HashMap::new()),
                sysproxy_set: AtomicBool::new(false),
                proxy_mtx: Mutex::new(()),
                store_mtx: Mutex::new(()),
                blackhole: Mutex::new(None),
                disconnect_requested: AtomicBool::new(false),
                restarts: AtomicU32::new(0),
            }),
        };
        // S22: a diagnostics toggle persisted ON must actually write —
        // re-enable the sink AT CONSTRUCTION. Previously the sink only
        // ever started on an off→on transition inside one process
        // lifetime, so after a restart the Settings toggle showed ON
        // while no file was written and "Open log folder" reported the
        // mode off. A failed enable keeps the persisted flag (the next
        // settings save retries through the off→on diff) and logs loudly.
        {
            let settings = SettingsFile::load(&svc.core.paths.settings_file());
            if settings.settings.diagnostics {
                let header = svc.build_diag_header(&settings.settings);
                match crate::diagnostics::enable(&svc.core.paths.log_dir, &header) {
                    Ok(p) => {
                        svc.core.emit(ServiceEvent::LogLine(format!(
                            "INFO  diagnostics mode ON (restored) — writing {}",
                            p.display()
                        )));
                    }
                    Err(e) => {
                        tracing::warn!("restoring diagnostics mode failed: {e}");
                        svc.core.emit(ServiceEvent::LogLine(format!(
                            "ERROR diagnostics mode could not start (persisted on): {e}"
                        )));
                    }
                }
            }
        }
        svc
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
        //    Best-effort — a failure never blocks connecting. This also
        //    clears any KILL SWITCH black-hole from a previous session.
        //    S20 (Copilot round 2): under the proxy lock — and the OWNERSHIP
        //    flag clears with a successful restore. Leaving it set after
        //    the self-heal consumed the snapshot meant a later disconnect
        //    "restored" with no snapshot, which DISABLES the user's
        //    pre-existing proxy instead of leaving it untouched.
        {
            let _guard = lock(&self.core.proxy_mtx);
            let engaged = self.core.sysproxy_set.load(Ordering::SeqCst);
            if crate::sysproxy::restore(&self.core.paths.state_dir).is_ok() {
                self.core.sysproxy_set.store(false, Ordering::SeqCst);
                stop_blackhole(&self.core);
            } else if !engaged {
                // Nothing owned — release any dangling listener.
                stop_blackhole(&self.core);
            }
            // engaged && restore failed: keep the flag AND the listener —
            // the proxy still points at the black hole (fail-closed) and
            // the next disconnect retries the restore.
        }

        let settings = SettingsFile::load(&self.core.paths.settings_file());
        // S9: refresh the engine log level BEFORE links are built.
        // S19: diagnostics mode forces `debug` for the session (the whole
        // point is capturing enough signal for a support conversation).
        if crate::diagnostics::is_on() {
            *wlock(&self.core.engine_log_level) = Some("debug".to_string());
            self.core.emit(ServiceEvent::LogLine(
                "INFO  diagnostics: engine log level forced to debug for this session".into(),
            ));
        } else {
            *wlock(&self.core.engine_log_level) = Some(settings.settings.log_level.clone());
        }
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
        // S21 P1: the background probers get the SAME user-configured probe
        // target as the connect-time probe (previously hardcoded 1.1.1.1:443
        // — a target the tunnel server may block or rate-limit, feeding
        // false quarantines). Parsed before the server is built so the
        // builder receives it.
        let probe_target = parse_probe_target(&settings.probe_target)
            .unwrap_or_else(|| parse_probe_target(DEFAULT_PROBE_TARGET).expect("constant parses"));
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
        let server = Arc::new(
            ProxyServer::new(socks, http, links.clone(), router, policy)
                .with_probe_target(probe_target.clone())
                // S21 (feature o): the fair-share scheduler is ALWAYS
                // attached (initialized from settings) so the Settings
                // toggle can flip it live without rebuilding the session.
                .with_qos(Arc::new(mt_net::QosScheduler::new(
                    settings.settings.qos_fair,
                ))),
        );
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
            profile_id: profile.id.clone(),
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
        // S20 (Copilot round 2): under the proxy lock — an explicit
        // disconnect landing mid-connect must never observe a half-armed
        // proxy hand-off.
        if settings.settings.system_proxy && settings.socks_port != 0 && settings.http_port != 0 {
            let http_addr = lock(&self.core.session)
                .as_ref()
                .and_then(|s| s.server.http_local_addr());
            if let Some(http) = http_addr {
                // The v0.2 full LAN-bypass override (S6-REV-C P1-4): LAN and
                // loopback hosts must never ride the tunnel.
                let override_list = crate::config::default_proxy_override();
                let _guard = lock(&self.core.proxy_mtx);
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

/// S19 (review fix): bind + OWN the black-hole port while the kill
/// switch is engaged. The accept loop drops every connection the moment
/// it lands — nothing legible ever flows. A bind failure is non-fatal:
/// the unbound port still refuses connections (dead-port semantics, still
/// fail-closed); only the local pre-bind guarantee is lost. On Windows an
/// unprivileged process may bind port 9; dev hosts without that right
/// simply take the fallback.
///
/// S20 (Copilot round 2): split into [`bind_blackhole`] (the async bind,
/// hoisted OUT of the proxy lock) and [`engage_blackhole`] (the sync
/// accept-loop install) so `give_up` can bind FIRST and only point the
/// system proxy at the port once ownership is proven — the old order
/// (set-then-bind) left a window where a local process could own port 9
/// and receive the black-holed traffic.
async fn bind_blackhole() -> Option<tokio::net::TcpListener> {
    let addr = SocketAddr::from(([127, 0, 0, 1], KILL_SWITCH_BLACKHOLE_PORT));
    match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => Some(l),
        Err(e) => {
            tracing::warn!(
                "kill switch: could not own the black-hole port {addr} ({e}) \
                 — dead-port fallback (still fail-closed)"
            );
            None
        }
    }
}

/// S20 (Copilot round 2): install the accept loop for an ALREADY-BOUND
/// listener and record it in [`Core::blackhole`]. Sync by design — called
/// with the proxy lock held (the bind was done beforehand).
fn engage_blackhole(core: &Core, listener: tokio::net::TcpListener) {
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                changed = rx.changed() => {
                    if changed.is_err() || *rx.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    if accepted.is_err() {
                        break;
                    }
                    // The accepted socket drops here BY DESIGN — the
                    // connection is swallowed, never read, never answered.
                }
            }
        }
    });
    *lock(&core.blackhole) = Some(tx);
}

/// S19 (review fix): stop the black-hole listener (every proxy-restore
/// path: explicit disconnect, the kill-switch toggle-off escape hatch,
/// and the connect step-0 self-heal).
fn stop_blackhole(core: &Core) {
    if let Some(tx) = lock(&core.blackhole).take() {
        let _ = tx.send(true);
    }
}

/// D5 fail-open give-up (v0.2 supervisor semantics for restart exhaustion):
/// retire the session, mark `Failed`, and stop the server. The system
/// proxy hand-off depends on the S19 kill switch:
///
/// - **kill switch OFF** (the pre-S19 D5 behavior): the system proxy is
///   RESTORED — the user's browsing keeps working, traffic silently goes
///   direct (fail-open by design).
/// - **kill switch ON**: the proxy is re-pointed at
///   [`KILL_SWITCH_BLACKHOLE_PROXY`] — a dead loopback port. WinINET
///   consumers then FAIL instead of leaking outside the tunnel
///   (fail-closed). The original pre-tunnel snapshot stays on disk
///   (keep-first), so a later connect's step-0 self-heal or an explicit
///   disconnect still restores the true pre-tunnel state. Apps that use
///   the SOCKS/HTTP listeners directly are fail-closed either way: the
///   listeners die with the session and proxied connections were already
///   refused when all links were down.
///
/// S20 (Copilot round 2) hardening:
/// - the black-hole port is BOUND FIRST — `sysproxy::set` only runs once
///   ownership is proven, so a local process that owns port 9 can never
///   receive the black-holed traffic (the stale proxy entry keeps
///   blocking instead, which is still fail-closed);
/// - the whole hand-off runs under [`Core::proxy_mtx`], so an explicit
///   `disconnect()` can no longer restore the user's proxy in the window
///   between `give_up`'s flag read and its registry write (which used to
///   re-install the black-hole AFTER the disconnect had returned).
///
/// Best-effort throughout — the app never dies invisibly (D15).
async fn give_up(core: &Core, shutdown: &tokio::sync::watch::Sender<bool>) {
    tracing::warn!("supervisor: all tunnel links exhausted — giving up");
    // Bind BEFORE any registry write (and outside the proxy lock — the
    // bind awaits). Ownership is proven up front; the listener is dropped
    // unused whenever the decisions below do not engage the kill switch.
    let listener = bind_blackhole().await;
    {
        let _guard = lock(&core.proxy_mtx);
        if core.sysproxy_set.load(Ordering::SeqCst) {
            let kill_switch = SettingsFile::load(&core.paths.settings_file())
                .settings
                .kill_switch;
            if kill_switch {
                // Keep sysproxy_set TRUE: we still own the proxy state (the
                // black hole IS ours); the next explicit disconnect or
                // connect self-heal restores it.
                let override_list = crate::config::default_proxy_override();
                if let Some(listener) = listener {
                    match crate::sysproxy::set(
                        KILL_SWITCH_BLACKHOLE_PROXY,
                        &override_list,
                        &core.paths.state_dir,
                    ) {
                        Ok(()) => {
                            // Review fix: OWN the port so a local process
                            // cannot pre-bind it and turn the black hole
                            // into a proxy of its own (the accept loop
                            // swallows connections).
                            engage_blackhole(core, listener);
                            core.emit(ServiceEvent::LogLine(
                                "KILL SWITCH engaged: the tunnel died on its own — system traffic is \
                                 now BLOCKED (fail-closed) instead of leaking direct. Connect again \
                                 to resume, or toggle the kill switch off in Settings."
                                    .into(),
                            ));
                        }
                        Err(e) => {
                            // The registry still points at OUR dead local
                            // listener — still fail-closed, just less tidy.
                            // The freshly-bound listener is useless now
                            // (nothing points at port 9): release it.
                            drop(listener);
                            core.emit(ServiceEvent::LogLine(format!(
                                "KILL SWITCH: could not engage the black-hole proxy ({e}) — the stale \
                                 proxy entry keeps blocking traffic (fail-closed)"
                            )));
                        }
                    }
                } else {
                    // S20 (Copilot round 2): port 9 is owned by someone else
                    // (or unbindable) — do NOT point the system proxy at
                    // it; the stale entry keeps refusing (fail-closed).
                    core.emit(ServiceEvent::LogLine(
                        "KILL SWITCH: could not own the black-hole port — the stale proxy entry \
                         keeps blocking traffic (fail-closed; nothing is routed to a foreign \
                         listener)"
                            .into(),
                    ));
                }
            } else if core.sysproxy_set.swap(false, Ordering::SeqCst) {
                if let Err(e) = crate::sysproxy::restore(&core.paths.state_dir) {
                    tracing::warn!("system proxy restore failed: {e}");
                }
            }
        }
    }
    // Retire the session so status/profiles views stop reporting the dead
    // server (the next connect replaces it anyway; dropping it also drops
    // the last server Arc → run() exits).
    drop(lock(&core.session).take());
    core.emit(ServiceEvent::LogLine(
        "all tunnel links down — disconnected".into(),
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
        adapter_name: adapter_of_link_name(link),
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

/// S20 (Copilot round 2): a support-file-safe rendering of a
/// user-configurable URL — scheme + host + port + path ONLY. Userinfo
/// (`http://user:pass@host/`), query strings (`?token=...`) and
/// fragments are stripped: those slots routinely carry API keys in
/// custom probe/speed endpoints, and the diagnostics file is designed
/// to be pasted into a chat verbatim. An unparseable URL renders as a
/// fixed placeholder (never echoed — the raw value may embed a secret).
fn redact_url(url: &str) -> String {
    match parse_http_url(url) {
        Some((host, port, path)) => {
            // parse_http_url leaves userinfo attached to the host
            // ("user:pass@host") — strip everything up to the last '@'.
            let host = match host.rsplit_once('@') {
                Some((_, h)) => h.to_string(),
                None => host,
            };
            // Path only up to the query ('?' — parse_http_url keeps it in
            // the path) and the fragment ('#').
            let path = path.split(['?', '#']).next().unwrap_or("/").to_string();
            format!("http://{host}:{port}{path}")
        }
        None => "<redacted (invalid URL)>".to_string(),
    }
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
                    adapter_name: adapter_of_link_name(&c.name),
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
            // S19: an explicit disconnect must ALSO release a kill-switch
            // black-hole (or any proxy state we still own from a session
            // that already died) — the user asked to stop tunneling, not
            // to keep the network blocked.
            // S20 (Copilot round 2): under the proxy lock, and the
            // ownership/listener are only released when the restore
            // SUCCEEDED — a failed restore keeps the black-hole engaged
            // (fail-closed) and this same path retries on the next
            // disconnect; the "restored" log line is only truthful then.
            {
                let _guard = lock(&self.core.proxy_mtx);
                if self.core.sysproxy_set.load(Ordering::SeqCst) {
                    match crate::sysproxy::restore(&self.core.paths.state_dir) {
                        Ok(()) => {
                            self.core.sysproxy_set.store(false, Ordering::SeqCst);
                            stop_blackhole(&self.core);
                            self.core
                                .emit(ServiceEvent::LogLine("system proxy restored".into()));
                        }
                        Err(e) => {
                            tracing::warn!("system proxy restore failed: {e}");
                            self.core.emit(ServiceEvent::LogLine(format!(
                                "ERROR system proxy restore failed: {e} — the kill switch stays \
                                 engaged; disconnect again to retry"
                            )));
                        }
                    }
                }
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
        // S20 (Copilot round 2): same locked, restore-first discipline as
        // the no-session branch above.
        {
            let _guard = lock(&self.core.proxy_mtx);
            if self.core.sysproxy_set.load(Ordering::SeqCst) {
                match crate::sysproxy::restore(&self.core.paths.state_dir) {
                    Ok(()) => {
                        self.core.sysproxy_set.store(false, Ordering::SeqCst);
                        stop_blackhole(&self.core);
                    }
                    Err(e) => {
                        tracing::warn!("system proxy restore failed: {e}");
                        self.core.emit(ServiceEvent::LogLine(format!(
                            "ERROR system proxy restore failed: {e} — the kill switch stays \
                             engaged; disconnect again to retry"
                        )));
                    }
                }
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
        // S21 P0 fix (H3): the load→upsert→save cycle is serialized with
        // the other store mutations (lock never held across an await —
        // the whole section below is synchronous filesystem work).
        let _store_guard = lock(&self.core.store_mtx);
        let mut store = self.core.load_store().map_err(|e| e.to_string())?;
        let id = store.upsert(profile.clone());
        self.core.save_store(&store).map_err(|e| e.to_string())?;
        drop(_store_guard);
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
        let _store_guard = lock(&self.core.store_mtx); // S21 H3 serialization
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
        let _store_guard = lock(&self.core.store_mtx); // S21 H3 serialization
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
        let links = self
            .core
            .factory
            .build(&profile, &bonded)
            .await
            .map_err(|e| e.to_string())?;
        // S20 (Copilot round 2): the FIRST link is the top-priority
        // adapter (engine_specs_for preserves the user's order) — pop()
        // used to probe through the lowest-priority one.
        let Some(link) = links.first().cloned() else {
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
        // S19: the speed-test URL lives under the same plain-HTTP rule.
        if parse_http_url(&settings.speed_test_url).is_none() {
            return Err(format!(
                "speed test URL must be plain http:// ({:?})",
                settings.speed_test_url
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
        // S19: remember the diagnostics + kill-switch transitions to act
        // on them AFTER the (successful) persist below.
        let diagnostics_changed = file.settings.diagnostics != settings.diagnostics;
        let kill_switch_disabled = file.settings.kill_switch && !settings.kill_switch;
        // S21 (feature o): the QoS fair-share scheduler flips LIVE on the
        // attached session — no reconnect (nothing in the data plane
        // changes; the scheduler's enabled flag is atomic).
        let qos_changed = file.settings.qos_fair != settings.qos_fair;
        // S20 (Copilot round 2): the engines only pick up the debug log
        // level at SPAWN time, so enabling diagnostics while CONNECTED
        // must reconnect — otherwise the file silently misses the engine
        // debug output the UI promises.
        let diag_enabled_live = !file.settings.diagnostics
            && settings.diagnostics
            && self.core.get_state() == ServiceState::Connected;
        // S20: only set when the sink actually started (a failed + reverted
        // enable must NOT trigger the live reconnect below).
        let mut diag_started = false;
        let mut settings = settings;
        settings.bonded_adapters = file.settings.bonded_adapters.clone();
        file.settings = settings.clone();
        file.sync_legacy();
        file.save(&self.core.paths.settings_file())
            .map_err(|e| e.to_string())?;
        // S19: diagnostics mode transitions (only after a successful save —
        // a rejected settings change never touches the log file).
        if diagnostics_changed {
            if settings.diagnostics {
                let header = self.build_diag_header(&settings);
                match crate::diagnostics::enable(&self.core.paths.log_dir, &header) {
                    Ok(p) => {
                        diag_started = true;
                        self.core.emit(ServiceEvent::LogLine(format!(
                            "INFO  diagnostics mode ON — writing {}",
                            p.display()
                        )));
                    }
                    Err(e) => {
                        // S20 (Copilot round 2): the toggle must not stay
                        // persisted-ON with no sink behind it (the UI
                        // would show the mode enabled while nothing is
                        // written, and a later settings save would skip
                        // the enable because the diff vanished). Revert
                        // the persisted flag and say so.
                        file.settings.diagnostics = false;
                        file.sync_legacy();
                        if let Err(save_err) = file.save(&self.core.paths.settings_file()) {
                            tracing::warn!("reverting the diagnostics toggle failed: {save_err}");
                        }
                        self.core.emit(ServiceEvent::LogLine(format!(
                            "ERROR diagnostics mode could not start: {e} — the toggle was \
                             reverted; fix the cause and enable it again"
                        )));
                    }
                }
            } else if let Some(p) = crate::diagnostics::disable() {
                self.core.emit(ServiceEvent::LogLine(format!(
                    "INFO  diagnostics mode OFF — file kept at {}",
                    p.display()
                )));
            }
        }
        // Reconnect when the LIVE data plane changed, or when diagnostics
        // was enabled on a live session (S20: the engines re-read their
        // log level only at spawn — see diag_enabled_live above). Both
        // AFTER the diagnostics transition so connect_flow sees the new
        // sink state.
        if (dataplane_changed || (diag_enabled_live && diag_started))
            && self.core.get_state() == ServiceState::Connected
        {
            self.core.emit(ServiceEvent::LogLine(if dataplane_changed {
                "data-plane settings changed — reconnecting".into()
            } else {
                "diagnostics on — reconnecting so the engines log at debug".into()
            }));
            self.disconnect().await;
            self.connect().await;
        }
        // S21 (feature o): live QoS flip — applies to the attached session
        // immediately (the pump gate re-checks the flag per read) and to
        // every future session through settings.json.
        if qos_changed {
            let sched = lock(&self.core.session)
                .as_ref()
                .and_then(|s| s.server.qos());
            match sched {
                Some(q) => {
                    q.set_enabled(settings.qos_fair);
                    self.core.emit(ServiceEvent::LogLine(if settings.qos_fair {
                        "INFO  QoS fair share ON — no single site/application can hog the bandwidth anymore".into()
                    } else {
                        "INFO  QoS fair share OFF".into()
                    }));
                }
                None => {
                    // No live session: the flag persists; the next connect
                    // attaches a scheduler initialized from settings.json.
                }
            }
        }
        // S19: turning the kill switch off is an explicit escape hatch for
        // an engaged black-hole (the give_up path keeps the proxy blocked
        // while the switch stays on). S20 (Copilot round 2): under the
        // proxy lock, restore-first — ownership and the listener are only
        // released on a successful restore, so a failed escape hatch can
        // be retried instead of stranding a black-holed proxy with no
        // owner.
        if kill_switch_disabled && self.core.get_state() != ServiceState::Connected {
            let _guard = lock(&self.core.proxy_mtx);
            if self.core.sysproxy_set.load(Ordering::SeqCst) {
                match crate::sysproxy::restore(&self.core.paths.state_dir) {
                    Ok(()) => {
                        self.core.sysproxy_set.store(false, Ordering::SeqCst);
                        stop_blackhole(&self.core);
                        self.core.emit(ServiceEvent::LogLine(
                            "kill switch disabled — system proxy restored".into(),
                        ));
                    }
                    Err(e) => {
                        tracing::warn!("system proxy restore failed: {e}");
                        self.core.emit(ServiceEvent::LogLine(format!(
                            "ERROR system proxy restore failed: {e} — the black hole stays \
                             engaged; toggle the kill switch off again to retry"
                        )));
                    }
                }
            }
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
        // 1. Prefer the LIVE session (the whole point is the tunnel's
        //    egress as the internet sees it, D18). S19: with no live
        //    session, fall back to a TEMPORARY probe tunnel through the
        //    active profile — same exit identity, works while the app is
        //    disconnected, no session left behind.
        //    S20 (Copilot round 2): the live listeners only count when the
        //    session was BUILT from this profile — the active profile can
        //    be switched without reconnecting, and the old tunnel's exit
        //    must never be reported against the new profile's endpoint.
        let store = self.core.load_store_lossy();
        let Some(profile) = store
            .active
            .as_deref()
            .and_then(|id| store.profiles.iter().find(|p| p.id == id))
            .cloned()
        else {
            return Err("no active profile — pick a server first".to_string());
        };
        let socks_addr = {
            let guard = lock(&self.core.session);
            match guard.as_ref() {
                Some(s) if s.profile_id == profile.id => s.server.socks_local_addr(),
                _ => None,
            }
        };
        let settings = SettingsFile::load(&self.core.paths.settings_file());
        let url = settings.settings.exit_probe_url;
        let (host, port, path) =
            parse_http_url(&url).ok_or_else(|| format!("invalid exit probe URL {url:?}"))?;
        // 2. Plain HTTP/1.1 GET through the tunnel (no TLS stack —
        //    zero-dep rule; the client→server leg is REALITY-encrypted
        //    and the payload is only an IP echo).
        let target = TargetAddr {
            host: if let Ok(ip) = host.parse() {
                Host::Ip(ip)
            } else {
                Host::Domain(host.clone())
            },
            port,
        };
        let probe = async {
            let body = if let Some(socks) = socks_addr {
                let outbound = mt_net::Socks5Outbound::new(socks);
                let mut stream = outbound.connect(&target).await?;
                http_get(&mut stream, &host, &path, EXIT_PROBE_MAX_BYTES).await?
            } else {
                let link = temp_link_for(&self.core, &profile)
                    .await
                    .map_err(anyhow::Error::msg)?;
                let mut stream = link.connect(&target).await?;
                http_get(&mut stream, &host, &path, EXIT_PROBE_MAX_BYTES).await?
            };
            Ok::<Vec<u8>, anyhow::Error>(body)
        };
        let body = tokio::time::timeout(EXIT_PROBE_BUDGET, probe)
            .await
            .map_err(|_| "exit probe timed out".to_string())?
            .map_err(|e| format!("exit probe failed: {e}"))?;
        // 3. Parse the identity document.
        let exit = parse_exit_info(&body)?;
        // 4. Direct-exit verdict: the exit IP should BE the profile server.
        //    A domain profile is resolved locally for the comparison.
        let server_endpoint = format!("{}:{}", profile.address, profile.port);
        let direct = exit.exit_ip == profile.address
            || std::net::ToSocketAddrs::to_socket_addrs(&server_endpoint)
                .map(|addrs| {
                    addrs
                        .map(|a| a.ip().to_string())
                        .any(|ip| ip == exit.exit_ip)
                })
                .unwrap_or(false);
        Ok(ExitInfo {
            exit_ip: exit.exit_ip,
            country: exit.country,
            city: exit.city,
            isp: exit.isp,
            asn: exit.asn,
            server_endpoint,
            direct_exit: direct,
            via_temp_tunnel: socks_addr.is_none(),
        })
    }

    async fn speed_test(&self, id: Option<String>) -> Result<SpeedTestInfo, String> {
        // 1. Resolve the target profile: explicit id (or tag), else the
        //    active one.
        let store = self.core.load_store().map_err(|e| e.to_string())?;
        let profile = match id {
            Some(id) => find_profile(&store, &id)
                .cloned()
                .ok_or_else(|| format!("no profile matches {id:?}"))?,
            None => store
                .active
                .as_deref()
                .and_then(|a| store.profiles.iter().find(|p| p.id == a))
                .cloned()
                .ok_or_else(|| "no active profile — pick a server first".to_string())?,
        };
        let settings = SettingsFile::load(&self.core.paths.settings_file());
        let url = settings.settings.speed_test_url;
        let (host, port, path) =
            parse_http_url(&url).ok_or_else(|| format!("invalid speed test URL {url:?}"))?;
        let target = TargetAddr {
            host: if let Ok(ip) = host.parse() {
                Host::Ip(ip)
            } else {
                Host::Domain(host.clone())
            },
            port,
        };
        // 2. The LIVE session only counts when it was BUILT from the
        //    target profile (S20, Copilot round 2 — the active profile can
        //    be switched without reconnecting, so matching on "is the
        //    target the active one" is not enough); otherwise a temporary
        //    tunnel through the target profile.
        let live_socks = lock(&self.core.session)
            .as_ref()
            .filter(|s| s.profile_id == profile.id)
            .and_then(|s| s.server.socks_local_addr());
        let via_temp = live_socks.is_none();
        let connect = async {
            Ok::<_, anyhow::Error>(if let Some(socks) = live_socks {
                let outbound = mt_net::Socks5Outbound::new(socks);
                outbound.connect(&target).await?
            } else {
                let link = temp_link_for(&self.core, &profile)
                    .await
                    .map_err(anyhow::Error::msg)?;
                link.connect(&target).await?
            })
        };
        // 3. Download: the clock starts AFTER the connect (tunnel setup —
        //    possibly an engine spawn — must not pollute the rate).
        let connect = tokio::time::timeout(SPEED_TEST_BUDGET, connect)
            .await
            .map_err(|_| "speed test: tunnel setup timed out".to_string())?
            .map_err(|e| format!("speed test: connect failed: {e}"))?;
        let mut stream = connect;
        // The request itself is outside the measurement window too (it is
        // one small write; servers answer after it lands).
        {
            use tokio::io::AsyncWriteExt as _;
            let req = format!(
                "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: MultiTunnel/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
                env!("CARGO_PKG_VERSION")
            );
            stream
                .write_all(req.as_bytes())
                .await
                .map_err(|e| format!("speed test: request failed: {e}"))?;
        }
        use tokio::io::AsyncReadExt as _;
        let started = Instant::now();
        let mut head = Vec::new();
        let mut total: u64 = 0;
        let mut chunk = vec![0u8; 64 * 1024];
        let mut first = true;
        loop {
            if total >= SPEED_TEST_MAX_BYTES || started.elapsed() >= SPEED_TEST_BUDGET {
                break;
            }
            let read = tokio::time::timeout(SPEED_TEST_STALL, stream.read(&mut chunk)).await;
            let n = match read {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    if total > 0 {
                        break; // partial measurement stands
                    }
                    return Err(format!("speed test: read failed: {e}"));
                }
                Err(_) => break, // stall: keep what we have
            };
            // The response head (up to the blank line) is not payload —
            // peel it off the first read(s).
            if first && n > 0 {
                head.extend_from_slice(&chunk[..n]);
                if let Some(split) = find_head_end(&head) {
                    // Body bytes = everything past the head end in the
                    // WHOLE head buffer (the split may sit mid-buffer
                    // when the head straddled reads).
                    total += head.len() as u64 - split as u64;
                    first = false;
                } else if head.len() > SPEED_TEST_MAX_HEAD {
                    // S20 (Copilot round 2): still inside the head past
                    // 64 KiB — no legitimate HTTP head is this big. Cap
                    // INDEPENDENTLY of the payload limit: until the
                    // terminator is found nothing counts toward `total`,
                    // so a header-less byte flood would otherwise grow
                    // this buffer for the whole 30 s budget.
                    return Err(format!(
                        "speed test: malformed response (no header end within {} KiB)",
                        SPEED_TEST_MAX_HEAD / 1024
                    ));
                }
                // Still inside the head (a slow server dribbling headers):
                // keep buffering, nothing counts yet.
            } else {
                total += n as u64;
            }
        }
        if first && !head.is_empty() {
            // The server closed before a complete head — nothing measured.
            return Err("speed test: no usable response (connection closed early)".into());
        }
        if total == 0 {
            return Err("speed test: no data received".into());
        }
        // Reject obvious HTTP errors (a captive portal answering 302/407
        // would otherwise measure as "throughput").
        let head_text = String::from_utf8_lossy(&head);
        if let Some(status) = head_text.lines().next() {
            let code = status.split_whitespace().nth(1).unwrap_or("");
            if let Ok(code) = code.parse::<u16>() {
                if code >= 400 {
                    return Err(format!("speed test: server answered {code} ({status})"));
                }
            }
        }
        let seconds = started.elapsed().as_secs_f64().max(0.001);
        let mbps = total as f64 * 8.0 / seconds / 1_000_000.0;
        Ok(SpeedTestInfo {
            mbps,
            mbytes: total as f64 / (1024.0 * 1024.0),
            seconds,
            profile_tag: profile.tag.clone(),
            via_temp_tunnel: via_temp,
        })
    }

    /// S22: the UPLOAD half of the speed test — POST a fixed-size payload
    /// through the tunnel and report the send rate. Same profile/tunnel
    /// selection and caps as `speed_test`; the clock starts AFTER the
    /// connect (tunnel setup must not pollute the rate), the request head
    /// is outside the measurement window, and a budget/stall-truncated
    /// run stands as a partial measurement exactly like the download side.
    async fn speed_test_upload(&self, id: Option<String>) -> Result<SpeedTestInfo, String> {
        // 1. Resolve the target profile: explicit id (or tag), else the
        //    active one.
        let store = self.core.load_store().map_err(|e| e.to_string())?;
        let profile = match id {
            Some(id) => find_profile(&store, &id)
                .cloned()
                .ok_or_else(|| format!("no profile matches {id:?}"))?,
            None => store
                .active
                .as_deref()
                .and_then(|a| store.profiles.iter().find(|p| p.id == a))
                .cloned()
                .ok_or_else(|| "no active profile — pick a server first".to_string())?,
        };
        let settings = SettingsFile::load(&self.core.paths.settings_file());
        let url = settings.settings.speed_test_upload_url;
        let (host, port, path) =
            parse_http_url(&url).ok_or_else(|| format!("invalid upload test URL {url:?}"))?;
        let target = TargetAddr {
            host: if let Ok(ip) = host.parse() {
                Host::Ip(ip)
            } else {
                Host::Domain(host.clone())
            },
            port,
        };
        // 2. Live session only when BUILT from the target profile (the
        //    S20 rule the download side carries); otherwise a temporary
        //    tunnel through the target profile.
        let live_socks = lock(&self.core.session)
            .as_ref()
            .filter(|s| s.profile_id == profile.id)
            .and_then(|s| s.server.socks_local_addr());
        let via_temp = live_socks.is_none();
        let connect = async {
            Ok::<_, anyhow::Error>(if let Some(socks) = live_socks {
                let outbound = mt_net::Socks5Outbound::new(socks);
                outbound.connect(&target).await?
            } else {
                let link = temp_link_for(&self.core, &profile)
                    .await
                    .map_err(anyhow::Error::msg)?;
                link.connect(&target).await?
            })
        };
        let connect = tokio::time::timeout(SPEED_TEST_BUDGET, connect)
            .await
            .map_err(|_| "upload speed test: tunnel setup timed out".to_string())?
            .map_err(|e| format!("upload speed test: connect failed: {e}"))?;
        let mut stream = connect;
        // The request head (with the announced Content-Length) is outside
        // the measurement window too.
        {
            use tokio::io::AsyncWriteExt as _;
            let req = format!(
                "POST {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: MultiTunnel/{}\r\nAccept: */*\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                env!("CARGO_PKG_VERSION"),
                SPEED_TEST_UPLOAD_BYTES
            );
            stream
                .write_all(req.as_bytes())
                .await
                .map_err(|e| format!("upload speed test: request failed: {e}"))?;
        }
        use tokio::io::AsyncWriteExt as _;
        let started = Instant::now();
        let chunk = vec![b'x'; 64 * 1024];
        let mut total: u64 = 0;
        let mut budget_hit = false;
        while total < SPEED_TEST_UPLOAD_BYTES {
            if started.elapsed() >= SPEED_TEST_BUDGET {
                budget_hit = true;
                break;
            }
            let left = (SPEED_TEST_UPLOAD_BYTES - total) as usize;
            let n = left.min(chunk.len());
            let write = tokio::time::timeout(SPEED_TEST_STALL, stream.write_all(&chunk[..n])).await;
            match write {
                Ok(Ok(())) => total += n as u64,
                Ok(Err(e)) => {
                    if total > 0 {
                        break; // partial measurement stands
                    }
                    return Err(format!("upload speed test: write failed: {e}"));
                }
                Err(_) => break, // stall: keep what we have
            }
        }
        if total == 0 {
            return Err("upload speed test: no data sent".into());
        }
        // When the payload completed cleanly we can afford the status
        // guard (a captive portal answering 302/407 must not measure as
        // "throughput"). A budget/stall-truncated run stands as a partial
        // measurement — the server may never answer a truncated body.
        if !budget_hit && total >= SPEED_TEST_UPLOAD_BYTES {
            use tokio::io::AsyncReadExt as _;
            let mut head = Vec::new();
            let mut buf = vec![0u8; 4096];
            loop {
                match tokio::time::timeout(SPEED_TEST_STALL, stream.read(&mut buf)).await {
                    Ok(Ok(0)) | Err(_) => break,
                    Ok(Ok(n)) => {
                        head.extend_from_slice(&buf[..n]);
                        if find_head_end(&head).is_some() {
                            break;
                        }
                        if head.len() > SPEED_TEST_MAX_HEAD {
                            return Err(format!(
                                "upload speed test: malformed response (no header end within {} KiB)",
                                SPEED_TEST_MAX_HEAD / 1024
                            ));
                        }
                    }
                    Ok(Err(e)) => {
                        return Err(format!("upload speed test: read failed: {e}"));
                    }
                }
            }
            let head_text = String::from_utf8_lossy(&head);
            if let Some(status) = head_text.lines().next() {
                let code = status.split_whitespace().nth(1).unwrap_or("");
                if let Ok(code) = code.parse::<u16>() {
                    if code >= 400 {
                        return Err(format!(
                            "upload speed test: server answered {code} ({status})"
                        ));
                    }
                }
            }
        }
        let seconds = started.elapsed().as_secs_f64().max(0.001);
        let mbps = total as f64 * 8.0 / seconds / 1_000_000.0;
        Ok(SpeedTestInfo {
            mbps,
            mbytes: total as f64 / (1024.0 * 1024.0),
            seconds,
            profile_tag: profile.tag.clone(),
            via_temp_tunnel: via_temp,
        })
    }

    async fn profile_details(&self, id: &str) -> Result<ProfileDetails, String> {
        let store = self.core.load_store().map_err(|e| e.to_string())?;
        let p = find_profile(&store, id).ok_or_else(|| format!("no profile matches {id:?}"))?;
        Ok(details_of(p))
    }

    async fn update_profile(&self, id: &str, edit: ProfileDetails) -> Result<ProfileView, String> {
        let _store_guard = lock(&self.core.store_mtx); // S21 H3 serialization
        let mut store = self.core.load_store().map_err(|e| e.to_string())?;
        let target = find_profile(&store, id)
            .ok_or_else(|| format!("no profile matches {id:?}"))?
            .id
            .clone();
        let original = store
            .profiles
            .iter()
            .find(|p| p.id == target)
            .cloned()
            .expect("find_profile just matched");
        // Apply the edits (trim; validate the primitives the engine will
        // need — a save that would break the profile fails loud here).
        let mut updated = original.clone();
        let tag = edit.tag.trim().to_string();
        if tag.is_empty() {
            return Err("name cannot be empty".to_string());
        }
        updated.tag = tag;
        updated.remark = edit.remark.trim().to_string();
        let address = edit.address.trim().to_string();
        if address.is_empty() {
            return Err("server address cannot be empty".to_string());
        }
        updated.address = address;
        updated.port = edit
            .port
            .trim()
            .parse::<u16>()
            .map_err(|_| format!("port must be a number 1-65535 ({:?})", edit.port))?;
        // S20 (Copilot round 2): u16::parse happily accepts 0, but a
        // server port of 0 is meaningless (imported links reject it; the
        // engine would only fail later at connect time).
        if updated.port == 0 {
            return Err(format!(
                "port 0 is not a valid server port ({:?}) — use 1-65535",
                edit.port
            ));
        }
        let identity = edit.identity.trim().to_string();
        if identity.is_empty() {
            return Err("identity (UUID/password) cannot be empty".to_string());
        }
        updated.identity = identity;
        // Security-layer fields only apply to a REALITY profile (the dialog
        // hides them otherwise; stale values never overwrite the store).
        if let Security::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x,
        } = &mut updated.security
        {
            *sni = edit.sni.trim().to_string();
            *fingerprint = edit.fingerprint.trim().to_string();
            *public_key = edit.public_key.trim().to_string();
            *short_id = edit.short_id.trim().to_string();
            let spider = edit.spider_x.trim().to_string();
            *spider_x = (!spider.is_empty()).then_some(spider);
        }
        // Capability regression check: if the ORIGINAL could drive the
        // engine, the edited one must still be able to (fail loud BEFORE
        // the save; a profile that never could connect may be edited
        // freely — its inability predates the edit). S20 (Copilot round
        // 2): for convertible profiles the edited REALITY material gets
        // the SAME structural validation the link parser enforces — an
        // empty SNI, a wrong-length/non-base64url public key or an
        // invalid short ID must not silently reach the Xray config.
        if endpoint_from_profile(&original).is_ok() {
            endpoint_from_profile(&updated).map_err(|e| e.to_string())?;
            if let Security::Reality {
                sni,
                fingerprint: _,
                public_key,
                short_id,
                spider_x: _,
            } = &updated.security
            {
                if sni.is_empty() {
                    return Err("sni cannot be empty (the REALITY serverName)".to_string());
                }
                if public_key.len() != 43 || !public_key.bytes().all(crate::vless::is_base64url) {
                    return Err(format!(
                        "public key must be 43 unpadded base64url characters \
                         (a 32-byte X25519 key); got {}",
                        public_key.len()
                    ));
                }
                if short_id.len() > 16 || !short_id.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err(format!(
                        "short id must be empty or 1-16 hex characters; got {}",
                        short_id.len()
                    ));
                }
            }
        }
        if !store.update(&target, updated.clone()) {
            return Err(format!("no profile matches {id:?}"));
        }
        self.core.save_store(&store).map_err(|e| e.to_string())?;
        let active = store.active.as_deref() == Some(target.as_str());
        if active && self.core.get_state() == ServiceState::Connected {
            self.core.emit(ServiceEvent::LogLine(
                "INFO  the active server was edited — reconnect to apply".into(),
            ));
        }
        Ok(view_of(&updated, active))
    }

    async fn diagnostics_path(&self) -> Option<String> {
        crate::diagnostics::path().map(|p| p.display().to_string())
    }

    async fn log_dir(&self) -> String {
        self.core.paths.log_dir.display().to_string()
    }

    fn events(&self) -> broadcast::Receiver<ServiceEvent> {
        self.core.events.subscribe()
    }
}

/// S19: find the end of the HTTP response head (`\r\n\r\n`), returning the
/// byte offset where the BODY starts (i.e. the index just past the blank
/// line). `None` while the head is incomplete.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

/// S19: plain HTTP/1.1 GET over an established stream, capped: reads to
/// EOF or `max_bytes` (whichever first). Shared by the exit probe (live +
/// temp-tunnel variants) — the speed test keeps its own loop because it
/// needs byte/time accounting the probe does not.
async fn http_get(
    stream: &mut mt_net::ProxyStream,
    host: &str,
    path: &str,
    max_bytes: u64,
) -> anyhow::Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: MultiTunnel/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    );
    stream.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        if buf.len() as u64 >= max_bytes {
            break;
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

/// The parsed exit-identity document (the ip-api.com JSON shape).
struct ExitDoc {
    exit_ip: String,
    country: String,
    city: String,
    isp: String,
    asn: String,
}

/// S19: body → [`ExitDoc`] (shared by the live and temp-tunnel probe
/// paths; factored out of the old inline parsing).
fn parse_exit_info(body: &[u8]) -> Result<ExitDoc, String> {
    let text = String::from_utf8_lossy(body);
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
    Ok(ExitDoc {
        exit_ip,
        country: field("country"),
        city: field("city"),
        isp: field("isp"),
        asn: field("as"),
    })
}

/// S19: build a one-shot link for `profile` (offline probes: exit-IP,
/// speed test). Mirrors `test_latency`'s factory use — engines spawn
/// lazily and die with the returned Arcs. With several bonded adapters
/// the FIRST link (the top-priority adapter) carries the probe — S20
/// (Copilot round 2): `engine_specs_for` preserves the user's priority
/// order, and `pop()` used to silently select the LAST (lowest-priority)
/// link instead.
async fn temp_link_for(core: &Core, profile: &Profile) -> Result<Arc<dyn Outbound>, String> {
    let settings = SettingsFile::load(&core.paths.settings_file());
    let bonded = resolve_bonded(
        &settings.settings.bonded_adapters,
        &mt_platform::enumerate_adapters(),
    );
    let links = core
        .factory
        .build(profile, &bonded)
        .await
        .map_err(|e| e.to_string())?;
    links
        .into_iter()
        .next()
        .ok_or_else(|| "link factory produced no links".to_string())
}

/// S19: [`Profile`] → the Edit dialog's material (credential fields
/// included by necessity; the dialog is the only consumer).
fn details_of(p: &Profile) -> ProfileDetails {
    let (security, sni, fingerprint, public_key, short_id, spider_x) = match &p.security {
        Security::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x,
        } => (
            "reality",
            sni.clone(),
            fingerprint.clone(),
            public_key.clone(),
            short_id.clone(),
            spider_x.clone().unwrap_or_default(),
        ),
        Security::Tls { sni, .. } => (
            "tls",
            sni.clone().unwrap_or_default(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ),
        Security::None => (
            "none",
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ),
    };
    ProfileDetails {
        id: p.id.clone(),
        tag: p.tag.clone(),
        remark: p.remark.clone(),
        protocol: protocol_name(p.protocol).to_string(),
        address: p.address.clone(),
        port: p.port.to_string(),
        identity: p.identity.clone(),
        security: security.to_string(),
        sni,
        fingerprint,
        public_key,
        short_id,
        spider_x,
    }
}

/// S19: [`Profile`] → the GUI list row (single mapping for profiles(),
/// import_link() and update_profile()).
fn view_of(p: &Profile, active: bool) -> ProfileView {
    ProfileView {
        active,
        id: p.id.clone(),
        tag: p.tag.clone(),
        protocol: protocol_name(p.protocol).to_string(),
        endpoint: format!("{}:{}", p.address, p.port),
        remark: p.remark.clone(),
    }
}

impl RealService {
    /// S19: the redacted diagnostics header — everything a support
    /// conversation needs, nothing it must not (D9: no uuid, no password,
    /// no public key values).
    fn build_diag_header(&self, settings: &SettingsView) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "MultiTunnel {} diagnostics — {} ({} {})\n",
            env!("CARGO_PKG_VERSION"),
            fmt_utc(SystemTime::now()),
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        out.push_str("=== settings (no credentials live here) ===\n");
        out.push_str(&format!(
            "merge_policy={} bond_mode={} socks_port={} http_port={} allow_lan={} \
             system_proxy={} kill_switch={} log_level={}\n",
            settings.merge_policy,
            settings.bond_mode,
            settings.socks_port,
            settings.http_port,
            settings.allow_lan,
            settings.system_proxy,
            settings.kill_switch,
            settings.log_level
        ));
        out.push_str(&format!(
            "probe_target={} exit_probe_url={} speed_test_url={} speed_test_upload_url={}\n",
            settings.probe_target,
            redact_url(&settings.exit_probe_url),
            redact_url(&settings.speed_test_url),
            redact_url(&settings.speed_test_upload_url)
        ));
        let store = self.core.load_store_lossy();
        out.push_str("=== profiles (endpoints only — ids/secrets redacted) ===\n");
        for p in &store.profiles {
            let active = if store.active.as_deref() == Some(p.id.as_str()) {
                " [active]"
            } else {
                ""
            };
            out.push_str(&format!(
                "{} {}/{}:{}{}\n",
                p.tag,
                protocol_name(p.protocol),
                p.address,
                p.port,
                active
            ));
        }
        out.push_str("=== bonded adapters ===\n");
        out.push_str(&format!("{:?}\n", settings.bonded_adapters));
        out.push_str("=== events (GUI log lines + tracing/engine output) ===");
        out
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
                kill_switch: false,
                diagnostics: false,
                speed_test_url: default_speed_test_url(),
                speed_test_upload_url: default_speed_test_upload_url(),
                qos_fair: false,
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

        // S19: DISCONNECTED, the probe now works through a temporary
        // probe tunnel (the user's ask) instead of erroring.
        let offline = svc.exit_probe().await.expect("offline probe");
        assert_eq!(offline.exit_ip, "127.0.0.1");
        assert!(offline.via_temp_tunnel, "no live session → temp tunnel");

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
        assert!(!info.via_temp_tunnel, "live session in use");
        assert!(
            !info.direct_exit,
            "203.0.113.10 profile must NOT match a 127.0.0.1 exit"
        );
        svc.disconnect().await;
    }

    #[tokio::test]
    async fn exit_probe_without_active_profile_errors() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("exitnoactive");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        // Profile exists but is NOT active.
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        assert_eq!(
            svc.exit_probe().await.unwrap_err(),
            "no active profile — pick a server first"
        );
        // Sanity: activating makes it work again (temp tunnel).
        svc.set_active_profile(&view.tag).await.expect("set active");
        assert!(svc.exit_probe().await.is_ok());
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

    /// S19: an HTTP server serving a fixed-size body for speed tests.
    async fn spawn_speed_server(mib: usize) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind speed server");
        let addr = listener.local_addr().expect("speed addr");
        let body = vec![b'x'; mib * 1024 * 1024];
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let body = body.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                    let mut buf = vec![0u8; 2048];
                    let _ = stream.read(&mut buf).await; // drain the request head
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                });
            }
        });
        addr
    }

    /// S22: an HTTP server that DRAINS an upload body and answers with a
    /// fixed status (200 = happy path, 403 = the rejection path).
    async fn spawn_upload_server(status: u16) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind upload server");
        let addr = listener.local_addr().expect("upload addr");
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                    // Peel the request head, learn the body length, drain
                    // the announced body, then answer.
                    let mut head = Vec::new();
                    let mut buf = vec![0u8; 8192];
                    let body_len;
                    let mut drained;
                    loop {
                        let n = match stream.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => n,
                        };
                        head.extend_from_slice(&buf[..n]);
                        if let Some(split) = find_head_end(&head) {
                            let text = String::from_utf8_lossy(&head[..split]);
                            body_len = text
                                .lines()
                                .find_map(|l| {
                                    let (k, v) = l.split_once(':')?;
                                    if k.trim().eq_ignore_ascii_case("content-length") {
                                        v.trim().parse().ok()
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(0);
                            drained = head.len() - split;
                            break;
                        }
                    }
                    while drained < body_len {
                        match stream.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => drained += n,
                        }
                    }
                    let head = format!(
                        "HTTP/1.1 {status} S22\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn speed_test_measures_through_temp_tunnel_when_disconnected() {
        let server = spawn_speed_server(2).await; // 2 MiB on loopback
        let paths = scratch("speedtest");
        write_settings(&paths, server.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", server.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: server,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        // Disconnected → temporary probe tunnel; the clock starts after
        // the connect, so the measured rate is the loopback body push.
        let info = svc.speed_test(None).await.expect("speed test");
        assert!(info.via_temp_tunnel);
        assert_eq!(info.profile_tag, view.tag);
        assert!((info.mbytes - 2.0).abs() < 0.05, "body ~2 MiB: {info:?}");
        assert!(info.seconds > 0.0);
        assert!(info.mbps > 1.0, "loopback must beat 1 Mbps: {info:?}");

        // No active profile at all → a clear error.
        svc.remove_profile(&view.id).await.expect("remove");
        assert_eq!(
            svc.speed_test(None).await.unwrap_err(),
            "no active profile — pick a server first"
        );

        // A bad URL is rejected before any dialing.
        let mut file = SettingsFile::load(&svc.core.paths.settings_file());
        file.settings.speed_test_url = "https://no-tls.example/".to_string();
        file.save(&svc.core.paths.settings_file()).expect("save");
        let view2 = svc.import_link(VLESS_REALITY).await.expect("re-import");
        svc.set_active_profile(&view2.tag).await.expect("active");
        let err = svc.speed_test(None).await.unwrap_err();
        assert!(err.contains("invalid speed test URL"), "{err}");
    }

    /// S22: the upload test mirrors the download's offline path — a
    /// temporary probe tunnel, clock after the connect, the full payload
    /// pushed through.
    #[tokio::test]
    async fn speed_test_upload_measures_through_temp_tunnel_when_disconnected() {
        let server = spawn_upload_server(200).await;
        let paths = scratch("uploadtest");
        write_settings(&paths, server.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.speed_test_upload_url = format!("http://127.0.0.1:{}/up", server.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: server,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        let info = svc.speed_test_upload(None).await.expect("upload test");
        assert!(info.via_temp_tunnel);
        assert_eq!(info.profile_tag, view.tag);
        assert!(info.seconds > 0.0);
        assert!(
            (info.mbytes - 25.0).abs() < 2.0,
            "payload ~25 MiB: {info:?}"
        );
        assert!(info.mbps > 10.0, "loopback must beat 10 Mbps: {info:?}");
    }

    /// S22: a 4xx answer must fail the test, never measure as throughput.
    #[tokio::test]
    async fn speed_test_upload_rejects_server_errors() {
        let server = spawn_upload_server(403).await;
        let paths = scratch("upload403");
        write_settings(&paths, server.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.speed_test_upload_url = format!("http://127.0.0.1:{}/up", server.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: server,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        let err = svc.speed_test_upload(None).await.unwrap_err();
        assert!(err.contains("403"), "{err}");
    }

    /// S22: the log dir is always resolvable (not diagnostics-gated) and
    /// IS the `logs` dir under the config dir.
    #[tokio::test]
    async fn log_dir_is_always_resolvable() {
        let echo = spawn_echo().await;
        let paths = scratch("logdir");
        let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
        let dir = svc.log_dir().await;
        assert_eq!(dir, paths.log_dir.display().to_string());
        assert!(paths.log_dir.ends_with("logs"), "{dir}");
    }

    /// S22: a diagnostics toggle persisted ON writes again after a
    /// restart — the sink starts AT CONSTRUCTION, not only on an off→on
    /// transition (the old gap made the Settings toggle lie and the
    /// "Open log folder" button refuse after every restart).
    #[tokio::test]
    async fn diagnostics_persisted_on_starts_at_construction() {
        let echo = spawn_echo().await;
        let paths = scratch("diagrestart");
        write_settings(&paths, echo.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.diagnostics = true;
        file.save(&paths.settings_file()).expect("save");
        let svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
        let p = svc
            .diagnostics_path()
            .await
            .expect("sink restored at startup");
        assert!(std::path::Path::new(&p).exists(), "{p:?}");
        let _ = crate::diagnostics::disable();
    }

    #[tokio::test]
    async fn speed_test_rides_live_session_for_active_profile() {
        let server = spawn_speed_server(1).await;
        let echo = spawn_echo().await;
        let paths = scratch("speedlive");
        // Probe target = echo (so connect() succeeds); speed URL = server.
        write_settings(&paths, echo.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", server.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: server,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        let info = svc.speed_test(None).await.expect("speed test");
        assert!(!info.via_temp_tunnel, "active profile + live session");
        assert!((info.mbytes - 1.0).abs() < 0.05, "1 MiB body: {info:?}");
        svc.disconnect().await;
    }

    #[tokio::test]
    async fn update_profile_edits_fields_and_keeps_id_and_active() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("editprofile");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        let mut details = svc.profile_details(&view.id).await.expect("details");
        assert_eq!(details.protocol, "vless");
        assert_eq!(details.security, "reality");
        assert_eq!(details.port, "443");
        assert_eq!(details.sni, "tunnel.example.org");
        details.tag = "renamed server".into();
        details.remark = "my remark".into();
        details.port = "8443".into();
        details.sni = "new.example.org".into();
        let updated = svc.update_profile(&view.id, details).await.expect("save");
        assert_eq!(updated.tag, "renamed server");
        assert_eq!(updated.endpoint, "203.0.113.10:8443");
        assert!(updated.active, "the edited profile stays active");

        // The store reloads with the same id, the edit and the active mark.
        let store = svc.core.load_store().expect("reload");
        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.profiles[0].id, view.id);
        assert_eq!(store.profiles[0].tag, "renamed server");
        assert_eq!(store.profiles[0].port, 8443);
        assert_eq!(store.active.as_deref(), Some(view.id.as_str()));
        let Security::Reality { sni, .. } = &store.profiles[0].security else {
            panic!("still reality");
        };
        assert_eq!(sni, "new.example.org");

        // Unknown id → clear error.
        assert!(svc.profile_details("nope").await.is_err());
        assert!(svc
            .update_profile("nope", svc.profile_details(&view.id).await.unwrap())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn update_profile_rejects_invalid_edits() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("editinvalid");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        let bad = |tag: &str, port: &str, identity: &str| ProfileDetails {
            id: view.id.clone(),
            tag: tag.to_string(),
            remark: String::new(),
            protocol: "vless".to_string(),
            address: "203.0.113.10".to_string(),
            port: port.to_string(),
            identity: identity.to_string(),
            security: "reality".to_string(),
            sni: "tunnel.example.org".to_string(),
            fingerprint: "chrome".to_string(),
            public_key: "k".to_string(),
            short_id: "0123abcd".to_string(),
            spider_x: String::new(),
        };
        assert!(svc
            .update_profile(&view.id, bad("", "443", "u"))
            .await
            .unwrap_err()
            .contains("name"));
        assert!(svc
            .update_profile(&view.id, bad("t", "not-a-port", "u"))
            .await
            .unwrap_err()
            .contains("port"));
        assert!(svc
            .update_profile(&view.id, bad("t", "443", "  "))
            .await
            .unwrap_err()
            .contains("identity"));
        // Nothing was saved.
        let store = svc.core.load_store().expect("reload");
        assert_eq!(store.profiles[0].tag, view.tag, "store untouched");
    }

    #[tokio::test]
    async fn diagnostics_mode_creates_file_and_forces_engine_debug() {
        // The sink is process-global: hold the crate's diag test lock for
        // the whole window.
        let _guard = crate::diagnostics::TEST_LOCK.lock().await;
        let _ = crate::diagnostics::disable();
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("diagmode");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("set active");

        // Turn the mode ON through the settings path (the real entry).
        let mut s = svc.settings().await;
        s.diagnostics = true;
        svc.set_settings(s).await.expect("save with diagnostics");
        let diag = svc.diagnostics_path().await.expect("file created");
        assert!(diag.contains("diagnostics-"), "{diag}");
        assert!(std::path::Path::new(&diag).is_file());

        // A connect forces the ENGINE log level to debug for the session.
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        let level = wlock(&svc.core.engine_log_level).clone();
        assert_eq!(level.as_deref(), Some("debug"));

        // GUI log lines (the connected notice) land in the file.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let text = std::fs::read_to_string(&diag).expect("read diag");
        assert!(text.contains("MultiTunnel"), "header present");
        assert!(text.contains("diagnostics mode started"));
        assert!(text.contains("engine log level forced to debug"));
        assert!(text.contains("[GUI]"), "GUI lines tee'd");
        // Redaction: the profile UUID must never appear.
        assert!(
            !text.contains("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"),
            "no credentials in the diagnostics file"
        );
        svc.disconnect().await;

        // Off: the path clears, the file stays.
        let mut s = svc.settings().await;
        s.diagnostics = false;
        svc.set_settings(s).await.expect("save without diagnostics");
        assert!(svc.diagnostics_path().await.is_none());
        assert!(std::path::Path::new(&diag).is_file(), "file kept");
    }

    #[tokio::test]
    async fn disconnect_releases_owned_proxy_state_without_a_session() {
        // S19: after a kill-switch give-up (or any death that left
        // sysproxy_set armed with no session), an explicit disconnect must
        // restore the proxy instead of leaving the black-hole engaged.
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("ksrelease");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let mut events = svc.events();
        // Simulate the post-give-up state: no session, proxy still owned.
        svc.core.sysproxy_set.store(true, Ordering::SeqCst);
        assert!(svc.core.session.lock().unwrap().is_none());
        svc.disconnect().await;
        assert!(!svc.core.sysproxy_set.load(Ordering::SeqCst));
        let mut saw_restore = false;
        while let Ok(ev) = events.try_recv() {
            if let ServiceEvent::LogLine(l) = ev {
                if l.contains("system proxy restored") {
                    saw_restore = true;
                }
            }
        }
        assert!(saw_restore, "the restore is announced in the GUI log");
    }

    /// S19 (review fix): the black-hole OWNERS the port while engaged (a
    /// local process cannot pre-bind it) and stop_blackhole releases it —
    /// the accept loop swallows connections (an open TCP dial to it lands
    /// and closes immediately instead of being answered by anyone else).
    /// On hosts where binding port 9 is not permitted the bind is a
    /// documented no-op fallback (dev/linux root-only port).
    /// S20 (Copilot round 2): the bind and the engage are separate steps —
    /// give_up binds FIRST and only points the proxy at the port once
    /// ownership is proven.
    #[tokio::test]
    async fn blackhole_listener_owns_and_releases_the_port() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("blackhole");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        // Engage (bind first, then install the accept loop — the give_up
        // order).
        if let Some(listener) = bind_blackhole().await {
            engage_blackhole(&svc.core, listener);
            let guard_taken = svc.core.blackhole.lock().unwrap().is_some();
            assert!(guard_taken, "engage records the listener");
            // The port answers to US: a connect lands (accepted + dropped).
            let dial = tokio::net::TcpStream::connect("127.0.0.1:9").await;
            assert!(dial.is_ok(), "our own listener owns the port");
            // Release: the next connect is refused again.
            stop_blackhole(&svc.core);
            tokio::time::sleep(Duration::from_millis(150)).await;
            let refused = tokio::net::TcpStream::connect("127.0.0.1:9").await;
            assert!(refused.is_err(), "port released after stop");
        } else {
            // Documented fallback host (e.g. Linux without CAP_NET_BIND_SERVICE):
            // dead-port semantics — nothing listens, connects are refused.
            let refused = tokio::net::TcpStream::connect("127.0.0.1:9").await;
            assert!(refused.is_err(), "fallback keeps the port dead");
        }
    }

    /// S20 (Copilot round 2, high finding): the connect step-0 self-heal
    /// must CLEAR `sysproxy_set` together with the restore — after a
    /// kill-switch give-up + reconnect, a stale-true flag made a later
    /// no-session disconnect "restore" with no snapshot (which DISABLES
    /// the user's pre-existing proxy instead of leaving it untouched).
    #[tokio::test]
    async fn connect_selfheal_clears_stale_proxy_ownership() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("selfheal");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("active");
        // Simulate the post-give-up state: ownership armed, no session.
        svc.core.sysproxy_set.store(true, Ordering::SeqCst);
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        assert!(
            !svc.core.sysproxy_set.load(Ordering::SeqCst),
            "the step-0 self-heal consumed the snapshot — ownership MUST clear"
        );
        svc.disconnect().await;
        assert!(!svc.core.sysproxy_set.load(Ordering::SeqCst));
    }

    /// S20 (Copilot round 2, medium findings): the edit dialog must reject
    /// port 0 and REALITY material the link parser would refuse — an empty
    /// SNI, a wrong-length/non-base64url public key or an invalid short ID
    /// must not silently reach the Xray config.
    #[tokio::test]
    async fn update_profile_rejects_port_zero_and_invalid_reality() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("editreality");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("active");

        // Port 0 parses as u16 but is not a server port.
        let mut d = svc.profile_details(&view.id).await.expect("details");
        d.port = "0".into();
        let err = svc.update_profile(&view.id, d).await.unwrap_err();
        assert!(err.contains("port 0"), "{err}");
        // Empty SNI.
        let mut d = svc.profile_details(&view.id).await.expect("details");
        d.sni = "   ".into();
        let err = svc.update_profile(&view.id, d).await.unwrap_err();
        assert!(err.contains("sni"), "{err}");
        // Wrong-length public key.
        let mut d = svc.profile_details(&view.id).await.expect("details");
        d.public_key = "short".into();
        let err = svc.update_profile(&view.id, d).await.unwrap_err();
        assert!(err.contains("public key"), "{err}");
        // Non-hex short id.
        let mut d = svc.profile_details(&view.id).await.expect("details");
        d.short_id = "zzzz".into();
        let err = svc.update_profile(&view.id, d).await.unwrap_err();
        assert!(err.contains("short id"), "{err}");
        // A fully valid edit still saves; an EMPTY short id is legal
        // (the parser allows `sid=` empty).
        let mut d = svc.profile_details(&view.id).await.expect("details");
        d.short_id = "".into();
        d.tag = "still fine".into();
        let updated = svc.update_profile(&view.id, d).await.expect("valid edit");
        assert_eq!(updated.tag, "still fine");
        // Nothing from the rejected edits leaked into the store.
        let store = svc.core.load_store().expect("reload");
        assert_eq!(store.profiles[0].tag, "still fine");
        assert_eq!(store.profiles[0].port, 443);
    }

    /// S20 (Copilot round 2, high finding): user-configurable URLs carry
    /// credentials in userinfo/query — the diagnostics header must render
    /// host:port:path only (the file is pasted into chats verbatim).
    #[tokio::test]
    async fn diag_header_redacts_url_credentials() {
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("diagredact");
        write_settings(&paths, exit.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let mut s = svc.settings().await;
        s.exit_probe_url = "http://user:hunter2@probe.example.com:8080/path?token=T0PS3CRET".into();
        s.speed_test_url = "http://speed.example.com/down?key=abc123#frag".into();
        let header = svc.build_diag_header(&s);
        assert!(header.contains("probe.example.com:8080/path"), "{header}");
        assert!(header.contains("speed.example.com:80/down"), "{header}");
        for secret in ["hunter2", "user:", "T0PS3CRET", "abc123", "#frag"] {
            assert!(!header.contains(secret), "leaked {secret:?} in:\n{header}");
        }
        // Unparseable URLs never echo the raw value.
        s.exit_probe_url = "not a url at all".into();
        let header = svc.build_diag_header(&s);
        assert!(header.contains("<redacted (invalid URL)>"), "{header}");
        assert!(!header.contains("not a url"), "{header}");
    }

    /// S20 (Copilot round 2, medium finding): until `\r\n\r\n` is found
    /// nothing counts toward the 64 MiB payload cap — an endpoint flooding
    /// header-less bytes must hit the independent 64 KiB head cap fast
    /// instead of growing the buffer for the whole 30 s budget.
    #[tokio::test]
    async fn speed_test_rejects_a_header_flood() {
        // A server that answers with 256 KiB of header-less bytes and then
        // holds the connection open.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind flood");
        let flood = listener.local_addr().expect("flood addr");
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                    let mut buf = vec![0u8; 1024];
                    let _ = stream.read(&mut buf).await; // drain the request
                    let flood = vec![b'x'; 256 * 1024];
                    let _ = stream.write_all(&flood).await;
                    // Keep the connection open (no EOF, no \r\n\r\n).
                    loop {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                });
            }
        });
        let paths = scratch("flood");
        write_settings(&paths, flood.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", flood.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: flood,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("active");
        let started = Instant::now();
        let err = svc.speed_test(None).await.unwrap_err();
        assert!(err.contains("malformed response"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the head cap must fire long before the 30 s budget"
        );
    }

    /// S20 (Copilot round 2, medium finding): the exit probe and speed test
    /// may only ride the LIVE listeners when the session was built from the
    /// target profile — switching the active profile without reconnecting
    /// must fall back to the temporary tunnel (not report the old tunnel's
    /// exit against the new profile's endpoint).
    #[tokio::test]
    async fn probes_fall_back_when_active_profile_switched() {
        let exit = spawn_exit_echo("198.51.100.7").await;
        let echo = spawn_echo().await;
        let paths = scratch("switched");
        // Probe target = echo (so connect() succeeds); exit URL = exit echo.
        write_settings(&paths, echo.port(), false);
        let mut file = SettingsFile::load(&paths.settings_file());
        file.settings.exit_probe_url = format!("http://127.0.0.1:{}/json", exit.port());
        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", exit.port());
        file.save(&paths.settings_file()).expect("save settings");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let a = svc.import_link(VLESS_REALITY).await.expect("import A");
        let b = svc
            .import_link(
                &VLESS_REALITY
                    .replace("203.0.113.10", "203.0.113.11")
                    .replace("synthetic-reality", "second-server"),
            )
            .await
            .expect("import B");
        svc.set_active_profile(&a.tag).await.expect("active A");
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        // Matching session → the probe rides the live listeners.
        let live = svc.exit_probe().await.expect("live probe");
        assert!(!live.via_temp_tunnel, "session matches the active profile");
        // Switch the active profile WITHOUT reconnecting → both probes
        // must report the temporary-tunnel path.
        svc.set_active_profile(&b.tag).await.expect("active B");
        let temp = svc.exit_probe().await.expect("temp probe");
        assert!(
            temp.via_temp_tunnel,
            "stale session must not pose as B's tunnel"
        );
        let speed = svc
            .speed_test(Some(b.id.clone()))
            .await
            .expect("temp speed");
        assert!(speed.via_temp_tunnel, "speed test ditto");
        // And an explicit target that DOES match the session still rides it.
        let live_speed = svc
            .speed_test(Some(a.id.clone()))
            .await
            .expect("live speed");
        assert!(!live_speed.via_temp_tunnel);
        svc.disconnect().await;
    }

    /// S20 (Copilot round 2, medium finding): `engine_specs_for` preserves
    /// the user's bonded-adapter priority order — the one-shot probe link
    /// must be the FIRST (top-priority), not whatever `pop()` produced.
    #[tokio::test]
    async fn temp_link_uses_the_top_priority_first_link() {
        let first_exit = spawn_exit_echo("203.0.113.7").await;
        let second_exit = spawn_exit_echo("203.0.113.9").await;
        let paths = scratch("templink");
        write_settings(&paths, first_exit.port(), false);
        struct TwoLinkFactory {
            first: SocketAddr,
            second: SocketAddr,
        }
        #[async_trait::async_trait]
        impl LinkFactory for TwoLinkFactory {
            async fn build(
                &self,
                _profile: &Profile,
                _bonded: &[BondedAdapter],
            ) -> anyhow::Result<Vec<Arc<dyn Outbound>>> {
                Ok(vec![
                    Arc::new(FakeLinkNamed {
                        name: "first [Wi-Fi]".into(),
                        echo: self.first,
                    }) as Arc<dyn Outbound>,
                    Arc::new(FakeLinkNamed {
                        name: "second [LTE]".into(),
                        echo: self.second,
                    }),
                ])
            }
        }
        let svc = RealService::with(
            paths,
            Arc::new(TwoLinkFactory {
                first: first_exit,
                second: second_exit,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        let store = svc.core.load_store().expect("store");
        let profile = store
            .profiles
            .iter()
            .find(|p| p.id == view.id)
            .cloned()
            .expect("profile");
        let link = temp_link_for(&svc.core, &profile)
            .await
            .expect("one-shot link");
        assert_eq!(
            link.name(),
            "first [Wi-Fi]",
            "the FIRST link carries the probe"
        );
        // Prove it end-to-end: the chosen link dials the FIRST exit echo.
        let target = TargetAddr {
            host: Host::Domain("irrelevant.example".into()),
            port: 80,
        };
        let mut stream = link.connect(&target).await.expect("connect");
        let body = http_get(&mut stream, "irrelevant.example", "/", EXIT_PROBE_MAX_BYTES)
            .await
            .expect("http get");
        let exit = parse_exit_info(&body).expect("identity");
        assert_eq!(exit.exit_ip, "203.0.113.7", "rode the top-priority link");
    }

    /// S20 (Copilot round 2, medium finding): a failed `diagnostics::enable`
    /// (e.g. the log dir is unwritable) must REVERT the persisted toggle —
    /// the UI must never show the mode enabled with no file being written
    /// and no retry path.
    #[tokio::test]
    async fn diagnostics_enable_failure_reverts_the_toggle() {
        let _guard = crate::diagnostics::TEST_LOCK.lock().await;
        let _ = crate::diagnostics::disable();
        let exit = spawn_exit_echo("127.0.0.1").await;
        let paths = scratch("diagfail");
        write_settings(&paths, exit.port(), false);
        // Make the log dir unusable: a FILE where the dir should be.
        std::fs::create_dir_all(&paths.config_dir).expect("base");
        std::fs::write(&paths.log_dir, b"not a directory").expect("block log dir");
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let mut events = svc.events();
        let mut s = svc.settings().await;
        s.diagnostics = true;
        svc.set_settings(s).await.expect("settings still save");
        // The persisted toggle is back OFF.
        let reloaded = SettingsFile::load(&svc.core.paths.settings_file());
        assert!(!reloaded.settings.diagnostics, "toggle reverted on failure");
        assert!(svc.diagnostics_path().await.is_none());
        let mut saw_revert = false;
        while let Ok(ev) = events.try_recv() {
            if let ServiceEvent::LogLine(l) = ev {
                if l.contains("could not start") && l.contains("reverted") {
                    saw_revert = true;
                }
            }
        }
        assert!(
            saw_revert,
            "the failure + revert is announced in the GUI log"
        );
    }

    /// S20 (Copilot round 2, medium finding): engines only read the debug
    /// log level at SPAWN time — enabling diagnostics on a LIVE session
    /// must reconnect, or the file silently misses the engine debug output
    /// the UI promises.
    #[tokio::test]
    async fn diagnostics_enable_while_connected_reconnects() {
        let _guard = crate::diagnostics::TEST_LOCK.lock().await;
        let _ = crate::diagnostics::disable();
        let exit = spawn_exit_echo("127.0.0.1").await;
        let echo = spawn_echo().await;
        let paths = scratch("diaglive");
        write_settings(&paths, echo.port(), false);
        let svc = RealService::with(
            paths,
            Arc::new(FakeFactory {
                echo: exit,
                fail: false,
            }),
        );
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("active");
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);
        let restarts_before = svc.core.restarts.load(Ordering::SeqCst);

        let mut s = svc.settings().await;
        s.diagnostics = true;
        svc.set_settings(s).await.expect("enable diagnostics");

        // The session was rebuilt at debug level.
        assert_eq!(
            svc.status().await.state,
            ServiceState::Connected,
            "the reconnect restores the session"
        );
        assert!(
            svc.core.restarts.load(Ordering::SeqCst) > restarts_before,
            "the live session reconnected"
        );
        let level = wlock(&svc.core.engine_log_level).clone();
        assert_eq!(level.as_deref(), Some("debug"));
        assert!(svc.diagnostics_path().await.is_some());
        svc.disconnect().await;
        let mut s = svc.settings().await;
        s.diagnostics = false;
        svc.set_settings(s).await.expect("disable diagnostics");
        assert!(svc.diagnostics_path().await.is_none());
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

    /// S21 P0 regression (user report: "it stops working when a second
    /// connection is added — the application must be restarted").
    ///
    /// Repro: connect, hold ONE live client connection open (a browser
    /// keep-alive through the proxy), then run the exact Status-picker
    /// switch sequence (import second profile → set_active_profile →
    /// disconnect → connect, as `StatusServerPicked` does).
    ///
    /// Platform note: on WINDOWS the lingering connection's socket pins the
    /// 10808 local endpoint and the re-bind fails `WSAEADDRINUSE`, landing
    /// `Failed` — dead tunnel until restart (Linux tolerates a listen re-
    /// bind over non-listening sockets, which is why this sandbox alone
    /// cannot reproduce the bind refusal). The platform-independent
    /// observable of the SAME defect is asserted here: the lingering
    /// client connection MUST be force-closed at the end of the drain
    /// grace (EOF on the client end). Pre-fix the connection task was
    /// never aborted and the client socket stayed ESTABLISHED forever.
    #[tokio::test]
    async fn picker_switch_with_lingering_client_reconnects() {
        use std::time::Duration;
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let echo = spawn_echo().await;
        let paths = scratch("picker-switch");
        write_settings(&paths, echo.port(), false);
        let svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));

        let a = svc.import_link(VLESS_REALITY).await.expect("import A");
        svc.set_active_profile(&a.tag).await.expect("activate A");
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);

        // The "browser": one long-lived connection through the proxy, held
        // OPEN across the whole switch (keep-alive; we never close it).
        // A reader parks on it and reports when the connection ENDS.
        let target = TargetAddr {
            host: Host::Domain("idle.example".to_string()),
            port: echo.port(),
        };
        let (socks, _) = svc.session_addrs();
        let held_client = Socks5Outbound::new(socks.expect("socks bound"));
        let held = held_client
            .connect(&target)
            .await
            .expect("open held connection");
        let (eof_tx, eof_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut s = held;
            let mut buf = [0u8; 64];
            loop {
                match s.read(&mut buf).await {
                    Ok(0) | Err(_) => {
                        let _ = eof_tx.send(());
                        break;
                    }
                    Ok(_) => continue,
                }
            }
        });

        // The user adds a SECOND server (same #fragment tag — also the H2
        // shape) and picks it in the Status picker while connected.
        let b_link = VLESS_REALITY.replace("@203.0.113.10:", "@203.0.113.11:");
        let b = svc.import_link(&b_link).await.expect("import B");
        assert_ne!(a.id, b.id, "two distinct profiles exist");
        svc.set_active_profile(&b.tag).await.expect("activate B");
        svc.disconnect().await;
        svc.connect().await;

        let snap = svc.status().await;
        assert_eq!(
            snap.state,
            ServiceState::Connected,
            "reconnect with a lingering client connection must succeed (P0 fix)"
        );

        // The P0 mechanism observable: the lingering connection was
        // force-closed when the drain grace expired (the client sees EOF).
        // Pre-fix nothing ever aborted the connection task and this read
        // stayed pending forever.
        let closed = tokio::time::timeout(Duration::from_secs(5), eof_rx).await;
        assert!(
            matches!(closed, Ok(Ok(()))),
            "lingering client connection must be force-closed at the end of the drain grace"
        );

        // And the data plane serves NEW connections again.
        let (socks, _) = svc.session_addrs();
        let client = Socks5Outbound::new(socks.expect("socks re-bound"));
        let mut stream = client.connect(&target).await.expect("new socks connect");
        stream.write_all(b"ping").await.expect("write");
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.expect("echo read");
        assert_eq!(&buf, b"ping");
    }

    /// S21 (feature o): the QoS toggle persists, flips the LIVE session's
    /// scheduler WITHOUT a reconnect, and is visible to a fresh service
    /// instance (settings.json round-trip).
    #[tokio::test]
    async fn qos_toggle_flips_live_without_reconnect_and_persists() {
        let echo = spawn_echo().await;
        let paths = scratch("qos");
        write_settings(&paths, echo.port(), false);
        let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
        let view = svc.import_link(VLESS_REALITY).await.expect("import");
        svc.set_active_profile(&view.tag).await.expect("activate");
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connected);

        // Flip ON: persisted, live scheduler enabled, session untouched.
        let mut settings = svc.settings().await;
        settings.qos_fair = true;
        svc.set_settings(settings).await.expect("save");
        assert!(svc.settings().await.qos_fair, "persisted");
        let sched = lock(&svc.core.session)
            .as_ref()
            .and_then(|s| s.server.qos())
            .expect("the live session carries the scheduler");
        assert!(sched.enabled(), "live flip applies without a rebuild");
        assert_eq!(
            svc.status().await.state,
            ServiceState::Connected,
            "the QoS toggle must NOT reconnect"
        );

        // Flip OFF: live disable.
        let mut settings = svc.settings().await;
        settings.qos_fair = false;
        svc.set_settings(settings).await.expect("save");
        assert!(!svc.settings().await.qos_fair);
        assert!(!sched.enabled(), "live disable applies");

        // Flip ON again and prove persistence across instances.
        let mut settings = svc.settings().await;
        settings.qos_fair = true;
        svc.set_settings(settings).await.expect("save");
        let svc2 = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
        assert!(svc2.settings().await.qos_fair, "survives a restart");
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
            kill_switch: false,
            diagnostics: false, // never true here: the sink is process-global
            speed_test_url: "http://speed.cloudflare.com/__down?bytes=25000000".to_string(),
            speed_test_upload_url: "http://speed.cloudflare.com/__up".to_string(),
            qos_fair: true, // round-trips a non-default value
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
