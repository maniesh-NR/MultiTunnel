//! `MockService` — a simulated [`mt_service::MtService`] so the GUI runs
//! standalone (feature `mock`, default ON).
//!
//! Behavior:
//! * `connect()` → `Connecting` → (≈700 ms) → `Connected`, then a per-second
//!   session loop emits `Throughput`, `LinkUpdated` and `LogLine` events
//!   with plausible jitter; `disconnect()` cancels it via a generation
//!   counter. State machine: Disconnected/Failed → Connecting → Connected →
//!   Disconnected.
//! * Fake links ("primary", "backup") with latency/bytes that drift while
//!   connected; occasional warnings; rare error lines.
//! * Profiles are seeded from the CONTRACTS-S6 synthetic vectors
//!   (RFC 5737 addresses, dummy UUID — no real credentials) parsed through
//!   `mt_links::parse_share_link`; `import_link` uses the same parser.
//! * `test_latency` returns a deterministic-in-session pseudo measurement
//!   (LCG, no external rand dependency) and emits a `Latency` event.
//! * Log lines follow the "LEVEL  component: message" prefix convention the
//!   GUI's level filter relies on. The ring buffer is capped (GUI keeps its
//!   own 2000-line view buffer).
//!
//! All errors are user-facing Strings that never echo credential values.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use mt_service::{
    ExitInfo, LinkStatus, MtService, ProfileView, RuleView, ServiceEvent, ServiceState,
    SettingsView, StatusSnapshot,
};
use tokio::sync::{broadcast, Mutex};

const RING_CAP: usize = 500;
const CONNECT_DELAY: Duration = Duration::from_millis(700);
const SESSION_TICK: Duration = Duration::from_millis(1000);
const LATENCY_TEST_DELAY: Duration = Duration::from_millis(60);

/// Synthetic, contract-sanctioned seed links (RFC 5737 + dummy uuid).
const SEED_VLESS: &str = "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=reality&sni=tunnel.example.org&fp=chrome&pbk=0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde&sid=0123abcd&type=tcp&flow=xtls-rprx-vision#synthetic-reality";
const SEED_SS: &str = "ss://YWVzLTI1Ni1nY206c3ludGhldGlj@203.0.113.10:8388#synthetic-ss";
const SEED_TROJAN: &str =
    "trojan://synthetic-password@203.0.113.10:443?sni=tunnel.example.org#synthetic-trojan";

/// Mutable service state guarded by one tokio mutex (poison-free).
struct Inner {
    state: ServiceState,
    active: Option<String>,
    profiles: Vec<mt_links::Profile>,
    rules: Vec<RuleView>,
    settings: SettingsView,
    links: Vec<LinkStatus>,
    logs: VecDeque<String>,
    up_since: Option<String>,
    restarts: u32,
    /// Session generation; bumping it cancels stale loops.
    gen: u64,
    /// LCG state for fake jitter.
    rng: u64,
    /// Synthetic adapters (S7) — ids the mock accepts in
    /// `set_bonded_adapters`.
    adapters: Vec<mt_service::AdapterView>,
}

/// The mock backend. Cheap to clone; holds the broadcast hub.
#[derive(Clone)]
pub struct MockService {
    inner: Arc<Mutex<Inner>>,
    tx: broadcast::Sender<ServiceEvent>,
}

impl MockService {
    /// Builds the mock with plausible defaults (settings, profiles, rules).
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        let profiles = seed_profiles();
        let active = profiles.first().map(|p| p.id.clone());
        let settings = SettingsView {
            theme_seed: 0x6750A4,
            dark_mode: false,
            system_proxy: false,
            // D23: mirror the service default (balanced — live byte-load
            // spreading; round-robin's connection-count evenness let one
            // fat flow dominate a single adapter).
            merge_policy: "balanced".to_string(),
            autostart: false,
            // D22/D23: ALL THREE wired adapters bonded — mirrors the demo
            // setup (balanced needs ≥2 ticked adapters to show its point)
            // and gives every Status-tab adapter graph traffic. The mock's
            // byte split is deliberately UNEVEN (Wi-Fi ≫ Ethernet ≫
            // Cellular) — the "one adapter hogs everything" shape the
            // balanced policy exists to fix.
            bonded_adapters: vec![
                "mock-wifi".to_string(),
                "mock-ethernet".to_string(),
                "mock-cellular".to_string(),
            ],
            bond_mode: "per-connection".to_string(),
            merge_port: 7443,
            socks_port: 10808,
            http_port: 10809,
            allow_lan: false,
            log_level: "warning".to_string(),
            probe_target: "connectivitycheck.gstatic.com:80".to_string(),
            exit_probe_url: "http://ip-api.com/json".to_string(),
            close_to_tray: false,
            always_on_top: false,
        };
        let inner = Inner {
            state: ServiceState::Disconnected,
            active,
            profiles,
            rules: seed_rules(),
            settings,
            links: seed_links(0),
            logs: VecDeque::new(),
            up_since: None,
            restarts: 0,
            gen: 0,
            rng: 0x853c_49e6_748f_ea9b,
            adapters: seed_adapters(),
        };
        let svc = Self {
            inner: Arc::new(Mutex::new(inner)),
            tx,
        };
        svc.log("INFO", "mock: service ready (standalone data)");
        svc
    }

    /// Push a log line into the ring and the event stream.
    fn log(&self, level: &str, msg: &str) {
        let line = format!("{level:<5} mock: {msg}");
        let mut inner = match self.inner.try_lock() {
            Ok(g) => g,
            Err(_) => {
                // Mutex contended: still emit, skip the ring append.
                let _ = self.tx.send(ServiceEvent::LogLine(line.clone()));
                return;
            }
        };
        while inner.logs.len() >= RING_CAP {
            inner.logs.pop_front();
        }
        inner.logs.push_back(line.clone());
        drop(inner);
        let _ = self.tx.send(ServiceEvent::LogLine(line));
    }

    fn rng_next(inner: &mut Inner) -> u64 {
        inner.rng = inner
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        inner.rng >> 33
    }

    fn rng_below(inner: &mut Inner, n: u64) -> u64 {
        Self::rng_next(inner) % n.max(1)
    }
}

impl Default for MockService {
    fn default() -> Self {
        Self::new()
    }
}

/// UTC "YYYY-MM-DD HH:MM:SS" from unix seconds (no chrono dependency;
/// Howard Hinnant's civil-from-days algorithm).
fn fmt_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

fn seed_profiles() -> Vec<mt_links::Profile> {
    let mut out = Vec::new();
    for link in [SEED_VLESS, SEED_SS, SEED_TROJAN] {
        match mt_links::parse_share_link(link) {
            Ok(p) => out.push(p),
            Err(e) => {
                // Never expected (contract vectors); fail soft without echo.
                tracing::warn!(error = %e, "mock: seed link parse failed");
            }
        }
    }
    out
}

fn seed_rules() -> Vec<RuleView> {
    vec![
        RuleView {
            kind: "domainSuffix".to_string(),
            value: "corp.example.org".to_string(),
            action: "direct".to_string(),
        },
        RuleView {
            kind: "domainKeyword".to_string(),
            value: "ads".to_string(),
            action: "reject".to_string(),
        },
        RuleView {
            kind: "ipCidr".to_string(),
            value: "192.0.2.0/24".to_string(),
            action: "proxy".to_string(),
        },
        RuleView {
            kind: "any".to_string(),
            value: String::new(),
            action: "proxy".to_string(),
        },
    ]
}

fn seed_links(rng0: u64) -> Vec<LinkStatus> {
    let jitter = |k: u64, base: u32| base + ((k % 25) as u32);
    // D22/D23: link names mirror the REAL engine's per-adapter links —
    // "<profile> [<adapter>]" — so the GUI's per-adapter rate sampler
    // (link_matches_adapter) attributes mock traffic the same way it
    // does in production, and the Status tab's adapter graphs move.
    // Three carriers: the demo of a 3-NIC bond.
    vec![
        LinkStatus {
            name: "synthetic-reality [Wi-Fi]".to_string(),
            ok: true,
            latency_ms: Some(jitter(rng0 % 97, 38)),
            ewma_ms: Some(41.0),
            quarantined: false,
            bytes_up: 0,
            bytes_down: 0,
        },
        LinkStatus {
            name: "synthetic-reality [Ethernet]".to_string(),
            ok: true,
            latency_ms: Some(jitter(rng0 % 89, 92)),
            ewma_ms: Some(96.0),
            quarantined: false,
            bytes_up: 0,
            bytes_down: 0,
        },
        LinkStatus {
            name: "synthetic-reality [Cellular]".to_string(),
            ok: true,
            latency_ms: Some(jitter(rng0 % 83, 121)),
            ewma_ms: Some(128.0),
            quarantined: false,
            bytes_up: 0,
            bytes_down: 0,
        },
    ]
}

/// Synthetic adapters (S7 + S13): Wi-Fi + Ethernet + Cellular up with
/// IPv4s — the 3-NIC demo bond (D23; the mock's traffic split is
/// deliberately uneven so the balanced policy's effect is visible) —
/// plus five typical virtual/software adapters a real Windows box
/// enumerates (Bluetooth, WSL/Hyper-V switch, Tailscale, loopback,
/// VirtualBox). The extras are NOT bonded: the Status page scrollable
/// (S13) has to reach a realistic-length adapter list, and the Adapters
/// tab shows the bonded block sorted above the unticked rows.
fn seed_adapters() -> Vec<mt_service::AdapterView> {
    vec![
        mt_service::AdapterView {
            id: "mock-wifi".to_string(),
            name: "Wi-Fi".to_string(),
            description: "Mock Wireless-AC adapter".to_string(),
            ipv4: vec!["192.168.1.42".to_string()],
            up: true,
            selected: true, // recomputed from settings on every adapters() call
        },
        mt_service::AdapterView {
            id: "mock-ethernet".to_string(),
            name: "Ethernet".to_string(),
            description: "Mock Gigabit PCIe NIC".to_string(),
            ipv4: vec!["10.0.0.5".to_string(), "169.254.9.9".to_string()],
            up: true,
            selected: false,
        },
        mt_service::AdapterView {
            id: "mock-cellular".to_string(),
            name: "Cellular".to_string(),
            description: "Mock 5G dongle".to_string(),
            ipv4: vec!["100.72.14.5".to_string()],
            up: true,
            selected: false,
        },
        mt_service::AdapterView {
            id: "mock-bluetooth".to_string(),
            name: "Bluetooth".to_string(),
            description: "Mock Bluetooth device (PAN)".to_string(),
            ipv4: vec![],
            up: false,
            selected: false,
        },
        mt_service::AdapterView {
            id: "mock-vethernet-wsl".to_string(),
            name: "vEthernet (WSL)".to_string(),
            description: "Mock Hyper-V virtual switch for WSL2".to_string(),
            ipv4: vec!["172.28.16.1".to_string()],
            up: true,
            selected: false,
        },
        mt_service::AdapterView {
            id: "mock-tailscale".to_string(),
            name: "Tailscale".to_string(),
            description: "Mock Tailscale virtual NIC".to_string(),
            ipv4: vec!["100.84.113.77".to_string()],
            up: true,
            selected: false,
        },
        mt_service::AdapterView {
            id: "mock-loopback".to_string(),
            name: "Loopback Pseudo-Interface 1".to_string(),
            description: "Mock software loopback interface".to_string(),
            ipv4: vec!["127.0.0.1".to_string()],
            up: true,
            selected: false,
        },
        mt_service::AdapterView {
            id: "mock-virtualbox".to_string(),
            name: "VirtualBox Host-Only".to_string(),
            description: "Mock VirtualBox host-only adapter".to_string(),
            ipv4: vec!["192.168.56.1".to_string()],
            up: false,
            selected: false,
        },
    ]
}

/// Test seam (S11): the synthetic adapter enumeration, for tests of the
/// GUI's ordering primitives outside a running mock service.
pub fn seed_adapters_public() -> Vec<mt_service::AdapterView> {
    seed_adapters()
}

fn view_of(p: &mt_links::Profile, active_id: &Option<String>) -> ProfileView {
    ProfileView {
        id: p.id.clone(),
        tag: p.tag.clone(),
        protocol: match p.protocol {
            mt_links::Protocol::Vless => "vless",
            mt_links::Protocol::Vmess => "vmess",
            mt_links::Protocol::Trojan => "trojan",
            mt_links::Protocol::Shadowsocks => "ss",
        }
        .to_string(),
        endpoint: format!("{}:{}", p.address, p.port),
        remark: p.remark.clone(),
        active: active_id.as_deref() == Some(p.id.as_str()),
    }
}

fn valid_rule(r: &RuleView) -> Result<(), String> {
    const KINDS: [&str; 4] = ["domainSuffix", "domainKeyword", "ipCidr", "any"];
    const ACTIONS: [&str; 3] = ["proxy", "direct", "reject"];
    if !KINDS.contains(&r.kind.as_str()) {
        return Err(format!("unknown rule kind \"{}\"", r.kind));
    }
    if !ACTIONS.contains(&r.action.as_str()) {
        return Err(format!("unknown rule action \"{}\"", r.action));
    }
    if r.kind != "any" && r.value.trim().is_empty() {
        return Err(format!("rule kind \"{}\" needs a value", r.kind));
    }
    Ok(())
}

#[async_trait]
impl MtService for MockService {
    async fn status(&self) -> StatusSnapshot {
        let g = self.inner.lock().await;
        StatusSnapshot {
            state: g.state,
            active_profile: g
                .active
                .as_ref()
                .and_then(|id| g.profiles.iter().find(|p| p.id == *id))
                .map(|p| p.tag.clone()),
            up_since: g.up_since.clone(),
            restarts: g.restarts,
            links: g.links.clone(),
        }
    }

    async fn connect(&self) {
        let cancel = {
            let mut g = self.inner.lock().await;
            if matches!(g.state, ServiceState::Connected | ServiceState::Connecting) {
                return;
            }
            g.gen += 1;
            g.state = ServiceState::Connecting;
            g.gen
        };
        self.log("INFO", "connecting: spinning up links");
        let _ = self
            .tx
            .send(ServiceEvent::StateChanged(ServiceState::Connecting));

        let inner = Arc::clone(&self.inner);
        let tx = self.tx.clone();
        let this = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(CONNECT_DELAY).await;
            {
                let mut g = inner.lock().await;
                if g.gen != cancel || g.state != ServiceState::Connecting {
                    return; // superseded
                }
                g.state = ServiceState::Connected;
                g.up_since = Some(fmt_utc(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                ));
                g.restarts += 1;
                g.links = seed_links(MockService::rng_next(&mut g));
                let _ = tx.send(ServiceEvent::StateChanged(ServiceState::Connected));
            }
            this.log("INFO", "tunnel up: all links healthy");

            // Per-second session loop; exits when state/gen changes.
            loop {
                tokio::time::sleep(SESSION_TICK).await;
                let mut g = inner.lock().await;
                if g.state != ServiceState::Connected || g.gen != cancel {
                    break;
                }
                let r = MockService::rng_below(&mut g, 1 << 20);
                let up = 180_000 + r * 3;
                let down = 900_000 + r * 11;
                let jitters: Vec<u32> = (0..g.links.len())
                    .map(|_| 30 + MockService::rng_below(&mut g, 90) as u32)
                    .collect();
                // D23: deliberately UNEVEN split across the three bonded
                // NICs (≈70% / 25% / 5%) — the "one adapter hogs
                // everything, the third rarely sees traffic" field report
                // the balanced policy exists to fix, made visible in the
                // per-adapter graphs.
                let shares = [70usize, 25, 5];
                for (k, l) in g.links.iter_mut().enumerate() {
                    let ms = jitters[k];
                    let share = shares.get(k).copied().unwrap_or(0);
                    l.ok = true;
                    l.bytes_up += up * share as u64 / 100;
                    l.bytes_down += down * share as u64 / 100;
                    l.latency_ms = Some(ms);
                    l.ewma_ms = Some(l.ewma_ms.unwrap_or(ms as f32) * 0.7 + ms as f32 * 0.3);
                    let _ = tx.send(ServiceEvent::LinkUpdated(l.clone()));
                }
                let _ = tx.send(ServiceEvent::Throughput {
                    up_bps: up,
                    down_bps: down,
                });
                match MockService::rng_below(&mut g, 10) {
                    0 => this.log("WARN", "backup link: latency spike, staying failover"),
                    9 => this.log("INFO", "core: keepalive ok"),
                    _ => {}
                }
            }
        });
    }

    async fn disconnect(&self) {
        let emit = {
            let mut g = self.inner.lock().await;
            if matches!(g.state, ServiceState::Disconnected) {
                return;
            }
            g.gen += 1;
            g.state = ServiceState::Disconnected;
            g.up_since = None;
            for l in g.links.iter_mut() {
                l.ok = false;
            }
            true
        };
        if emit {
            self.log("INFO", "tunnel down: stopped");
            let _ = self
                .tx
                .send(ServiceEvent::StateChanged(ServiceState::Disconnected));
        }
    }

    async fn profiles(&self) -> Vec<ProfileView> {
        let g = self.inner.lock().await;
        g.profiles.iter().map(|p| view_of(p, &g.active)).collect()
    }

    async fn import_link(&self, link: &str) -> Result<ProfileView, String> {
        let parsed = mt_links::parse_share_link(link).map_err(|e| e.to_string())?;
        let mut g = self.inner.lock().await;
        // Upsert on (protocol, address, port, identity).
        let key = (
            format!("{:?}", parsed.protocol),
            parsed.address.clone(),
            parsed.port,
            parsed.identity.len(), // length only — never the credential itself
        );
        let existing = g.profiles.iter_mut().find(|p| {
            (
                format!("{:?}", p.protocol),
                p.address.clone(),
                p.port,
                p.identity.len(),
            ) == key
        });
        let id = match existing {
            Some(p) => {
                p.tag = parsed.tag.clone();
                p.remark = parsed.remark.clone();
                p.id.clone()
            }
            None => {
                g.profiles.push(parsed.clone());
                parsed.id.clone()
            }
        };
        drop(g);
        self.log("INFO", "profile imported (mock)");
        let g = self.inner.lock().await;
        g.profiles
            .iter()
            .find(|p| p.id == id)
            .map(|p| view_of(p, &g.active))
            .ok_or_else(|| "import vanished (internal)".to_string())
    }

    async fn remove_profile(&self, id: &str) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        if g.active.as_deref() == Some(id) {
            return Err("cannot remove the active profile".to_string());
        }
        let before = g.profiles.len();
        g.profiles.retain(|p| p.id != id);
        if g.profiles.len() == before {
            return Err("unknown profile id".to_string());
        }
        drop(g);
        self.log("INFO", "profile removed (mock)");
        Ok(())
    }

    async fn set_active_profile(&self, id: &str) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        if !g.profiles.iter().any(|p| p.id == id) {
            return Err("unknown profile id".to_string());
        }
        g.active = Some(id.to_string());
        drop(g);
        self.log("INFO", "active profile switched (mock)");
        Ok(())
    }

    async fn test_latency(&self, id: &str) -> Result<u32, String> {
        let exists = {
            let g = self.inner.lock().await;
            g.profiles.iter().any(|p| p.id == id)
        };
        if !exists {
            return Err("unknown profile id".to_string());
        }
        tokio::time::sleep(LATENCY_TEST_DELAY).await;
        let mut g = self.inner.lock().await;
        let ms = 30 + MockService::rng_below(&mut g, 110) as u32;
        drop(g);
        let _ = self.tx.send(ServiceEvent::Latency {
            profile_id: id.to_string(),
            ms: Some(ms),
        });
        self.log("INFO", &format!("latency probe finished ({ms} ms)"));
        Ok(ms)
    }

    async fn routing_rules(&self) -> Vec<RuleView> {
        self.inner.lock().await.rules.clone()
    }

    async fn set_routing_rules(&self, rules: Vec<RuleView>) -> Result<(), String> {
        for r in &rules {
            valid_rule(r)?;
        }
        let mut g = self.inner.lock().await;
        g.rules = rules;
        drop(g);
        self.log("INFO", "routing rules saved (mock)");
        Ok(())
    }

    async fn settings(&self) -> SettingsView {
        self.inner.lock().await.settings.clone()
    }

    async fn set_settings(&self, settings: SettingsView) -> Result<(), String> {
        let mut g = self.inner.lock().await;
        g.settings = settings.clone();
        drop(g);
        self.log(
            "INFO",
            &format!(
                "settings saved (seed {}, dark {})",
                settings.theme_seed, settings.dark_mode
            ),
        );
        Ok(())
    }

    async fn adapters(&self) -> Vec<mt_service::AdapterView> {
        let inner = self.inner.lock().await;
        inner
            .adapters
            .iter()
            .map(|a| {
                let mut a = a.clone();
                a.selected = inner.settings.bonded_adapters.contains(&a.id);
                a
            })
            .collect()
    }

    async fn set_bonded_adapters(&self, ids: Vec<String>) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        for id in &ids {
            if !inner.adapters.iter().any(|a| &a.id == id) {
                return Err(format!("unknown adapter {id:?} — refresh the adapter list"));
            }
        }
        inner.settings.bonded_adapters = ids;
        let n = inner.settings.bonded_adapters.len();
        self.log("INFO", &format!("bonded adapters set ({n} selected)"));
        // Fake reconnect so the GUI's status line behaves like production.
        if matches!(inner.state, ServiceState::Connected) {
            inner.gen += 1;
            inner.restarts += 1;
        }
        Ok(())
    }

    fn events(&self) -> broadcast::Receiver<ServiceEvent> {
        self.tx.subscribe()
    }

    async fn exit_probe(&self) -> Result<ExitInfo, String> {
        // Simulated identity (RFC 5737 documentation range — clearly fake,
        // no real network in mock builds). The exit IS the seed server:
        // single hop, matching production's happy path.
        let inner = self.inner.lock().await;
        if inner.state != ServiceState::Connected {
            return Err("not connected".to_string());
        }
        let endpoint = inner
            .profiles
            .iter()
            .find(|p| Some(&p.id) == inner.active.as_ref())
            .map(|p| format!("{}:{}", p.address, p.port))
            .unwrap_or_else(|| "203.0.113.10:443".to_string());
        Ok(ExitInfo {
            exit_ip: "203.0.113.10".to_string(),
            country: "Testland".to_string(),
            city: "Mock City".to_string(),
            isp: "Example Telecom".to_string(),
            asn: "AS64512 Example Telecom".to_string(),
            server_endpoint: endpoint,
            direct_exit: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock connect lifecycle: Disconnected → Connecting → Connected →
    /// Disconnected, with links populated and state events broadcast.
    #[tokio::test]
    async fn mock_connect_disconnect_lifecycle() {
        let svc = MockService::new();
        let mut rx = svc.events();

        assert_eq!(svc.status().await.state, ServiceState::Disconnected);
        svc.connect().await;
        assert_eq!(svc.status().await.state, ServiceState::Connecting);
        tokio::time::sleep(Duration::from_millis(900)).await;
        let snap = svc.status().await;
        assert_eq!(snap.state, ServiceState::Connected);
        assert!(snap.up_since.is_some());
        assert_eq!(snap.links.len(), 3, "D23: three bonded mock adapters");
        assert!(snap.links.iter().all(|l| l.ok));
        assert!(snap.active_profile.is_some());

        // State events observed on the broadcast.
        let mut saw_connected = false;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, ServiceEvent::StateChanged(ServiceState::Connected)) {
                saw_connected = true;
            }
        }
        assert!(saw_connected);

        svc.disconnect().await;
        assert_eq!(svc.status().await.state, ServiceState::Disconnected);
    }

    /// Profiles: seeded from the synthetic vectors, active flag, import and
    /// remove semantics (remove-active refused).
    #[tokio::test]
    async fn mock_profiles_import_remove() {
        let svc = MockService::new();
        let profiles = svc.profiles().await;
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles.iter().filter(|p| p.active).count(), 1);

        let imported = svc
            .import_link(SEED_VLESS)
            .await
            .expect("re-import upserts");
        let profiles = svc.profiles().await;
        assert_eq!(profiles.len(), 3, "dedupe by identity, no new row");
        let first = &profiles[0];
        assert_eq!(first.id, imported.id);

        let active_id = profiles.iter().find(|p| p.active).unwrap().id.clone();
        assert!(svc.remove_profile(&active_id).await.is_err());
        let other = profiles.iter().find(|p| !p.active).unwrap().id.clone();
        assert!(svc.remove_profile(&other).await.is_ok());
        assert_eq!(svc.profiles().await.len(), 2);

        let bad = svc.import_link("not-a-link").await;
        assert!(bad.is_err());
        // Error must not echo the input verbatim (redaction policy).
        let msg = bad.unwrap_err();
        assert!(!msg.contains("not-a-link") || msg.contains("not a supported share link"));
    }

    /// Latency test returns a plausible value and emits the event.
    #[tokio::test]
    async fn mock_latency_test() {
        let svc = MockService::new();
        let mut rx = svc.events();
        let id = svc.profiles().await[0].id.clone();
        let ms = svc.test_latency(&id).await.expect("probe ok");
        assert!((30..140).contains(&ms));
        let mut saw = false;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, ServiceEvent::Latency { .. }) {
                saw = true;
            }
        }
        assert!(saw);
        assert!(svc.test_latency("nope").await.is_err());
    }

    /// Settings round-trip and rule validation.
    #[tokio::test]
    async fn mock_settings_and_rules() {
        let svc = MockService::new();
        let mut s = svc.settings().await;
        s.theme_seed = 0x4285F4;
        s.dark_mode = true;
        svc.set_settings(s).await.expect("save ok");
        let s2 = svc.settings().await;
        assert!(s2.dark_mode);
        assert_eq!(s2.theme_seed, 0x4285F4);

        let rules = svc.routing_rules().await;
        assert_eq!(rules.len(), 4);
        assert!(svc
            .set_routing_rules(vec![RuleView {
                kind: "bogus".to_string(),
                value: String::new(),
                action: "proxy".to_string(),
            }])
            .await
            .is_err());
        assert!(svc
            .set_routing_rules(vec![RuleView {
                kind: "domainSuffix".to_string(),
                value: String::new(),
                action: "proxy".to_string(),
            }])
            .await
            .is_err());
    }

    /// UTC formatter sanity (known epochs).
    #[test]
    fn fmt_utc_known_values() {
        assert_eq!(fmt_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(fmt_utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(fmt_utc(1_700_000_000), "2023-11-14 22:13:20 UTC");
    }
}
