//! Error type for the mt-net data plane.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B" + global rule 5): `thiserror`
//! for library errors; error messages never echo credentials (this crate
//! handles no credentials at all — host/port/address strings only).
//!
//! IMPLEMENTATION NOTES (S6-B):
//! - The public [`Outbound`] trait boundary uses `anyhow::Result` per the
//!   frozen signature; internal code raises `NetError` which converts into
//!   `anyhow` automatically via `From`.
//! - One enum for the whole crate keeps `?` plumbing simple across the
//!   socks5/http/router/merge layers.

use std::net::SocketAddr;

use thiserror::Error;

/// Errors produced by the mt-net data plane.
#[derive(Debug, Error)]
pub enum NetError {
    /// The proxy's own listen address could not be reached (TCP connect
    /// failed — refused/timeout/unreachable). Nothing was sent upstream,
    /// so callers may safely treat this as "the proxy endpoint is dead"
    /// and, e.g., force a respawn of a managed engine (CI incident
    /// 2026-11-17: the kernel can tear an engine's socket down before the
    /// process is reaped, so a liveness check alone can return a stale
    /// port).
    #[error("cannot reach proxy {0}: {1}")]
    ProxyUnreachable(SocketAddr, #[source] std::io::Error),

    /// SOCKS5 protocol-level failure (client or server side).
    #[error("socks5: {0}")]
    Socks(String),

    /// The SOCKS5 proxy refused the request; payload is the REP code.
    #[error("socks5: proxy refused the request (status 0x{0:02x})")]
    SocksStatus(u8),

    /// HTTP proxy protocol-level failure.
    #[error("http proxy: {0}")]
    Http(String),

    /// A target address could not be parsed or is out of wire range.
    #[error("invalid target address: {0}")]
    Target(String),

    /// Underlying I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
