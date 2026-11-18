//! The GUI application: MVU model, messages, update loop, subscriptions.
//!
//! The app never touches credentials and never panics on service errors:
//! every service result maps to user-facing state. All widget styling comes
//! from [`crate::theme`] helpers built on `mt-m3` tokens.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use mt_service::{MtService, ServiceEvent, ServiceState, SettingsView, StatusSnapshot};
use tokio::sync::broadcast;
use tokio::sync::Mutex as AsyncMutex;

use crate::theme::{MtTheme, SEED_SWATCHES};
use crate::{screens, util, Element};

/// The five top-level screens (left navigation rail order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Status,
    Servers,
    Routing,
    Settings,
    Logs,
}

impl Screen {
    /// Rail order.
    pub const ALL: [Screen; 5] = [
        Screen::Status,
        Screen::Servers,
        Screen::Routing,
        Screen::Settings,
        Screen::Logs,
    ];

    /// Human label used in the rail and the screen title.
    pub const fn label(self) -> &'static str {
        match self {
            Screen::Status => "Status",
            Screen::Servers => "Servers",
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
}

/// Valid routing rule kinds (matches the mt-net router matcher).
pub const RULE_KINDS: [&str; 4] = ["domain-suffix", "domain-keyword", "ip-cidr", "any"];
/// Valid routing rule actions (matches the mt-net router actions).
pub const RULE_ACTIONS: [&str; 3] = ["proxy", "direct", "reject"];
/// Valid merge policies (matches the mt-net aggregator).
pub const MERGE_POLICIES: [&str; 3] = ["failover", "round-robin", "lowest-latency"];

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
            settings,
            seed_text,
            settings_status: None,
            latency: HashMap::new(),
            logs: VecDeque::new(),
            log_filter: LogFilter::All,
        }
    }

    /// Synchronous init for the iced runtime: returns the app plus a task
    /// batch that loads all screen data.
    pub fn init(service: Arc<dyn MtService>, flags: Flags) -> (Self, iced::Task<Message>) {
        let app = Self::new(service, flags);
        let tasks = [
            app.refresh_status(),
            app.load_profiles(),
            app.load_rules(),
            app.load_settings(),
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
                return self.refresh_status();
            }
            Message::Event(ev) => match ev {
                ServiceEvent::StateChanged(_) => return self.refresh_status(),
                ServiceEvent::LinkUpdated(link) => {
                    if let Some(s) = self.status.as_mut() {
                        if let Some(slot) = s.links.iter_mut().find(|l| l.name == link.name) {
                            *slot = link;
                        }
                    }
                }
                ServiceEvent::Throughput { up_bps, down_bps } => {
                    self.throughput = Some((up_bps, down_bps));
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
            Message::StatusLoaded(s) => self.status = Some(s),
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
            Message::SettingsLoaded(s) => {
                self.settings = s.clone();
                self.seed_text = s.theme_seed.to_string();
                self.theme.rederive(s.theme_seed, s.dark_mode);
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
        }
        iced::Task::none()
    }

    // ------------------------------------------------------------------
    // View + theme + subscriptions
    // ------------------------------------------------------------------

    /// The MVU view function (wired into `iced::application`).
    pub fn view(&self) -> Element<'_> {
        let content = match self.screen {
            Screen::Status => screens::status(self),
            Screen::Servers => screens::servers(self),
            Screen::Routing => screens::routing(self),
            Screen::Settings => screens::settings(self),
            Screen::Logs => screens::logs(self),
        };
        let rail = screens::nav_rail(self);
        let s = self.theme.scheme;
        iced::widget::container(iced::widget::row![rail, content])
            .style(crate::theme::page(s))
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
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
        iced::Subscription::batch([events, tick_subscription()])
    }

    /// Whether the connect switch should read "on".
    pub fn is_connected(&self) -> bool {
        matches!(
            self.status.as_ref().map(|s| s.state),
            Some(ServiceState::Connected)
        )
    }
}

/// Stable subscription ids (iced deduplicates by id per render).
const EVENTS_SUB_ID: &str = "mt-app-events";

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

/// Launches the GUI with the given service backend (the root crate calls
/// this with its `RealService`; the `mt-app` binary passes a `MockService`).
pub fn run_app(service: Arc<dyn MtService>, flags: Flags) -> Result<(), crate::Error> {
    iced::application("MultiTunnel", App::update, App::view)
        .window(iced::window::Settings {
            size: iced::Size::new(1080.0, 720.0),
            min_size: Some(iced::Size::new(880.0, 580.0)),
            ..Default::default()
        })
        .theme(App::theme)
        .subscription(App::subscription)
        .run_with(move || App::init(service, flags))?;
    Ok(())
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

    /// Rule editor model: add, edit, remove, dirty detection.
    #[test]
    fn rule_editor_add_edit_remove() {
        let saved = vec![RuleRow {
            kind: "domain-suffix".into(),
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

        let report = run_smoke(&mut app, 5, Duration::from_millis(10)).expect("smoke ok");
        assert_eq!(report.ticks, 5);
        assert_eq!(report.views_built, 5);
        assert!(app.status.is_some());
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
}
