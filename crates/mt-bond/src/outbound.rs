//! Client side of stream bonding: [`BondOutbound`] wraps N engine links
//! into ONE `mt_net::Outbound` whose streams are split across carriers.
//!
//! Drop-in for the RealService data plane: `ProxyServer` keeps spreading
//! CONNECTIONS across links in `per-connection` mode; in `stream` mode it
//! sees a single `BondOutbound` link and every connection's BYTES are
//! striped across all carriers (D17), merged by the daemon on the server.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;

use mt_net::{Outbound, ProxyStream, TargetAddr};

use crate::endpoint::{Endpoint, Inbound, Session};
use crate::frame::CloseReason;

/// Budget for the initial carrier establishment (covers engine spawn and,
/// on first run, the engine download — mirrors CONNECT_PROBE_BUDGET).
pub const CARRIER_ESTABLISH_BUDGET: Duration = Duration::from_secs(15);

/// Outcome of one carrier establishment pass (D20): the live count PLUS a
/// human line per link ("live" or the dial failure) so the service can
/// surface dead carriers in the GUI log instead of only `tracing` (the
/// v0.5.0 "both adapters ticked, traffic only on Ethernet" report was
/// undiagnosable from the GUI: the dead carrier was invisible).
#[derive(Debug, Clone)]
pub struct CarrierReport {
    /// Live carriers after this pass.
    pub live: usize,
    /// One human line per link, in link order: `"<name>: live"` or
    /// `"<name>: down (<error>)"`.
    pub lines: Vec<String>,
}

/// The client bond: carriers over the given links, lazily established.
pub struct BondOutbound {
    endpoint: Arc<Endpoint>,
    links: Vec<Arc<dyn Outbound>>,
    merge_target: TargetAddr,
    /// Serializes (re)establishment; see [`Self::ensure_carriers`].
    establish_lock: AsyncMutex<()>,
    established: AtomicBool,
    sweeper_spawned: AtomicBool,
}

impl BondOutbound {
    /// New bond over `links` (one per bonded adapter). `merge_target` is
    /// the daemon address AS SEEN BY THE SERVER (e.g. 127.0.0.1:7443) —
    /// carriers dial it THROUGH each link's tunnel.
    pub fn new(links: Vec<Arc<dyn Outbound>>, merge_target: TargetAddr) -> Arc<Self> {
        // Bond id: time+pid hash — unique across reconnects (the daemon
        // groups carriers by it; a reconnect must form a NEW bond).
        let bond_id = {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let pid = std::process::id() as u64;
            let h = nanos
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(pid.rotate_left(32));
            (h >> 32) as u32 ^ (h as u32)
        };
        let endpoint = Endpoint::client(bond_id, links.clone(), merge_target.clone());
        Arc::new(Self {
            endpoint,
            links,
            merge_target,
            establish_lock: AsyncMutex::new(()),
            established: AtomicBool::new(false),
            sweeper_spawned: AtomicBool::new(false),
        })
    }

    /// (Re)establish carriers until at least one is live. Idempotent;
    /// concurrent callers serialize (the second re-checks and returns).
    /// Every link keeps its slot index (carrier idx == link idx) so the
    /// respawn path maps a dead slot back to its own link.
    pub async fn ensure_carriers(&self, budget: Duration) -> anyhow::Result<CarrierReport> {
        if self.established.load(Ordering::Relaxed) && self.endpoint.live_carriers() > 0 {
            return Ok(CarrierReport {
                live: self.endpoint.live_carriers(),
                lines: self.link_names_live_lines(),
            });
        }
        let _guard = self.establish_lock.lock().await;
        // Re-check after acquiring (another caller may have established).
        if self.endpoint.live_carriers() > 0 {
            self.established.store(true, Ordering::Relaxed);
            return Ok(CarrierReport {
                live: self.endpoint.live_carriers(),
                lines: self.link_names_live_lines(),
            });
        }
        if self.links.is_empty() {
            anyhow::bail!("bond has no links to build carriers from");
        }
        // Dial every link in parallel (engine spawns overlap). Each task
        // reports (link idx, link name, result) — JoinSet joins in
        // COMPLETION order, not spawn order.
        let mut dialer = tokio::task::JoinSet::new();
        for (idx, link) in self.links.iter().enumerate() {
            let link = Arc::clone(link);
            let name = link.name().to_string();
            let target = self.merge_target.clone();
            dialer.spawn(async move { (idx, name, link.connect(&target).await) });
        }
        let dial = async {
            let mut results: Vec<Option<(String, anyhow::Result<ProxyStream>)>> =
                (0..self.links.len()).map(|_| None).collect();
            while let Some(joined) = dialer.join_next().await {
                match joined {
                    Ok((idx, name, res)) => {
                        if let Some(slot) = results.get_mut(idx) {
                            *slot = Some((name, res));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("bond dial task panicked: {e}");
                    }
                }
            }
            results
        };
        let results = tokio::time::timeout(budget, dial)
            .await
            .map_err(|_| anyhow::anyhow!("carrier establishment timed out"))?;
        // S11 honest carriers: a successful DIAL is not enough. Xray's
        // SOCKS inbound replies "connected" BEFORE the upstream tunnel is
        // established, so with the merge daemon absent (or the server's
        // Xray freedom policy blackholing the merge port) every carrier
        // attaches phantom-live and bytes vanish. Demand a verify-pong
        // (PING right after HELLO, answered by the daemon) per carrier.
        let mut attached: Vec<(usize, String)> = Vec::new();
        let mut live = 0usize;
        let mut lines = Vec::with_capacity(results.len());
        for (idx, result) in results.into_iter().enumerate() {
            match result {
                Some((name, Ok(stream))) => {
                    if self
                        .endpoint
                        .attach_carrier_at(stream, name.clone(), idx)
                        .await
                        .is_ok()
                    {
                        attached.push((idx, name));
                    } else {
                        lines.push(format!("{name}: down (carrier handshake failed)"));
                    }
                }
                Some((name, Err(e))) => {
                    // A dead adapter must not kill the bond — log and let
                    // the respawn path keep trying that slot.
                    tracing::warn!(carrier = %name, error = %e, "bond carrier dial failed");
                    lines.push(format!("{name}: down ({e})"));
                }
                None => {
                    tracing::warn!(carrier = idx, "bond carrier dial produced no result");
                    lines.push(format!(
                        "carrier slot {idx}: down (dial produced no result)"
                    ));
                }
            }
        }
        let slots: Vec<usize> = attached.iter().map(|(i, _)| *i).collect();
        let verified = self
            .endpoint
            .await_verified(&slots, crate::endpoint::VERIFY_PONG_TIMEOUT)
            .await;
        for (idx, name) in &attached {
            if verified.contains(idx) {
                live += 1;
                lines.push(format!("{name}: live"));
            } else {
                self.endpoint.abandon_unverified(*idx);
                lines.push(format!(
                    "{name}: down (no pong from the merge daemon — is mt-merge-server \
                     running on the tunnel server, and does its Xray allow the merge \
                     port? re-run server/setup-xray-reality.sh to patch finalRules)"
                ));
            }
        }
        if live == 0 {
            anyhow::bail!(
                "no bond carrier could be established ({} links tried)",
                self.links.len()
            );
        }
        self.established.store(true, Ordering::Relaxed);
        if !self.sweeper_spawned.swap(true, Ordering::Relaxed) {
            tokio::spawn(Arc::clone(&self.endpoint).sweeper_task());
        }
        Ok(CarrierReport { live, lines })
    }

    /// Per-link live/down lines from the CURRENT carrier table (used when
    /// `ensure_carriers` short-circuits on an established bond).
    fn link_names_live_lines(&self) -> Vec<String> {
        let snap = self.endpoint.carrier_snapshot();
        self.links
            .iter()
            .enumerate()
            .map(|(idx, link)| match snap.iter().find(|c| c.idx == idx) {
                Some(c) if c.alive => format!("{}: live", link.name()),
                Some(_) => format!("{}: down", link.name()),
                None => format!("{}: down (never established)", link.name()),
            })
            .collect()
    }

    /// Per-carrier status rows (Status screen).
    pub fn carrier_snapshot(&self) -> Vec<crate::endpoint::CarrierSnapshot> {
        self.endpoint.carrier_snapshot()
    }

    /// Open sessions right now.
    pub fn session_count(&self) -> usize {
        self.endpoint.session_count()
    }

    /// Tear the bond down (drops carriers; engines die with the links).
    pub fn shutdown(&self) {
        let _ = self.endpoint.shutdown_handle().send(true);
    }

    async fn connect_stream(
        &self,
        target: &TargetAddr,
        budget: Duration,
    ) -> anyhow::Result<BondStream> {
        self.ensure_carriers(budget).await?;
        let (session, rx) = self.endpoint.open_local_session();
        self.endpoint.open_remote(&session, target).await?;
        Ok(BondStream {
            endpoint: Arc::clone(&self.endpoint),
            session,
            rx,
            chunk: None,
            eof: None,
            pending_write: None,
            wrote_shutdown: false,
        })
    }
}

#[async_trait::async_trait]
impl Outbound for BondOutbound {
    fn name(&self) -> &str {
        "stream-bond"
    }

    async fn connect(&self, target: &TargetAddr) -> anyhow::Result<ProxyStream> {
        let stream = self
            .connect_stream(target, CARRIER_ESTABLISH_BUDGET)
            .await?;
        Ok(Box::new(stream))
    }

    /// A full bond round trip: carrier establishment (if needed) + OPEN +
    /// daemon dial, then an immediate clean close.
    async fn probe(&self, target: &TargetAddr) -> anyhow::Result<Duration> {
        let started = std::time::Instant::now();
        let mut stream = self
            .connect_stream(target, CARRIER_ESTABLISH_BUDGET)
            .await?;
        use tokio::io::AsyncWriteExt as _;
        let _ = stream.shutdown().await;
        Ok(started.elapsed())
    }
}

// ---------------------------------------------------------------------------
// BondStream: the app-facing virtual stream
// ---------------------------------------------------------------------------

/// One bonded session as seen by the app: reads reorder inbound carriers,
/// writes are striped across carriers by the endpoint.
pub struct BondStream {
    endpoint: Arc<Endpoint>,
    session: Arc<Session>,
    rx: mpsc::UnboundedReceiver<Inbound>,
    /// Current undelivered inbound chunk (offset into it).
    chunk: Option<(Vec<u8>, usize)>,
    /// Terminal condition once seen.
    eof: Option<CloseReason>,
    /// In-flight write (a `send_data` future; see poll_write).
    pending_write: Option<Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>>,
    wrote_shutdown: bool,
}

impl BondStream {
    fn eof_error(reason: CloseReason) -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            format!("bond session: {}", reason.as_str()),
        )
    }
}

impl tokio::io::AsyncRead for BondStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            // 1. Drain the current chunk first.
            if let Some((chunk, off)) = self.chunk.as_mut() {
                let remaining = chunk.len() - *off;
                let n = remaining.min(buf.remaining());
                buf.put_slice(&chunk[*off..*off + n]);
                *off += n;
                if *off >= chunk.len() {
                    self.chunk = None;
                }
                self.session.account_read(n);
                return Poll::Ready(Ok(()));
            }
            // 2. Terminal conditions.
            if let Some(reason) = self.eof {
                if reason == CloseReason::Eof {
                    return Poll::Ready(Ok(())); // clean EOF = 0 bytes
                }
                return Poll::Ready(Err(Self::eof_error(reason)));
            }
            // 3. Pull the next inbound message.
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(Inbound::Data(bytes))) => {
                    self.chunk = Some((bytes, 0));
                }
                Poll::Ready(Some(Inbound::Opened(_))) => continue, // client never opens
                Poll::Ready(Some(Inbound::Eof(reason))) => {
                    self.eof = Some(reason);
                }
                Poll::Ready(None) => {
                    self.eof = Some(CloseReason::Eof);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl tokio::io::AsyncWrite for BondStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.wrote_shutdown {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "bond session already shut down",
            )));
        }
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        // Finish any in-flight send first (the AsyncWrite contract
        // serializes poll_write per stream, so at most one is parked).
        if let Some(fut) = self.pending_write.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Ok(())) => {
                    self.pending_write = None;
                }
                Poll::Ready(Err(e)) => {
                    self.pending_write = None;
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        format!("bond send failed: {e}"),
                    )));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        // One frame per write slice keeps chunking symmetric with the
        // daemon side (bigger writes split naturally across calls).
        let n = buf.len().min(crate::frame::MAX_PAYLOAD);
        let endpoint = Arc::clone(&self.endpoint);
        let session = Arc::clone(&self.session);
        let bytes = buf[..n].to_vec();
        let fut = async move { endpoint.send_data(&session, &bytes).await };
        let mut fut = Box::pin(fut);
        match fut.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(n)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                format!("bond send failed: {e}"),
            ))),
            Poll::Pending => {
                self.pending_write = Some(fut);
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if !self.wrote_shutdown {
            self.wrote_shutdown = true;
            // HALF-close: the write side ends; reads keep flowing until the
            // daemon's final CLOSE (e.g. an HTTP response tail). The CLOSE
            // frame waits for our forward data to be acked first (no
            // overtaking) — that happens inside the spawned task.
            self.endpoint.half_close_session(&self.session);
        }
        Poll::Ready(Ok(()))
    }
}

impl Drop for BondStream {
    fn drop(&mut self) {
        if !self.wrote_shutdown {
            // Best-effort teardown when the app drops without shutdown().
            self.endpoint.half_close_session(&self.session);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A link that always refuses (drives the fallback paths).
    struct DeadLink;

    #[async_trait::async_trait]
    impl Outbound for DeadLink {
        fn name(&self) -> &str {
            "dead"
        }
        async fn connect(&self, _t: &TargetAddr) -> anyhow::Result<ProxyStream> {
            anyhow::bail!("dead link")
        }
        async fn probe(&self, _t: &TargetAddr) -> anyhow::Result<Duration> {
            anyhow::bail!("dead link")
        }
    }

    fn merge_target() -> TargetAddr {
        TargetAddr {
            host: mt_net::Host::Ip("127.0.0.1".parse().unwrap()),
            port: 7443,
        }
    }

    #[tokio::test]
    async fn ensure_carriers_fails_cleanly_with_no_links() {
        let bond = BondOutbound::new(Vec::new(), merge_target());
        let err = bond
            .ensure_carriers(Duration::from_secs(1))
            .await
            .expect_err("no links must fail");
        assert!(err.to_string().contains("no links"));
    }

    #[tokio::test]
    async fn ensure_carriers_reports_all_dead_links() {
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(DeadLink), Arc::new(DeadLink)];
        let bond = BondOutbound::new(links, merge_target());
        let err = bond
            .ensure_carriers(Duration::from_secs(2))
            .await
            .expect_err("all dead must fail");
        assert!(err.to_string().contains("no bond carrier"));
    }
}
