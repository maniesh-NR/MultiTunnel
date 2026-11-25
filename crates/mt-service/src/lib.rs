//! mt-service — the UI service contract.
//!
//! Written by the orchestrator (complete, no expert fills this in): the
//! types and trait the GUI (mt-app) codes against. The root crate provides
//! `RealService` (ProfileStore + mt-net::ProxyServer + mt-engine +
//! mt-platform + legacy sysproxy); mt-app ships a `MockService` for
//! standalone development. All payloads are UI-shaped (Strings, millis) —
//! this crate depends on no other mt-* crate by design.
//!
//! S7 (adapter bonding): [`AdapterView`] + the `adapters` /
//! `set_bonded_adapters` trait methods + `SettingsView::bonded_adapters`.
//! `bonded_adapters` holds adapter IDS (not display names): on Windows the
//! stable adapter GUID, on dev hosts the interface name. Empty = no
//! pinning (one link, the OS routing table decides — v0.3 behavior).

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Connection lifecycle state (mirrors the root supervisor's State).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

/// Point-in-time status for the Status screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub state: ServiceState,
    pub active_profile: Option<String>,
    pub up_since: Option<String>,
    pub restarts: u32,
    pub links: Vec<LinkStatus>,
}

/// One aggregated link's live status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkStatus {
    pub name: String,
    /// S22: the local adapter this link rides — parsed from the engine
    /// link name's `"<tag> [<adapter>]"` suffix (the same grammar the
    /// GUI's per-adapter rate sampler has always matched; the dedup
    /// form `"[Ethernet #2]"` keeps the adapter name without the
    /// ordinal). None when the name carries no adapter suffix (e.g. a
    /// single unbonded link).
    #[serde(default)]
    pub adapter_name: Option<String>,
    pub ok: bool,
    pub latency_ms: Option<u32>,
    pub ewma_ms: Option<f32>,
    pub quarantined: bool,
    pub bytes_up: u64,
    pub bytes_down: u64,
}

/// S22: the adapter a link rides, from the `"<tag> [<adapter>]"` link
/// name grammar (shared by the core's status builder, the mock seeds
/// and the GUI's rate sampler).
pub fn adapter_of_link_name(link_name: &str) -> Option<String> {
    let (_, rest) = link_name.rsplit_once(" [")?;
    let bracketed = rest.strip_suffix(']')?;
    let bracketed = bracketed.trim();
    if bracketed.is_empty() {
        return None;
    }
    // The dedup ordinal ("Wi-Fi #2") belongs to the matcher, not to the
    // displayed adapter — strip " #<n>" for display.
    match bracketed.rsplit_once(" #") {
        Some((base, ordinal))
            if ordinal.chars().all(|c| c.is_ascii_digit()) && !base.is_empty() =>
        {
            Some(base.to_string())
        }
        _ => Some(bracketed.to_string()),
    }
}

/// A profile as the GUI sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileView {
    pub id: String,
    pub tag: String,
    pub protocol: String,
    pub endpoint: String,
    pub remark: String,
    pub active: bool,
}

/// One network adapter as the GUI's Adapters screen sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterView {
    /// Stable id — matches entries in
    /// [`SettingsView::bonded_adapters`].
    pub id: String,
    /// Friendly display name ("Wi-Fi", "Ethernet").
    pub name: String,
    /// Hardware/driver description ("Intel(R) Wi-Fi 6 AX201").
    pub description: String,
    /// Unicast IPv4 addresses (display; the service binds the engine to
    /// the first one when the adapter is bonded).
    pub ipv4: Vec<String>,
    /// OS reports the interface operationally up.
    pub up: bool,
    /// Currently selected for bonding (derived from settings by the
    /// service; a pure convenience for the GUI draft state).
    pub selected: bool,
}

/// A routing rule as the GUI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleView {
    pub kind: String,
    pub value: String,
    pub action: String,
}

/// App settings as the GUI sees them.
///
/// S9 advanced fields carry `#[serde(default)]` so settings.json written
/// by v0.3/v0.4 loads unchanged; the RealService mirrors the legacy
/// top-level `socksPort`/`httpPort`/`probeTarget` into the view on load
/// (top-level wins — it is what v0.4 actually used at runtime).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsView {
    pub theme_seed: u32,
    pub dark_mode: bool,
    /// S18: how the app resolves light vs dark — "system" | "light" |
    /// "dark". Absent in files written by v0.11 and older → the legacy
    /// [`SettingsView::dark_mode`] bool decides (true → Dark, false →
    /// Light); see [`SettingsView::theme_mode`]. `dark_mode` itself stays
    /// serialized (the GUI keeps it at the EFFECTIVE state on every save)
    /// so a v0.11-or-older binary still reads future files.
    #[serde(default)]
    pub theme_mode: String,
    pub system_proxy: bool,
    pub merge_policy: String,
    pub autostart: bool,
    /// Adapter ids to bond the tunnel across (S7). `#[serde(default)]` so
    /// settings.json written by v0.3 loads unchanged (empty = one link,
    /// no adapter pinning).
    #[serde(default)]
    pub bonded_adapters: Vec<String>,
    /// S9: how the bonded adapters carry traffic — "per-connection"
    /// (S8: whole TCP connections spread across links) or "stream"
    /// (bytes of every connection split across carriers, merged by the
    /// mt-merge-server daemon on the tunnel server; falls back to
    /// per-connection when the daemon is unreachable).
    #[serde(default = "default_bond_mode")]
    pub bond_mode: String,
    /// S9: the merge-daemon port ON THE SERVER (stream mode).
    #[serde(default = "default_merge_port")]
    pub merge_port: u16,
    /// S9 (v2rayN parity): local SOCKS5 listener port.
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    /// S9 (v2rayN parity): local HTTP proxy listener port.
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    /// S9 (v2rayN parity): bind listeners on 0.0.0.0 instead of loopback
    /// (exposes the proxy to the LAN — opt-in, off by default).
    #[serde(default)]
    pub allow_lan: bool,
    /// S9: engine/core log level — "error" | "warning" | "info" | "debug".
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// S9 (v2rayN parity): latency-test target, "host:port".
    #[serde(default = "default_probe_target")]
    pub probe_target: String,
    /// S9: exit-IP probe URL (plain HTTP inside the tunnel, D18).
    #[serde(default = "default_exit_probe_url")]
    pub exit_probe_url: String,
    /// S15: closing the window keeps the app running in the tray
    /// (Windows; elsewhere the window minimizes instead). Absent in
    /// older files → false, i.e. close really closes (D8 version-free
    /// migration by serde default, D20 precedent).
    #[serde(default)]
    pub close_to_tray: bool,
    /// S16: keep the window above all normal windows (titlebar
    /// right-click menu + Settings → Window & tray). Absent in older
    /// files → false (same D8 version-free migration).
    #[serde(default)]
    pub always_on_top: bool,
    /// S19: fail-closed on spontaneous tunnel death — when the tunnel
    /// dies on its own while the app owns the system proxy, the proxy is
    /// repointed at a dead local port (traffic FAILS) instead of being
    /// restored (traffic would silently go direct). A manual disconnect
    /// always restores. Absent in files written by v0.12 and older →
    /// false (D8 version-free migration by serde default).
    #[serde(default)]
    pub kill_switch: bool,
    /// S19: diagnostics mode — while on, the service writes one
    /// self-contained log file (connection lifecycle, engine output,
    /// link health; credentials redacted) the user can paste into a
    /// support chat. Absent → false (D8).
    #[serde(default)]
    pub diagnostics: bool,
    /// S19: speed-test download URL (Status screen). Plain `http://` by
    /// design — the client→server leg rides the REALITY tunnel, so the
    /// payload needs no TLS of its own (D18's zero-TLS rule).
    #[serde(default = "default_speed_test_url")]
    pub speed_test_url: String,
    /// S22: speed-test UPLOAD URL (the "Test upload speed" button that
    /// appears after the download test concludes). Plain `http://` by
    /// design — same zero-TLS rule as `speed_test_url` (the payload
    /// rides the REALITY tunnel). Absent in older files → the stock
    /// Cloudflare `__up` endpoint (D8 version-free migration by serde
    /// default).
    #[serde(default = "default_speed_test_upload_url")]
    pub speed_test_upload_url: String,
    /// S21 (feature o): fair-share bandwidth scheduler ("QoS") — when ON,
    /// the proxy groups flows by TARGET and, under contention (≥2 groups
    /// racing for an aggregate above the floor), caps each group at its
    /// `aggregate / groups` share, so no single application/site can
    /// dominate all the bandwidth. Flips live (no reconnect). Absent in
    /// older files → false (D8 version-free migration by serde default).
    #[serde(default)]
    pub qos_fair: bool,
}

/// S18: how the app resolves light vs dark appearance. Serialized as
/// the lowercase string in [`SettingsView::theme_mode`] (the existing
/// merge_policy String convention; the GUI owns the parse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the OS preference (re-polled while the app runs).
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl ThemeMode {
    /// The serialized form (also the pick_list value).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Parse a stored `theme_mode` string. Unknown/absent values fall
    /// back to the LEGACY `dark_mode` bool (files written by v0.11 and
    /// older carry no `theme_mode` key).
    pub fn parse(s: &str, legacy_dark_mode: bool) -> Self {
        match s {
            "system" => Self::System,
            "light" => Self::Light,
            "dark" => Self::Dark,
            // Absent (serde default "") or garbage: keep what the legacy
            // bool said so an upgrade never flips the user's theme.
            _ if legacy_dark_mode => Self::Dark,
            _ => Self::Light,
        }
    }

    /// The effective dark flag given the OS preference (`System` mode).
    #[must_use]
    pub fn resolves_dark(self, system_prefers_dark: bool) -> bool {
        match self {
            Self::System => system_prefers_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

impl SettingsView {
    /// S18: the parsed appearance mode (legacy `dark_mode` decides when
    /// the stored string is absent/invalid).
    pub fn theme_mode(&self) -> ThemeMode {
        ThemeMode::parse(&self.theme_mode, self.dark_mode)
    }

    /// S18: the dark flag the GUI should apply right now.
    #[must_use]
    pub fn effective_dark(&self, system_prefers_dark: bool) -> bool {
        self.theme_mode().resolves_dark(system_prefers_dark)
    }
}

fn default_bond_mode() -> String {
    "per-connection".to_string()
}
fn default_merge_port() -> u16 {
    7443
}
fn default_socks_port() -> u16 {
    10808
}
fn default_http_port() -> u16 {
    10809
}
fn default_log_level() -> String {
    "warning".to_string()
}
fn default_probe_target() -> String {
    "connectivitycheck.gstatic.com:80".to_string()
}
fn default_exit_probe_url() -> String {
    "http://ip-api.com/json".to_string()
}
fn default_speed_test_url() -> String {
    "http://speed.cloudflare.com/__down?bytes=25000000".to_string()
}
fn default_speed_test_upload_url() -> String {
    "http://speed.cloudflare.com/__up".to_string()
}

/// Live events pushed to the GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceEvent {
    StateChanged(ServiceState),
    LinkUpdated(LinkStatus),
    Throughput { up_bps: u64, down_bps: u64 },
    Latency { profile_id: String, ms: Option<u32> },
    LogLine(String),
}

/// The service the GUI talks to. Implementations: root crate's
/// `RealService`, mt-app's `MockService`.
#[async_trait::async_trait]
pub trait MtService: Send + Sync {
    async fn status(&self) -> StatusSnapshot;
    async fn connect(&self);
    async fn disconnect(&self);

    async fn profiles(&self) -> Vec<ProfileView>;
    /// Parse + store a share link; errors are user-facing Strings that
    /// never contain credential values.
    async fn import_link(&self, link: &str) -> Result<ProfileView, String>;
    async fn remove_profile(&self, id: &str) -> Result<(), String>;
    async fn set_active_profile(&self, id: &str) -> Result<(), String>;
    async fn test_latency(&self, id: &str) -> Result<u32, String>;

    /// S19: the full editable material of one stored profile (the Edit
    /// dialog's prefill). Errors are user-facing.
    async fn profile_details(&self, id: &str) -> Result<ProfileDetails, String>;

    /// S19: apply edits to a stored profile. The id and the active
    /// selection are preserved; a live session keeps running on the old
    /// links until the next reconnect (the service logs a reminder when
    /// the edited profile is the active one).
    async fn update_profile(&self, id: &str, edit: ProfileDetails) -> Result<ProfileView, String>;

    /// S19: download a speed-test payload through the tunnel and report
    /// throughput — through the LIVE session when the target is the
    /// active profile, through a temporary probe tunnel otherwise.
    /// `None` targets the active profile.
    async fn speed_test(&self, id: Option<String>) -> Result<SpeedTestInfo, String>;

    /// S22: push a fixed-size payload UP through the tunnel and report
    /// the send rate — same profile/tunnel selection and caps as
    /// [`Self::speed_test`] (live session for the active profile, a
    /// temporary probe tunnel otherwise). `None` targets the active
    /// profile.
    async fn speed_test_upload(&self, id: Option<String>) -> Result<SpeedTestInfo, String>;

    async fn routing_rules(&self) -> Vec<RuleView>;
    async fn set_routing_rules(&self, rules: Vec<RuleView>) -> Result<(), String>;

    async fn settings(&self) -> SettingsView;
    async fn set_settings(&self, settings: SettingsView) -> Result<(), String>;

    /// Enumerate the machine's network adapters with their current
    /// bonding selection (never errors — enumeration fails open to an
    /// empty list, like the rest of the contract's read paths).
    async fn adapters(&self) -> Vec<AdapterView>;

    /// Replace the bonded-adapter selection (ids from [`Self::adapters`]).
    /// Persists immediately; a live session is restarted so the new link
    /// set takes effect. Unknown ids are an `Err` naming the id.
    async fn set_bonded_adapters(&self, ids: Vec<String>) -> Result<(), String>;

    /// Subscribe to live events (state, links, throughput, logs).
    fn events(&self) -> broadcast::Receiver<ServiceEvent>;

    /// Probe the tunnel's EXIT identity (Status screen, D18): rides the
    /// LIVE session when one is up; S19 falls back to a temporary probe
    /// tunnel through the active profile when the app is disconnected
    /// (same exit identity, no session left behind).
    async fn exit_probe(&self) -> Result<ExitInfo, String>;

    /// S19: the diagnostics log file currently being written (Settings
    /// screen), if diagnostics mode is on. The file stays on disk after
    /// the mode is turned off.
    async fn diagnostics_path(&self) -> Option<String>;

    /// S22: the log directory — ALWAYS resolvable (not gated on
    /// diagnostics mode), so the Settings "Open log folder" button works
    /// in every state (mode off, after a restart, mock builds).
    async fn log_dir(&self) -> String;
}

/// Result of the Status-screen exit probe (D18): which IP the tunnel
/// EXITS from, as the internet sees it, plus the profile endpoint for the
/// match/mismatch verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitInfo {
    /// The measured exit IP (the echo service's "query" field).
    pub exit_ip: String,
    /// Geolocation reported for the exit IP ("" when unknown).
    pub country: String,
    pub city: String,
    /// ISP / organization of the exit IP ("" when unknown).
    pub isp: String,
    /// Autonomous-system label of the exit ("" when unknown).
    pub asn: String,
    /// The ACTIVE profile's server endpoint ("host:port") for comparison.
    pub server_endpoint: String,
    /// Human verdict: true when the exit IS the profile server (single
    /// hop); false means an extra egress hop exists (e.g. a VPN on the
    /// server VM's default route).
    pub direct_exit: bool,
    /// S19: true when the probe rode a TEMPORARY tunnel spawned for the
    /// check (the app was disconnected); false when it rode the live
    /// session. Same exit identity either way — the flag only tells the
    /// GUI which caption to show.
    #[serde(default)]
    pub via_temp_tunnel: bool,
}

/// S19: the full editable material of one server profile (Servers screen
/// → Edit dialog). `profile_details` produces the prefill; `update_profile`
/// consumes the edited copy. Carries credential fields by necessity — it
/// exists only for the one GUI edit round-trip and is never logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileDetails {
    pub id: String,
    /// Display name (derived from the link on import; freely editable).
    pub tag: String,
    /// Free-form note shown under the name ("" = hidden).
    pub remark: String,
    /// Protocol label — "vless" | "vmess" | "trojan" | "ss" (read-only:
    /// re-protocoling is a re-import, not an edit).
    pub protocol: String,
    /// Server host (domain or IP literal).
    pub address: String,
    /// Server port (decimal string — the dialog is a text field).
    pub port: String,
    /// UUID (vless/vmess) or password (trojan/ss).
    pub identity: String,
    /// Security label — "reality" | "tls" | "none" (read-only).
    pub security: String,
    /// REALITY SNI ("" for non-reality profiles).
    pub sni: String,
    /// REALITY uTLS fingerprint, e.g. "chrome" ("" for non-reality).
    pub fingerprint: String,
    /// REALITY public key ("" for non-reality).
    pub public_key: String,
    /// REALITY short ID ("" for non-reality).
    pub short_id: String,
    /// REALITY spiderX path ("" = default "/" on save).
    pub spider_x: String,
}

/// S19: one speed-test run's outcome (Status screen → Speed test card).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeedTestInfo {
    /// Measured download throughput, megabits per second.
    pub mbps: f64,
    /// Megabytes actually received (1 MiB units, one decimal of
    /// precision in the GUI).
    pub mbytes: f64,
    /// Wall-clock seconds of the measured transfer (excludes tunnel
    /// setup — the clock starts once the stream is connected).
    pub seconds: f64,
    /// The profile that was measured (tag).
    pub profile_tag: String,
    /// true = measured through a temporary probe tunnel (the app was
    /// disconnected / another profile was picked); false = through the
    /// live session.
    pub via_temp_tunnel: bool,
}

// ---------------------------------------------------------------------------
// S10: exclusions import (v2rayN parity)
// ---------------------------------------------------------------------------

/// Outcome of parsing an exclusions file (Routing screen → "Import from
/// file"). Pure — file I/O is the caller's business.
#[derive(Debug, Clone, PartialEq)]
pub struct ExclusionImport {
    /// Parsed rules in file order. A plain list maps every entry to
    /// `action = "direct"` (that is what "exclude from the tunnel" means);
    /// a v2rayN routing JSON keeps each rule's own outbound
    /// (direct/freedom → "direct", proxy → "proxy", block/blackhole →
    /// "reject").
    pub rules: Vec<RuleView>,
    /// Entries that could not be mapped, each with a human reason.
    /// Never fatal: the import proceeds with everything it understood.
    pub skipped: Vec<(String, String)>,
    /// Human name of the detected format.
    pub format: &'static str,
}

impl ExclusionImport {
    /// One-line summary for the Routing screen status field.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "imported {} rule{} from {}",
            self.rules.len(),
            if self.rules.len() == 1 { "" } else { "s" },
            self.format
        );
        if !self.skipped.is_empty() {
            s.push_str(&format!(
                ", skipped {} (see tooltips / logs)",
                self.skipped.len()
            ));
        }
        s
    }
}

/// Parse an exclusions file: either a plain text list (one domain / IP /
/// CIDR per line, `#`/`//`/`;` comments, every entry becomes a DIRECT
/// rule) or a v2rayN/Xray routing-rules JSON (`{"routing":{"rules":[..]}}`,
/// a bare `{"rules":[..]}`, or a bare `[..]` array — entries keep their
/// outbound tags where they map to ours).
///
/// Mapping notes (documented, honest):
/// - `example.com` (or `.example.com`) → domainSuffix — matches the domain
///   itself AND all subdomains (label-boundary semantics).
/// - `full:example.com` in JSON → domainSuffix (WIDENED to subdomains; we
///   have no exact-match matcher and widening is the safe direction for
///   an exclusion list).
/// - a bare IP → `ip/32` (v4) or `ip/128` (v6).
/// - `geosite:x`, `geoip:x`, `regexp:x` are NOT supported (no geo
///   databases in a zero-dependency build) — they are skipped with a
///   reason. `geoip:private` is unnecessary: private ranges are already
///   routed direct by the engine.
pub fn parse_exclusion_list(text: &str) -> ExclusionImport {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        parse_v2rayn_routing_json(text)
    } else {
        parse_plain_list(text)
    }
}

fn parse_plain_list(text: &str) -> ExclusionImport {
    let mut rules = Vec::new();
    let mut skipped = Vec::new();

    for raw in text.lines() {
        // Full-line AND inline comments: '#' never occurs in a domain,
        // IP or CIDR, so a plain split is unambiguous.
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
            continue;
        }
        match classify_entry(line) {
            Ok((kind, value)) => rules.push(RuleView {
                kind: kind.to_string(),
                value,
                action: "direct".to_string(),
            }),
            Err(reason) => skipped.push((line.to_string(), reason)),
        }
    }

    ExclusionImport {
        rules,
        skipped,
        format: "plain list",
    }
}

/// Classify one plain-list entry into a (kind, value) rule pair.
fn classify_entry(entry: &str) -> Result<(&'static str, String), String> {
    // CIDR / bare IP?
    if entry.contains('/') || entry.parse::<std::net::IpAddr>().is_ok() {
        return classify_ip(entry);
    }
    // Known unsupported prefixes (v2rayN list styles).
    let lower = entry.to_ascii_lowercase();
    for prefix in ["geosite:", "geoip:", "regexp:"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let _ = rest;
            return Err(format!(
                "{prefix} entries need geo/regex databases — not supported in the zero-dependency build"
            ));
        }
    }
    // Domain (leading '.' = explicit suffix form).
    let domain = entry.trim_start_matches('.');
    if is_plausible_domain(domain) {
        return Ok(("domainSuffix", domain.to_ascii_lowercase()));
    }
    Err("not a domain, IP or CIDR".to_string())
}

/// Validate + normalize an IP / CIDR entry.
fn classify_ip(entry: &str) -> Result<(&'static str, String), String> {
    if let Some((ip, prefix)) = entry.split_once('/') {
        let ip: std::net::IpAddr = ip
            .parse()
            .map_err(|_| format!("'{entry}' is not a valid IP/CIDR"))?;
        let prefix: u8 = prefix
            .parse()
            .map_err(|_| format!("'{entry}' has a non-numeric prefix length"))?;
        let max = match ip {
            std::net::IpAddr::V4(_) => 32,
            std::net::IpAddr::V6(_) => 128,
        };
        if prefix > max {
            return Err(format!("'{entry}': prefix /{prefix} exceeds /{max}"));
        }
        return Ok(("ipCidr", entry.to_string()));
    }
    // Bare IP → host route.
    let ip: std::net::IpAddr = entry
        .parse()
        .map_err(|_| format!("'{entry}' is not a valid IP"))?;
    let cidr = match ip {
        std::net::IpAddr::V4(_) => format!("{ip}/32"),
        std::net::IpAddr::V6(_) => format!("{ip}/128"),
    };
    Ok(("ipCidr", cidr))
}

/// Conservative domain plausibility: non-empty, no spaces, no path/query
/// separators, at least one dot (bare TLDs like "localhost" are not
/// routing-relevant) and only ASCII host characters.
fn is_plausible_domain(s: &str) -> bool {
    !s.is_empty()
        && s.contains('.')
        && !s.contains(' ')
        && !s.contains('/')
        && !s.contains(':')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

fn parse_v2rayn_routing_json(text: &str) -> ExclusionImport {
    let mut rules: Vec<RuleView> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();

    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            return ExclusionImport {
                rules,
                skipped: vec![("<whole file>".to_string(), format!("invalid JSON: {e}"))],
                format: "v2rayN routing JSON",
            }
        }
    };

    // Accept {"routing":{"rules":[..]}}, {"rules":[..]} and a bare [..].
    let array = value
        .get("routing")
        .and_then(|r| r.get("rules"))
        .or_else(|| value.get("rules"))
        .and_then(|r| r.as_array())
        .cloned()
        .or_else(|| value.as_array().cloned());

    let Some(array) = array else {
        return ExclusionImport {
            rules,
            skipped: vec![(
                "<whole file>".to_string(),
                "JSON has no routing.rules array".to_string(),
            )],
            format: "v2rayN routing JSON",
        };
    };

    for rule in &array {
        // outboundTag → our action vocabulary. v2rayN stock tags:
        // direct/freedom → direct, proxy → proxy, block/blackhole → reject.
        let tag = rule
            .get("outboundTag")
            .and_then(|t| t.as_str())
            .unwrap_or("direct");
        let action = match tag.to_ascii_lowercase().as_str() {
            "direct" | "freedom" => "direct",
            "proxy" => "proxy",
            "block" | "blackhole" => "reject",
            other => {
                skipped.push((
                    rule.to_string(),
                    format!("outboundTag '{other}' has no MultiTunnel equivalent"),
                ));
                continue;
            }
        };

        let mut rule_added = false;
        // domain: [ "domain:x", "full:x", "geosite:x", "x" ]
        if let Some(domains) = rule.get("domain").and_then(|d| d.as_array()) {
            for d in domains {
                let Some(d) = d.as_str() else {
                    skipped.push((d.to_string(), "non-string domain entry".to_string()));
                    continue;
                };
                let lower = d.to_ascii_lowercase();
                let (suffix, widened) = if let Some(rest) = lower.strip_prefix("domain:") {
                    (rest, false)
                } else if let Some(rest) = lower.strip_prefix("full:") {
                    (rest, true) // widened to subdomains — documented
                } else if lower.starts_with("geosite:") || lower.starts_with("regexp:") {
                    skipped.push((
                        d.to_string(),
                        format!("'{d}': geo/regex matchers are not supported"),
                    ));
                    continue;
                } else {
                    (lower.as_str(), false)
                };
                if !is_plausible_domain(suffix) {
                    skipped.push((d.to_string(), "not a plausible domain".to_string()));
                    continue;
                }
                let _ = widened; // summary-level note only; semantics documented
                rules.push(RuleView {
                    kind: "domainSuffix".to_string(),
                    value: suffix.to_string(),
                    action: action.to_string(),
                });
                rule_added = true;
            }
        }
        // ip: [ "1.2.3.4", "10.0.0.0/8", "geoip:cn" ]
        if let Some(ips) = rule.get("ip").and_then(|i| i.as_array()) {
            for ip in ips {
                let Some(ip) = ip.as_str() else {
                    skipped.push((ip.to_string(), "non-string ip entry".to_string()));
                    continue;
                };
                if ip.to_ascii_lowercase().starts_with("geoip:") {
                    if ip.eq_ignore_ascii_case("geoip:private") {
                        // Already enforced by the engine; importing it would
                        // be redundant, not wrong — skip with the reason.
                        skipped.push((
                            ip.to_string(),
                            "geoip:private is already routed direct by the engine".to_string(),
                        ));
                    } else {
                        skipped.push((
                            ip.to_string(),
                            "geoip entries need geo databases — not supported".to_string(),
                        ));
                    }
                    continue;
                }
                match classify_ip(ip) {
                    Ok((kind, value)) => {
                        rules.push(RuleView {
                            kind: kind.to_string(),
                            value,
                            action: action.to_string(),
                        });
                        rule_added = true;
                    }
                    Err(reason) => skipped.push((ip.to_string(), reason)),
                }
            }
        }

        if !rule_added {
            skipped.push((
                rule.to_string(),
                "rule carried no mappable domain/ip entries".to_string(),
            ));
        }
    }

    ExclusionImport {
        rules,
        skipped,
        format: "v2rayN routing JSON",
    }
}

#[cfg(test)]
mod exclusion_tests {
    use super::*;

    #[test]
    fn plain_list_domains_ips_and_comments() {
        let text = "# my exclusions\n\
                    example.com\n\
                    .bank.example.org   # leading dot = suffix form\n\
                    192.168.50.7\n\
                    10.20.0.0/16\n\
                    ;; semicolon comment\n\
                    geosite:cn\n";
        let out = parse_exclusion_list(text);
        assert_eq!(out.format, "plain list");
        assert_eq!(out.rules.len(), 4);
        assert_eq!(
            out.rules[0],
            RuleView {
                kind: "domainSuffix".into(),
                value: "example.com".into(),
                action: "direct".into()
            }
        );
        assert_eq!(out.rules[1].value, "bank.example.org");
        assert_eq!(
            out.rules[2],
            RuleView {
                kind: "ipCidr".into(),
                value: "192.168.50.7/32".into(),
                action: "direct".into()
            }
        );
        assert_eq!(out.rules[3].value, "10.20.0.0/16");
        assert_eq!(out.skipped.len(), 1);
        assert!(out.skipped[0].0.contains("geosite:cn"));
    }

    #[test]
    fn plain_list_ipv6_and_garbage() {
        let out = parse_exclusion_list("fd00::1\nnot a domain!!\n2001:db8::/32\n");
        assert_eq!(out.rules.len(), 2);
        assert_eq!(out.rules[0].value, "fd00::1/128");
        assert_eq!(out.rules[1].value, "2001:db8::/32");
        assert_eq!(out.skipped.len(), 1);
        assert!(out.skipped[0].1.contains("not a domain"));
    }

    #[test]
    fn plain_list_rejects_bad_cidr_prefix() {
        let out = parse_exclusion_list("10.0.0.0/40\n");
        assert!(out.rules.is_empty());
        assert!(out.skipped[0].1.contains("exceeds /32"));
    }

    #[test]
    fn v2rayn_routing_json_full_shape() {
        let text = r#"{
            "routing": {
                "rules": [
                    { "type": "field", "domain": ["domain:example.com", "full:exact.org", "geosite:cn"], "outboundTag": "direct" },
                    { "type": "field", "ip": ["geoip:private", "geoip:cn", "192.0.2.0/24"], "outboundTag": "direct" },
                    { "type": "field", "domain": ["ads.example.net"], "outboundTag": "block" },
                    { "type": "field", "domain": ["proxy.example.net"], "outboundTag": "proxy" },
                    { "type": "field", "domain": ["weird.example.net"], "outboundTag": "my-own-tag" }
                ]
            }
        }"#;
        let out = parse_exclusion_list(text);
        assert_eq!(out.format, "v2rayN routing JSON");
        assert_eq!(out.rules.len(), 5);
        assert_eq!(out.rules[0].value, "example.com");
        assert_eq!(out.rules[1].value, "exact.org");
        assert_eq!(out.rules[2].kind, "ipCidr");
        assert_eq!(out.rules[2].value, "192.0.2.0/24");
        assert_eq!(out.rules[3].action, "reject");
        assert_eq!(out.rules[4].action, "proxy");
        // geosite:cn, geoip:private, geoip:cn, unknown outboundTag rule.
        assert_eq!(out.skipped.len(), 4);
        assert!(out.summary().contains("imported 5 rules"));
    }

    #[test]
    fn v2rayn_bare_rules_array_and_invalid_json() {
        let out = parse_exclusion_list(r#"[{"domain":["a.com"],"outboundTag":"direct"}]"#);
        assert_eq!(out.rules.len(), 1);
        assert_eq!(out.rules[0].value, "a.com");

        let bad = parse_exclusion_list("{not json");
        assert!(bad.rules.is_empty());
        assert!(bad.skipped[0].1.contains("invalid JSON"));
    }

    #[test]
    fn summary_counts() {
        let out = parse_exclusion_list("a.com\nb.net\ngoop:x\n");
        assert!(out
            .summary()
            .starts_with("imported 2 rules from plain list"));
        assert!(out.summary().contains("skipped 1"));
    }
}

#[cfg(test)]
mod close_to_tray_migration_tests {
    use super::*;

    /// S15: settings.json written by v0.7/v0.8 (no `close_to_tray` key)
    /// loads with close_to_tray = false — the serde-default migration
    /// (D8 version-free, D20 precedent), so the toggle starts honest.
    #[test]
    fn old_settings_without_close_to_tray_default_off() {
        let json = r#"{
            "theme_seed": 42,
            "dark_mode": false,
            "system_proxy": true,
            "merge_policy": "balanced",
            "autostart": false,
            "bonded_adapters": ["wifi"],
            "bond_mode": "per-connection",
            "merge_port": 7443,
            "socks_port": 10808,
            "http_port": 10809,
            "allow_lan": false,
            "log_level": "info",
            "probe_target": "connectivitycheck.gstatic.com:80",
            "exit_probe_url": "http://ip-api.com/json"
        }"#;
        let v: SettingsView = serde_json::from_str(json).expect("old settings load");
        assert!(!v.close_to_tray);
        assert_eq!(v.merge_policy, "balanced", "unrelated fields intact");
    }

    /// The round trip: a file written by S15 reads back the same value.
    #[test]
    fn close_to_tray_roundtrips() {
        let mut v = SettingsView::default_settings_for_tests();
        v.close_to_tray = true;
        let json = serde_json::to_string(&v).expect("serialize");
        assert!(json.contains("\"close_to_tray\":true"));
        let back: SettingsView = serde_json::from_str(&json).expect("reload");
        assert!(back.close_to_tray);
    }

    impl SettingsView {
        pub(crate) fn default_settings_for_tests() -> Self {
            Self {
                theme_seed: 0,
                dark_mode: false,
                theme_mode: String::new(),
                system_proxy: false,
                merge_policy: "balanced".into(),
                autostart: false,
                bonded_adapters: vec![],
                bond_mode: "per-connection".into(),
                merge_port: 7443,
                socks_port: 10808,
                http_port: 10809,
                allow_lan: false,
                log_level: "info".into(),
                probe_target: String::new(),
                exit_probe_url: String::new(),
                close_to_tray: false,
                always_on_top: false,
                kill_switch: false,
                diagnostics: false,
                speed_test_url: default_speed_test_url(),
                speed_test_upload_url: default_speed_test_upload_url(),
                qos_fair: false,
            }
        }
    }
}

#[cfg(test)]
mod always_on_top_migration_tests {
    use super::*;

    /// S16: settings.json written by v0.9 (no `always_on_top` key) loads
    /// with always_on_top = false — the serde-default migration (D8).
    #[test]
    fn old_settings_without_always_on_top_default_off() {
        let json = r#"{
            "theme_seed": 42,
            "dark_mode": false,
            "system_proxy": true,
            "merge_policy": "balanced",
            "autostart": false,
            "close_to_tray": true,
            "log_level": "info",
            "probe_target": "connectivitycheck.gstatic.com:80",
            "exit_probe_url": "http://ip-api.com/json"
        }"#;
        let v: SettingsView = serde_json::from_str(json).expect("old settings load");
        assert!(!v.always_on_top);
        assert!(v.close_to_tray, "S15 field intact");
    }

    /// The round trip: a file written by S16 reads back the same value.
    #[test]
    fn always_on_top_roundtrips() {
        let mut v = SettingsView::default_settings_for_tests();
        v.always_on_top = true;
        let json = serde_json::to_string(&v).expect("serialize");
        assert!(json.contains("\"always_on_top\":true"));
        let back: SettingsView = serde_json::from_str(&json).expect("reload");
        assert!(back.always_on_top);
    }
}

#[cfg(test)]
mod theme_mode_migration_tests {
    use super::*;

    /// S18: settings.json written by v0.11 and older (no `theme_mode`
    /// key) loads with the mode derived from the legacy `dark_mode` bool —
    /// an upgrade never flips the user's appearance (D8 version-free
    /// migration via serde default + the legacy fallback).
    #[test]
    fn old_settings_derive_mode_from_legacy_dark_mode() {
        let base = r#"{
            "theme_seed": 42,
            "dark_mode": {},
            "system_proxy": false,
            "merge_policy": "balanced",
            "autostart": false
        }"#;
        let dark: SettingsView =
            serde_json::from_str(&base.replace("{}", "true")).expect("legacy dark loads");
        assert_eq!(dark.theme_mode(), ThemeMode::Dark);
        assert!(dark.effective_dark(false));
        let light: SettingsView =
            serde_json::from_str(&base.replace("{}", "false")).expect("legacy light loads");
        assert_eq!(light.theme_mode(), ThemeMode::Light);
        assert!(!light.effective_dark(true));
    }

    /// The three S18 modes round-trip through JSON and resolve correctly
    /// against both OS preferences.
    #[test]
    fn theme_mode_strings_roundtrip_and_resolve() {
        for (mode, sys_dark, want) in [
            (ThemeMode::System, true, true),
            (ThemeMode::System, false, false),
            (ThemeMode::Light, true, false),
            (ThemeMode::Dark, false, true),
        ] {
            let mut v = SettingsView::default_settings_for_tests();
            v.theme_mode = mode.as_str().to_string();
            let json = serde_json::to_string(&v).expect("serialize");
            assert!(json.contains(&format!("\"theme_mode\":\"{}\"", mode.as_str())));
            let back: SettingsView = serde_json::from_str(&json).expect("reload");
            assert_eq!(back.theme_mode(), mode);
            assert_eq!(back.effective_dark(sys_dark), want);
        }
    }

    /// Garbage in the stored string keeps the legacy bool's answer
    /// (never a panic, never a silent theme flip).
    #[test]
    fn invalid_theme_mode_falls_back_to_legacy_bool() {
        assert_eq!(ThemeMode::parse("neon", true), ThemeMode::Dark);
        assert_eq!(ThemeMode::parse("neon", false), ThemeMode::Light);
        assert_eq!(ThemeMode::parse("", true), ThemeMode::Dark);
    }

    /// S22: the per-link adapter grammar — the same `"<tag> [<adapter>]"`
    /// shapes the GUI's rate sampler matches, including the dedup
    /// ordinal (display keeps the adapter, drops " #<n>"); names
    /// without a suffix parse to None.
    #[test]
    fn adapter_of_link_name_parses_the_engine_grammar() {
        assert_eq!(
            adapter_of_link_name("gcp-vm [Wi-Fi]"),
            Some("Wi-Fi".to_string())
        );
        assert_eq!(
            adapter_of_link_name("gcp-vm [Ethernet #2]"),
            Some("Ethernet".to_string())
        );
        // No prefix smuggling / no suffix → None.
        assert_eq!(adapter_of_link_name("gcp-vm"), None);
        assert_eq!(adapter_of_link_name("gcp-vm ["), None);
        assert_eq!(adapter_of_link_name("gcp-vm []"), None);
        // The rate sampler's tricky cases round-trip verbatim.
        assert_eq!(
            adapter_of_link_name("gcp-vm [Wi-Fi Direct]"),
            Some("Wi-Fi Direct".to_string())
        );
        assert_eq!(
            adapter_of_link_name("gcp-vm [Ethernet 2]"),
            Some("Ethernet 2".to_string())
        );
    }
}

#[cfg(test)]
mod s19_migration_tests {
    use super::*;

    /// S19: settings.json written by v0.12 and older (no `kill_switch`,
    /// `diagnostics` or `speed_test_url` keys) loads with the safe
    /// defaults — kill switch off, diagnostics off, the stock speed-test
    /// URL (D8 version-free migration via serde defaults).
    #[test]
    fn old_settings_without_s19_keys_load_with_defaults() {
        let json = r#"{
            "theme_seed": 42,
            "dark_mode": false,
            "system_proxy": true,
            "merge_policy": "balanced",
            "autostart": false,
            "close_to_tray": true,
            "always_on_top": false,
            "log_level": "info",
            "probe_target": "connectivitycheck.gstatic.com:80",
            "exit_probe_url": "http://ip-api.com/json"
        }"#;
        let v: SettingsView = serde_json::from_str(json).expect("old settings load");
        assert!(!v.kill_switch);
        assert!(!v.diagnostics);
        assert_eq!(v.speed_test_url, default_speed_test_url());
        assert_eq!(v.speed_test_upload_url, default_speed_test_upload_url());
        assert!(v.close_to_tray, "S15 field intact");
        assert!(!v.always_on_top, "S16 field intact");
    }

    /// The S19 fields round-trip through JSON verbatim (an explicit
    /// non-default choice persists).
    #[test]
    fn s19_fields_roundtrip() {
        let mut v = SettingsView::default_settings_for_tests();
        v.kill_switch = true;
        v.diagnostics = true;
        v.speed_test_url = "http://127.0.0.1:9/down".to_string();
        v.speed_test_upload_url = "http://127.0.0.1:9/up".to_string();
        let json = serde_json::to_string(&v).expect("serialize");
        assert!(json.contains("\"kill_switch\":true"));
        assert!(json.contains("\"diagnostics\":true"));
        let back: SettingsView = serde_json::from_str(&json).expect("reload");
        assert!(back.kill_switch);
        assert!(back.diagnostics);
        assert_eq!(back.speed_test_url, "http://127.0.0.1:9/down");
        assert_eq!(back.speed_test_upload_url, "http://127.0.0.1:9/up");
    }

    /// S21 (feature o): settings.json written by v0.13 and older (no
    /// `qos_fair` key) loads with the scheduler OFF (D8 version-free
    /// migration via serde default), and an explicit choice round-trips.
    #[test]
    fn s21_qos_fair_defaults_and_roundtrip() {
        let json = r#"{
            "theme_seed": 42,
            "dark_mode": false,
            "system_proxy": true,
            "merge_policy": "balanced",
            "autostart": false,
            "log_level": "info"
        }"#;
        let v: SettingsView = serde_json::from_str(json).expect("old settings load");
        assert!(!v.qos_fair, "absent key → scheduler off");
        let mut v = SettingsView::default_settings_for_tests();
        v.qos_fair = true;
        let json = serde_json::to_string(&v).expect("serialize");
        assert!(json.contains("\"qos_fair\":true"));
        let back: SettingsView = serde_json::from_str(&json).expect("reload");
        assert!(back.qos_fair);
    }
}
