//! Managed engine child process ([`ManagedEngine`]) + `mt_net::Outbound`.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert E"):
//! - `ManagedEngine::new(spec, locator: Arc<dyn EngineLocator>, state_dir,
//!   log_dir)` — lazily spawns on first connect; free loopback port via
//!   bind(127.0.0.1:0)-then-drop (retry once with a fresh port if the
//!   child fails to bind); config `state_dir/<stem>.json` 0600
//!   (embeds the UUID); child `xray run -c <path>` with piped
//!   stdout/stderr appended to `log_dir/<stem>.log` and forwarded
//!   to tracing; kill-on-drop; unexpected-exit detection → respawn with
//!   backoff 1s→2s→…→30s, budget 5, then the outbound errors (fail-open
//!   at the data plane is mt-net's job, not ours).
//! - `#[async_trait] impl Outbound` — connects through the engine's
//!   private loopback SOCKS via `mt_net::Socks5Outbound`; `probe` = full
//!   socks CONNECT through the engine (real tunnel RTT).
//! - `with_command(test_hook)` — alternate binary+args for tests.
//!
//! IMPLEMENTATION NOTES (S6-E decisions, "pick one and document it"):
//! - CHILD MODEL: `std::process::Child` (NOT `tokio::process`) in a
//!   poison-tolerant `std::sync::Mutex` slot — the exact `core_proc`
//!   discipline. Kill-on-drop is then a plain sync `Drop` (no runtime
//!   needed), and the proven waiter/pump thread design carries over. The
//!   async layer only resolves binaries (locator), waits for ports,
//!   sleeps the backoff and speaks SOCKS5 (via mt-net).
//! - WAITER ARCHITECTURE (ported from `core_proc`): a per-cycle waiter
//!   thread polls `try_wait()` every 100 ms (lock held only per poll),
//!   reaps the child, clears the slot on death, and logs unexpected
//!   exits. Every waiter carries its cycle's generation token and exits
//!   when a newer cycle owns the slot, so respawns never leave stale
//!   watchers. Connect paths never trust the 100 ms poll: they ask
//!   `try_wait()` directly (the `core_proc::child_alive` discipline — a
//!   success must be attributable to OUR child). Any exit we did not
//!   cause (including a clean exit 0) is "unexpected".
//! - PORT-PICK RETRY: a free port is grabbed with bind(127.0.0.1:0)-then-
//!   drop. The TOCTOU race (something else binds it first) shows up as
//!   "port never comes up / child exits first" during the port wait; the
//!   whole spawn (fresh port, fresh config write, fresh header) is then
//!   retried ONCE per spawn cycle. Both tries are ONE respawn attempt;
//!   only after the second try fails does the attempt count against the
//!   respawn budget.
//! - RESPAWN POLICY: on demand, inside `ensure_alive` (first connect
//!   after an unexpected exit). A background respawner was considered and
//!   rejected — two spawn paths means spawn races; the contract ties
//!   respawn to "(connect failure + dead child)". Cold start (never
//!   spawned) gets no delay on attempt 1; respawn attempt n waits
//!   `backoff_base * 2^(n-1)` capped at `backoff_cap` (defaults 1 s → 2 s
//!   → 4 s → 8 s → 16 s, cap 30 s, budget 5). Budget exhaustion marks the
//!   engine DEGRADED permanently: every later connect/probe errors
//!   immediately; drop the engine and build a new one to recover (the GUI
//!   service layer can do that). Concurrent connects serialize on a spawn
//!   mutex and re-check liveness after it, so a respawn episode is shared,
//!   never doubled.
//! - LOG FORWARDING: ONE append-mode file handle per spawn cycle is
//!   shared by the header writer and both pump threads (per-write mutex
//!   exclusivity — `core_proc` S2-REV-C P2-4). stdout → `tracing::info`,
//!   stderr → `tracing::warn`; everything is also appended to
//!   `engine-<tag>.log`. A failed log write silently drops that chunk;
//!   pumps never panic. The header carries tag + port only — never the
//!   config path or its contents (contract test asserts absence).
//! - KILL-ON-DROP: `Drop` sets the manual-stop flag, then `kill()`s the
//!   child through the slot handle (synchronous, runtime-independent);
//!   the waiter thread reaps it. Abandoned spawns (failed port wait)
//!   clean up the same way — manual flag first, so the kill is never
//!   "unexpected".
//! - TUNNEL-LEVEL FAILURES: if the SOCKS5 handshake fails while the child
//!   is ALIVE, the error propagates untouched (mt-net failover decides);
//!   exactly one transparent respawn+retry happens only when the child is
//!   dead.
//! - FILENAMES: the tag is sanitized (`[A-Za-z0-9._-]`, else `-`) before
//!   it enters `<stem>.json` / `<stem>.log` (see [`engine_file_stem`]).
//! - `with_command`/`with_policy` are builder methods that must be used
//!   before the first connect (they mutate through `Arc::get_mut` and
//!   silently no-op once the engine is shared with its threads).

use std::io::Write as _;
use std::net::{Ipv4Addr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::net::TcpStream;
use tokio::time::sleep;
use tracing::Level;

use mt_net::{Outbound, ProxyStream, Socks5Outbound, TargetAddr};

use crate::{generate_config, EngineLocator, EngineSpec};

/// Waiter poll interval (reap latency bound; `core_proc` discipline).
const WAITER_POLL_MS: u64 = 100;
/// Port-up probe step for the spawn wait loop.
const PORT_STEP_MS: u64 = 100;
/// Windows: CREATE_NO_WINDOW — the core runs headless, no console flash
/// (`core_proc` discipline).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One append-mode log handle shared by the header and both output pumps
/// (single-handle discipline: one pump's write is never torn by a
/// concurrent write from the other).
type SharedLog = Arc<Mutex<std::fs::File>>;

/// Poison-tolerant lock (repo discipline).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Alternate binary+args for the child (test hook / `with_command`).
/// The engine appends the config path as the FINAL argument; the default
/// production command is `xray run -c <path>`. A fake engine therefore
/// reads its SOCKS port from the same config file a real Xray would get.
#[derive(Debug, Clone)]
pub struct EngineCommand {
    /// Program to exec.
    pub program: PathBuf,
    /// Arguments placed before the appended config path.
    pub args: Vec<String>,
}

/// Tunables for the spawn/respawn machinery. Defaults implement the
/// contract: budget 5 respawns, backoff 1 s doubling to a 30 s cap,
/// 10 s port-up wait.
#[derive(Debug, Clone, Copy)]
pub struct EnginePolicy {
    /// Respawn attempts per recovery episode before degrading.
    pub max_respawns: u32,
    /// Delay before the first respawn attempt; doubles per attempt.
    pub backoff_base: Duration,
    /// Upper bound on a respawn delay.
    pub backoff_cap: Duration,
    /// Budget for one port-up wait (per spawn try).
    pub port_wait: Duration,
}

impl Default for EnginePolicy {
    fn default() -> Self {
        Self {
            max_respawns: 5,
            backoff_base: Duration::from_secs(1),
            backoff_cap: Duration::from_secs(30),
            port_wait: Duration::from_secs(10),
        }
    }
}

impl EnginePolicy {
    /// Delay before respawn `attempt` (1-based): `base * 2^(attempt-1)`,
    /// capped. Pure and unit-tested.
    fn delay_before_attempt(&self, attempt: u32) -> Duration {
        let exp = attempt.saturating_sub(1).min(30);
        self.backoff_base
            .saturating_mul(2u32.saturating_pow(exp))
            .min(self.backoff_cap)
    }
}

/// Library error surface (`thiserror`; D10: never echoes credentials).
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Respawn budget exhausted; the engine refuses further connects.
    #[error(
        "engine `{tag}` degraded after {attempts} failed respawn attempts (drop and rebuild to recover)"
    )]
    Degraded { tag: String, attempts: u32 },
    /// No binary resolvable and no command override set.
    #[error("engine `{tag}`: no engine binary resolved and no command override set")]
    NoBinary { tag: String },
}

/// Live child state. Guarded by one poison-tolerant `std::sync::Mutex`
/// behind an `Arc` (waiter/pumps never keep the engine itself alive).
#[derive(Default)]
struct LiveState {
    child: Option<Child>,
    socks_addr: Option<SocketAddr>,
    /// Windows kill-on-close job owning `child` (S6-REV-C P1-2): the ONLY
    /// handle lives here, so if this process dies for ANY reason (including
    /// TerminateProcess / Ctrl+C / power loss, where Drop never runs) the
    /// kernel closes the handle and kills the engine. Replaced together
    /// with `child` on every respawn (dropping the old one is safe — the
    /// old child is already dead in the respawn path, and `kill_current`
    /// has already killed it in the abandon path).
    #[cfg(windows)]
    job: Option<JobObjectHandle>,
    /// Cycle token: waiters exit when a newer cycle owns the slot.
    generation: u64,
    /// Set before a kill we caused (drop / abandoned spawn); a death
    /// observed while this is set is NOT unexpected.
    manual_stop: bool,
    /// Set when the respawn budget is exhausted.
    degraded: bool,
    /// True once any spawn attempt has been made (cold-start detection).
    spawned_once: bool,
}

struct EngineInner {
    spec: EngineSpec,
    locator: Option<Arc<dyn EngineLocator>>,
    command: Option<EngineCommand>,
    policy: EnginePolicy,
    state_dir: PathBuf,
    log_dir: PathBuf,
    /// Serializes respawn episodes (tokio Mutex: sleeps happen inside).
    spawn_lock: tokio::sync::Mutex<()>,
    state: Arc<Mutex<LiveState>>,
}

/// A supervised Xray child bound to one link. Kill-on-drop; self-heals
/// with backoff; connects through its private loopback SOCKS port.
pub struct ManagedEngine {
    inner: Arc<EngineInner>,
}

impl ManagedEngine {
    /// Production constructor: resolves the binary through `locator`.
    pub fn new(
        spec: EngineSpec,
        locator: std::sync::Arc<dyn EngineLocator>,
        state_dir: PathBuf,
        log_dir: PathBuf,
    ) -> Self {
        Self {
            inner: Arc::new(EngineInner {
                spec,
                locator: Some(locator),
                command: None,
                policy: EnginePolicy::default(),
                state_dir,
                log_dir,
                spawn_lock: tokio::sync::Mutex::new(()),
                state: Arc::new(Mutex::new(LiveState::default())),
            }),
        }
    }

    /// Builder: run `command` instead of the locator-resolved Xray (test
    /// hook). The config path is appended as the final argument. Must be
    /// called before the first connect.
    pub fn with_command(mut self, command: EngineCommand) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.command = Some(command);
        }
        self
    }

    /// Builder: override the respawn/port-wait policy (tests shrink the
    /// backoff to keep the suite fast). Must be called before the first
    /// connect.
    pub fn with_policy(mut self, policy: EnginePolicy) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.policy = policy;
        }
        self
    }

    /// The link tag (stable `Outbound` name).
    pub fn tag(&self) -> &str {
        &self.inner.spec.tag
    }

    /// Make sure a child is up and return its private loopback SOCKS
    /// address. This is the respawn-episode driver (see module docs).
    pub async fn ensure_alive(&self) -> anyhow::Result<SocketAddr> {
        if let Some(addr) = self.inner.child_addr_if_alive() {
            return Ok(addr);
        }
        let _episode = self.inner.spawn_lock.lock().await;
        // Re-check: a concurrent connect may have finished the respawn.
        if let Some(addr) = self.inner.child_addr_if_alive() {
            return Ok(addr);
        }
        let tag = self.inner.spec.tag.clone();
        {
            let st = lock(&self.inner.state);
            if st.degraded {
                return Err(EngineError::Degraded {
                    tag,
                    attempts: self.inner.policy.max_respawns,
                }
                .into());
            }
        }
        let cold = !lock(&self.inner.state).spawned_once;
        let policy = self.inner.policy;
        let mut attempt: u32 = 0;
        loop {
            if attempt >= policy.max_respawns {
                lock(&self.inner.state).degraded = true;
                tracing::warn!(tag, "respawn budget exhausted; engine degraded");
                return Err(EngineError::Degraded {
                    tag,
                    attempts: attempt,
                }
                .into());
            }
            attempt += 1;
            // Cold start: attempt 1 is immediate. Respawns: 1s→2s→4s→…→cap.
            if !cold || attempt > 1 {
                let delay = policy.delay_before_attempt(attempt);
                tracing::debug!(tag, "respawn attempt {attempt} after {delay:?}");
                sleep(delay).await;
            }
            match self.inner.spawn_cycle().await {
                Ok(addr) => return Ok(addr),
                Err(e) => {
                    tracing::warn!(
                        tag,
                        "respawn attempt {attempt}/{} failed: {e}",
                        policy.max_respawns
                    );
                }
            }
        }
    }
}

impl EngineInner {
    fn tag(&self) -> &str {
        &self.spec.tag
    }

    fn config_path(&self) -> PathBuf {
        self.state_dir
            .join(format!("{}.json", engine_file_stem(&self.spec)))
    }

    fn log_path(&self) -> PathBuf {
        self.log_dir
            .join(format!("{}.log", engine_file_stem(&self.spec)))
    }

    /// Liveness question to OUR child, independent of the 100 ms waiter
    /// poll (`core_proc::child_alive` discipline): a dead child is reaped
    /// on the spot and its address cleared.
    fn child_addr_if_alive(&self) -> Option<SocketAddr> {
        let mut st = lock(&self.state);
        match st.child.as_mut() {
            None => None,
            Some(c) => match c.try_wait() {
                // Alive: the recorded address is ours.
                Ok(None) => st.socks_addr,
                // Exited (reaped here) or broken handle: clear.
                Ok(Some(_)) | Err(_) => {
                    st.child = None;
                    st.socks_addr = None;
                    #[cfg(windows)]
                    {
                        st.job = None;
                    }
                    None
                }
            },
        }
    }

    fn child_alive(&self) -> bool {
        self.child_addr_if_alive().is_some()
    }

    /// Kill the current child (if any) as a MANUAL stop; the waiter (or
    /// the next liveness check) reaps it. Used by Drop and abandoned
    /// spawns.
    fn kill_current(&self) {
        let mut st = lock(&self.state);
        st.manual_stop = true;
        if let Some(c) = st.child.as_mut() {
            let _ = c.kill();
        }
        // Drop the job handle first-hand too: closing it makes the kernel
        // kill anything still inside the job (belt and suspenders with
        // c.kill(); also covers grandchildren the engine may have forked).
        #[cfg(windows)]
        {
            st.job = None;
        }
    }

    /// Kill + SYNCHRONOUSLY REAP + clear the slot (forced-respawn path).
    /// `kill_current` leaves the dying child in the slot for the waiter to
    /// reap — but a just-killed child still answers `try_wait` with
    /// "alive" for a moment, so an immediate `ensure_alive` would hand
    /// back the same stale address. Reaping here closes that window
    /// (bounded: the kill was already delivered). The address is cleared
    /// too, so even a waiter race cannot resurrect it.
    fn kill_and_clear(&self) {
        let mut st = lock(&self.state);
        st.manual_stop = true;
        if let Some(mut c) = st.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        st.socks_addr = None;
        #[cfg(windows)]
        {
            st.job = None;
        }
    }

    /// One respawn attempt: resolve the command, then try the full spawn
    /// up to twice with a FRESH port each time (bind-race retry).
    async fn spawn_cycle(&self) -> anyhow::Result<SocketAddr> {
        lock(&self.state).spawned_once = true;
        let command = self.resolve_command().await?;
        let tag = self.tag();
        let mut last_err: Option<anyhow::Error> = None;
        for try_no in 0..2u32 {
            let addr = pick_free_port()?;
            match self.spawn_once(&command, addr).await {
                Ok(()) => return Ok(addr),
                Err(e) => {
                    tracing::warn!(
                        tag,
                        "spawn try {} failed ({e}); {}",
                        try_no + 1,
                        if try_no == 0 {
                            "retrying with a fresh port"
                        } else {
                            "no tries left"
                        }
                    );
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("spawn failed without an error")))
    }

    /// Resolve the command for this spawn cycle: explicit override wins,
    /// otherwise `locate()` (never downloads) with `ensure()` (may
    /// download) as fallback — the all-in-one app self-provisions on
    /// first connect.
    async fn resolve_command(&self) -> anyhow::Result<EngineCommand> {
        if let Some(cmd) = &self.command {
            return Ok(cmd.clone());
        }
        let tag = self.tag();
        let locator = self.locator.as_ref().ok_or_else(|| EngineError::NoBinary {
            tag: tag.to_owned(),
        })?;
        let bin = match locator.locate() {
            Some(bin) => bin,
            None => locator
                .ensure()
                .await
                .map_err(|e| anyhow::anyhow!("engine `{tag}`: binary download failed: {e}"))?,
        };
        Ok(EngineCommand {
            program: bin,
            args: vec!["run".into(), "-c".into()],
        })
    }

    /// One spawn try: write config 0600, open the shared log, spawn with
    /// CREATE_NO_WINDOW on Windows, pump output, publish the child, start
    /// the waiter, wait for the port, re-verify our child answered.
    async fn spawn_once(&self, command: &EngineCommand, addr: SocketAddr) -> anyhow::Result<()> {
        let tag = self.tag();
        // Fresh cycle: a manual stop from a previous kill must not leak.
        lock(&self.state).manual_stop = false;

        // 1. Config (embeds the UUID → 0600, tmp + rename).
        let cfg_path = self.config_path();
        write_config_0600(&cfg_path, &generate_config(&self.spec, addr))?;

        // 2. Log dir + shared append handle + header. The header NEVER
        //    names the config path or anything from inside it.
        std::fs::create_dir_all(&self.log_dir)?;
        let log = open_shared_log(&self.log_path())?;
        {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut g = lock(&log);
            let _ = writeln!(
                &mut *g,
                "[ts {ts}] spawn engine tag={tag} socks_port={}",
                addr.port()
            );
            let _ = g.flush();
        }

        // 3. Spawn.
        let mut cmd = Command::new(&command.program);
        cmd.args(&command.args)
            .arg(&cfg_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("engine `{tag}`: spawn failed: {e}"))?;

        // 3b. Windows: put the child in a kill-on-close job object
        //     (S6-REV-C P1-2). Assignment failures are fatal for this
        //     spawn — without the job, an abnormal death of THIS process
        //     would orphan the engine.
        #[cfg(windows)]
        let job = {
            let j = make_kill_on_close_job(&child).map_err(|e| {
                // Never leave the child running outside a job.
                let _ = child.kill();
                let _ = child.wait();
                anyhow::anyhow!("engine `{tag}`: job object setup failed: {e}")
            })?;
            Some(j)
        };

        // 4. Pumps (stdout → info, stderr → warn; both to the log file).
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("engine `{tag}`: child without stdout pipe")),
        };
        let stderr = match child.stderr.take() {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("engine `{tag}`: child without stderr pipe")),
        };
        pump_to_log(stdout, Level::INFO, tag.to_owned(), Arc::clone(&log));
        pump_to_log(stderr, Level::WARN, tag.to_owned(), log);

        // 5. Publish the child, then start the generation-tagged waiter.
        let generation = {
            let mut st = lock(&self.state);
            st.generation += 1;
            st.socks_addr = Some(addr);
            #[cfg(windows)]
            {
                st.job = job;
            }
            st.child = Some(child);
            st.generation
        };
        spawn_waiter(Arc::clone(&self.state), generation, tag.to_owned());

        // 6. Wait for the port; bail early if the child dies first.
        if let Err(e) = wait_port_up(addr, self.policy.port_wait, &self.state).await {
            // Abandon: manual kill (never "unexpected"), then reap.
            self.kill_current();
            return Err(e);
        }
        // REV-C#1 lesson: prove OUR child, not a foreign listener, took
        // the port.
        if !self.child_alive() {
            self.kill_current();
            return Err(anyhow::anyhow!(
                "engine `{tag}`: child died right after the port came up"
            ));
        }
        Ok(())
    }
}

/// Grab a free loopback port: bind 127.0.0.1:0, read it, drop the
/// listener. The TOCTOU race is handled by the spawn-cycle retry.
fn pick_free_port() -> anyhow::Result<SocketAddr> {
    let s = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let addr = s.local_addr()?;
    drop(s);
    Ok(addr)
}

// ---- Windows: kill-on-close job object (S6-REV-C P1-2) --------------------
// Ported from the v0.2 core_proc discipline (2026-11-17 audit REV-C#2): Drop
// alone cannot guarantee "the engine cannot outlive this process" —
// TerminateProcess, console Ctrl+C and power loss never run destructors. The
// kernel CAN: a job object with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE kills
// every process inside it the moment the last handle closes, and our process
// death closes all our handles unconditionally.

/// Owned job HANDLE: dropping closes it → kernel kill of the job members.
#[cfg(windows)]
struct JobObjectHandle(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: a job HANDLE is a reference to a kernel object with no thread
// affinity — it can be closed from any thread (access is serialized by the
// enclosing Mutex anyway). The raw-pointer type of HANDLE is an artifact of
// the windows-sys definition, not memory in need of thread-local access.
#[cfg(windows)]
unsafe impl Send for JobObjectHandle {}

#[cfg(windows)]
impl std::fmt::Debug for JobObjectHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobObjectHandle").finish_non_exhaustive()
    }
}

#[cfg(windows)]
impl Drop for JobObjectHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was created by make_kill_on_close_job and is
        // only ever closed here.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

/// Create an unnamed kill-on-close job and put `child` into it. The handle
/// is NOT inheritable (NULL security attributes), so the child cannot
/// accidentally keep the job alive itself.
#[cfg(windows)]
fn make_kill_on_close_job(child: &Child) -> anyhow::Result<JobObjectHandle> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // SAFETY: unnamed job object (both pointers NULL) — cannot fail on
    // arguments; only resource exhaustion can make it return NULL.
    let job = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
    if job.is_null() {
        return Err(anyhow::anyhow!(
            "CreateJobObjectW failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: `job` is a valid job handle; `info` is a properly aligned
    // struct of exactly the declared information class' size.
    let ok = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        let e = std::io::Error::last_os_error();
        // SAFETY: `job` is valid and not yet handed out.
        unsafe { CloseHandle(job) };
        return Err(anyhow::anyhow!("SetInformationJobObject failed: {e}"));
    }
    // SAFETY: `job` is a valid job handle; the child raw handle is a valid
    // process handle owned by `Child` (valid for the lifetime of the call).
    if unsafe { AssignProcessToJobObject(job, child.as_raw_handle()) } == 0 {
        let e = std::io::Error::last_os_error();
        // SAFETY: as above.
        unsafe { CloseHandle(job) };
        return Err(anyhow::anyhow!("AssignProcessToJobObject failed: {e}"));
    }
    Ok(JobObjectHandle(job))
}

/// Poll `addr` until a TCP connect succeeds or `budget` elapses; bails
/// early when our child dies (bind failure → immediate fresh-port retry).
async fn wait_port_up(
    addr: SocketAddr,
    budget: Duration,
    state: &Arc<Mutex<LiveState>>,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + budget;
    loop {
        if Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "socks port {addr} did not come up within {} ms",
                budget.as_millis()
            ));
        }
        {
            let mut st = lock(state);
            match st.child.as_mut() {
                None => {
                    return Err(anyhow::anyhow!(
                        "child exited before socks port {addr} came up"
                    ));
                }
                Some(c) => {
                    if c.try_wait().is_ok_and(|r| r.is_some()) {
                        st.child = None;
                        st.socks_addr = None;
                        return Err(anyhow::anyhow!(
                            "child exited before socks port {addr} came up"
                        ));
                    }
                }
            }
        }
        match TcpStream::connect(addr).await {
            // Probe connection only: drop it; the inbound sees an empty
            // connection and closes it.
            Ok(s) => {
                drop(s);
                return Ok(());
            }
            Err(_) => sleep(Duration::from_millis(PORT_STEP_MS)).await,
        }
    }
}

/// Waiter thread body (`core_proc` discipline + generation token): reap
/// the child when it dies, clear the slot, log unexpected exits. Exits
/// when the slot is empty, the child died, or a newer cycle owns it.
fn waiter_loop(state: Arc<Mutex<LiveState>>, generation: u64, tag: String) {
    let mut died = false;
    loop {
        let mut done = false;
        {
            let mut st = lock(&state);
            if st.generation != generation {
                // A newer cycle owns the slot; this watcher is stale.
                return;
            }
            match st.child.as_mut() {
                // Slot empty: someone else owns cleanup.
                None => done = true,
                Some(c) => match c.try_wait() {
                    Ok(Some(_)) | Err(_) => {
                        st.child = None;
                        st.socks_addr = None;
                        died = true;
                        done = true;
                    }
                    Ok(None) => {}
                },
            }
        }
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(WAITER_POLL_MS));
    }
    if died {
        let st = lock(&state);
        if st.manual_stop {
            tracing::debug!(tag, "engine child stopped manually");
        } else {
            tracing::warn!(
                tag,
                "engine child exited unexpectedly; next connect respawns"
            );
        }
    }
}

/// Detached waiter thread; failure to spawn one is degraded, not fatal
/// (liveness checks still work; only backstop reaping is lost).
fn spawn_waiter(state: Arc<Mutex<LiveState>>, generation: u64, tag: String) {
    let spawned = std::thread::Builder::new()
        .name(format!("mt-engine-waiter-{tag}"))
        .spawn(move || waiter_loop(state, generation, tag));
    if let Err(e) = spawned {
        tracing::warn!("mt-engine: waiter thread spawn failed: {e}");
    }
}

/// Pump body (synchronous, `core_proc` discipline): append every chunk to
/// the SHARED log handle and forward complete lines to tracing until pipe
/// EOF. A failed log write drops that chunk; the pump never panics.
fn pump_pipe_to_log<R: std::io::Read>(mut pipe: R, level: Level, tag: String, log: &SharedLog) {
    let mut buf = [0u8; 4096];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                {
                    let mut g = lock(log);
                    let _ = g.write_all(&buf[..n]);
                }
                carry.extend_from_slice(&buf[..n]);
                while let Some(pos) = carry.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = carry.drain(..=pos).collect();
                    forward_line(&line[..line.len() - 1], level, &tag);
                }
                if carry.len() > 8192 {
                    // Pathological no-newline stream: flush to tracing.
                    forward_line(&carry, level, &tag);
                    carry.clear();
                }
            }
        }
    }
    if !carry.is_empty() {
        forward_line(&carry, level, &tag);
    }
}

fn forward_line(bytes: &[u8], level: Level, tag: &str) {
    let line = String::from_utf8_lossy(bytes);
    match level {
        Level::INFO => tracing::info!(target: "mt_engine::child", tag, "{line}"),
        Level::WARN => tracing::warn!(target: "mt_engine::child", tag, "{line}"),
        _ => tracing::debug!(target: "mt_engine::child", tag, "{line}"),
    }
}

/// Detached pump thread for one child pipe.
fn pump_to_log<R: std::io::Read + Send + 'static>(
    pipe: R,
    level: Level,
    tag: String,
    log: SharedLog,
) {
    let spawned = std::thread::Builder::new()
        .name(format!("mt-engine-pump-{tag}"))
        .spawn(move || pump_pipe_to_log(pipe, level, tag, &log));
    if let Err(e) = spawned {
        tracing::warn!("mt-engine: pump thread spawn failed: {e}");
    }
}

/// Open the engine log in append mode, wrapped as the cycle-wide shared
/// handle.
fn open_shared_log(log_path: &Path) -> anyhow::Result<SharedLog> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| anyhow::anyhow!("cannot open engine log {}: {e}", log_path.display()))?;
    Ok(Arc::new(Mutex::new(file)))
}

/// Sanitize a tag for use in filenames: keep `[A-Za-z0-9._-]`, map
/// everything else to `-`, never empty.
fn sanitize_tag(tag: &str) -> String {
    let mut out: String = tag
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("engine");
    }
    out
}

/// S21 P0 fix (H2): the config/log file stem for an engine spec.
///
/// Historically the stem was the sanitized tag alone — and profile tags
/// are NOT unique (subscription links routinely share one `#fragment`).
/// Two engines with the same tag — e.g. the live session's engine and a
/// temporary probe/speed-test tunnel engine built from a DIFFERENT
/// profile with the same tag — shared one `engine-<tag>.json`, so the
/// temp spawn REWROTE the live engine's config; the live child's next
/// respawn then dialed the wrong server, exhausted its respawn budget
/// and went permanently DEGRADED while the GUI kept showing Connected.
///
/// The stem therefore appends a deterministic FNV-1a hash of the FULL
/// endpoint identity (address, port, uuid, sni, public key, short id,
/// fingerprint, spiderX, sendThrough): the same profile across
/// reconnects keeps reusing the same file (no proliferation), while two
/// different profiles that share a tag never collide again.
pub fn engine_file_stem(spec: &EngineSpec) -> String {
    // FNV-1a 64-bit, hand-rolled (zero-dependency rule) and fed through
    // the public lib re-export. Separate every field with \0 so
    // ("a" "bc") and ("ab" "c") cannot hash alike.
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    let feed = |bytes: &[u8], hash: &mut u64| {
        for b in bytes.iter().chain(std::iter::once(&0u8)) {
            *hash ^= u64::from(*b);
            *hash = hash.wrapping_mul(FNV_PRIME);
        }
    };
    feed(spec.endpoint.address.as_bytes(), &mut hash);
    feed(&spec.endpoint.port.to_le_bytes(), &mut hash);
    feed(spec.endpoint.uuid.as_bytes(), &mut hash);
    feed(spec.endpoint.sni.as_bytes(), &mut hash);
    feed(spec.endpoint.public_key.as_bytes(), &mut hash);
    feed(spec.endpoint.short_id.as_bytes(), &mut hash);
    feed(spec.endpoint.fingerprint.as_bytes(), &mut hash);
    feed(
        spec.endpoint.spider_x.as_deref().unwrap_or("").as_bytes(),
        &mut hash,
    );
    feed(
        spec.send_through.as_deref().unwrap_or("").as_bytes(),
        &mut hash,
    );
    format!(
        "engine-{}-{:08x}",
        sanitize_tag(&spec.tag),
        (hash >> 32) as u32
    )
}

/// Write the config pretty-printed + newline. POSIX: a fresh 0600 tmp
/// file is renamed over the destination (atomic, embeds the UUID).
/// Windows: no POSIX perms; the profile ACLs apply.
fn write_config_0600(path: &Path, cfg: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut body = serde_json::to_string_pretty(cfg)?;
    body.push('\n');
    let tmp = path.with_extension("tmp");
    {
        #[cfg(unix)]
        {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(body.as_bytes())?;
            f.flush()?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&tmp, body.as_bytes())?;
        }
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[async_trait::async_trait]
impl Outbound for ManagedEngine {
    fn name(&self) -> &str {
        self.inner.tag()
    }

    async fn connect(&self, target: &TargetAddr) -> anyhow::Result<ProxyStream> {
        let addr = self.ensure_alive().await?;
        match Socks5Outbound::new(addr).connect(target).await {
            Ok(s) => Ok(s),
            Err(e) => {
                // Tunnel failure with a live child propagates untouched; a
                // dead child gets exactly one transparent respawn+retry.
                // A ProxyUnreachable ALWAYS forces the respawn: the engine's
                // own port refused the connection, so nothing was sent
                // upstream — and the kernel can tear an engine's socket down
                // before the process is reaped (2026-11-17 CI race), which
                // makes `child_alive()` a lying witness for that window. A
                // hung engine with a dead listener is indistinguishable from
                // that case, and both want the same medicine.
                let port_dead = e
                    .downcast_ref::<mt_net::NetError>()
                    .is_some_and(|ne| matches!(ne, mt_net::NetError::ProxyUnreachable(_, _)));
                if !port_dead && self.inner.child_alive() {
                    return Err(e);
                }
                tracing::warn!(
                    tag = self.inner.tag(),
                    "engine port unreachable; forcing respawn"
                );
                self.inner.kill_and_clear();
                let addr = self.ensure_alive().await?;
                Socks5Outbound::new(addr).connect(target).await
            }
        }
    }

    async fn probe(&self, target: &TargetAddr) -> anyhow::Result<Duration> {
        let addr = self.ensure_alive().await?;
        match Socks5Outbound::new(addr).probe(target).await {
            Ok(d) => Ok(d),
            Err(e) => {
                let port_dead = e
                    .downcast_ref::<mt_net::NetError>()
                    .is_some_and(|ne| matches!(ne, mt_net::NetError::ProxyUnreachable(_, _)));
                if !port_dead && self.inner.child_alive() {
                    return Err(e);
                }
                tracing::warn!(
                    tag = self.inner.tag(),
                    "engine port unreachable (probe); forcing respawn"
                );
                self.inner.kill_and_clear();
                let addr = self.ensure_alive().await?;
                Socks5Outbound::new(addr).probe(target).await
            }
        }
    }
}

impl Drop for ManagedEngine {
    fn drop(&mut self) {
        // Manual flag FIRST: the death we cause is never "unexpected".
        // kill() is synchronous (no runtime needed); the waiter thread
        // reaps afterwards.
        self.inner.kill_current();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- pure policy math (contract: 1s→2s→…→30s, budget 5) ---

    #[test]
    fn default_policy_matches_contract() {
        let p = EnginePolicy::default();
        assert_eq!(p.max_respawns, 5);
        assert_eq!(p.backoff_base, Duration::from_secs(1));
        assert_eq!(p.backoff_cap, Duration::from_secs(30));
        assert_eq!(p.port_wait, Duration::from_secs(10));
    }

    #[test]
    fn backoff_sequence_is_doubling_with_cap() {
        let p = EnginePolicy::default();
        assert_eq!(p.delay_before_attempt(1), Duration::from_secs(1));
        assert_eq!(p.delay_before_attempt(2), Duration::from_secs(2));
        assert_eq!(p.delay_before_attempt(3), Duration::from_secs(4));
        assert_eq!(p.delay_before_attempt(4), Duration::from_secs(8));
        assert_eq!(p.delay_before_attempt(5), Duration::from_secs(16));
        // Attempt 6+ would be 32 s — capped at 30 s (and beyond budget).
        assert_eq!(p.delay_before_attempt(6), Duration::from_secs(30));
        assert_eq!(p.delay_before_attempt(10), Duration::from_secs(30));
        // Tiny test policy scales down proportionally.
        let t = EnginePolicy {
            backoff_base: Duration::from_millis(50),
            backoff_cap: Duration::from_millis(200),
            ..EnginePolicy::default()
        };
        assert_eq!(t.delay_before_attempt(1), Duration::from_millis(50));
        assert_eq!(t.delay_before_attempt(2), Duration::from_millis(100));
        assert_eq!(t.delay_before_attempt(3), Duration::from_millis(200));
    }

    // --- tag sanitization for engine-<tag> filenames ---

    #[test]
    fn sanitize_tag_maps_hostile_characters() {
        assert_eq!(sanitize_tag("eu-west"), "eu-west");
        assert_eq!(sanitize_tag("srv/1:8080"), "srv-1-8080");
        assert_eq!(sanitize_tag("a b\tc"), "a-b-c");
        assert_eq!(sanitize_tag("隧道"), "--");
        assert_eq!(sanitize_tag(""), "engine");
    }

    // --- S21: engine_file_stem — same-tag profiles must not collide ---

    use crate::VlessRealityEndpoint;

    const STEM_UUID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    const STEM_PBK: &str = "0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";

    fn stem_spec(tag: &str, address: &str, port: u16, uuid: &str) -> EngineSpec {
        EngineSpec {
            tag: tag.into(),
            endpoint: VlessRealityEndpoint {
                address: address.into(),
                port,
                uuid: uuid.into(),
                sni: "tunnel.example.org".into(),
                public_key: STEM_PBK.into(),
                short_id: "0123abcd".into(),
                fingerprint: "chrome".into(),
                spider_x: None,
            },
            send_through: None,
            log_level: None,
        }
    }

    #[test]
    fn engine_stem_is_deterministic_for_the_same_spec() {
        let a = stem_spec("node", "203.0.113.10", 443, STEM_UUID);
        let b = stem_spec("node", "203.0.113.10", 443, STEM_UUID);
        assert_eq!(engine_file_stem(&a), engine_file_stem(&b));
    }

    #[test]
    fn engine_stem_separates_same_tag_different_endpoints() {
        // The H2 regression: two profiles share the tag but differ in the
        // endpoint — the stems (and thus the config/log files) must differ.
        let a = stem_spec("node", "203.0.113.10", 443, STEM_UUID);
        let b = stem_spec("node", "203.0.113.11", 443, STEM_UUID);
        let c = stem_spec("node", "203.0.113.10", 8443, STEM_UUID);
        let d = stem_spec(
            "node",
            "203.0.113.10",
            443,
            "ffffffff-0000-4ccc-8ddd-eeeeeeeeeeee",
        );
        let sa = engine_file_stem(&a);
        assert_ne!(sa, engine_file_stem(&b));
        assert_ne!(sa, engine_file_stem(&c));
        assert_ne!(sa, engine_file_stem(&d));
    }

    #[test]
    fn engine_stem_send_through_is_part_of_the_identity() {
        let mut a = stem_spec("node", "203.0.113.10", 443, STEM_UUID);
        let b = stem_spec("node", "203.0.113.10", 443, STEM_UUID);
        a.send_through = Some("192.168.1.10".into());
        assert_ne!(engine_file_stem(&a), engine_file_stem(&b));
    }

    #[test]
    fn engine_stem_shape_is_tag_plus_eight_hex() {
        let stem = engine_file_stem(&stem_spec("eu/west 1", "203.0.113.10", 443, STEM_UUID));
        let suffix = stem.rsplit('-').next().expect("stem has a suffix");
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(stem.starts_with("engine-eu-west-1-"));
    }

    // --- free port picker never returns port 0 and is loopback ---

    #[tokio::test]
    async fn pick_free_port_is_loopback_and_reusable() {
        let addr = pick_free_port().unwrap();
        assert!(addr.ip().is_loopback());
        assert_ne!(addr.port(), 0);
        // The listener was dropped; the port must be bindable again.
        let s = std::net::TcpListener::bind(addr).unwrap();
        drop(s);
    }
}
