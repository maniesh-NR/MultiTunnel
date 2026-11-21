//! mt-app — the MultiTunnel Material You GUI (iced 0.13).
//!
//! # CONTRACT
//!
//! Frozen by the orchestrator (see `harness/CONTRACTS-S6.md` "Expert C2"):
//!
//! * iced 0.13 GUI, **tiny-skia** renderer (chosen over wgpu for
//!   windows-gnu zig cross-compile robustness; proven to link).
//! * Codes against the frozen [`mt_service`] UI contract: [`mt_service::MtService`],
//!   [`mt_service::StatusSnapshot`], [`mt_service::LinkStatus`],
//!   [`mt_service::ProfileView`], [`mt_service::RuleView`],
//!   [`mt_service::SettingsView`], [`mt_service::ServiceEvent`].
//! * Left M3 navigation rail (Status / Servers / Routing / Settings / Logs)
//!   with an active pill indicator; M3 filled/tonal/outlined/text buttons,
//!   switches, text fields, cards on tonal surfaces; all styling derived from
//!   [`mt_m3`] tokens (colors, type scale, shape radii, state layers).
//! * Live re-theme: the whole palette is re-derived from
//!   `SettingsView::theme_seed` (+ the S18 appearance mode —
//!   system/light/dark, `System` resolving through the OS preference) on
//!   every change.
//! * Screens: Status (state card, connect switch, links with latency chips,
//!   throughput), Servers (add-by-paste with parse preview via
//!   [`mt_links::parse_share_link`], set active, latency test, delete,
//!   export link *stub*), Routing (rule table editor), Settings (seed
//!   swatches + numeric field, dark, system proxy, merge policy, autostart),
//!   Logs (streaming view with level filter).
//! * `--smoke` flag: builds the app, drives the update loop and every view
//!   for ~3 s WITHOUT opening a window, then exits 0. Limitation: iced 0.13
//!   cannot create a headless window, so smoke exercises model + update +
//!   view construction only (no renderer/winit). See [`run_smoke`].
//! * Feature `mock` (default ON) ships [`MockService`], a simulated
//!   [`mt_service::MtService`] with a fake connect lifecycle so the GUI runs
//!   standalone. The real backend comes from the root crate in wave 3.
//!
//! # IMPLEMENTATION NOTES
//!
//! * Architecture: MVU (`iced::application`): [`App`] holds the view model;
//!   [`Message`] is the only mutation entry point; async service calls are
//!   mapped back through `Task::perform`.
//! * Rendering: the theme is a *snapshot* ([`MtTheme`]) copied into every
//!   style closure (M3Scheme is `Copy`), so live re-theming is a plain field
//!   update — no global state.
//! * Style approximations (documented): M3 has no "disabled" container token
//!   here; we approximate it with `state_layer(surface, on_surface)` at the
//!   pressed opacity. Font sizes map 1 M3 "sp" to 1 iced point.
//! * Known `MtService` gaps found while implementing (reported to the
//!   orchestrator, NOT worked around): **no `export_link`** on the trait —
//!   `ProfileView` carries no link material, so the Servers screen shows a
//!   disabled "Export link" control. The GUI never logs credential values;
//!   error strings from the service are rendered verbatim (the contract
//!   guarantees they are redacted).
//! * The binary target `mt-app` runs against [`MockService`]; the root crate
//!   will call [`run_app`] with its own `Arc<dyn MtService>`.

pub mod app;
pub mod graph;
pub mod screens;
pub mod theme;
pub mod tray;
pub mod tray_win;

#[cfg(feature = "mock")]
pub mod mock;

pub use app::{
    run_app, run_smoke, AdapterRow, App, Flags, LogFilter, Message, ParsedPreview, RuleRow, Screen,
    SmokeReport,
};
pub use theme::MtTheme;

#[cfg(feature = "mock")]
pub use mock::MockService;

/// Errors surfaced by the GUI entry points.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// iced runtime failed to start (window creation, renderer init…).
    #[error("gui runtime error: {0}")]
    Gui(#[from] iced::Error),
    /// `run_app` called without any service backend compiled in.
    #[error(
        "no service backend compiled; build with feature `mock` (default) or wire a real MtService"
    )]
    NoBackend,
}

/// Convenience alias for the element type produced by every view.
pub type Element<'a> = iced::Element<'a, Message, iced::Theme, iced::Renderer>;

/// Re-exports used by the screen modules.
pub(crate) mod util {
    use std::collections::VecDeque;

    /// Cap for the in-memory log buffer shown on the Logs screen.
    pub(crate) const LOG_CAP: usize = 2000;

    /// Formats a bit/byte rate ("832 B/s", "1.4 MB/s", "2.0 GB/s").
    pub(crate) fn fmt_bps(bps: u64) -> String {
        fmt_bytes(bps) + "/s"
    }

    /// Formats a byte count ("0 B", "832 B", "1.4 MB", "2.0 GB", "1.0 TB").
    pub(crate) fn fmt_bytes(b: u64) -> String {
        const UNIT: f64 = 1024.0;
        let v = b as f64;
        if v < UNIT {
            format!("{b} B")
        } else if v < UNIT * UNIT {
            format!("{:.1} KB", v / UNIT)
        } else if v < UNIT * UNIT * UNIT {
            format!("{:.1} MB", v / (UNIT * UNIT))
        } else if v < UNIT * UNIT * UNIT * UNIT {
            format!("{:.1} GB", v / (UNIT * UNIT * UNIT))
        } else {
            format!("{:.1} TB", v / (UNIT * UNIT * UNIT * UNIT))
        }
    }

    /// Formats a latency chip label.
    pub(crate) fn fmt_ms(ms: u32) -> String {
        format!("{ms} ms")
    }

    /// Shortens a string for chip display.
    pub(crate) fn short(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            s.to_string()
        } else {
            let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
            t.push('…');
            t
        }
    }

    /// Bounded push for the log buffer (drops the oldest lines).
    pub(crate) fn push_log(buf: &mut VecDeque<String>, line: String) {
        while buf.len() >= LOG_CAP {
            buf.pop_front();
        }
        buf.push_back(line);
    }
}
