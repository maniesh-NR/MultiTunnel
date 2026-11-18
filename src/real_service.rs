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

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mt_links::{parse_share_link, Profile, ProfileStore, Protocol, Security, Transport};
use mt_net::{
    Action, Host, MergePolicy, Outbound, ProxyEvent, ProxyServer, Router, Rule, RuleMatcher,
    TargetAddr,
};
use mt_service::{
    LinkStatus, MtService, ProfileView, RuleView, ServiceEvent, ServiceState, SettingsView,
    StatusSnapshot,
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
/// [`ManagedEngine`]; tests inject fakes. `Err` messages must be
/// credential-free (they become UI error strings).
#[async_trait::async_trait]
pub trait LinkFactory: Send + Sync {
    async fn build(&self, profile: &Profile) -> anyhow::Result<Vec<Arc<dyn Outbound>>>;
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
        }
    }
}

#[async_trait::async_trait]
impl LinkFactory for EngineLinkFactory {
    async fn build(&self, profile: &Profile) -> anyhow::Result<Vec<Arc<dyn Outbound>>> {
        let endpoint = endpoint_from_profile(profile).map_err(anyhow::Error::msg)?;
        let spec = mt_engine::EngineSpec {
            tag: profile.tag.clone(),
            endpoint,
        };
        let engine = mt_engine::ManagedEngine::new(
            spec,
            Arc::clone(&self.locator),
            self.state_dir.clone(),
            self.log_dir.clone(),
        );
        Ok(vec![Arc::new(engine)])
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

/// Lenient `merge_policy` string → policy (unknown values fall back to
/// `Failover` with a warning; the GUI sends the canonical lowercase names,
/// which are also mt-net's own serde names).
pub fn merge_policy_from_str(s: &str) -> MergePolicy {
    match s {
        "roundrobin" => MergePolicy::RoundRobin,
        "lowestlatency" => MergePolicy::LowestLatency,
        "failover" => MergePolicy::Failover,
        other => {
            tracing::warn!("unknown merge policy {other:?}; using failover");
            MergePolicy::Failover
        }
    }
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
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            settings: SettingsView {
                // Default seed: the Material baseline primary #6750A4.
                theme_seed: 0x6750A4,
                dark_mode: false,
                system_proxy: false,
                merge_policy: "failover".to_string(),
                autostart: false,
            },
            // Stock rules = the LAN/private bypass (S6-REV-C P1-4): a fresh
            // settings.json must carry private-CIDR → direct rules, mirroring
            // v0.2's `geoip:private → direct` invariant, so router admin
            // pages / printers / NAS never ride the tunnel.
            rules: default_lan_rules(),
            socks_port: DEFAULT_SOCKS_PORT,
            http_port: DEFAULT_HTTP_PORT,
            probe_target: DEFAULT_PROBE_TARGET.to_string(),
        }
    }
}

impl SettingsFile {
    fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                tracing::warn!("settings.json unreadable ({e}); using defaults");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
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
        let factory: Arc<dyn LinkFactory> = Arc::new(EngineLinkFactory::new(
            Arc::new(mt_platform::DefaultEngineLocator::new()),
            paths.state_dir.clone(),
            paths.log_dir.clone(),
        ));
        Self::with(paths, factory)
    }

    /// Injected paths + link factory (tests, embedders).
    pub fn with(paths: ServicePaths, factory: Arc<dyn LinkFactory>) -> Self {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Self {
            core: Arc::new(Core {
                paths,
                factory,
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

        let links = match self.core.factory.build(&profile).await {
            Ok(l) if !l.is_empty() => l,
            Ok(_) => {
                Self::fail_connect(&self.core, "link factory produced no links".into()).await;
                return;
            }
            Err(e) => {
                Self::fail_connect(&self.core, format!("{}", e)).await;
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
        let socks = SocketAddr::from(([127, 0, 0, 1], settings.socks_port));
        let http = SocketAddr::from(([127, 0, 0, 1], settings.http_port));
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
        let (up_since, server) = {
            let guard = lock(&self.core.session);
            match guard.as_ref() {
                Some(s) => (Some(fmt_utc(s.started_at)), Some(Arc::clone(&s.server))),
                None => (None, None),
            }
        }; // guard dropped before the await below (never hold a std lock
           // across .await — real_service decision #13)
        let links = match server {
            Some(server) => server
                .health_snapshot()
                .await
                .iter()
                .map(|(name, h)| link_status_of(&self.core, name, Some(h)))
                .collect(),
            None => Vec::new(),
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
        let mut links = self
            .core
            .factory
            .build(&profile)
            .await
            .map_err(|e| e.to_string())?;
        let Some(link) = links.pop() else {
            return Err("link factory produced no links".to_string());
        };
        let settings = SettingsFile::load(&self.core.paths.settings_file());
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
        file.settings = settings.clone();
        file.save(&self.core.paths.settings_file())
            .map_err(|e| e.to_string())?;
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
        async fn build(&self, _profile: &Profile) -> anyhow::Result<Vec<Arc<dyn Outbound>>> {
            if self.fail {
                anyhow::bail!("no engine available (synthetic factory failure)");
            }
            Ok(vec![Arc::new(FakeLink {
                name: "fake-0",
                echo: self.echo,
            })])
        }
    }

    /// settings.json with ephemeral local ports and a loopback probe target.
    fn write_settings(paths: &ServicePaths, echo_port: u16, system_proxy: bool) {
        let file = SettingsFile {
            settings: SettingsView {
                theme_seed: 0x6750A4,
                dark_mode: false,
                system_proxy,
                merge_policy: "failover".to_string(),
                autostart: false,
            },
            rules: Vec::new(),
            socks_port: 0,
            http_port: 0,
            probe_target: format!("127.0.0.1:{echo_port}"),
        };
        std::fs::create_dir_all(&paths.config_dir).expect("config dir");
        file.save(&paths.settings_file()).expect("save settings");
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
            system_proxy: false, // never true in tests: autostart/sysproxy touch the host
            merge_policy: "lowestlatency".to_string(),
            autostart: false,
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
}
