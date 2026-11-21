//! MultiTunnel library.
//!
//! Layout (stage S1 fills netmerge/xrayconf; later stages add the supervisor
//! and Windows-specific modules):
//!
//! - [`config`]: JSON config load/save/validate (defaults, path resolution)
//! - [`xrayconf`]: Xray client config generation (mux OFF — see crate docs)
//! - [`netmerge`]: net-merge CLI integration + tolerant status parser
//! - [`vless`]: `vless://` share-link parsing into the server config block
//! - [`tray`]: Windows notification-area UI (stage S3; raw win32 per D4)
//! - [`real_service`]: the all-in-one v0.3 backend (S6): ProfileStore +
//!   mt-net ProxyServer + mt-engine + mt-platform + legacy sysproxy behind
//!   the frozen `mt_service::MtService` UI contract (consumed by mt-app's
//!   GUI through the `gui` command)
//! - [`profiles`]: the `profiles` CLI over the shared multi-server store
//!
//! Architecture contract (do not break):
//! 1. mux is always disabled: net-merge bonds by spreading individual TCP
//!    connections; a muxed tunnel is one stream and rides one link.
//! 2. the core exposes loopback-only SOCKS/HTTP inbounds; net-merge's wintun
//!    adapter is the single system-wide routing layer (no TUN inbound).
//! 3. LAN/private traffic bypasses the tunnel.

pub mod config;
pub mod core_proc;
pub mod diagnostics;
pub mod elevate;
pub mod error;
pub mod fsutil;
pub mod netmerge;
pub mod profiles;
pub mod real_service;
pub mod supervisor;
pub mod sysproxy;
pub mod tray;
pub mod vless;
pub mod xrayconf;

pub use error::Error;
pub use real_service::RealService;
