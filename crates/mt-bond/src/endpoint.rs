//! The bond endpoint: carriers, sessions, scheduling, reliability.
//!
//! ONE implementation serves both ends (D17): the client wraps it in
//! [`crate::outbound::BondOutbound`] (sessions opened by the app), the
//! daemon wraps it in [`crate::daemon`] (sessions opened by OPEN frames
//! and dialed toward real targets). Everything symmetric — DATA, ACK,
//! CLOSE, PING/PONG — is handled here; only OPEN processing differs
//! (`is_server`).
//!
//! Sequencing rules (see `frame.rs` docs):
//! - A session's frames are numbered per DIRECTION starting at seq 0.
//!   Client→server: seq 0 is OPEN (target address payload), DATA follows
//!   from seq 1. Server→client: DATA from seq 0.
//! - CLOSE is fire-and-forget (not sequenced): the stall timeout is the
//!   backstop when a CLOSE is lost with a dying carrier.
//! - ACK is cumulative per session: "received in order through seq".
//!
//! Reliability model:
//! - The sender keeps sent-but-unacked frames (encoded bytes + the carrier
//!   they were queued on). A carrier death replays exactly the frames
//!   tagged to the dead carrier on live carriers; the receiver's
//!   [`ReorderBuffer`] drops the duplicates.
//! - Flow control is TCP-native: writer channels are bounded, so a
//!   congested carrier applies backpressure to `send_data` awaits, which
//!   propagates to the app socket. Inbound delivery uses an UNBOUNDED
//!   channel with a byte cap ([`APP_BACKLOG_CAP`]) — a session whose app
//!   stops draining is closed (`CloseReason::Error`) rather than
//!   head-of-line blocking the carrier readers of other sessions.
//!
//! Lock discipline: `std` locks only, NEVER held across an await
//! (poison-tolerant unwraps per the repo rule).

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWriteExt as _;
use tokio::sync::{mpsc, watch};

use mt_net::{Outbound, ProxyStream};

use crate::frame::{self, CloseReason, Frame, Kind};
use crate::reorder::{ReorderBuffer, ReorderError};

/// Per-session reorder byte cap.
pub const REORDER_CAP: usize = 4 * 1024 * 1024;
/// Undrained inbound backlog tolerated before a session is failed.
pub const APP_BACKLOG_CAP: usize = 16 * 1024 * 1024;
/// No in-order delivery progress for this long (with a gap open) → stall.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Carrier keepalive probe cadence.
pub const PING_INTERVAL: Duration = Duration::from_secs(5);
/// No frame received from a carrier for this long → dead (half-open).
pub const PONG_TIMEOUT: Duration = Duration::from_secs(20);
/// Frames queued per carrier writer (16 KiB payloads → ~4 MiB in flight).
pub const WRITER_QUEUE: usize = 256;
/// Sweeper cadence (pings, liveness, stall + backlog checks).
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(2);
/// Daemon: dial budget for one target.
pub const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
/// Server: maximum concurrent sessions per bond (resource bound).
pub const MAX_SESSIONS_PER_BOND: usize = 4096;
/// Server: a session whose OPEN never completes in order within this long
/// is a zombie (e.g. late DATA for an already-closed id) — closed.
pub const PENDING_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a closing side waits for its unacked frames to be acked
/// before sending CLOSE anyway (overtaking protection, D17 §3).
pub const CLOSE_FLUSH_TIMEOUT: Duration = Duration::from_secs(3);
/// Client: carrier respawn attempts after a carrier dies.
pub const CARRIER_RESPAWN_ATTEMPTS: u32 = 3;
/// Client: backoff between respawn attempts.
pub const CARRIER_RESPAWN_BACKOFF: Duration = Duration::from_secs(2);

/// Inbound message delivered to a session consumer (app side or daemon pump).
#[derive(Debug)]
pub enum Inbound {
    /// The session's OPEN frame (server side, always the first in-order
    /// delivery, seq 0).
    Opened(mt_net::TargetAddr),
    /// In-order payload bytes.
    Data(Vec<u8>),
    /// The session ended (peer CLOSE, stall, backlog or teardown).
    Eof(CloseReason),
}

/// Live stats of one carrier (client status rows + diagnostics).
#[derive(Debug, Clone)]
pub struct CarrierSnapshot {
    pub idx: usize,
    pub name: String,
    pub alive: bool,
    /// Last PING round trip (wire bytes incl. framing).
    pub rtt_ms: Option<u32>,
    pub bytes_up: u64,
    pub bytes_down: u64,
}

/// Poison-tolerant std Mutex lock (repo rule).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Wall-clock milliseconds (ping timestamps; sender-side comparison only).
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Carrier
// ---------------------------------------------------------------------------

/// Shared stats + writer handle of one carrier connection.
struct Carrier {
    idx: usize,
    name: String,
    tx: mpsc::Sender<Vec<u8>>,
    alive: AtomicBool,
    rtt_ms: AtomicU32,
    /// Wire bytes written (header + payload) — status accounting.
    bytes_up: AtomicU64,
    bytes_down: AtomicU64,
    /// Millis of the last frame received on this carrier (liveness).
    last_rx_ms: AtomicU64,
}

impl Carrier {
    fn new(idx: usize, name: String, tx: mpsc::Sender<Vec<u8>>) -> Arc<Self> {
        Arc::new(Self {
            idx,
            name,
            tx,
            alive: AtomicBool::new(true),
            rtt_ms: AtomicU32::new(0),
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
            last_rx_ms: AtomicU64::new(now_millis()),
        })
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Session (one end's half of a multiplexed stream)
// ---------------------------------------------------------------------------

/// One session as seen by this endpoint.
pub(crate) struct Session {
    pub id: u32,
    /// Outbound direction (my DATA): next seq to assign.
    next_seq: AtomicU64,
    /// Sent-but-unacked frames: seq → (carrier idx, encoded bytes).
    unacked: Mutex<BTreeMap<u64, (usize, Vec<u8>)>>,
    /// Inbound direction (their DATA): reorder + delivery.
    reorder: Mutex<ReorderBuffer>,
    /// Last in-order delivery (stall detection).
    last_progress: Mutex<Instant>,
    /// Delivery channel (unbounded; backlog-capped by the sweeper). The
    /// RECEIVER lives in the session until its consumer takes it: the
    /// client's `BondStream` gets it at `open_local_session`, the daemon
    /// pump takes it when the new-session notifier fires.
    inbound_tx: mpsc::UnboundedSender<Inbound>,
    inbound_rx: Mutex<Option<mpsc::UnboundedReceiver<Inbound>>>,
    /// Bytes queued for the app that the app has not read yet.
    backlog: AtomicUsize,
    /// Server-side: session creation time (pending-open zombie sweep).
    created: Mutex<Option<Instant>>,
    /// True once the inbound OPEN (server) has been delivered.
    opened_delivered: AtomicBool,
    /// The local app half-closed its write side (client: no more DATA
    /// will be sent; the read side keeps flowing until the peer's CLOSE).
    pub write_closed: AtomicBool,
    closed: AtomicBool,
}

impl Session {
    fn new(id: u32) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            id,
            next_seq: AtomicU64::new(0),
            unacked: Mutex::new(BTreeMap::new()),
            reorder: Mutex::new(ReorderBuffer::new(REORDER_CAP)),
            last_progress: Mutex::new(Instant::now()),
            inbound_tx: tx,
            inbound_rx: Mutex::new(Some(rx)),
            backlog: AtomicUsize::new(0),
            created: Mutex::new(None),
            opened_delivered: AtomicBool::new(false),
            write_closed: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// The app drained `n` backlog bytes (backlog-cap accounting).
    pub fn account_read(&self, n: usize) {
        self.backlog.fetch_sub(n, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Endpoint
// ---------------------------------------------------------------------------

/// The bond endpoint shared by client and daemon.
pub(crate) struct Endpoint {
    pub bond_id: u32,
    pub is_server: bool,
    carriers: RwLock<Vec<Arc<Carrier>>>,
    rr: AtomicU64,
    sessions: Mutex<HashMap<u32, Arc<Session>>>,
    /// Monotonic LOCAL session id (client side; starts at 1).
    next_session_id: AtomicU32,
    /// Server mode: notifies the daemon pump of newly announced sessions.
    new_session_tx: Mutex<Option<mpsc::UnboundedSender<u32>>>,
    shutdown: watch::Sender<bool>,
    /// Carrier-death reports (writer/reader tasks never await
    /// `carrier_died` directly — that would be an opaque-type cycle;
    /// the sweeper consumes these).
    death_tx: mpsc::UnboundedSender<(usize, &'static str)>,
    death_rx: Mutex<Option<mpsc::UnboundedReceiver<(usize, &'static str)>>>,
    /// Client only: the links carriers are dialed through + merge target.
    links: Vec<Arc<dyn Outbound>>,
    merge_target: Option<mt_net::TargetAddr>,
    next_ping_seq: AtomicU64,
}

impl Endpoint {
    /// Client-side endpoint (carriers dialed through `links`).
    pub fn client(
        bond_id: u32,
        links: Vec<Arc<dyn Outbound>>,
        merge_target: mt_net::TargetAddr,
    ) -> Arc<Self> {
        let (shutdown, _) = watch::channel(false);
        let (death_tx, death_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            bond_id,
            is_server: false,
            carriers: RwLock::new(Vec::new()),
            rr: AtomicU64::new(0),
            sessions: Mutex::new(HashMap::new()),
            next_session_id: AtomicU32::new(1),
            new_session_tx: Mutex::new(None),
            shutdown,
            death_tx,
            death_rx: Mutex::new(Some(death_rx)),
            links,
            merge_target: Some(merge_target),
            next_ping_seq: AtomicU64::new(0),
        })
    }

    /// Server-side endpoint (carriers are accepted connections).
    pub fn server(bond_id: u32) -> Arc<Self> {
        let (shutdown, _) = watch::channel(false);
        let (death_tx, death_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            bond_id,
            is_server: true,
            carriers: RwLock::new(Vec::new()),
            rr: AtomicU64::new(0),
            sessions: Mutex::new(HashMap::new()),
            next_session_id: AtomicU32::new(1),
            new_session_tx: Mutex::new(None),
            shutdown,
            death_tx,
            death_rx: Mutex::new(Some(death_rx)),
            links: Vec::new(),
            merge_target: None,
            next_ping_seq: AtomicU64::new(0),
        })
    }

    /// Server mode: install the new-session notifier (daemon pump).
    pub fn set_session_notifier(&self, tx: mpsc::UnboundedSender<u32>) {
        *lock(&self.new_session_tx) = Some(tx);
    }

    pub fn shutdown_handle(&self) -> watch::Sender<bool> {
        self.shutdown.clone()
    }

    pub fn is_shutting_down(&self) -> bool {
        *self.shutdown.borrow()
    }

    /// Register an established carrier connection: spawns writer + reader
    /// tasks; the writer sends the HELLO frame first. `replace_idx` =
    /// Some(i) reuses slot i (client respawn — name/idx continuity);
    /// None appends (daemon accept).
    pub(crate) async fn attach_carrier(
        self: &Arc<Self>,
        stream: ProxyStream,
        name: String,
        replace_idx: Option<usize>,
    ) -> anyhow::Result<usize> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(WRITER_QUEUE);
        let carrier;
        let idx;
        {
            let mut cs = self.carriers.write().expect("carrier lock");
            match replace_idx {
                Some(i) => {
                    if i >= cs.len() {
                        anyhow::bail!("carrier replace index {i} out of range (len {})", cs.len());
                    }
                    idx = i;
                    carrier = Carrier::new(i, name, tx);
                    cs[i] = carrier.clone();
                }
                None => {
                    idx = cs.len();
                    carrier = Carrier::new(idx, name, tx);
                    cs.push(carrier.clone());
                }
            }
        }
        let hello = Frame::session(
            Kind::Hello,
            self.bond_id,
            frame::CONTROL_SESSION,
            0,
            carrier.name.clone().into_bytes(),
        )
        .encode_to_vec();
        let (rh, wh) = tokio::io::split(stream);
        let this = Arc::clone(self);
        tokio::spawn(async move {
            // HELLO must precede everything else on this carrier.
            let mut wh = wh;
            if let Err(e) = wh.write_all(&hello).await {
                tracing::debug!(carrier = idx, error = %e, "bond carrier hello failed");
                let _ = this.death_tx.send((idx, "hello write failed"));
                return;
            }
            this.writer_task(idx, wh, rx).await;
        });
        let this = Arc::clone(self);
        tokio::spawn(async move { this.reader_task(idx, rh).await });
        Ok(idx)
    }

    /// Client: attach a carrier at an EXPLICIT slot index (carrier idx ==
    /// link idx). Gaps are filled with dead placeholders so a failed link
    /// keeps its slot for the respawn path.
    pub(crate) async fn attach_carrier_at(
        self: &Arc<Self>,
        stream: ProxyStream,
        name: String,
        idx: usize,
    ) -> anyhow::Result<usize> {
        {
            let mut cs = self.carriers.write().expect("carrier lock");
            while cs.len() <= idx {
                let i = cs.len();
                let (tx, rx) = mpsc::channel::<Vec<u8>>(1);
                drop(rx); // dead channel: sends fail
                let placeholder = Carrier::new(i, format!("slot-{i}"), tx);
                placeholder.alive.store(false, Ordering::Relaxed);
                cs.push(placeholder);
            }
        }
        self.attach_carrier(stream, name, Some(idx)).await
    }

    /// Carrier writer: drains queued encoded frames to the socket.
    async fn writer_task(
        self: Arc<Self>,
        idx: usize,
        mut w: tokio::io::WriteHalf<ProxyStream>,
        mut rx: mpsc::Receiver<Vec<u8>>,
    ) {
        let mut shutdown = self.shutdown.subscribe();
        loop {
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                item = rx.recv() => {
                    let Some(bytes) = item else { break };
                    if let Err(e) = w.write_all(&bytes).await {
                        tracing::debug!(carrier = idx, error = %e, "bond carrier write failed");
                        let _ = self.death_tx.send((idx, "write failed"));
                        break;
                    }
                    if let Some(c) = self.carrier_of(idx) {
                        c.bytes_up.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    }
                }
            }
        }
        let _ = w.shutdown().await;
    }

    /// Carrier reader: decodes frames and routes them.
    async fn reader_task(self: Arc<Self>, idx: usize, mut r: tokio::io::ReadHalf<ProxyStream>) {
        let mut shutdown = self.shutdown.subscribe();
        loop {
            let frame = tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                    continue;
                }
                f = frame::read_frame(&mut r) => match f {
                    Ok(f) => f,
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::UnexpectedEof {
                            tracing::debug!(carrier = idx, error = %e, "bond carrier read failed");
                        }
                        let _ = self.death_tx.send((idx, "read failed"));
                        break;
                    }
                },
            };
            if let Some(c) = self.carrier_of(idx) {
                c.last_rx_ms.store(now_millis(), Ordering::Relaxed);
                c.bytes_down
                    .fetch_add(frame.wire_len() as u64, Ordering::Relaxed);
            }
            if let Err(e) = self.handle_frame(idx, frame).await {
                tracing::debug!(carrier = idx, error = %e, "bond frame handling failed");
            }
        }
    }

    fn carrier_of(&self, idx: usize) -> Option<Arc<Carrier>> {
        self.carriers
            .read()
            .expect("carrier lock")
            .get(idx)
            .cloned()
    }

    /// Frame routing (see module docs for the sequencing rules).
    async fn handle_frame(
        self: &Arc<Self>,
        _carrier_idx: usize,
        frame: Frame,
    ) -> anyhow::Result<()> {
        match frame.kind {
            Kind::Hello => { /* consumed by the daemon accept loop; ignore */ }
            Kind::Ping => {
                // Echo on the SAME carrier (cheap, path-symmetric).
                let pong = Frame::control(Kind::Pong, self.bond_id, frame.seq, frame.payload)
                    .encode_to_vec();
                self.schedule_bytes(pong).await?;
            }
            Kind::Pong => {
                if frame.payload.len() == 8 {
                    let sent = u64::from_le_bytes(frame.payload[..8].try_into().expect("8 bytes"));
                    let rtt = now_millis().saturating_sub(sent).min(u32::MAX as u64) as u32;
                    if let Some(c) = self.carrier_of(_carrier_idx) {
                        c.rtt_ms.store(rtt, Ordering::Relaxed);
                    }
                }
            }
            Kind::Open | Kind::Data => {
                self.session_inbound(frame).await?;
            }
            Kind::Ack => {
                let Some(s) = self.session_of(frame.session) else {
                    return Ok(());
                };
                let mut un = lock(&s.unacked);
                let before = un.len();
                un.retain(|&seq, _| seq > frame.seq);
                if before != un.len() {
                    tracing::trace!(session = frame.session, freed = before - un.len(), "ack");
                }
            }
            Kind::Close => {
                let reason = CloseReason::from_u8(frame.payload.first().copied().unwrap_or(0));
                let Some(s) = self.session_of(frame.session) else {
                    return Ok(());
                };
                if self.is_server && reason == CloseReason::Eof && !s.closed() {
                    // Client write-close (half-close): the forward stream
                    // ends, the reverse stream keeps flowing until OUR
                    // final CLOSE. The session stays open.
                    s.write_closed.store(true, Ordering::Relaxed);
                    let _ = s.inbound_tx.send(Inbound::Eof(CloseReason::Eof));
                } else {
                    // Full close (peer-initiated — never notify back).
                    self.close_session(&s, reason, false).await;
                }
            }
        }
        Ok(())
    }

    /// OPEN/DATA inbound: reorder, deliver, ack.
    async fn session_inbound(self: &Arc<Self>, frame: Frame) -> anyhow::Result<()> {
        // Unknown session: the server creates a PENDING one (the OPEN may
        // still be in flight on another carrier); the client never sees
        // server-initiated sessions, so it ignores.
        let session = match self.session_of(frame.session) {
            Some(s) => s,
            None if self.is_server => self.create_remote_session(frame.session),
            None => return Ok(()),
        };
        if session.closed() {
            return Ok(());
        }
        // Insert under the lock, handle the result AFTER the scope — a
        // std MutexGuard must never live across an await (Send rule).
        let insert_result = {
            let mut reorder = lock(&session.reorder);
            reorder.insert(frame.seq, frame.payload)
        };
        let delivered = match insert_result {
            Ok(d) => d,
            Err(ReorderError::Overflow) => {
                self.close_session(&session, CloseReason::Error, true).await;
                return Ok(());
            }
        };
        if !delivered.0.is_empty() {
            *lock(&session.last_progress) = Instant::now();
        }
        for payload in delivered.0 {
            // The first in-order payload of a server session is the OPEN
            // target; everything after is DATA.
            if self.is_server && !session.opened_delivered.swap(true, Ordering::Relaxed) {
                let target = frame::decode_target(&payload)?;
                let _ = session.inbound_tx.send(Inbound::Opened(target));
            } else {
                session.backlog.fetch_add(payload.len(), Ordering::Relaxed);
                let _ = session.inbound_tx.send(Inbound::Data(payload));
            }
        }
        // Cumulative ACK for everything received in order so far.
        let ack_seq = lock(&session.reorder).expected().saturating_sub(1);
        let ack = Frame::session(Kind::Ack, self.bond_id, session.id, ack_seq, Vec::new())
            .encode_to_vec();
        self.schedule_bytes(ack).await?;
        Ok(())
    }

    fn session_of(&self, id: u32) -> Option<Arc<Session>> {
        lock(&self.sessions).get(&id).cloned()
    }

    /// Crate-facing accessor (daemon pump).
    pub(crate) fn get_session(&self, id: u32) -> Option<Arc<Session>> {
        self.session_of(id)
    }

    /// Server-side: register a session the client announced; fires the
    /// new-session notifier so the daemon pump can take the inbound rx.
    /// Caps the session count per bond (resource bound).
    fn create_remote_session(&self, id: u32) -> Arc<Session> {
        let mut map = lock(&self.sessions);
        if let Some(existing) = map.get(&id) {
            return Arc::clone(existing);
        }
        if map.len() >= MAX_SESSIONS_PER_BOND {
            tracing::warn!(session = id, "bond session cap reached — dropping");
            // Return a detached, already-closed session: the frame is
            // dropped, the client's session will stall-close.
            let s = Session::new(id);
            s.closed.store(true, Ordering::Relaxed);
            return s;
        }
        let s = Session::new(id);
        *lock(&s.created) = Some(Instant::now());
        map.insert(id, Arc::clone(&s));
        drop(map);
        if let Some(tx) = lock(&self.new_session_tx).as_ref() {
            let _ = tx.send(id);
        }
        s
    }

    /// Client-side: open a NEW local session and return (session, receiver).
    pub(crate) fn open_local_session(
        self: &Arc<Self>,
    ) -> (Arc<Session>, mpsc::UnboundedReceiver<Inbound>) {
        let id = self.next_session_id.fetch_add(1, Ordering::Relaxed);
        let session = Session::new(id);
        let rx = lock(&session.inbound_rx)
            .take()
            .expect("fresh session carries its receiver");
        lock(&self.sessions).insert(id, Arc::clone(&session));
        (session, rx)
    }

    /// Send one session frame: track it as unacked (keyed by seq, tagged
    /// with the carrier) then queue the encoded bytes. DATA + OPEN only —
    /// CLOSE/ACK/PING are fire-and-forget.
    ///
    /// RETRY: a carrier can die between the pick and the queue (the death
    /// raced our snapshot of the alive set); re-pick and re-tag instead of
    /// erroring the session (the receiver drops duplicates).
    pub(crate) async fn send_tracked(
        &self,
        session: &Session,
        kind: Kind,
        seq: u64,
        payload: Vec<u8>,
    ) -> anyhow::Result<()> {
        let frame = Frame::session(kind, self.bond_id, session.id, seq, payload);
        let bytes = frame.encode_to_vec();
        let mut last_err = None;
        for _ in 0..4 {
            let Some((idx, _c)) = self.pick_carrier() else {
                break;
            };
            lock(&session.unacked).insert(seq, (idx, bytes.clone()));
            match self.send_bytes(idx, bytes.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // The pick went stale — the entry stays in `unacked`
                    // (the death handler replays it onto a live carrier).
                    last_err = Some(e);
                }
            }
        }
        Err(last_err
            .unwrap_or_else(|| anyhow::anyhow!("no live bond carrier")))
    }

    /// Client session open: OPEN is seq 0 of the client→server direction.
    pub(crate) async fn open_remote(
        &self,
        session: &Session,
        target: &mt_net::TargetAddr,
    ) -> anyhow::Result<()> {
        let seq = session.next_seq.fetch_add(1, Ordering::Relaxed);
        debug_assert_eq!(seq, 0, "OPEN must be the first sequenced frame");
        self.send_tracked(session, Kind::Open, seq, frame::encode_target(target))
            .await
    }

    /// Split `bytes` into ≤MAX_PAYLOAD DATA frames and send them.
    pub(crate) async fn send_data(&self, session: &Session, bytes: &[u8]) -> anyhow::Result<()> {
        if bytes.is_empty() || session.closed() {
            return Ok(());
        }
        let mut offset = 0usize;
        while offset < bytes.len() {
            let end = (offset + frame::MAX_PAYLOAD).min(bytes.len());
            let seq = session.next_seq.fetch_add(1, Ordering::Relaxed);
            self.send_tracked(session, Kind::Data, seq, bytes[offset..end].to_vec())
                .await?;
            offset = end;
        }
        Ok(())
    }

    /// Close a session: idempotent; removes it, delivers EOF locally, and
    /// (when `notify_peer`) sends CLOSE AFTER flushing unacked frames — a
    /// CLOSE must never overtake in-flight DATA on another carrier, so we
    /// wait for the cumulative ACKs of everything we sent (bounded by
    /// [`CLOSE_FLUSH_TIMEOUT`]).
    pub(crate) async fn close_session(
        self: &Arc<Self>,
        session: &Arc<Session>,
        reason: CloseReason,
        notify_peer: bool,
    ) {
        if session.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        lock(&self.sessions).remove(&session.id);
        let _ = session.inbound_tx.send(Inbound::Eof(reason));
        if notify_peer {
            let session = Arc::clone(session);
            let this = Arc::clone(self);
            // Detached: the flush may legitimately take a moment.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    this.flush_then_close(session, reason).await;
                });
            }
        }
    }

    /// Client write-close: no more DATA will be sent on this session, but
    /// the read side keeps flowing until the daemon's final CLOSE. The
    /// CLOSE frame is sent only after our forward data is fully acked
    /// (overtaking protection — CLOSE is not sequenced).
    pub(crate) fn half_close_session(self: &Arc<Self>, session: &Arc<Session>) {
        if session.closed.load(Ordering::Relaxed)
            || session.write_closed.swap(true, Ordering::Relaxed)
        {
            return;
        }
        let session = Arc::clone(session);
        let this = Arc::clone(self);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                this.flush_then_close(session, CloseReason::Eof).await;
            });
        }
    }

    /// Wait until `session`'s unacked frames are acked (bounded), then send
    /// the CLOSE frame.
    async fn flush_then_close(&self, session: Arc<Session>, reason: CloseReason) {
        let deadline = Instant::now() + CLOSE_FLUSH_TIMEOUT;
        while Instant::now() < deadline {
            if lock(&session.unacked).is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let close = Frame::session(Kind::Close, self.bond_id, session.id, 0, vec![reason as u8])
            .encode_to_vec();
        let _ = self.schedule_bytes(close).await;
    }

    /// Pick a live carrier (round-robin over the live set, D17 §4).
    fn pick_carrier(&self) -> Option<(usize, Arc<Carrier>)> {
        let cs = self.carriers.read().expect("carrier lock");
        let live: Vec<&Arc<Carrier>> = cs.iter().filter(|c| c.alive()).collect();
        if live.is_empty() {
            return None;
        }
        let n = self.rr.fetch_add(1, Ordering::Relaxed);
        let c = live[(n % live.len() as u64) as usize];
        Some((c.idx, Arc::clone(c)))
    }

    /// Queue encoded bytes on a specific carrier.
    async fn send_bytes(&self, carrier_idx: usize, bytes: Vec<u8>) -> anyhow::Result<()> {
        let carrier = self
            .carriers
            .read()
            .expect("carrier lock")
            .get(carrier_idx)
            .cloned()
            .filter(|c| c.alive())
            .ok_or_else(|| anyhow::anyhow!("carrier {carrier_idx} not alive"))?;
        carrier
            .tx
            .send(bytes)
            .await
            .map_err(|_| anyhow::anyhow!("carrier {carrier_idx} writer gone"))?;
        Ok(())
    }

    /// Queue encoded bytes on the scheduler's carrier pick (retrying past
    /// the pick-vs-death race; fire-and-forget frames only).
    async fn schedule_bytes(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        let mut last_err = None;
        for _ in 0..4 {
            let Some((idx, _c)) = self.pick_carrier() else {
                break;
            };
            match self.send_bytes(idx, bytes.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no live bond carrier")))
    }

    /// Carrier death: mark dead, replay its unacked frames elsewhere,
    /// (client) schedule a respawn attempt — even when ALL carriers died
    /// (the recovery story of D17: a Wi-Fi blip must not kill the tunnel).
    pub(crate) async fn carrier_died(self: &Arc<Self>, idx: usize, why: &'static str) {
        let Some(carrier) = self.carrier_of(idx) else {
            return;
        };
        if !carrier.alive.swap(false, Ordering::Relaxed) {
            return; // already handled
        }
        tracing::info!(carrier = idx, why, "bond carrier down");
        // Replay unacked frames that were queued on the dead carrier.
        let sessions: Vec<Arc<Session>> = lock(&self.sessions).values().cloned().collect();
        for s in sessions {
            let replay: Vec<(u64, Vec<u8>)> = {
                let mut un = lock(&s.unacked);
                let doomed: Vec<u64> = un
                    .iter()
                    .filter(|(_, (c, _))| *c == idx)
                    .map(|(seq, _)| *seq)
                    .collect();
                doomed
                    .into_iter()
                    .map(|seq| (seq, un.remove(&seq).expect("just checked").1))
                    .collect()
            };
            for (seq, bytes) in replay {
                match self.pick_carrier() {
                    Some((new_idx, _)) => {
                        lock(&s.unacked).insert(seq, (new_idx, bytes.clone()));
                        if self.send_bytes(new_idx, bytes).await.is_err() {
                            // That carrier died too — the sweeper/stall path
                            // finishes the session if nothing recovers.
                        }
                    }
                    None => {
                        // No live carrier: park the frame; the respawn (or,
                        // failing that, the stall timeout) resolves it.
                        lock(&s.unacked).insert(seq, (idx, bytes));
                        break;
                    }
                }
            }
        }
        if !self.is_server && !self.is_shutting_down() {
            let this = Arc::clone(self);
            let links = this.links.clone();
            let target = this
                .merge_target
                .clone()
                .expect("client endpoint has a merge target");
            // Boxed: respawn → attach → writer → carrier_died is a cycle in
            // opaque-type computation; dyn-erasure breaks it.
            let respawn: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(async move {
                this.respawn_carrier(idx, links, target).await;
            });
            tokio::spawn(respawn);
        }
    }

    /// Client: re-dial a dead carrier slot through its link.
    async fn respawn_carrier(
        self: &Arc<Self>,
        idx: usize,
        links: Vec<Arc<dyn Outbound>>,
        target: mt_net::TargetAddr,
    ) {
        let Some(link) = links.get(idx).cloned() else {
            return;
        };
        let name = self
            .carrier_of(idx)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("carrier-{idx}"));
        for attempt in 1..=CARRIER_RESPAWN_ATTEMPTS {
            if self.is_shutting_down() || self.carrier_of(idx).is_some_and(|c| c.alive()) {
                return;
            }
            if attempt > 1 {
                tokio::time::sleep(CARRIER_RESPAWN_BACKOFF).await;
            }
            match link.connect(&target).await {
                Ok(stream) => {
                    tracing::info!(carrier = idx, attempt, "bond carrier respawned");
                    if self
                        .attach_carrier(stream, name.clone(), Some(idx))
                        .await
                        .is_ok()
                    {
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!(carrier = idx, attempt, error = %e, "bond carrier respawn dial failed");
                }
            }
        }
        tracing::warn!(carrier = idx, "bond carrier respawn budget exhausted");
    }

    /// Periodic maintenance: carrier-death reports, pings, liveness,
    /// stall + backlog sweeps.
    pub(crate) async fn sweeper_task(self: Arc<Self>) {
        let mut tick = tokio::time::interval(SWEEP_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut shutdown = self.shutdown.subscribe();
        let mut deaths = lock(&self.death_rx)
            .take()
            .expect("sweeper spawns exactly once");
        loop {
            let woke = tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                    continue;
                }
                death = deaths.recv() => match death {
                    Some((idx, why)) => {
                        self.carrier_died(idx, why).await;
                        continue;
                    }
                    None => break,
                },
                _ = tick.tick() => "tick",
            };
            let _ = woke;
            // 1) Ping every live carrier (RTT + keepalive). Collect the
            //    carrier list first — the RwLock guard never crosses an
            //    await (Send rule).
            let carriers: Vec<Arc<Carrier>> = self
                .carriers
                .read()
                .expect("carrier lock")
                .iter()
                .cloned()
                .collect();
            for c in carriers {
                if !c.alive() {
                    continue;
                }
                let seq = self.next_ping_seq.fetch_add(1, Ordering::Relaxed);
                let ping = Frame::control(
                    Kind::Ping,
                    self.bond_id,
                    seq,
                    now_millis().to_le_bytes().to_vec(),
                )
                .encode_to_vec();
                if self.send_bytes(c.idx, ping).await.is_err() {
                    let _ = self.death_tx.send((c.idx, "ping send failed"));
                }
            }
            // 2) Carrier liveness: nothing received for PONG_TIMEOUT.
            let now = now_millis();
            let liveness: Vec<Arc<Carrier>> = self
                .carriers
                .read()
                .expect("carrier lock")
                .iter()
                .cloned()
                .collect();
            for c in liveness {
                if c.alive()
                    && now.saturating_sub(c.last_rx_ms.load(Ordering::Relaxed))
                        > PONG_TIMEOUT.as_millis() as u64
                {
                    let _ = self.death_tx.send((c.idx, "pong timeout"));
                }
            }
            // 3) Session stall + backlog + pending-open sweeps.
            let sessions: Vec<Arc<Session>> = lock(&self.sessions).values().cloned().collect();
            for s in sessions {
                let (stalled, backlog) = {
                    let reorder = lock(&s.reorder);
                    let stalled =
                        reorder.has_gap() && lock(&s.last_progress).elapsed() > STALL_TIMEOUT;
                    (stalled, s.backlog.load(Ordering::Relaxed))
                };
                let zombie_open = self.is_server
                    && !s.opened_delivered.load(Ordering::Relaxed)
                    && lock(&s.created)
                        .map(|c| c.elapsed() > PENDING_OPEN_TIMEOUT)
                        .unwrap_or(false);
                if stalled || zombie_open {
                    tracing::warn!(session = s.id, stalled, zombie_open, "bond session closing");
                    self.close_session(&s, CloseReason::Stall, true).await;
                } else if backlog > APP_BACKLOG_CAP {
                    tracing::warn!(session = s.id, backlog, "bond session backlog cap exceeded");
                    self.close_session(&s, CloseReason::Error, true).await;
                }
            }
        }
    }

    /// Client status surface: one row per carrier.
    pub fn carrier_snapshot(&self) -> Vec<CarrierSnapshot> {
        self.carriers
            .read()
            .expect("carrier lock")
            .iter()
            .map(|c| CarrierSnapshot {
                idx: c.idx,
                name: c.name.clone(),
                alive: c.alive(),
                rtt_ms: (c.rtt_ms.load(Ordering::Relaxed) > 0)
                    .then(|| c.rtt_ms.load(Ordering::Relaxed)),
                bytes_up: c.bytes_up.load(Ordering::Relaxed),
                bytes_down: c.bytes_down.load(Ordering::Relaxed),
            })
            .collect()
    }

    /// Number of live carriers.
    pub fn live_carriers(&self) -> usize {
        self.carriers
            .read()
            .expect("carrier lock")
            .iter()
            .filter(|c| c.alive())
            .count()
    }

    /// Number of open sessions.
    pub fn session_count(&self) -> usize {
        lock(&self.sessions).len()
    }

    /// True when no carrier is alive.
    pub fn all_carriers_dead(&self) -> bool {
        !self
            .carriers
            .read()
            .expect("carrier lock")
            .iter()
            .any(|c| c.alive())
    }

    /// Daemon-side: take the inbound receiver of a remote session (exactly
    /// once; the pump consumes it).
    pub(crate) fn take_inbound(&self, id: u32) -> Option<mpsc::UnboundedReceiver<Inbound>> {
        self.session_of(id).and_then(|s| lock(&s.inbound_rx).take())
    }
}
