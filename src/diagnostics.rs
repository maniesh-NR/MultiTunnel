//! S19: diagnostics mode — one self-contained, paste-ready log file.
//!
//! While diagnostics mode is ON the service appends EVERYTHING a support
//! conversation needs to a single file under the log dir:
//!
//! - a redacted header (version, OS, settings summary, profile endpoints —
//!   NEVER a UUID/password/public key value);
//! - every GUI-visible log line (`ServiceEvent::LogLine` tee — connection
//!   lifecycle, carrier verdicts, sysproxy operations);
//! - every `tracing` event the process emits (engine child stdout/stderr
//!   forwarded by mt-engine, supervisor decisions, warnings) via a
//!   hand-rolled process-global [`tracing::Subscriber`] — no
//!   `tracing-subscriber` dependency (zero-dependency rule; the e2e rig's
//!   `PrintSubscriber` is the pattern).
//!
//! Design notes:
//! - The sink is a PROCESS-GLOBAL static: the tracing subscriber can only
//!   be installed once per process, so the file handle it writes to must be
//!   reachable from it. `RealService` instances (and tests) share it.
//! - Toggling the mode off just clears the handle — the file stays on disk
//!   for the user to paste; a later re-enable starts a NEW timestamped
//!   file (fresh context per support conversation).
//! - Writes are line-buffered by construction (`File` is unbuffered; one
//!   `writeln!` per event) so a crash never eats the tail of the log.
//! - `enable`/`disable`/`line` are cheap and never panic: a diagnostics
//!   sink failure must degrade to "no diagnostics", never to a dead app
//!   (the GUI-never-dies-invisibly rule, D15).

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// The one sink the whole process (service + subscriber) writes through.
static SINK: LazyLock<Mutex<Option<Sink>>> = LazyLock::new(|| Mutex::new(None));
/// The global subscriber is installed at most once per process.
static SUBSCRIBER: OnceLock<()> = OnceLock::new();
/// Fast-path check: events skip formatting entirely when off.
static ENABLED: AtomicBool = AtomicBool::new(false);
/// Crate-visible lock for tests that flip the (process-global) sink: the
/// crate's tests run in parallel inside one binary, so every test that
/// enables/disables diagnostics holds this to keep windows from
/// interleaving (see the real_service S19 tests too). Tokio's mutex —
/// guards may be held across `.await` (the clippy rule); LazyLock because
/// its `new` is not const.
#[cfg(test)]
pub(crate) static TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

struct Sink {
    path: PathBuf,
    file: File,
}

/// Turn diagnostics ON: create `<log_dir>/diagnostics-<timestamp>.log`,
/// write the caller-prepared redacted header, install the process-global
/// subscriber (first enable only). Any previous sink is closed first
/// (toggles are idempotent-ish: re-enable = fresh file).
/// Returns the file path on success.
pub(crate) fn enable(log_dir: &Path, header: &str) -> Result<PathBuf, String> {
    install_subscriber();
    std::fs::create_dir_all(log_dir).map_err(|e| format!("create {}: {e}", log_dir.display()))?;
    let path = log_dir.join(format!("diagnostics-{}.log", timestamp_for_filename()));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    let _ = writeln!(file, "{header}");
    let _ = writeln!(file, "{} [DIAG] diagnostics mode started", fmt_hms(now()));
    *SINK.lock().unwrap_or_else(|p| p.into_inner()) = Some(Sink {
        path: path.clone(),
        file,
    });
    ENABLED.store(true, Ordering::Release);
    Ok(path)
}

/// Turn diagnostics OFF: close the sink, keep the file. Returns the path
/// of the file that was being written (None when the mode was already off).
pub(crate) fn disable() -> Option<PathBuf> {
    ENABLED.store(false, Ordering::Release);
    let mut sink = SINK.lock().unwrap_or_else(|p| p.into_inner()).take()?;
    let _ = writeln!(
        sink.file,
        "{} [DIAG] diagnostics mode stopped",
        fmt_hms(now())
    );
    Some(sink.path)
}

/// The file currently being written, if the mode is on.
pub(crate) fn path() -> Option<PathBuf> {
    SINK.lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .map(|s| s.path.clone())
}

/// Is the mode on right now? (The engine log level override asks this.)
pub(crate) fn is_on() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// Append one service-side line (the GUI `LogLine` tee; `kind` tags the
/// source, e.g. "GUI"). Best-effort — a write failure just disables the
/// sink (a broken log file must never break the app).
pub(crate) fn line(kind: &str, msg: &str) {
    let mut guard = SINK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(sink) = guard.as_mut() else {
        return;
    };
    let wrote = writeln!(sink.file, "{} [{kind}] {msg}", fmt_hms(now()));
    if wrote.is_err() {
        *guard = None;
        ENABLED.store(false, Ordering::Release);
    }
}

fn install_subscriber() {
    let _ = SUBSCRIBER.set(());
    // set_global_default fails when another subscriber exists (e.g. a test
    // rig's) — then tracing output simply stays wherever it was going and
    // only the LogLine tee reaches the file. Acceptable, never fatal.
    let _ = tracing::subscriber::set_global_default(DiagSubscriber);
}

/// Hand-rolled subscriber (the e2e `PrintSubscriber` pattern): formats
/// every event as `<time> [LEVEL] <target> <fields>` into the sink.
struct DiagSubscriber;

impl tracing::Subscriber for DiagSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        // The cheap path: everything is offered to `event`, which checks
        // the fast-path flag itself (mirrors the e2e PrintSubscriber
        // discipline — no filtering logic duplicated here).
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if !ENABLED.load(Ordering::Acquire) {
            return;
        }
        struct V(String);
        impl tracing::field::Visit for V {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                use std::fmt::Write as _;
                let _ = write!(self.0, " {}={:?}", field.name(), value);
            }
        }
        let mut v = V(String::new());
        event.record(&mut v);
        line(
            &format!("LEVEL:{}", event.metadata().level()),
            &format!("[{}]{}", event.metadata().target(), v.0),
        );
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

fn now() -> SystemTime {
    SystemTime::now()
}

/// `HH:MM:SS.mmm` UTC (the file's own header carries the full date).
fn fmt_hms(t: SystemTime) -> String {
    let ms = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let secs = ms / 1000;
    let ms_part = ms % 1000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60,
        ms_part
    )
}

/// `YYYYMMDD-HHMMSS` UTC for the filename (sortable, no spaces).
fn timestamp_for_filename() -> String {
    let secs = now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs.div_euclid(86_400) as i64;
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days (the fmt_utc sibling in
    // real_service; duplicated here to keep the module self-contained).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "mt-diag-{}-{}-{}",
            tag,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).expect("scratch dir");
        d
    }

    #[tokio::test]
    async fn enable_writes_header_and_lines_disable_keeps_file() {
        let _guard = TEST_LOCK.lock().await;
        let _ = disable(); // a previous test may have left it on
        let dir = scratch("enable");
        let diag_file = enable(&dir, "HEADER-LINE v0.0.0-test").expect("enable");
        assert!(diag_file.to_string_lossy().contains("diagnostics-"));
        line("GUI", "hello diagnostics");
        // A tracing event must land too (enable installs the subscriber —
        // or it was already installed; either way the sink receives it).
        tracing::warn!(target: "mt_test_diag", "traced through");
        let closed = disable().expect("disable returns the path");
        assert_eq!(closed, diag_file);
        assert!(path().is_none(), "no sink after disable");
        let text = std::fs::read_to_string(&diag_file).expect("file kept on disk");
        assert!(text.contains("HEADER-LINE v0.0.0-test"));
        assert!(text.contains("diagnostics mode started"));
        assert!(text.contains("[GUI] hello diagnostics"));
        assert!(
            text.contains("traced through"),
            "tracing events must reach the sink: {text}"
        );
        assert!(text.contains("diagnostics mode stopped"));
        // Off-state writes are dropped, never panic.
        line("GUI", "after close: dropped silently");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn filename_is_sortable_utc() {
        let stamp = timestamp_for_filename();
        assert_eq!(stamp.len(), 15, "YYYYMMDD-HHMMSS: {stamp}");
        assert_eq!(stamp.as_bytes()[8], b'-');
        assert!(stamp.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }
}
