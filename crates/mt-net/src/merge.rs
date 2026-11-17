//! Link aggregation (net-merge style): health table, merge policies,
//! quarantine and the per-link background prober.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"):
//! - `MergePolicy::{Failover, RoundRobin, LowestLatency}` (frozen in lib).
//! - `LinkHealth { ok, latency, ewma_ms, consecutive_failures,
//!   quarantined_until }` (frozen in lib).
//! - Quarantine after 3 consecutive failures for 30 s (constants
//!   [`QUARANTINE_THRESHOLD`] / [`QUARANTINE_DURATION`], overridable per
//!   aggregator via [`QuarantinePolicy`]).
//! - `pick()` is a pure, unit-testable function.
//! - A per-link background prober task with a configurable interval.
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
//!   failures; quarantine is the only HARD gate. No candidates → `None`
//!   (fail-open: the connection errors, the server keeps serving).
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
    let candidates = if tier1.is_empty() { &tier2 } else { &tier1 };
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
    }
}

/// Poison-tolerant lock for the tiny round-robin counter (root-crate style).
fn lock_counter(m: &Mutex<u64>) -> MutexGuard<'_, u64> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Optimistic initial health (see module docs).
fn initial_health() -> LinkHealth {
    LinkHealth {
        ok: true,
        latency: None,
        ewma_ms: 0.0,
        consecutive_failures: 0,
        quarantined_until: None,
    }
}

/// Shared-state aggregator over the link table: health recording, policy
/// picks and snapshots. All mutable state is behind the async health mutex
/// (poison-free) and the tiny counter mutex (poison-tolerant).
pub struct Aggregator {
    links: Vec<Arc<dyn Outbound>>,
    healths: Arc<AsyncMutex<Vec<LinkHealth>>>,
    counter: Mutex<u64>,
    quarantine: QuarantinePolicy,
}

impl Aggregator {
    /// New aggregator with optimistic initial health for every link.
    pub fn new(links: Vec<Arc<dyn Outbound>>) -> Self {
        let healths = links.iter().map(|_| initial_health()).collect();
        Self {
            links,
            healths: Arc::new(AsyncMutex::new(healths)),
            counter: Mutex::new(0),
            quarantine: QuarantinePolicy::default(),
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
        assert_eq!(pick1(MergePolicy::Failover, &mut all_q, None), None);
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
        }];
        assert_eq!(pick1(MergePolicy::Failover, &mut hs, None), Some(0));
        assert_eq!(hs[0].quarantined_until, None, "expired quarantine cleared");
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
