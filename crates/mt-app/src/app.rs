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

use iced::widget::scrollable;

use crate::theme::MtTheme;
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
    /// S18: appearance mode picked — "system" | "light" | "dark"
    /// (replaces the S9 dark-mode toggler).
    ThemeModePicked(String),
    /// S18: the accent color wheel was pressed (drag starts; the color
    /// under the cursor is picked immediately).
    WheelPressed,
    /// S18: the cursor moved over the wheel (local coords; live seed
    /// update while dragging).
    WheelMoved(iced::Point),
    /// S18: the wheel drag ended (release anywhere) — persist the seed.
    WheelReleased,
    /// System proxy toggled.
    SystemProxyToggled(bool),
    /// Merge policy picked.
    MergePolicyPicked(String),
    /// Autostart toggled.
    AutostartToggled(bool),
    /// S23: "Reset all settings to defaults" pressed — the FIRST press
    /// arms the button (label + status line tell the user what a reset
    /// does), a SECOND press executes it. Any other setting change
    /// disarms.
    ResetSettingsPressed,
    /// set_settings finished.
    SettingsSaved(Result<(), String>),
    /// Logs level filter picked.
    LogFilterPicked(String),
    /// Clear the log buffer.
    LogsCleared,
    // --- S17: logs screen navigation + autoscroll (D27) ---
    /// Scroll the logs viewport to the very top (floating button).
    LogsScrollTop,
    /// Scroll the logs viewport to the very bottom (floating button).
    LogsScrollBottom,
    /// Toggle autoscroll-to-bottom on new log lines.
    LogsAutoscrollToggled,
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
    // --- S19: server editing, speed test, kill switch, diagnostics ---
    /// Open the Edit dialog for a server (fetch the editable material).
    EditServer(String),
    /// `profile_details` finished (id + the prefill, or an error).
    EditLoaded(String, Result<mt_service::ProfileDetails, String>),
    /// One Edit-dialog field changed.
    EditField(EditField, String),
    /// Save the Edit dialog's draft (`update_profile`).
    EditSave,
    /// `update_profile` finished.
    EditSaved(Result<mt_service::ProfileView, String>),
    /// Close the Edit dialog without saving.
    EditCancel,
    /// Run the speed test through the active profile.
    SpeedTest,
    /// The speed test finished.
    SpeedTested(Result<mt_service::SpeedTestInfo, String>),
    /// S22: push the upload payload through the tunnel (the button
    /// appears only after the download test has concluded).
    SpeedTestUpload,
    /// S22: the upload test finished.
    SpeedUploadTested(Result<mt_service::SpeedTestInfo, String>),
    /// Kill switch toggled (Settings → Network).
    KillSwitchToggled(bool),
    /// S21 (feature o): the fair-share bandwidth scheduler toggle.
    QosToggled(bool),
    /// Diagnostics mode toggled (Settings → Diagnostics).
    DiagnosticsToggled(bool),
    /// The diagnostics file path arrived (init / after a toggle).
    DiagPath(Option<String>),
    /// S22: the log directory path arrived (init).
    LogDirLoaded(String),
    /// Speed-test URL changed (Settings → Advanced).
    SpeedUrlChanged(String),
    /// S22: upload-test URL changed (Settings → Advanced).
    SpeedUploadUrlChanged(String),
    /// Open the log folder in the OS file manager.
    OpenLogsFolder,
}

/// S16: one entry of the titlebar right-click window menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleMenuItem {
    AlwaysOnTop,
    Minimize,
    Maximize,
    Close,
}

/// S19: one editable field of the server-Edit dialog (indexes into
/// [`EditDraft`]'s string fields; the update() arm writes exactly one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Tag,
    Remark,
    Address,
    Port,
    Identity,
    Sni,
    Fingerprint,
    PublicKey,
    ShortId,
    SpiderX,
}

/// S19: the Edit dialog's draft state (all text fields are strings; the
/// service validates + converts on save).
#[derive(Debug, Clone)]
pub struct EditDraft {
    pub id: String,
    pub tag: String,
    pub remark: String,
    /// Read-only protocol label ("vless" …).
    pub protocol: String,
    pub address: String,
    pub port: String,
    pub identity: String,
    /// Read-only security label ("reality" …); the REALITY-only fields
    /// below render only when this is "reality".
    pub security: String,
    pub sni: String,
    pub fingerprint: String,
    pub public_key: String,
    pub short_id: String,
    pub spider_x: String,
    /// A save is in flight (disables the Save button).
    pub saving: bool,
    /// Last save error (shown in the dialog; keeps it open).
    pub error: Option<String>,
}

impl EditDraft {
    /// Prefill from the service's `profile_details` payload.
    pub fn from_details(d: mt_service::ProfileDetails) -> Self {
        Self {
            id: d.id,
            tag: d.tag,
            remark: d.remark,
            protocol: d.protocol,
            address: d.address,
            port: d.port,
            identity: d.identity,
            security: d.security,
            sni: d.sni,
            fingerprint: d.fingerprint,
            public_key: d.public_key,
            short_id: d.short_id,
            spider_x: d.spider_x,
            saving: false,
            error: None,
        }
    }

    /// The draft as a save payload (the service re-validates).
    pub fn to_details(&self) -> mt_service::ProfileDetails {
        mt_service::ProfileDetails {
            id: self.id.clone(),
            tag: self.tag.clone(),
            remark: self.remark.clone(),
            protocol: self.protocol.clone(),
            address: self.address.clone(),
            port: self.port.clone(),
            identity: self.identity.clone(),
            security: self.security.clone(),
            sni: self.sni.clone(),
            fingerprint: self.fingerprint.clone(),
            public_key: self.public_key.clone(),
            short_id: self.short_id.clone(),
            spider_x: self.spider_x.clone(),
        }
    }

    /// TRUE iff the security-layer fields apply (REALITY profiles only).
    pub fn is_reality(&self) -> bool {
        self.security.eq_ignore_ascii_case("reality")
    }
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
/// S23: quiet ticks before a debounced window-size save (1 tick ≈ 1 s;
/// 2 keeps a drag's writes to one at the end).
pub const WINDOW_SAVE_DEBOUNCE_TICKS: u32 = 2;
/// S23: the persisted window size is clamped to the window's own
/// min_size (run_app) before it ever reaches settings.json.
pub const WINDOW_MIN_W: u32 = 880;
pub const WINDOW_MIN_H: u32 = 580;
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
    /// S23: the reset-to-defaults button is ARMED (one click warns, the
    /// second executes; any other settings change disarms).
    pub reset_armed: bool,
    /// S23: a reset is being persisted — when the save finishes, the
    /// status line says "reset to defaults" instead of plain "saved".
    pub settings_reset_pending: bool,
    /// S23: the persisted window size changed and is waiting for the
    /// debounce to settle (resize events stream continuously during a
    /// drag; writes happen only after WINDOW_SAVE_DEBOUNCE_TICKS quiet
    /// ticks).
    pub window_save_pending: bool,
    /// S23: how many quiet ticks since the last Resized event (only
    /// meaningful while [`Self::window_save_pending`] is set).
    pub window_quiet_ticks: u32,
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
    /// S17: autoscroll the logs viewport to the bottom as new lines land
    /// (default ON — a log viewer should track the tail until the user
    /// opts out; the toggle lives in the Logs toolbar).
    pub logs_autoscroll: bool,
    /// S18: last-detected OS dark preference (System mode resolves through
    /// it; `MT_THEME_SYSTEM_DARK` env override for the rig).
    pub sys_dark: bool,
    /// S18: Tick counter for the System-mode OS preference re-poll.
    pub sys_poll: u32,
    /// S18: the accent wheel drag is in progress (seed updates live, the
    /// pick persists on release).
    pub wheel_dragging: bool,
    /// S18: last cursor position inside the wheel (local coords) — the
    /// press picks the color under the cursor without needing a move.
    pub wheel_cursor: Option<iced::Point>,
    /// S19: the open server-Edit dialog (None = closed). Holds the draft
    /// fields + the save-in-flight flag; opens via `Message::EditServer`.
    pub edit: Option<EditDraft>,
    /// S19: last speed-test result for the Status screen (None = never
    /// run).
    pub speed: Option<Result<mt_service::SpeedTestInfo, String>>,
    /// S19: a speed test is in flight (drives the card's button state).
    pub speed_testing: bool,
    /// S22: the upload-test result (right column of the merged panel;
    /// the button appears after the download test concluded).
    pub speed_up: Option<Result<mt_service::SpeedTestInfo, String>>,
    /// S22: an upload test is in flight.
    pub speed_up_testing: bool,
    /// S19: the diagnostics log file currently being written (Settings
    /// screen; None when the mode is off).
    pub diag_path: Option<String>,
    /// S22: the service's log directory (fetched once at init; the
    /// "Open log folder" button opens it in EVERY state).
    pub log_dir: Option<String>,
}

/// S17: the release version shown next to the app name in the titlebar.
/// Inherited from the workspace package version (one-line release bumps).
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// S19 (review fix): should the Status screen show the KILL SWITCH banner?
/// Pure gate (unit-tested): the switch is armed AND the tunnel FAILED —
/// the give_up path engages the black hole exactly in that state.
pub fn kill_switch_banner_visible(state: ServiceState, kill_switch: bool) -> bool {
    kill_switch && state == ServiceState::Failed
}

impl App {
    /// Bare constructor with empty state (service loads fill it).
    pub fn new(service: Arc<dyn MtService>, flags: Flags) -> Self {
        let events_rx = Arc::new(AsyncMutex::new(service.events()));
        let seed_text = String::new();
        // S23: the pre-load placeholder IS the stock view (seed
        // 0x6750A4 = SEED_SWATCHES[0], balanced, per-connection,
        // warning — the same values the literals used to spell out).
        let settings = SettingsView::default();
        // S18: detect the OS appearance once so the System mode (the
        // stock default) starts honest.
        let sys_dark = crate::theme::system_prefers_dark();
        Self {
            theme: MtTheme::derive(settings.theme_seed, settings.effective_dark(sys_dark)),
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
            // S23
            reset_armed: false,
            settings_reset_pending: false,
            window_save_pending: false,
            window_quiet_ticks: 0,
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
            // S17
            logs_autoscroll: true,
            // S18
            sys_dark,
            sys_poll: 0,
            wheel_dragging: false,
            wheel_cursor: None,
            // S19
            edit: None,
            speed: None,
            speed_testing: false,
            // S22
            speed_up: None,
            speed_up_testing: false,
            diag_path: None,
            log_dir: None,
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
            app.fetch_diag_path(),
            app.fetch_log_dir(),
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
        app.diag_path = app.service.diagnostics_path().await;
        app.log_dir = Some(app.service.log_dir().await);
        // S18: resolve light/dark through the picked mode + OS preference.
        app.sys_dark = crate::theme::system_prefers_dark();
        app.theme.rederive(
            app.settings.theme_seed,
            app.settings.effective_dark(app.sys_dark),
        );
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

    /// S19: refresh the diagnostics file path shown on the Settings screen
    /// (the service creates the file when the mode turns on).
    fn fetch_diag_path(&self) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(
            async move { svc.diagnostics_path().await },
            Message::DiagPath,
        )
    }

    /// S22: the log directory for the "Open log folder" button (fetched
    /// once at boot; the button opens it in every state).
    fn fetch_log_dir(&self) -> iced::Task<Message> {
        let svc = Arc::clone(&self.service);
        iced::Task::perform(async move { svc.log_dir().await }, Message::LogDirLoaded)
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

    fn save_settings(&mut self, s: SettingsView) -> iced::Task<Message> {
        // S23: any settings mutation disarms the pending reset (the
        // two-click guard only survives its own confirm click).
        self.reset_armed = false;
        // S18: keep the legacy `dark_mode` bool at the EFFECTIVE state so a
        // v0.11-or-older binary reading this file lands on the appearance
        // the user currently sees (forward-compat shadow; D8).
        let mut s = s;
        s.dark_mode = s.effective_dark(self.sys_dark);
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
                // S18: System mode follows the OS — re-check the preference
                // every few ticks (cheap on Windows: one registry read; the
                // env seam makes the rig deterministic).
                if self.settings.theme_mode() == mt_service::ThemeMode::System {
                    self.sys_poll = self.sys_poll.saturating_add(1);
                    if self.sys_poll >= crate::theme::SYSTEM_THEME_POLL_TICKS {
                        self.sys_poll = 0;
                        let now_dark = crate::theme::system_prefers_dark();
                        if now_dark != self.sys_dark {
                            self.sys_dark = now_dark;
                            util::push_log(
                                &mut self.logs,
                                format!(
                                    "INFO  gui: system appearance changed → {}",
                                    if now_dark { "dark" } else { "light" }
                                ),
                            );
                            self.apply_theme();
                        }
                    }
                }
                let t = self.refresh_status();
                self.sync_tray();
                // S23: debounced window-size persistence — the size
                // settles for WINDOW_SAVE_DEBOUNCE_TICKS quiet ticks,
                // then one save lands (a drag never writes per event).
                if self.window_save_pending {
                    self.window_quiet_ticks = self.window_quiet_ticks.saturating_add(1);
                    if self.window_quiet_ticks >= WINDOW_SAVE_DEBOUNCE_TICKS {
                        self.window_save_pending = false;
                        self.window_quiet_ticks = 0;
                        return self.save_settings(self.settings.clone());
                    }
                }
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
                ServiceEvent::LogLine(line) => {
                    util::push_log(&mut self.logs, line);
                    // S17: keep the tail in view while autoscroll is on and
                    // the Logs screen is showing (the task no-ops if the
                    // scrollable with this id is not on screen).
                    if self.autoscroll_logs_now() {
                        return scrollable::snap_to(
                            screens::logs_scroll_id(),
                            scrollable::RelativeOffset { x: 0.5, y: 1.0 },
                        );
                    }
                }
            },
            Message::Screen(s) => {
                self.screen = s;
                return match s {
                    Screen::Servers => self.load_profiles(),
                    Screen::Adapters => self.load_adapters(),
                    Screen::Routing => self.load_rules(),
                    // S17: entering Logs with autoscroll on lands on the
                    // tail — the natural place to start reading live logs.
                    Screen::Logs if self.logs_autoscroll => scrollable::snap_to(
                        screens::logs_scroll_id(),
                        scrollable::RelativeOffset { x: 0.5, y: 1.0 },
                    ),
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
                // S23: the shared "a fresh canonical settings view just
                // arrived" path (also used by the reset button): apply
                // theme/seed/adapters/window state from it.
                //
                // A fresh load also cancels any armed reset (the state it
                // was about to clear is now current).
                self.reset_armed = false;
                // S23: restore the remembered window size once the
                // window exists (a load that races the first Opened
                // event simply keeps the stock size — the Resized event
                // will correct it).
                if self.window_id.is_some() && !s.window_size_is_default() {
                    let id = self.window_id.expect("checked above");
                    let apply = self.apply_settings_view(s.clone());
                    return iced::Task::batch([
                        apply,
                        iced::window::resize(
                            id,
                            iced::Size::new(s.window_w as f32, s.window_h as f32),
                        ),
                    ]);
                }
                return self.apply_settings_view(s);
            }
            Message::SeedSwatch(seed) => {
                self.settings.theme_seed = seed;
                self.seed_text = seed.to_string();
                self.apply_theme();
                return self.save_settings(self.settings.clone());
            }
            Message::SeedFieldChanged(t) => self.seed_text = t,
            Message::SeedCommitted => match parse_seed(&self.seed_text) {
                Some(seed) => {
                    self.settings.theme_seed = seed;
                    self.apply_theme();
                    return self.save_settings(self.settings.clone());
                }
                None => {
                    self.settings_status =
                        Some("invalid seed: use decimal or 0xRRGGBB".to_string());
                }
            },
            Message::ThemeModePicked(mode) => {
                if !["system", "light", "dark"].contains(&mode.as_str()) {
                    self.settings_status = Some(format!("unknown theme mode: {mode:?}"));
                    return iced::Task::none();
                }
                self.settings.theme_mode = mode;
                self.apply_theme();
                return self.save_settings(self.settings.clone());
            }
            // S18: the accent wheel. Press picks the color under the
            // cursor (the move that GOT the cursor there already recorded
            // the position); drag updates live; release persists.
            Message::WheelPressed => {
                self.wheel_dragging = true;
                if let Some(p) = self.wheel_cursor {
                    self.set_wheel_seed(crate::theme::wheel_seed_at(
                        p,
                        crate::theme::COLOR_WHEEL_DRAW,
                    ));
                }
            }
            Message::WheelMoved(p) => {
                self.wheel_cursor = Some(p);
                if self.wheel_dragging {
                    self.set_wheel_seed(crate::theme::wheel_seed_at(
                        p,
                        crate::theme::COLOR_WHEEL_DRAW,
                    ));
                }
            }
            Message::WheelReleased => {
                let was_dragging = self.wheel_dragging;
                self.wheel_dragging = false;
                if was_dragging {
                    // The mouse_area's own on_release also lands here — the
                    // second arrival no-ops (dragging already false).
                    return self.save_settings(self.settings.clone());
                }
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
            // S23: "Reset all settings to defaults" — two-click confirm.
            // The FIRST press arms (the button relabels, the status line
            // explains the scope); the SECOND executes: the stock view
            // is applied locally (theme/seed/adapters/window level —
            // the same apply_settings_view path a fresh load takes)
            // and persisted through the NORMAL validated save (stock
            // values pass every check by construction). Profiles and
            // routing rules are NOT part of the reset — they live in
            // their own screens and files.
            Message::ResetSettingsPressed => {
                if !self.reset_armed {
                    self.reset_armed = true;
                    self.settings_status = Some(
                        "click the button again to reset every setting to \
                         defaults — servers and routing rules are kept"
                            .to_string(),
                    );
                    util::push_log(
                        &mut self.logs,
                        "INFO  gui: settings reset armed — click again to confirm".to_string(),
                    );
                    return iced::Task::none();
                }
                self.reset_armed = false;
                self.settings_reset_pending = true;
                let stock = SettingsView::default();
                let apply = self.apply_settings_view(stock.clone());
                // Reset restores the stock window size too (a live
                // resize to 1080×720 — the next launch reads it back).
                let resize = if let Some(id) = self.window_id {
                    iced::window::resize(
                        id,
                        iced::Size::new(stock.window_w as f32, stock.window_h as f32),
                    )
                } else {
                    iced::Task::none()
                };
                let save = self.save_settings(stock);
                // bonded_adapters is owned by the ADAPTERS path on the
                // real service (set_settings deliberately never clobbers
                // it — a stale GUI copy must not undo a fresh selection),
                // so the reset clears it through its own API: the
                // service persists the empty (single-link, unpinned)
                // selection and restarts a live session if one is up.
                let svc = Arc::clone(&self.service);
                let clear_adapters = iced::Task::perform(
                    async move { svc.set_bonded_adapters(Vec::new()).await },
                    Message::AdaptersSaved,
                );
                // Mirror the service-side clear in the rows immediately
                // (AdaptersSaved's status counts them BEFORE its reload).
                for row in &mut self.adapters {
                    row.selected = false;
                }
                util::push_log(
                    &mut self.logs,
                    "INFO  gui: settings reset to defaults (servers and rules kept)".to_string(),
                );
                return iced::Task::batch([apply, resize, save, clear_adapters]);
            }
            Message::SettingsSaved(res) => match res {
                Ok(()) => {
                    // S23: the reset's completion note beats the generic
                    // "saved" (the user just clicked a destructive
                    // button — say what it did).
                    if self.settings_reset_pending {
                        self.settings_reset_pending = false;
                        self.settings_status = Some(
                            "settings reset to defaults — servers and routing rules \
                             untouched"
                                .to_string(),
                        );
                    } else {
                        self.settings_status = Some("settings saved".to_string());
                    }
                }
                Err(e) => {
                    self.settings_reset_pending = false;
                    self.settings_status = Some(format!("save failed: {e}"));
                }
            },
            Message::LogFilterPicked(l) => self.log_filter = LogFilter::from_label(&l),
            Message::LogsCleared => self.logs.clear(),
            // S17: floating navigation buttons + autoscroll toggle (D27).
            Message::LogsScrollTop => {
                return scrollable::snap_to(
                    screens::logs_scroll_id(),
                    scrollable::RelativeOffset { x: 0.5, y: 0.0 },
                );
            }
            Message::LogsScrollBottom => {
                return scrollable::snap_to(
                    screens::logs_scroll_id(),
                    scrollable::RelativeOffset { x: 0.5, y: 1.0 },
                );
            }
            Message::LogsAutoscrollToggled => {
                self.logs_autoscroll = !self.logs_autoscroll;
                // Flipping it back ON should immediately re-anchor to the
                // tail (the user asked to follow again).
                if self.logs_autoscroll {
                    return scrollable::snap_to(
                        screens::logs_scroll_id(),
                        scrollable::RelativeOffset { x: 0.5, y: 1.0 },
                    );
                }
            }
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
            Message::SpeedUrlChanged(t) => {
                self.settings.speed_test_url = t;
                return self.save_settings(self.settings.clone());
            }
            Message::SpeedUploadUrlChanged(t) => {
                self.settings.speed_test_upload_url = t;
                return self.save_settings(self.settings.clone());
            }
            // --- S19: server editing, speed test, kill switch, diagnostics ---
            Message::EditServer(id) => {
                let svc = Arc::clone(&self.service);
                let id_for_res = id.clone();
                return iced::Task::perform(
                    async move {
                        let res = svc.profile_details(&id_for_res).await;
                        (id_for_res, res)
                    },
                    |(id, res)| Message::EditLoaded(id, res),
                );
            }
            Message::EditLoaded(_id, res) => match res {
                Ok(details) => {
                    // Only open when no dialog is up (a stale fetch must
                    // never clobber the draft the user is typing into).
                    if self.edit.is_none() {
                        self.edit = Some(EditDraft::from_details(details));
                        self.notice = None;
                    }
                }
                Err(e) => self.notice = Some(format!("cannot edit server: {e}")),
            },
            Message::EditField(field, value) => {
                if let Some(d) = self.edit.as_mut() {
                    match field {
                        EditField::Tag => d.tag = value,
                        EditField::Remark => d.remark = value,
                        EditField::Address => d.address = value,
                        EditField::Port => d.port = value,
                        EditField::Identity => d.identity = value,
                        EditField::Sni => d.sni = value,
                        EditField::Fingerprint => d.fingerprint = value,
                        EditField::PublicKey => d.public_key = value,
                        EditField::ShortId => d.short_id = value,
                        EditField::SpiderX => d.spider_x = value,
                    }
                }
            }
            Message::EditSave => {
                let Some(draft) = self.edit.clone() else {
                    return iced::Task::none();
                };
                if draft.saving {
                    return iced::Task::none();
                }
                if let Some(d) = self.edit.as_mut() {
                    d.saving = true;
                    d.error = None;
                }
                let svc = Arc::clone(&self.service);
                let id = draft.id.clone();
                let payload = draft.to_details();
                return iced::Task::perform(
                    async move { svc.update_profile(&id, payload).await },
                    Message::EditSaved,
                );
            }
            Message::EditSaved(res) => match res {
                Ok(pv) => {
                    self.edit = None;
                    self.notice = Some(format!("saved server \"{}\"", pv.tag));
                    return self.load_profiles();
                }
                Err(e) => {
                    // Review fix: surface the failure in the Logs tab too —
                    // the dialog error is easy to dismiss and lose.
                    util::push_log(&mut self.logs, format!("WARN  server edit failed: {e}"));
                    if let Some(d) = self.edit.as_mut() {
                        d.saving = false;
                        d.error = Some(e);
                    }
                }
            },
            Message::EditCancel => self.edit = None,
            Message::SpeedTest => {
                if self.speed_testing {
                    return iced::Task::none();
                }
                self.speed_testing = true;
                let svc = Arc::clone(&self.service);
                return iced::Task::perform(
                    async move { svc.speed_test(None).await },
                    Message::SpeedTested,
                );
            }
            Message::SpeedTested(res) => {
                self.speed_testing = false;
                if let Err(e) = &res {
                    util::push_log(&mut self.logs, format!("WARN  speed test: {e}"));
                }
                self.speed = Some(res);
            }
            Message::SpeedTestUpload => {
                if self.speed_up_testing || self.speed_testing {
                    return iced::Task::none();
                }
                self.speed_up_testing = true;
                let svc = Arc::clone(&self.service);
                return iced::Task::perform(
                    async move { svc.speed_test_upload(None).await },
                    Message::SpeedUploadTested,
                );
            }
            Message::SpeedUploadTested(res) => {
                self.speed_up_testing = false;
                if let Err(e) = &res {
                    util::push_log(&mut self.logs, format!("WARN  upload speed test: {e}"));
                }
                self.speed_up = Some(res);
            }
            Message::KillSwitchToggled(v) => {
                self.settings.kill_switch = v;
                return self.save_settings(self.settings.clone());
            }
            // S21 (feature o): flips live in the service (no reconnect).
            Message::QosToggled(v) => {
                self.settings.qos_fair = v;
                return self.save_settings(self.settings.clone());
            }
            Message::DiagnosticsToggled(v) => {
                self.settings.diagnostics = v;
                // Review fix: CHAIN the path refresh after the save — a
                // batch could observe the pre-toggle path (stale caption).
                let save = self.save_settings(self.settings.clone());
                let svc = Arc::clone(&self.service);
                return save.then(move |_| {
                    let svc = Arc::clone(&svc);
                    iced::Task::perform(
                        async move { svc.diagnostics_path().await },
                        Message::DiagPath,
                    )
                });
            }
            Message::DiagPath(p) => self.diag_path = p,
            Message::OpenLogsFolder => {
                // S22: the button opens the LOG FOLDER in every state —
                // no more diagnostics-mode gate (the old gate reported
                // "turn diagnostics on first" after every restart, since
                // the sink only started on an off→on transition). The
                // dir is created on demand, the shell-open is the
                // platform file manager, and failures land in BOTH the
                // Settings status line and the Logs tab (never silent).
                //
                // S23: shell_open::open_folder — on Windows this is
                // ShellExecuteW first (the v0.15.0 explorer spawn hit
                // ERROR_NOT_SUPPORTED / "os error 50" in GUI-subsystem
                // builds: inherited null stdio handles), with an
                // explorer.exe fallback that now uses NUL stdio.
                let Some(dir) = self.log_dir.clone() else {
                    self.settings_status = Some(
                        "the log folder path is not available yet — try again in a moment"
                            .to_string(),
                    );
                    return iced::Task::none();
                };
                let path = std::path::PathBuf::from(&dir);
                if let Err(e) = std::fs::create_dir_all(&path) {
                    util::push_log(
                        &mut self.logs,
                        format!("WARN  open log folder: {}: {e}", path.display()),
                    );
                    self.settings_status = Some(format!("could not create the log folder: {e}"));
                    return iced::Task::none();
                }
                match crate::shell_open::open_folder(&path) {
                    Ok(()) => {
                        self.settings_status = Some(format!("log folder: {}", path.display()));
                    }
                    Err(e) => {
                        util::push_log(&mut self.logs, format!("WARN  {e}"));
                        self.settings_status = Some(e);
                    }
                }
            }
            Message::LogDirLoaded(dir) => {
                self.log_dir = Some(dir);
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
                        // S23: persist the size (debounced on Tick — a
                        // drag streams Resized events; only the settled
                        // size is ever written). Clamped to the window
                        // minimum so a stored value can never wedge the
                        // layout; sub-minimal events (restored bounds
                        // before the WM clamps them) are ignored.
                        let w = size.width.max(WINDOW_MIN_W as f32);
                        let h = size.height.max(WINDOW_MIN_H as f32);
                        if (w as u32) != self.settings.window_w
                            || (h as u32) != self.settings.window_h
                        {
                            self.settings.window_w = w as u32;
                            self.settings.window_h = h as u32;
                            self.window_save_pending = true;
                            self.window_quiet_ticks = 0;
                        }
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
        // S19: the server Edit dialog — same full-window layer discipline.
        if self.edit.is_some() {
            stack = stack.push(screens::edit_overlay(self));
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
            // S16: cursor tracking (anchors the titlebar window menu) +
            // S18: a left release ANYWHERE ends an accent-wheel drag (the
            // mouse_area only sees releases inside its own bounds).
            iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::CursorMoved(position))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::WheelReleased),
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

    /// S17: should a freshly-landed log line re-anchor the logs viewport
    /// to the bottom? Pure gating for the autoscroll task (unit-tested):
    /// the user asked for it AND the Logs screen is the one on display
    /// (scrolling a viewport that is not on screen is a no-op task, but
    /// skipping it outright keeps the update path allocation-free when
    /// the user is elsewhere).
    pub fn autoscroll_logs_now(&self) -> bool {
        self.logs_autoscroll && self.screen == Screen::Logs
    }

    /// S18: the dark flag the picked mode + OS preference resolve to right
    /// now (the single source of truth for `MtTheme` re-derivation).
    pub fn theme_dark(&self) -> bool {
        self.settings.effective_dark(self.sys_dark)
    }

    /// S18: re-derive the theme snapshot from the CURRENT seed + resolved
    /// light/dark (every seed/mode change funnels through here).
    pub fn apply_theme(&mut self) {
        self.theme
            .rederive(self.settings.theme_seed, self.theme_dark());
    }

    /// S23: apply a freshly loaded (or freshly RESET) canonical settings
    /// view to the GUI state: seed field, OS-preference re-read, theme
    /// re-derivation, always-on-top window level, adapter re-sort. The
    /// one place that knows how a settings view becomes visible state
    /// (previously inline in SettingsLoaded).
    fn apply_settings_view(&mut self, s: SettingsView) -> iced::Task<Message> {
        self.settings = s.clone();
        self.seed_text = s.theme_seed.to_string();
        // S18: light/dark resolves through the picked mode + the CURRENT
        // OS preference (not the legacy bool).
        self.sys_dark = crate::theme::system_prefers_dark();
        self.apply_theme();
        // S16: honor (or CLEAR) always-on-top — `false` lowers the
        // window back to Normal, which a reset requires.
        let top = s.always_on_top;
        let level_task = if self.window_id.is_some() {
            self.apply_window_level(top)
        } else {
            iced::Task::none()
        };
        // S11: the saved bonded order may have arrived after the adapter
        // rows (the init batch is concurrent) — re-sort UNLESS a draft is
        // in progress (never reshuffle edits).
        if !self.adapters_dirty() && !self.adapters.is_empty() {
            sort_adapter_rows(&mut self.adapters, &s.bonded_adapters);
        }
        level_task
    }

    /// S18: live-update the accent seed during a wheel drag/pick (no disk
    /// write — the release persists; see [`Message::WheelReleased`]).
    fn set_wheel_seed(&mut self, seed: u32) {
        if seed != self.settings.theme_seed {
            self.settings.theme_seed = seed;
            self.seed_text = seed.to_string();
            self.apply_theme();
        }
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
                adapter_name: mt_service::adapter_of_link_name(name),
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

    /// S17: autoscroll defaults ON (a log viewer tracks the tail until the
    /// user opts out), the toggle flips it, and re-enabling re-anchors.
    #[test]
    fn logs_autoscroll_default_and_toggle() {
        let mut app = App::new(Arc::new(crate::mock::MockService::new()), Flags::default());
        assert!(app.logs_autoscroll, "autoscroll defaults ON");
        let _ = app.update(Message::LogsAutoscrollToggled);
        assert!(!app.logs_autoscroll);
        let _ = app.update(Message::LogsAutoscrollToggled);
        assert!(app.logs_autoscroll);
    }

    /// S17: the autoscroll task is gated on the Logs screen being on
    /// display (scrolling an off-screen viewport is pointless work).
    #[test]
    fn logs_autoscroll_gated_on_screen() {
        let mut app = App::new(Arc::new(crate::mock::MockService::new()), Flags::default());
        app.screen = Screen::Status;
        assert!(!app.autoscroll_logs_now(), "off-screen: no task");
        app.screen = Screen::Logs;
        assert!(app.autoscroll_logs_now());
        app.logs_autoscroll = false;
        assert!(!app.autoscroll_logs_now(), "user opted out: no task");
    }

    /// S17: the titlebar version comes from the workspace-inherited
    /// CARGO_PKG_VERSION — a bare x.y.z triplet that moves with the
    /// release tag (guards the `version.workspace = true` wiring).
    #[test]
    fn app_version_is_release_triplet() {
        let v = app_version();
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "version '{v}' must be x.y.z");
        assert!(
            parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
            "version '{v}' must be numeric"
        );
    }

    /// S17: the logs scrollable id is stable across view rebuilds (the
    /// update path's snap_to targets the id the viewport was given).
    #[test]
    fn logs_scroll_id_stable() {
        assert_eq!(
            crate::screens::logs_scroll_id(),
            crate::screens::logs_scroll_id()
        );
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

        // S18: explicit Dark mode flips the theme now (and the legacy
        // dark_mode shadow follows on save).
        drop(app.update(Message::ThemeModePicked("dark".to_string())));
        assert!(app.theme.dark);
        drop(app.update(Message::ThemeModePicked("light".to_string())));
        assert!(!app.theme.dark);
        // System mode resolves through the detected OS preference.
        drop(app.update(Message::ThemeModePicked("system".to_string())));
        assert_eq!(app.theme.dark, app.theme_dark());
        assert_eq!(app.settings.theme_mode, "system");
        // Garbage is rejected, leaving the mode untouched.
        drop(app.update(Message::ThemeModePicked("neon".to_string())));
        assert_eq!(app.settings.theme_mode, "system");

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

    /// S18: the accent wheel — press picks the color under the cursor,
    /// drag follows, release persists via the same save path as swatches.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn wheel_drag_updates_seed_live_and_persists_on_release() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        let seed_before = app.theme.palette.seed();
        let s = crate::theme::COLOR_WHEEL_DRAW;

        // Cursor moves over the wheel WITHOUT dragging: remembered, no pick.
        drop(app.update(Message::WheelMoved(iced::Point::new(s / 2.0, s / 2.0))));
        assert_eq!(app.theme.palette.seed(), seed_before);

        // Press at the 12 o'clock rim: red seed, immediately applied.
        drop(app.update(Message::WheelPressed));
        assert!(app.wheel_dragging);
        let red = crate::theme::wheel_seed_at(iced::Point::new(s / 2.0, 4.0), s);
        drop(app.update(Message::WheelMoved(iced::Point::new(s / 2.0, 4.0))));
        assert_eq!(app.theme.palette.seed(), red);
        assert_eq!(app.seed_text, red.to_string());

        // Release: the drag ends and the pick rides the SAME save path as
        // the seed swatches (`save_settings` → Task::perform; the iced
        // runtime owns task execution, so a unit test drops the task —
        // the mock round-trip test covers the service save itself).
        drop(app.update(Message::WheelReleased));
        assert!(!app.wheel_dragging);
        assert_eq!(app.settings.theme_seed, red);
        assert_eq!(app.theme.palette.seed(), red);
        assert_eq!(app.settings.theme_mode, "system");

        // A stray release (no drag in progress) does nothing.
        drop(app.update(Message::WheelReleased));
        assert_eq!(app.theme.palette.seed(), red);
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
                adapter_name: mt_service::adapter_of_link_name("gcp-vm [Ethernet]"),
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

    // --- S19: server editing, speed test, kill switch, diagnostics ---------

    /// S19: the Edit dialog flow — open (fetch), type into fields, save
    /// (update_profile), close; a failing save keeps the dialog open with
    /// the error in it.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn edit_dialog_open_type_save_close() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        let id = app.profiles[0].id.clone();
        assert!(app.edit.is_none());

        // Open: the fetch completes into a draft.
        drop(app.update(Message::EditServer(id.clone())));
        let details = app.service.profile_details(&id).await.expect("details");
        drop(app.update(Message::EditLoaded(id.clone(), Ok(details.clone()))));
        let draft = app.edit.as_ref().expect("dialog open");
        assert_eq!(draft.id, id);
        assert_eq!(draft.tag, details.tag);
        assert!(
            draft.is_reality(),
            "the mock's seed vless profile is REALITY"
        );

        // Type: rename + retag the port.
        drop(app.update(Message::EditField(EditField::Tag, "edited".into())));
        drop(app.update(Message::EditField(EditField::Port, "8443".into())));
        drop(app.update(Message::EditField(EditField::Sni, "new.sni.example".into())));
        let draft = app.edit.as_ref().expect("still open");
        assert_eq!(draft.tag, "edited");
        assert_eq!(draft.port, "8443");
        assert_eq!(draft.sni, "new.sni.example");

        // Save: the service applies the edit; the dialog closes and the
        // notice names the saved server.
        let res = app
            .service
            .update_profile(&id, app.edit.as_ref().unwrap().to_details())
            .await
            .expect("save");
        drop(app.update(Message::EditSaved(Ok(res))));
        assert!(app.edit.is_none(), "closed after a good save");
        assert!(app.notice.as_deref().unwrap_or("").contains("edited"));

        // A failing save keeps the dialog open with the error shown.
        drop(app.update(Message::EditServer(id.clone())));
        let details = app.service.profile_details(&id).await.expect("details");
        drop(app.update(Message::EditLoaded(id.clone(), Ok(details))));
        drop(app.update(Message::EditSaved(Err(
            "port must be a number 0-65535".into(),
        ))));
        let draft = app.edit.as_ref().expect("still open on error");
        assert_eq!(
            draft.error.as_deref(),
            Some("port must be a number 0-65535")
        );
        assert!(!draft.saving);
        drop(app.update(Message::EditCancel));
        assert!(app.edit.is_none(), "cancel closes");
    }

    /// S19: the speed-test card state machine — in-flight guard, result
    /// lands, errors land too.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn speed_test_message_flow() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        assert!(app.speed.is_none());
        drop(app.update(Message::SpeedTest));
        assert!(app.speed_testing);
        let res = app.service.speed_test(None).await;
        drop(app.update(Message::SpeedTested(res)));
        assert!(!app.speed_testing);
        let info = app
            .speed
            .as_ref()
            .expect("result present")
            .as_ref()
            .expect("ok");
        assert!(info.mbps > 0.0);
        // Errors surface in the result and the log buffer.
        drop(app.update(Message::SpeedTest));
        drop(app.update(Message::SpeedTested(Err("no data received".into()))));
        assert!(app.speed.as_ref().expect("error result present").is_err());
        assert!(app.logs.iter().any(|l| l.contains("speed test")));
    }

    /// S22: the upload-test state machine — the in-flight guard, the
    /// result landing, and errors landing in the log buffer too.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn speed_upload_test_message_flow() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        assert!(app.speed_up.is_none());
        drop(app.update(Message::SpeedTestUpload));
        assert!(app.speed_up_testing);
        // The in-flight guard: a second trigger while testing is a no-op.
        drop(app.update(Message::SpeedTestUpload));
        let res = app.service.speed_test_upload(None).await;
        drop(app.update(Message::SpeedUploadTested(res)));
        assert!(!app.speed_up_testing);
        let info = app
            .speed_up
            .as_ref()
            .expect("result present")
            .as_ref()
            .expect("ok");
        assert!(info.mbps > 0.0);
        // Errors surface in the result and the log buffer.
        drop(app.update(Message::SpeedTestUpload));
        drop(app.update(Message::SpeedUploadTested(Err("no data sent".into()))));
        assert!(app
            .speed_up
            .as_ref()
            .expect("error result present")
            .is_err());
        assert!(app.logs.iter().any(|l| l.contains("upload speed test")));
    }

    /// S23: the shell-open module carries the file-manager command test
    /// (crate::shell_open::tests::file_manager_command_targets_the_os_file_manager)
    /// plus the spawn-failure and ShellExecuteW-code-table tests.
    #[test]
    fn shell_open_module_is_wired() {
        // The handler path goes through crate::shell_open::open_folder;
        // smoke-cover the re-export direction on every platform.
        let dir = std::env::temp_dir().join("mt-shell-open-wired");
        let _ = std::fs::create_dir_all(&dir);
        let _ = crate::shell_open::open_folder(&dir);
    }

    /// S21 (feature o): the QoS toggle updates the draft settings exactly
    /// like the kill-switch twin (persistence through the service is
    /// proven directly in real_service's qos_toggle test).
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn s21_qos_toggle_updates_draft() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        assert!(!app.settings.qos_fair);
        drop(app.update(Message::QosToggled(true)));
        assert!(app.settings.qos_fair);
        drop(app.update(Message::QosToggled(false)));
        assert!(!app.settings.qos_fair);
    }

    /// S23: the reset-to-defaults button — one click arms (status line
    /// explains the scope, nothing changes), a second click applies the
    /// stock view locally, persists it through the service AND clears
    /// the bonded-adapter selection, and any other settings mutation
    /// disarms an armed reset.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn s23_reset_settings_two_click_flow() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service.clone(), Flags::default()).await;
        // Dirty several settings (seed, policy, qos, bond mode) so the
        // reset has something to actually revert.
        app.settings.theme_seed = 0xEA4335;
        app.settings.merge_policy = "failover".to_string();
        app.settings.qos_fair = true;
        app.settings.kill_switch = true;
        app.settings.window_w = 1280;
        app.settings.window_h = 900;
        drop(app.update(Message::SettingsSaved(Ok(())))); // clear status noise

        // FIRST click: armed, nothing applied yet.
        drop(app.update(Message::ResetSettingsPressed));
        assert!(app.reset_armed);
        assert!(app
            .settings_status
            .as_deref()
            .unwrap_or_default()
            .contains("servers and routing rules are kept"));
        assert_eq!(app.settings.theme_seed, 0xEA4335, "first click is inert");

        // SECOND click: stock view applied locally, the persistence tasks
        // are dispatched (the iced runtime awaits them; this test drops
        // them — the service-side round-trip is proven in real_service's
        // s23_reset_round_trip_persists test).
        drop(app.update(Message::ResetSettingsPressed));
        assert!(!app.reset_armed);
        assert!(app.settings_reset_pending);
        assert_eq!(
            app.settings,
            SettingsView::default(),
            "GUI draft is the stock view"
        );
        assert_eq!(app.seed_text, "6770852", "seed field follows the reset");
        // The completion note (the SettingsSaved arrival follows the
        // persisted save in the runtime; here it is driven directly).
        drop(app.update(Message::SettingsSaved(Ok(()))));
        assert_eq!(
            app.settings_status.as_deref(),
            Some("settings reset to defaults — servers and routing rules untouched")
        );
        assert!(!app.settings_reset_pending);

        // An armed reset disarms on any other settings mutation.
        drop(app.update(Message::ResetSettingsPressed));
        assert!(app.reset_armed);
        drop(app.update(Message::QosToggled(true)));
        assert!(!app.reset_armed, "a settings mutation disarms the reset");
    }

    /// S23: window-size persistence — Resized updates the draft (clamped
    /// to the minimum), the debounce holds writes until quiet ticks, and
    /// the save round-trip lands exactly the settled size.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn s23_window_size_persistence() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        assert_eq!((app.settings.window_w, app.settings.window_h), (1080, 720));

        // A stream of Resized events: only the LAST one matters.
        let id = iced::window::Id::unique();
        for (w, h) in [(1200.0, 800.0), (1280.0, 850.0), (1400.0, 900.0)] {
            drop(app.update(Message::WindowEvent(
                id,
                iced::window::Event::Resized(iced::Size::new(w, h)),
            )));
        }
        assert_eq!((app.settings.window_w, app.settings.window_h), (1400, 900));
        assert!(app.window_save_pending, "waiting for the debounce");

        // Debounce: quiet ticks below the threshold do not save yet.
        for _ in 0..(WINDOW_SAVE_DEBOUNCE_TICKS - 1) {
            drop(app.update(Message::Tick));
        }
        assert!(app.window_save_pending);
        drop(app.update(Message::Tick)); // the Nth quiet tick flushes
        assert!(!app.window_save_pending);
        // The flush dispatches the (dropped here) save task; the
        // service-side round-trip is proven in real_service's
        // s23_window_size_round_trip test.

        // Sub-minimal events are clamped, never wedging the layout.
        drop(app.update(Message::WindowEvent(
            id,
            iced::window::Event::Resized(iced::Size::new(400.0, 300.0)),
        )));
        assert_eq!(
            (app.settings.window_w, app.settings.window_h),
            (WINDOW_MIN_W, WINDOW_MIN_H),
            "clamped to the window minimum"
        );
    }

    /// S19: kill switch + diagnostics + speed-URL toggles update the draft
    /// settings (the save task is runtime-driven; persistence through the
    /// service is proven directly like the S15/S16 twins) and the view
    /// modal only exists while a dialog is open.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn s19_settings_toggles_and_modal_gating() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        assert!(!app.settings.kill_switch);
        drop(app.update(Message::KillSwitchToggled(true)));
        assert!(app.settings.kill_switch);
        assert!(!app.settings.diagnostics);
        drop(app.update(Message::DiagnosticsToggled(true)));
        assert!(app.settings.diagnostics);
        drop(app.update(Message::SpeedUrlChanged("http://x.example/f".into())));
        assert_eq!(app.settings.speed_test_url, "http://x.example/f");

        // The modal layer only renders while the draft exists (view()
        // gates on edit.is_some()).
        drop(app.view());
        assert!(app.edit.is_none());
        let id = app.profiles[0].id.clone();
        let details = app.service.profile_details(&id).await.expect("details");
        drop(app.update(Message::EditLoaded(id, Ok(details))));
        assert!(app.edit.is_some());
        drop(app.view()); // builds fine with the dialog up (no panic)

        // The mock keeps no diagnostics file: DiagPath(None) keeps the
        // Settings caption honest.
        drop(app.update(Message::DiagPath(None)));
        assert!(app.diag_path.is_none());
        drop(app.update(Message::DiagPath(Some("/tmp/x.log".into()))));
        assert_eq!(app.diag_path.as_deref(), Some("/tmp/x.log"));
    }

    /// S19 (review fix): the KILL SWITCH banner gate — visible exactly
    /// when the switch is armed AND the tunnel FAILED (the give_up state).
    #[test]
    fn kill_switch_banner_gate() {
        use mt_service::ServiceState as St;
        assert!(crate::app::kill_switch_banner_visible(St::Failed, true));
        assert!(!crate::app::kill_switch_banner_visible(St::Failed, false));
        assert!(!crate::app::kill_switch_banner_visible(St::Connected, true));
        assert!(!crate::app::kill_switch_banner_visible(
            St::Disconnected,
            true
        ));
        assert!(!crate::app::kill_switch_banner_visible(
            St::Connecting,
            true
        ));
    }

    /// S19 (review fix): a failed server edit ALSO lands in the Logs tab.
    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn edit_save_failure_is_logged() {
        let service = Arc::new(crate::mock::MockService::new());
        let mut app = App::init_async(service, Flags::default()).await;
        let id = app.profiles[0].id.clone();
        let details = app.service.profile_details(&id).await.expect("details");
        drop(app.update(Message::EditLoaded(id, Ok(details))));
        drop(app.update(Message::EditSaved(Err("boom".into()))));
        assert!(app
            .logs
            .iter()
            .any(|l| l.contains("server edit failed: boom")));
        assert!(app.edit.is_some(), "dialog stays open with the error");
    }
}
