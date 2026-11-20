//! The GUI application: MVU model, messages, update loop, subscriptions.
//!
//! The app never touches credentials and never panics on service errors:
//! every service result maps to user-facing state. All widget styling comes
//! from [`crate::theme`] helpers built on `mt-m3` tokens.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use mt_service::{ExitInfo, MtService, ServiceEvent, ServiceState, SettingsView, StatusSnapshot};
use tokio::sync::broadcast;
use tokio::sync::Mutex as AsyncMutex;

use crate::theme::{MtTheme, SEED_SWATCHES};
use crate::{screens, util, Element};

/// The six top-level screens (left navigation rail order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Status,
    Servers,
    Adapters,
    Routing,
    Settings,
    Logs,
}

impl Screen {
    /// Rail order.
    pub const ALL: [Screen; 6] = [
        Screen::Status,
        Screen::Servers,
        Screen::Adapters,
        Screen::Routing,
        Screen::Settings,
        Screen::Logs,
    ];

    /// Human label used in the rail and the screen title.
    pub const fn label(self) -> &'static str {
        match self {
            Screen::Status => "Status",
            Screen::Servers => "Servers",
            Screen::Adapters => "Adapters",
            Screen::Routing => "Routing",
            Screen::Settings => "Settings",
            Screen::Logs => "Logs",
        }
    }
}

/// Log level filter for the Logs screen (derived from the "LEVEL " prefix
/// convention of service log lines).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilter {
    All,
    Info,
    Warning,
    Error,
}

impl LogFilter {
    /// Pick-list labels.
    pub const LABELS: [&'static str; 4] = ["all", "info", "warning", "error"];

    /// Maps a pick-list label back to the filter.
    pub fn from_label(l: &str) -> Self {
        match l {
            "info" => LogFilter::Info,
            "warning" => LogFilter::Warning,
            "error" => LogFilter::Error,
            _ => LogFilter::All,
        }
    }

    /// The pick-list label of this filter.
    pub fn label(self) -> &'static str {
        match self {
            LogFilter::All => "all",
            LogFilter::Info => "info",
            LogFilter::Warning => "warning",
            LogFilter::Error => "error",
        }
    }

    pub fn matches(self, line: &str) -> bool {
        match self {
            LogFilter::All => true,
            LogFilter::Info => line.starts_with("INFO") || line.starts_with("DEBUG"),
            LogFilter::Warning => line.starts_with("WARN"),
            LogFilter::Error => line.starts_with("ERROR"),
        }
    }
}

/// One editable row of the Routing rule editor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuleRow {
    /// One of: domain-suffix, domain-keyword, ip-cidr, any.
    pub kind: String,
    /// Match value (suffix, keyword, CIDR; ignored for `any`).
    pub value: String,
    /// One of: proxy, direct, reject.
    pub action: String,
}

/// One row of the Adapters screen: the service's adapter view plus the
/// DRAFT checkbox state (the view's own `selected` mirrors what is
/// persisted; the row's `selected` is what the user is editing).
#[derive(Debug, Clone)]
pub struct AdapterRow {
    pub view: mt_service::AdapterView,
    pub selected: bool,
    /// D20: per-interface graph expanded (arrow toggle).
    pub expanded: bool,
}

/// The parsed preview shown under the "add by paste" field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPreview {
    /// Protocol short name (vless/vmess/ss/trojan).
    pub protocol: String,
    /// Profile tag (from the link fragment).
    pub tag: String,
    /// "address:port".
    pub endpoint: String,
    /// Security + transport summary (reality/tls/none + tcp/ws/grpc).
    pub security: String,
}

impl From<&mt_links::Profile> for ParsedPreview {
    fn from(p: &mt_links::Profile) -> Self {
        let proto = match p.protocol {
            mt_links::Protocol::Vless => "vless",
            mt_links::Protocol::Vmess => "vmess",
            mt_links::Protocol::Trojan => "trojan",
            mt_links::Protocol::Shadowsocks => "ss",
        };
        let security = match &p.security {
            mt_links::Security::None => "none".to_string(),
            mt_links::Security::Tls { allow_insecure, .. } => if *allow_insecure {
                "tls (insecure)"
            } else {
                "tls"
            }
            .to_string(),
            mt_links::Security::Reality { .. } => "reality".to_string(),
        };
        let transport = match &p.transport {
            mt_links::Transport::Tcp => "tcp".to_string(),
            mt_links::Transport::Ws { .. } => "ws".to_string(),
            mt_links::Transport::Grpc { .. } => "grpc".to_string(),
            mt_links::Transport::HttpUpgrade { .. } => "httpupgrade".to_string(),
        };
        Self {
            protocol: proto.to_string(),
            tag: p.tag.clone(),
            endpoint: format!("{}:{}", p.address, p.port),
            security: format!("{security} / {transport}"),
        }
    }
}

/// Latency test state per profile id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LatencyState {
    /// Request in flight.
    InFlight,
    /// Result: measured ms or an error string.
    Done(Result<u32, String>),
}

/// Startup flags (CLI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// `--smoke`: run the update loop headlessly, then exit 0.
    pub smoke: bool,
}

impl Flags {
    /// Parses the binary's arguments. Unknown flags are ignored so the root
    /// crate can forward extra arguments without breakage.
    pub fn from_args<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut flags = Flags::default();
        for a in args {
            if a.as_ref() == "--smoke" {
                flags.smoke = true;
            }
        }
        flags
    }
}

/// Report returned by a completed smoke run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmokeReport {
    /// Update ticks driven.
    pub ticks: usize,
    /// Service events pumped through the update loop.
    pub events_seen: usize,
    /// Number of view builds executed.
    pub views_built: usize,
}

/// Every mutation the GUI can perform.
#[derive(Debug, Clone)]
pub enum Message {
    /// Periodic refresh tick (1 s subscription).
    Tick,
    /// A service event arrived (broadcast bridge).
    Event(ServiceEvent),
    /// Switch the active screen.
    Screen(Screen),
    /// The connect/disconnect switch changed.
    ConnectToggle(bool),
    /// Loaded status snapshot.
    StatusLoaded(StatusSnapshot),
    /// Loaded profile list.
    ProfilesLoaded(Vec<mt_service::ProfileView>),
    /// Paste field content changed.
    PasteChanged(String),
    /// Add the pasted link.
    PasteAdd,
    /// Clear the paste field.
    PasteClear,
    /// import_link finished.
    ImportDone(Result<mt_service::ProfileView, String>),
    /// set_active_profile(id) finished.
    ActiveSet(String, Result<(), String>),
    /// remove_profile(id) finished.
    ProfileRemoved(String, Result<(), String>),
    /// Request: test_latency(id).
    LatencyTest(String),
    /// test_latency(id) finished.
    LatencyTested(String, Result<u32, String>),
    /// Request: set_active_profile(id).
    SetActive(String),
    /// Request: remove_profile(id).
    RemoveProfile(String),
    /// Loaded routing rules (canonical copy).
    RulesLoaded(Vec<mt_service::RuleView>),
    /// Edited a rule's kind.
    RuleKind(usize, String),
    /// Edited a rule's value.
    RuleValue(usize, String),
    /// Edited a rule's action.
    RuleAction(usize, String),
    /// Append a blank rule row.
    RuleAdded,
    /// Remove rule row at index.
    RuleRemoved(usize),
    /// Push the edited rules to the service.
    RulesApply,
    /// set_routing_rules finished.
    RulesSaved(Result<(), String>),
    /// Discard unsaved rule edits.
    RulesReverted,
    /// S10: the exclusions-import file path field changed.
    RulesImportPathChanged(String),
    /// S10: import exclusions from the file at [`App::rules_import_path`].
    RulesImport,
    /// S10: the import finished (parsed rules, or an I/O / parse error).
    RulesImported(Result<mt_service::ExclusionImport, String>),
    /// Loaded settings (canonical copy).
    SettingsLoaded(SettingsView),
    /// A seed swatch was pressed.
    SeedSwatch(u32),
    /// The numeric seed field changed.
    SeedFieldChanged(String),
    /// Commit the numeric seed field.
    SeedCommitted,
    /// Dark mode toggled.
    DarkToggled(bool),
    /// System proxy toggled.
    SystemProxyToggled(bool),
    /// Merge policy picked.
    MergePolicyPicked(String),
    /// Autostart toggled.
    AutostartToggled(bool),
    /// set_settings finished.
    SettingsSaved(Result<(), String>),
    /// Logs level filter picked.
    LogFilterPicked(String),
    /// Clear the log buffer.
    LogsCleared,
    /// Loaded the adapter list (Adapters screen).
    AdaptersLoaded(Vec<mt_service::AdapterView>),
    /// An adapter checkbox flipped (draft state, index into `adapters`).
    AdapterToggled(usize),
    /// D20: an adapter's per-interface graph expanded/collapsed.
    AdapterExpanded(usize),
    /// S11: move a SELECTED adapter row within the priority order
    /// (index into `adapters`, delta −1/±1; ignored at the block edges
    /// and for unselected rows).
    AdapterMoved(usize, i32),
    /// Push the drafted selection to the service.
    AdaptersApply,
    /// set_bonded_adapters finished.
    AdaptersSaved(Result<(), String>),
    /// Discard the draft, restore the persisted selection.
    AdaptersReverted,
    /// Re-enumerate adapters from the OS.
    AdaptersRefresh,
    // --- S9: Status screen (server picker, exit probe) ---
    /// Server picked in the Status dropdown (set active + reconnect).
    StatusServerPicked(String),
    /// Run the exit-IP probe through the live tunnel.
    ExitProbe,
    /// Exit probe finished.
    ExitProbed(Result<ExitInfo, String>),
    // --- S9: Advanced settings ---
    SocksPortChanged(String),
    HttpPortChanged(String),
    MergePortChanged(String),
    AllowLanToggled(bool),
    LogLevelPicked(String),
    BondModePicked(String),
    ProbeTargetChanged(String),
    ExitUrlChanged(String),
    // --- S15: blended title bar, tray, close-to-tray (D25) ---
    /// A window event arrived (Opened/CloseRequested/Resized…).
    WindowEvent(iced::window::Id, iced::window::Event),
    /// The titlebar drag strip was pressed (drag; double = maximize).
    TitleDrag,
    /// The titlebar minimize button.
    TitleMinimize,
    /// The titlebar maximize/restore button.
    TitleMaximize,
    /// The titlebar close button (same path as the OS close request).
    TitleClose,
    /// Queried maximize state arrived (drives the □/▣ glyph).
    WindowMaximized(bool),
    /// "close to tray" toggled in Settings.
    CloseToTrayToggled(bool),
    /// An event from the tray icon (left click / menu selection).
    Tray(crate::tray::TrayEvent),
    // --- S16: titlebar right-click menu + always-on-top ---
    /// The mouse moved inside the window (anchors the context menu).
    CursorMoved(iced::Point),
    /// The titlebar was RIGHT-clicked (open the window menu there).
    TitleContext,
    /// A click landed outside the open window menu (close it).
    TitleContextDismiss,
    /// A window-menu entry was picked.
    TitleMenu(TitleMenuItem),
    /// "always on top" toggled (window menu or Settings).
    AlwaysOnTopToggled(bool),
}

/// S16: one entry of the titlebar right-click window menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleMenuItem {
    AlwaysOnTop,
    Minimize,
    Maximize,
    Close,
}

/// Valid routing rule kinds (matches the mt-net router matcher).
pub const RULE_KINDS: [&str; 4] = ["domainSuffix", "domainKeyword", "ipCidr", "any"];
/// Valid routing rule actions (matches the mt-net router actions).
pub const RULE_ACTIONS: [&str; 3] = ["proxy", "direct", "reject"];
/// Valid merge policies (matches the mt-net aggregator + the real
/// service's parser; the first entry is the stock default).
pub const MERGE_POLICIES: [&str; 5] = [
    "balanced",
    "round-robin",
    "priority",
    "failover",
    "lowest-latency",
];
/// Valid bond modes (S9): whole-connection spreading vs stream bonding.
pub const BOND_MODES: [&str; 2] = ["per-connection", "stream"];
/// Valid log levels (engine + service).
pub const LOG_LEVELS: [&str; 4] = ["error", "warning", "info", "debug"];
/// Bandwidth graph window, one sample per Throughput event (1 s ticks).
pub const RATE_HISTORY_LEN: usize = 120;

/// The GUI view model.
pub struct App {
    /// The service backend (mock in standalone builds; real in wave 3).
    pub service: Arc<dyn MtService>,
    /// Broadcast receiver bridged into the events subscription (guarded so
    /// both the subscription stream and the lag-pump can drain it).
    pub events_rx: Arc<AsyncMutex<broadcast::Receiver<ServiceEvent>>>,
    /// Startup flags.
    pub flags: Flags,
    /// Active screen.
    pub screen: Screen,
    /// Theme snapshot (re-derived on seed/dark changes — live re-theme).
    pub theme: MtTheme,
    /// Latest status snapshot.
    pub status: Option<StatusSnapshot>,
    /// Latest throughput sample (up_bps, down_bps).
    pub throughput: Option<(u64, u64)>,
    /// Profiles for the Servers screen.
    pub profiles: Vec<mt_service::ProfileView>,
    /// Paste field content.
    pub paste: String,
    /// Parse preview of the paste field.
    pub paste_preview: Option<Result<ParsedPreview, String>>,
    /// Transient user-facing notice (import/remove errors, confirmations).
    pub notice: Option<String>,
    /// Editable routing rules (draft).
    pub rules: Vec<RuleRow>,
    /// Canonical routing rules as last loaded/saved.
    pub rules_saved: Vec<RuleRow>,
    /// Routing editor status line.
    pub rules_status: Option<String>,
    /// S10: path of the exclusions file to import (Routing screen).
    pub rules_import_path: String,
    /// Adapters screen rows (service view + draft selection).
    pub adapters: Vec<AdapterRow>,
    /// Adapters screen status line.
    pub adapters_status: Option<String>,
    /// Settings draft (mutated live, saved immediately on change).
    pub settings: SettingsView,
    /// Numeric seed field text.
    pub seed_text: String,
    /// Settings status line.
    pub settings_status: Option<String>,
    /// Latency test states per profile id.
    pub latency: HashMap<String, LatencyState>,
    /// Log buffer (ring, capped).
    pub logs: VecDeque<String>,
    /// Logs level filter.
    pub log_filter: LogFilter,
    /// Bandwidth graph samples: (up_bps, down_bps) per second, newest last
    /// (S9 / D19). Cleared on connect/disconnect.
    pub rate_history: VecDeque<(u64, u64)>,
    /// D20: per-ADAPTER rate samples (same shape as [`Self::rate_history`]),
    /// keyed by adapter id — one series per Adapters-screen row, driving
    /// the expandable per-interface graphs.
    pub adapter_rates: HashMap<String, VecDeque<(u64, u64)>>,
    /// D20: last seen CUMULATIVE (up, down) per link name, the delta base
    /// for [`Self::adapter_rates`] (links report lifetime byte counters;
    /// the graph needs bytes/second).
    pub link_last_bytes: HashMap<String, (u64, u64)>,
    /// Exit probe result for the Status screen (None = never run).
    pub exit: Option<Result<ExitInfo, String>>,
    /// An exit probe is in flight.
    pub exit_probing: bool,
    /// S15: the main window's iced id (captured from the first Opened
    /// event; window Tasks need it).
    pub window_id: Option<iced::window::Id>,
    /// S15: maximize state for the titlebar restore glyph.
    pub window_maximized: bool,
    /// S15: last titlebar press (double-click-to-maximize detect).
    pub last_title_press: Option<std::time::Instant>,
    /// S15: the in-app tray handle (None on non-Windows / spawn failure).
    pub tray: Option<crate::tray::TrayHandle>,
    /// S15: tray event receiver bridged into the subscription.
    pub tray_rx:
        Option<Arc<AsyncMutex<tokio::sync::mpsc::UnboundedReceiver<crate::tray::TrayEvent>>>>,
    /// S15: last tooltip text pushed to the tray (dedup on tick).
    pub tray_tip: String,
    /// S15: last Quick Settings model pushed to the tray (dedup).
    pub tray_model: Option<Vec<crate::tray::QuickEntry>>,
    /// S16: last cursor position inside the window (anchors the
    /// titlebar context menu; None until the first move).
    pub cursor: Option<iced::Point>,
    /// S16: current window size (clamps the context menu on screen).
    pub window_size: Option<iced::Size>,
    /// S16: the open titlebar window menu's anchor (None = closed).
    pub title_menu: Option<iced::Point>,
    /// S16: session-cumulative (up, down) bytes — the tray tooltip's
    /// "volume so far" (reset when the tunnel leaves Connected).
    pub session_bytes: (u64, u64),
    /// S16: tray icon style last pushed (dedup; starts Mono — the app
    /// boots not-connected).
    pub tray_icon: crate::tray::TrayIcon,
}

impl App {
    /// Bare constructor with empty state (service loads fill it).
    pub fn new(service: Arc<dyn MtService>, flags: Flags) -> Self {
        let events_rx = Arc::new(AsyncMutex::new(service.events()));
        let seed_text = String::new();
        let settings = SettingsView {
            theme_seed: SEED_SWATCHES[0],
            dark_mode: false,
            system_proxy: false,
            merge_policy: MERGE_POLICIES[0].to_string(),
            autostart: false,
            bonded_adapters: Vec::new(),
            bond_mode: BOND_MODES[0].to_string(),
            merge_port: 7443,
            socks_port: 10808,
            http_port: 10809,
            allow_lan: false,
            log_level: LOG_LEVELS[1].to_string(),
            probe_target: "connectivitycheck.gstatic.com:80".to_string(),
            exit_probe_url: "http://ip-api.com/json".to_string(),
            close_to_tray: false,
            always_on_top: false,
        };
        Self {
            theme: MtTheme::from_settings(&settings),
            service,
            events_rx,
            flags,
            screen: Screen::Status,
            status: None,
            throughput: None,
            profiles: Vec::new(),
            paste: String::new(),
            paste_preview: None,
            notice: None,
            rules: Vec::new(),
            rules_saved: Vec::new(),
            rules_status: None,
            rules_import_path: String::new(),
            adapters: Vec::new(),
            adapters_status: None,
            settings,
            seed_text,
            settings_status: None,
            latency: HashMap::new(),
            logs: VecDeque::new(),
            log_filter: LogFilter::All,
            rate_history: VecDeque::new(),
            adapter_rates: HashMap::new(),
            link_last_bytes: HashMap::new(),
            exit: None,
            exit_probing: false,
            window_id: None,
            window_maximized: false,
            last_title_press: None,
            tray: None,
            tray_rx: None,
            tray_tip: String::new(),
            tray_model: None,
            // S16
            cursor: None,
            window_size: None,
            title_menu: None,
            session_bytes: (0, 0),
            tray_icon: crate::tray::TrayIcon::Mono,
        }
    }

    /// Synchronous init for the iced runtime: returns the app plus a task
    /// batch that loads all screen data. The tray is spawned here — its
    /// lifetime is the app's (Windows; the stub returns None elsewhere),
    /// and NEVER in `init_async` (smoke/tests must not create real
    /// notification icons).
    pub fn init(service: Arc<dyn MtService>, flags: Flags) -> (Self, iced::Task<Message>) {
        let mut app = Self::new(service, flags);
        // S16: the app boots not-connected → the monochrome tray icon.
        let initial_icon = crate::tray::TrayIcon::Mono;
        if let Some((handle, rx)) = crate::tray::spawn(
            crate::tray::tooltip(ServiceState::Disconnected, ""),
            crate::tray::model_from(None, &[]),
            initial_icon,
        ) {
            app.tray = Some(handle);
            app.tray_tip = crate::tray::tooltip(ServiceState::Disconnected, "");
            app.tray_model = Some(crate::tray::model_from(None, &[]));
            app.tray_icon = initial_icon;
            app.tray_rx = Some(Arc::new(AsyncMutex::new(rx)));
        }
        let tasks = [
            app.refresh_status(),
            app.load_profiles(),
            app.load_rules(),
            app.load_settings(),
            app.load_adapters(),
        ];
        (app, iced::Task::batch(tasks))
    }

    /// Async init for `--smoke` and tests: awaits the loads directly (no
    /// iced executor available outside the runtime).
    pub async fn init_async(service: Arc<dyn MtService>, flags: Flags) -> Self {
        let mut app = Self::new(service, flags);
        app.status = Some(app.service.status().await);
        app.profiles = app.service.profiles().await;
        app.rules = app
            .service
            .routing_rules()
            .await
            .into_iter()
            .map(|r| RuleRow {
                kind: r.kind,
                value: r.value,
                action: r.action,
            })
            .collect();
        app.rules_saved = app.rules.clone();
        app.settings = app.service.settings().await;
        app.seed_text = app.settings.theme_seed.to_string();
        app.adapters = app
            .service
            .adapters()
            .await
            .into_iter()
            .map(|view| AdapterRow {
                selected: view.selected,
                expanded: false,
                view,
            })
            .collect();
        app.theme
            .rederive(app.settings.theme_seed, app.settings.dark_mode);
        app.seed_initial_logs();
        app
    }

    /// Seeds the log buffer with a header line (real history lives on the
    /// service side; the GUI shows what it receives).
    fn seed_initial_logs(&mut self) {
        util::push_log(
            &mut self.logs,
            "INFO  gui: session started (log buffer only holds this session)".to_string(),
        );
    }

    // ------------------------------------------------------------------
    // Task helpers
    // ------------------------------------------------------------------

    fn refresh_status(&self) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(async move { svc.status().await }, Message::StatusLoaded)
    }

    fn load_profiles(&self) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(async move { svc.profiles().await }, Message::ProfilesLoaded)
    }

    fn load_rules(&self) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(
            async move { svc.routing_rules().await },
            Message::RulesLoaded,
        )
    }

    fn load_settings(&self) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(async move { svc.settings().await }, Message::SettingsLoaded)
    }

    fn load_adapters(&self) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(async move { svc.adapters().await }, Message::AdaptersLoaded)
    }

    /// D20: one per-interface rate sample from a fresh status snapshot.
    ///
    /// Links (per-connection mode) and bond carriers (stream mode) both
    /// report CUMULATIVE wire byte counters named `"<profile> [<adapter>]"`
    /// (the engine tag; the unbound single link has no bracket). For every
    /// adapter row we match its links, take the 1 s DELTA of the counters,
    /// and append `(up_bps, down_bps)` to that adapter's series. A counter
    /// reset (reconnect inside the same Connected state, engine respawn)
    /// yields a saturated zero delta for one sample — the graph dips, as it
    /// should.
    ///
    /// Sampling rides the 1 s `StatusLoaded` cadence (the same clock the
    /// combined graph uses), so "bps" here is "bytes since the previous
    /// snapshot" — Task-Manager-accurate for a 1 s poll.
    pub fn sample_adapter_rates(&mut self, snap: &StatusSnapshot) {
        // New per-adapter sample accumulation.
        let mut deltas: HashMap<String, (u64, u64)> = HashMap::new();
        for link in &snap.links {
            let cur = (link.bytes_up, link.bytes_down);
            let prev = self
                .link_last_bytes
                .insert(link.name.clone(), cur)
                .unwrap_or(cur); // first sight → delta 0 (no fake spike)
                                 // S16: session totals from the same counters — a counter
                                 // RESET (engine respawn / reconnect inside Connected) counts
                                 // its full value, not zero, so the totals never dip.
            if prev != cur {
                let (du, dd) = if cur >= prev {
                    (cur.0 - prev.0, cur.1 - prev.1)
                } else {
                    cur
                };
                self.session_bytes.0 = self.session_bytes.0.saturating_add(du);
                self.session_bytes.1 = self.session_bytes.1.saturating_add(dd);
            }
            let (du, dd) = (cur.0.saturating_sub(prev.0), cur.1.saturating_sub(prev.1));
            // Map the link to every adapter row whose name it carries.
            let mut matched = false;
            for row in &self.adapters {
                if link_matches_adapter(&link.name, &row.view.name) {
                    let e = deltas.entry(row.view.id.clone()).or_default();
                    e.0 = e.0.saturating_add(du);
                    e.1 = e.1.saturating_add(dd);
                    matched = true;
                }
            }
            if !matched {
                // Unbound single link (no adapter selected): it feeds the
                // combined graph via Throughput; no per-adapter row owns it.
                tracing::debug!(link = %link.name, "link maps to no adapter row");
            }
        }
        // Drop link entries that vanished (reconnect with fewer links) so
        // stale names never poison a later delta base.
        self.link_last_bytes
            .retain(|name, _| snap.links.iter().any(|l| &l.name == name));
        // Append one sample to EVERY adapter series (zeros included — a
        // flat line is the honest signal for "this NIC carries nothing").
        for row in &self.adapters {
            let series = self.adapter_rates.entry(row.view.id.clone()).or_default();
            let sample = deltas.get(&row.view.id).copied().unwrap_or((0, 0));
            series.push_back(sample);
            while series.len() > RATE_HISTORY_LEN {
                series.pop_front();
            }
        }
    }

    /// One numeric advanced field: parse (empty = 0), apply to the draft,
    /// save; invalid input is reported in the settings status line and NOT
    /// persisted.
    fn advanced_port_field(
        &mut self,
        apply: impl FnOnce(&mut SettingsView, u16),
        raw: String,
        label: &str,
    ) -> iced::Task<Message> {
        let trimmed = raw.trim().to_string();
        let parsed: Option<u16> = if trimmed.is_empty() {
            Some(0)
        } else {
            trimmed.parse().ok()
        };
        match parsed {
            Some(v) => {
                apply(&mut self.settings, v);
                self.save_settings(self.settings.clone())
            }
            None => {
                self.settings_status = Some(format!("{label}: enter a port number (0-65535)"));
                iced::Task::none()
            }
        }
    }

    fn save_settings(&self, s: SettingsView) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(
            async move { svc.set_settings(s).await },
            Message::SettingsSaved,
        )
    }

    /// Drains pending broadcast events into the update loop. Returns the
    /// number of events handled. Tasks produced while pumping are dropped
    /// (in the GUI the subscription delivers events with live tasks; this
    /// path only reconciles lagged deliveries and drives the smoke run).
    pub fn pump_events(&mut self) -> usize {
        let mut events = Vec::new();
        {
            let Ok(mut rx) = self.events_rx.try_lock() else {
                return 0; // subscription stream is draining concurrently
            };
            loop {
                match rx.try_recv() {
                    Ok(ev) => events.push(ev),
                    Err(broadcast::error::TryRecvError::Empty)
                    | Err(broadcast::error::TryRecvError::Closed) => break,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                }
            }
        }
        let n = events.len();
        for ev in events {
            let task = self.update(Message::Event(ev));
            drop(task);
        }
        n
    }

    // ------------------------------------------------------------------
    // Update
    // ------------------------------------------------------------------

    /// The MVU update function (wired into `iced::application`).
    pub fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Tick => {
                self.pump_events();
                let t = self.refresh_status();
                self.sync_tray();
                return t;
            }
            Message::Event(ev) => match ev {
                ServiceEvent::StateChanged(state) => {
                    if matches!(
                        state,
                        ServiceState::Disconnected
                            | ServiceState::Connecting
                            | ServiceState::Failed
                    ) {
                        // New session (or none): fresh graph, stale probe gone.
                        self.rate_history.clear();
                        self.adapter_rates.clear();
                        self.link_last_bytes.clear();
                        self.session_bytes = (0, 0);
                        self.exit = None;
                    }
                    return self.refresh_status();
                }
                ServiceEvent::LinkUpdated(link) => {
                    if let Some(s) = self.status.as_mut() {
                        if let Some(slot) = s.links.iter_mut().find(|l| l.name == link.name) {
                            *slot = link;
                        }
                    }
                }
                ServiceEvent::Throughput { up_bps, down_bps } => {
                    self.throughput = Some((up_bps, down_bps));
                    self.rate_history.push_back((up_bps, down_bps));
                    while self.rate_history.len() > RATE_HISTORY_LEN {
                        self.rate_history.pop_front();
                    }
                }
                ServiceEvent::Latency { profile_id, ms } => {
                    self.latency.insert(
                        profile_id,
                        LatencyState::Done(ms.map_or_else(|| Err(String::from("no reply")), Ok)),
                    );
                }
                ServiceEvent::LogLine(line) => util::push_log(&mut self.logs, line),
            },
            Message::Screen(s) => {
                self.screen = s;
                return match s {
                    Screen::Servers => self.load_profiles(),
                    Screen::Adapters => self.load_adapters(),
                    Screen::Routing => self.load_rules(),
                    _ => iced::Task::none(),
                };
            }
            Message::ConnectToggle(on) => {
                let cur = self
                    .status
                    .as_ref()
                    .map(|s| s.state)
                    .unwrap_or(ServiceState::Disconnected);
                let svc = Arc::clone(&self.service);
                if on && matches!(cur, ServiceState::Disconnected | ServiceState::Failed) {
                    return iced::Task::perform(
                        async move {
                            svc.connect().await;
                        },
                        |_| Message::Tick,
                    );
                } else if !on && matches!(cur, ServiceState::Connected | ServiceState::Connecting) {
                    // Connecting counts too (S6-REV-C P2-2): the service
                    // honors a disconnect request mid-connect (flag checked
                    // right after the link probe), so the switch cancels a
                    // pending connect instead of bouncing back silently.
                    return iced::Task::perform(
                        async move {
                            svc.disconnect().await;
                        },
                        |_| Message::Tick,
                    );
                }
            }
            Message::StatusLoaded(s) => {
                self.sample_adapter_rates(&s);
                self.status = Some(s);
            }
            Message::ProfilesLoaded(p) => self.profiles = p,
            Message::PasteChanged(t) => {
                self.paste = t;
                self.paste_preview = None;
                let trimmed = self.paste.trim();
                if trimmed.is_empty() {
                    return iced::Task::none();
                }
                self.paste_preview = Some(match mt_links::parse_share_link(trimmed) {
                    Ok(p) => Ok(ParsedPreview::from(&p)),
                    Err(e) => Err(e.to_string()),
                });
            }
            Message::PasteAdd => {
                let link = self.paste.trim().to_string();
                if link.is_empty() {
                    return iced::Task::none();
                }
                let svc = Arc::clone(&self.service);
                return iced::Task::perform(
                    async move { svc.import_link(&link).await },
                    Message::ImportDone,
                );
            }
            Message::PasteClear => {
                self.paste.clear();
                self.paste_preview = None;
                self.notice = None;
            }
            Message::ImportDone(res) => match res {
                Ok(pv) => {
                    self.notice = Some(format!("added server \"{}\"", pv.tag));
                    self.paste.clear();
                    self.paste_preview = None;
                    return self.load_profiles();
                }
                Err(e) => self.notice = Some(format!("import failed: {e}")),
            },
            Message::ActiveSet(id, res) => match res {
                Ok(()) => {
                    self.notice = Some("active server set".to_string());
                    return self.load_profiles();
                }
                Err(e) => self.notice = Some(format!("cannot set active {id}: {e}")),
            },
            Message::ProfileRemoved(id, res) => match res {
                Ok(()) => {
                    self.notice = Some("server removed".to_string());
                    return self.load_profiles();
                }
                Err(e) => self.notice = Some(format!("cannot remove {id}: {e}")),
            },
            Message::LatencyTest(id) => {
                self.latency.insert(id.clone(), LatencyState::InFlight);
                let svc = Arc::clone(&self.service);
                let id_for_fut = id.clone();
                let id_for_map = id;
                return iced::Task::perform(
                    async move { svc.test_latency(&id_for_fut).await },
                    move |res| Message::LatencyTested(id_for_map.clone(), res),
                );
            }
            Message::LatencyTested(id, res) => {
                self.latency.insert(id, LatencyState::Done(res));
            }
            Message::SetActive(id) => {
                let svc = Arc::clone(&self.service);
                return iced::Task::perform(
                    async move {
                        let res = svc.set_active_profile(&id).await;
                        (id, res)
                    },
                    |(id, res)| Message::ActiveSet(id, res),
                );
            }
            Message::RemoveProfile(id) => {
                let svc = Arc::clone(&self.service);
                return iced::Task::perform(
                    async move {
                        let res = svc.remove_profile(&id).await;
                        (id, res)
                    },
                    |(id, res)| Message::ProfileRemoved(id, res),
                );
            }
            Message::RulesLoaded(v) => {
                self.rules = v
                    .into_iter()
                    .map(|r| RuleRow {
                        kind: r.kind,
                        value: r.value,
                        action: r.action,
                    })
                    .collect();
                self.rules_saved = self.rules.clone();
                self.rules_status = None;
            }
            Message::RuleKind(i, k) => {
                if let Some(row) = self.rules.get_mut(i) {
                    row.kind = k;
                }
            }
            Message::RuleValue(i, v) => {
                if let Some(row) = self.rules.get_mut(i) {
                    row.value = v;
                }
            }
            Message::RuleAction(i, a) => {
                if let Some(row) = self.rules.get_mut(i) {
                    row.action = a;
                }
            }
            Message::RuleAdded => {
                self.rules.push(RuleRow {
                    kind: RULE_KINDS[0].to_string(),
                    value: String::new(),
                    action: RULE_ACTIONS[0].to_string(),
                });
            }
            Message::RuleRemoved(i) => {
                if i < self.rules.len() {
                    self.rules.remove(i);
                }
            }
            Message::RulesApply => {
                let svc = Arc::clone(&self.service);
                let rules = self.rules.clone();
                return iced::Task::perform(
                    async move {
                        svc.set_routing_rules(
                            rules
                                .into_iter()
                                .map(|r| mt_service::RuleView {
                                    kind: r.kind,
                                    value: r.value,
                                    action: r.action,
                                })
                                .collect(),
                        )
                        .await
                    },
                    Message::RulesSaved,
                );
            }
            Message::RulesSaved(res) => match res {
                Ok(()) => {
                    self.rules_saved = self.rules.clone();
                    self.rules_status = Some("rules saved".to_string());
                }
                Err(e) => self.rules_status = Some(format!("save failed: {e}")),
            },
            Message::RulesReverted => {
                self.rules = self.rules_saved.clone();
                self.rules_status = None;
            }
            Message::RulesImportPathChanged(t) => {
                self.rules_import_path = t;
            }
            Message::RulesImport => {
                let path = self.rules_import_path.trim().to_string();
                if path.is_empty() {
                    self.rules_status = Some("enter a file path first".to_string());
                    return iced::Task::none();
                }
                return iced::Task::perform(
                    async move {
                        let text = tokio::fs::read_to_string(&path)
                            .await
                            .map_err(|e| format!("cannot read {path:?}: {e}"))?;
                        Ok::<_, String>(mt_service::parse_exclusion_list(&text))
                    },
                    Message::RulesImported,
                );
            }
            Message::RulesImported(res) => match res {
                Ok(imp) => {
                    for (entry, reason) in &imp.skipped {
                        util::push_log(
                            &mut self.logs,
                            format!("WARN  routing import: skipped {entry:?} — {reason}"),
                        );
                    }
                    self.rules.extend(imp.rules.iter().map(|r| RuleRow {
                        kind: r.kind.clone(),
                        value: r.value.clone(),
                        action: r.action.clone(),
                    }));
                    self.rules_status =
                        Some(format!("{} — press Apply to activate", imp.summary()));
                }
                Err(e) => {
                    self.rules_status = Some(format!("import failed: {e}"));
                }
            },
            Message::SettingsLoaded(s) => {
                self.settings = s.clone();
                self.seed_text = s.theme_seed.to_string();
                self.theme.rederive(s.theme_seed, s.dark_mode);
                // S16: honor a persisted always-on-top now that settings
                // exist (the Opened path covers the reverse order).
                let top = s.always_on_top;
                if top && self.window_id.is_some() {
                    return self.apply_window_level(true);
                }
                // S11: the saved bonded order may have arrived after the
                // adapter rows (the init batch is concurrent) — re-sort
                // UNLESS a draft is in progress (never reshuffle edits).
                if !self.adapters_dirty() && !self.adapters.is_empty() {
                    sort_adapter_rows(&mut self.adapters, &s.bonded_adapters);
                }
            }
            Message::SeedSwatch(seed) => {
                self.settings.theme_seed = seed;
                self.seed_text = seed.to_string();
                self.theme.rederive(seed, self.settings.dark_mode);
                return self.save_settings(self.settings.clone());
            }
            Message::SeedFieldChanged(t) => self.seed_text = t,
            Message::SeedCommitted => match parse_seed(&self.seed_text) {
                Some(seed) => {
                    self.settings.theme_seed = seed;
                    self.theme.rederive(seed, self.settings.dark_mode);
                    return self.save_settings(self.settings.clone());
                }
                None => {
                    self.settings_status =
                        Some("invalid seed: use decimal or 0xRRGGBB".to_string());
                }
            },
            Message::DarkToggled(dark) => {
                self.settings.dark_mode = dark;
                self.theme.rederive(self.settings.theme_seed, dark);
                return self.save_settings(self.settings.clone());
            }
            Message::SystemProxyToggled(v) => {
                self.settings.system_proxy = v;
                return self.save_settings(self.settings.clone());
            }
            Message::MergePolicyPicked(p) => {
                if MERGE_POLICIES.contains(&p.as_str()) {
                    self.settings.merge_policy = p;
                    return self.save_settings(self.settings.clone());
                }
                self.settings_status = Some("unknown merge policy".to_string());
            }
            Message::AutostartToggled(v) => {
                self.settings.autostart = v;
                return self.save_settings(self.settings.clone());
            }
            Message::SettingsSaved(res) => match res {
                Ok(()) => self.settings_status = Some("settings saved".to_string()),
                Err(e) => self.settings_status = Some(format!("save failed: {e}")),
            },
            Message::LogFilterPicked(l) => self.log_filter = LogFilter::from_label(&l),
            Message::LogsCleared => self.logs.clear(),
            Message::AdaptersLoaded(views) => {
                // Fresh enumeration: the draft resets to the persisted
                // selection (an unsaved draft is discarded — same contract
                // as the Routing screen's reload). Expansion state follows
                // the rows (a vanished adapter's graph goes with it).
                // S11: rows land in PRIORITY order — selected adapters
                // first, in the saved bonded-adapter order, then the rest.
                let mut rows: Vec<AdapterRow> = views
                    .into_iter()
                    .map(|view| AdapterRow {
                        selected: view.selected,
                        expanded: false,
                        view,
                    })
                    .collect();
                sort_adapter_rows(&mut rows, &self.settings.bonded_adapters);
                self.adapters = rows;
            }
            Message::AdapterToggled(idx) => {
                // S11 invariant: the list always reads selected-block-first
                // (the block IS the priority order). Ticking ON appends at
                // the END of the block (new adapter = lowest priority);
                // ticking OFF drops it to the head of the unselected rest.
                if idx < self.adapters.len() {
                    let mut row = self.adapters.remove(idx);
                    row.selected = !row.selected;
                    let insert_at = self.adapters.iter().filter(|r| r.selected).count();
                    self.adapters.insert(insert_at, row);
                }
                self.adapters_status = None;
            }
            Message::AdapterExpanded(idx) => {
                if let Some(row) = self.adapters.get_mut(idx) {
                    row.expanded = !row.expanded;
                }
            }
            Message::AdapterMoved(idx, delta) => {
                // Swap with the previous/next SELECTED row — moving is a
                // priority-order operation, so unselected rows in between
                // are skipped, not displaced.
                let Some(row) = self.adapters.get(idx) else {
                    return iced::Task::none();
                };
                if !row.selected {
                    return iced::Task::none();
                }
                if delta < 0 {
                    if let Some(prev) = self.adapters[..idx].iter().rposition(|r| r.selected) {
                        self.adapters.swap(idx, prev);
                    }
                } else if let Some(next) = self.adapters[idx + 1..]
                    .iter()
                    .position(|r| r.selected)
                    .map(|i| i + idx + 1)
                {
                    self.adapters.swap(idx, next);
                }
                self.adapters_status = None;
            }
            Message::AdaptersApply => {
                let svc = Arc::clone(&self.service);
                let ids: Vec<String> = self
                    .adapters
                    .iter()
                    .filter(|r| r.selected)
                    .map(|r| r.view.id.clone())
                    .collect();
                self.adapters_status = Some("applying…".to_string());
                return iced::Task::perform(
                    async move { svc.set_bonded_adapters(ids).await },
                    Message::AdaptersSaved,
                );
            }
            Message::AdaptersSaved(result) => {
                match result {
                    Ok(()) => {
                        let n = self.adapters.iter().filter(|r| r.selected).count();
                        self.adapters_status = Some(if n == 0 {
                            "applied — no adapters selected (single link, OS picks the route)"
                                .to_string()
                        } else {
                            format!(
                                "applied — bonding across {n} adapter(s) in this priority \
                                 order; a live session was restarted"
                            )
                        });
                    }
                    Err(e) => self.adapters_status = Some(format!("apply failed: {e}")),
                }
                // Re-sync: SETTINGS first (the saved bonded order is the
                // sort key), then the rows (the service re-derived
                // `selected`), then status so the link list reflects the
                // new link set after the reconnect.
                return self
                    .load_settings()
                    .chain(self.load_adapters())
                    .chain(self.refresh_status());
            }
            Message::AdaptersReverted => {
                for row in &mut self.adapters {
                    row.selected = row.view.selected;
                }
                sort_adapter_rows(&mut self.adapters, &self.settings.bonded_adapters);
                self.adapters_status = None;
            }
            Message::AdaptersRefresh => {
                self.adapters_status = Some("refreshing…".to_string());
                return self.load_adapters();
            }
            Message::StatusServerPicked(tag) => {
                let Some(profile) = self.profiles.iter().find(|p| p.tag == tag).cloned() else {
                    return iced::Task::none();
                };
                let was_connected = matches!(
                    self.status.as_ref().map(|s| s.state),
                    Some(ServiceState::Connected)
                );
                let id = profile.id.clone();
                let svc = Arc::clone(&self.service);
                return iced::Task::perform(
                    async move {
                        svc.set_active_profile(&id).await?;
                        if was_connected {
                            svc.disconnect().await;
                            svc.connect().await;
                        }
                        Ok::<(), String>(())
                    },
                    |_| Message::Tick,
                );
            }
            Message::ExitProbe => {
                if self.exit_probing {
                    return iced::Task::none();
                }
                self.exit_probing = true;
                let svc = Arc::clone(&self.service);
                return iced::Task::perform(
                    async move { svc.exit_probe().await },
                    Message::ExitProbed,
                );
            }
            Message::ExitProbed(res) => {
                self.exit_probing = false;
                if let Err(e) = &res {
                    util::push_log(&mut self.logs, format!("WARN  exit probe: {e}"));
                }
                self.exit = Some(res);
            }
            Message::SocksPortChanged(t) => {
                return self.advanced_port_field(|st, v| st.socks_port = v, t, "SOCKS port");
            }
            Message::HttpPortChanged(t) => {
                return self.advanced_port_field(|st, v| st.http_port = v, t, "HTTP port");
            }
            Message::MergePortChanged(t) => {
                return self.advanced_port_field(|st, v| st.merge_port = v, t, "merge port");
            }
            Message::AllowLanToggled(on) => {
                self.settings.allow_lan = on;
                return self.save_settings(self.settings.clone());
            }
            Message::LogLevelPicked(level) => {
                self.settings.log_level = level;
                return self.save_settings(self.settings.clone());
            }
            Message::BondModePicked(mode) => {
                self.settings.bond_mode = mode;
                return self.save_settings(self.settings.clone());
            }
            Message::ProbeTargetChanged(t) => {
                self.settings.probe_target = t;
                return self.save_settings(self.settings.clone());
            }
            Message::ExitUrlChanged(t) => {
                self.settings.exit_probe_url = t;
                return self.save_settings(self.settings.clone());
            }
            // --- S15: title bar / tray / close-to-tray (D25) ---
            Message::WindowEvent(id, event) => {
                if self.window_id.is_none() {
                    if let iced::window::Event::Opened { .. } = event {
                        self.window_id = Some(id);
                        // S16: honor a persisted always-on-top once the
                        // window exists (Settings arrive later → the
                        // SettingsLoaded path covers the reverse order).
                        if self.settings.always_on_top {
                            return self.apply_window_level(true);
                        }
                    }
                }
                match event {
                    iced::window::Event::CloseRequested => {
                        return self.handle_close_requested();
                    }
                    iced::window::Event::Resized(size) => {
                        // S16: remember the size (context-menu clamping).
                        self.window_size = Some(size);
                        // Maximize state may have changed (OS restore,
                        // snap, our own toggle): re-query for the glyph.
                        if let Some(id) = self.window_id {
                            return iced::window::get_maximized(id).map(Message::WindowMaximized);
                        }
                    }
                    _ => {}
                }
            }
            Message::TitleDrag => {
                let now = std::time::Instant::now();
                let double = is_double_click(self.last_title_press.take(), now);
                self.last_title_press = Some(now);
                if let Some(id) = self.window_id {
                    if double {
                        return iced::Task::batch([
                            iced::window::toggle_maximize(id),
                            iced::window::get_maximized(id).map(Message::WindowMaximized),
                        ]);
                    }
                    return iced::window::drag(id);
                }
            }
            Message::TitleMinimize => {
                if let Some(id) = self.window_id {
                    return iced::window::minimize(id, true);
                }
            }
            Message::TitleMaximize => {
                if let Some(id) = self.window_id {
                    return iced::Task::batch([
                        iced::window::toggle_maximize(id),
                        iced::window::get_maximized(id).map(Message::WindowMaximized),
                    ]);
                }
            }
            Message::TitleClose => return self.handle_close_requested(),
            Message::WindowMaximized(m) => self.window_maximized = m,
            Message::CloseToTrayToggled(v) => {
                self.settings.close_to_tray = v;
                return self.save_settings(self.settings.clone());
            }
            Message::Tray(ev) => match ev {
                crate::tray::TrayEvent::Open => {
                    if let Some(id) = self.window_id {
                        return iced::Task::batch([
                            iced::window::run_with_handle(id, show_window).map(Message::Event),
                            iced::window::gain_focus(id),
                        ]);
                    }
                }
                crate::tray::TrayEvent::Connect => {
                    return self.update(Message::ConnectToggle(true));
                }
                crate::tray::TrayEvent::Disconnect => {
                    return self.update(Message::ConnectToggle(false));
                }
                crate::tray::TrayEvent::Server(id) => {
                    return self.update(Message::SetActive(id));
                }
                crate::tray::TrayEvent::Exit => return self.begin_exit(),
            },
            // --- S16: titlebar right-click window menu (D26) ---
            Message::CursorMoved(p) => self.cursor = Some(p),
            Message::TitleContext => {
                // Chromium-style: the menu opens AT the cursor, dropped
                // just below the 40 px titlebar strip; without a known
                // cursor (keyboard/edge case) it anchors top-right.
                let fallback =
                    iced::Point::new(self.window_size.map_or(900.0, |s| s.width - 260.0), 44.0);
                let raw = self.cursor.unwrap_or(fallback);
                let (mx, my) = Self::menu_bounds();
                let w = self.window_size.map_or(1080.0, |s| s.width);
                let h = self.window_size.map_or(720.0, |s| s.height);
                self.title_menu = Some(iced::Point::new(
                    raw.x.clamp(4.0, (w - mx - 8.0).max(4.0)),
                    raw.y.min(h - my - 8.0).max(44.0),
                ));
            }
            Message::TitleContextDismiss => self.title_menu = None,
            Message::TitleMenu(item) => {
                self.title_menu = None;
                return match item {
                    TitleMenuItem::AlwaysOnTop => {
                        self.set_always_on_top(!self.settings.always_on_top)
                    }
                    TitleMenuItem::Minimize => self.update(Message::TitleMinimize),
                    TitleMenuItem::Maximize => self.update(Message::TitleMaximize),
                    TitleMenuItem::Close => self.update(Message::TitleClose),
                };
            }
            Message::AlwaysOnTopToggled(v) => return self.set_always_on_top(v),
        }
        iced::Task::none()
    }

    // ------------------------------------------------------------------
    // S15: window close paths + tray sync (D25)
    // ------------------------------------------------------------------

    /// S16: the window menu's (width, height) — kept in one place so the
    /// TitleContext clamp and the screens.rs overlay agree.
    pub fn menu_bounds() -> (f32, f32) {
        (232.0, 196.0)
    }

    /// S16: apply the always-on-top window level (cross-platform iced
    /// action — HWND_TOPMOST on Windows, _NET_WM_STATE_ABOVE on X11).
    fn apply_window_level(&self, on_top: bool) -> iced::Task<Message> {
        match self.window_id {
            Some(id) => iced::window::change_level(
                id,
                if on_top {
                    iced::window::Level::AlwaysOnTop
                } else {
                    iced::window::Level::Normal
                },
            ),
            None => iced::Task::none(),
        }
    }

    /// S16: the one always-on-top mutation point (window menu + Settings
    /// both land here): persist, then apply the level.
    fn set_always_on_top(&mut self, v: bool) -> iced::Task<Message> {
        if self.settings.always_on_top == v {
            return iced::Task::none();
        }
        self.settings.always_on_top = v;
        util::push_log(
            &mut self.logs,
            format!(
                "INFO  gui: always on top {}",
                if v {
                    "on — the window stays above others"
                } else {
                    "off"
                }
            ),
        );
        let t = self.save_settings(self.settings.clone());
        iced::Task::batch([t, self.apply_window_level(v)])
    }

    /// The one close decision point (window ✕, OS close request, tray
    /// Exit all land here after their own pre-steps). Pure decision via
    /// [`close_action`].
    fn handle_close_requested(&mut self) -> iced::Task<Message> {
        let tray_alive = self.tray.is_some();
        match close_action(self.settings.close_to_tray, tray_alive) {
            CloseAction::Hide => {
                if let Some(id) = self.window_id {
                    util::push_log(
                        &mut self.logs,
                        "INFO  gui: hidden to tray — reopen from the tray icon \
                         (right-click it for Quick Settings)"
                            .to_string(),
                    );
                    return iced::window::run_with_handle(id, hide_window).map(Message::Event);
                }
            }
            CloseAction::Minimize => {
                util::push_log(
                    &mut self.logs,
                    "INFO  gui: close-to-tray is on but this platform has no tray \
                     — minimizing instead (the window stays reachable)"
                        .to_string(),
                );
                if let Some(id) = self.window_id {
                    return iced::window::minimize(id, true);
                }
            }
            CloseAction::Exit => return self.begin_exit(),
        }
        iced::Task::none()
    }

    /// Real exit: tray down first (icon + thread), then close the window
    /// (with `exit_on_close_request: false`, closing is OUR call).
    fn begin_exit(&mut self) -> iced::Task<Message> {
        if let Some(tray) = self.tray.take() {
            tray.shutdown(std::time::Duration::from_millis(600));
        }
        if let Some(id) = self.window_id {
            return iced::window::close(id);
        }
        iced::Task::none()
    }

    /// Push tooltip + Quick Settings model + icon style to the tray when
    /// they changed (called on tick; the tray renders the menu fresh at
    /// every open).
    fn sync_tray(&mut self) {
        let Some(tray) = &self.tray else { return };
        let state = self
            .status
            .as_ref()
            .map(|s| s.state)
            .unwrap_or(ServiceState::Disconnected);
        let active = self
            .status
            .as_ref()
            .and_then(|s| s.active_profile.clone())
            .unwrap_or_default();
        // S16: while connected the hover tooltip carries the live speeds
        // and session totals (the user's ask); other states keep the
        // plain word form.
        let tip = if state == ServiceState::Connected {
            let (up_bps, down_bps) = self.throughput.unwrap_or((0, 0));
            let (total_up, total_down) = self.session_bytes;
            crate::tray::tooltip_live(up_bps, down_bps, total_up, total_down)
        } else {
            crate::tray::tooltip(state, &active)
        };
        if tip != self.tray_tip {
            self.tray_tip = tip.clone();
            tray.send(crate::tray::TrayCommand::Tooltip(tip));
        }
        // S16: colored icon while connected, monochrome otherwise.
        let icon = crate::tray::TrayIcon::for_state(state);
        if icon != self.tray_icon {
            self.tray_icon = icon;
            tray.send(crate::tray::TrayCommand::Icon(icon));
        }
        let model = crate::tray::model_from(self.status.as_ref(), &self.profiles);
        if self.tray_model.as_ref() != Some(&model) {
            self.tray_model = Some(model.clone());
            tray.send(crate::tray::TrayCommand::Model(model));
        }
    }

    // ------------------------------------------------------------------
    // View + theme + subscriptions
    // ------------------------------------------------------------------

    /// The MVU view function (wired into `iced::application`).
    pub fn view(&self) -> Element<'_> {
        let content = match self.screen {
            Screen::Status => screens::status(self),
            Screen::Servers => screens::servers(self),
            Screen::Adapters => screens::adapters(self),
            Screen::Routing => screens::routing(self),
            Screen::Settings => screens::settings(self),
            Screen::Logs => screens::logs(self),
        };
        let rail = screens::nav_rail(self);
        let titlebar = screens::titlebar(self);
        let s = self.theme.scheme;
        let base = iced::widget::container(iced::widget::column![
            titlebar,
            iced::widget::row![rail, content]
        ])
        .style(crate::theme::page(s))
        .width(iced::Length::Fill)
        .height(iced::Length::Fill);
        // S16: the titlebar window menu renders as a full-window layer on
        // top of everything (stack later children draw above the base).
        let mut stack = iced::widget::stack![base];
        if self.title_menu.is_some() {
            stack = stack.push(screens::title_menu_overlay(self));
        }
        stack.into()
    }

    /// The window-level iced theme (custom palette from the seed).
    pub fn theme(&self) -> iced::Theme {
        self.theme.iced_theme()
    }

    /// Live subscriptions: service events bridge + 1 s tick.
    pub fn subscription(&self) -> iced::Subscription<Message> {
        let events = {
            let rx = Arc::clone(&self.events_rx);
            iced::Subscription::run_with_id(
                EVENTS_SUB_ID,
                iced::stream::channel(256, move |mut sender| async move {
                    use iced::futures::SinkExt;
                    loop {
                        let next = {
                            let mut guard = rx.lock().await;
                            guard.recv().await
                        };
                        match next {
                            Ok(ev) => {
                                let _ = sender.send(Message::Event(ev)).await;
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                let _ = sender
                                    .send(Message::Event(ServiceEvent::LogLine(format!(
                                        "WARN  gui: missed {n} service events"
                                    ))))
                                    .await;
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }),
            )
        };
        let tray_events = match &self.tray_rx {
            Some(rx) => {
                let rx = Arc::clone(rx);
                iced::Subscription::run_with_id(
                    TRAY_SUB_ID,
                    iced::stream::channel(32, move |mut sender| async move {
                        use iced::futures::SinkExt;
                        loop {
                            let next = {
                                let mut guard = rx.lock().await;
                                guard.recv().await
                            };
                            match next {
                                Some(ev) => {
                                    let _ = sender.send(Message::Tray(ev)).await;
                                }
                                None => break,
                            }
                        }
                    }),
                )
            }
            None => iced::Subscription::none(),
        };
        iced::Subscription::batch([
            events,
            tick_subscription(),
            // S15: window events (Open/CloseRequested/Resized) + the tray
            // event stream.
            iced::window::events().map(|(id, ev)| Message::WindowEvent(id, ev)),
            // S16: cursor tracking (anchors the titlebar window menu).
            iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::CursorMoved(position))
                }
                _ => None,
            }),
            tray_events,
        ])
    }

    /// Whether the connect switch should read "on".
    pub fn is_connected(&self) -> bool {
        matches!(
            self.status.as_ref().map(|s| s.state),
            Some(ServiceState::Connected)
        )
    }

    /// S11: is the Adapters draft different from what is persisted?
    /// Dirty = the selected SET differs OR the selected ORDER differs
    /// from `settings.bonded_adapters` (restricted to adapters present in
    /// the current enumeration — a vanished id cannot be re-ordered).
    pub fn adapters_dirty(&self) -> bool {
        let draft: Vec<String> = self
            .adapters
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.view.id.clone())
            .collect();
        let saved: Vec<String> = self
            .settings
            .bonded_adapters
            .iter()
            .filter(|id| self.adapters.iter().any(|r| &r.view.id == *id))
            .cloned()
            .collect();
        draft != saved
    }

    /// S11: the priority position (1-based) of adapter `id` in the SAVED
    /// bonded order — `None` when the adapter is not bonded. Drives the
    /// position chips on the Adapters and Status screens.
    pub fn priority_position_of(&self, id: &str) -> Option<usize> {
        self.settings
            .bonded_adapters
            .iter()
            .position(|saved| saved == id)
            .map(|p| p + 1)
    }
}

/// Stable subscription ids (iced deduplicates by id per render).
const EVENTS_SUB_ID: &str = "mt-app-events";
/// S15: the tray event stream subscription id.
const TRAY_SUB_ID: &str = "mt-app-tray";

/// What a close request should do (pure decision, D25 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Windows + tray alive: hide the window, keep the app in the tray.
    Hide,
    /// close-to-tray on a platform without a tray: minimize (the window
    /// stays reachable — hiding it would orphan the app).
    Minimize,
    /// Really exit (tray down + window close).
    Exit,
}

/// The close-request decision: only Windows-with-a-live-tray can truly
/// hide; everywhere else close-to-tray degrades to minimize; off = exit.
pub fn close_action(close_to_tray: bool, tray_alive: bool) -> CloseAction {
    if !close_to_tray {
        return CloseAction::Exit;
    }
    if tray_alive && cfg!(windows) {
        CloseAction::Hide
    } else {
        CloseAction::Minimize
    }
}

/// Titlebar double-click window (a bit above the OS default 500 ms to be
/// forgiving over a drag that never started).
const DOUBLE_CLICK_MS: u64 = 450;

/// Pure double-press detector for the titlebar strip.
pub fn is_double_click(last: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last.is_some_and(|t| now.duration_since(t) <= std::time::Duration::from_millis(DOUBLE_CLICK_MS))
}

/// Hide the main window via its native handle (run on the iced runtime
/// through `window::run_with_handle`; Windows-only effect — on other
/// platforms the caller already chose Minimize).
fn hide_window(handle: iced::window::raw_window_handle::WindowHandle<'_>) -> ServiceEvent {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        if let iced::window::raw_window_handle::RawWindowHandle::Win32(w) = handle.as_raw() {
            // SAFETY: w.hwnd is the live main window's handle handed to us
            // by the iced runtime; SW_HIDE is a plain visibility toggle.
            unsafe { ShowWindow(w.hwnd.get() as _, SW_HIDE) };
        }
    }
    #[cfg(not(windows))]
    {
        // Unreachable in practice (the caller chose Minimize on this
        // platform); kept for cross-platform compilation of the closure.
        let _ = &handle;
    }
    ServiceEvent::LogLine("INFO  gui: window hidden to tray".to_string())
}

/// Restore the main window from the tray (inverse of [`hide_window`]).
fn show_window(handle: iced::window::raw_window_handle::WindowHandle<'_>) -> ServiceEvent {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW};
        if let iced::window::raw_window_handle::RawWindowHandle::Win32(w) = handle.as_raw() {
            // SAFETY: see hide_window; SW_SHOW restores without focus
            // stealing rules (gain_focus follows separately).
            unsafe { ShowWindow(w.hwnd.get() as _, SW_SHOW) };
        }
    }
    #[cfg(not(windows))]
    {
        // Unreachable in practice (the tray is Windows-only); kept for
        // cross-platform compilation of the closure.
        let _ = &handle;
    }
    ServiceEvent::LogLine("INFO  gui: window restored from tray".to_string())
}

/// 1 s refresh tick subscription.
fn tick_subscription() -> iced::Subscription<Message> {
    iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
}

/// Parses the numeric seed field: decimal or 0x-prefixed hex.
pub fn parse_seed(s: &str) -> Option<u32> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<u32>().ok()
    }
}

/// D20: does the status link `link_name` belong to the adapter named
/// `adapter_name`?
///
/// Engine tags (both per-connection links and stream carriers) are
/// `"<profile-tag> [<adapter-name>]"`, where the bracketed part carries a
/// ` #n` suffix when two bonded adapters share a friendly name
/// (`engine_specs_for` dedup). The matcher accepts both shapes. The
/// unbound single link (no brackets) matches nothing — no adapter row owns
/// it. Matching is by the LAST ` [` so a profile tag containing brackets
/// cannot smuggle a match.
pub fn link_matches_adapter(link_name: &str, adapter_name: &str) -> bool {
    let Some((_, rest)) = link_name.rsplit_once(" [") else {
        return false;
    };
    let Some(bracketed) = rest.strip_suffix(']') else {
        return false;
    };
    let adapter_name = adapter_name.trim();
    if adapter_name.is_empty() {
        return false;
    }
    bracketed == adapter_name || bracketed.starts_with(&format!("{adapter_name} #"))
}

/// S11: sort adapter rows into DISPLAY order — SELECTED rows first, in
/// the saved bonded-adapter (priority) order, then the unselected rows in
/// enumeration order. Pure + stable (unselected rows keep their relative
/// order; a selected id missing from `saved_order` — a fresh draft tick —
/// sorts last within the selected block, matching the toggle's
/// append-at-end invariant).
pub fn sort_adapter_rows(rows: &mut [AdapterRow], saved_order: &[String]) {
    rows.sort_by_key(|row| {
        if row.selected {
            (
                0,
                saved_order
                    .iter()
                    .position(|id| id == &row.view.id)
                    .unwrap_or(usize::MAX),
            )
        } else {
            (1, 0)
        }
    });
}

/// Launches the GUI with the given service backend (the root crate calls
/// this with its `RealService`; the `mt-app` binary passes a `MockService`).
pub fn run_app(service: Arc<dyn MtService>, flags: Flags) -> Result<(), crate::Error> {
    iced::application("MultiTunnel", App::update, App::view)
        .window(iced::window::Settings {
            size: iced::Size::new(1080.0, 720.0),
            min_size: Some(iced::Size::new(880.0, 580.0)),
            // S15 (D25): Chromium-style blended title bar — no native
            // decorations; the app draws its own strip (screens::titlebar).
            decorations: false,
            // S15: the app OWNS the close path (close-to-tray decides
            // between hide/minimize/exit in handle_close_requested).
            exit_on_close_request: false,
            icon: Some(window_icon()),
            ..Default::default()
        })
        .theme(App::theme)
        .subscription(App::subscription)
        .run_with(move || App::init(service, flags))?;
    Ok(())
}

/// The embedded 64×64 app logo (RGBA8; `scripts/make_logo_assets.py`).
fn window_icon() -> iced::window::Icon {
    const RGBA: &[u8] = include_bytes!("../../../assets/logo_64.rgba");
    iced::window::icon::from_rgba(RGBA.to_vec(), 64, 64)
        .expect("embedded logo_64.rgba is a 64x64 RGBA buffer (asset invariant)")
}

/// Headless smoke run: drives `ticks` update ticks (each pumps events,
/// refreshes status, rotates through every screen's view) without opening a
/// window. iced 0.13 cannot create a display-less window, so this exercises
/// model + update + view construction only; the CI gate asserts exit 0.
pub fn run_smoke(
    app: &mut App,
    ticks: usize,
    tick_sleep: Duration,
) -> Result<SmokeReport, crate::Error> {
    let mut events_seen = 0usize;
    let mut views_built = 0usize;
    for i in 0..ticks {
        events_seen += app.pump_events();
        drop(app.update(Message::Tick));
        let screen = Screen::ALL[i % Screen::ALL.len()];
        drop(app.update(Message::Screen(screen)));
        drop(app.view());
        views_built += 1;
        if !tick_sleep.is_zero() {
            std::thread::sleep(tick_sleep);
        }
    }
    // Final status sanity: the app must hold a loaded snapshot.
    if app.status.is_none() {
        return Err(crate::Error::NoBackend);
    }
    Ok(SmokeReport {
        ticks,
        events_seen,
        views_built,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--smoke` flag parsing.
    #[test]
    fn smoke_flag_parse() {
        assert!(Flags::from_args(["mt-app", "--smoke"]).smoke);
        assert!(!Flags::from_args(["mt-app"]).smoke);
        assert!(!Flags::from_args(["--smokeful"]).smoke);
        assert!(!Flags::from_args(Vec::<String>::new()).smoke);
    }

    /// Numeric seed parsing (decimal + hex, garbage rejected).
    #[test]
    fn seed_parse() {
        assert_eq!(parse_seed("123"), Some(123));
        assert_eq!(parse_seed(" 123 "), Some(123));
        assert_eq!(parse_seed("0x6750A4"), Some(0x6750A4));
        assert_eq!(parse_seed("0XFF"), Some(0xFF));
        assert_eq!(parse_seed(" 4294967295 "), Some(u32::MAX));
        assert_eq!(
            parse_seed("6750A4"),
            None,
            "bare hex without 0x is rejected"
        );
        assert_eq!(parse_seed("-1"), None);
        assert_eq!(parse_seed("nope"), None);
    }

    /// D20: engine-tag → adapter matching (the per-interface graph's data
    /// router). Covers the plain shape, the dedup `#n` shape, the unbound
    /// link, name-prefix traps and bracket-in-tag traps.
    #[test]
    fn link_matches_adapter_shapes() {
        // Plain per-adapter link / carrier name.
        assert!(link_matches_adapter("gcp-vm [Wi-Fi]", "Wi-Fi"));
        assert!(link_matches_adapter("gcp-vm [Ethernet]", "Ethernet"));
        // Dedup suffix when two adapters share a friendly name.
        assert!(link_matches_adapter("gcp-vm [Ethernet #2]", "Ethernet"));
        // A DIFFERENT adapter must not match (no prefix smuggling:
        // "Ethernet 2" is not "Ethernet #2").
        assert!(!link_matches_adapter("gcp-vm [Ethernet 2]", "Ethernet"));
        assert!(!link_matches_adapter("gcp-vm [Wi-Fi Direct]", "Wi-Fi"));
        assert!(!link_matches_adapter("gcp-vm [Ethernet]", "Wi-Fi"));
        // The unbound single link (no adapter selected) owns no row.
        assert!(!link_matches_adapter("gcp-vm", "gcp-vm"));
        assert!(!link_matches_adapter("gcp-vm", "Ethernet"));
        // Brackets inside the PROFILE tag must not fake a match: the LAST
        // " [" is the adapter part.
        assert!(link_matches_adapter("weird [tag] [Ethernet]", "Ethernet"));
        assert!(!link_matches_adapter("weird [tag] [Ethernet]", "tag]"));
        // Empty adapter names never match.
        assert!(!link_matches_adapter("gcp-vm []", ""));
        assert!(!link_matches_adapter("gcp-vm [x]", "  "));
        // Unterminated bracket (defensive) matches nothing.
        assert!(!link_matches_adapter("gcp-vm [Ethernet", "Ethernet"));
    }

    /// D20: per-interface rate sampling — cumulative counters in, one
    /// (up, down) bytes/second sample per adapter row out; counter resets
    /// and vanished links degrade to a zero sample instead of a spike.
    #[cfg(feature = "mock")]
    #[test]
    fn adapter_rate_sampling_deltas_and_resets() {
        let mut app = App::new(Arc::new(crate::mock::MockService::new()), Flags::default());
        app.adapters = vec![
            AdapterRow {
                view: mt_service::AdapterView {
                    id: "eth-id".into(),
                    name: "Ethernet".into(),
                    description: String::new(),
                    ipv4: vec!["192.168.1.5".into()],
                    up: true,
                    selected: true,
                },
                selected: true,
                expanded: false,
            },
            AdapterRow {
                view: mt_service::AdapterView {
                    id: "wifi-id".into(),
                    name: "Wi-Fi".into(),
                    description: String::new(),
                    ipv4: vec!["192.168.1.6".into()],
                    up: true,
                    selected: true,
                },
                selected: true,
                expanded: false,
            },
        ];

        let snap = |eth: (u64, u64), wifi: (u64, u64)| StatusSnapshot {
            state: mt_service::ServiceState::Connected,
            active_profile: Some("gcp-vm".into()),
            up_since: None,
            restarts: 0,
            links: vec![
                mk_link("gcp-vm [Ethernet]", eth),
                mk_link("gcp-vm [Wi-Fi]", wifi),
            ],
        };
        fn mk_link(name: &str, bytes: (u64, u64)) -> mt_service::LinkStatus {
            mt_service::LinkStatus {
                name: name.into(),
                ok: true,
                latency_ms: None,
                ewma_ms: None,
                quarantined: false,
                bytes_up: bytes.0,
                bytes_down: bytes.1,
            }
        }

        // t0: first sight → zero sample (no fake spike).
        app.sample_adapter_rates(&snap((1000, 5000), (200, 900)));
        assert_eq!(
            app.adapter_rates["eth-id"].back(),
            Some(&(0, 0)),
            "first sample is zero"
        );
        // t1: +1 s counters → the delta is the rate.
        app.sample_adapter_rates(&snap((4000, 9000), (1200, 1900)));
        assert_eq!(app.adapter_rates["eth-id"].back(), Some(&(3000, 4000)));
        assert_eq!(app.adapter_rates["wifi-id"].back(), Some(&(1000, 1000)));
        // t2: counter RESET (engine respawn) saturates to zero, not u64 wrap.
        app.sample_adapter_rates(&snap((10, 10), (1300, 2900)));
        assert_eq!(app.adapter_rates["eth-id"].back(), Some(&(0, 0)));
        assert_eq!(app.adapter_rates["wifi-id"].back(), Some(&(100, 1000)));
        // t3: WiFi link GONE (adapter unticked + reconnect) → base cleared,
        // the surviving link keeps sampling; the vanished row goes flat-zero.
        let snap_eth_only = |eth: (u64, u64)| StatusSnapshot {
            state: mt_service::ServiceState::Connected,
            active_profile: Some("gcp-vm".into()),
            up_since: None,
            restarts: 0,
            links: vec![mk_link("gcp-vm [Ethernet]", eth)],
        };
        app.sample_adapter_rates(&snap_eth_only((5000, 12000)));
        assert_eq!(
            app.link_last_bytes.len(),
            1,
            "vanished link dropped from the delta base"
        );
        assert_eq!(app.adapter_rates["eth-id"].back(), Some(&(4990, 11990)));
        assert_eq!(app.adapter_rates["wifi-id"].back(), Some(&(0, 0)));
        // Series length is bounded by the graph window.
        for i in 0u64..(RATE_HISTORY_LEN as u64 + 20) {
            app.sample_adapter_rates(&snap_eth_only((5000 + i, 12000 + i)));
        }
        assert!(app.adapter_rates["eth-id"].len() <= RATE_HISTORY_LEN);
        assert!(app.adapter_rates["wifi-id"].len() <= RATE_HISTORY_LEN);
    }

    /// Rule editor model: add, edit, remove, dirty detection.
    #[test]
    fn rule_editor_add_edit_remove() {
        let saved = vec![RuleRow {
            kind: "domainSuffix".into(),
            value: "example.org".into(),
            action: "proxy".into(),
        }];
        let mut rules = saved.clone();
        rules.push(RuleRow {
            kind: RULE_KINDS[1].to_string(),
            value: "ads".to_string(),
            action: RULE_ACTIONS[2].to_string(),
        });
        assert!(rules != saved, "editor is dirty after add");
        rules[1].value = "trackers".to_string();
        assert_eq!(rules[1].value, "trackers");
        rules.remove(1);
        assert_eq!(rules, saved, "revert to canonical after remove");
    }

    /// fmt helpers (chip labels).
    #[test]
    fn fmt_helpers() {
        assert_eq!(util::fmt_bps(0), "0 B/s");
        assert_eq!(util::fmt_bps(1500), "1.5 KB/s");
        assert_eq!(util::fmt_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(util::fmt_ms(84), "84 ms");
        assert_eq!(util::short("abcdef", 4), "abc…");
        assert_eq!(util::short("abc", 4), "abc");
    }

    /// Log filter prefix matching.
    #[test]
    fn log_filter_matches() {
        assert!(LogFilter::All.matches("anything"));
        assert!(LogFilter::Info.matches("INFO  core: tunnel up"));
        assert!(!LogFilter::Warning.matches("INFO  core: tunnel up"));
        assert!(LogFilter::Warning.matches("WARN  core: link degraded"));
        assert!(LogFilter::Error.matches("ERROR core: handshake failed"));
    }

    /// Smoke run against the mock: init → tick loop → all screen views built.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn smoke_run_drives_app_with_mock() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags { smoke: true }).await;
        assert!(app.status.is_some());
        assert_eq!(app.profiles.len(), 3, "mock seeds three synthetic profiles");
        assert_eq!(
            app.adapters.len(),
            8,
            "mock seeds the 3-NIC bond plus five virtual adapters (S13)"
        );

        // One tick per screen: every view (incl. Adapters) gets built.
        let report =
            run_smoke(&mut app, Screen::ALL.len(), Duration::from_millis(10)).expect("smoke ok");
        assert_eq!(report.ticks, Screen::ALL.len());
        assert_eq!(report.views_built, Screen::ALL.len());
        assert!(app.status.is_some());
    }

    /// Adapters screen flow (S7 + S11): toggle → priority REORDER → apply
    /// → rows re-sync to the persisted selection AND order; revert drops a
    /// dirty draft (selection and order).
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn adapters_screen_toggle_move_apply_revert() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;

        // Seed (D23 + S13): the THREE wired adapters bonded, saved order
        // wifi → ethernet → cellular (the mock demo setup), plus five
        // virtual adapters that stay unticked (enumeration order below
        // the selected block).
        const UNTICKED: [&str; 5] = [
            "mock-bluetooth",
            "mock-vethernet-wsl",
            "mock-tailscale",
            "mock-loopback",
            "mock-virtualbox",
        ];
        assert_eq!(app.adapters.len(), 8);
        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
        let mut expected = vec!["mock-wifi", "mock-ethernet", "mock-cellular"];
        expected.extend_from_slice(&UNTICKED);
        assert_eq!(ids, expected);
        assert!(app.adapters.iter().take(3).all(|r| r.selected));
        assert!(app.adapters.iter().skip(3).all(|r| !r.selected));
        assert!(!app.adapters_dirty(), "fresh load matches the saved order");
        assert_eq!(app.priority_position_of("mock-ethernet"), Some(2));
        assert_eq!(app.priority_position_of("mock-cellular"), Some(3));
        assert_eq!(app.priority_position_of("nope"), None);

        // Untick cellular (draft dirty, not persisted): it drops out of the
        // selected block but keeps its row position here (it was last).
        drop(app.update(Message::AdapterToggled(2)));
        assert!(!app.adapters[2].selected);
        assert!(app.adapters[2].view.selected, "draft only — not applied");
        assert!(app.adapters_dirty());

        // Re-tick cellular: it must APPEND at the END of the selected
        // block (a re-admitted adapter gets the lowest priority).
        drop(app.update(Message::AdapterToggled(2)));
        assert!(app.adapters[2].selected);
        assert!(!app.adapters_dirty(), "back to the saved set and order");

        // Untick ethernet: its row leaves the block (index moves!).
        drop(app.update(Message::AdapterToggled(1)));
        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
        let mut expected = vec!["mock-wifi", "mock-cellular", "mock-ethernet"];
        expected.extend_from_slice(&UNTICKED);
        assert_eq!(
            ids, expected,
            "unselected row sinks below the selected block"
        );
        // Re-tick ethernet (now index 2): appends at the END of the block.
        drop(app.update(Message::AdapterToggled(2)));
        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
        let mut expected = vec!["mock-wifi", "mock-cellular", "mock-ethernet"];
        expected.extend_from_slice(&UNTICKED);
        assert_eq!(
            ids, expected,
            "re-ticked adapter lands at the lowest priority"
        );
        assert!(app.adapters_dirty(), "order differs from the saved one");

        // REORDER: move Wi-Fi (index 0) DOWN — swaps with cellular.
        drop(app.update(Message::AdapterMoved(0, 1)));
        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
        let mut expected = vec!["mock-cellular", "mock-wifi", "mock-ethernet"];
        expected.extend_from_slice(&UNTICKED);
        assert_eq!(ids, expected);
        // Edge moves are no-ops (Wi-Fi is at the top of the block).
        drop(app.update(Message::AdapterMoved(0, -1)));
        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
        assert_eq!(ids, expected);

        // Apply persists the new order (service call directly — the
        // established test idiom; the update task only wraps it in
        // Task::perform). NOTE: the real AdaptersApply filters to
        // SELECTED rows only — the unticked virtual adapters must NOT
        // land in bonded_adapters (this test idiom used to send every
        // row id, which the old all-bonded 3-seed could never expose).
        // The saved-handler chains settings + adapters + status reloads;
        // drive those service calls and feed the results back in the
        // same order.
        let draft: Vec<String> = app
            .adapters
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.view.id.clone())
            .collect();
        app.service.set_bonded_adapters(draft).await.expect("apply");
        drop(app.update(Message::AdaptersSaved(Ok(()))));
        let settings = app.service.settings().await;
        drop(app.update(Message::SettingsLoaded(settings)));
        let views = app.service.adapters().await;
        drop(app.update(Message::AdaptersLoaded(views)));
        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
        let mut expected = vec!["mock-cellular", "mock-wifi", "mock-ethernet"];
        expected.extend_from_slice(&UNTICKED);
        assert_eq!(ids, expected, "applied order survives the reload");
        assert!(!app.adapters_dirty());
        assert_eq!(app.priority_position_of("mock-cellular"), Some(1));
        assert!(app
            .adapters_status
            .as_deref()
            .unwrap_or("")
            .contains("priority"));

        // Revert drops an unsaved draft (selection AND order).
        drop(app.update(Message::AdapterToggled(2))); // untick ethernet
        drop(app.update(Message::AdapterMoved(0, 1))); // cellular below wifi
        drop(app.update(Message::AdaptersReverted));
        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
        assert_eq!(
            ids, expected,
            "revert restores the saved selection and order"
        );
        assert!(app.adapters.iter().take(3).all(|r| r.selected));
        assert!(app.adapters.iter().skip(3).all(|r| !r.selected));
    }

    /// S11: `sort_adapter_rows` (the load/revert ordering primitive).
    #[cfg(feature = "mock")]
    #[test]
    fn sort_adapter_rows_selected_first_in_saved_order() {
        let views = crate::mock::seed_adapters_public();
        let saved = vec!["mock-cellular".to_string(), "mock-wifi".to_string()];
        let mut rows: Vec<AdapterRow> = views
            .into_iter()
            .map(|view| AdapterRow {
                selected: saved.contains(&view.id),
                expanded: false,
                view,
            })
            .collect();
        sort_adapter_rows(&mut rows, &saved);
        let ids: Vec<&str> = rows.iter().map(|r| r.view.id.as_str()).collect();
        let mut expected = vec!["mock-cellular", "mock-wifi", "mock-ethernet"];
        expected.extend_from_slice(&[
            "mock-bluetooth",
            "mock-vethernet-wsl",
            "mock-tailscale",
            "mock-loopback",
            "mock-virtualbox",
        ]);
        assert_eq!(
            ids, expected,
            "selected block first in SAVED order, rest in enumeration order"
        );
    }

    /// Live re-theme: dispatching settings messages re-derives the theme.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn theme_reroutes_on_settings_messages() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        let seed_before = app.theme.palette.seed();

        drop(app.update(Message::SeedSwatch(0xEA4335)));
        assert_eq!(app.theme.palette.seed(), 0xEA4335);
        assert_ne!(app.theme.palette.seed(), seed_before);
        assert_eq!(app.settings.theme_seed, 0xEA4335);
        assert_eq!(app.seed_text, "15352629");

        drop(app.update(Message::SeedFieldChanged("0x42f45b".to_string())));
        drop(app.update(Message::SeedCommitted));
        assert_eq!(app.theme.palette.seed(), 0x42F45B);

        drop(app.update(Message::DarkToggled(true)));
        assert!(app.theme.dark);
        assert!(app.settings.dark_mode);

        // Invalid seed commit leaves the theme untouched.
        drop(app.update(Message::SeedFieldChanged("nope".to_string())));
        drop(app.update(Message::SeedCommitted));
        assert_eq!(app.theme.palette.seed(), 0x42F45B);
        assert!(app
            .settings_status
            .as_deref()
            .unwrap_or_default()
            .contains("invalid seed"));
    }

    /// GUI update loop over the mock service: screen switches, the rules
    /// editor round-trips through the service, log events land, and the
    /// connect lifecycle is observable through the event pump.
    ///
    /// NOTE: `Task`s returned by `update` run on the iced runtime in
    /// production; here they are dropped, so service round-trips are driven
    /// by awaiting the service directly and re-entering `update` with the
    /// same messages the task mappers would produce.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn update_loop_screens_and_rules() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;

        drop(app.update(Message::Screen(Screen::Routing)));
        assert_eq!(app.screen, Screen::Routing);
        let loaded = app.service.routing_rules().await;
        drop(app.update(Message::RulesLoaded(loaded)));
        assert_eq!(app.rules.len(), 4, "rules loaded");

        drop(app.update(Message::RuleAdded));
        assert_eq!(app.rules.len(), 5);
        assert_ne!(app.rules, app.rules_saved, "editor is dirty after add");
        drop(app.update(Message::RuleValue(4, "example.net".to_string())));

        // What `Message::RulesApply`'s task would do, then its result.
        let res = app
            .service
            .set_routing_rules(
                app.rules
                    .iter()
                    .map(|r| mt_service::RuleView {
                        kind: r.kind.clone(),
                        value: r.value.clone(),
                        action: r.action.clone(),
                    })
                    .collect(),
            )
            .await;
        drop(app.update(Message::RulesSaved(res)));
        assert_eq!(app.rules_saved, app.rules);
        assert_eq!(app.rules_saved[4].value, "example.net");
        assert_eq!(app.service.routing_rules().await.len(), 5);

        drop(app.update(Message::RuleRemoved(4)));
        let res = app
            .service
            .set_routing_rules(
                app.rules
                    .iter()
                    .map(|r| mt_service::RuleView {
                        kind: r.kind.clone(),
                        value: r.value.clone(),
                        action: r.action.clone(),
                    })
                    .collect(),
            )
            .await;
        drop(app.update(Message::RulesSaved(res)));

        // Event plumbing: a LogLine event reaches the buffer + filter.
        drop(app.update(Message::Event(mt_service::ServiceEvent::LogLine(
            "ERROR test: boom".to_string(),
        ))));
        assert!(app.logs.iter().any(|l| l.contains("boom")));
        drop(app.update(Message::LogFilterPicked("error".to_string())));
        assert_eq!(app.log_filter, LogFilter::Error);
        assert!(
            app.logs
                .iter()
                .find(|l| app.log_filter.matches(l))
                .is_some(),
            "filtered view is non-empty"
        );
        drop(app.update(Message::LogsCleared));
        assert!(app.logs.is_empty());

        // Connect lifecycle observed through the event pump.
        app.service.connect().await;
        tokio::time::sleep(Duration::from_millis(900)).await;
        assert!(app.pump_events() > 0, "state/link/log events pumped");
        app.status = Some(app.service.status().await);
        assert_eq!(
            app.status.as_ref().map(|s| s.state),
            Some(mt_service::ServiceState::Connected)
        );
        assert!(app.is_connected());
    }

    /// S15: the close-request decision table (D25 §3). The platform arm
    /// is compiled per-target; this test pins the reachable arms.
    #[test]
    fn close_action_decision() {
        // Off (the default) always really closes.
        assert_eq!(close_action(false, false), CloseAction::Exit);
        assert_eq!(close_action(false, true), CloseAction::Exit);
        // On: Windows + live tray hides; everything else minimizes.
        if cfg!(windows) {
            assert_eq!(close_action(true, true), CloseAction::Hide);
        }
        assert_eq!(close_action(true, false), CloseAction::Minimize);
        if !cfg!(windows) {
            assert_eq!(close_action(true, true), CloseAction::Minimize);
        }
    }

    /// S15: titlebar double-press detection window.
    #[test]
    fn titlebar_double_click_window() {
        let now = std::time::Instant::now();
        assert!(!is_double_click(None, now), "first press is never a double");
        assert!(is_double_click(
            Some(now - std::time::Duration::from_millis(100)),
            now
        ));
        assert!(
            is_double_click(Some(now - std::time::Duration::from_millis(449)), now),
            "inside the window"
        );
        assert!(
            !is_double_click(Some(now - std::time::Duration::from_millis(600)), now),
            "outside the window"
        );
    }

    /// S15: close_to_tray persists through the service (settings roundtrip
    /// through the mock backend, same pattern as the other toggles).
    #[tokio::test]
    async fn close_to_tray_settings_roundtrip() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut s = service.settings().await;
        assert!(!s.close_to_tray, "default off");
        s.close_to_tray = true;
        service.set_settings(s).await.expect("save");
        let reloaded = service.settings().await;
        assert!(reloaded.close_to_tray);
    }

    /// S15: `begin_exit` releases the tray handle idempotently (the tray
    /// is None on this platform's test build; the take() path must still
    /// be safe to run twice).
    #[tokio::test]
    async fn begin_exit_is_idempotent_without_tray() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        drop(app.begin_exit());
        drop(app.begin_exit());
    }

    // --- S16: session totals, window-menu anchor, always-on-top --------------

    /// S16: session byte totals accumulate per-link deltas, count a
    /// counter RESET as its full new value (no dip), and zero when the
    /// tunnel leaves Connected.
    #[test]
    fn session_totals_accumulate_and_reset() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::new(service, Flags::default());
        let snap = |eth: (u64, u64)| StatusSnapshot {
            state: mt_service::ServiceState::Connected,
            active_profile: Some("gcp-vm".into()),
            up_since: None,
            restarts: 0,
            links: vec![mt_service::LinkStatus {
                name: "gcp-vm [Ethernet]".into(),
                ok: true,
                latency_ms: None,
                ewma_ms: None,
                quarantined: false,
                bytes_up: eth.0,
                bytes_down: eth.1,
            }],
        };
        // first sight: zero.
        app.sample_adapter_rates(&snap((1000, 5000)));
        assert_eq!(app.session_bytes, (0, 0));
        // +3 KB up / +4 KB down.
        app.sample_adapter_rates(&snap((4000, 9000)));
        assert_eq!(app.session_bytes, (3000, 4000));
        // counter reset (engine respawn): the full 10/10 counts.
        app.sample_adapter_rates(&snap((10, 10)));
        assert_eq!(app.session_bytes, (3010, 4010));
        // leaving Connected wipes the session totals (next session is
        // fresh, like the graphs).
        drop(app.update(Message::Event(ServiceEvent::StateChanged(
            ServiceState::Disconnected,
        ))));
        assert_eq!(app.session_bytes, (0, 0));
    }

    /// S16: the window menu anchor clamps to the window and drops below
    /// the 40 px titlebar strip; a missing cursor falls back top-right.
    #[test]
    fn title_menu_anchor_clamps() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::new(service, Flags::default());
        app.window_size = Some(iced::Size::new(1080.0, 720.0));

        // Cursor near the right edge: the 232 px menu is pulled left.
        drop(app.update(Message::CursorMoved(iced::Point::new(1070.0, 20.0))));
        drop(app.update(Message::TitleContext));
        let a = app.title_menu.expect("menu open");
        assert_eq!(a.y, 44.0, "drops below the strip");
        assert!(a.x <= 1080.0 - 232.0 - 8.0 + 0.5, "clamped right: {}", a.x);
        assert!(a.x >= 4.0);

        // No cursor at all: falls back to a sane top-right anchor.
        app.cursor = None;
        drop(app.update(Message::TitleContext));
        let b = app.title_menu.expect("menu open");
        assert!(b.x >= 700.0, "fallback anchors right: {}", b.x);
        assert_eq!(b.y, 44.0);

        // Dismiss closes it.
        drop(app.update(Message::TitleContextDismiss));
        assert!(app.title_menu.is_none());
    }

    /// S16: the always-on-top menu item flips the setting and closes the
    /// menu; the Settings toggler flips it back; the view persists
    /// through the service (the update()'s Task save is runtime-driven,
    /// so persistence is proven directly, like the S15 twin test).
    #[tokio::test]
    async fn always_on_top_menu_item_roundtrips() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        assert!(!app.settings.always_on_top);
        drop(app.update(Message::TitleMenu(TitleMenuItem::AlwaysOnTop)));
        assert!(app.settings.always_on_top, "toggled on");
        assert!(app.title_menu.is_none(), "menu closed after pick");
        // Idempotent same-value path.
        drop(app.update(Message::AlwaysOnTopToggled(true)));
        assert!(app.settings.always_on_top);
        // And back off via the Settings toggler.
        drop(app.update(Message::AlwaysOnTopToggled(false)));
        assert!(!app.settings.always_on_top);
        // The SettingsView round-trips the field through the service.
        let mut s = app.service.settings().await;
        assert!(!s.always_on_top, "mock default off");
        s.always_on_top = true;
        app.service.set_settings(s).await.expect("save");
        assert!(app.service.settings().await.always_on_top);
    }
}
