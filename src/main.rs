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
use std::time::Duration;

use multitunnel::{config::Config, xrayconf};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cfg_path, cmd_args) = match split_args(&args) {
        Ok(v) => v,
        Err(flag) => {
            eprintln!("error: {flag} requires a path");
            return ExitCode::from(2);
        }
    };
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
            "the tray UI ships as the separate multitunnel-tray.exe binary".into(),
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

/// Split argv (without argv[0]) into the optional `-config`/`--config` path
/// and the remaining command arguments. Both the space form
/// (`-config <path>`) and the equals form (`-config=<path>`,
/// `--config=<path>`) are accepted (reviewer S2-REV-C P2-9).
/// `Err(flag)` when a config flag appears without a usable path (last
/// argument, or an empty `=` value); a later flag wins over an earlier one.
fn split_args(args: &[String]) -> Result<(Option<PathBuf>, Vec<String>), String> {
    let mut cfg_path: Option<PathBuf> = None;
    let mut cmd_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "-config" || arg == "--config" {
            let Some(path) = args.get(i + 1) else {
                return Err(arg.to_string());
            };
            i += 1;
            cfg_path = Some(PathBuf::from(path));
        } else if let Some(path) = arg
            .strip_prefix("--config=")
            .or_else(|| arg.strip_prefix("-config="))
        {
            if path.is_empty() {
                return Err(arg.to_string());
            }
            cfg_path = Some(PathBuf::from(path));
        } else {
            cmd_args.push(args[i].clone());
        }
        i += 1;
    }
    Ok((cfg_path, cmd_args))
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
/// (POSIX 0600; on Windows the default user-profile ACL applies) and never
/// leave a half-written file behind: the write is atomic via
/// [`multitunnel::fsutil::write_private_atomic`] (S2-REV-C P2-6).
/// (reviewer finding S1-REV-C #1)
fn write_private(path: &std::path::Path, contents: String) -> std::io::Result<()> {
    multitunnel::fsutil::write_private_atomic(path, &contents)
}

// ---- doctor (stage S2) -----------------------------------------------------

/// Doctor budget for `xray version` (S2-REV-C P2-8): a hung xray must fail
/// the check within 5 s, never hang doctor itself.
const XRAY_VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll slice while waiting for a bounded child to exit.
const BOUNDED_POLL_SLICE: Duration = Duration::from_millis(50);

/// How long a bounded run tolerates a reader thread after the child exited
/// (a grandchild inheriting the pipe could otherwise keep it open forever).
const READER_GRACE: Duration = Duration::from_secs(1);

/// Outcome of [`run_bounded`]: the child exited within the budget (status
/// plus captured output), or it was killed at the deadline.
enum BoundedRun {
    Done {
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
    },
    TimedOut,
}

/// Bounded child execution (S2-REV-C P2-8): spawn `cmd` (stdin closed,
/// stdout/stderr piped — the same stdio setup the old `.output()` had) and
/// poll `try_wait()` in short slices until `timeout` elapses; on timeout the
/// child is killed and reaped and `BoundedRun::TimedOut` is returned. The
/// pipes are drained on reader threads, so a chatty child cannot block its
/// own exit (which would turn into a spurious timeout).
fn run_bounded(cmd: &mut std::process::Command, timeout: Duration) -> std::io::Result<BoundedRun> {
    use std::process::Stdio;

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let stdout_reader = pipe_reader(child.stdout.take());
    let stderr_reader = pipe_reader(child.stderr.take());

    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = String::from_utf8_lossy(&join_reader(stdout_reader)).into_owned();
            let stderr = String::from_utf8_lossy(&join_reader(stderr_reader)).into_owned();
            return Ok(BoundedRun::Done {
                status,
                stdout,
                stderr,
            });
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait(); // reap the killed child; reader threads are
                                  // detached — they wind down at EOF.
            return Ok(BoundedRun::TimedOut);
        }
        std::thread::sleep(BOUNDED_POLL_SLICE);
    }
}

/// Read a pipe to EOF on its own thread (empty bytes when there is no pipe).
fn pipe_reader<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    })
}

/// Join a pipe-reader thread, but never block longer than [`READER_GRACE`].
fn join_reader(handle: std::thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    let deadline = std::time::Instant::now() + READER_GRACE;
    while std::time::Instant::now() < deadline {
        if handle.is_finished() {
            return handle.join().unwrap_or_default();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Vec::new()
}

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
        doctor_line(
            "ok",
            "elevation",
            "elevated (administrator rights available)",
        );
    } else {
        doctor_line(
            "warn",
            "elevation",
            "not elevated — net-merge and system-proxy changes may fail",
        );
    }

    // 2. xray binary exists and answers `xray version` (first line shown).
    //    Bounded run (S2-REV-C P2-8): a hung xray fails the check after 5 s
    //    instead of hanging doctor; CREATE_NO_WINDOW on Windows so no
    //    console window flashes when doctor is launched from a GUI context.
    let xray = &cfg.paths.xray;
    if !xray.exists() {
        doctor_line(
            "fail",
            "xray binary",
            &format!("not found at {}", xray.display()),
        );
        fails += 1;
    } else {
        let mut cmd = std::process::Command::new(xray);
        cmd.arg("version");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW (0x0800_0000), same as net-merge/core
            // spawns — see netmerge.rs / core_proc.rs.
            cmd.creation_flags(0x0800_0000);
        }
        match run_bounded(&mut cmd, XRAY_VERSION_TIMEOUT) {
            Ok(BoundedRun::Done {
                status,
                stdout,
                stderr,
            }) if status.success() => {
                let mut first = stdout.lines().next().unwrap_or("").trim().to_string();
                if first.is_empty() {
                    first = stderr.lines().next().unwrap_or("").trim().to_string();
                }
                doctor_line(
                    "ok",
                    "xray binary",
                    &format!("{} — {first}", xray.display()),
                );
            }
            Ok(BoundedRun::Done { status, .. }) => {
                doctor_line(
                    "fail",
                    "xray binary",
                    &format!("`{} version` exited with {}", xray.display(), status),
                );
                fails += 1;
            }
            Ok(BoundedRun::TimedOut) => {
                doctor_line(
                    "fail",
                    "xray binary",
                    &format!(
                        "`{} version` timed out after {}s and was killed",
                        xray.display(),
                        XRAY_VERSION_TIMEOUT.as_secs()
                    ),
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
    for (label, port) in [
        ("socks port", cfg.local.socks_port),
        ("http port", cfg.local.http_port),
    ] {
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
                doctor_line(
                    "fail",
                    label,
                    &format!("invalid listen address {addr}: {e}"),
                );
                fails += 1;
            }
        }
    }

    // 5. writable dirs (create).
    for (label, dir) in [
        ("log dir", &cfg.paths.log_dir),
        ("state dir", &cfg.paths.state_dir),
    ] {
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
        Err(multitunnel::Error(format!(
            "doctor: {fails} check(s) failed"
        )))
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
    println!("Usage: multitunnel [command] [-config <path> | -config=<path>]\n");
    println!("  The config path may follow the flag after a space");
    println!("  (-config <path>, --config <path>) or be attached with an equals");
    println!("  sign (-config=<path>, --config=<path>).\n");
    println!("Commands:");
    println!("  (none)     start the tray app (Windows)");
    println!("  connect    connect headless, run until Ctrl+C");
    println!("  generate   write the generated Xray client config and exit");
    println!("  doctor     check environment (binaries, config, ports)");
    println!("  version    print version");
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- -config parsing (S2-REV-C P2-9) ------------------------------------

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn config_flag_space_form_is_unchanged() {
        let (cfg, rest) = split_args(&args(&["connect", "-config", "/x/config.json"]))
            .expect("space form parses");
        assert_eq!(cfg.as_deref(), Some(std::path::Path::new("/x/config.json")));
        assert_eq!(rest, ["connect"]);

        let (cfg, rest) = split_args(&args(&["--config", "/x/config.json", "connect"]))
            .expect("long space form parses");
        assert_eq!(cfg.as_deref(), Some(std::path::Path::new("/x/config.json")));
        assert_eq!(rest, ["connect"]);
    }

    #[test]
    fn config_flag_equals_form_parses() {
        for flag in ["-config=/x/c.json", "--config=/x/c.json"] {
            let (cfg, rest) = split_args(&args(&["doctor", flag]))
                .unwrap_or_else(|e| panic!("{flag} must parse: {e}"));
            assert_eq!(cfg.as_deref(), Some(std::path::Path::new("/x/c.json")));
            assert_eq!(
                rest,
                ["doctor"],
                "equals form must be consumed, not a command"
            );
        }
    }

    #[test]
    fn args_without_config_flag_pass_through() {
        let (cfg, rest) = split_args(&args(&["version"])).expect("no flag");
        assert!(cfg.is_none());
        assert_eq!(rest, ["version"]);

        let (cfg, rest) = split_args(&args(&[])).expect("empty argv");
        assert!(cfg.is_none());
        assert!(rest.is_empty());
    }

    #[test]
    fn config_flag_without_a_path_is_an_error() {
        assert_eq!(
            split_args(&args(&["connect", "-config"])),
            Err("-config".to_string())
        );
        assert_eq!(
            split_args(&args(&["--config"])),
            Err("--config".to_string())
        );
        // An empty `=` value carries no path — same error as the missing one.
        assert_eq!(
            split_args(&args(&["-config="])),
            Err("-config=".to_string())
        );
        assert_eq!(
            split_args(&args(&["--config="])),
            Err("--config=".to_string())
        );
    }

    #[test]
    fn last_config_flag_wins_and_position_is_free() {
        // `-config=a.json doctor` ≡ `doctor -config=a.json`, later flag wins.
        let (cfg, rest) = split_args(&args(&["-config=a.json", "doctor", "-config=b.json"]))
            .expect("two flags parse");
        assert_eq!(cfg.as_deref(), Some(std::path::Path::new("b.json")));
        assert_eq!(rest, ["doctor"]);
    }

    // ---- write_private delegation (S2-REV-C P2-6) --------------------------

    #[test]
    fn write_private_writes_contents_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!(
            "mt-main-wp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let p = dir.join("xray-client.json");
        write_private(&p, "credential\n".to_string()).expect("write_private");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "credential\n");
        let leftovers: Vec<std::ffi::OsString> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            leftovers,
            vec![std::ffi::OsString::from("xray-client.json")],
            "no temp sibling left behind"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- bounded doctor run (S2-REV-C P2-8) --------------------------------

    #[cfg(unix)]
    #[test]
    fn run_bounded_returns_output_for_a_fast_child() {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg("echo fast-ok");
        let out = run_bounded(&mut cmd, Duration::from_secs(5)).expect("spawn sh");
        match out {
            BoundedRun::Done {
                status,
                stdout,
                stderr,
            } => {
                assert!(status.success(), "exit status");
                assert_eq!(stdout.trim(), "fast-ok");
                assert!(stderr.trim().is_empty(), "stderr: {stderr}");
            }
            BoundedRun::TimedOut => panic!("fast child must not time out"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn run_bounded_kills_a_slow_child_at_the_deadline() {
        let mut cmd = std::process::Command::new("sh");
        // exec: sh replaces itself with sleep, so the deadline KILL reaps the
        // sleeper itself and orphans nothing (S5-REV-B P2-6).
        cmd.arg("-c").arg("exec sleep 30");
        let start = std::time::Instant::now();
        let out = run_bounded(&mut cmd, Duration::from_millis(250)).expect("spawn sh");
        assert!(
            matches!(out, BoundedRun::TimedOut),
            "slow child must be reported as timed out"
        );
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "must return near the 250ms budget, not after the 30s child"
        );
    }
}
