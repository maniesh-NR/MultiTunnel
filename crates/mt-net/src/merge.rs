//! Link aggregation (net-merge style): health table, merge policies,
//! quarantine, per-link LOAD tracking and the per-link background prober.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B", extended by S11/D23):
//! - `MergePolicy::{Failover, RoundRobin, LowestLatency, Balanced, Priority}`.
//! - `LinkHealth { ok, latency, ewma_ms, consecutive_failures,
//!   quarantined_until, active_conns, rate_bps, capacity_bps, saturated }`.
//! - Quarantine after 3 consecutive failures for 30 s (constants
//!   [`QUARANTINE_THRESHOLD`] / [`QUARANTINE_DURATION`], overridable per
//!   aggregator via [`QuarantinePolicy`]).
//! - `pick()` is a pure, unit-testable function.
//! - A per-link background prober task with a configurable interval.
//! - S11: the data plane feeds per-link byte counters + connection
//!   open/close events into the health table once per tick
//!   ([`Aggregator::tick_loads`]); `Balanced` picks by live byte load and
//!   `Priority` picks by user order with a saturation gate.
//!
//! IMPLEMENTATION NOTES (S6-B decisions, "pick one and document it"):
//! - ARCHITECTURE — shared state, not an actor. Healths live in one
//!   `Arc<tokio::sync::Mutex<Vec<LinkHealth>>>`; critical sections never
//!   await while the guard is held. Rationale: `pick()` stays a pure
//!   function over `&mut [LinkHealth]` exactly as the contract requires; an
//!   actor would add message plumbing for negligible benefit at
//!   probe-interval contention levels. tokio's `Mutex` is poison-free by
//!   construction (contract style rule); the round-robin counter uses a
//!   tiny `std::sync::Mutex<u64>` with the poison-tolerant unlock pattern
//!   used by the root crate.
//! - `pick` injects `now: Instant` so quarantine expiry is testable without
//!   sleeping (a deliberate, reported deviation from the literal
//!   `pick(&mut healths, policy, counter)` signature — the extra parameter
//!   exists to keep the function pure).
//! - `pick` lazily EXPIRES quarantines in place (`quarantined_until` in the
//!   past → cleared) — that is why it takes `&mut healths`. A FULLY SERVED
//!   quarantine re-admits the link OPTIMISTICALLY (`ok=true`, failure
//!   streak reset): the quota of failures was paid, and "recovers" must
//!   mean the link re-enters the normal candidate pool, not a
//!   permanently-demoted tier — the next prober tick corrects reality if
//!   the link is still dead (it re-quarantines after a fresh 3-failure
//!   streak).
//! - Candidate TIERS: (1) `ok && not quarantined` preferred; (2) fallback
//!   to `!ok but not quarantined` — a probe false-negative must not
//!   hard-block traffic, and the server's transparent retry catches real
//!   failures. S21: quarantine is NO LONGER a hard kill — when every
//!   non-excluded link is quarantined, the picker falls back to them as a
//!   LAST RESORT (a probe must never blackhole the data plane; the dial
//!   itself decides, fail-open at the server). No candidates at all →
//!   `None` (the connection errors, the server keeps serving).
//! - EWMA: the first successful sample seeds it; later samples blend with
//!   weight 0.3 new / 0.7 old. Failures clear `latency` (the sample went
//!   stale) but keep `ewma_ms` for LowestLatency ranking.
//! - Initial health is OPTIMISTIC (`ok=true`, no sample): connections work
//!   immediately at startup and the first prober tick corrects reality; a
//!   pessimistic init would black-hole traffic until the first probe.
//! - The prober is one spawned task per link (first tick immediate) probing
//!   a configurable target through the REAL outbound path
//!   (`Outbound::probe`) under a 5 s budget ([`PROBE_TIMEOUT`]). Probe
//!   results AND data-plane connect outcomes feed the same
//!   consecutive-failure counter (server.rs documents the tradeoff).
//! - LowestLatency ranking: links with a known sample rank by `ewma_ms`
//!   (ties → lowest index); unknown-sample links rank last (but ahead of
//!   nothing) — among unknowns the lowest index wins, so an all-unknown
//!   table degrades gracefully to failover order.
//! - S11 `Balanced`: pick the candidate with the smallest PRESSURE =
//!   `rate_bps + active_conns × CONN_RESERVE_BPS`. The reservation makes
//!   a burst of connections inside one 1 s tick spread instead of
//!   stacking on the one link whose byte counters have not ticked yet;
//!   ties rotate via the shared cursor (so an idle table behaves exactly
//!   like round-robin).
//! - S11 `Priority`: candidates are walked in LINK ORDER (which IS the
//!   user's adapter priority order — links are built one per bonded
//!   adapter in the settings-file order). The first candidate that is NOT
//!   saturated wins; when every candidate is saturated, the one with the
//!   smallest RELATIVE load (`rate_bps / capacity_bps`, unknown capacity
//!   → 0 = most headroom) takes the excess. Saturation needs a KNOWN
//!   capacity (≥ [`CAPACITY_FLOOR_BPS`]) so a fresh link is assumed to
//!   have headroom — the first fat flow ramps on the top-priority link,
//!   the capacity estimate converges on its plateau, and only then does
//!   the next link start taking traffic (the "fill first, spill when
//!   saturated" semantics the users asked for).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, watch, Mutex as AsyncMutex};

use crate::shutdown::wait_for_shutdown;
use crate::{LinkHealth, MergePolicy, Outbound, ProxyEvent, TargetAddr};

/// Consecutive failures before quarantine (contract value).
pub const QUARANTINE_THRESHOLD: u32 = 3;
/// Quarantine duration (contract value).
pub const QUARANTINE_DURATION: Duration = Duration::from_secs(30);
/// Weight of a NEW latency sample in the EWMA blend.
const EWMA_NEW_WEIGHT: f32 = 0.3;
/// Budget for one background probe.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// S11 load tracking (per-connection Balanced / Priority scheduling)
// ---------------------------------------------------------------------------

/// Weight of a NEW rate sample in the throughput EWMA blend.
pub const LOAD_EWMA_NEW_WEIGHT: f64 = 0.35;
/// Per-tick decay of the learned capacity when the current rate sits
/// below it (multiplicative; 0.995/s ≈ halves in ~2.3 min). Keeps a stale
/// spike from pinning "saturated" forever while staying flat enough that
/// a link sitting AT its capacity remains saturated.
pub const CAPACITY_DECAY_PER_TICK: f64 = 0.995;
/// A capacity estimate below this floor counts as UNKNOWN (the link is
/// assumed to have headroom). 256 KiB/s ≈ 2 Mbps — a link carrying only
/// background chatter should never count as saturated.
pub const CAPACITY_FLOOR_BPS: u64 = 256 * 1024;
/// A link counts as saturated when its rate reaches this fraction of its
/// learned capacity (0.85 leaves headroom for ramp jitter).
pub const SATURATION_FRACTION: f64 = 0.85;
/// A capacity rise above this fraction (2%/tick) counts as STILL RAMPING
/// and resets the plateau streak (a slower ramp is within the saturation
/// band's headroom anyway).
const CAPACITY_RISE_RESET: f64 = 1.02;
/// Consecutive non-rising ticks before the capacity counts as a PLATEAU
/// (the rate is pinned at the adapter's real bandwidth, not still
/// climbing toward it).
pub const PLATEAU_TICKS: u32 = 3;
/// Assumed pressure of one OPEN connection with no measured bytes yet —
/// lets bursts inside a single 1 s tick spread across links instead of
/// stacking on the one whose counters have not ticked (128 KiB/s ≈ 1 Mbps
/// of nominal expectation per connection).
pub const CONN_RESERVE_BPS: u64 = 128 * 1024;

/// Advance one link's load estimate by one tick (pure; unit-tested).
///
/// * `delta_bytes` — data-plane bytes observed on this link during the tick.
/// * `dt` — the tick's wall length (rate = delta/dt).
///
/// Updates `rate_bps` (EWMA), `capacity_bps` (rising instantly to the
/// observed rate, decaying by [`CAPACITY_DECAY_PER_TICK`] otherwise),
/// `plateau_ticks` (consecutive ticks without a ≥2% capacity rise) and
/// the `saturated` flag. A zero `dt` is treated as 1 s (defensive; the
/// caller is the 1 s server ticker).
///
/// Saturation is a PLATEAU, not a level: because the capacity estimate
/// rises to meet any new rate, "rate ≈ capacity" is true during a ramp
/// too — so saturation additionally requires the capacity to have
/// STOPPED rising for [`PLATEAU_TICKS`] ticks. A link still climbing is
/// never saturated; a link pinned at its ceiling is.
pub fn update_load(h: &mut LinkHealth, delta_bytes: u64, dt: Duration) {
    let secs = dt.as_secs_f64();
    let secs = if secs > 0.0 { secs } else { 1.0 };
    let sample = delta_bytes as f64 / secs;
    // EWMA of the rate (first sample seeds it).
    let rate = if h.rate_bps == 0 {
        sample
    } else {
        h.rate_bps as f64 * (1.0 - LOAD_EWMA_NEW_WEIGHT) + sample * LOAD_EWMA_NEW_WEIGHT
    };
    h.rate_bps = rate.max(0.0) as u64;
    // Capacity: rises instantly, decays slowly toward the current rate.
    let cap_old = h.capacity_bps as f64;
    let cap_new = (cap_old * CAPACITY_DECAY_PER_TICK).max(h.rate_bps as f64);
    h.capacity_bps = cap_new as u64;
    // Plateau streak: a ≥2% rise means the link is still ramping.
    if cap_new > cap_old * CAPACITY_RISE_RESET {
        h.plateau_ticks = 0;
    } else {
        h.plateau_ticks = h.plateau_ticks.saturating_add(1);
    }
    // Saturation needs a KNOWN capacity, a rate at the ceiling and a
    // capacity that has stopped rising.
    h.saturated = h.capacity_bps >= CAPACITY_FLOOR_BPS
        && h.plateau_ticks >= PLATEAU_TICKS
        && h.rate_bps as f64 >= h.capacity_bps as f64 * SATURATION_FRACTION;
}

/// Balanced-policy pressure of one link (pure).
pub fn pressure_of(h: &LinkHealth) -> u64 {
    h.rate_bps
        .saturating_add(h.active_conns as u64 * CONN_RESERVE_BPS)
}

/// Priority-policy relative load of one link (pure; 0.0 = most headroom).
pub fn relative_load_of(h: &LinkHealth) -> f64 {
    if h.capacity_bps < CAPACITY_FLOOR_BPS {
        0.0
    } else {
        h.rate_bps as f64 / h.capacity_bps as f64
    }
}

/// Quarantine knobs; `Default` = contract values (3 failures / 30 s).
#[derive(Debug, Clone, Copy)]
pub struct QuarantinePolicy {
    /// Consecutive failures that trigger quarantine.
    pub threshold: u32,
    /// How long a quarantined link is excluded from picking.
    pub duration: Duration,
}

impl Default for QuarantinePolicy {
    fn default() -> Self {
        Self {
            threshold: QUARANTINE_THRESHOLD,
            duration: QUARANTINE_DURATION,
        }
    }
}

/// Pick a link index from the health table (pure; see module docs for the
/// tiering/quarantine/EWMA rules). `counter` is the round-robin cursor and
/// is advanced only by `RoundRobin` picks. `now` drives quarantine expiry.
pub fn pick(
    healths: &mut [LinkHealth],
    policy: MergePolicy,
    counter: &mut u64,
    now: Instant,
) -> Option<usize> {
    pick_excluding(healths, policy, counter, now, None)
}

/// [`pick`] with one index excluded from the candidate set (the server's
/// transparent retry uses this to guarantee a DIFFERENT link).
pub(crate) fn pick_excluding(
    healths: &mut [LinkHealth],
    policy: MergePolicy,
    counter: &mut u64,
    now: Instant,
    excluded: Option<usize>,
) -> Option<usize> {
    // Lazily expire quarantines in place (why pick takes &mut healths).
    // Expiry is a full recovery: the failure streak is closed and the link
    // is re-admitted optimistically (see module docs).
    for h in healths.iter_mut() {
        if let Some(until) = h.quarantined_until {
            if until <= now {
                h.quarantined_until = None;
                h.ok = true;
                h.consecutive_failures = 0;
            }
        }
    }
    let mut tier1 = Vec::new();
    let mut tier2 = Vec::new();
    for (i, h) in healths.iter().enumerate() {
        if excluded == Some(i) || h.quarantined_until.is_some() {
            continue;
        }
        if h.ok {
            tier1.push(i);
        } else {
            tier2.push(i);
        }
    }
    // S21 P1 stability: a quarantine must never BLACKHOLE every link.
    // In stream-bonding mode the bond is the ONLY link — when a loaded
    // probe crosses PROBE_TIMEOUT three times, the old contract returned
    // None and every new connection was refused while the tunnel looked
    // Connected (the user-visible "less stable than v2rayN" blackout).
    // Quarantine is a load-balancing signal, not a kill switch: with no
    // healthy candidate, prefer a quarantined / down link over certain
    // failure — a probe false negative now just works, and a truly dead
    // link still fails the dial with the usual error (fail-open at the
    // data plane, per the module contract). The `excluded` guarantee
    // (retry lands on a DIFFERENT link) is preserved.
    let last_resort: Vec<usize> = healths
        .iter()
        .enumerate()
        .filter(|&(i, _)| excluded != Some(i))
        .map(|(i, _)| i)
        .collect();
    let candidates = if !tier1.is_empty() {
        &tier1
    } else if !tier2.is_empty() {
        &tier2
    } else {
        &last_resort
    };
    let &first = candidates.first()?;
    match policy {
        MergePolicy::Failover => Some(first),
        MergePolicy::RoundRobin => {
            let idx = candidates[(*counter % candidates.len() as u64) as usize];
            *counter = counter.wrapping_add(1);
            Some(idx)
        }
        MergePolicy::LowestLatency => {
            let mut best = first;
            let mut best_known = healths[best].latency.is_some();
            let mut best_ewma = if best_known {
                healths[best].ewma_ms
            } else {
                f32::INFINITY
            };
            for &i in candidates.iter().skip(1) {
                let h = &healths[i];
                let known = h.latency.is_some();
                let ewma = if known { h.ewma_ms } else { f32::INFINITY };
                // known beats unknown; equal class → strictly lower ewma
                // wins (ties keep the earlier index).
                if (known && !best_known) || (known == best_known && ewma < best_ewma) {
                    best = i;
                    best_known = known;
                    best_ewma = ewma;
                }
            }
            Some(best)
        }
        // S11: least live byte pressure; ties rotate (idle table ==
        // round-robin; a burst spreads via the per-connection reserve).
        MergePolicy::Balanced => {
            let min = candidates
                .iter()
                .map(|&i| pressure_of(&healths[i]))
                .min()
                .expect("candidates is non-empty");
            let tied: Vec<usize> = candidates
                .iter()
                .copied()
                .filter(|&i| pressure_of(&healths[i]) == min)
                .collect();
            let idx = tied[(*counter % tied.len() as u64) as usize];
            *counter = counter.wrapping_add(1);
            Some(idx)
        }
        // S11: user priority order (link order); first unsaturated wins,
        // all saturated → smallest relative load takes the excess.
        MergePolicy::Priority => {
            for &i in candidates {
                if !healths[i].saturated {
                    return Some(i);
                }
            }
            let mut best = first;
            let mut best_rel = relative_load_of(&healths[best]);
            for &i in candidates.iter().skip(1) {
                let rel = relative_load_of(&healths[i]);
                if rel < best_rel {
                    best = i;
                    best_rel = rel;
                }
            }
            Some(best)
        }
    }
}

/// Poison-tolerant std lock (repo rule; generic over the guarded type).
fn lock_std<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Poison-tolerant lock for the tiny round-robin counter (root-crate style).
fn lock_counter(m: &Mutex<u64>) -> MutexGuard<'_, u64> {
    lock_std(m)
}

/// Optimistic initial health (see module docs).
fn initial_health() -> LinkHealth {
    LinkHealth {
        ok: true,
        latency: None,
        ewma_ms: 0.0,
        consecutive_failures: 0,
        quarantined_until: None,
        active_conns: 0,
        rate_bps: 0,
        capacity_bps: 0,
        plateau_ticks: 0,
        saturated: false,
    }
}

/// Shared-state aggregator over the link table: health recording, policy
/// picks, snapshots and (S11) live LOAD tracking. All mutable state is
/// behind the async health mutex (poison-free) and the tiny counter mutex
/// (poison-tolerant); the per-link byte counters are lock-free atomics the
/// data-plane pumps bump directly.
pub struct Aggregator {
    links: Vec<Arc<dyn Outbound>>,
    healths: Arc<AsyncMutex<Vec<LinkHealth>>>,
    counter: Mutex<u64>,
    quarantine: QuarantinePolicy,
    /// S11: cumulative data-plane bytes per link (up+down), bumped by the
    /// tunnel pumps; `tick_loads` derives the per-tick deltas.
    link_bytes: Vec<Arc<AtomicU64>>,
    /// S11: last cumulative reading per link (the delta base).
    link_bytes_last: Mutex<Vec<u64>>,
}

impl Aggregator {
    /// New aggregator with optimistic initial health for every link.
    pub fn new(links: Vec<Arc<dyn Outbound>>) -> Self {
        let healths = links.iter().map(|_| initial_health()).collect();
        let n = links.len();
        Self {
            links,
            healths: Arc::new(AsyncMutex::new(healths)),
            counter: Mutex::new(0),
            quarantine: QuarantinePolicy::default(),
            link_bytes: (0..n).map(|_| Arc::new(AtomicU64::new(0))).collect(),
            link_bytes_last: Mutex::new(vec![0; n]),
        }
    }

    /// Override the contract-default quarantine policy (tests / tuning).
    pub fn with_quarantine_policy(mut self, quarantine: QuarantinePolicy) -> Self {
        self.quarantine = quarantine;
        self
    }

    /// Number of links.
    pub fn len(&self) -> usize {
        self.links.len()
    }

    /// True when there are no links at all.
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Link at `idx`, if any.
    pub fn link(&self, idx: usize) -> Option<Arc<dyn Outbound>> {
        self.links.get(idx).cloned()
    }

    /// S11: the cumulative byte counter of link `idx` (up+down). The
    /// tunnel pumps `fetch_add` on it as bytes flow; [`Self::tick_loads`]
    /// derives the deltas. `None` when `idx` is out of range.
    pub fn link_bytes(&self, idx: usize) -> Option<Arc<AtomicU64>> {
        self.link_bytes.get(idx).cloned()
    }

    /// S11: one live connection was placed on link `idx` (establish
    /// success). Feeds the Balanced pressure and the GUI status.
    pub async fn record_conn_open(&self, idx: usize) {
        let mut hs = self.healths.lock().await;
        if let Some(h) = hs.get_mut(idx) {
            h.active_conns = h.active_conns.saturating_add(1);
        }
    }

    /// S11: one connection on link `idx` ended (tunnel teardown).
    pub async fn record_conn_close(&self, idx: usize) {
        let mut hs = self.healths.lock().await;
        if let Some(h) = hs.get_mut(idx) {
            h.active_conns = h.active_conns.saturating_sub(1);
        }
    }

    /// S11: advance every link's load estimate by one tick — reads the
    /// cumulative per-link counters, derives deltas against the last
    /// reading and runs [`update_load`] for each link. Called by the
    /// server's 1 s ticker.
    pub async fn tick_loads(&self, dt: Duration) {
        // Collect-then-lock (repo rule: no guard across an await — the
        // collect is sync, the update takes the async lock once).
        let deltas: Vec<u64> = {
            let mut last = lock_std(&self.link_bytes_last);
            self.link_bytes
                .iter()
                .zip(last.iter_mut())
                .map(|(c, l)| {
                    let cur = c.load(Ordering::Relaxed);
                    let d = cur.saturating_sub(*l);
                    *l = cur;
                    d
                })
                .collect()
        };
        let mut hs = self.healths.lock().await;
        for (h, d) in hs.iter_mut().zip(deltas) {
            update_load(h, d, dt);
        }
    }

    /// `(name, health)` snapshot of the whole table.
    pub async fn snapshot(&self) -> Vec<(String, LinkHealth)> {
        let hs = self.healths.lock().await;
        self.links
            .iter()
            .zip(hs.iter())
            .map(|(l, h)| (l.name().to_string(), h.clone()))
            .collect()
    }

    /// Record one outcome for link `idx` (probe result or data-plane
    /// connect result). Success resets the failure streak (and optionally
    /// blends a latency sample into the EWMA); failure extends it and
    /// quarantines at the configured threshold. Returns the updated entry
    /// for event emission, or `None` when `idx` is out of range.
    pub async fn record_result(
        &self,
        idx: usize,
        ok: bool,
        latency: Option<Duration>,
    ) -> Option<LinkHealth> {
        let mut hs = self.healths.lock().await;
        let h = hs.get_mut(idx)?;
        if ok {
            h.ok = true;
            h.consecutive_failures = 0;
            h.quarantined_until = None;
            if let Some(d) = latency {
                let ms = d.as_secs_f32() * 1000.0;
                h.ewma_ms = if h.latency.is_some() {
                    h.ewma_ms * (1.0 - EWMA_NEW_WEIGHT) + ms * EWMA_NEW_WEIGHT
                } else {
                    ms
                };
                h.latency = Some(d);
            }
        } else {
            h.ok = false;
            h.latency = None;
            h.consecutive_failures = h.consecutive_failures.saturating_add(1);
            if h.consecutive_failures >= self.quarantine.threshold {
                h.quarantined_until = Some(Instant::now() + self.quarantine.duration);
            }
        }
        Some(h.clone())
    }

    /// Pick a link under `policy`, optionally excluding one index (the
    /// transparent retry). Returns `(idx, link)` or `None` when nothing is
    /// eligible (fail-open: the caller errors the connection).
    pub async fn pick_link(
        &self,
        policy: MergePolicy,
        excluded: Option<usize>,
    ) -> Option<(usize, Arc<dyn Outbound>)> {
        let now = Instant::now();
        let mut hs = self.healths.lock().await;
        let mut counter = lock_counter(&self.counter);
        let idx = pick_excluding(&mut hs, policy, &mut counter, now, excluded)?;
        let link = self.links.get(idx)?.clone();
        Some((idx, link))
    }
}

/// Run one probe against `link` under `budget`; returns `(ok, latency)`.
pub(crate) async fn probe_once(
    link: &dyn Outbound,
    target: &TargetAddr,
    budget: Duration,
) -> (bool, Option<Duration>) {
    match tokio::time::timeout(budget, link.probe(target)).await {
        Ok(Ok(d)) => (true, Some(d)),
        Ok(Err(_)) | Err(_) => (false, None),
    }
}

/// Per-link background prober loop (one spawned task per link; first tick
/// immediate). Emits a `LinkHealth` event after every result.
pub(crate) async fn prober_loop(
    aggregator: Arc<Aggregator>,
    idx: usize,
    target: TargetAddr,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
    events: broadcast::Sender<ProxyEvent>,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = wait_for_shutdown(&mut shutdown) => break,
        }
        let Some(link) = aggregator.link(idx) else {
            break;
        };
        let (ok, latency) = probe_once(link.as_ref(), &target, PROBE_TIMEOUT).await;
        if let Some(h) = aggregator.record_result(idx, ok, latency).await {
            tracing::debug!(link = link.name(), ok, "probe result");
            let _ = events.send(ProxyEvent::LinkHealth {
                link: link.name().to_string(),
                health: h,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support::FakeOutbound, Host};

    fn health(ok: bool, ewma_ms: f32, quarantine: Option<Duration>) -> LinkHealth {
        LinkHealth {
            ok,
            latency: (ewma_ms > 0.0).then(|| Duration::from_millis(ewma_ms as u64)),
            ewma_ms,
            consecutive_failures: 0,
            quarantined_until: quarantine.map(|d| Instant::now() + d),
            active_conns: 0,
            rate_bps: 0,
            capacity_bps: 0,
            plateau_ticks: 0,
            saturated: false,
        }
    }

    /// A link with explicit S11 load state.
    fn loaded(
        ok: bool,
        rate_bps: u64,
        capacity_bps: u64,
        active_conns: u32,
        saturated: bool,
    ) -> LinkHealth {
        LinkHealth {
            ok,
            latency: Some(Duration::from_millis(20)),
            ewma_ms: 20.0,
            consecutive_failures: 0,
            quarantined_until: None,
            active_conns,
            rate_bps,
            capacity_bps,
            plateau_ticks: if saturated { PLATEAU_TICKS } else { 0 },
            saturated,
        }
    }

    fn pick1(
        policy: MergePolicy,
        healths: &mut [LinkHealth],
        excluded: Option<usize>,
    ) -> Option<usize> {
        let mut counter = 0u64;
        pick_excluding(healths, policy, &mut counter, Instant::now(), excluded)
    }

    #[test]
    fn failover_prefers_healthy_then_falls_back_to_live() {
        let mut hs = vec![health(false, 0.0, None), health(true, 0.0, None)];
        assert_eq!(pick1(MergePolicy::Failover, &mut hs, None), Some(1));
        // No healthy link: fall back to live-but-unhealthy (probe false
        // negatives must not hard-block; the retry catches real failures).
        let mut all_down = vec![health(false, 0.0, None), health(false, 0.0, None)];
        assert_eq!(pick1(MergePolicy::Failover, &mut all_down, None), Some(0));
        assert_eq!(pick1(MergePolicy::Failover, &mut Vec::new(), None), None);
    }

    #[test]
    fn failover_skips_quarantined_links() {
        let mut hs = vec![
            health(true, 0.0, Some(Duration::from_secs(10))),
            health(true, 0.0, None),
        ];
        assert_eq!(pick1(MergePolicy::Failover, &mut hs, None), Some(1));
        let mut all_q = vec![
            health(true, 0.0, Some(Duration::from_secs(10))),
            health(true, 0.0, Some(Duration::from_secs(10))),
        ];
        // S21 P1 stability: an all-quarantined table now falls back to the
        // quarantined links (last resort) instead of returning None — a
        // probe must never blackhole the data plane. Failover takes the
        // first non-excluded link in order.
        assert_eq!(pick1(MergePolicy::Failover, &mut all_q, None), Some(0));
    }

    #[test]
    fn quarantined_sole_link_is_a_last_resort_never_a_blackout() {
        // The stream-bonding shape: ONE link (the bond). Three slow probes
        // quarantine it; picking it must STILL happen (S21) — the dial
        // decides, not the probe streak.
        let mut single = vec![health(true, 0.0, Some(Duration::from_secs(10)))];
        assert_eq!(pick1(MergePolicy::Failover, &mut single, None), Some(0));
        // The retry-exclusion contract still holds: excluded → no OTHER
        // link exists → None.
        assert_eq!(pick1(MergePolicy::Failover, &mut single, Some(0)), None);
    }

    #[test]
    fn failover_retry_excludes_the_failed_link() {
        let mut hs = vec![health(true, 0.0, None), health(true, 0.0, None)];
        assert_eq!(
            pick1(MergePolicy::Failover, &mut hs, Some(0)),
            Some(1),
            "transparent retry must land on a DIFFERENT link"
        );
        let mut single = vec![health(true, 0.0, None)];
        assert_eq!(pick1(MergePolicy::Failover, &mut single, Some(0)), None);
    }

    #[test]
    fn round_robin_cycles_over_healthy_links() {
        let mut hs = vec![
            health(false, 0.0, None),
            health(true, 0.0, None),
            health(true, 0.0, None),
            health(true, 0.0, None),
        ];
        let mut counter = 0u64;
        let now = Instant::now();
        // Tier-1 candidates are 1, 2, 3 — cycling in order.
        let seq: Vec<usize> = (0..4)
            .map(|_| {
                pick_excluding(&mut hs, MergePolicy::RoundRobin, &mut counter, now, None).unwrap()
            })
            .collect();
        assert_eq!(seq, vec![1, 2, 3, 1]);
        assert_eq!(counter, 4, "counter advances once per round-robin pick");
    }

    #[test]
    fn lowest_latency_prefers_known_fast_and_ranks_unknown_last() {
        // Fastest known wins.
        let mut hs = vec![
            health(true, 100.0, None),
            health(true, 20.0, None),
            health(true, 0.0, None),
        ];
        assert_eq!(pick1(MergePolicy::LowestLatency, &mut hs, None), Some(1));
        // A known sample beats unknown (even with a worse ewma number).
        let mut mixed = vec![health(true, 0.0, None), health(true, 500.0, None)];
        assert_eq!(pick1(MergePolicy::LowestLatency, &mut mixed, None), Some(1));
        // All unknown → lowest index (failover-like degradation).
        let mut unknowns = vec![health(true, 0.0, None), health(true, 0.0, None)];
        assert_eq!(
            pick1(MergePolicy::LowestLatency, &mut unknowns, None),
            Some(0)
        );
        // Tie on ewma → earlier index.
        let mut tie = vec![health(true, 50.0, None), health(true, 50.0, None)];
        assert_eq!(pick1(MergePolicy::LowestLatency, &mut tie, None), Some(0));
    }

    #[test]
    fn pick_expires_expired_quarantines_in_place() {
        let past = Instant::now() - Duration::from_millis(1);
        let mut hs = vec![LinkHealth {
            ok: true,
            latency: None,
            ewma_ms: 0.0,
            consecutive_failures: 3,
            quarantined_until: Some(past),
            active_conns: 0,
            rate_bps: 0,
            capacity_bps: 0,
            plateau_ticks: 0,
            saturated: false,
        }];
        assert_eq!(pick1(MergePolicy::Failover, &mut hs, None), Some(0));
        assert_eq!(hs[0].quarantined_until, None, "expired quarantine cleared");
    }

    // -----------------------------------------------------------------
    // S11: Balanced / Priority / update_load
    // -----------------------------------------------------------------

    #[test]
    fn balanced_picks_the_least_loaded_link() {
        // Link 0 carries a fat flow, link 1 is idle, link 2 has some
        // traffic — the exact "one adapter hogs everything" shape.
        let mut hs = vec![
            loaded(true, 8_000_000, 8_000_000, 1, false),
            loaded(true, 0, 0, 0, false),
            loaded(true, 2_000_000, 4_000_000, 1, false),
        ];
        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(1));
    }

    #[test]
    fn balanced_counts_the_connection_reserve_for_bursts() {
        // All rates zero (counters have not ticked yet): two links each
        // hold one open connection, one holds none — the empty one wins
        // WITHOUT waiting for a byte tick.
        let mut hs = vec![
            loaded(true, 0, 0, 2, false),
            loaded(true, 0, 0, 0, false),
            loaded(true, 0, 0, 1, false),
        ];
        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(1));
    }

    #[test]
    fn balanced_ties_rotate_like_round_robin() {
        // Three equally idle links: the cursor cycles (an idle table
        // behaves exactly like round-robin).
        let mut hs = vec![
            loaded(true, 0, 0, 0, false),
            loaded(true, 0, 0, 0, false),
            loaded(true, 0, 0, 0, false),
        ];
        let mut counter = 0u64;
        let now = Instant::now();
        let seq: Vec<usize> = (0..3)
            .map(|_| {
                pick_excluding(&mut hs, MergePolicy::Balanced, &mut counter, now, None).unwrap()
            })
            .collect();
        assert_eq!(seq, vec![0, 1, 2]);
        assert_eq!(counter, 3);
    }

    #[test]
    fn balanced_skips_quarantined_and_down_links() {
        let mut hs = vec![
            loaded(false, 0, 0, 0, false), // down → tier 2
            loaded(true, 9_000_000, 9_000_000, 0, false),
            loaded(true, 0, 0, 0, false),
        ];
        // Tier-1 candidates are 1 (loaded) and 2 (idle): 2 wins.
        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(2));
        // Quarantine is a hard gate for every policy.
        hs[2].quarantined_until = Some(Instant::now() + Duration::from_secs(10));
        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(1));
    }

    #[test]
    fn priority_fills_the_first_unsaturated_link_in_order() {
        // Link 0 saturated at its learned capacity → spill to link 1;
        // link 1 unsaturated → wins despite link 2 being idle.
        let mut hs = vec![
            loaded(true, 8_000_000, 8_000_000, 3, true),
            loaded(true, 1_000_000, 6_000_000, 1, false),
            loaded(true, 0, 0, 0, false),
        ];
        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(1));
        // Link 0 unsaturated → everything goes to the top priority.
        hs[0] = loaded(true, 1_000_000, 8_000_000, 1, false);
        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(0));
    }

    #[test]
    fn priority_all_saturated_spills_to_least_relative_load() {
        // 0 at 95% of capacity, 1 at 60%, 2 unknown (0%) — the unknown
        // link has the most headroom and wins.
        let mut hs = vec![
            loaded(true, 9_500_000, 10_000_000, 2, true),
            loaded(true, 6_000_000, 10_000_000, 1, true),
            loaded(true, 500_000, 100_000, 1, true), // capacity < floor
        ];
        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(2));
        // Known capacities only: the smaller ratio wins.
        hs[2] = loaded(true, 4_000_000, 10_000_000, 0, true);
        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(2));
    }

    #[test]
    fn priority_honors_quarantine_and_exclusion() {
        let mut hs = vec![loaded(true, 0, 0, 0, false), loaded(true, 0, 0, 0, false)];
        hs[0].quarantined_until = Some(Instant::now() + Duration::from_secs(10));
        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(1));
        // Transparent retry must land on a DIFFERENT link.
        let mut hs2 = vec![loaded(true, 0, 0, 0, false), loaded(true, 0, 0, 0, false)];
        assert_eq!(pick1(MergePolicy::Priority, &mut hs2, Some(0)), Some(1));
    }

    #[test]
    fn update_load_ramps_then_saturates_then_recovers() {
        let mut h = initial_health();
        let s = Duration::from_secs(1);
        // RAMP: 10% growth per tick — the capacity (tracking the rate
        // EWMA) keeps rising by more than the 2% ramp threshold, so a
        // climbing link is never marked saturated.
        let mut rate = 4_000_000u64;
        for _ in 0..10 {
            update_load(&mut h, rate, s);
            assert!(!h.saturated, "still ramping (rate {rate})");
            rate = rate * 110 / 100;
        }
        // PLATEAU: the sample pins. The EWMA first converges up to it
        // (still "ramping"), then the capacity stops rising and the link
        // reads saturated — and STAYS saturated while pinned.
        let mut saw_saturated = false;
        for _ in 0..20 {
            update_load(&mut h, rate, s);
            if saw_saturated {
                assert!(h.saturated, "pinned rate stays saturated");
            } else {
                saw_saturated = h.saturated;
            }
        }
        assert!(saw_saturated, "a pinned rate must saturate within 20 ticks");
        assert!(h.capacity_bps >= CAPACITY_FLOOR_BPS);
        // RECOVERY: demand halves — the rate falls out of the saturation
        // band long before the capacity decays.
        update_load(&mut h, rate / 2, s);
        update_load(&mut h, rate / 2, s);
        assert!(!h.saturated, "load below the band → unsaturated");
    }

    #[test]
    fn update_load_ignores_tiny_flows_and_zero_dt() {
        let mut h = initial_health();
        // Background chatter stays under the floor: never saturated.
        for _ in 0..30 {
            update_load(&mut h, 50_000, Duration::from_secs(1));
        }
        assert!(!h.saturated);
        assert!(h.capacity_bps < CAPACITY_FLOOR_BPS);
        // Zero dt is defensive, not a panic (fresh health → seeds).
        let mut h2 = initial_health();
        update_load(&mut h2, 12345, Duration::ZERO);
        assert_eq!(h2.rate_bps, 12345);
    }

    #[tokio::test]
    async fn aggregator_tick_loads_and_conn_accounting() {
        let links: Vec<Arc<dyn Outbound>> = vec![
            Arc::new(FakeOutbound::connect_fail("a")),
            Arc::new(FakeOutbound::connect_fail("b")),
        ];
        let agg = Aggregator::new(links);
        // Two open connections on link 0, one on link 1.
        agg.record_conn_open(0).await;
        agg.record_conn_open(0).await;
        agg.record_conn_open(1).await;
        // Bytes flow only on link 0.
        agg.link_bytes(0)
            .unwrap()
            .fetch_add(5_000_000, Ordering::Relaxed);
        agg.tick_loads(Duration::from_secs(1)).await;
        let snap = agg.snapshot().await;
        assert_eq!(snap[0].1.active_conns, 2);
        assert_eq!(snap[1].1.active_conns, 1);
        assert_eq!(snap[0].1.rate_bps, 5_000_000);
        assert_eq!(snap[1].1.rate_bps, 0);
        // Balanced now avoids link 0 (5 MB/s + 2 reserves) for link 1
        // (1 reserve).
        let (idx, _) = agg.pick_link(MergePolicy::Balanced, None).await.unwrap();
        assert_eq!(idx, 1);
        // A second tick with no NEW bytes: the delta base moved, rate
        // decays but stays positive-ish; connections close → pressure
        // drops.
        agg.record_conn_close(0).await;
        agg.record_conn_close(0).await;
        agg.tick_loads(Duration::from_secs(1)).await;
        let snap = agg.snapshot().await;
        assert_eq!(snap[0].1.active_conns, 0);
        assert!(
            snap[0].1.rate_bps < 5_000_000,
            "EWMA decays with no new bytes"
        );
    }

    #[tokio::test]
    async fn aggregator_quarantines_after_three_failures_then_recovers() {
        let links: Vec<Arc<dyn Outbound>> = vec![
            Arc::new(FakeOutbound::connect_fail("link-a")),
            Arc::new(FakeOutbound::connect_fail("link-b")),
        ];
        let agg = Aggregator::new(links).with_quarantine_policy(QuarantinePolicy {
            threshold: 3,
            duration: Duration::from_millis(150),
        });
        // Optimistic init: failover picks link 0 first.
        let (idx, _) = agg.pick_link(MergePolicy::Failover, None).await.unwrap();
        assert_eq!(idx, 0);
        for _ in 0..3 {
            agg.record_result(0, false, None).await;
        }
        let snap = agg.snapshot().await;
        assert!(!snap[0].1.ok);
        assert_eq!(snap[0].1.consecutive_failures, 3);
        assert!(
            snap[0].1.quarantined_until.is_some(),
            "3 failures → quarantine"
        );
        // Quarantined link is a hard no — failover moves to the other link.
        let (idx, _) = agg.pick_link(MergePolicy::Failover, None).await.unwrap();
        assert_eq!(idx, 1);
        // After the (shortened) quarantine elapses the link is pickable and
        // the expired entry is cleared by the pick itself.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (idx, _) = agg.pick_link(MergePolicy::Failover, None).await.unwrap();
        assert_eq!(idx, 0, "expired quarantine admits the link again");
        let snap = agg.snapshot().await;
        assert!(snap[0].1.quarantined_until.is_none());
    }

    #[tokio::test]
    async fn record_success_resets_failures_and_blends_ewma() {
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::connect_fail("a"))];
        let agg = Aggregator::new(links);
        agg.record_result(0, true, Some(Duration::from_millis(100)))
            .await;
        agg.record_result(0, true, Some(Duration::from_millis(50)))
            .await;
        let snap = agg.snapshot().await;
        assert_eq!(snap[0].1.consecutive_failures, 0);
        assert!(snap[0].1.ok);
        let ewma = snap[0].1.ewma_ms;
        assert!(
            (ewma - 85.0).abs() < 1e-3,
            "0.7*100 + 0.3*50 = 85, got {ewma}"
        );
        assert_eq!(snap[0].1.latency, Some(Duration::from_millis(50)));
        // Two failures do not quarantine (below threshold)…
        agg.record_result(0, false, None).await;
        agg.record_result(0, false, None).await;
        let snap = agg.snapshot().await;
        assert_eq!(snap[0].1.consecutive_failures, 2);
        assert!(snap[0].1.quarantined_until.is_none());
        assert_eq!(snap[0].1.latency, None, "failure clears the stale sample");
        assert!(
            (snap[0].1.ewma_ms - 85.0).abs() < 1e-3,
            "ewma kept for ranking"
        );
        // …and one success resets the streak.
        agg.record_result(0, true, None).await;
        let snap = agg.snapshot().await;
        assert_eq!(snap[0].1.consecutive_failures, 0);
        assert!(snap[0].1.ok);
    }

    #[tokio::test]
    async fn round_robin_cursor_persists_across_picks() {
        let links: Vec<Arc<dyn Outbound>> = vec![
            Arc::new(FakeOutbound::connect_fail("a")),
            Arc::new(FakeOutbound::connect_fail("b")),
        ];
        let agg = Aggregator::new(links);
        let (i0, _) = agg.pick_link(MergePolicy::RoundRobin, None).await.unwrap();
        let (i1, _) = agg.pick_link(MergePolicy::RoundRobin, None).await.unwrap();
        let (i2, _) = agg.pick_link(MergePolicy::RoundRobin, None).await.unwrap();
        assert_eq!((i0, i1, i2), (0, 1, 0));
    }

    #[tokio::test]
    async fn prober_task_updates_health_and_emits_events() {
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::always_fail("bad"))];
        let agg = Arc::new(Aggregator::new(links));
        let (events, mut rx) = broadcast::channel(16);
        let (stx, srx) = watch::channel(false);
        tokio::spawn(prober_loop(
            Arc::clone(&agg),
            0,
            TargetAddr {
                host: Host::Domain("probe.synthetic".into()),
                port: 80,
            },
            Duration::from_millis(20),
            srx,
            events,
        ));
        // The prober flips the optimistic init to down within ~2 ticks.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(Instant::now() < deadline, "prober never marked link down");
            if !agg.snapshot().await.remove(0).1.ok {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        match rx.try_recv().expect("LinkHealth event emitted") {
            ProxyEvent::LinkHealth { link, health } => {
                assert_eq!(link, "bad");
                assert!(!health.ok);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        stx.send(true).unwrap(); // prober loop ends promptly
    }
}
