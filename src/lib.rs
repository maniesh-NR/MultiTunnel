//! MultiTunnel library.
//!
//! Layout (stage S1 fills netmerge/xrayconf; later stages add the supervisor
//! and Windows-specific modules):
//!
//! - [`config`]: JSON config load/save/validate (defaults, path resolution)
//! - [`xrayconf`]: Xray client config generation (mux OFF — see crate docs)
//! - [`netmerge`]: net-merge CLI integration + tolerant status parser
//!
//! Architecture contract (do not break):
//! 1. mux is always disabled: net-merge bonds by spreading individual TCP
//!    connections; a muxed tunnel is one stream and rides one link.
//! 2. the core exposes loopback-only SOCKS/HTTP inbounds; net-merge's wintun
//!    adapter is the single system-wide routing layer (no TUN inbound).
//! 3. LAN/private traffic bypasses the tunnel.

pub mod config;
pub mod error;
pub mod netmerge;
pub mod xrayconf;

pub use error::Error;
