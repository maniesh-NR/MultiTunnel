# S3 Reviewer-B Report — BUILD / PORTABILITY / DEPENDENCIES / LICENSING / TEST QUALITY

- Stage: S3 adversarial review, Reviewer-B (build/portability/dependency hygiene/licensing/test-quality lens)
- Repo: /home/z/my-project/MultiTunnel, branch main, HEAD **81c7e72** (working tree clean; `git ls-remote` confirms origin/main == 81c7e72 — the pushed tree is the one verified below). S3 landed in 0fb81f0 on top of S2-close fe613ab.
- Scope: src/tray.rs (1354 lines), src/bin/tray.rs, assets/icon.ico, scripts/make_icon.py, lib.rs (additive), Cargo.toml/Cargo.lock (expected unchanged), CI workflows, +20 tests (57→77).
- No project files modified. Environment writes only: regenerated the icon to /tmp for byte-comparison (cleaned up), ran the host tray stub binary in /tmp (cleaned up), rust-src component already installed in S2 round 2.

## 1. Verification log (all run by me on 81c7e72)

| Command | Result |
|---|---|
| `git show 0fb81f0 --stat` | 6 files: assets/icon.ico (5430 B), s3-implementation.md, scripts/make_icon.py, src/bin/tray.rs, src/lib.rs (+2), src/tray.rs (1353) — **no frozen module touched, no workflow/Cargo change** |
| `cargo test` (run 1) | **77/77 ok**, 5.96 s, 0 ignored |
| `cargo test` (runs 2–3) | 77/77 ok, 6.05 s / 6.05 s |
| `cargo test` under full 2-core load (2 busy-loops) | **77/77 ok**, 6.38 s |
| `cargo test --lib -- supervisor:: core_proc:: tray:: --test-threads=1` (×3) | **37/37 ok ×3** (7.39/7.36/7.27 s) — timing-sensitive subset single-threaded |
| `cargo test --lib -- tray:: --test-threads=1` (×5) | **20/20 ok ×5, 0.00 s each** — the new tests are pure/instant |
| `cargo clippy --all-targets` (forced rebuild after touch) | clean, no warnings |
| `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | **clean** — compiles the entire `mod win` FFI for the target with `-D warnings`; proves every import resolves against windows-sys 0.61.2's mapping |
| `cargo fmt --check` | clean |
| `cargo zigbuild --target x86_64-pc-windows-gnu` (after touch) | green, "Compiling multitunnel v0.1.0" |
| `file …/*.exe` | `multitunnel-tray.exe` **PE32+ GUI**, x86-64; `multitunnel.exe` **PE32+ console** — subsystems correct |
| `objdump -p multitunnel-tray.exe` | imports **Shell_NotifyIconW** (shell32), **CreateWindowExW / AppendMenuW / TrackPopupMenuEx / CreateIconFromResourceEx / SetWindowLongPtrW / GetMessageW** (user32) + wininet/advapi32 (lib sysproxy/elevate) — the tray FFI genuinely compiled and linked for the target |
| `git diff fe613ab..HEAD -- Cargo.toml Cargo.lock` | **empty** — no dependency or manifest change in S3 |
| host tray binary stub (manual, /tmp) | no config → template + `config-needed.txt` written **beside the exe**, exit 2; cwd `config.json` → loaded (cwd fallback), validation error surfaced, exit 2 — file-based error flows confirmed end-to-end |
| `git ls-remote origin main` | 81c7e72 == local HEAD — pushed tree consistent, tests green before push (no 06:19Z-style incident this round) |

**Test stability: 13/13 green runs (5 full incl. 1 under full 2-core load; 3 subset single-threaded; 5 tray-only single-threaded), 0 flakes.**

## 2. Scrutiny items — results

### 2.1 cfg gating & both-sides compilation
Pure helpers are `#[cfg(any(windows, test))]` (the sysproxy/elevate pattern): host non-test builds compile only the non-Windows `run` stub; host test builds compile helpers + 20 tests; Windows builds compile everything incl. `mod win`. No POSIX-only trait inside windows code; no windows API leaks onto the host. `ICON_BYTES` (`include_bytes!`) is cfg'd `any(windows, test)` so even the asset embedding is absent from the host non-test build. Both sides verified by clippy clean on host AND on the windows-gnu target with `-D warnings`. The lib.rs change is purely additive (doc line + `pub mod tray;`).

### 2.2 windows-sys feature list vs APIs used (item 8)
mod win references: Foundation `{HWND, LPARAM, LRESULT, POINT, WPARAM}`; UI::Shell `{Shell_NotifyIconW, NOTIFYICONDATAW, NIF_*, NIM_*}`; UI::WindowsAndMessaging `{AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, KillTimer, LoadIconW, PostMessageW, PostQuitMessage, SetForegroundWindow, SetTimer, SetWindowLongPtrW, TrackPopupMenuEx, TranslateMessage, GWLP_WNDPROC, HICON, HMENU, HWND_MESSAGE, IDI_APPLICATION, LR_DEFAULTCOLOR, MF_*, MSG, TPM_*, WM_*}`. All inside the three already-enabled features (Win32_Foundation, Win32_UI_Shell, Win32_UI_WindowsAndMessaging) — **no new features needed, none added**. `CreateIconFromResourceEx`'s windows-sys 0.61.2 signature matches the call site (verified by the target clippy + link; the crate declares the same mapping for gnu and MSVC, so the windows-latest MSVC leg compiles identically — windows-sys has no target-specific declarations for these). `SetWindowLongPtrW` exists only on 64-bit targets — fine, the project targets x86_64 only. `HWND_MESSAGE`, `IDI_APPLICATION` (PCWSTR), `WM_APP` all resolve. The `-D warnings` cross clippy is the machine proof; the PE import list is the link proof.

### 2.3 GUI-subsystem discipline (item 9)
Grep over src: every `println!`/`eprintln!` lives in main.rs (the console binary) or in test code. The tray paths use only `let _ = writeln!(std::io::stderr(), …)` (bin main + `log_err`) — the S2-verified non-panicking form. Error/first-run flows are file-based: template + `config-needed.txt` beside the exe (verified by running the host binary). `spawn_detached` targets GUI apps (notepad.exe, explorer.exe) so no console flash even from the GUI-subsystem process. Clean.

### 2.4 Custom build tooling — icon (item 6)
- **Deterministic**: stdlib-only (`os/struct/sys`), no RNG, no timestamps, no environment dependence; accepts an output path, so I regenerated to /tmp: `cmp` reports the committed 5430-byte `assets/icon.ico` **byte-identical** to regeneration.
- **Format validity** (independent python parse + `file`): ICONDIR reserved=0/type=1/count=2; entries 32×32 (offset 38, 4264 B) and 16×16 (offset 4302, 1128 B); each payload is a BITMAPINFOHEADER with biSize=40, biWidth=n, biHeight=2n (XOR+AND doubled), planes=1, 32 bpp, BI_RGB, and biSizeImage == 40 + n²·4 + padded-AND — exactly the RT_ICON shape `CreateIconFromResourceEx` consumes; XOR rows bottom-up BGRA; corners fully transparent (AND bit set), center navy (42,27,13,255); file ends exactly at the last image (no trailing bytes). `file(1)`: "MS Windows icon resource - 2 icons, 32x32, 32 bits/pixel, 16x16, 32 bits/pixel".
- **No emoji**: the only non-ASCII bytes in make_icon.py are em-dashes (U+2014) in comments — punctuation, consistent with repo style; the motif is pure geometry (navy rounded square + two link bars), no text/emoji.
- The Rust side pins the 32×32 DIB header in a unit test (biSize=40/biWidth=32/biHeight=64).

### 2.5 Licensing (item 7)
Cargo.toml + Cargo.lock **byte-identical to S2-close** — zero new dependencies, so the S2 notices file remains accurate. The icon is original work generated by the project's own committed script (pure geometry, no third-party artwork) → covered by the project MIT license; scripts/make_icon.py is project source. No copyleft, no contamination. My S2 round-2 follow-ups (serde_core row + full MIT text, windows-target clippy in cross-check, `set_nonblocking(false)` in the core_proc stub) verified present at fe613ab.

### 2.6 Test quality (item 10)
20 new tests, all host-runnable against the pure helpers: state labels (all 4 states), links line (3 modes + 0-link edge), tooltip (clean / error+bond+probe / first-line cap at 48 / blank-skip), tip_array (NUL termination, 127-unit cap with slot-127 reserved, surrogate-pair boundary at unit 126, intact pair), ICO (embedded asset parse + DIB header asserts, preference order exact→fits→largest, rejection: empty/truncated/cursor-type/reserved/count-mismatch/len-0/out-of-bounds), status dump (content, placeholder, create-dirs + overwrite semantics), menu (enablement matrix across 4 states, 11-row structure with separators and labels, links line per config), cmd id roundtrip incl. 0/999/0xFFFF. None `#[ignore]`d (run reports 0 ignored); none vacuous — every one asserts exact strings/indices/bytes. Deterministic: no sleeps, clocks, sockets, or processes; the file-based dump test uses the pid+counter unique-dir pattern. 5/5 single-threaded runs at 0.00 s. All Windows-leg-safe (temp dirs + file IO only). One hygiene nit → P2-1.

### 2.7 CI sanity
- ci.yml test leg (windows-latest, MSVC) compiles the tray module: same windows-sys crate and feature set proven to compile on windows-gnu; no MSVC-vs-mingw declaration divergence for these APIs.
- lint job (host clippy -D warnings) and cross-check (zigbuild + windows-gnu clippy -D warnings, added at S2-close) both cover the new module and pass locally on the exact commands.
- release.yml already packages `multitunnel-tray.exe`; the icon is embedded via `include_bytes!`, so the release zip has no runtime asset dependency. No workflow changes needed for S3 — and none were made.

## 3. Findings by severity

### P0 — none
Cross-build green and machine-proven (target clippy -D warnings + zigbuild + PE imports); no new dependencies; no license change; CI workflows cover the new code on all three legs (host test, windows test, cross lint+build).

### P1 — 1

**P1-1. Menu-dismiss pattern on a message-only window is runtime-unverifiable from this sandbox (tray.rs `show_menu`, :839-886).**
The KB Q135788 fix calls `SetForegroundWindow(hwnd)` before `TrackPopupMenuEx` (return value discarded) so the menu dismisses when the user clicks elsewhere. `hwnd` is a **message-only** window (child of `HWND_MESSAGE`, style 0, never shown). Activation is a top-level-window concept; if `SetForegroundWindow` is a no-op for a message-only child (plausible per its docs — the call activates the window and brings its thread to the foreground), the classic stuck-menu bug returns despite the pattern being present. Nobody has run this tray on real Windows yet, and it cannot be exercised from Linux. Not build/CI-breaking and not UB — but it should be smoke-tested on real Windows before the first release, or the window created as a **hidden top-level** window (no `HWND_MESSAGE` parent, never shown — the pattern .NET NotifyIcon and the tray-icon crate use), which also keeps `Shell_NotifyIconW` callbacks working. Cross-lens flag for the unsafe reviewer to confirm.

### P2 — 4

**P2-1. Temp-dir leak in one new test.** `write_status_dump_creates_dirs_and_overwrites` (tray.rs:1249-1263) never `remove_dir_all`s its unique dir — the only test in the crate that doesn't clean up after itself (the S1/S2 review criterion). One line fixes it.

**P2-2. Exit's 2 s disconnect bound gives up on a legitimate ~15 s teardown.** `exit_app` (EXIT_WAIT=2 s) races the supervisor op_lock/poll-loop join that can legally take up to ~15 s; on timeout the process exits mid-disconnect, potentially leaving the system proxy set and net-merge on until the next launch (step-0 self-heal covers it, and the bound is a deliberate anti-hang). Documented trade-off — consider a brief "shutting down…" tooltip update or a slightly larger bound; behavior is safe, just occasionally untidy.

**P2-3. Embedded-asset test under-pins the icon.** `ico_embedded_asset_parses_with_32_entry` asserts the 32×32 DIB header only; a drifted or hand-edited icon.ico would still pass. Cheap hardening: also assert the 16×16 DIB header and the total file length (5430) so any divergence from make_icon.py's deterministic output is caught (a full byte-compare needs python and can't live in cargo test).

**P2-4. `spawn_detached` silently drops spawn failures** (`let _ = Command::…spawn()`) for notepad/explorer — both are inbox Windows components, so this is a nit; a `log_err` on failure would make a missing-shell case diagnosable.

## 4. Verdict

**APPROVE** — 0 P0. All mandatory gates pass on my own runs (77/77 ×5 incl. full-load, 37/37 subset ×3 single-threaded, 20/20 tray ×5, host + windows-gnu clippy `-D warnings` clean, fmt clean, zigbuild green, both PEs with correct subsystems, tray FFI proven linked via import table). No new dependencies, no licensing change, deterministic committed tooling with a byte-identical regeneration, and 20 genuinely assertive host-runnable tests. The single P1 is a runtime smoke-test recommendation (message-only window + SetForegroundWindow) that should be resolved on real Windows before the first release — it does not affect the build, CI, or any host-verifiable behavior.
