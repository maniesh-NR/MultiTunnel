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
        "doctor" => cmd_doctor(cfg_path),
        "connect" => cmd_connect(cfg_path),
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
    write_private(&out, blob + "\n")?;
    println!("wrote {}", out.display());
    println!(
        "note: mux is disabled by design so net-merge can spread individual connections across links"
    );
    Ok(())
}

/// The generated config embeds the VLESS credential — keep it private
/// (POSIX 0600; on Windows the default user-profile ACL applies).
/// (reviewer finding S1-REV-C #1)
fn write_private(path: &std::path::Path, contents: String) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(contents.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)
    }
}

// ---- doctor (stage S2) -----------------------------------------------------

/// Print one check line: `[ ok ]`, `[warn]` or `[fail]`.
fn doctor_line(level: &str, label: &str, detail: &str) {
    match level {
        "ok" => println!("[ ok ] {label}: {detail}"),
        "warn" => println!("[warn] {label}: {detail}"),
        _ => println!("[fail] {label}: {detail}"),
    }
}

/// Environment check. Every failure bumps `fails`; any fail exits 1.
fn cmd_doctor(cfg_path: Option<PathBuf>) -> Result<(), multitunnel::Error> {
    let cfg = load_config(cfg_path)?;
    let mut fails = 0u32;
    println!("MultiTunnel {VERSION} doctor\n");

    // 1. elevation: net-merge bonding and HKCU/proxy changes want admin
    //    rights; the stub reports "elevated" on non-Windows hosts.
    if multitunnel::elevate::is_elevated() {
        doctor_line("ok", "elevation", "elevated (administrator rights available)");
    } else {
        doctor_line(
            "warn",
            "elevation",
            "not elevated — net-merge and system-proxy changes may fail",
        );
    }

    // 2. xray binary exists and answers `xray version` (first line shown).
    let xray = &cfg.paths.xray;
    if !xray.exists() {
        doctor_line("fail", "xray binary", &format!("not found at {}", xray.display()));
        fails += 1;
    } else {
        match std::process::Command::new(xray).arg("version").output() {
            Ok(out) if out.status.success() => {
                let mut first = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if first.is_empty() {
                    first = String::from_utf8_lossy(&out.stderr)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                }
                doctor_line(
                    "ok",
                    "xray binary",
                    &format!("{} — {first}", xray.display()),
                );
            }
            Ok(out) => {
                doctor_line(
                    "fail",
                    "xray binary",
                    &format!("`{} version` exited with {}", xray.display(), out.status),
                );
                fails += 1;
            }
            Err(e) => {
                doctor_line(
                    "fail",
                    "xray binary",
                    &format!("cannot run `{}`: {e}", xray.display()),
                );
                fails += 1;
            }
        }
    }

    // 3. net-merge status reachable (or explicitly disabled in the config).
    if cfg.net_merge.enable {
        let nm = cfg.paths.net_merge.to_string_lossy().into_owned();
        match multitunnel::netmerge::status(&nm) {
            Ok(st) => doctor_line(
                "ok",
                "net-merge",
                &format!(
                    "status reachable — merging: {}, {} link(s)",
                    st.merging,
                    st.links.len()
                ),
            ),
            Err(e) => {
                doctor_line("fail", "net-merge", &format!("status unreachable: {e}"));
                fails += 1;
            }
        }
    } else {
        doctor_line("warn", "net-merge", "disabled in config (bonding off)");
    }

    // 4. local socks/http ports must be free (bind probe).
    for (label, port) in [("socks port", cfg.local.socks_port), ("http port", cfg.local.http_port)] {
        let addr = format!("{}:{}", cfg.local.listen, port);
        match addr.parse::<std::net::SocketAddr>() {
            Ok(a) => match std::net::TcpListener::bind(a) {
                Ok(l) => {
                    drop(l);
                    doctor_line("ok", label, &format!("{addr} free"));
                }
                Err(e) => {
                    doctor_line("fail", label, &format!("{addr} not free: {e}"));
                    fails += 1;
                }
            },
            Err(e) => {
                doctor_line("fail", label, &format!("invalid listen address {addr}: {e}"));
                fails += 1;
            }
        }
    }

    // 5. writable dirs (create).
    for (label, dir) in [("log dir", &cfg.paths.log_dir), ("state dir", &cfg.paths.state_dir)] {
        match std::fs::create_dir_all(dir) {
            Ok(()) => doctor_line("ok", label, &format!("{} writable", dir.display())),
            Err(e) => {
                doctor_line("fail", label, &format!("{}: {e}", dir.display()));
                fails += 1;
            }
        }
    }

    // 6. probeTarget parses (host:port used for the periodic tunnel probe).
    match multitunnel::supervisor::parse_probe_target(&cfg.behavior.probe_target) {
        Some((host, port)) => doctor_line(
            "ok",
            "probe target",
            &format!(
                "{} → host {host}, port {port}",
                cfg.behavior.probe_target.trim()
            ),
        ),
        None => {
            doctor_line(
                "fail",
                "probe target",
                &format!("{:?} is not host:port", cfg.behavior.probe_target),
            );
            fails += 1;
        }
    }

    if fails > 0 {
        Err(multitunnel::Error(format!("doctor: {fails} check(s) failed")))
    } else {
        println!("\nall checks passed");
        Ok(())
    }
}

// ---- connect (stage S2) ----------------------------------------------------

/// Headless connect: elevate on Windows if needed, bring the tunnel up and
/// stay alive until killed. Ctrl+C detaches (the system proxy stays set;
/// the next `connect` self-heals the stale proxy at its step 0).
fn cmd_connect(cfg_path: Option<PathBuf>) -> Result<(), multitunnel::Error> {
    // Windows only: the supervisor needs admin rights for net-merge and the
    // registry-backed system proxy; relaunch elevated and let it take over.
    #[cfg(windows)]
    {
        if !multitunnel::elevate::is_elevated() {
            let args: Vec<String> = std::env::args().skip(1).collect();
            println!("not elevated — relaunching with administrator rights (UAC prompt)…");
            multitunnel::elevate::relaunch_elevated(&args)?;
            return Ok(()); // the elevated instance continues from here
        }
    }

    let cfg = load_config(cfg_path)?;
    let socks = format!("{}:{}", cfg.local.listen, cfg.local.socks_port);
    let http = format!("{}:{}", cfg.local.listen, cfg.local.http_port);

    let sup = multitunnel::supervisor::Supervisor::new(cfg);
    sup.connect()?;

    let snap = sup.snapshot();
    println!("state : {:?}", snap.state);
    println!("socks : {socks}");
    println!("http  : {http}");
    if snap.merging {
        println!("bond  : merging across {} link(s)", snap.links.len());
        for l in &snap.links {
            println!("        - {}: {}", l.name, l.detail);
        }
    } else {
        println!("bond  : not merging");
    }
    if let Some(e) = &snap.bond_error {
        println!("warn  : bonding failed (tunnel still up): {e}");
    }
    if let Some(e) = &snap.sys_error {
        println!("warn  : system proxy not set (tunnel still up): {e}");
    }
    println!("tunnel is up — press Ctrl+C to detach.");
    println!(
        "note  : the system proxy stays set after Ctrl+C; the next `multitunnel connect` self-heals it."
    );

    // Supervise until killed; the poll loop keeps restarting/probing while
    // this process lives.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
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
