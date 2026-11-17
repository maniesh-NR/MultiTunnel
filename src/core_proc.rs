//! Xray core process management (stage S2 — implementer agent S2-A).
//!
//! CONTRACT (fixed by orchestrator; keep signatures stable):
//! - `CoreProc::new(bin, cfg_path, log_dir)`
//! - `start()` spawns `xray run -c <cfg>` with stdout+stderr appended to
//!   `<log_dir>/xray.log` (create dir first); on Windows set
//!   CREATE_NO_WINDOW (0x0800_0000) via CommandExt so no console flashes.
//! - `stop()` kills + reaps, marks manual so the exit is NOT an "unexpected
//!   exit"; safe when not running; `running()` reflects live state.
//! - `exited_unexpectedly()` -> bool consumed-once flag set by the waiter
//!   thread when the child dies on its own.
//! - `wait_for_port(addr, timeout)` — TcpStream::connect_timeout loop.
//! - `socks_probe(proxy_addr, host, port, timeout)` — hand-rolled SOCKS5:
//!   greeting 05 01 00 → expect 05 00; CONNECT 05 01 00 03 len host port →
//!   expect 05 00 ..; drain bound address per ATYP; full chain test
//!   (local core → tunnel → server → internet). Pure std, no deps.
//! - Unit tests: in-test SOCKS5 stub server (TcpListener + thread) for
//!   probe success/refusal; timeout math; log file creation.
//!
//! IMPLEMENTATION NOTES (S2-A1 decisions, "pick one and document it"):
//! - `new(bin, cfg_path, log_dir)`: all three args are `impl Into<PathBuf>`,
//!   so `&str`, `String`, `&Path`, `&PathBuf` and `PathBuf` all work.
//! - Waiter thread: pure std cannot block in `Child::wait()` inside a helper
//!   thread while `stop()` kills the child through the same handle (no
//!   cross-thread handle duplication without unsafe/PID races), so the
//!   waiter polls `try_wait()` every 50 ms while holding the child mutex.
//!   Semantics match a blocking `wait()`: the exit is observed exactly once,
//!   the child slot is cleared (reaped), and the manual-stop flag arbitrates
//!   "unexpected" vs "stopped".
//! - Thread lifecycle: the waiter is *joined* — by `stop()` and by the next
//!   `start()` (covers restarts after an unexpected exit). No leaks.
//! - stdout/stderr are `Stdio::piped()`; two detached pump threads append
//!   everything to `<log_dir>/xray.log` and self-terminate on pipe EOF,
//!   which follows child death + reap (kill/wait or try_wait). No leaks.
//! - `start()` while already running is an error; `exited_unexpectedly` is
//!   intentionally NOT reset by `start()` (consumed-once belongs to the
//!   reader), while `stop_flag` IS reset so a fresh cycle starts clean.
//! - The log header (timestamp + command) is written BEFORE the spawn
//!   attempt, so failed spawns are always diagnosable.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::Result;

/// Poll interval of the child-waiter thread.
const WAITER_POLL_MS: u64 = 50;
/// Delay between `wait_for_port` attempts (contract: 300 ms).
const PORT_STEP_MS: u64 = 300;
/// Log file created under `log_dir` (contract).
const LOG_FILE: &str = "xray.log";

/// Poison-tolerant lock: no panics in non-test code. A poisoned mutex here
/// only means another thread panicked mid-IO — the guarded data stays valid.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Owns and supervises the `xray` core child process.
#[derive(Debug)]
pub struct CoreProc {
    bin: PathBuf,
    cfg_path: PathBuf,
    log_dir: PathBuf,
    /// The live child if any; `None` once stopped or exited (reaped).
    child: Arc<Mutex<Option<Child>>>,
    /// JoinHandle of the waiter thread; joined on stop()/next start().
    waiter: Mutex<Option<JoinHandle<()>>>,
    /// Set by `stop()` so the waiter treats the exit as intentional.
    stop_flag: Arc<AtomicBool>,
    /// Consumed-once (swap) flag: child died without a manual stop.
    exited_unexpectedly: Arc<AtomicBool>,
}

impl CoreProc {
    /// Creates a supervisor handle. All path arguments accept anything that
    /// converts into a `PathBuf` (`&str`, `String`, `&Path`, `PathBuf`).
    pub fn new(
        bin: impl Into<PathBuf>,
        cfg_path: impl Into<PathBuf>,
        log_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            bin: bin.into(),
            cfg_path: cfg_path.into(),
            log_dir: log_dir.into(),
            child: Arc::new(Mutex::new(None)),
            waiter: Mutex::new(None),
            stop_flag: Arc::new(AtomicBool::new(false)),
            exited_unexpectedly: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Spawns `xray run -c <cfg>`. Creates `log_dir` and writes a header
    /// line (timestamp + command) to `<log_dir>/xray.log` first, so failed
    /// spawns are diagnosable; spawn errors mention the binary path.
    /// Returns an error if a child is already running.
    pub fn start(&self) -> Result<()> {
        // Join a leftover waiter from a previous cycle (instant if the
        // previous child already died on its own). Prevents leaks.
        let leftover = lock(&self.waiter).take();
        if let Some(h) = leftover {
            let _ = h.join();
        }

        let mut slot = lock(&self.child);
        if slot.is_some() {
            return Err(crate::err!("core already running: {}", self.bin.display()));
        }

        // 1. log dir + header (must exist even if the spawn then fails).
        fs::create_dir_all(&self.log_dir).map_err(|e| {
            crate::err!("cannot create log dir {}: {}", self.log_dir.display(), e)
        })?;
        let log_path = self.log_dir.join(LOG_FILE);
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| crate::err!("cannot open log {}: {}", log_path.display(), e))?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        writeln!(
            log,
            "[ts {ts}] spawn: {} run -c {}",
            self.bin.display(),
            self.cfg_path.display()
        )
        .map_err(|e| crate::err!("cannot write log header: {}", e))?;
        let _ = log.flush();

        // 2. spawn (fresh cycle: clear the manual-stop flag first).
        self.stop_flag.store(false, Ordering::SeqCst);
        let mut cmd = Command::new(&self.bin);
        cmd.arg("run")
            .arg("-c")
            .arg(&self.cfg_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: the core runs headless, no console flash.
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| crate::err!("failed to spawn core {}: {}", self.bin.display(), e))?;

        // 3. pump piped output into the log (EOF-terminated threads).
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::err!("core spawned without stdout pipe"));
            }
        };
        let stderr = match child.stderr.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::err!("core spawned without stderr pipe"));
            }
        };
        pump_to_log(stdout, log_path.clone());
        pump_to_log(stderr, log_path);

        // 4. publish the child, then start the waiter thread.
        *slot = Some(child);
        drop(slot);
        let handle = thread::spawn({
            let slot = Arc::clone(&self.child);
            let stop = Arc::clone(&self.stop_flag);
            let exited = Arc::clone(&self.exited_unexpectedly);
            move || waiter_loop(slot, stop, exited)
        });
        *lock(&self.waiter) = Some(handle);
        Ok(())
    }

    /// Kills + reaps the child and marks the stop as manual. Safe when not
    /// running and idempotent.
    pub fn stop(&self) {
        // Mark manual FIRST: any exit observed from now on is intentional.
        self.stop_flag.store(true, Ordering::SeqCst);
        let child = lock(&self.child).take();
        if let Some(mut c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }
        // The waiter sees the empty slot (or the stop flag) and exits.
        let handle = lock(&self.waiter).take();
        if let Some(h) = handle {
            let _ = h.join();
        }
    }

    /// True while a spawned child has neither been stopped nor reaped.
    pub fn running(&self) -> bool {
        lock(&self.child).is_some()
    }

    /// Consumed-once check: did the child die without a manual stop?
    /// Returns the previous value and resets to `false`.
    pub fn exited_unexpectedly(&self) -> bool {
        self.exited_unexpectedly.swap(false, Ordering::SeqCst)
    }

    /// Loops `TcpStream::connect_timeout` (≤300 ms per attempt, 300 ms
    /// sleeps) until `addr` accepts a connection or `timeout` elapses.
    /// `Ok(())` on the first successful connect.
    pub fn wait_for_port(addr: &str, timeout: Duration) -> Result<()> {
        let target: SocketAddr = addr
            .parse()
            .map_err(|e| crate::err!("invalid address '{}': {}", addr, e))?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| crate::err!("wait_for_port: timeout overflow"))?;
        let step = Duration::from_millis(PORT_STEP_MS);
        let mut last: Option<std::io::Error> = None;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(crate::err!(
                    "{} not reachable within {} ms (last error: {})",
                    addr,
                    timeout.as_millis(),
                    last.map(|e| e.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
            }
            match TcpStream::connect_timeout(&target, step.min(deadline - now)) {
                Ok(_) => return Ok(()),
                Err(e) => last = Some(e),
            }
            let after = Instant::now();
            if after < deadline {
                thread::sleep(step.min(deadline - after));
            }
        }
    }

    /// Hand-rolled SOCKS5 client probe (pure std, no deps): greeting
    /// `05 01 00` → expect `05 00`; CONNECT `05 01 00 03 len host port` →
    /// expect reply version 05 and status 00 (errors name the status code
    /// otherwise); drains BND.ADDR/BND.PORT per ATYP. Read/write timeouts
    /// are set on the socket, so nothing blocks forever.
    pub fn socks_probe(proxy_addr: &str, host: &str, port: u16, timeout: Duration) -> Result<()> {
        let target: SocketAddr = proxy_addr
            .parse()
            .map_err(|e| crate::err!("invalid proxy address '{}': {}", proxy_addr, e))?;
        let mut s = TcpStream::connect_timeout(&target, timeout)
            .map_err(|e| crate::err!("socks5: cannot reach proxy {}: {}", proxy_addr, e))?;
        s.set_read_timeout(Some(timeout))
            .map_err(|e| crate::err!("socks5: set_read_timeout: {}", e))?;
        s.set_write_timeout(Some(timeout))
            .map_err(|e| crate::err!("socks5: set_write_timeout: {}", e))?;

        // Greeting: offer no-auth (00) only.
        s.write_all(&[0x05, 0x01, 0x00])
            .map_err(|e| crate::err!("socks5: send greeting: {}", e))?;
        let mut g = [0u8; 2];
        socks_read_exact(&mut s, &mut g)?;
        if g[0] != 0x05 || g[1] != 0x00 {
            return Err(crate::err!(
                "socks5: greeting refused (ver 0x{:02x}, method 0x{:02x})",
                g[0],
                g[1]
            ));
        }

        // CONNECT through a domain name (ATYP 03).
        let hb = host.as_bytes();
        if hb.len() > 255 {
            return Err(crate::err!("socks5: host too long ({} bytes)", hb.len()));
        }
        let mut req = Vec::with_capacity(7 + hb.len());
        req.extend_from_slice(&[0x05, 0x01, 0x00, 0x03, hb.len() as u8]);
        req.extend_from_slice(hb);
        req.extend_from_slice(&port.to_be_bytes());
        s.write_all(&req)
            .map_err(|e| crate::err!("socks5: send CONNECT: {}", e))?;

        // Reply head: VER, REP, RSV, ATYP.
        let mut head = [0u8; 4];
        socks_read_exact(&mut s, &mut head)?;
        if head[0] != 0x05 {
            return Err(crate::err!("socks5: bad reply version 0x{:02x}", head[0]));
        }
        if head[1] != 0x00 {
            return Err(crate::err!(
                "socks5: connect refused by proxy (status 0x{:02x})",
                head[1]
            ));
        }
        // Drain BND.ADDR + BND.PORT per ATYP.
        match head[3] {
            0x01 => {
                let mut b = [0u8; 6];
                socks_read_exact(&mut s, &mut b)?;
            }
            0x03 => {
                let mut l = [0u8; 1];
                socks_read_exact(&mut s, &mut l)?;
                let mut b = vec![0u8; l[0] as usize + 2];
                socks_read_exact(&mut s, &mut b)?;
            }
            0x04 => {
                let mut b = [0u8; 18];
                socks_read_exact(&mut s, &mut b)?;
            }
            a => return Err(crate::err!("socks5: unknown ATYP 0x{:02x}", a)),
        }
        Ok(())
    }
}

/// Waiter thread body: watches the child and flips the consumed-once flag
/// when it dies without a manual stop. Exits when the child slot is empty
/// (stop() took it / we reaped it) or the stop flag is set (stop() in
/// flight); joined by stop() and by the next start() — no leaks.
fn waiter_loop(slot: Arc<Mutex<Option<Child>>>, stop: Arc<AtomicBool>, exited: Arc<AtomicBool>) {
    let mut died = false;
    loop {
        let done = {
            let mut g = lock(&slot);
            match g.as_mut() {
                // stop() took the child; it owns kill+reap now.
                None => true,
                Some(c) => match c.try_wait() {
                    // Exited on its own: try_wait reaps; clear the slot so
                    // running() flips false.
                    Ok(Some(_)) => {
                        *g = None;
                        died = true;
                        true
                    }
                    Ok(None) => false,
                    // Broken handle: treat as an exit we did not cause.
                    Err(_) => {
                        *g = None;
                        died = true;
                        true
                    }
                },
            }
        };
        if done || stop.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(WAITER_POLL_MS));
    }
    if died && !stop.load(Ordering::SeqCst) {
        exited.store(true, Ordering::SeqCst);
    }
}

/// Detached pump: appends one pipe (stdout or stderr) to the log until EOF.
/// EOF follows child death + reap, so the thread always terminates.
fn pump_to_log<R: Read + Send + 'static>(mut pipe: R, log_path: PathBuf) {
    thread::spawn(move || {
        let mut log = match OpenOptions::new().create(true).append(true).open(&log_path) {
            Ok(f) => f,
            Err(_) => return, // unusable log; child output is dropped
        };
        let mut buf = [0u8; 4096];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = log.write_all(&buf[..n]);
                }
            }
        }
    });
}

/// read_exact with SOCKS5 context (timeouts surface as io errors here).
fn socks_read_exact<S: Read>(s: &mut S, buf: &mut [u8]) -> Result<()> {
    s.read_exact(buf)
        .map_err(|e| crate::err!("socks5: short read ({} bytes): {}", buf.len(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    /// Unique tag for temp paths (pid + counter): keeps parallel tests apart.
    fn unique_tag() -> String {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        format!(
            "mt-a1-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        )
    }

    /// Grab an ephemeral loopback port and release it (the port is then dead).
    fn free_loopback_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let port = l.local_addr().expect("local_addr").port();
        drop(l);
        port
    }

    /// Minimal SOCKS5 stub server: validates the probe's wire format and
    /// answers with `status` in the final reply (0x00 = success).
    fn stub_socks(listener: TcpListener, status: u8) -> JoinHandle<()> {
        thread::spawn(move || {
            let (mut s, _) = listener.accept().expect("stub: accept");
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut g = [0u8; 3];
            s.read_exact(&mut g).expect("stub: read greeting");
            assert_eq!(g, [0x05, 0x01, 0x00], "stub: unexpected greeting");
            s.write_all(&[0x05, 0x00]).expect("stub: reply greeting");
            let mut head = [0u8; 4];
            s.read_exact(&mut head).expect("stub: read request head");
            assert_eq!(head[0], 0x05, "stub: bad request version");
            assert_eq!(head[3], 0x03, "stub: probe must use domain ATYP");
            let mut l = [0u8; 1];
            s.read_exact(&mut l).expect("stub: read host length");
            let mut hb = vec![0u8; l[0] as usize];
            s.read_exact(&mut hb).expect("stub: read host");
            let mut p = [0u8; 2];
            s.read_exact(&mut p).expect("stub: read port");
            // VER REP RSV ATYP(01) + BND.ADDR(4) + BND.PORT(2)
            s.write_all(&[0x05, status, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
                .expect("stub: send reply");
            let _ = s.flush();
        })
    }

    /// (a) probe success against an in-test stub server.
    #[test]
    fn socks_probe_success() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr").to_string();
        let stub = stub_socks(listener, 0x00);
        CoreProc::socks_probe(&addr, "example.com", 443, Duration::from_secs(3))
            .expect("probe should succeed against the stub");
        stub.join().expect("stub thread");
    }

    /// (b) refusal (reply status 0x05) → Err naming the status.
    #[test]
    fn socks_probe_refusal_names_status() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr").to_string();
        let stub = stub_socks(listener, 0x05);
        match CoreProc::socks_probe(&addr, "example.com", 443, Duration::from_secs(3)) {
            Ok(()) => panic!("probe must fail on refusal status 0x05"),
            Err(e) => assert!(e.0.contains("0x05"), "error must name the status: {}", e.0),
        }
        stub.join().expect("stub thread");
    }

    /// (c) probe against a dead port → Err quickly (timeout math works).
    #[test]
    fn socks_probe_dead_port_fails_fast() {
        let addr = format!("127.0.0.1:{}", free_loopback_port());
        let t0 = Instant::now();
        let res = CoreProc::socks_probe(&addr, "example.com", 443, Duration::from_secs(2));
        assert!(res.is_err(), "dead proxy port must fail");
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "must fail fast, took {:?}",
            t0.elapsed()
        );
    }

    /// (d) wait_for_port: Ok against a live listener; Err against a closed
    /// port with a 500 ms timeout.
    #[test]
    fn wait_for_port_ok_then_closed_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let open = listener.local_addr().expect("addr").to_string();
        CoreProc::wait_for_port(&open, Duration::from_secs(3))
            .expect("live listener must be reachable");

        let closed = format!("127.0.0.1:{}", free_loopback_port());
        let t0 = Instant::now();
        assert!(
            CoreProc::wait_for_port(&closed, Duration::from_millis(500)).is_err(),
            "closed port must time out"
        );
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "timeout must be respected, took {:?}",
            t0.elapsed()
        );
    }

    /// (e) spawn failure: start() errs mentioning the bin, and log_dir (plus
    /// the header line) is created first.
    #[test]
    fn start_failure_creates_log_dir_and_logs_command() {
        let tag = unique_tag();
        let tmp = std::env::temp_dir();
        let bin = tmp.join(format!("no-such-dir-{tag}")).join("xray-core");
        let cfg = tmp.join(format!("cfg-{tag}.json"));
        let log_dir = tmp.join(format!("logs-{tag}"));

        let cp = CoreProc::new(&bin, &cfg, &log_dir);
        assert!(!cp.running());
        assert!(!cp.exited_unexpectedly());

        let msg = match cp.start() {
            Ok(()) => panic!("spawn of a missing binary must fail"),
            Err(e) => e.0,
        };
        assert!(msg.contains("xray-core"), "error must mention the bin: {msg}");
        assert!(log_dir.is_dir(), "start() must create log_dir first");
        let log = fs::read_to_string(log_dir.join("xray.log")).expect("log file");
        assert!(log.contains("run -c"), "header must record the command: {log}");
        let _ = fs::remove_dir_all(&log_dir); // best-effort cleanup
    }
}
