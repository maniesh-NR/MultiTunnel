//! Per-session inbound reordering (D17 §3).
//!
//! Carriers are TCP, so frames on ONE carrier arrive in order; frames of
//! one session are spread across ALL carriers, so a session's frame stream
//! arrives interleaved-out-of-order. [`ReorderBuffer`] restores per-session
//! order: duplicate/already-delivered seqs are dropped (idempotent replay
//! is a core protocol property), future seqs are buffered (bounded by
//! `cap` bytes — an overflow is a hard session error), and a contiguous
//! run starting at `expected` is returned as deliverable payloads.
//!
//! Pure data structure: no clocks, no I/O — the endpoint layer applies the
//! stall window (a gap with no delivery progress for
//! [`crate::STALL_TIMEOUT`] closes the session with `CloseReason::Stall`).

use std::collections::BTreeMap;

/// Insert outcome: payloads that became deliverable, in order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Delivered(pub Vec<Vec<u8>>);

impl Delivered {
    /// True when nothing became deliverable (duplicate / out-of-order).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Reorder error kinds (both are fatal for the session).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReorderError {
    /// More than `cap` bytes buffered past the expected seq.
    Overflow,
}

/// Per-session, per-direction reorder state.
#[derive(Debug)]
pub struct ReorderBuffer {
    /// Next seq to deliver (starts at 0: DATA seqs are zero-based).
    expected: u64,
    /// Buffered future frames, keyed by seq (BTreeMap → in-order scan).
    buf: BTreeMap<u64, Vec<u8>>,
    /// Total payload bytes currently buffered.
    buffered: usize,
    /// Maximum tolerated buffered bytes.
    cap: usize,
    /// Highest seq ever inserted (duplicate detection for stats/tests).
    highest_seen: u64,
}

impl ReorderBuffer {
    /// New buffer with a byte cap (the endpoint uses [`crate::REORDER_CAP`]).
    pub fn new(cap: usize) -> Self {
        Self {
            expected: 0,
            buf: BTreeMap::new(),
            buffered: 0,
            cap,
            highest_seen: 0,
        }
    }

    /// Next expected seq.
    pub fn expected(&self) -> u64 {
        self.expected
    }

    /// Bytes currently buffered past `expected`.
    pub fn buffered_bytes(&self) -> usize {
        self.buffered
    }

    /// True when a gap is open (frames are buffered but not deliverable).
    pub fn has_gap(&self) -> bool {
        !self.buf.is_empty()
    }

    /// Highest seq inserted so far (test/diagnostic).
    pub fn highest_seen(&self) -> u64 {
        self.highest_seen
    }

    /// True when buffering one more `len`-byte future frame would exceed
    /// the cap. The endpoint's S14 overflow path polls this while holding
    /// the frame (carrier backpressure) instead of failing the session.
    pub fn would_overflow(&self, len: usize) -> bool {
        self.buffered + len > self.cap
    }

    /// Insert one payload. Returns the in-order deliverable run starting
    /// at `expected` (possibly empty for out-of-order or duplicate input).
    /// `Err(Overflow)` when the buffered backlog would exceed the cap.
    pub fn insert(&mut self, seq: u64, payload: Vec<u8>) -> Result<Delivered, ReorderError> {
        if seq < self.expected {
            // Already delivered (idempotent replay) — drop.
            return Ok(Delivered::default());
        }
        self.highest_seen = self.highest_seen.max(seq);
        if seq == self.expected {
            // Fast path: in-order frame. Deliver it, then drain any
            // contiguous buffered run that follows.
            let mut out = vec![payload];
            self.expected += 1;
            while let Some(next) = self.buf.remove(&self.expected) {
                self.buffered = self.buffered.saturating_sub(next.len());
                out.push(next);
                self.expected += 1;
            }
            return Ok(Delivered(out));
        }
        // Future frame: buffer it (idempotent — a replayed duplicate that
        // is already buffered keeps the FIRST copy).
        if self.buf.contains_key(&seq) {
            return Ok(Delivered::default());
        }
        if self.buffered + payload.len() > self.cap {
            return Err(ReorderError::Overflow);
        }
        self.buffered += payload.len();
        self.buf.insert(seq, payload);
        Ok(Delivered::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(n: u64) -> Vec<u8> {
        n.to_le_bytes().to_vec()
    }

    #[test]
    fn in_order_delivery() {
        let mut rb = ReorderBuffer::new(1024);
        for seq in 0..5u64 {
            let d = rb.insert(seq, payload(seq)).expect("insert");
            assert_eq!(d.0, vec![payload(seq)], "immediate delivery");
        }
        assert_eq!(rb.expected(), 5);
        assert!(!rb.has_gap());
    }

    #[test]
    fn gap_buffers_then_flushes() {
        let mut rb = ReorderBuffer::new(1024);
        // Frame 1 arrives before frame 0.
        let d = rb.insert(1, payload(1)).expect("insert");
        assert!(d.is_empty(), "future frame not delivered");
        assert!(rb.has_gap());
        assert_eq!(rb.buffered_bytes(), 8);
        // Frame 0 closes the gap and flushes both in order.
        let d = rb.insert(0, payload(0)).expect("insert");
        assert_eq!(d.0, vec![payload(0), payload(1)]);
        assert_eq!(rb.expected(), 2);
        assert!(!rb.has_gap());
        assert_eq!(rb.buffered_bytes(), 0);
    }

    #[test]
    fn long_out_of_order_run_reorders_completely() {
        let mut rb = ReorderBuffer::new(64 * 1024);
        // 2..=5 buffered first…
        for seq in (2..=5u64).rev() {
            assert!(rb.insert(seq, payload(seq)).expect("insert").is_empty());
        }
        // …then 0, 1 flush the whole run in order.
        let d = rb.insert(0, payload(0)).expect("insert");
        assert_eq!(d.0, vec![payload(0)]);
        let d = rb.insert(1, payload(1)).expect("insert");
        assert_eq!(
            d.0,
            vec![payload(1), payload(2), payload(3), payload(4), payload(5)]
        );
        assert_eq!(rb.expected(), 6);
    }

    #[test]
    fn duplicates_and_replays_are_idempotent() {
        let mut rb = ReorderBuffer::new(1024);
        rb.insert(0, payload(0)).unwrap();
        rb.insert(1, payload(1)).unwrap();
        // Replay of already-delivered frames: dropped, no double delivery.
        assert!(rb.insert(0, payload(0)).expect("insert").is_empty());
        assert!(rb
            .insert(1, b"DIFFERENT-BYTES".to_vec())
            .expect("insert")
            .is_empty());
        // Duplicate of a BUFFERED frame keeps the first copy.
        rb.insert(3, payload(3)).unwrap();
        assert!(rb
            .insert(3, b"SECOND-COPY".to_vec())
            .expect("insert")
            .is_empty());
        let d = rb.insert(2, payload(2)).expect("insert");
        assert_eq!(d.0, vec![payload(2), payload(3)], "first copy wins");
    }

    #[test]
    fn overflow_is_an_error() {
        let mut rb = ReorderBuffer::new(16);
        assert!(rb.insert(5, vec![0u8; 10]).is_ok());
        assert_eq!(
            rb.insert(6, vec![0u8; 10]),
            Err(ReorderError::Overflow),
            "20 buffered bytes > cap 16"
        );
        // The failed insert must not corrupt state.
        assert_eq!(rb.buffered_bytes(), 10);
        let d = rb.insert(0, vec![0u8; 4]).expect("in-order ok");
        assert_eq!(d.0.len(), 1);
    }

    #[test]
    fn cap_zero_still_delivers_in_order() {
        // The cap only bounds BUFFERED (out-of-order) bytes; in-order
        // frames never buffer, so even cap = 0 must deliver them.
        let mut rb = ReorderBuffer::new(0);
        let d = rb.insert(0, payload(0)).expect("in-order never buffers");
        assert_eq!(d.0.len(), 1);
        assert!(rb.insert(1, payload(1)).is_ok(), "still in-order");
        // But any future frame exceeds cap 0.
        assert_eq!(rb.insert(9, payload(9)), Err(ReorderError::Overflow));
    }

    #[test]
    fn would_overflow_mirrors_the_insert_boundary() {
        // S14: the endpoint's backpressure path polls would_overflow BEFORE
        // inserting — it must agree exactly with insert's Overflow check.
        let mut rb = ReorderBuffer::new(16);
        assert!(rb.insert(5, vec![0u8; 10]).is_ok());
        assert!(!rb.would_overflow(6), "10 + 6 == cap 16 fits");
        assert!(rb.would_overflow(7), "10 + 7 > cap 16 overflows");
        // An insert that would_overflow rejects must be the same size that
        // insert() errors on — the pause loop's invariant.
        let probe = vec![0u8; 7];
        assert_eq!(rb.would_overflow(probe.len()), rb.insert(6, probe).is_err());
        assert_eq!(rb.buffered_bytes(), 10, "rejected insert left state intact");
        // Once the gap ahead of seq 5 drains (in-order 0..=4 flushes the
        // buffered 5), the previously-rejected frame fits — the backpressure
        // pause resolves instead of erroring.
        for seq in 0..5u64 {
            assert!(rb.insert(seq, vec![0u8; 4]).is_ok());
        }
        assert_eq!(rb.buffered_bytes(), 0, "seq 5 flushed by the 0..=4 run");
        assert!(!rb.would_overflow(7));
        assert!(rb.insert(6, vec![0u8; 7]).is_ok());
    }
}
