//! `mt-app` binary — the MultiTunnel GUI entry point.
//!
//! Usage:
//! * `mt-app`          — launch the windowed GUI against `MockService`
//!   (feature `mock`, default ON). The root crate launches the real backend
//!   by calling `mt_app::run_app` with its own `Arc<dyn MtService>`.
//! * `mt-app --smoke`  — headless CI mode: build the app, drive the update
//!   loop and every screen's view for ~3 s (no display), exit 0.
//!
//! Limitation (documented): iced 0.13 requires a display for `run()`; the
//! `--smoke` path therefore never enters the iced runtime — it exercises
//! model + update + view construction only (see `mt_app::run_smoke`).

#[cfg(feature = "mock")]
fn main() -> Result<(), mt_app::Error> {
    use std::sync::Arc;
    use std::time::Duration;

    let flags = mt_app::Flags::from_args(std::env::args().skip(1));
    let service: Arc<dyn mt_service::MtService> = Arc::new(mt_app::MockService::new());

    if flags.smoke {
        // Outside the iced runtime: drive the smoke loop on a dedicated
        // current-thread runtime (service init awaits are async).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| mt_app::Error::NoBackend)?;
        let mut app = rt.block_on(mt_app::App::init_async(service, flags));
        let report = mt_app::run_smoke(&mut app, 6, Duration::from_millis(500))?;
        // Every screen view must have been built at least once.
        for scr in mt_app::Screen::ALL {
            let _ = scr; // exercised by the rotation inside run_smoke
        }
        tracing::info!(
            ticks = report.ticks,
            events = report.events_seen,
            views = report.views_built,
            "smoke run complete"
        );
        return Ok(());
    }

    tracing::info!("launching MultiTunnel GUI (mock backend)");
    mt_app::run_app(service, flags)
}

#[cfg(not(feature = "mock"))]
fn main() {
    eprintln!(
        "mt-app was built without a service backend; rebuild with feature `mock` \
         (default) or embed the real MtService from the root crate."
    );
    std::process::exit(1);
}
