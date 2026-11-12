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
pub struct CoreProc;
