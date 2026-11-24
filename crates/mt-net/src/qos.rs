//! Per-destination fair-share bandwidth scheduler (S21, user ask o):
//! "a QoS toggle that can be turned on to prevent a single application
//! from dominating all the bandwidth".
//!
//! DESIGN (pick one and document it — the mt-net discipline):
//! - Every proxied connection is one FLOW; flows are grouped by their
//!   TARGET (`host:port`). A browser's many connections to one site form
//!   one group; a download manager hitting one CDN also forms one group —
//!   exactly the "one application hogs everything" shape the toggle is
//!   about.
//! - The 1 s ticker measures the tunnel's aggregate up+down rate and, when
//!   ≥2 groups are active AND the aggregate is above
//!   [`QOS_CONTENTION_FLOOR`], refills every group's token bucket with
//!   `aggregate / groups` bytes. A greedy group is then throttled at each
//!   read (its pump sleeps until the next refill) while everyone else
//!   keeps their full share. Below the floor — or with a single active
//!   group — buckets are UNLIMITED: one app alone never pays a
//!   throughput tax, and idle tunnels are never shaped.
//! - The pump acquires budget BEFORE each read (the read is capped by
//!   `PUMP_BUF`), so over-reserve is bounded by one buffer and the OS
//!   socket buffers; writes back-pressure naturally behind reads.
//! - Live toggle: the enabled flag is an AtomicBool the service flips on
//!   the settings toggle WITHOUT rebuilding the session (no reconnect —
//!   unlike the diagnostics toggle, nothing here needs a data-plane
//!   rebuild).
//!
//! NOT an app-level classifier: local flows arrive from 127.0.0.1 with
//! per-connection ephemeral ports, so per-PROCESS attribution is not
//! available at this layer; per-target grouping is the honest proxy-level
//! approximation (documented in the Settings copy).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Aggregate rate below which no shaping happens (bytes/s): shaping only
/// engages when the tunnel is actually loaded — a 1 Mbps tunnel cannot
/// "hog" anything.
pub const QOS_CONTENTION_FLOOR: u64 = 2 * 1024 * 1024;
/// How long a throttled pump sleeps between acquire attempts (the next
/// refill lands within one 1 s tick).
const QOS_RETRY: Duration = Duration::from_millis(20);
/// How long a group with no active flows stays in the table (ticks it was
/// last seen with a live flow).
const GROUP_STALE_TICKS: u32 = 3;

#[derive(Default)]
struct GroupState {
    /// Token budget for the CURRENT tick (bytes this group may still read).
    tokens: u64,
    /// Live flows in this group (the pump registers/unregisters).
    flows: usize,
    /// Ticks since the last refill where the group had no flows.
    idle_ticks: u32,
}

/// Fair-share scheduler shared by the proxy pumps (see module docs).
/// Cheap in the hot path: one AtomicBool load + a short mutex critical
/// section per ≤16 KiB read.
#[derive(Default)]
pub struct QosScheduler {
    enabled: AtomicBool,
    groups: Mutex<HashMap<String, GroupState>>,
}

impl QosScheduler {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            groups: Mutex::new(HashMap::new()),
        }
    }

    /// Whether shaping is currently ON.
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Live-toggle the scheduler (Settings toggle; no reconnect needed).
    /// Turning it OFF also unblocks every waiting pump (they re-check the
    /// flag each retry) and clears the group table.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
        if !on {
            if let Ok(mut g) = self.groups.lock() {
                g.clear();
            }
        }
    }

    /// A flow starts (tunnel entry).
    pub fn register(&self, group: &str) {
        if let Ok(mut g) = self.groups.lock() {
            g.entry(group.to_string()).or_default().flows += 1;
        }
    }

    /// A flow ends (tunnel exit).
    pub fn unregister(&self, group: &str) {
        if let Ok(mut g) = self.groups.lock() {
            if let Some(state) = g.get_mut(group) {
                state.flows = state.flows.saturating_sub(1);
            }
        }
    }

    /// 1 s tick: refill buckets from the measured aggregate rate. Called
    /// from the proxy ticker with the window's up+down bytes/s.
    pub fn tick(&self, up_bps: u64, down_bps: u64) {
        let aggregate = up_bps.saturating_add(down_bps);
        let Ok(mut g) = self.groups.lock() else {
            return;
        };
        g.retain(|_, s| {
            s.flows > 0 || {
                s.idle_ticks += 1;
                s.idle_ticks < GROUP_STALE_TICKS
            }
        });
        let active: usize = g.values().filter(|s| s.flows > 0).count();
        // Shaping engages ONLY under real contention: ≥2 groups racing for
        // an aggregate above the floor. Otherwise everything is unlimited.
        if active < 2 || aggregate < QOS_CONTENTION_FLOOR {
            for s in g.values_mut() {
                s.tokens = u64::MAX;
            }
            return;
        }
        let quantum = aggregate / active as u64;
        for s in g.values_mut() {
            s.tokens = quantum;
        }
    }

    /// The pump's read gate: wait until this group has budget for one
    /// buffer-sized read (or shaping is off / not contended). Blocks at
    /// most until the next 1 s refill.
    pub async fn acquire(&self, group: &str, budget: u64) {
        loop {
            if !self.enabled() {
                return;
            }
            {
                let Ok(mut g) = self.groups.lock() else {
                    return; // poisoned lock: fail open, never stall traffic
                };
                let state = g.entry(group.to_string()).or_default();
                if state.tokens >= budget {
                    state.tokens -= budget;
                    return;
                }
            }
            tokio::time::sleep(QOS_RETRY).await;
        }
    }

    /// Test/observability hook: the tokens currently available to a group.
    #[cfg(test)]
    pub(crate) fn tokens_of(&self, group: &str) -> u64 {
        self.groups
            .lock()
            .map(|g| g.get(group).map(|s| s.tokens).unwrap_or(0))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn groups_active(s: &QosScheduler) -> usize {
        s.groups
            .lock()
            .unwrap()
            .values()
            .filter(|g| g.flows > 0)
            .count()
    }

    #[tokio::test]
    async fn single_group_is_never_shaped() {
        let s = QosScheduler::new(true);
        s.register("a.example:443");
        s.register("a.example:443");
        assert_eq!(groups_active(&s), 1);
        s.tick(100 * 1024 * 1024, 0); // 100 Mbps, 1 group
                                      // Unlimited tokens: big acquires never block.
        tokio::time::timeout(
            Duration::from_millis(100),
            s.acquire("a.example:443", 1024 * 1024),
        )
        .await
        .expect("unlimited group must not block");
    }

    #[tokio::test]
    async fn disabled_scheduler_never_blocks() {
        let s = QosScheduler::new(false);
        s.set_enabled(false);
        s.register("a:1");
        s.register("b:1");
        s.tick(1024 * 1024 * 1024, 0);
        tokio::time::timeout(Duration::from_millis(50), s.acquire("a:1", 1 << 30))
            .await
            .expect("disabled scheduler must not block");
    }

    #[tokio::test]
    async fn contended_groups_get_a_fair_quantum() {
        let s = QosScheduler::new(true);
        s.register("greedy:443");
        s.register("small:443");
        // 80 MiB/s aggregate, 2 groups → 40 MiB/s each per tick.
        let agg = 80 * 1024 * 1024;
        s.tick(agg, 0);
        let quantum = agg / 2;
        assert!(
            s.tokens_of("greedy:443") == quantum,
            "each contended group gets aggregate/groups"
        );
        // The greedy group exhausts its quantum, then must WAIT for the
        // next refill (bounded: below one tick + retry cadence).
        let started = std::time::Instant::now();
        // Acquire the full quantum in one 1 MiB step: the NEXT acquire of
        // another quantum-sized budget must block (not resolve instantly).
        tokio::time::timeout(Duration::from_millis(200), s.acquire("greedy:443", quantum))
            .await
            .expect("initial quantum is available");
        let blocked = tokio::time::timeout(Duration::from_millis(60), async {
            s.acquire("greedy:443", quantum).await;
        })
        .await;
        assert!(blocked.is_err(), "exhausted group must block (no budget)");
        // The small group keeps its own full quantum meanwhile.
        tokio::time::timeout(Duration::from_millis(50), s.acquire("small:443", quantum))
            .await
            .expect("the other group still has its share");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn idle_flow_groups_expire() {
        let s = QosScheduler::new(true);
        s.register("gone:80");
        s.unregister("gone:80");
        for _ in 0..GROUP_STALE_TICKS + 1 {
            s.tick(0, 0);
        }
        assert!(
            s.groups.lock().unwrap().is_empty(),
            "flow-less groups are garbage-collected"
        );
    }

    #[tokio::test]
    async fn set_enabled_false_unblocks_waiters() {
        let s = Arc::new(QosScheduler::new(true));
        s.register("a:1");
        s.register("b:1");
        s.tick(80 * 1024 * 1024, 0);
        let quantum = 40 * 1024 * 1024;
        tokio::time::timeout(Duration::from_millis(200), s.acquire("a:1", quantum))
            .await
            .expect("first quantum ok");
        let waiter = {
            let s2 = Arc::clone(&s);
            tokio::spawn(async move { s2.acquire("a:1", quantum).await })
        };
        tokio::time::sleep(Duration::from_millis(60)).await;
        s.set_enabled(false); // live toggle must unblock the pump
        tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("join")
            .expect("waiter resolves after disable");
    }
}
