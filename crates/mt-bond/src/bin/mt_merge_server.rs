//! `mt-merge-server` — the MultiTunnel stream-bonding merge daemon.
//!
//! Runs ON THE TUNNEL SERVER next to Xray. Carriers arrive via Xray's
//! freedom outbound (each bonded adapter's VLESS connection targets
//! 127.0.0.1:<port>); this daemon re-merges the split streams and dials
//! the real targets. Loopback-only by default — authentication happened
//! at the REALITY edge (D17 §6).
//!
//! Usage: `mt-merge-server [--listen 127.0.0.1:7443]`

use std::net::SocketAddr;

use mt_bond::daemon;

fn main() {
    let mut listen: SocketAddr = "127.0.0.1:7443".parse().expect("default listen addr");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => match args.next().map(|v| v.parse::<SocketAddr>()) {
                Some(Ok(addr)) => listen = addr,
                _ => {
                    eprintln!("error: --listen expects host:port (e.g. 127.0.0.1:7443)");
                    std::process::exit(2);
                }
            },
            "--help" | "-h" => {
                println!("mt-merge-server — MultiTunnel stream-bonding merge daemon");
                println!();
                println!("USAGE:");
                println!("  mt-merge-server [--listen 127.0.0.1:7443]");
                println!();
                println!("The daemon must be reachable from the local Xray (freedom");
                println!("outbound); the default binds loopback only. Client bonding");
                println!("sessions target this address through the VLESS tunnel.");
                std::process::exit(0);
            }
            other => {
                eprintln!("error: unknown argument {other:?} (try --help)");
                std::process::exit(2);
            }
        }
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    rt.spawn(async move {
        // Ctrl-C / SIGTERM / service stop → graceful shutdown.
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("[mt-merge-server] signal received — shutting down");
        let _ = shutdown_tx.send(true);
    });
    let code = match rt.block_on(daemon::serve(listen, shutdown_rx)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[mt-merge-server] fatal: {e}");
            1
        }
    };
    std::process::exit(code);
}
