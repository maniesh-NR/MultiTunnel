# S3 Review — Reviewer-C (Correctness & Security; special FFI mandate)

- **Reviewer:** Reviewer-C, adversarial round, stage S3
- **Date:** 2026-11-13, repo HEAD `81c7e72` (S3 landed in `0fb81f0` on base `fe613ab`), branch main, clean tree — no files modified; only this report written.
- **Scope:** line-by-line audit of every `unsafe` block in `src/tray.rs` (+ `src/bin/tray.rs`, `assets/icon.ico`, `scripts/make_icon.py`, lib.rs diff) against the special S3 mandate: FFI correctness, memory/pointer safety, wndproc re-entrancy, races, GDI/user-handle leaks, panic paths in message handlers, Supervisor-lifecycle interaction.

## Verification log (run myself, trusted nothing)

| Gate | Result |
|---|---|
| `cargo test` | **77/77 passed, 0 failed** in 6.05 s (20 new tray tests included) |
| `cargo clippy --all-targets` (host) | **clean** — 0 warnings |
| `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | **clean** — exit 0 (denies warnings on the actual FFI target) |
| `cargo zigbuild --target x86_64-pc-windows-gnu` | **success** — `multitunnel-tray.exe` = **PE32+ GUI subsystem x86-64** (14 sections), console bin unaffected |
| `git show 0fb81f0 --stat` | 6 files: `src/tray.rs` (1353), `src/bin/tray.rs` (106), `assets/icon.ico` (5430 B), `scripts/make_icon.py`, `s3-implementation.md`, lib.rs (+2 doc/module lines only — S1/S2 modules untouched) |
| Icon asset byte-level parse (my own Python walk) | 2 entries: 32×32 @ off 38 len 4264, 16×16 @ off 4302 len 1128; payload math exact (40 + w·h·4 + padded AND-mask = declared len for both); no overlap; file ends exactly at 5430. Directory + DIB headers match the Rust unit-test assertions |
| windows-sys 0.61.2 source inspection (vendored crate) | `WNDPROC = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>` — matches `tray_wndproc`; `CreateIconFromResourceEx(*const u8, u32, BOOL, u32, i32, i32, IMAGE_FLAGS) -> HICON` — matches call; `NOTIFYICONDATAW` on x86_64 is `#[repr(C)]` with `szTip: [u16; 128]` (W variant, unicode) — matches `tip_array`'s `[u16;128]`; `SetTimer(HWND, usize, u32, TIMERPROC)`, `TrackPopupMenuEx(HMENU, u32, i32, i32, HWND, *const TPMPARAMS)`, `Shell_NotifyIconW(NOTIFY_ICON_MESSAGE, *const NOTIFYICONDATAW)` — all match. Constants (GWLP_WNDPROC=-4, HWND_MESSAGE, NIF_*=1/2/4, NIM_*=0/1/2, TPM_*=2/32, MF_*, LR_DEFAULTCOLOR=0, WM_APP=0x8000) all verified |
| Panic-surface grep in `mod win` (lines 361–958) + `bin/tray.rs` | **zero** `unwrap`/`expect`/`panic!`/indexing/slicing |

---

## Unsafe-block audit — 32 blocks, 32 OK

The wndproc itself (`tray_wndproc`, :675, `unsafe extern "system" fn`) is audited separately below; every `unsafe { … }` block carries a `SAFETY:` comment (32/32), and each comment's claim was checked, not just its presence.

| # | Line | API | Verdict & reasoning |
|---|---|---|---|
| 1 | :482 | `DestroyIcon` | OK — only when `icon_owned` (from `CreateIconFromResourceEx`) and non-null; nothing references it (no tray icon was added on this failure path) |
| 2 | :493 | `DestroyWindow` | OK — window created by this thread on this path; no icon registered |
| 3 | :496 | `DestroyIcon` | OK — same guard as #1 on the NIM_ADD-failure path |
| 4 | :508 | `SetTimer` | OK — valid hwnd, fresh id 1, `None` TIMERPROC ⇒ WM_TIMER to the subclassed proc; failure (returns 0) is non-fatal and logged |
| 5 | :523 | `GetMessageW` | OK — `&mut msg` is a valid aligned `MSG` out-slot; NULL hwnd filter; both i32 outcomes (0 = WM_QUIT, −1 = error → teardown + Err) handled |
| 6 | :535 | `TranslateMessage` | OK — `msg` holds a message returned by #5 |
| 7 | :537 | `DispatchMessageW` | OK — same; routes to our subclass; wndproc is panic-free (see below) |
| 8 | :559 | `Shell_NotifyIconW(NIM_DELETE)` (teardown) | OK — identity fields (cbSize/hWnd/uID) set via `nid_base`; guarded by `icon_added` (swap ⇒ idempotent with exit_app's delete); FALSE (already gone) ignored |
| 9 | :564 | `KillTimer` | OK — our timer/hwnd; idempotent; ordered before DestroyWindow |
| 10 | :566 | `DestroyWindow` (teardown) | OK — our message-only window; the defensive WM_DESTROY handler posts one extra (harmless, never-consumed) WM_QUIT |
| 11 | :571 | `DestroyIcon` (teardown) | OK — only when owned; the `LoadIconW` fallback is a SHARED icon and is correctly never destroyed (`icon_owned=false`) — no GDI leak, no shared-icon UAF |
| 12 | :587 | `CreateIconFromResourceEx` | OK — the classic mistake (passing the whole ICO file) is avoided: `ico_image_bytes` returns exactly the RT_ICON payload a directory entry points at, bounds-checked (`checked_add`, `slice::get`, len≠0). Args verified against the windows-sys 0.61.2 signature: `(ptr, len as u32, fIcon=1, dwVersion=0x00030000, 32, 32, LR_DEFAULTCOLOR)`. Null return falls back to #13 |
| 13 | :605 | `LoadIconW` | OK — `IDI_APPLICATION` system shared resource; owned=false so teardown never destroys it |
| 14 | :622 | `CreateWindowExW` | OK — predefined `"STATIC"` system class (no RegisterClass), `HWND_MESSAGE` parent ⇒ message-only, style 0, both wide strings NUL-terminated and owned (`to_wide`); creation-time messages (WM_NCCREATE et al.) are handled by the original STATIC proc before the subclass exists — the install order (create → subclass) is sound |
| 15 | :651 | `SetWindowLongPtrW(GWLP_WNDPROC)` | OK — `tray_wndproc as *const () as usize as isize` is the documented Win32 idiom; the fn signature matches `WNDPROC` exactly (verified in the windows-sys source); `prev == 0` failure path destroys the window and errors out |
| 16 | :664 | `DestroyWindow` (subclass-failure path) | OK — same as #2 |
| 17 | :692 | `PostQuitMessage` (WM_DESTROY) | OK — no preconditions; defensive so an externally destroyed window can't wedge the loop |
| 18 | :712 | `DefWindowProcW` (unmatched tray callback) | OK — parameter passthrough |
| 19 | :717 | `DefWindowProcW` (default) | OK — passthrough. Not calling the saved STATIC proc via `CallWindowProcW` is acceptable for a message-only window (nothing sends static-control messages to it; DefWindowProcW handles generic messages); documented reasoning is accurate |
| 20 | :743 | `Shell_NotifyIconW(NIM_MODIFY)` (on_timer) | OK — `NIF_TIP` + `szTip` = `tip_array` (≤127 UTF-16 units, NUL-terminated, surrogate-pair-safe — see pure-helper analysis); FALSE (icon deleted mid-Exit) ignored; the 2 s exit wait blocks the pump so this cannot even race the delete |
| 21 | :818 | `Shell_NotifyIconW(NIM_DELETE)` (exit_app) | OK — `icon_added` swapped to false first ⇒ idempotent with teardown; icon removed before the (slow) disconnect so no ghost icon even if teardown takes the full budget |
| 22 | :831 | `PostQuitMessage` (exit_app) | OK — posted after the bounded wait; the GetMessageW loop returns 0 and teardown runs |
| 23 | :846 | `CreatePopupMenu` | OK — null-checked with early return |
| 24 | :859 | `GetCursorPos` | OK — writable, aligned `POINT` out-slot; failure degrades to menu-at-(0,0) (P2-6) |
| 25 | :864 | `SetForegroundWindow` | OK — our hwnd; the KB Q135788 pre-menu half (see P2-7 for the message-only caveat that needs on-hardware verification) |
| 26 | :869 | `TrackPopupMenuEx` | OK — fresh valid hmenu; `TPM_RIGHTBUTTON \| TPM_BOTTOMALIGN`, no `TPM_RETURNCMD` ⇒ selection arrives as WM_COMMAND (consistent with the handler); NULL `lptpm`; **modal-loop re-entrancy is safe**: `show_menu` holds no `ui` RefCell borrow and no other shared mutable state across the call (verified by reading every line of show_menu/append_entry) |
| 27 | :882 | `PostMessageW(WM_NULL)` | OK — the Q135788 post-menu half; our window; ignored result is fine |
| 28 | :885 | `DestroyMenu` | OK — unconditional after `TrackPopupMenuEx` returns (whether shown, cancelled, or failed) ⇒ **no HMENU leak on any path**; re-entrant `show_menu` invocations each own their own menu |
| 29 | :895 | `AppendMenuW` (separator) | OK — `MF_SEPARATOR` requires NULL lpNewItem (docs-conformant); id 0 |
| 30 | :901 | `AppendMenuW` (info row) | OK — NUL-terminated owned wide buffer; id 0 + MF_GRAYED ⇒ unselectable; WM_COMMAND with id 0 maps to `None` and is ignored |
| 31 | :916 | `AppendMenuW` (item) | OK — ids 1001–1006 round-trip through `cmd_from_id` (tested); failure logged, menu still destroyed |
| 32 | :950 | `Shell_NotifyIconW(NIM_ADD)` | OK — `cbSize = size_of::<NOTIFYICONDATAW>()` on the verified x86_64 `#[repr(C)]` W variant; `NIF_MESSAGE\|NIF_ICON\|NIF_TIP`; callback `WM_APP+1` (private range, no collision with WM_TIMER/WM_COMMAND/WM_DESTROY); `szTip` NUL-terminated; failure path cleans window + icon and errors out |

**Wndproc panic-freedom (panic across `extern "system"` = abort/UB):** `tray_wndproc` and everything it calls were walked exhaustively: no indexing, slicing, `unwrap`, `expect`, or arithmetic that hostile data can reach (grep-confirmed zero in `mod win`). The two remaining panic-capable constructs are both proven unreachable: (a) `ctx()`'s `RefCell::borrow` — the CTX cell is mutably borrowed exactly once, before the window exists and before any message dispatch, on the same thread; (b) `ui.borrow_mut()` in `on_timer`/`exit_app`/`teardown` — never held across any message-pumping call (`show_menu` never touches `ui`; the exit wait blocks rather than pumps). Supervisor `snapshot()`/locks use the poison-tolerant `lock()` and are contention-brief (workers hold `inner` only momentarily). Interior NULs in net-merge-sourced tooltip text merely truncate display (fixed-size NUL-terminated buffer), no memory-safety impact.

**Threading / op_lock interaction (mandate item 2):** verified — `connect`/`disconnect` run ONLY via `spawn_supervisor_op` workers; nothing in the wndproc or `show_menu` ever calls a supervisor op synchronously. `exit_app`: NIM_DELETE → worker `disconnect()` → `recv_timeout(2 s)` → PostQuitMessage. No deadlock is possible: the message thread holds no lock the worker needs; op_lock holders never wait on the message thread (verified all op paths: spawn_loop/net-merge/registry/core — none send messages to the window). The orphaned-worker-at-exit scenario is bounded and implicitly documented — but its teardown-completion gap is P1-1 below.

**ICO byte-level bounds (mandate item 1):** `ico_entries`/`ico_image_bytes` re-audited line-by-line: `len < 6` reject, reserved/type magic check, per-entry `checked_add(16)` + `end > len` reject, `dim` from directory byte (0 ⇒ 256), `len == 0` reject, `offset.checked_add(len)` then `slice::get` (no panic on any malformed input; no unwrap). The committed asset itself parses byte-perfect (my independent Python walk). `include_bytes!` is a compile-time embed — no runtime path dependency, stable in release builds.

**Supervisor integration (mandate item 5):** menu enablement matches the state machine exactly (Connect: Disconnected/Failed; Disconnect: Connecting/Connected — cancelling an in-flight connect via op_lock, per the S2 contract; snapshot-staleness between menu open and click is harmless because the supervisor state gate arbitrates). Tooltip surfacing `snapshot.error` (implementer's documented deviation) is correct and I endorse it — without it a Failed state would show no reason. The status dump contains no secrets (snapshot fields + net-merge text; no credentials in any supervisor error string — verified in S2). 1 s timer → `NIM_MODIFY` only on text change — no shell churn.

**Bin cross-check (mandate item 6):** elevation flow forwards original args (`-config` survives; quoting hardened in S2); GUI-subsystem stderr writes are all best-effort `writeln!` with ignored errors — **no `eprintln!` panic-on-invalid-handle anywhere**; the missing-config path is file-based (template + `config-needed.txt` beside the exe — visible without a console); config search order (explicit → beside exe → cwd) and template placement logic verified correct.

---

## Findings

### P0 — none
I could not construct any UB, handle leak, deadlock, or reachable panic in the shipped paths. The three highest-risk patterns (wndproc + RefCell + panic-across-FFI; SetWindowLongPtrW subclass; NOTIFYICONDATAW ABI) are all sound by construction and verified against the actual windows-sys 0.61.2 definitions.

### P1 — 2
1. **`src/tray.rs:807-832` (`exit_app`)** — the 2 s exit budget can be exceeded by a *healthy* disconnect: `disconnect()` joins the poll loop, which may legitimately sit inside `socks_probe` (15 s total budget), a restart campaign's `wait_for_port` (15 s), or `netmerge::off`'s CLI timeout (10 s) — or an in-flight `connect` worker may still hold the op_lock (up to ~50 s). When the 2 s expires, the process exits with teardown incomplete: the xray core is **orphaned** (Windows does not kill children on parent exit) and the system proxy stays set — the user believes the app (and tunnel) exited, while traffic keeps flowing through the VM, or browsing is silently routed through a half-dead proxy. Fix suggestion: on timeout, perform an op_lock-free emergency stop — `CoreProc::stop` (own mutexes, no op_lock) plus `sysproxy::restore(state_dir)` — both safe to call from the message thread; or escalate the wait with a visible "disconnecting…" state. At minimum the incomplete-teardown possibility must be documented as more than "best-effort".
2. **`src/tray.rs` (whole module — no `TaskbarCreated` handling)** — if explorer.exe restarts (shell crash, user-initiated restart — not rare), the tray icon is silently lost and never re-added: the process becomes an elevated, unreachable zombie holding the tunnel with no UI and no window (message-only ⇒ not findable); the only exit is Task Manager. Standard fix: `RegisterWindowMessageW("TaskbarCreated")` handled in the wndproc → re-`NIM_ADD` (icon handle and window survive explorer restarts). Not in the ROADMAP spec text, but it is a production-readiness must for any tray app and a small, well-understood addition.

### P2 — 7
1. `src/bin/tray.rs:28,41` + `src/tray.rs:491` — GUI-subsystem errors are invisible: a declined UAC prompt, `NIM_ADD` failure, or any `tray::run` error goes to a best-effort stderr that usually doesn't exist — the user sees nothing happen. Safe (no panic) but a `MessageBoxW` (or writing an error file like the config-note pattern) would make failures diagnosable.
2. `src/tray.rs:797-799` — "Edit config"/"Show raw status" spawn **notepad running elevated**; an elevated editor is the classic admin-shell footgun (its file dialogs can launch arbitrary executables with admin rights). User-initiated and the whole app is elevated by requirement, but worth documenting or delegating to an unelevated broker in a future stage.
3. `src/tray.rs:776-794` — `show_raw_status`/`open_log_folder` do synchronous file writes + `create_dir_all` inside the wndproc (small, but a state dir on slow/network storage would stall the pump); the connect/disconnect worker pattern is the right home for these too.
4. `src/tray.rs:79-87` — `snippet(s, cap)` underflows (`cap - 1`) if ever called with `cap == 0`; unreachable today (sole call site uses 48) but it is a latent panic-in-wndproc if reused — guard or `debug_assert!(cap > 0)`.
5. `src/tray.rs:456` — the "call exactly once" contract for `run` is unenforced; a second call would leak another ctx and create a second window. A cheap `debug_assert`/atomic once-flag would pin the invariant.
6. `src/tray.rs:857-865` — if `GetCursorPos` fails the menu still shows at (0,0) but *without* the SetForegroundWindow/WM_NULL pair (the Q135788 dismissal guarantee). Practically unreachable (the click that opens the menu implies a cursor), but inconsistent; better to skip showing the menu entirely.
7. `src/tray.rs:864` — `SetForegroundWindow` on a **message-only** window: this host cannot verify empirically whether the thread-foreground trick works for HWND_MESSAGE parents (the standard KB Q135788 samples usually use hidden normal windows). If it fails, the menu won't dismiss on outside click (annoyance, not a safety issue). Flagging for the S4 on-hardware smoke test: click the icon, open the menu, click elsewhere — confirm dismissal.

### Positive notes
- The RT_ICON-not-the-file insight (#12), the surrogate-pair-safe `tip_array`, the no-GWLP_USERDATA thread-local design, the Q135788 dance, the `icon_owned` shared-vs-owned distinction, and the idempotent teardown with swap-guarded `icon_added` are all exactly right — this is a carefully done raw-FFI tray.
- The pure-helper/test split (316 pure lines host-tested) is the same pattern that made S1/S2 reviewable, and the 20 new tests genuinely pin the helpers (tip boundary at unit 126, ICO preference order and rejections, menu enablement per state, id round-trip, dump overwrite).
- The icon asset is byte-perfect and exactly the size the format dictates (5430 B); `make_icon.py` is deterministic and dev-only (not shipped).

## Verdict

**APPROVE.** No P0: all 32 unsafe blocks are correct with accurate SAFETY comments (signatures and struct layouts verified against the vendored windows-sys 0.61.2 source, not assumed); the wndproc is provably panic-free; no GDI/user-handle leaks on any path; no deadlock in the op_lock interaction; all four mandatory gates green. The two P1s (exit-teardown completeness, TaskbarCreated re-add) are production-readiness items that should be fixed before the S4 release but do not introduce UB, leaks, deadlocks, or reachable panics; P2-7 needs one on-hardware check during S4 smoke testing.

*Repo untouched (clean tree at `81c7e72`); the only write is this report.*
