//! `multitunnel-tray` — Windows notification-area UI (implemented stage S3).
//!
//! Locates config.json next to the executable (falling back to the working
//! directory), elevates via UAC when Windows needs it, then hands control to
//! [`multitunnel::tray::run`]. On a missing config it writes a template plus
//! a friendly note file and exits — a GUI binary has no stdout to talk to.
//! Non-Windows: the tray module's stub reports the error and main exits 2
//! (same behavior as the pre-S3 stub).
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use multitunnel::config::Config;
use multitunnel::supervisor::Supervisor;
use multitunnel::{tray, Error};

fn main() -> ExitCode {
    // GUI-subsystem note (S2-REV-B P2-5): without a console the stderr
    // handle can be invalid, and eprintln! PANICS on a failed write
    // (exit 101 instead of the intended 2). Write best-effort and ignore
    // the error.
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "multitunnel-tray: {e}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), Error> {
    // net-merge (wintun) and the HKCU proxy writes need administrator
    // rights — relaunch elevated (UAC prompt) and let the new instance take
    // over. Original args ride along so a --config override survives.
    #[cfg(windows)]
    if !multitunnel::elevate::is_elevated() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        multitunnel::elevate::relaunch_elevated(&args)?;
        return Ok(());
    }

    let (config_path, cfg) = load_or_template()?;
    let supervisor = Arc::new(Supervisor::new(cfg));
    tray::run(supervisor, config_path)
}

/// `-config <path>` / `--config <path>` (same spelling as the console bin).
fn explicit_config_arg() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "-config" || a == "--config" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

/// Locate the config: explicit arg, else beside the executable, else the
/// working directory. When nothing exists, the TEMPLATE lands beside the
/// executable (its home), never in a random cwd.
fn load_or_template() -> Result<(PathBuf, Config), Error> {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.json")));
    let path = match explicit_config_arg() {
        Some(p) => p,
        None => match &beside_exe {
            Some(p) if p.exists() => p.clone(),
            _ if Path::new("config.json").exists() => PathBuf::from("config.json"),
            // Neither side has one: template goes next to the exe.
            None => PathBuf::from("config.json"),
            Some(p) => p.clone(),
        },
    };
    if !path.exists() {
        Config::save_template(&path)?;
        let note = path.with_file_name("config-needed.txt");
        // The note is a courtesy; a failed write must not mask the error.
        let _ = std::fs::write(&note, instructions(&path));
        return Err(Error(format!(
            "no config found — a template was written to {};\n\
             see {} for what to fill in",
            path.display(),
            note.display()
        )));
    }
    let cfg = Config::load(&path)?;
    Ok((path, cfg))
}

/// Friendly first-run note (the GUI equivalent of the console bin's
/// "fill in the server.* fields" message).
fn instructions(template: &Path) -> String {
    format!(
        "MultiTunnel tray — first run\n\n\
         No config.json was found, so a template was written to:\n    {}\n\n\
         Fill in the server.* fields:\n\
         - run server/setup-xray-reality.sh on your VM; the block it prints\n\
         at the end contains every value you need (server.shortId may stay\n\
         empty)\n\
         - then start multitunnel-tray again\n\n\
         (This note file can be deleted.)\n",
        template.display()
    )
}
