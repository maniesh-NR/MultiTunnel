//! `ProxyServer` — the in-process net-merge replacement.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"):
//! - Owns the SOCKS5 + HTTP listeners, the link table
//!   (`Vec<Arc<dyn Outbound>>` via the [`Aggregator`]), the [`Router`] and
//!   the [`MergePolicy`].
//! - `run(shutdown: tokio::sync::watch::Receiver<bool>)` serves until the
//!   watch flips true. Per connection: parse target → route → pick link
//!   (or direct/reject) → connect with ONE transparent retry on a DIFFERENT
//!   link if the first connect fails before any byte flows → bidirectional
//!   copy with byte counters.
//! - Emits [`ProxyEvent`]s on a `tokio::sync::broadcast::Sender`:
//!   `Started`, `Stopped`, `ConnOpened`, `ConnClosed`, `LinkHealth`,
//!   `Tick` (1 s).
//! - Fail-open at the data plane: if ALL links fail, connections error but
//!   the listeners keep serving; an accept loop never unwraps/panics.
//!
//! IMPLEMENTATION NOTES (S6-B decisions, "pick one and document it"):
//! - `run` takes `self: Arc<Self>` (instead of `self`): callers must be
//!   able to `subscribe()` to events and poll `socks_local_addr()` /
//!   `http_local_addr()` (ephemeral `127.0.0.1:0` binds!) while the server
//!   future is running. The contract's parameter list is otherwise
//!   unchanged; this is a reported signature refinement.
//! - RETRY SEMANTICS: `Outbound::connect()` returning `Err` is the only
//!   failure mode that can occur "before any byte flows" — nothing is ever
//!   relayed to the inbound client until `connect()` hands back an
//!   established tunnel — so ANY connect error qualifies for the single
//!   transparent retry. The retry re-picks with the failed link EXCLUDED
//!   (guaranteeing a different link while one exists), records a health
//!   failure for the dead link and a success for the working one. After
//!   the tunnel is established there are NO retries (bytes may have
//!   flowed). Data-plane connect outcomes also feed aggregator health;
//!   probing remains the primary source (tradeoff: a healthy proxy whose
//!   CLIENTS repeatedly pick dead destinations accrues failures too; a
//!   probe success heals it).
//! - Copy engine: two spawned half-pumps (client→upstream counts `up`,
//!   upstream→client counts `down`) over `tokio::io::split` halves with
//!   16 KiB buffers, updating per-connection AND global byte counters per
//!   chunk — so 1 s Ticks show LIVE throughput mid-transfer
//!   (`tokio::io::copy_bidirectional` would only report at connection end).
//!   If one pump ERRORS the other is aborted; if one pump ends with a clean
//!   EOF it shuts its write half down (TCP FIN) and we keep relaying the
//!   other direction — `copy_bidirectional` semantics.
//! - Graceful shutdown: the watch flipping true (or its sender being
//!   dropped) aborts the accept loops (listeners drop → ports close
//!   immediately), aborts probers + ticker, then waits up to
//!   [`DRAIN_GRACE`] (3 s) for in-flight connections before emitting
//!   `Stopped` and returning Ok. Connections still alive after the grace
//!   are left to finish on their own (fail-open; no user-data abort).
//! - Established tunnels have NO idle timeout (the contract does not ask
//!   for one); stuck handshakes are bounded by the 10 s inbound budgets.
//! - Accept errors are logged and retried after 50 ms — an accept loop
//!   never unwraps/panics (fail-open listener rule).
//! - `Tick` counts bytes per second over the preceding 1 s window
//!   (saturating subtraction; idle ticks report 0/0).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch, Notify};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::merge::{prober_loop, Aggregator};
use crate::outbound::DirectOutbound;
use crate::router::Router;
use crate::shutdown::wait_for_shutdown;
use crate::socks5::{self, rep};
use crate::{http, Action, Host, MergePolicy, Outbound, ProxyEvent, ProxyStream, TargetAddr};

/// Drain grace for in-flight connections at shutdown (see module docs).
pub const DRAIN_GRACE: Duration = Duration::from_secs(3);
/// Contract-mandated 1 s tick interval.
pub const TICK_INTERVAL: Duration = Duration::from_secs(1);
/// Default background probe interval (configurable via
/// [`ProxyServer::with_probe_interval`]).
pub const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_secs(10);
/// Default probe target: a well-known anycast endpoint; only reachability
/// through the link matters. Configurable via `with_probe_target`.
const DEFAULT_PROBE_TARGET: TargetAddr = TargetAddr {
    host: Host::Ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))),
    port: 443,
};

const EVENT_BUFFER: usize = 1024;
const PUMP_BUF: usize = 16 * 1024;
const ACCEPT_RETRY: Duration = Duration::from_millis(50);

/// Byte counters + bound addresses + liveness shared across tasks.
struct Shared {
    up_total: Arc<AtomicU64>,
    down_total: Arc<AtomicU64>,
    active: AtomicUsize,
    idle: Notify,
    socks_local: OnceLock<SocketAddr>,
    http_local: OnceLock<SocketAddr>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            up_total: Arc::new(AtomicU64::new(0)),
            down_total: Arc::new(AtomicU64::new(0)),
            active: AtomicUsize::new(0),
            idle: Notify::new(),
            socks_local: OnceLock::new(),
            http_local: OnceLock::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InboundKind {
    Socks,
    Http,
}

/// The all-in-one proxy server: SOCKS5 + HTTP inbounds, rule routing and
/// net-merge-style link spreading. Build with [`ProxyServer::new`], tune
/// with the `with_*` builders, observe via `subscribe()` /
/// `health_snapshot()`, run with [`ProxyServer::run`] on the caller's
/// runtime (this library never creates a runtime).
pub struct ProxyServer {
    socks_addr: SocketAddr,
    http_addr: SocketAddr,
    aggregator: Arc<Aggregator>,
    router: Arc<Router>,
    policy: MergePolicy,
    probe_target: TargetAddr,
    probe_interval: Duration,
    direct: Arc<dyn Outbound>,
    events: broadcast::Sender<ProxyEvent>,
    shared: Arc<Shared>,
}

impl ProxyServer {
    /// Create a server bound (at run time) to `socks_addr` + `http_addr`,
    /// spreading connections across `links` per `policy` under `router`.
    /// `links` may be empty — connections then fail open (see module docs).
    pub fn new(
        socks_addr: SocketAddr,
        http_addr: SocketAddr,
        links: Vec<Arc<dyn Outbound>>,
        router: Router,
        policy: MergePolicy,
    ) -> Self {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Self {
            socks_addr,
            http_addr,
            aggregator: Arc::new(Aggregator::new(links)),
            router: Arc::new(router),
            policy,
            probe_target: DEFAULT_PROBE_TARGET,
            probe_interval: DEFAULT_PROBE_INTERVAL,
            direct: Arc::new(DirectOutbound::new()),
            events,
            shared: Arc::new(Shared::default()),
        }
    }

    /// Override the background probe interval.
    pub fn with_probe_interval(mut self, interval: Duration) -> Self {
        self.probe_interval = interval;
        self
    }

    /// Override the background probe target.
    pub fn with_probe_target(mut self, target: TargetAddr) -> Self {
        self.probe_target = target;
        self
    }

    /// Subscribe to [`ProxyEvent`]s (call before/while running; events sent
    /// while no receiver exists are dropped, never block).
    pub fn subscribe(&self) -> broadcast::Receiver<ProxyEvent> {
        self.events.subscribe()
    }

    /// The SOCKS5 listener's bound address once `run` has bound it.
    pub fn socks_local_addr(&self) -> Option<SocketAddr> {
        self.shared.socks_local.get().copied()
    }

    /// The HTTP listener's bound address once `run` has bound it.
    pub fn http_local_addr(&self) -> Option<SocketAddr> {
        self.shared.http_local.get().copied()
    }

    /// `(name, health)` snapshot of every link (for status surfaces).
    pub async fn health_snapshot(&self) -> Vec<(String, crate::LinkHealth)> {
        self.aggregator.snapshot().await
    }

    fn emit(&self, ev: ProxyEvent) {
        // Zero subscribers is normal; sending must never fail the server.
        let _ = self.events.send(ev);
    }

    /// Bind both listeners, spawn probers + ticker, serve until `shutdown`
    /// flips true (or its sender is dropped), then close the listeners
    /// immediately and drain in-flight connections briefly. Bind failures
    /// are the only `Err` path (e.g. port already in use).
    pub async fn run(self: Arc<Self>, shutdown: watch::Receiver<bool>) -> anyhow::Result<()> {
        let socks_listener = TcpListener::bind(self.socks_addr)
            .await
            .with_context(|| format!("bind socks listener {}", self.socks_addr))?;
        let http_listener = TcpListener::bind(self.http_addr)
            .await
            .with_context(|| format!("bind http listener {}", self.http_addr))?;
        let socks_local = socks_listener.local_addr()?;
        let http_local = http_listener.local_addr()?;
        let _ = self.shared.socks_local.set(socks_local);
        let _ = self.shared.http_local.set(http_local);
        tracing::info!(
            socks = %socks_local,
            http = %http_local,
            links = self.aggregator.len(),
            "mt-net proxy server started"
        );
        self.emit(ProxyEvent::Started);

        let mut background: Vec<JoinHandle<()>> = Vec::new();
        for idx in 0..self.aggregator.len() {
            background.push(tokio::spawn(prober_loop(
                Arc::clone(&self.aggregator),
                idx,
                self.probe_target.clone(),
                self.probe_interval,
                shutdown.clone(),
                self.events.clone(),
            )));
        }
        background.push(tokio::spawn(tick_loop(
            Arc::clone(&self.shared),
            self.events.clone(),
            shutdown.clone(),
        )));

        let socks_task = tokio::spawn(accept_loop(
            Arc::clone(&self),
            socks_listener,
            InboundKind::Socks,
        ));
        let http_task = tokio::spawn(accept_loop(
            Arc::clone(&self),
            http_listener,
            InboundKind::Http,
        ));

        let mut shutdown = shutdown;
        wait_for_shutdown(&mut shutdown).await;

        // Abort accept loops first: their listeners drop, so the ports
        // close immediately (the "prompt listener shutdown" guarantee).
        socks_task.abort();
        http_task.abort();
        for t in &background {
            t.abort();
        }
        tracing::debug!(
            active = self.shared.active.load(Ordering::SeqCst),
            "shutdown: listeners closed; draining connections"
        );
        let _ = timeout(DRAIN_GRACE, self.wait_idle()).await;
        tracing::info!("mt-net proxy server stopped");
        self.emit(ProxyEvent::Stopped);
        Ok(())
    }

    async fn wait_idle(&self) {
        loop {
            if self.shared.active.load(Ordering::SeqCst) == 0 {
                return;
            }
            // Register interest BEFORE re-checking to close the race with a
            // concurrent decrement (tokio Notify registers on first poll).
            let notified = self.shared.idle.notified();
            tokio::pin!(notified);
            let _ = notified.as_mut().enable();
            if self.shared.active.load(Ordering::SeqCst) == 0 {
                return;
            }
            notified.await;
        }
    }

    /// Pick a link (or go direct) and connect. `Err` carries the SOCKS5 REP
    /// code to report (the HTTP side maps any failure to 502). Reject is
    /// handled by the callers before this point.
    async fn establish(
        &self,
        action: Action,
        target: &TargetAddr,
    ) -> Result<(ProxyStream, String), u8> {
        match action {
            Action::Reject => Err(rep::GENERAL_FAILURE), // defensive; callers reject earlier
            Action::Direct => match self.direct.connect(target).await {
                Ok(s) => Ok((s, self.direct.name().to_string())),
                Err(e) => {
                    tracing::warn!(target = %target, "direct connect failed: {e:#}");
                    Err(socks5::rep_for_error(&e))
                }
            },
            Action::Proxy => {
                let Some((idx, link)) = self.aggregator.pick_link(self.policy, None).await else {
                    tracing::warn!(
                        target = %target,
                        "no candidate link available — failing this connection (listeners keep serving)"
                    );
                    return Err(rep::GENERAL_FAILURE);
                };
                match link.connect(target).await {
                    Ok(s) => {
                        self.aggregator.record_result(idx, true, None).await;
                        Ok((s, link.name().to_string()))
                    }
                    Err(first) => {
                        // ONE transparent retry on a DIFFERENT link (see
                        // module docs for the "before any byte flows"
                        // reasoning).
                        tracing::warn!(
                            link = link.name(),
                            target = %target,
                            "connect failed ({first:#}) — one transparent retry on a different link"
                        );
                        self.aggregator.record_result(idx, false, None).await;
                        match self.aggregator.pick_link(self.policy, Some(idx)).await {
                            Some((idx2, link2)) => match link2.connect(target).await {
                                Ok(s) => {
                                    tracing::info!(
                                        link = link2.name(),
                                        target = %target,
                                        "retry after first-link failure succeeded"
                                    );
                                    self.aggregator.record_result(idx2, true, None).await;
                                    Ok((s, link2.name().to_string()))
                                }
                                Err(second) => {
                                    self.aggregator.record_result(idx2, false, None).await;
                                    tracing::warn!(
                                        link = link2.name(),
                                        target = %target,
                                        "retry also failed: {second:#}"
                                    );
                                    Err(socks5::rep_for_error(&second))
                                }
                            },
                            None => Err(rep::GENERAL_FAILURE),
                        }
                    }
                }
            }
        }
    }

    async fn handle_socks(&self, mut stream: TcpStream, peer: SocketAddr) {
        let target =
            match socks5::server_handshake(&mut stream, socks5::SERVER_HANDSHAKE_TIMEOUT).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::trace!(%peer, "socks5 handshake failed: {e}");
                    return;
                }
            };
        let action = self.router.route(&target);
        if action == Action::Reject {
            tracing::debug!(%peer, target = %target, "reject rule hit (socks5)");
            let _ = socks5::send_error_reply(&mut stream, rep::NOT_ALLOWED).await;
            return;
        }
        match self.establish(action, &target).await {
            Ok((upstream, link)) => {
                if socks5::send_success_reply(&mut stream).await.is_err() {
                    return;
                }
                self.emit(ProxyEvent::ConnOpened {
                    target: target.to_string(),
                    link: link.clone(),
                });
                self.tunnel(stream, upstream, &target, &link, Vec::new())
                    .await;
            }
            Err(rep_code) => {
                let _ = socks5::send_error_reply(&mut stream, rep_code).await;
            }
        }
    }

    async fn handle_http(&self, mut stream: TcpStream, peer: SocketAddr) {
        let parsed = match http::read_request(&mut stream).await {
            Ok(p) => p,
            Err(e) => {
                tracing::trace!(%peer, "http request failed: {e}");
                return;
            }
        };
        let action = self.router.route(&parsed.target);
        if action == Action::Reject {
            tracing::debug!(%peer, target = %parsed.target, "reject rule hit (http)");
            let _ = http::write_simple(&mut stream, http::RESP_403).await;
            return;
        }
        match self.establish(action, &parsed.target).await {
            Ok((mut upstream, link)) => {
                if parsed.is_connect {
                    if http::write_simple(&mut stream, http::RESP_OK_CONNECT)
                        .await
                        .is_err()
                    {
                        return;
                    }
                } else {
                    let Some(forward) = parsed.forward else {
                        return;
                    };
                    if upstream.write_all(&forward).await.is_err() {
                        tracing::trace!(%peer, "upstream closed while forwarding request head");
                        return;
                    }
                }
                self.emit(ProxyEvent::ConnOpened {
                    target: parsed.target.to_string(),
                    link: link.clone(),
                });
                self.tunnel(
                    stream,
                    upstream,
                    &parsed.target,
                    &link,
                    parsed.tunnel_prefix,
                )
                .await;
            }
            Err(_) => {
                let _ = http::write_simple(&mut stream, http::RESP_502).await;
            }
        }
    }

    /// Bidirectional copy with byte counters + `ConnClosed` event (see
    /// module docs for the pump semantics).
    async fn tunnel(
        &self,
        client: TcpStream,
        upstream: ProxyStream,
        target: &TargetAddr,
        link: &str,
        prefix_up: Vec<u8>,
    ) {
        let started = Instant::now();
        let up_count = Arc::new(AtomicU64::new(0));
        let down_count = Arc::new(AtomicU64::new(0));
        let (client_rd, client_wr) = tokio::io::split(client);
        let (up_rd, up_wr) = tokio::io::split(upstream);
        let pump_up = tokio::spawn(pump(
            client_rd,
            up_wr,
            prefix_up,
            Arc::clone(&up_count),
            Arc::clone(&self.shared.up_total),
        ));
        let pump_down = tokio::spawn(pump(
            up_rd,
            client_wr,
            Vec::new(),
            Arc::clone(&down_count),
            Arc::clone(&self.shared.down_total),
        ));
        let (ra, rb) = join_pumps(pump_up, pump_down).await;
        for r in [&ra, &rb] {
            if let Err(e) = r {
                tracing::trace!(target = %target, link, "tunnel half ended: {e}");
            }
        }
        let up = up_count.load(Ordering::Relaxed);
        let down = down_count.load(Ordering::Relaxed);
        let dur_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        tracing::debug!(target = %target, link, up, down, dur_ms, "connection closed");
        self.emit(ProxyEvent::ConnClosed {
            link: link.to_string(),
            up,
            down,
            dur_ms,
        });
    }
}

/// One direction of the tunnel: read → write loop with live counters.
/// A clean EOF (0 bytes) shuts down the write half (TCP FIN) and returns
/// Ok; the other direction keeps running until ITS EOF (or abort, on
/// error).
async fn pump<R, W>(
    mut rd: R,
    mut wr: W,
    prefix: Vec<u8>,
    conn: Arc<AtomicU64>,
    global: Arc<AtomicU64>,
) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    if !prefix.is_empty() {
        wr.write_all(&prefix).await?;
        bump(&conn, &global, prefix.len() as u64);
    }
    let mut buf = vec![0u8; PUMP_BUF];
    loop {
        let n = rd.read(&mut buf).await?;
        if n == 0 {
            let _ = wr.shutdown().await;
            return Ok(());
        }
        wr.write_all(&buf[..n]).await?;
        bump(&conn, &global, n as u64);
    }
}

fn bump(conn: &AtomicU64, global: &AtomicU64, n: u64) {
    conn.fetch_add(n, Ordering::Relaxed);
    global.fetch_add(n, Ordering::Relaxed);
}

/// Await both pumps: when the first one finishes with an ERROR, abort the
/// other; a clean first finish keeps the other relaying until its own EOF.
async fn join_pumps(
    a: JoinHandle<std::io::Result<()>>,
    b: JoinHandle<std::io::Result<()>>,
) -> (std::io::Result<()>, std::io::Result<()>) {
    use futures::future::Either;
    let (first, other) = match futures::future::select(a, b).await {
        Either::Left((ra, rest)) => (ra, rest),
        Either::Right((rb, rest)) => (rb, rest),
    };
    let first_failed = matches!(first, Err(_) | Ok(Err(_)));
    if first_failed {
        other.abort();
    }
    let first =
        first.unwrap_or_else(|e| Err(std::io::Error::other(format!("pump task panicked: {e}"))));
    let second = match other.await {
        Ok(r) => r,
        Err(e) => Err(std::io::Error::other(format!("pump task aborted: {e}"))),
    };
    (first, second)
}

async fn accept_loop(server: Arc<ProxyServer>, listener: TcpListener, kind: InboundKind) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let _ = stream.set_nodelay(true); // interactive proxy traffic
                server.shared.active.fetch_add(1, Ordering::SeqCst);
                let s2 = Arc::clone(&server);
                tokio::spawn(async move {
                    match kind {
                        InboundKind::Socks => s2.handle_socks(stream, peer).await,
                        InboundKind::Http => s2.handle_http(stream, peer).await,
                    }
                    if s2.shared.active.fetch_sub(1, Ordering::SeqCst) == 1 {
                        s2.shared.idle.notify_waiters();
                    }
                });
            }
            Err(e) => {
                // Never unwrap-panic a listener (fail-open): log, breathe,
                // keep accepting.
                tracing::warn!("accept failed ({e}); retrying in {ACCEPT_RETRY:?}");
                tokio::time::sleep(ACCEPT_RETRY).await;
            }
        }
    }
}

/// 1 s throughput ticker: bytes/s over the preceding window (see docs).
async fn tick_loop(
    shared: Arc<Shared>,
    events: broadcast::Sender<ProxyEvent>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(TICK_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await; // consume the immediate first tick
    let (mut last_up, mut last_down) = (0u64, 0u64);
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = wait_for_shutdown(&mut shutdown) => break,
        }
        let up = shared.up_total.load(Ordering::Relaxed);
        let down = shared.down_total.load(Ordering::Relaxed);
        let ev = ProxyEvent::Tick {
            up_bps: up.saturating_sub(last_up),
            down_bps: down.saturating_sub(last_down),
        };
        last_up = up;
        last_down = down;
        let _ = events.send(ev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{wait_for_addr, FakeOutbound};
    use crate::{Action, Host, Rule, RuleMatcher};
    use std::time::Duration;

    // ---------- helpers ----------

    struct TestServer {
        server: Arc<ProxyServer>,
        shutdown_tx: watch::Sender<bool>,
        events: broadcast::Receiver<ProxyEvent>,
        run: tokio::task::JoinHandle<anyhow::Result<()>>,
    }

    async fn start(links: Vec<Arc<dyn Outbound>>, router: Router) -> TestServer {
        start_with_probe_interval(links, router, DEFAULT_PROBE_INTERVAL).await
    }

    /// [`start`] with a custom background probe interval (tests that need
    /// repeated `LinkHealth` events inside a short observation window).
    async fn start_with_probe_interval(
        links: Vec<Arc<dyn Outbound>>,
        router: Router,
        probe_interval: Duration,
    ) -> TestServer {
        let server = Arc::new(
            ProxyServer::new(
                "127.0.0.1:0".parse().unwrap(),
                "127.0.0.1:0".parse().unwrap(),
                links,
                router,
                MergePolicy::Failover,
            )
            .with_probe_interval(probe_interval),
        );
        let events = server.subscribe();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let run = tokio::spawn(Arc::clone(&server).run(shutdown_rx));
        wait_for_addr(|| server.socks_local_addr()).await;
        wait_for_addr(|| server.http_local_addr()).await;
        TestServer {
            server,
            shutdown_tx,
            events,
            run,
        }
    }

    async fn wait_event(
        rx: &mut broadcast::Receiver<ProxyEvent>,
        dur: Duration,
        pred: impl Fn(&ProxyEvent) -> bool,
    ) -> ProxyEvent {
        let deadline = Instant::now() + dur;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "expected event not observed in time");
            match tokio::time::timeout(left, rx.recv()).await {
                Ok(Ok(ev)) if pred(&ev) => return ev,
                Ok(Ok(_)) => continue,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(e)) => panic!("event channel closed: {e}"),
                Err(_) => panic!("timed out waiting for expected event"),
            }
        }
    }

    async fn read_response_head(stream: &mut TcpStream) -> (String, Vec<u8>) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let n = stream.read(&mut chunk).await.expect("read response");
            assert!(n > 0, "peer closed before a response head arrived");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                // Bytes past the head are already consumed into `buf`; a
                // peer that pipelines head+body in one segment would lose
                // them if discarded, so hand them back to the caller.
                let head = String::from_utf8_lossy(&buf[..pos + 4]).into_owned();
                let extra = buf[pos + 4..].to_vec();
                return (head, extra);
            }
        }
    }

    // ---------- SOCKS5 inbound E2E (client↔server↔echo, contract) ----------

    #[tokio::test]
    async fn socks_link_e2e_emits_lifecycle_events() {
        let echo = crate::test_support::spawn_echo().await;
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::echo("link-a", echo))];
        // Short probe interval: the prober's FIRST (immediate) tick emits a
        // LinkHealth event that the ConnOpened/ConnClosed waits below will
        // consume along the way; waiting for a FRESH LinkHealth therefore
        // needs a follow-up probe inside the observation window.
        let mut ts =
            start_with_probe_interval(links, Router::new(Vec::new()), Duration::from_millis(250))
                .await;
        let socks_addr = ts.server.socks_local_addr().unwrap();
        let target = TargetAddr {
            host: Host::Ip(echo.ip()),
            port: echo.port(),
        };
        let cli = socks5::Socks5Outbound::new(socks_addr);
        let mut s = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
            .await
            .expect("connect in time")
            .expect("socks tunnel established");
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        s.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
        drop(s);

        let opened = wait_event(
            &mut ts.events,
            Duration::from_secs(2),
            |e| matches!(e, ProxyEvent::ConnOpened { link, .. } if link == "link-a"),
        )
        .await;
        match opened {
            ProxyEvent::ConnOpened { target: t, link } => {
                assert_eq!(t, target.to_string());
                assert_eq!(link, "link-a");
            }
            other => panic!("unexpected: {other:?}"),
        }
        let closed = wait_event(
            &mut ts.events,
            Duration::from_secs(2),
            |e| matches!(e, ProxyEvent::ConnClosed { link, .. } if link == "link-a"),
        )
        .await;
        match closed {
            ProxyEvent::ConnClosed {
                up, down, dur_ms, ..
            } => {
                assert_eq!(up, 4, "4 'ping' bytes upstream");
                assert_eq!(down, 4, "4 echoed bytes downstream");
                assert!(dur_ms <= 10_000);
            }
            other => panic!("unexpected: {other:?}"),
        }
        // The prober reports health for the link on every tick (the first
        // one may already have been consumed by the waits above, so this
        // matches the NEXT tick — the 250 ms interval guarantees one).
        wait_event(&mut ts.events, Duration::from_secs(2), |e| {
            matches!(e, ProxyEvent::LinkHealth { link, health } if link == "link-a" && health.ok)
        })
        .await;

        // Graceful shutdown: Stopped is emitted and run() returns Ok.
        ts.shutdown_tx.send(true).unwrap();
        wait_event(&mut ts.events, Duration::from_secs(2), |e| {
            matches!(e, ProxyEvent::Stopped)
        })
        .await;
        tokio::time::timeout(Duration::from_secs(2), ts.run)
            .await
            .expect("run() returns promptly after shutdown")
            .expect("join")
            .expect("run ok");
    }

    // ---------- HTTP inbound E2E (contract) ----------

    #[tokio::test]
    async fn http_connect_tunnel_e2e_echo() {
        let echo = crate::test_support::spawn_echo().await;
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::echo("link-a", echo))];
        let ts = start(links, Router::new(Vec::new())).await;
        let http_addr = ts.server.http_local_addr().unwrap();
        let mut s = TcpStream::connect(http_addr).await.unwrap();
        s.write_all(
            format!(
                "CONNECT 127.0.0.1:{} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
                echo.port()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        let (resp, _extra) = read_response_head(&mut s).await;
        assert!(resp.starts_with("HTTP/1.1 200"), "got: {resp}");
        s.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 5];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    }

    #[tokio::test]
    async fn http_absolute_form_reaches_origin_and_rewrites_head() {
        // Origin server: captures the forwarded request line, replies canned.
        let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin_addr = origin.local_addr().unwrap();
        let (line_tx, line_rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            let (mut s, _) = origin.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 512];
            loop {
                let n = s.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buf).into_owned();
            let _ = line_tx.send(head);
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .unwrap();
        });

        let links: Vec<Arc<dyn Outbound>> =
            vec![Arc::new(FakeOutbound::dial("origin-link", origin_addr))];
        let ts = start(links, Router::new(Vec::new())).await;
        let http_addr = ts.server.http_local_addr().unwrap();
        let mut s = TcpStream::connect(http_addr).await.unwrap();
        s.write_all(
            b"GET http://example.org/p?q=1 HTTP/1.1\r\nHost: example.org\r\nProxy-Authorization: Basic c3ludGhldGljOnZlY3Rvcg==\r\n\r\n",
        )
        .await
        .unwrap();
        let (resp, mut body) = read_response_head(&mut s).await;
        assert!(resp.starts_with("HTTP/1.1 200 OK"), "got: {resp}");
        // The origin writes head+body in ONE segment and closes right after
        // the reply, so the body may already sit in `body` from the head
        // read above — consume it before reading more from the socket.
        let mut chunk = [0u8; 64];
        while body.len() < 2 {
            let n = s.read(&mut chunk).await.unwrap();
            assert!(n > 0, "origin closed before the body arrived");
            body.extend_from_slice(&chunk[..n]);
        }
        assert_eq!(body.as_slice(), b"ok");

        let forwarded = line_rx.await.unwrap();
        let first_line = forwarded.lines().next().unwrap().to_string();
        assert_eq!(
            first_line, "GET /p?q=1 HTTP/1.1",
            "rewritten to origin-form"
        );
        assert!(
            !forwarded.contains("Proxy-Authorization"),
            "hop-by-hop headers must not leak upstream"
        );
    }

    #[tokio::test]
    async fn http_upstream_failure_returns_502() {
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::connect_fail("dead"))];
        let ts = start(links, Router::new(Vec::new())).await;
        let http_addr = ts.server.http_local_addr().unwrap();
        let mut s = TcpStream::connect(http_addr).await.unwrap();
        s.write_all(b"CONNECT 93.184.216.34:443 HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let (resp, _extra) = read_response_head(&mut s).await;
        assert!(resp.starts_with("HTTP/1.1 502"), "got: {resp}");
    }

    #[tokio::test]
    async fn http_oversized_head_gets_431() {
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::echo(
            "link-a",
            "127.0.0.1:9".parse().unwrap(),
        ))];
        let ts = start(links, Router::new(Vec::new())).await;
        let http_addr = ts.server.http_local_addr().unwrap();
        let mut s = TcpStream::connect(http_addr).await.unwrap();
        let garbage = vec![b'x'; 40 * 1024];
        s.write_all(&garbage).await.unwrap();
        let (resp, _extra) = read_response_head(&mut s).await;
        assert!(resp.starts_with("HTTP/1.1 431"), "got: {resp}");
    }

    #[tokio::test]
    async fn http_malformed_request_gets_400() {
        let links: Vec<Arc<dyn Outbound>> = vec![];
        let ts = start(links, Router::new(Vec::new())).await;
        let http_addr = ts.server.http_local_addr().unwrap();
        let mut s = TcpStream::connect(http_addr).await.unwrap();
        s.write_all(b"NONSENSE\r\n\r\n").await.unwrap();
        let (resp, _extra) = read_response_head(&mut s).await;
        assert!(resp.starts_with("HTTP/1.1 400"), "got: {resp}");
    }

    // ---------- routing behaviors over the wire ----------

    #[tokio::test]
    async fn reject_rule_answers_socks_02_and_http_403() {
        let router = Router::new(vec![Rule {
            matcher: RuleMatcher::Any,
            action: Action::Reject,
        }]);
        let ts = start(Vec::new(), router).await;
        let socks_addr = ts.server.socks_local_addr().unwrap();

        let cli = socks5::Socks5Outbound::new(socks_addr);
        let err = cli
            .connect(&TargetAddr {
                host: Host::Domain("blocked.example".into()),
                port: 80,
            })
            .await
            .err()
            .expect("reject must fail the connect");
        let ne = err.downcast_ref::<crate::NetError>().unwrap();
        assert!(
            matches!(ne, crate::NetError::SocksStatus(0x02)),
            "expected REP 02, got: {ne}"
        );

        let http_addr = ts.server.http_local_addr().unwrap();
        let mut s = TcpStream::connect(http_addr).await.unwrap();
        s.write_all(b"CONNECT 93.184.216.34:443 HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let (resp, _extra) = read_response_head(&mut s).await;
        assert!(resp.starts_with("HTTP/1.1 403"), "got: {resp}");
    }

    #[tokio::test]
    async fn direct_action_works_even_with_only_dead_links() {
        let echo = crate::test_support::spawn_echo().await;
        let router = Router::new(vec![Rule {
            matcher: RuleMatcher::IpCidr {
                cidr: "127.0.0.0/8".into(),
            },
            action: Action::Direct,
        }]);
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::always_fail("dead"))];
        let mut ts = start(links, router).await;
        let socks_addr = ts.server.socks_local_addr().unwrap();
        let cli = socks5::Socks5Outbound::new(socks_addr);
        let mut s = tokio::time::timeout(
            Duration::from_secs(3),
            cli.connect(&TargetAddr {
                host: Host::Ip(echo.ip()),
                port: echo.port(),
            }),
        )
        .await
        .expect("connect in time")
        .expect("direct route must bypass dead links");
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        s.write_all(b"direct").await.unwrap();
        let mut buf = [0u8; 6];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"direct");
        let opened = wait_event(
            &mut ts.events,
            Duration::from_secs(2),
            |e| matches!(e, ProxyEvent::ConnOpened { link, .. } if link == "direct"),
        )
        .await;
        assert!(matches!(opened, ProxyEvent::ConnOpened { .. }));
    }

    // ---------- aggregator behaviors over the wire ----------

    #[tokio::test]
    async fn retry_on_fresh_connect_lands_on_a_different_link() {
        let echo = crate::test_support::spawn_echo().await;
        let links: Vec<Arc<dyn Outbound>> = vec![
            Arc::new(FakeOutbound::connect_fail("link-a")),
            Arc::new(FakeOutbound::echo("link-b", echo)),
        ];
        let mut ts = start(links, Router::new(Vec::new())).await;
        let socks_addr = ts.server.socks_local_addr().unwrap();
        let cli = socks5::Socks5Outbound::new(socks_addr);
        let target = TargetAddr {
            host: Host::Ip(echo.ip()),
            port: echo.port(),
        };
        let mut s = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
            .await
            .expect("connect in time")
            .expect("retry must rescue the connection");
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        s.write_all(b"rr").await.unwrap();
        let mut buf = [0u8; 2];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"rr");
        // ConnOpened must name the SECOND link — proof of the retry.
        wait_event(
            &mut ts.events,
            Duration::from_secs(2),
            |e| matches!(e, ProxyEvent::ConnOpened { link, .. } if link == "link-b"),
        )
        .await;
    }

    #[tokio::test]
    async fn all_links_down_connections_error_but_listeners_keep_serving() {
        let links: Vec<Arc<dyn Outbound>> = vec![
            Arc::new(FakeOutbound::connect_fail("dead-a")),
            Arc::new(FakeOutbound::connect_fail("dead-b")),
        ];
        let ts = start(links, Router::new(Vec::new())).await;
        let socks_addr = ts.server.socks_local_addr().unwrap();
        let target = TargetAddr {
            host: Host::Domain("synthetic.test".into()),
            port: 80,
        };
        let cli = socks5::Socks5Outbound::new(socks_addr);
        let e1 = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
            .await
            .expect("attempt 1 in time")
            .err()
            .expect("all links down → error");
        let ne1 = e1.downcast_ref::<crate::NetError>().unwrap();
        assert!(
            matches!(ne1, crate::NetError::SocksStatus(_)),
            "server must ANSWER with an error reply, not just hang up: {ne1}"
        );
        // Second attempt: still a proper error reply → listener is alive.
        let e2 = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
            .await
            .expect("attempt 2 in time")
            .err()
            .expect("still all down");
        assert!(matches!(
            e2.downcast_ref::<crate::NetError>().unwrap(),
            crate::NetError::SocksStatus(_)
        ));
    }

    // ---------- events, tick, shutdown ----------

    #[tokio::test]
    async fn tick_reports_live_throughput() {
        let echo = crate::test_support::spawn_echo().await;
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::echo("link-a", echo))];
        let mut ts = start(links, Router::new(Vec::new())).await;
        let socks_addr = ts.server.socks_local_addr().unwrap();
        let cli = socks5::Socks5Outbound::new(socks_addr);
        let mut s = cli
            .connect(&TargetAddr {
                host: Host::Ip(echo.ip()),
                port: echo.port(),
            })
            .await
            .unwrap();
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let payload = vec![7u8; 2048];
        let mut echoed = 0usize;
        s.write_all(&payload).await.unwrap();
        let mut buf = vec![0u8; 2048];
        while echoed < payload.len() {
            let n = s.read(&mut buf).await.unwrap();
            assert!(n > 0);
            echoed += n;
        }
        // Within the next ~3 ticks a Tick with nonzero up_bps must appear.
        let deadline = Duration::from_secs(4);
        let mut saw_nonzero = false;
        let start_at = Instant::now();
        while start_at.elapsed() < deadline {
            let left = deadline.saturating_sub(start_at.elapsed());
            if let Ok(Ok(ev)) = tokio::time::timeout(left, ts.events.recv()).await {
                if let ProxyEvent::Tick { up_bps, down_bps } = ev {
                    if up_bps > 0 && down_bps > 0 {
                        saw_nonzero = true;
                        break;
                    }
                }
            } else {
                break;
            }
        }
        assert!(saw_nonzero, "no Tick carried the transferred bytes");
    }

    #[tokio::test]
    async fn graceful_shutdown_closes_listeners_promptly() {
        let links: Vec<Arc<dyn Outbound>> = vec![];
        let ts = start(links, Router::new(Vec::new())).await;
        let socks_addr = ts.server.socks_local_addr().unwrap();
        let http_addr = ts.server.http_local_addr().unwrap();
        // Both listeners are alive pre-shutdown.
        assert!(TcpStream::connect(socks_addr).await.is_ok());
        assert!(TcpStream::connect(http_addr).await.is_ok());

        ts.shutdown_tx.send(true).unwrap();
        // run() must return promptly (no active conns → no drain wait).
        tokio::time::timeout(Duration::from_secs(2), ts.run)
            .await
            .expect("run() must return promptly after shutdown")
            .expect("join")
            .expect("run ok");
        // Listeners are closed: fresh connects to both ports are refused.
        assert!(
            TcpStream::connect(socks_addr).await.is_err(),
            "socks listener must be closed after shutdown"
        );
        assert!(
            TcpStream::connect(http_addr).await.is_err(),
            "http listener must be closed after shutdown"
        );
    }
}
