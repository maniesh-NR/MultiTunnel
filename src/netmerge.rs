//! net-merge CLI integration (https://github.com/net-merge/net-merge).
//!
//! net-merge runs as its own tray application; we talk to it through its
//! command-line verbs (`on`, `off`, `status`, `tray`). The exact `status`
//! output format is not a stable contract, so [`parse_status`] is
//! deliberately heuristic: it extracts per-link lines plus a merging flag
//! when it can, and always keeps the raw text for display.
//!
//! Every CLI call goes through the bounded [`run`], which kills the child on
//! timeout instead of ever hanging the supervisor.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Serialize;

use crate::err;
use crate::error::Result;

/// One physical uplink seen in the status output.
#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub name: String,
    pub detail: String,
}

/// Parsed result of `net-merge status`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    pub merging: bool,
    pub raw: String,
    pub links: Vec<Link>,
}

/// How often [`run`] polls the child for exit while it is alive.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long [`run`] tolerates a reader thread after the child has exited
/// (a grandchild inheriting the pipe could otherwise keep it open forever).
const READER_GRACE: Duration = Duration::from_secs(1);

/// A throughput/latency metric: a number, optional blank(s), and one of the
/// units net-merge prints (case-insensitive). `%` deliberately carries no
/// trailing `\b` — after a non-word char a boundary can never match.
const METRIC_SRC: &str = concat!(
    r"(?i:\b\d+(?:\.\d+)?[ \t]*",
    r"(?:Mbit/s\b|Gbps\b|Mbps\b|Kbps\b|KiB/s\b|MiB/s\b|KB/s\b|MB/s\b|B/s\b|ms\b|%)",
    r")",
);

/// `merging|active|enabled` followed within ~20 chars by a standalone
/// `on|yes|true`. So `Merging: on` and `bonding enabled: yes` match, while
/// `status: merging` (nothing after the keyword) and `Merging: off` do not.
static MERGING_RE: OnceLock<Regex> = OnceLock::new();
fn merging_re() -> &'static Regex {
    MERGING_RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:merging|active|enabled)\b[^\n]{0,20}?\b(?:on|yes|true)\b")
            .expect("static merging regex must compile")
    })
}

/// The canonical per-link line: name, 2+ blanks, then the rest of the line
/// (which must contain a metric — validated post-match with [`metric_re`]).
/// A leading bullet is consumed by the anchor and stripped again post-match.
/// Metric validation is deliberately NOT inside this pattern: embedding
/// METRIC_SRC after `\S.*` would force its leading `\b` to match mid-number
/// (e.g. `\S` eats the `4` of `45 Mbps` and `\b5` can never match).
static LINK_LINE_RE: OnceLock<Regex> = OnceLock::new();
fn link_line_re() -> &'static Regex {
    LINK_LINE_RE.get_or_init(|| {
        Regex::new(r"^[ \t]*(?:[-*•][ \t]*)?(.+?)[ \t]{2,}(.+)$")
            .expect("static link-line regex must compile")
    })
}

/// Just the metric — used by the fallback path.
static METRIC_RE: OnceLock<Regex> = OnceLock::new();
fn metric_re() -> &'static Regex {
    METRIC_RE.get_or_init(|| Regex::new(METRIC_SRC).expect("static metric regex must compile"))
}

/// Run `bin args…` under a hard time bound and capture its combined output.
///
/// Lenient variant (used by [`status`]): a nonzero exit WITH captured output
/// still yields `Ok(output)` — the CLI may print useful diagnostics on
/// failure and the text beats an error.
///
/// - the child's stdin is null; stdout/stderr are piped and drained on
///   threads, so a chatty child can never fill the OS pipe buffer and wedge
///   us while we poll;
/// - the child is polled every 50 ms; past `timeout` it is killed and reaped
///   and a timeout error naming `bin` and `args` is returned;
/// - exit 0 → `Ok(output)`, trimmed;
/// - nonzero exit without any output → `Err` naming the exit status;
/// - spawn failure (binary missing) → `Err` with the underlying io error.
pub fn run(bin: &str, timeout: Duration, args: &[&str]) -> Result<String> {
    let (status, combined) = run_full(bin, timeout, args)?;
    if status.success() || !combined.is_empty() {
        return Ok(combined);
    }
    Err(err!(
        "`{bin} {}` failed with {status} and produced no output",
        args.join(" ")
    ))
}

/// Strict variant used by [`on`]/[`off`]: a nonzero exit is ALWAYS an error,
/// even with output — a failed `net-merge on` must never look like success
/// to the supervisor (reviewer finding S1-REV-C #2).
pub fn run_strict(bin: &str, timeout: Duration, args: &[&str]) -> Result<String> {
    let (status, combined) = run_full(bin, timeout, args)?;
    if status.success() {
        return Ok(combined);
    }
    if combined.is_empty() {
        Err(err!("`{bin} {}` failed with {status}", args.join(" ")))
    } else {
        Err(err!(
            "`{bin} {}` failed with {status}: {combined}",
            args.join(" ")
        ))
    }
}

/// Core bounded execution shared by [`run`] and [`run_strict`].
fn run_full(
    bin: &str,
    timeout: Duration,
    args: &[&str],
) -> Result<(std::process::ExitStatus, String)> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| err!("failed to start `{bin} {}`: {e}", args.join(" ")))?;

    // Drain both pipes concurrently: reading only after wait() deadlocks as
    // soon as the child writes more than the pipe buffer holds.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });

    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| err!("timeout {timeout:?} is too large"))?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = join_reader(stdout_reader);
                let stderr = join_reader(stderr_reader);
                let mut combined = String::from_utf8_lossy(&stdout).into_owned();
                combined.push_str(&String::from_utf8_lossy(&stderr));
                let combined = combined.trim().to_string();

                return Ok((status, combined));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait(); // reap the killed child
                                          // Reader threads are dropped (detached): they wind down
                                          // at EOF and cannot hang this call.
                    return Err(err!(
                        "`{bin} {}` timed out after {timeout:?} and was killed",
                        args.join(" ")
                    ));
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => return Err(err!("`{bin} {}`: {e}", args.join(" "))),
        }
    }
}

/// Join a pipe-reader thread, but never block longer than [`READER_GRACE`].
fn join_reader(handle: std::thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    let deadline = Instant::now() + READER_GRACE;
    while Instant::now() < deadline {
        if handle.is_finished() {
            return handle.join().unwrap_or_default();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Vec::new()
}

/// Start merging (`net-merge on`). Strict: failure is an error even when the
/// CLI prints a diagnostic, so the supervisor never believes bonding is up
/// when it is not.
pub fn on(bin: &str) -> Result<()> {
    run_strict(bin, Duration::from_secs(10), &["on"]).map(|_| ())
}

/// Pause merging (`net-merge off`). Strict like [`on`].
pub fn off(bin: &str) -> Result<()> {
    run_strict(bin, Duration::from_secs(10), &["off"]).map(|_| ())
}

/// Parsed `net-merge status`.
pub fn status(bin: &str) -> Result<Status> {
    let raw = run(bin, Duration::from_secs(10), &["status"])?;
    Ok(parse_status(&raw))
}

/// Make sure the net-merge tray app is up: probe status, start `net-merge
/// tray` detached if needed, wait for it to answer.
pub fn ensure_running(bin: &str, timeout: Duration) -> Result<()> {
    if status(bin).is_ok() {
        return Ok(());
    }
    let mut cmd = Command::new(bin);
    cmd.arg("tray");
    // The tray must not inherit our console pipes: it would keep our stdout
    // open (breaking our own piped output) and could echo into it.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    let mut child = cmd.spawn().map_err(|e| err!("start {bin} tray: {e}"))?;
    // Reap the tray in the background so it can never linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| err!("timeout {timeout:?} is too large"))?;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(1));
        if status(bin).is_ok() {
            return Ok(());
        }
    }
    Err(err!("net-merge tray did not answer within {timeout:?}"))
}

/// Heuristic status parser (see module docs). Never panics on any input.
///
/// Contract (refined during S1):
/// - `raw` is the untouched input.
/// - `merging` is true when some line contains `merging|active|enabled`
///   (case-insensitive, word-bounded) followed within ~20 characters by the
///   standalone word `on|yes|true` — so `Merging: on` and `bonding enabled:
///   yes` are true, while `status: merging` and `Merging: off` are false.
/// - a link line is a name separated from its metrics by 2+ blanks, where the
///   metrics contain a number + optional blank + unit from {Mbit/s, Mbps,
///   Kbps, Gbps, KiB/s, MiB/s, KB/s, MB/s, B/s, ms, %} (case-insensitive); a
///   leading bullet (`-`, `*`, `•`) is stripped from the name and `detail`
///   keeps the metrics part of the line.
/// - fallback: a trimmed line shorter than 120 chars that contains a
///   number+unit but did not match the name form becomes a link whose name is
///   its first whitespace token (so `Wi-Fi:` still contains `Wi-Fi`) and
///   whose detail is the whole trimmed line.
pub fn parse_status(raw: &str) -> Status {
    let mut status = Status {
        merging: false,
        raw: raw.to_string(),
        links: Vec::new(),
    };
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if merging_re().is_match(line) {
            status.merging = true;
        }
        if let Some(caps) = link_line_re().captures(line) {
            let name = strip_bullet(caps[1].trim());
            let detail = caps[2].trim();
            if !name.is_empty() && metric_re().is_match(detail) {
                status.links.push(Link {
                    name: name.to_string(),
                    detail: detail.to_string(),
                });
            }
            continue;
        }
        if line.chars().count() < 120 && metric_re().is_match(line) {
            if let Some(token) = line.split_whitespace().next() {
                let name = strip_bullet(token);
                if !name.is_empty() {
                    status.links.push(Link {
                        name: name.to_string(),
                        detail: line.to_string(),
                    });
                }
            }
        }
    }
    status
}

/// Strip leading bullet characters (`-`, `*`, `•`) plus following blanks.
fn strip_bullet(s: &str) -> &str {
    let mut cur = s.trim_start();
    loop {
        let stripped = cur
            .strip_prefix('-')
            .or_else(|| cur.strip_prefix('*'))
            .or_else(|| cur.strip_prefix('•'));
        match stripped {
            Some(rest) => cur = rest.trim_start(),
            None => return cur,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: the process tests below use POSIX tools (echo/sleep/sh/tr/head);
    // they run on the Linux dev host, not on Windows targets.

    const FIXTURE_A: &str = "net-merge 0.3.7\nMerging: on\nLinks:\n  Wi-Fi      12.4 Mbit/s down   1.2 Mbit/s up\n  Ethernet   45.0 Mbit/s down   3.1 Mbit/s up";
    const FIXTURE_B: &str = "status: merging\nWi-Fi: 3 connections, 12.5 Mbps";

    #[test]
    fn fixture_a_parses_merging_and_two_links() {
        let st = parse_status(FIXTURE_A);
        assert!(st.merging, "merging flag");
        assert_eq!(st.raw, FIXTURE_A);
        assert_eq!(st.links.len(), 2, "links: {:?}", st.links);
        assert_eq!(st.links[0].name, "Wi-Fi");
        assert_eq!(st.links[1].name, "Ethernet");
        assert!(st.links[0].detail.contains("12.4"), "{:?}", st.links[0]);
        assert!(st.links[0].detail.contains("Mbit/s"), "{:?}", st.links[0]);
        assert!(st.links[1].detail.contains("45.0"), "{:?}", st.links[1]);
        assert!(st.links[1].detail.contains("Mbit/s"), "{:?}", st.links[1]);
    }

    #[test]
    fn fixture_b_uses_fallback_and_stays_not_merging() {
        let st = parse_status(FIXTURE_B);
        // Intentional: "status: merging" has no on/yes/true after the keyword.
        assert!(!st.merging);
        assert_eq!(st.raw, FIXTURE_B);
        assert!(
            st.links.iter().any(|l| l.name.contains("Wi-Fi")),
            "links: {:?}",
            st.links
        );
        assert!(
            st.links.iter().any(|l| l.detail.contains("12.5")),
            "links: {:?}",
            st.links
        );
    }

    #[test]
    fn merging_flag_variants() {
        assert!(parse_status("Merging: on").merging);
        assert!(parse_status("bonding enabled: yes").merging);
        assert!(parse_status("ACTIVE: TRUE").merging);
        assert!(parse_status("merging: Yes").merging);
        assert!(!parse_status("status: merging").merging);
        assert!(!parse_status("Merging: off").merging);
        assert!(!parse_status("no flags here").merging);
    }

    #[test]
    fn garbage_never_panics_and_preserves_raw() {
        for raw in ["", "\n\n", "no metrics here", "Wi-Fi"] {
            let st = parse_status(raw);
            assert!(st.links.is_empty(), "{raw:?} -> {:?}", st.links);
            assert!(!st.merging, "{raw:?}");
            assert_eq!(st.raw, raw);
        }
    }

    #[test]
    fn bullets_are_stripped_from_link_names() {
        for (line, name) in [
            ("• Wi-Fi  3.2 Mbps", "Wi-Fi"),
            ("- Wi-Fi  3.2 Mbps", "Wi-Fi"),
            ("* Ethernet  45 Mbps", "Ethernet"),
        ] {
            let st = parse_status(line);
            assert_eq!(st.links.len(), 1, "{line:?} -> {:?}", st.links);
            assert_eq!(st.links[0].name, name, "{line:?}");
            assert!(st.links[0].detail.contains("Mbps"), "{:?}", st.links[0]);
        }
    }

    #[test]
    fn run_missing_binary_is_err() {
        let res = status("definitely-missing-binary-xyz");
        let msg = res.expect_err("missing binary must error").to_string();
        assert!(msg.contains("definitely-missing-binary-xyz"), "{msg}");
    }

    #[test]
    fn run_strict_missing_binary_is_err() {
        let err = run_strict(
            "definitely-missing-binary-xyz",
            Duration::from_secs(1),
            &["x"],
        )
        .expect_err("strict: missing binary must error");
        assert!(
            err.to_string().contains("definitely-missing-binary-xyz"),
            "{err}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn run_strict_nonzero_with_output_is_err() {
        // The lenient `run` returns Ok for this input; strict must not.
        let lenient = run("sh", Duration::from_secs(5), &["-c", "echo boom; exit 3"]);
        assert!(lenient.is_ok());
        let err = run_strict("sh", Duration::from_secs(5), &["-c", "echo boom; exit 3"])
            .expect_err("strict: nonzero exit must error even with output");
        let msg = err.to_string();
        assert!(msg.contains("boom"), "{msg}");
        assert!(msg.contains("3"), "{msg}");
    }

    #[test]
    fn on_missing_binary_is_err() {
        // on/off must surface failures (strict), never swallow them.
        assert!(on("definitely-missing-binary-xyz").is_err());
    }

    #[test]
    #[cfg(unix)]
    fn run_echo_returns_trimmed_output() {
        let out = run("echo", Duration::from_secs(1), &["hi"]).expect("echo must succeed");
        assert_eq!(out, "hi");
    }

    #[test]
    #[cfg(unix)]
    fn run_kills_child_on_timeout() {
        let started = Instant::now();
        let err = run("sleep", Duration::from_millis(100), &["2"])
            .expect_err("sleep 2 must hit the timeout");
        let msg = err.to_string();
        assert!(msg.contains("timed out"), "{msg}");
        assert!(msg.contains("sleep"), "{msg}");
        assert!(msg.contains('2'), "{msg}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "kill must be prompt"
        );
    }

    #[test]
    #[cfg(unix)]
    fn run_large_output_does_not_deadlock() {
        // 200 KB through a 64 KB pipe buffer: reading only after wait() would
        // deadlock; the concurrent readers must keep up instead.
        let out = run(
            "sh",
            Duration::from_secs(10),
            &["-c", "head -c 200000 /dev/zero | tr '\\000' 'x'"],
        )
        .expect("large output must be captured");
        assert_eq!(out.len(), 200_000);
    }

    #[test]
    #[cfg(unix)]
    fn run_failure_with_output_beats_error() {
        // net-merge may print diagnostics with a nonzero exit; keep the text.
        let out = run("sh", Duration::from_secs(5), &["-c", "echo boom; exit 3"])
            .expect("output must beat the nonzero exit");
        assert_eq!(out, "boom");
    }

    #[test]
    #[cfg(unix)]
    fn run_captures_stderr_too() {
        let out = run(
            "sh",
            Duration::from_secs(5),
            &["-c", "echo to-stderr 1>&2; exit 4"],
        )
        .expect("stderr output must beat the nonzero exit");
        assert_eq!(out, "to-stderr");
    }

    #[test]
    #[cfg(unix)]
    fn run_failure_without_output_is_err_with_status() {
        let err = run("sh", Duration::from_secs(5), &["-c", "exit 7"])
            .expect_err("no-output failure must error");
        let msg = err.to_string();
        assert!(msg.contains("7"), "{msg}");
    }

    #[test]
    fn ensure_running_missing_binary_is_err() {
        assert!(ensure_running("definitely-missing-binary-xyz", Duration::from_secs(2)).is_err());
    }
}
