//! Connection lifecycle orchestration (stage S2 — implementer agent S2-A).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `State`: Disconnected / Connecting / Connected / Failed (Serialize).
//! - `Snapshot`: state, error, bond_error, sys_error, probe_error, merging,
//!   links, raw_status, system_proxy, restarts (Serialize).
//! - `Supervisor::new(Config)`, `connect(&self) -> Result<()>`,
//!   `disconnect(&self) -> Result<()>`, `snapshot(&self) -> Snapshot`,
//!   accessors log_dir()/state_dir()/net_merge_enabled().
//! - CONNECT ORDER (must not change):
//!   1 write xrayconf::render to paths.xray_config (0600 private write)
//!   2 core start + wait_for_port socks (15s)
//!   3 socks_probe (fail fast → stop core, restore proxy, State::Failed)
//!   4 netmerge::ensure_running + netmerge::on (NON-FATAL: bond_error set)
//!   5 sysproxy::set(http_addr) if behavior.system_proxy (NON-FATAL)
//! - DISCONNECT: reverse 5→2 (sysproxy::restore, netmerge::off if merging,
//!   core stop).
//! - Fail-open: core death → restart w/ backoff 1s,2s,... cap 30s up to
//!   behavior.restart_max (0 = never); exhausted → restore proxy,
//!   State::Failed. Background thread polls every 5s (netmerge status →
//!   snapshot) and probes every behavior.probe_interval_sec (15s timeout).
//! - connect() when already connected/connecting = no-op Ok.
//! - Stale sysproxy self-heal: connect() starts with sysproxy::restore()
//!   best-effort (covers previous abrupt Ctrl+C exits).
//! - Tests: backoff sequence math; probe-stub-driven connect failure path
//!   (bad server → Failed, no system proxy change); snapshot serde.
//!
//! IMPLEMENTATION NOTES (S2-A2 decisions, "pick one and document it"):
//! - Every method takes `&self`; mutable state lives in `Arc<Shared>` so the
//!   stage-S3 tray can wrap the `Supervisor` in an `Arc` and call from menu
//!   handlers on any thread.
//! - One poll-loop thread per connect cycle. Double spawn is prevented by
//!   the connect() state gate + operation lock (`disconnect()` joins the
//!   previous cycle's thread first); a loop never outlives its cycle. The
//!   5 s tick sleeps in 100 ms slices, so a disconnect is honored within
//!   ~100 ms while the loop is between blocking calls; worst case one
//!   wait_for_port/probe window (~15 s) finishes first (S2-REV-C P2-1).
//! - A restart "campaign" owns the backoff for one core death: it re-checks
//!   the stop flag between attempts (a disconnect during a campaign aborts
//!   it cleanly), flips the snapshot to Connecting while retrying, counts
//!   every spawn attempt into `Snapshot::restarts`, and on exhaustion runs
//!   the fail-closed path (core stop, sysproxy restore, `State::Failed`).
//! - The consumed-once `exited_unexpectedly` flag is swallowed at the
//!   connect reset point (right after the probe succeeds), so a reconnect
//!   after a give-up cannot launch a spurious restart campaign against a
//!   healthy core; the tick additionally treats `!core.running()` as a death
//!   signal, which covers the flag-consumed window.
//! - `disconnect` is best-effort and ALWAYS ends `Disconnected`/Ok: a failed
//!   `net-merge off` is recorded in `bond_error` rather than failing the
//!   call — matching the fail-open philosophy of the connect path.
//! - `emergency_stop(&self)`: additive (S3-REV-C P1-1) op-lock-free emergency
//!   teardown for the tray's exit-timeout path — kill the core, restore the
//!   system proxy, flip to `Failed`; see the method docs for the semantics.
//! - `Snapshot::restarts` counts spawn attempts made by restart campaigns
//!   since the last successful connect (reset at the connect reset point).

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::core_proc::CoreProc;
use crate::error::Result;
use crate::netmerge::{self, Link};
use crate::sysproxy;
use crate::xrayconf;

/// Poll tick of the background loop (contract: 5 s).
const TICK: Duration = Duration::from_secs(5);
/// Granularity of stoppable sleeps — bounds disconnect latency (~100 ms).
const SLEEP_SLICE: Duration = Duration::from_millis(100);
/// `wait_for_port` timeout after a core start (contract: 15 s).
const PORT_WAIT: Duration = Duration::from_secs(15);
/// Initial tunnel probe timeout (contract: 20 s).
const PROBE_WAIT: Duration = Duration::from_secs(20);
/// Background probe timeout (contract: 15 s).
const POLL_PROBE_WAIT: Duration = Duration::from_secs(15);
/// `netmerge::ensure_running` timeout (contract: 15 s).
const BOND_WAIT: Duration = Duration::from_secs(15);
/// Restart backoff base in seconds (contract: 1 s doubling).
const BACKOFF_BASE: u64 = 1;
/// Restart backoff cap (contract: 30 s).
const BACKOFF_CAP: Duration = Duration::from_secs(30);
/// Backoff shift ceiling: 1<<5 s = 32 s ≥ cap, so every attempt past the
/// fifth returns the cap (see [`backoff_delay`]).
const BACKOFF_MAX_SHIFT: u32 = 5;

/// Poison-tolerant lock: no panics in non-test code. A poisoned mutex here
/// only means another thread panicked mid-IO — the guarded data stays valid.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Connection lifecycle state (Serialize per contract; Deserialize added so
/// snapshots can round-trip through JSON in tests and in the tray).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

/// Point-in-time view for the tray/UI (Serialize per contract).
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub state: State,
    pub error: Option<String>,
    pub bond_error: Option<String>,
    pub sys_error: Option<String>,
    pub probe_error: Option<String>,
    pub merging: bool,
    pub links: Vec<Link>,
    pub raw_status: String,
    pub system_proxy: bool,
    pub restarts: u32,
}

/// Internal mutable state guarded by one mutex (mirrors [`Snapshot`]).
#[derive(Debug)]
struct Inner {
    state: State,
    error: Option<String>,
    bond_error: Option<String>,
    sys_error: Option<String>,
    probe_error: Option<String>,
    merging: bool,
    links: Vec<Link>,
    raw_status: String,
    system_proxy: bool,
    restarts: u32,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            state: State::Disconnected,
            error: None,
            bond_error: None,
            sys_error: None,
            probe_error: None,
            merging: false,
            links: Vec::new(),
            raw_status: String::new(),
            system_proxy: false,
            restarts: 0,
        }
    }
}

impl Inner {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            state: self.state,
            error: self.error.clone(),
            bond_error: self.bond_error.clone(),
            sys_error: self.sys_error.clone(),
            probe_error: self.probe_error.clone(),
            merging: self.merging,
            links: self.links.clone(),
            raw_status: self.raw_status.clone(),
            system_proxy: self.system_proxy,
            restarts: self.restarts,
        }
    }
}

/// State shared between the `Supervisor` handle and its poll-loop thread.
#[derive(Debug)]
struct Shared {
    inner: Mutex<Inner>,
    /// Serializes whole connect()/disconnect() operations (S2-REV-C P0-2,
    /// S2-REV-B P1-3): a disconnect during an in-flight connect sequence
    /// previously joined a None loop handle, returned Ok, and the sequence
    /// then finished into Connected with the system proxy re-set.
    op_lock: Mutex<()>,
    /// Per-cycle stop signal: checked every tick and every 100 ms slice.
    stop: AtomicBool,
    /// JoinHandle of the live poll loop (joined by `disconnect`).
    loop_thread: Mutex<Option<JoinHandle<()>>>,
    /// Loop-alive diagnostic flag; double-spawn prevention is the
    /// connect() state gate + `op_lock` (S2-REV-B P2-4).
    loop_running: AtomicBool,
}

/// Orchestrates the xray core, net-merge bonding and the system proxy.
#[derive(Debug)]
pub struct Supervisor {
    cfg: Config,
    core: Arc<CoreProc>,
    shared: Arc<Shared>,
}

impl Supervisor {
    /// Creates a supervisor handle for `cfg` (nothing is spawned yet).
    pub fn new(cfg: Config) -> Self {
        let core = Arc::new(CoreProc::new(
            cfg.paths.xray.clone(),
            cfg.paths.xray_config.clone(),
            cfg.paths.log_dir.clone(),
        ));
        Self {
            cfg,
            core,
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner::default()),
                op_lock: Mutex::new(()),
                stop: AtomicBool::new(false),
                loop_thread: Mutex::new(None),
                loop_running: AtomicBool::new(false),
            }),
        }
    }

    // ---- accessors (contract) -------------------------------------------

    pub fn log_dir(&self) -> &Path {
        &self.cfg.paths.log_dir
    }

    pub fn state_dir(&self) -> &Path {
        &self.cfg.paths.state_dir
    }

    pub fn net_merge_enabled(&self) -> bool {
        self.cfg.net_merge.enable
    }

    /// True while the background poll loop of the current connect cycle is
    /// alive (diagnostics/tray convenience; not part of the frozen contract).
    pub fn loop_running(&self) -> bool {
        self.shared.loop_running.load(Ordering::SeqCst)
    }

    /// Point-in-time snapshot of the lifecycle state.
    pub fn snapshot(&self) -> Snapshot {
        lock(&self.shared.inner).snapshot()
    }

    // ---- connect ---------------------------------------------------------

    /// Brings the tunnel up (see the module contract for the exact order).
    /// No-op `Ok` while Connecting/Connected; a full retry from Failed.
    pub fn connect(&self) -> Result<()> {
        // Whole-operation serialization: see `Shared::op_lock` docs.
        let _op_guard = lock(&self.shared.op_lock);
        {
            let mut inner = lock(&self.shared.inner);
            if matches!(inner.state, State::Connecting | State::Connected) {
                return Ok(()); // contract: no-op when already up/connecting
            }
            inner.state = State::Connecting;
            inner.error = None;
        }
        match self.connect_sequence() {
            Ok(()) => {
                lock(&self.shared.inner).state = State::Connected;
                self.spawn_loop();
                Ok(())
            }
            Err(e) => {
                let mut inner = lock(&self.shared.inner);
                inner.state = State::Failed;
                inner.error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// Steps 0–5 of the connect order. Fail-fast steps (1–3) return `Err`;
    /// bonding and sysproxy failures are recorded but never fatal.
    fn connect_sequence(&self) -> Result<()> {
        // 0. stale sysproxy self-heal (covers abrupt Ctrl+C exits) — best
        //    effort, never blocks connecting.
        let _ = sysproxy::restore(self.state_dir());

        // 0.5 loopback guard (S2-REV-C P1-3, contract #2): the SOCKS inbound
        //     is noauth — it must never bind beyond loopback.
        ensure_loopback_listen(&self.cfg.local.listen)?;

        // 1. write the generated client config (private: 0600 on unix).
        self.write_xray_config()?;

        // 2. core start + wait for the SOCKS port.
        self.core.start()?;
        if let Err(e) = CoreProc::wait_for_port(&socks_addr(&self.cfg), PORT_WAIT) {
            self.core.stop(); // never leave a half-started core behind
            return Err(e);
        }

        // 3. full-chain probe — fail fast: stop core, restore proxy, Failed.
        let (probe_host, probe_port) = match parse_probe_target(&self.cfg.behavior.probe_target) {
            Some(v) => v,
            None => {
                self.core.stop();
                return Err(crate::err!(
                    "invalid behavior.probeTarget '{}': expected host:port",
                    self.cfg.behavior.probe_target
                ));
            }
        };
        if let Err(e) =
            CoreProc::socks_probe(&socks_addr(&self.cfg), &probe_host, probe_port, PROBE_WAIT)
        {
            self.core.stop();
            let _ = sysproxy::restore(self.state_dir());
            lock(&self.shared.inner).system_proxy = false;
            return Err(crate::err!(
                "tunnel probe failed ({}:{} through socks {}): {}",
                probe_host,
                probe_port,
                socks_addr(&self.cfg),
                e
            ));
        }

        // Reset point (contract): steps 1–3 succeeded → fresh error/counters.
        {
            let mut inner = lock(&self.shared.inner);
            inner.bond_error = None;
            inner.sys_error = None;
            inner.probe_error = None;
            inner.restarts = 0;
            inner.merging = false;
            inner.links = Vec::new();
            inner.raw_status = String::new();
        }
        // Swallow a stale consumed-once exit flag from a previous cycle so
        // the new loop cannot fire a spurious restart campaign.
        let _ = self.core.exited_unexpectedly();

        // 4. bonding — NON-FATAL (bond_error captures the failure).
        if self.cfg.net_merge.enable {
            let bin = self.cfg.paths.net_merge.to_string_lossy().into_owned();
            match netmerge::ensure_running(&bin, BOND_WAIT).and_then(|()| netmerge::on(&bin)) {
                Ok(()) => self.refresh_netmerge_status(),
                Err(e) => lock(&self.shared.inner).bond_error = Some(e.to_string()),
            }
        }

        // 5. system proxy — NON-FATAL (sys_error captures the failure).
        if self.cfg.behavior.system_proxy {
            match sysproxy::set(
                &http_addr(&self.cfg),
                &self.cfg.behavior.proxy_override,
                self.state_dir(),
            ) {
                Ok(()) => lock(&self.shared.inner).system_proxy = true,
                Err(e) => lock(&self.shared.inner).sys_error = Some(e.to_string()),
            }
        }
        Ok(())
    }

    /// Render + write the client config with a main-style private write.
    fn write_xray_config(&self) -> Result<()> {
        let blob = xrayconf::render(&self.cfg)?;
        let path = self.cfg.paths.xray_config.clone();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        write_private(&path, &(blob + "\n"))
    }

    /// Best-effort `net-merge status` refresh into the snapshot (bond_error
    /// is cleared on success, updated on failure).
    fn refresh_netmerge_status(&self) {
        if !self.cfg.net_merge.enable {
            return;
        }
        let bin = self.cfg.paths.net_merge.to_string_lossy().into_owned();
        match netmerge::status(&bin) {
            Ok(st) => {
                let mut inner = lock(&self.shared.inner);
                inner.merging = st.merging;
                inner.links = st.links;
                inner.raw_status = st.raw;
                inner.bond_error = None;
            }
            Err(e) => lock(&self.shared.inner).bond_error = Some(e.to_string()),
        }
    }

    // ---- poll loop -------------------------------------------------------

    /// Spawns the per-cycle poll loop (guarded against double spawn).
    fn spawn_loop(&self) {
        // The connect() state gate + op_lock guarantee the previous cycle is
        // over; join its exiting thread instead of racing the narrow
        // loop_running window (S2-REV-B P2-4: the old early-return could
        // leave a new cycle Connected with no poll loop at all).
        let mut slot = lock(&self.shared.loop_thread);
        if let Some(h) = slot.take() {
            drop(slot);
            let _ = h.join();
            slot = lock(&self.shared.loop_thread);
        }
        self.shared.stop.store(false, Ordering::SeqCst);
        self.shared.loop_running.store(true, Ordering::SeqCst);
        let core = Arc::clone(&self.core);
        let shared = Arc::clone(&self.shared);
        let cfg = self.cfg.clone();
        *slot = Some(thread::spawn(move || poll_loop(core, shared, cfg)));
    }

    /// Tears the tunnel down: stop + join the poll loop first, then reverse
    /// sysproxy → netmerge → core. Best-effort; always ends Disconnected.
    pub fn disconnect(&self) -> Result<()> {
        // Whole-operation serialization: see `Shared::op_lock` docs.
        let _op_guard = lock(&self.shared.op_lock);
        if lock(&self.shared.inner).state == State::Disconnected {
            return Ok(()); // contract: no-op when already down
        }
        // 1. stop the poll loop and JOIN it (never leak threads).
        self.shared.stop.store(true, Ordering::SeqCst);
        let handle = lock(&self.shared.loop_thread).take();
        if let Some(h) = handle {
            let _ = h.join();
        }

        // 2. system proxy restore (reverse of step 5).
        let _ = sysproxy::restore(self.state_dir());

        // 3. net-merge off if our merging flag says we turned it on.
        if lock(&self.shared.inner).merging {
            let bin = self.cfg.paths.net_merge.to_string_lossy().into_owned();
            let off_res = netmerge::off(&bin);
            let mut inner = lock(&self.shared.inner);
            if let Err(e) = off_res {
                inner.bond_error = Some(e.to_string());
            }
            inner.merging = false;
            inner.links = Vec::new();
            inner.raw_status = String::new();
        }

        // 4. core stop (safe + idempotent even when already dead).
        self.core.stop();

        let mut inner = lock(&self.shared.inner);
        inner.system_proxy = false;
        inner.sys_error = None;
        inner.probe_error = None;
        inner.error = None;
        inner.state = State::Disconnected;
        Ok(())
    }

    /// Emergency teardown for the tray's exit-timeout path (S3-REV-C P1-1).
    ///
    /// When a normal [`disconnect`](Self::disconnect) worker is still blocked
    /// on the `op_lock` (an in-flight `connect` can hold it up to ~50 s) or
    /// sits inside a probe / `wait_for_port` / `netmerge::off` window, and the
    /// process must exit NOW, this brings the dangerous parts down WITHOUT
    /// taking the operation lock:
    ///
    /// 1. set the poll-loop stop flag (the loop exits within ~100 ms; the
    ///    next `spawn_loop` resets it, so future connects are not poisoned),
    /// 2. `core.stop()` — kill + reap the xray child (CoreProc owns its own
    ///    mutexes; no `op_lock` involved, idempotent),
    /// 3. `sysproxy::restore(state_dir)` — clear the system proxy from the
    ///    saved snapshot (best-effort, fast registry IO),
    /// 4. snapshot: `State::Failed` with a "emergency stop after exit
    ///    timeout" error; `merging`/`system_proxy` flags cleared.
    ///
    /// net-merge is deliberately NOT turned off here: `netmerge::off` runs
    /// the CLI with a 10 s timeout (it could itself exceed the exit budget
    /// and race a lock-holding worker); the next `connect()` re-syncs it via
    /// `ensure_running`. Never blocks on `op_lock`; does not synchronize
    /// with an in-flight `connect`/`disconnect` (that is the point — it is
    /// the lock-free last resort); idempotent and safe from any thread.
    pub fn emergency_stop(&self) {
        // 1. Ask the current cycle's poll loop to stop (see doc point 1).
        self.shared.stop.store(true, Ordering::SeqCst);
        // 2. Kill + reap the core.
        self.core.stop();
        // 3. Clear the system proxy.
        let _ = sysproxy::restore(self.state_dir());
        // 4. Reflect the emergency state in the snapshot.
        let mut inner = lock(&self.shared.inner);
        inner.state = State::Failed;
        inner.error = Some("emergency stop after exit timeout".into());
        inner.merging = false;
        inner.system_proxy = false;
    }
}

// ---- free functions (poll loop body lives outside the handle) -------------

/// Poll-loop body (one thread per connect cycle): 5 s ticks — restart the
/// core after an unexpected death (backoff campaign), refresh the net-merge
/// status, and probe the tunnel on its own cadence. Exits when the stop flag
/// is set (disconnect) or after a give-up (state already Failed).
fn poll_loop(core: Arc<CoreProc>, shared: Arc<Shared>, cfg: Config) {
    let socks = socks_addr(&cfg);
    let probe_target = parse_probe_target(&cfg.behavior.probe_target);
    let probe_interval = Duration::from_secs(u64::from(cfg.behavior.probe_interval_sec).max(1));
    let mut next_probe = std::time::Instant::now() + probe_interval;
    let nm_bin = cfg.paths.net_merge.to_string_lossy().into_owned();

    loop {
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        if !stoppable_sleep(&shared.stop, TICK) {
            break;
        }
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }

        // Core death → restart campaign (consumes the once-flag; a missing
        // child with a consumed flag is also treated as a death).
        if core.exited_unexpectedly() || !core.running() {
            match restart_campaign(&core, &shared, &cfg, &socks) {
                CampaignOutcome::Restarted => {
                    next_probe = std::time::Instant::now() + probe_interval;
                }
                // GaveUp: state is Failed; Stopped: disconnect is in flight.
                CampaignOutcome::GaveUp | CampaignOutcome::Stopped => break,
            }
        }

        // net-merge status refresh into the snapshot (bond-only, non-fatal).
        if cfg.net_merge.enable {
            match netmerge::status(&nm_bin) {
                Ok(st) => {
                    let mut inner = lock(&shared.inner);
                    inner.merging = st.merging;
                    inner.links = st.links;
                    inner.raw_status = st.raw;
                    inner.bond_error = None;
                }
                Err(e) => lock(&shared.inner).bond_error = Some(e.to_string()),
            }
        }

        // Tunnel probe on its own cadence (contract: 15 s timeout here).
        if std::time::Instant::now() >= next_probe {
            next_probe = std::time::Instant::now() + probe_interval;
            if let Some((ref host, port)) = probe_target {
                match CoreProc::socks_probe(&socks, host, port, POLL_PROBE_WAIT) {
                    Ok(()) => lock(&shared.inner).probe_error = None,
                    Err(e) => lock(&shared.inner).probe_error = Some(e.to_string()),
                }
            }
        }
    }
    shared.loop_running.store(false, Ordering::SeqCst);
}

/// Outcome of one restart campaign.
enum CampaignOutcome {
    /// Core is back up and the port answers again.
    Restarted,
    /// Restart budget exhausted → proxy restored, State::Failed (loop ends).
    GaveUp,
    /// Disconnect observed mid-campaign (loop ends; disconnect cleans up).
    Stopped,
}

/// Try to bring a dead core back: backoff 1s,2s,4s… capped at 30 s, at most
/// `behavior.restart_max` spawn attempts (0 = never → immediate give-up).
fn restart_campaign(
    core: &CoreProc,
    shared: &Shared,
    cfg: &Config,
    socks: &str,
) -> CampaignOutcome {
    lock(&shared.inner).state = State::Connecting;
    let max = cfg.behavior.restart_max;
    let mut attempt: u32 = 0;
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            return CampaignOutcome::Stopped;
        }
        attempt += 1;
        if max == 0 || attempt > max {
            // Fail-closed: core stop, proxy restore, Failed.
            core.stop();
            let _ = sysproxy::restore(&cfg.paths.state_dir);
            let mut inner = lock(&shared.inner);
            inner.system_proxy = false;
            inner.state = State::Failed;
            inner.error = Some(format!(
                "xray core exited unexpectedly; giving up after {} restart attempt(s) (restartMax={max})",
                attempt - 1
            ));
            return CampaignOutcome::GaveUp;
        }
        if !stoppable_sleep(&shared.stop, backoff_delay(attempt)) {
            return CampaignOutcome::Stopped;
        }
        let started = match core.start() {
            Ok(()) => true,
            Err(e) => {
                if core.running() {
                    // Stale death flag with a live child: treat as started
                    // and let the port wait below verify it.
                    true
                } else {
                    let mut inner = lock(&shared.inner);
                    inner.restarts = inner.restarts.saturating_add(1);
                    inner.error = Some(format!("restart attempt {attempt}/{max} failed: {e}"));
                    continue;
                }
            }
        };
        if started {
            match CoreProc::wait_for_port(socks, PORT_WAIT) {
                Ok(()) => {
                    let mut inner = lock(&shared.inner);
                    inner.restarts = inner.restarts.saturating_add(1);
                    inner.state = State::Connected;
                    inner.error = None;
                    return CampaignOutcome::Restarted;
                }
                Err(e) => {
                    core.stop();
                    let mut inner = lock(&shared.inner);
                    inner.restarts = inner.restarts.saturating_add(1);
                    inner.error = Some(format!(
                        "restart attempt {attempt}/{max}: core did not open {socks}: {e}"
                    ));
                }
            }
        }
    }
}

/// Backoff for restart attempt `attempt` (1-based): 1s, 2s, 4s, 8s, 16s,
/// then capped at 30 s forever. Attempt 0 is clamped to 1 s.
fn backoff_delay(attempt: u32) -> Duration {
    let shift = attempt.max(1).saturating_sub(1).min(BACKOFF_MAX_SHIFT);
    Duration::from_secs(BACKOFF_BASE << shift).min(BACKOFF_CAP)
}

/// Sleep `dur` in 100 ms slices; `false` as soon as the stop flag is set.
fn stoppable_sleep(stop: &AtomicBool, dur: Duration) -> bool {
    let deadline = std::time::Instant::now() + dur;
    while std::time::Instant::now() < deadline {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        thread::sleep(SLEEP_SLICE.min(deadline - std::time::Instant::now()));
    }
    !stop.load(Ordering::SeqCst)
}

/// Loopback enforcement for lib.rs contract #2 (S2-REV-C P1-3): the core's
/// noauth SOCKS inbound (and the HTTP inbound) must never bind beyond
/// loopback. "localhost" is allowed; any other non-loopback address is not.
fn ensure_loopback_listen(listen: &str) -> Result<()> {
    if listen.eq_ignore_ascii_case("localhost") {
        return Ok(());
    }
    match listen.parse::<std::net::IpAddr>() {
        Ok(ip) if ip.is_loopback() => Ok(()),
        Ok(ip) => Err(crate::err!(
            "local.listen '{listen}' ({ip}) is not loopback — refusing to expose \
             the unauthenticated SOCKS/HTTP inbound (architecture contract #2)"
        )),
        Err(e) => Err(crate::err!(
            "local.listen '{listen}' is not a valid IP address: {e}"
        )),
    }
}

/// `"host:port"` → `(host, port)`; `None` when it does not parse.
pub fn parse_probe_target(spec: &str) -> Option<(String, u16)> {
    let (host, port) = spec.trim().rsplit_once(':')?;
    if host.is_empty() || port.is_empty() {
        return None;
    }
    let port = port.parse().ok()?;
    Some((host.trim().to_string(), port))
}

/// The local SOCKS listen address, `listen:socksPort`.
fn socks_addr(cfg: &Config) -> String {
    format!("{}:{}", cfg.local.listen, cfg.local.socks_port)
}

/// The local HTTP listen address, `listen:httpPort` (WinINET ProxyServer).
fn http_addr(cfg: &Config) -> String {
    format!("{}:{}", cfg.local.listen, cfg.local.http_port)
}

/// Main-style private write: POSIX 0600; Windows default ACL (reviewer
/// finding S1-REV-C #1 — the config embeds the VLESS credential).
fn write_private(path: &Path, contents: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(contents.as_bytes())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    // Unix-only test imports (rig/stub/process tests) — gating keeps the
    // windows-target test build clean (S2 close: cross-target clippy).
    #[cfg(unix)]
    use std::io::{Read as _, Write as _};
    #[cfg(unix)]
    use std::net::TcpListener;
    #[cfg(unix)]
    use std::sync::mpsc;

    /// Unique temp dir (pid + counter): keeps parallel tests apart.
    fn unique_dir(tag: &str) -> PathBuf {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "mt-a2-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("create temp dir");
        d
    }

    /// Grab an ephemeral loopback port and release it (the port is then dead).
    #[cfg(unix)]
    fn free_loopback_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        l.local_addr().expect("local_addr").port()
    }

    fn server() -> crate::config::Server {
        crate::config::Server {
            address: "203.0.113.10".into(),
            port: 8443,
            uuid: "11111111-2222-3333-4444-555555555555".into(),
            sni: "tunnel.example.org".into(),
            public_key: "pubkey-abc".into(),
            short_id: "0123abcd".into(),
        }
    }

    /// Fake "xray": a shell script that ignores `run -c …` and either stays
    /// alive (`sleep 120`) or dies after `die_after` seconds.
    #[cfg(unix)]
    fn write_fake_xray(dir: &Path, die_after: Option<&str>) -> PathBuf {
        let p = dir.join("fake-xray.sh");
        let body = match die_after {
            Some(t) => format!("#!/bin/sh\nsleep {t}\n"),
            None => "#!/bin/sh\nexec sleep 120\n".to_string(),
        };
        fs::write(&p, body).expect("write fake xray");
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).expect("chmod +x");
        p
    }

    /// SOCKS5 stub in the spirit of core_proc's tests: answers each CONNECT
    /// with a success reply, or — when `refuse` — accepts and immediately
    /// drops the socket so the probe can never complete. Serves any number
    /// of connects.
    ///
    /// Deterministic shutdown (root cause of the S2 suite hang): the listener
    /// is set NONBLOCKING and the loop re-checks `quit` every ~10 ms. Merely
    /// dropping the rig's listener fd is NOT enough — the stub owns a
    /// try_clone'd fd, so the socket stays open and accept() blocks forever;
    /// Drop's join then hung the whole suite on every rig test.
    #[cfg(unix)]
    fn spawn_stub(listener: TcpListener, refuse: bool, quit: Arc<AtomicBool>) -> JoinHandle<()> {
        listener
            .set_nonblocking(true)
            .expect("set stub listener nonblocking");
        thread::spawn(move || {
            loop {
                if quit.load(Ordering::SeqCst) {
                    break;
                }
                let mut s = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(_) => break, // listener closed for real
                };
                if refuse {
                    drop(s);
                    continue;
                }
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                let mut g = [0u8; 3];
                if s.read_exact(&mut g).is_err() {
                    continue;
                }
                if s.write_all(&[0x05, 0x00]).is_err() {
                    continue;
                }
                let mut head = [0u8; 4];
                if s.read_exact(&mut head).is_err() {
                    continue;
                }
                let mut l = [0u8; 1];
                if s.read_exact(&mut l).is_err() {
                    continue;
                }
                let mut hb = vec![0u8; l[0] as usize];
                if s.read_exact(&mut hb).is_err() {
                    continue;
                }
                let mut p = [0u8; 2];
                if s.read_exact(&mut p).is_err() {
                    continue;
                }
                // VER REP RSV ATYP(01) + BND.ADDR(4) + BND.PORT(2)
                let _ = s.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
            }
        })
    }

    use std::path::PathBuf;

    /// Test rig: bound SOCKS listener (the "port the core opens"), SOCKS stub
    /// thread, fake xray binary, isolated config/dirs.
    #[cfg(unix)]
    struct Rig {
        sup: Arc<Supervisor>,
        socks_listener: Option<TcpListener>,
        stub: Option<JoinHandle<()>>,
        quit: Arc<AtomicBool>,
        dir: PathBuf,
    }

    #[cfg(unix)]
    impl Rig {
        /// `refuse` makes the SOCKS stub kill every probe; `die_after` makes
        /// the fake core exit after N seconds; `net_merge_bin` enables the
        /// (non-fatal) bonding step against a given CLI path; `restart_max`
        /// feeds behavior.restartMax (0 = never restart, give up at once).
        fn build(
            refuse: bool,
            die_after: Option<&str>,
            net_merge_bin: Option<&str>,
            restart_max: u32,
        ) -> Rig {
            let dir = unique_dir("rig");
            let socks_listener = TcpListener::bind("127.0.0.1:0").expect("bind socks listener");
            let socks_port = socks_listener.local_addr().expect("addr").port();
            let http_port = free_loopback_port();
            let xray = write_fake_xray(&dir, die_after);
            let cfg = Config {
                server: server(),
                local: crate::config::Local {
                    listen: "127.0.0.1".into(),
                    socks_port,
                    http_port,
                },
                paths: crate::config::Paths {
                    xray,
                    net_merge: PathBuf::from(net_merge_bin.unwrap_or("net-merge")),
                    xray_config: dir.join("xray-client.json"),
                    log_dir: dir.join("logs"),
                    state_dir: dir.join("state"),
                },
                net_merge: crate::config::NetMerge {
                    enable: net_merge_bin.is_some(),
                },
                behavior: crate::config::Behavior {
                    system_proxy: true,
                    proxy_override: "localhost".into(),
                    probe_target: "example.com:443".into(),
                    probe_interval_sec: 60,
                    restart_max,
                },
            };
            let sup = Arc::new(Supervisor::new(cfg));
            let quit = Arc::new(AtomicBool::new(false));
            let stub = spawn_stub(
                socks_listener.try_clone().expect("clone listener"),
                refuse,
                Arc::clone(&quit),
            );
            Rig {
                sup,
                socks_listener: Some(socks_listener),
                stub: Some(stub),
                quit,
                dir,
            }
        }

        /// Deterministic disconnect: joined via channel + recv_timeout so a
        /// stuck loop join can never hang the test.
        fn joined_disconnect(&self) {
            let sup = Arc::clone(&self.sup);
            let (tx, rx) = mpsc::channel::<()>();
            let watcher = thread::spawn(move || {
                sup.disconnect().expect("disconnect");
                let _ = tx.send(());
            });
            assert!(
                rx.recv_timeout(Duration::from_secs(20)).is_ok(),
                "disconnect must join the poll loop promptly"
            );
            watcher.join().expect("watcher thread");
        }
    }

    #[cfg(unix)]
    impl Drop for Rig {
        fn drop(&mut self) {
            // Flip quit FIRST: the stub's nonblocking loop exits within
            // ~10 ms, so the join below is bounded (this was the hang).
            self.quit.store(true, Ordering::SeqCst);
            let _ = self.sup.disconnect(); // no-op when already down
            drop(self.socks_listener.take());
            if let Some(h) = self.stub.take() {
                let _ = h.join();
            }
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    /// (1) Backoff math: 1s,2s,4s,8s,16s → 30s cap, then stays 30s forever.
    #[test]
    fn backoff_sequence_doubles_then_caps_at_thirty() {
        let want = [1u64, 2, 4, 8, 16, 30, 30, 30, 30];
        for (i, w) in want.iter().enumerate() {
            assert_eq!(
                backoff_delay(i as u32 + 1).as_secs(),
                *w,
                "attempt {}",
                i + 1
            );
        }
        assert_eq!(backoff_delay(500).as_secs(), 30, "far past the cap");
        assert_eq!(backoff_delay(u32::MAX).as_secs(), 30, "no overflow panic");
        assert_eq!(backoff_delay(0).as_secs(), 1, "attempt is clamped to >= 1");
    }

    /// (2) Snapshot serde round-trip through JSON (links are Serialize-only
    /// in netmerge, so they are verified at the Value level).
    #[test]
    fn snapshot_round_trips_through_json() {
        let snap = Snapshot {
            state: State::Connected,
            error: None,
            bond_error: Some("bond down".into()),
            sys_error: None,
            probe_error: Some("timeout".into()),
            merging: true,
            links: vec![Link {
                name: "Wi-Fi".into(),
                detail: "12.4 Mbit/s".into(),
            }],
            raw_status: "Merging: on".into(),
            system_proxy: true,
            restarts: 2,
        };
        let json = serde_json::to_string(&snap).expect("serialize snapshot");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(v["state"], "Connected");
        assert!(v["error"].is_null());
        assert_eq!(v["bond_error"], "bond down");
        assert!(v["sys_error"].is_null());
        assert_eq!(v["probe_error"], "timeout");
        assert_eq!(v["merging"], true);
        assert_eq!(v["raw_status"], "Merging: on");
        assert_eq!(v["system_proxy"], true);
        assert_eq!(v["restarts"], 2);
        assert_eq!(v["links"][0]["name"], "Wi-Fi");
        assert_eq!(v["links"][0]["detail"], "12.4 Mbit/s");

        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Scalars {
            state: State,
            error: Option<String>,
            bond_error: Option<String>,
            sys_error: Option<String>,
            probe_error: Option<String>,
            merging: bool,
            raw_status: String,
            system_proxy: bool,
            restarts: u32,
        }
        let back: Scalars = serde_json::from_str(&json).expect("typed round-trip");
        assert_eq!(back.state, State::Connected);
        assert_eq!(back.bond_error.as_deref(), Some("bond down"));
        assert_eq!(back.probe_error.as_deref(), Some("timeout"));
        assert_eq!(back.restarts, 2);
        assert!(back.merging && back.system_proxy);
    }

    /// (3) probeTarget parsing.
    #[test]
    fn probe_target_parsing() {
        assert_eq!(
            parse_probe_target("example.com:80"),
            Some(("example.com".into(), 80))
        );
        assert_eq!(
            parse_probe_target(" 9.9.9.9:53 "),
            Some(("9.9.9.9".into(), 53))
        );
        assert_eq!(parse_probe_target("no-port"), None);
        assert_eq!(parse_probe_target(":80"), None);
        assert_eq!(parse_probe_target("host:notaport"), None);
        assert_eq!(parse_probe_target(""), None);
    }

    /// (3b) loopback guard (S2-REV-C P1-3).
    #[test]
    fn loopback_listen_is_enforced() {
        assert!(ensure_loopback_listen("127.0.0.1").is_ok());
        assert!(ensure_loopback_listen("127.9.9.9").is_ok());
        assert!(ensure_loopback_listen("::1").is_ok());
        assert!(ensure_loopback_listen("localhost").is_ok());
        assert!(ensure_loopback_listen("LOCALHOST").is_ok());
        for bad in [
            "0.0.0.0",
            "192.168.1.5",
            "10.0.0.1",
            "169.254.0.1",
            "::",
            "bogus",
            "",
        ] {
            let err =
                ensure_loopback_listen(bad).expect_err("non-loopback listen must be rejected");
            assert!(err.0.contains("local.listen"), "{bad}: {}", err.0);
        }
    }

    /// (3c) whole-operation serialization (S2-REV-C P0-2): a connect()
    /// blocked on op_lock must not even move the state to Connecting.
    #[test]
    #[cfg(unix)]
    fn connect_waits_for_op_lock_of_in_flight_operation() {
        let rig = Rig::build(true, None, None, 3); // refused probe → fast Err
                                                   // Hold the operation lock via one Arc clone; a second clone runs
                                                   // connect() in a thread — it must block until the guard drops.
        let lock_holder = Arc::clone(&rig.sup);
        let guard = lock(&lock_holder.shared.op_lock);
        let (tx, rx) = mpsc::channel::<()>();
        let runner = Arc::clone(&rig.sup);
        let t = thread::spawn(move || {
            let _ = runner.connect();
            let _ = tx.send(());
        });
        thread::sleep(Duration::from_millis(300));
        assert!(
            matches!(rig.sup.snapshot().state, State::Disconnected),
            "connect() must block on the operation lock before touching state"
        );
        drop(guard);
        rx.recv_timeout(Duration::from_secs(10))
            .expect("connect finishes once released");
        t.join().expect("connect thread");
        assert!(matches!(rig.sup.snapshot().state, State::Failed));
        rig.joined_disconnect();
    }

    /// (3e) Emergency stop (S3-REV-C P1-1): the tray's exit path calls this
    /// when the normal disconnect worker exceeds its budget — it must work
    /// WHILE op_lock is held by someone else (never block), kill the core,
    /// clear the proxy/merging flags and flip the state to Failed with the
    /// emergency error, promptly.
    #[test]
    #[cfg(unix)]
    fn emergency_stop_works_while_op_lock_is_held() {
        let rig = Rig::build(false, None, None, 3);
        rig.sup.connect().expect("connect against the working stub");
        assert!(matches!(rig.sup.snapshot().state, State::Connected));
        assert!(
            rig.sup.core.running(),
            "fake core must be alive after connect"
        );

        // The exit-race: an operation holds op_lock, so a normal
        // disconnect() would block behind it for the op's full budget.
        let guard = lock(&rig.sup.shared.op_lock);

        let started = std::time::Instant::now();
        rig.sup.emergency_stop();
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "emergency_stop must not block on op_lock (took {elapsed:?})"
        );

        assert!(!rig.sup.core.running(), "core must be killed + reaped");
        let s = rig.sup.snapshot();
        assert!(matches!(s.state, State::Failed), "state: {:?}", s.state);
        assert_eq!(
            s.error.as_deref(),
            Some("emergency stop after exit timeout"),
            "error: {:?}",
            s.error
        );
        assert!(!s.merging, "merging flag cleared");
        assert!(!s.system_proxy, "proxy flag cleared");
        drop(guard);

        // The old cycle's loop saw the stop flag and exits; the rig's
        // deterministic disconnect (op_lock now free) joins it and lands
        // Disconnected — proving emergency_stop left no stuck state.
        rig.joined_disconnect();
        assert!(matches!(rig.sup.snapshot().state, State::Disconnected));
    }

    /// (4) Probe-driven failure path: the in-test stub refuses every probe →
    /// connect() fails cleanly (State::Failed, proxy flag down, no panic)
    /// and — because the fake xray stays alive until killed — the retry
    /// reaching the probe again proves the core was actually STOPPED.
    #[test]
    #[cfg(unix)]
    fn connect_fails_cleanly_when_probe_is_refused() {
        let rig = Rig::build(true, None, None, 3);

        let err1 = rig
            .sup
            .connect()
            .expect_err("refused probe must fail connect");
        assert!(err1.to_string().contains("probe"), "{err1}");
        let s1 = rig.sup.snapshot();
        assert!(matches!(s1.state, State::Failed), "state: {:?}", s1.state);
        assert!(
            s1.error.as_deref().unwrap_or("").contains("probe"),
            "error: {:?}",
            s1.error
        );
        assert!(!s1.system_proxy, "fail-fast must restore the proxy flag");
        assert!(s1.bond_error.is_none() && s1.sys_error.is_none());
        assert!(!rig.sup.loop_running(), "no loop for a failed connect");

        // Retry: must fail at the probe again, NOT with "core already
        // running" (which would mean the first core was never stopped).
        let err2 = rig
            .sup
            .connect()
            .expect_err("retry also fails at the probe");
        let msg2 = err2.to_string();
        assert!(
            !msg2.contains("already running"),
            "core was not stopped: {msg2}"
        );
        assert!(msg2.contains("probe"), "{msg2}");
        assert!(matches!(rig.sup.snapshot().state, State::Failed));
    }

    /// (5) Happy path + contract no-ops: connect → Connected with a clean
    /// snapshot, second connect is a no-op, disconnect joins the loop
    /// deterministically and lands Disconnected.
    #[test]
    #[cfg(unix)]
    fn connect_succeeds_reconnect_is_noop_disconnect_joins_loop() {
        let rig = Rig::build(false, None, None, 3);
        rig.sup.connect().expect("connect against the working stub");
        let s = rig.sup.snapshot();
        assert!(matches!(s.state, State::Connected), "{:?}", s.state);
        assert!(s.error.is_none() && s.probe_error.is_none(), "{:?}", s);
        assert!(s.bond_error.is_none() && s.sys_error.is_none(), "{:?}", s);
        assert!(s.system_proxy, "sysproxy::set is a no-op Ok on non-Windows");
        assert_eq!(s.restarts, 0);
        assert!(rig.sup.loop_running(), "poll loop must run after connect");

        // connect-when-connected: no-op Ok, nothing changes.
        rig.sup.connect().expect("no-op reconnect");
        let s2 = rig.sup.snapshot();
        assert!(matches!(s2.state, State::Connected));
        assert_eq!(s2.restarts, 0);
        assert!(rig.sup.loop_running());

        // disconnect-when-connected: joins the loop (recv_timeout pattern).
        rig.joined_disconnect();
        let s3 = rig.sup.snapshot();
        assert!(matches!(s3.state, State::Disconnected));
        assert!(!rig.sup.loop_running(), "loop must be joined by disconnect");
    }

    /// (6) Bonding is NON-FATAL: a missing net-merge CLI sets bond_error but
    /// the tunnel still comes up.
    #[test]
    #[cfg(unix)]
    fn bonding_failure_is_non_fatal() {
        let rig = Rig::build(false, None, Some("definitely-missing-net-merge-xyz"), 3);
        rig.sup
            .connect()
            .expect("connect must succeed despite bond failure");
        let s = rig.sup.snapshot();
        assert!(matches!(s.state, State::Connected), "{:?}", s.state);
        let bond = s
            .bond_error
            .expect("bond_error must capture the net-merge failure");
        assert!(bond.contains("definitely-missing-net-merge-xyz"), "{bond}");
        assert!(!s.merging, "nothing is merging");
        rig.joined_disconnect();
    }

    /// (7) Core dies right after connect with restartMax=0 → the loop's next
    /// tick gives up immediately: proxy flag down, State::Failed, loop ends.
    #[test]
    #[cfg(unix)]
    fn core_death_with_restart_max_zero_fails_fast() {
        let rig = Rig::build(false, Some("0.3"), None, 0);
        rig.sup.connect().expect("initial connect");
        assert!(matches!(rig.sup.snapshot().state, State::Connected));

        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let s = rig.sup.snapshot();
            if matches!(s.state, State::Failed) {
                let msg = s.error.unwrap_or_default();
                assert!(msg.contains("restart") || msg.contains("exited"), "{msg}");
                assert!(!s.system_proxy, "give-up must restore the proxy flag");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "never reached Failed: {:?}",
                rig.sup.snapshot()
            );
            thread::sleep(Duration::from_millis(200));
        }
        assert!(
            !rig.sup.loop_running(),
            "loop must have ended after give-up"
        );
        rig.joined_disconnect(); // joins the finished loop + cleans up
    }

    /// (8) disconnect-when-disconnected: a no-op, twice, no processes needed.
    #[test]
    fn disconnect_when_disconnected_is_a_noop() {
        let dir = unique_dir("noop");
        let cfg = Config {
            server: server(),
            local: crate::config::Local::default(),
            paths: crate::config::Paths {
                xray: dir.join("xray"),
                net_merge: dir.join("net-merge"),
                xray_config: dir.join("cfg.json"),
                log_dir: dir.join("logs"),
                state_dir: dir.join("state"),
            },
            net_merge: crate::config::NetMerge::default(),
            behavior: crate::config::Behavior::default(),
        };
        let sup = Supervisor::new(cfg);
        sup.disconnect()
            .expect("disconnect from scratch is a no-op");
        assert!(matches!(sup.snapshot().state, State::Disconnected));
        sup.disconnect()
            .expect("second disconnect is still a no-op");
        assert!(matches!(sup.snapshot().state, State::Disconnected));
        assert!(!sup.loop_running());
        let _ = fs::remove_dir_all(&dir);
    }
}
