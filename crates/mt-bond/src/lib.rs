//! mt-bond — MultiTunnel stream bonding (D17).
//!
//! ONE logical connection, bytes split across the bonded adapters,
//! re-merged at the server; reverse symmetric — the net-merge promise at
//! stream granularity, riding N ordinary VLESS+REALITY tunnels:
//!
//! ```text
//! app ── SOCKS/HTTP ──▶ ProxyServer ──▶ BondOutbound ─┬─ carrier 0 (engine, Wi-Fi) ─▶ VLESS+REALITY ─▶ Xray ─┐
//!                                                     └─ carrier 1 (engine, LTE)  ─▶ VLESS+REALITY ─▶ Xray ─┤
//!                                                                                                              ▼
//!                                                                             mt-merge-server (127.0.0.1) — merge → target
//! ```
//!
//! Module map:
//! - `frame`   — the 24-byte wire framing + OPEN target codec (pure).
//! - `reorder` — per-session inbound reordering (pure).
//! - `endpoint`— carriers, sessions, policy-aware scheduling (S11),
//!   cumulative ACKs, replay-on-carrier-death, keepalives, sweeps.
//! - `outbound`— [`BondOutbound`] (the client `mt_net::Outbound`) and the
//!   app-facing [`outbound::BondStream`].
//! - `daemon`  — the server half (`mt-merge-server` binary).
//!
//! The crate is dependency-lean on purpose: tokio + mt-net + anyhow +
//! tracing only (zero-dep distribution rule).

pub mod daemon;
mod endpoint;
mod frame;
mod outbound;
mod reorder;

pub use daemon::{serve, BOND_IDLE_GC};
pub use endpoint::{
    CarrierSnapshot, APP_BACKLOG_CAP, CARRIER_RESPAWN_ATTEMPTS, CARRIER_RESPAWN_BACKOFF,
    DIAL_TIMEOUT, MAX_SESSIONS_PER_BOND, PENDING_OPEN_TIMEOUT, PING_INTERVAL, PONG_TIMEOUT,
    PRIORITY_QUEUE_HIGH_WATER, REORDER_CAP, STALL_TIMEOUT, SWEEP_INTERVAL, WRITER_QUEUE,
};
pub use frame::{
    decode_header, decode_target, encode_target, read_frame, write_frame, CloseReason, Frame, Kind,
    MAGIC, MAX_PAYLOAD, VERSION,
};
pub use outbound::{BondOutbound, BondStream, CARRIER_ESTABLISH_BUDGET};
pub use reorder::{Delivered, ReorderBuffer, ReorderError};

#[cfg(test)]
mod tests {
    use super::*;

    /// Coherence of the protocol constants (guards accidental config
    /// drift). The usize relations are compile-time; Duration comparisons
    /// stay runtime (PartialOrd is not const).
    #[test]
    fn constants_are_coherent() {
        const {
            assert!(MAX_PAYLOAD <= 64 * 1024);
            assert!(WRITER_QUEUE >= 8);
            assert!(REORDER_CAP >= MAX_PAYLOAD * 16);
            assert!(APP_BACKLOG_CAP >= REORDER_CAP);
        }
        assert!(PONG_TIMEOUT > PING_INTERVAL);
        assert!(STALL_TIMEOUT >= PING_INTERVAL);
    }
}
