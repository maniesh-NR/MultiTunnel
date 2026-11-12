//! `multitunnel` — console binary.
//!
//! Commands:
//!   (none)/tray   start the tray app (Windows; implemented stage S3)
//!   connect       connect headless, run until Ctrl+C (stage S2)
//!   generate      write the generated Xray client config and exit
//!   doctor        environment check (stage S2)
//!   version       print version

use std::path::PathBuf;
use std::process::ExitCode;

use multitunnel::{config::Config, xrayconf};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg_path: Option<PathBuf> = None;
    let mut cmd_args = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-config" || args[i] == "--config" {
            if i + 1 >= args.len() {
                eprintln!("error: {} requires a path", args[i]);
                return ExitCode::from(2);
            }
            i += 1;
            cfg_path = Some(PathBuf::from(&args[i]));
        } else {
            cmd_args.push(args[i].clone());
        }
        i += 1;
    }
    let cmd = cmd_args.first().map(String::as_str).unwrap_or("tray");

    let result = match cmd {
        "version" => {
            println!("multitunnel {VERSION}");
            Ok(())
        }
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        "generate" => load_config(cfg_path).and_then(|ref cfg| generate(cfg)),
        "doctor" => Err(multitunnel::Error("doctor lands in stage S2".into())),
        "connect" => Err(multitunnel::Error("connect lands in stage S2".into())),
        "tray" => Err(multitunnel::Error(
            "tray lands in stage S3 — build multitunnel-tray".into(),
        )),
        other => {
            eprintln!("unknown command {other:?}\n");
            usage();
            Err(multitunnel::Error(String::new()))
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if !e.0.is_empty() {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn default_config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.json")))
        .unwrap_or_else(|| PathBuf::from("config.json"))
}

/// Load config.json; if missing, write a template and explain what to fill.
fn load_config(explicit: Option<PathBuf>) -> Result<Config, multitunnel::Error> {
    let path = explicit.unwrap_or_else(default_config_path);
    if !path.exists() {
        Config::save_template(&path)?;
        return Err(multitunnel::Error(format!(
            "no config found — a template was written to {};\n\
             fill in the server.* fields (run server/setup-xray-reality.sh on your VM —\n\
             its final output block contains every value you need) and start again",
            path.display()
        )));
    }
    Config::load(&path)
}

fn generate(cfg: &Config) -> Result<(), multitunnel::Error> {
    let out = cfg.paths.xray_config.clone();
    let blob = xrayconf::render(cfg)?;
    std::fs::write(&out, blob + "\n")?;
    println!("wrote {}", out.display());
    println!(
        "note: mux is disabled by design so net-merge can spread individual connections across links"
    );
    Ok(())
}

fn usage() {
    println!("MultiTunnel {VERSION} — Xray tunnel + net-merge link bonding\n");
    println!("Usage: multitunnel [command] [-config <path>]\n");
    println!("Commands:");
    println!("  (none)     start the tray app (Windows)");
    println!("  connect    connect headless, run until Ctrl+C");
    println!("  generate   write the generated Xray client config and exit");
    println!("  doctor     check environment (binaries, config, ports)");
    println!("  version    print version");
}
