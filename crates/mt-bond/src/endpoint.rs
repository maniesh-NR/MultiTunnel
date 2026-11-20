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

/// Per-session reorder byte cap (S14: 4 → 16 MiB). Capacity-aware
/// scheduling can briefly outrun a slow carrier during saturation
/// shifts; the new overflow path (see [`Endpoint::session_inbound`])
/// turns a would-be overflow into carrier backpressure instead of a
/// hard session error, so a larger cap just buys more shift tolerance.
pub const REORDER_CAP: usize = 16 * 1024 * 1024;
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
/// Sweeper cadence (pings, liveness, stall + backlog checks). Since S14
/// this is ALSO the per-carrier delivery-rate report cadence: every
/// sweep, each endpoint pings each carrier carrying its measured inbound
/// rate, so both schedulers converge on capacity-proportional spreading
/// within ~2–3 s.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(1);
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
/// Client: how long a fresh carrier has to prove itself with a PING/PONG
/// round trip before it is considered dead (S11). Xray's SOCKS inbound
/// replies "connected" BEFORE the upstream tunnel is established, so a
/// just-dialed carrier is PHANTOM-LIVE when the merge daemon is absent or
/// the server's Xray freedom policy blackholes the merge port — bytes
/// queued into it vanish. The pong is the honest "carrier works" signal.
pub const VERIFY_PONG_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// S11: true once this carrier PROVED itself with a PING/PONG round
    /// trip (client side). The server marks carriers verified at attach
    /// (it has no dial-phantom problem: freedom hands it the stream).
    verified: AtomicBool,
    rtt_ms: AtomicU32,
    /// Wire bytes written (header + payload) — status accounting.
    bytes_up: AtomicU64,
    bytes_down: AtomicU64,
    /// Millis of the last frame received on this carrier (liveness).
    last_rx_ms: AtomicU64,
    /// S11: bytes handed to the writer channel but not yet flushed to the
    /// socket — the carrier's live BACKPRESSURE. `send_bytes` increments,
    /// the writer task decrements after each socket write. A carrier whose
    /// socket keeps up hovers near zero; a bottlenecked one grows.
    queued: AtomicU64,
    /// S14: total bytes handed to the writer (every frame kind). The
    /// sweeper differences this counter into an EWMA ASSIGNMENT rate —
    /// the scheduler's offer side.
    assigned: AtomicU64,
    /// S14: peer-reported inbound rate on this carrier (bytes/s) — the
    /// rate at which the far end actually RECEIVES on this path, i.e. the
    /// carrier's measured delivery capacity. Updated from the report
    /// piggybacked on received PINGs; 0 until the first report (an old
    /// peer never reports — the scheduler then falls back to the S12
    /// writer-queue scoring).
    deliver_bps: AtomicU64,
}

impl Carrier {
    fn new(idx: usize, name: String, tx: mpsc::Sender<Vec<u8>>) -> Arc<Self> {
        Arc::new(Self {
            idx,
            name,
            tx,
            alive: AtomicBool::new(true),
            verified: AtomicBool::new(false),
            rtt_ms: AtomicU32::new(0),
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
            last_rx_ms: AtomicU64::new(now_millis()),
            queued: AtomicU64::new(0),
            assigned: AtomicU64::new(0),
            deliver_bps: AtomicU64::new(0),
        })
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Schedulable: the transport is up AND the carrier proved its tunnel
    /// (client verify-pong). Frames are never queued into a carrier that
    /// has not proven the merge daemon actually receives them.
    fn usable(&self) -> bool {
        self.alive() && self.verified.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// S11 carrier scheduling (pure)
// ---------------------------------------------------------------------------

/// Scheduler snapshot of one LIVE carrier (the pure picker's input).
#[derive(Debug, Clone, Copy)]
struct CarrierSlot {
    /// Carrier slot index == link index == the user's adapter priority
    /// order (links are built one per bonded adapter, in settings order).
    idx: usize,
    /// Bytes queued for the socket writer (the live backpressure).
    queued: u64,
    /// Last PING round trip in ms (0 = no sample yet).
    rtt_ms: u32,
    /// S14: EWMA of this carrier's ASSIGNMENT rate (bytes/s) — what the
    /// scheduler has been handing it.
    assign_bps: f64,
    /// S14: peer-reported DELIVERY rate (bytes/s) — what the far end
    /// actually receives on this carrier (0 = no report yet).
    deliver_bps: f64,
}

/// S14 relative-load score: assignment rate over delivered rate. ≈1.0
/// means the carrier is keeping up with what it is handed; >1.0 means
/// its path cannot drain the offered rate (over-committed). Carriers
/// without a report are probed up to [`PROBE_BPS`] before they look
/// busy. The equilibrium of min-score scheduling is every carrier
/// carrying ≈ its own capacity — the aggregate throughput is the SUM of
/// the paths instead of the pace of the slowest one.
fn carrier_score(slot: &CarrierSlot) -> f64 {
    slot.assign_bps / slot.deliver_bps.max(PROBE_BPS)
}

/// A carrier whose writer queue exceeds this many BYTES counts as
/// SATURATED for the `Priority` policy — its socket provably cannot keep
/// up with the offered rate. 512 KiB ≈ 8 max-size frames: a healthy
/// carrier hovers near zero, a bottlenecked one grows without bound, so
/// the spill decision lands within a fraction of a second at any real
/// rate difference.
pub const PRIORITY_QUEUE_HIGH_WATER: u64 = 512 * 1024;
/// S14 scheduler score floor (bytes/s): while a carrier has no
/// peer-reported delivery rate (fresh carrier, or an old peer that does
/// not send reports), it is PROBED up to this rate before its score
/// makes it look busy.
pub const PROBE_BPS: f64 = 32.0 * 1024.0;
/// S14 `Priority` spill ratio: a carrier counts as saturated once its
/// EWMA assignment rate exceeds its peer-reported delivery rate by
/// ≥ 15 % (a little margin avoids flapping on report jitter).
pub const PRIORITY_SPILL_RATIO: f64 = 1.15;
/// S14 EWMA smoothing per 1 s sweep for the assignment/inbound rates.
pub const RATE_EWMA_ALPHA: f64 = 0.5;
/// S14: pause granularity when an inbound frame would overflow the
/// reorder buffer (the reader holds the frame; the carrier's TCP
/// backpressure throttles the sender while the gap drains).
pub const REORDER_PAUSE: Duration = Duration::from_millis(100);
/// S14: per-frame reorder-overflow pause budget. When it expires the
/// frame is inserted anyway (soft cap — the overshoot is at most one
/// frame per budget window; the stall sweeper closes a truly stuck
/// session long before this accumulates).
pub const REORDER_PAUSE_MAX: Duration = Duration::from_secs(5);

/// Pure carrier pick (the D17 round-robin, made policy-aware in S11,
/// capacity-aware in S14). Returns the SLOT INDEX to send the next
/// frame on:
/// - `Failover`       — first live carrier (idx order).
/// - `RoundRobin`     — rotate over live carriers (cursor).
/// - `LowestLatency`  — best sampled RTT (known beats unknown, ties keep
///   the earlier carrier).
/// - `Balanced`       — least relative load (assignment rate ÷ the
///   peer-reported delivery rate); ties rotate via the cursor. A fast
///   adapter reports a high delivery rate and absorbs frames in
///   proportion to its capacity; an adapter whose reported rate lags its
///   assignment (a saturated uplink/downlink) is bypassed until it
///   catches up. With NO reports at all (old peer) this degrades to the
///   S12 least-queued-writer behavior.
/// - `Priority`       — first carrier in USER ORDER that is neither
///   over-reported (score < [`PRIORITY_SPILL_RATIO`]) nor writer-backed-up
///   ([`PRIORITY_QUEUE_HIGH_WATER`]); when every carrier is saturated,
///   the least-loaded one takes the excess.
fn pick_carrier_index(
    policy: mt_net::MergePolicy,
    slots: &[CarrierSlot],
    cursor: &mut u64,
) -> Option<usize> {
    if slots.is_empty() {
        return None;
    }
    match policy {
        mt_net::MergePolicy::Failover => Some(slots[0].idx),
        mt_net::MergePolicy::RoundRobin => {
            let s = slots[(*cursor % slots.len() as u64) as usize];
            *cursor = cursor.wrapping_add(1);
            Some(s.idx)
        }
        mt_net::MergePolicy::LowestLatency => {
            let mut best = 0;
            for i in 1..slots.len() {
                let (b, c) = (slots[best], slots[i]);
                let b_known = b.rtt_ms > 0;
                let c_known = c.rtt_ms > 0;
                if (c_known && !b_known) || (c_known == b_known && c.rtt_ms < b.rtt_ms) {
                    best = i;
                }
            }
            Some(slots[best].idx)
        }
        mt_net::MergePolicy::Balanced => {
            if slots.iter().all(|s| s.deliver_bps <= 0.0) {
                // No reports (old peer): the S12 behavior — least writer
                // backpressure, ties rotating like round-robin.
                let min = slots.iter().map(|s| s.queued).min().expect("non-empty");
                let tied: Vec<usize> = (0..slots.len())
                    .filter(|&i| slots[i].queued == min)
                    .collect();
                let pick = tied[(*cursor % tied.len() as u64) as usize];
                *cursor = cursor.wrapping_add(1);
                return Some(slots[pick].idx);
            }
            // Capacity-aware: least relative load; ties rotate.
            let min = slots
                .iter()
                .map(carrier_score)
                .fold(f64::INFINITY, f64::min);
            let tied: Vec<usize> = (0..slots.len())
                .filter(|&i| (carrier_score(&slots[i]) - min).abs() < f64::EPSILON)
                .collect();
            let pick = tied[(*cursor % tied.len() as u64) as usize];
            *cursor = cursor.wrapping_add(1);
            Some(slots[pick].idx)
        }
        mt_net::MergePolicy::Priority => {
            if slots.iter().all(|s| s.deliver_bps <= 0.0) {
                // No reports (old peer): the S12 behavior.
                for s in slots {
                    if s.queued < PRIORITY_QUEUE_HIGH_WATER {
                        return Some(s.idx);
                    }
                }
                return slots.iter().min_by_key(|s| s.queued).map(|s| s.idx);
            }
            for s in slots {
                if carrier_score(s) < PRIORITY_SPILL_RATIO && s.queued < PRIORITY_QUEUE_HIGH_WATER {
                    return Some(s.idx);
                }
            }
            slots
                .iter()
                .min_by(|a, b| {
                    carrier_score(a)
                        .partial_cmp(&carrier_score(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|s| s.idx) // every carrier saturated → least relative load
        }
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

/// S14 per-endpoint scheduler statistics, updated once per sweep from
/// the carriers' monotonic counters (the hot send path only touches the
/// atomics). Index == carrier idx; rebuilt when the carrier table
/// changes (a respawn replaces the Carrier, resetting its counters —
/// a counter that moves backwards is read as "reset", delta = current).
#[derive(Default)]
struct RateState {
    /// Previous `assigned` snapshot per carrier idx.
    prev_assigned: Vec<u64>,
    /// Previous `bytes_down` snapshot per carrier idx.
    prev_down: Vec<u64>,
    /// EWMA assignment rate (bytes/s) per carrier idx — the scheduler's
    /// offer side (read by [`Endpoint::pick_carrier`]).
    assign_ewma: Vec<f64>,
    /// EWMA inbound rate (bytes/s) per carrier idx — the value reported
    /// to the peer in the 1 s PINGs ("this is what I receive on you").
    inbound_ewma: Vec<f64>,
}

/// The bond endpoint shared by client and daemon.
pub(crate) struct Endpoint {
    pub bond_id: u32,
    pub is_server: bool,
    carriers: RwLock<Vec<Arc<Carrier>>>,
    rr: AtomicU64,
    /// S11: how `pick_carrier` spreads frames across live carriers (the
    /// user's merge policy, mapped onto carrier scheduling; the daemon
    /// side uses `Balanced` — capacity-proportional in BOTH directions,
    /// the user's priority order being a client concern).
    carrier_policy: mt_net::MergePolicy,
    /// S14: assignment/inbound rate EWMA state (sweeper-written,
    /// pick_carrier-read).
    rates: Mutex<RateState>,
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
    /// Client only: links carriers are dialed through + merge target.
    links: Vec<Arc<dyn Outbound>>,
    merge_target: Option<mt_net::TargetAddr>,
    next_ping_seq: AtomicU64,
    /// Verify pings in flight: ping seq → carrier idx (S11). A pong
    /// carrying that seq proves THAT carrier's tunnel is bidirectional.
    pending_verify: Mutex<HashMap<u64, usize>>,
}

impl Endpoint {
    /// Client-side endpoint with the carrier-scheduling policy (S11: the
    /// GUI's merge policy rides the bond — `Balanced` spreads frames by
    /// live writer backpressure, `Priority` fills carriers in user order
    /// until one backs up; `RoundRobin` is the stock behavior).
    pub fn client_with_policy(
        bond_id: u32,
        links: Vec<Arc<dyn Outbound>>,
        merge_target: mt_net::TargetAddr,
        carrier_policy: mt_net::MergePolicy,
    ) -> Arc<Self> {
        let (shutdown, _) = watch::channel(false);
        let (death_tx, death_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            bond_id,
            is_server: false,
            carriers: RwLock::new(Vec::new()),
            rr: AtomicU64::new(0),
            carrier_policy,
            rates: Mutex::new(RateState::default()),
            sessions: Mutex::new(HashMap::new()),
            next_session_id: AtomicU32::new(1),
            new_session_tx: Mutex::new(None),
            shutdown,
            death_tx,
            death_rx: Mutex::new(Some(death_rx)),
            links,
            merge_target: Some(merge_target),
            next_ping_seq: AtomicU64::new(0),
            pending_verify: Mutex::new(HashMap::new()),
        })
    }

    /// Server-side endpoint (carriers are accepted connections). Since
    /// S14 the daemon schedules its carriers with `Balanced` — the
    /// DOWNLOAD direction is the fat one for most users, and a plain
    /// round-robin paces the whole stream at the slowest adapter; the
    /// peer-reported delivery rates make the spreading
    /// capacity-proportional. (The user's priority order is a client
    /// concern — the daemon cannot know it.)
    pub fn server(bond_id: u32) -> Arc<Self> {
        let (shutdown, _) = watch::channel(false);
        let (death_tx, death_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            bond_id,
            is_server: true,
            carriers: RwLock::new(Vec::new()),
            rr: AtomicU64::new(0),
            carrier_policy: mt_net::MergePolicy::Balanced,
            rates: Mutex::new(RateState::default()),
            sessions: Mutex::new(HashMap::new()),
            next_session_id: AtomicU32::new(1),
            new_session_tx: Mutex::new(None),
            shutdown,
            death_tx,
            death_rx: Mutex::new(Some(death_rx)),
            links: Vec::new(),
            merge_target: None,
            next_ping_seq: AtomicU64::new(0),
            pending_verify: Mutex::new(HashMap::new()),
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
        if self.is_server {
            // The server has no dial-phantom problem: freedom handed it the
            // stream, so the tunnel demonstrably exists. Verified at attach.
            carrier.verified.store(true, Ordering::Relaxed);
        } else {
            // S11 honest carriers: queue a verify PING right behind the
            // HELLO (same writer, so in-order on this carrier). The carrier
            // becomes schedulable only when ITS pong returns — matched by
            // seq, wherever the daemon schedules the reply. Until then the
            // carrier is phantom-live: Xray's SOCKS inbound answered the
            // dial, but nothing proves the merge daemon receives frames.
            let seq = self.next_ping_seq.fetch_add(1, Ordering::Relaxed);
            lock(&self.pending_verify).insert(seq, idx);
            let ping = Frame::control(
                Kind::Ping,
                self.bond_id,
                seq,
                now_millis().to_le_bytes().to_vec(),
            )
            .encode_to_vec();
            if carrier.tx.send(ping).await.is_err() {
                let _ = self.death_tx.send((idx, "verify ping failed"));
            }
        }
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

    /// Client: wait until every slot in `slots` has proven itself with a
    /// verify-pong (or the budget expires). Returns the VERIFIED subset in
    /// slot order. Callers treat the unverified remainder as dead — bytes
    /// queued into them would vanish into a tunnel that never came up.
    pub(crate) async fn await_verified(
        self: &Arc<Self>,
        slots: &[usize],
        budget: Duration,
    ) -> Vec<usize> {
        let deadline = Instant::now() + budget;
        loop {
            let verified: Vec<usize> = slots
                .iter()
                .copied()
                .filter(|i| self.carrier_of(*i).is_some_and(|c| c.usable()))
                .collect();
            if verified.len() == slots.len() || Instant::now() >= deadline {
                return verified;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Client: abandon a dialed-but-unproven carrier slot — drop its verify
    /// registration, mark it dead. Unlike `carrier_died` this does NOT
    /// replay frames (nothing was ever scheduled onto an unverified
    /// carrier) and does NOT spawn another respawn task (the caller —
    /// ensure_carriers or respawn_carrier — owns the retry loop).
    pub(crate) fn abandon_unverified(&self, idx: usize) {
        {
            let mut pv = lock(&self.pending_verify);
            pv.retain(|_, &mut v| v != idx);
        }
        if let Some(c) = self.carrier_of(idx) {
            c.alive.store(false, Ordering::Relaxed);
        }
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
                    let flushed = w.write_all(&bytes).await;
                    // S11: the bytes left the writer queue either way
                    // (a failed write kills the carrier — a fresh Carrier
                    // with a zeroed counter replaces it on respawn).
                    if let Some(c) = self.carrier_of(idx) {
                        c.queued.fetch_sub(bytes.len() as u64, Ordering::Relaxed);
                        if flushed.is_ok() {
                            c.bytes_up.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                        }
                    }
                    if let Err(e) = flushed {
                        tracing::debug!(carrier = idx, error = %e, "bond carrier write failed");
                        let _ = self.death_tx.send((idx, "write failed"));
                        break;
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
                        // An EOF is DIAGNOSTIC GOLD (S11 field failure): it
                        // means the peer (or the tunnel under us) closed our
                        // write direction — never swallow it silently.
                        tracing::debug!(carrier = idx, error = %e, "bond carrier read ended");
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
                // S14: a 16-byte ping payload carries [ts_ms, inbound_bps]
                // — the PEER's measured delivery rate on THIS carrier.
                // This is the capacity signal the scheduler scores on
                // (both ends send these every sweep). Short/old payloads
                // carry no report and are ignored.
                if frame.payload.len() >= 16 {
                    let rate =
                        u64::from_le_bytes(frame.payload[8..16].try_into().expect("8 bytes"));
                    if let Some(c) = self.carrier_of(_carrier_idx) {
                        c.deliver_bps.store(rate, Ordering::Relaxed);
                    }
                }
                // Echo on the SAME carrier (cheap, path-symmetric).
                let pong = Frame::control(Kind::Pong, self.bond_id, frame.seq, frame.payload)
                    .encode_to_vec();
                self.schedule_bytes(pong).await?;
            }
            Kind::Pong => {
                // S14: payloads are >= 8 bytes (16 since the S14 rate
                // report; older peers echoed exactly 8).
                if frame.payload.len() >= 8 {
                    let sent = u64::from_le_bytes(frame.payload[..8].try_into().expect("8 bytes"));
                    let rtt = now_millis().saturating_sub(sent).min(u32::MAX as u64) as u32;
                    if let Some(c) = self.carrier_of(_carrier_idx) {
                        c.rtt_ms.store(rtt, Ordering::Relaxed);
                    }
                }
                // S11: a pong carrying a VERIFY ping's seq proves that
                // carrier's tunnel end-to-end (the daemon exists, receives
                // frames, and answers) — wherever the reply rode back.
                let verified_idx = lock(&self.pending_verify).remove(&frame.seq);
                if let Some(v_idx) = verified_idx {
                    if let Some(c) = self.carrier_of(v_idx) {
                        c.verified.store(true, Ordering::Relaxed);
                        tracing::debug!(carrier = v_idx, "bond carrier verified (pong)");
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
        //
        // S14: when the frame would overflow the reorder buffer (the
        // fast carriers have run far ahead of a slow one), HOLD the
        // frame instead of failing the session: this reader stops
        // draining its carrier, TCP backpressure throttles the sender,
        // and the gap drains via the other carriers (frames on one
        // carrier arrive in seq order, so the missing frames can never
        // be stuck behind THIS one on the same carrier — no deadlock).
        // Bounded by REORDER_PAUSE_MAX (soft insert, at most one frame
        // of overshoot per budget) and by the stall sweeper.
        let payload = frame.payload;
        let mut waited = Duration::ZERO;
        let insert_result = loop {
            let fits = {
                let reorder = lock(&session.reorder);
                !reorder.would_overflow(payload.len())
            };
            if fits {
                break {
                    let mut reorder = lock(&session.reorder);
                    reorder.insert(frame.seq, payload)
                };
            }
            if waited >= REORDER_PAUSE_MAX || session.closed() || self.is_shutting_down() {
                tracing::warn!(
                    session = session.id,
                    waited_ms = waited.as_millis() as u64,
                    "bond reorder overflow — pause budget exhausted"
                );
                break {
                    let mut reorder = lock(&session.reorder);
                    reorder.insert(frame.seq, payload)
                };
            }
            tokio::time::sleep(REORDER_PAUSE).await;
            waited += REORDER_PAUSE;
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
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no live bond carrier")))
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

    /// Pick a live VERIFIED carrier under the endpoint's scheduling
    /// policy (the D17 round-robin became policy-aware in S12 and
    /// capacity-aware in S14 — see [`pick_carrier_index`]). Unverified
    /// (phantom-live) carriers are never schedulable: bytes queued into
    /// them are lost when the tunnel never actually came up.
    fn pick_carrier(&self) -> Option<(usize, Arc<Carrier>)> {
        let cs = self.carriers.read().expect("carrier lock");
        let assign: Vec<f64> = {
            // Snapshot the assignment EWMA under the (short-held) stats
            // lock; a missing entry (fresh carrier) reads as 0.
            let rates = lock(&self.rates);
            cs.iter()
                .map(|c| rates.assign_ewma.get(c.idx).copied().unwrap_or(0.0))
                .collect()
        };
        let slots: Vec<CarrierSlot> = cs
            .iter()
            .enumerate()
            .filter(|(_, c)| c.usable())
            .map(|(i, c)| CarrierSlot {
                idx: c.idx,
                queued: c.queued.load(Ordering::Relaxed),
                rtt_ms: c.rtt_ms.load(Ordering::Relaxed),
                assign_bps: assign[i],
                deliver_bps: c.deliver_bps.load(Ordering::Relaxed) as f64,
            })
            .collect();
        drop(cs);
        // Best-effort fairness cursor: concurrent senders may lose a
        // tie-rotation increment (both pick the same carrier — harmless,
        // one extra frame there); correctness never depends on it.
        let mut cursor = self.rr.load(Ordering::Relaxed);
        let idx = pick_carrier_index(self.carrier_policy, &slots, &mut cursor)?;
        self.rr.store(cursor, Ordering::Relaxed);
        let carrier = self
            .carriers
            .read()
            .expect("carrier lock")
            .get(idx)
            .cloned()
            .filter(|c| c.usable())?;
        Some((idx, carrier))
    }

    /// Queue encoded bytes on a specific carrier.
    async fn send_bytes(&self, carrier_idx: usize, bytes: Vec<u8>) -> anyhow::Result<()> {
        let carrier = self
            .carriers
            .read()
            .expect("carrier lock")
            .get(carrier_idx)
            .cloned()
            .filter(|c| c.usable())
            .ok_or_else(|| anyhow::anyhow!("carrier {carrier_idx} not usable"))?;
        // S12: count the bytes as queued BEFORE the send (backpressure
        // accounting for the Balanced / Priority picks); S14: also count
        // them as ASSIGNED (the rate the scheduler offers this carrier —
        // differenced into an EWMA by the sweeper).
        carrier
            .queued
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        carrier
            .assigned
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        if carrier
            .tx
            .send(bytes)
            .await
            .map_err(|_| anyhow::anyhow!("carrier {carrier_idx} writer gone"))
            .is_err()
        {
            // The writer is gone: the queue will never drain. Reset so a
            // respawned carrier (or this dead one, briefly) does not read
            // as eternally backlogged.
            carrier.queued.store(0, Ordering::Relaxed);
            return Err(anyhow::anyhow!("carrier {carrier_idx} writer gone"));
        }
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
                    // The DIAL succeeding is NOT enough (S11): Xray's SOCKS
                    // inbound answers before the upstream exists, so the
                    // slot may be phantom-live. Demand a verify-pong before
                    // declaring the respawn a success.
                    if self
                        .attach_carrier(stream, name.clone(), Some(idx))
                        .await
                        .is_ok()
                    {
                        let verified = self.await_verified(&[idx], VERIFY_PONG_TIMEOUT).await;
                        if !verified.is_empty() {
                            tracing::info!(carrier = idx, attempt, "bond carrier respawned");
                            return;
                        }
                        tracing::warn!(
                            carrier = idx,
                            attempt,
                            "bond carrier respawn dial answered but the merge daemon \
                             never ponged — treating the slot as down"
                        );
                        self.abandon_unverified(idx);
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
            // 1) S14 rate sweep: difference every carrier's assigned /
            //    received counters into EWMA rates (the scheduler's input
            //    on this end) and snapshot the per-carrier inbound EWMA
            //    (the report the pings below carry to the peer).
            let carriers: Vec<Arc<Carrier>> = self
                .carriers
                .read()
                .expect("carrier lock")
                .iter()
                .cloned()
                .collect();
            let reports: Vec<(usize, u64)> = {
                let mut rates = lock(&self.rates);
                let n = carriers.len();
                if rates.prev_assigned.len() != n {
                    // Table changed (first sweep / respawn / new carrier):
                    // re-seed from the current counters — deltas start
                    // next sweep.
                    rates.prev_assigned = carriers
                        .iter()
                        .map(|c| c.assigned.load(Ordering::Relaxed))
                        .collect();
                    rates.prev_down = carriers
                        .iter()
                        .map(|c| c.bytes_down.load(Ordering::Relaxed))
                        .collect();
                    rates.assign_ewma.resize(n, 0.0);
                    rates.inbound_ewma.resize(n, 0.0);
                }
                let dt = SWEEP_INTERVAL.as_secs_f64().max(0.001);
                carriers
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let cur_a = c.assigned.load(Ordering::Relaxed);
                        let cur_d = c.bytes_down.load(Ordering::Relaxed);
                        // saturating_sub: a respawned carrier's counters
                        // restart at 0 — read as "reset", delta = current.
                        let d_assigned = cur_a.saturating_sub(rates.prev_assigned[i]) as f64 / dt;
                        let d_down = cur_d.saturating_sub(rates.prev_down[i]) as f64 / dt;
                        rates.prev_assigned[i] = cur_a;
                        rates.prev_down[i] = cur_d;
                        rates.assign_ewma[i] = RATE_EWMA_ALPHA * d_assigned
                            + (1.0 - RATE_EWMA_ALPHA) * rates.assign_ewma[i];
                        rates.inbound_ewma[i] = RATE_EWMA_ALPHA * d_down
                            + (1.0 - RATE_EWMA_ALPHA) * rates.inbound_ewma[i];
                        (c.idx, rates.inbound_ewma[i].max(0.0) as u64)
                    })
                    .collect()
            };
            // 1b) Ping every live carrier (RTT + keepalive); since S14 the
            //     payload carries [ts_ms, inbound_bps] — this end's
            //     measured receive rate on THAT carrier, which the peer's
            //     scheduler scores as the carrier's delivery capacity.
            for (idx, inbound_bps) in reports {
                let Some(c) = self.carrier_of(idx) else {
                    continue;
                };
                if !c.alive() {
                    continue;
                }
                let seq = self.next_ping_seq.fetch_add(1, Ordering::Relaxed);
                let mut payload = now_millis().to_le_bytes().to_vec();
                payload.extend_from_slice(&inbound_bps.to_le_bytes());
                let ping = Frame::control(Kind::Ping, self.bond_id, seq, payload).encode_to_vec();
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
                // Honest status (S11): a dialed-but-unproven carrier is NOT
                // alive — the GUI must not show a phantom carrier as up.
                alive: c.usable(),
                rtt_ms: (c.rtt_ms.load(Ordering::Relaxed) > 0)
                    .then(|| c.rtt_ms.load(Ordering::Relaxed)),
                bytes_up: c.bytes_up.load(Ordering::Relaxed),
                bytes_down: c.bytes_down.load(Ordering::Relaxed),
            })
            .collect()
    }

    /// Number of usable (alive AND verified) carriers.
    pub fn live_carriers(&self) -> usize {
        self.carriers
            .read()
            .expect("carrier lock")
            .iter()
            .filter(|c| c.usable())
            .count()
    }

    /// Number of open sessions.
    pub fn session_count(&self) -> usize {
        lock(&self.sessions).len()
    }

    /// True when no carrier is usable (alive AND verified).
    pub fn all_carriers_dead(&self) -> bool {
        !self
            .carriers
            .read()
            .expect("carrier lock")
            .iter()
            .any(|c| c.usable())
    }

    /// Daemon-side: take the inbound receiver of a remote session (exactly
    /// once; the pump consumes it).
    pub(crate) fn take_inbound(&self, id: u32) -> Option<mpsc::UnboundedReceiver<Inbound>> {
        self.session_of(id).and_then(|s| lock(&s.inbound_rx).take())
    }
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;

    fn slot(idx: usize, queued: u64, rtt_ms: u32) -> CarrierSlot {
        CarrierSlot {
            idx,
            queued,
            rtt_ms,
            // No peer report: exercises the S12 fallback paths (least
            // writer backpressure) until a delivery rate arrives.
            assign_bps: 0.0,
            deliver_bps: 0.0,
        }
    }

    /// Capacity-aware slot with explicit S14 rate state.
    fn cap_slot(idx: usize, queued: u64, rtt_ms: u32, assign: f64, deliver: f64) -> CarrierSlot {
        CarrierSlot {
            idx,
            queued,
            rtt_ms,
            assign_bps: assign,
            deliver_bps: deliver,
        }
    }

    fn pick1(policy: mt_net::MergePolicy, slots: &[CarrierSlot]) -> Option<usize> {
        let mut cursor = 0u64;
        pick_carrier_index(policy, slots, &mut cursor)
    }

    #[test]
    fn failover_takes_the_first_live_carrier() {
        let slots = [slot(0, 900_000, 0), slot(1, 0, 0), slot(2, 0, 0)];
        assert_eq!(pick1(mt_net::MergePolicy::Failover, &slots), Some(0));
        assert_eq!(pick1(mt_net::MergePolicy::Failover, &[]), None);
    }

    #[test]
    fn round_robin_cycles_the_cursor() {
        let slots = [slot(0, 0, 0), slot(1, 0, 0), slot(2, 0, 0)];
        let mut cursor = 0u64;
        let seq: Vec<usize> = (0..4)
            .map(|_| {
                pick_carrier_index(mt_net::MergePolicy::RoundRobin, &slots, &mut cursor).unwrap()
            })
            .collect();
        assert_eq!(seq, vec![0, 1, 2, 0]);
        assert_eq!(cursor, 4);
    }

    #[test]
    fn lowest_latency_prefers_sampled_rtt_then_known_over_unknown() {
        // Fastest sampled carrier wins.
        let slots = [slot(0, 0, 120), slot(1, 0, 0), slot(2, 0, 45)];
        assert_eq!(
            pick1(mt_net::MergePolicy::LowestLatency, &slots),
            Some(2),
            "known 45 ms beats unknown and known 120 ms"
        );
        // A sample beats no sample (0 = not yet measured).
        let unknowns = [slot(0, 0, 0), slot(1, 0, 300)];
        assert_eq!(
            pick1(mt_net::MergePolicy::LowestLatency, &unknowns),
            Some(1)
        );
        // All unknown → first.
        let all_unknown = [slot(0, 0, 0), slot(1, 0, 0)];
        assert_eq!(
            pick1(mt_net::MergePolicy::LowestLatency, &all_unknown),
            Some(0)
        );
    }

    #[test]
    fn balanced_picks_the_least_backlogged_carrier() {
        // The user's exact complaint shape: one adapter carries the fat
        // flow (deep queue), the others are nearly drained.
        let slots = [
            slot(0, 800_000, 30),
            slot(1, 120_000, 40),
            slot(2, 5_000, 55),
        ];
        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(2));
    }

    #[test]
    fn balanced_ties_rotate() {
        let slots = [slot(0, 0, 0), slot(1, 0, 0), slot(2, 0, 0)];
        let mut cursor = 0u64;
        let seq: Vec<usize> = (0..3)
            .map(|_| {
                pick_carrier_index(mt_net::MergePolicy::Balanced, &slots, &mut cursor).unwrap()
            })
            .collect();
        assert_eq!(seq, vec![0, 1, 2], "equal queues rotate like round-robin");
    }

    #[test]
    fn priority_fills_in_order_until_the_queue_backs_up() {
        // Everything below the high-water mark rides carrier 0.
        let healthy = [slot(0, 100_000, 30), slot(1, 0, 40), slot(2, 0, 55)];
        assert_eq!(pick1(mt_net::MergePolicy::Priority, &healthy), Some(0));
        // Carrier 0 backed up (≥ high water) → spill to carrier 1.
        let c0_saturated = [
            slot(0, PRIORITY_QUEUE_HIGH_WATER, 30),
            slot(1, 200_000, 40),
            slot(2, 0, 55),
        ];
        assert_eq!(pick1(mt_net::MergePolicy::Priority, &c0_saturated), Some(1));
        // Everything backed up → the least-queued carrier takes the excess.
        let all_saturated = [
            slot(0, 2_000_000, 30),
            slot(1, 700_000, 40),
            slot(2, 1_500_000, 55),
        ];
        assert_eq!(
            pick1(mt_net::MergePolicy::Priority, &all_saturated),
            Some(1)
        );
    }

    #[test]
    fn carrier_score_is_relative_load_with_probe_floor() {
        // Keeping up: offered == delivered → score 1.0.
        let ok = cap_slot(0, 0, 0, 1_000_000.0, 1_000_000.0);
        assert!((carrier_score(&ok) - 1.0).abs() < 1e-9);
        // Over-committed uplink: 2 MiB/s offered on a 1 MiB/s path → 2.0.
        let over = cap_slot(0, 0, 0, 2_000_000.0, 1_000_000.0);
        assert!((carrier_score(&over) - 2.0).abs() < 1e-9);
        // No report yet → probed at PROBE_BPS: 64 KiB/s offered → 2.0.
        let probe = cap_slot(0, 0, 0, 2.0 * PROBE_BPS, 0.0);
        assert!((carrier_score(&probe) - 2.0).abs() < 1e-9);
        // Idle + no report → 0 (looks free).
        let idle = cap_slot(0, 0, 0, 0.0, 0.0);
        assert_eq!(carrier_score(&idle), 0.0);
    }

    #[test]
    fn balanced_capacity_aware_bypasses_the_saturated_adapter() {
        // The user's saturation complaint: adapter 0 (Wi-Fi) is at its
        // ceiling — it is OFFERED 1.6 MiB/s but only DELIVERS 1.0 — while
        // adapter 1 (Ethernet) has headroom (offered 0.2, delivers 2.0).
        // Capacity-aware Balanced must pick adapter 1 even though adapter
        // 0 has the SHALLOWER writer queue (queue depth alone is not a
        // saturation signal).
        let slots = [
            cap_slot(0, 10_000, 30, 1.6 * 1024.0 * 1024.0, 1.0 * 1024.0 * 1024.0),
            cap_slot(1, 900_000, 40, 0.2 * 1024.0 * 1024.0, 2.0 * 1024.0 * 1024.0),
        ];
        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(1));
        // Equal relative load (both 0.5, above the probe floor) →
        // capacity-proportional tie via the cursor.
        let tied = [
            cap_slot(0, 0, 30, 1.0 * 1024.0 * 1024.0, 2.0 * 1024.0 * 1024.0),
            cap_slot(1, 0, 40, 0.5 * 1024.0 * 1024.0, 1.0 * 1024.0 * 1024.0),
        ];
        let mut cursor = 0u64;
        let seq: Vec<usize> = (0..2)
            .map(|_| pick_carrier_index(mt_net::MergePolicy::Balanced, &tied, &mut cursor).unwrap())
            .collect();
        assert_eq!(seq, vec![0, 1]);
    }

    #[test]
    fn balanced_spreads_proportionally_to_reported_capacity() {
        // 10:1 capacity ratio. Greedy min-relative-load picking converges
        // on the equilibrium where every carrier's assign/deliver ratio is
        // equal — i.e. the fat path ends up carrying ~10x the thin one's
        // share. Assert the crossover points around that equilibrium.
        let fat = cap_slot(0, 0, 30, 0.0, 10_000_000.0);
        let thin = cap_slot(1, 0, 40, 3_000_000.0, 1_000_000.0);
        // fat score 0 < thin score 3.0 → fat wins; still winning at 0.4.
        let mut slots = [fat, thin];
        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(0));
        slots[0].assign_bps = 4_000_000.0; // 0.4 < 3.0
        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(0));
        // Near equilibrium (fat 0.99 vs thin 1.0) fat still wins…
        slots[0].assign_bps = 9_900_000.0; // 0.99
        slots[1].assign_bps = 1_000_000.0; // 1.0
        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(0));
        // …and flips the moment its relative load exceeds thin's.
        slots[0].assign_bps = 10_100_000.0; // 1.01 > 1.0
        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(1));
    }

    #[test]
    fn priority_spills_when_relative_load_exceeds_ratio() {
        // Adapter 0 is #1 in user order and its writer queue is EMPTY —
        // but the peer reports it only delivers 1 MiB/s while it is being
        // offered 1.2 MiB/s (score 1.2 ≥ PRIORITY_SPILL_RATIO) → spill to
        // adapter 1 even though its queue is deeper.
        let c0_ceiling = [
            cap_slot(0, 0, 30, 1.2 * 1024.0 * 1024.0, 1.0 * 1024.0 * 1024.0),
            cap_slot(1, 300_000, 40, 0.1 * 1024.0 * 1024.0, 4.0 * 1024.0 * 1024.0),
        ];
        assert_eq!(
            pick1(mt_net::MergePolicy::Priority, &c0_ceiling),
            Some(1),
            "queue-empty but over-reported #1 must spill"
        );
        // Just under the ratio (1.1 < 1.15) → #1 keeps filling.
        let c0_healthy = [
            cap_slot(0, 0, 30, 1.1 * 1024.0 * 1024.0, 1.0 * 1024.0 * 1024.0),
            cap_slot(1, 300_000, 40, 0.1 * 1024.0 * 1024.0, 4.0 * 1024.0 * 1024.0),
        ];
        assert_eq!(pick1(mt_net::MergePolicy::Priority, &c0_healthy), Some(0));
        // Everything over-reported → the least RELATIVE load takes the
        // excess (0.9 beats 2.5), not the least-queued.
        let all_over = [
            cap_slot(
                0,
                1_000_000,
                30,
                0.9 * 1024.0 * 1024.0,
                1.0 * 1024.0 * 1024.0,
            ),
            cap_slot(1, 100_000, 40, 5.0 * 1024.0 * 1024.0, 2.0 * 1024.0 * 1024.0),
        ];
        assert_eq!(pick1(mt_net::MergePolicy::Priority, &all_over), Some(0));
    }

    #[test]
    fn mixed_reports_probe_the_unreported_carrier() {
        // One reporter (delivering fine, score 0.5) vs one silent peer
        // being offered 64 KiB/s → probed score 2.0: the reported carrier
        // wins despite the silent one having the empty queue.
        let slots = [
            cap_slot(0, 500_000, 30, 0.5 * 1024.0 * 1024.0, 1.0 * 1024.0 * 1024.0),
            cap_slot(1, 0, 40, 64.0 * 1024.0, 0.0),
        ];
        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(0));
    }
}
