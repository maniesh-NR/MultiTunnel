# S3 Implementation — Tray (notification-area UI)

- **Implementer:** S3 agent (implementation round, stage S3)
- **Date:** 2026-11-13, repo base fe613ab (S2 closed), branch main
- **Scope:** new `src/tray.rs` (+`pub mod tray;` in lib.rs), real GUI binary
  `src/bin/tray.rs`, new `assets/icon.ico` + `scripts/make_icon.py`. No frozen
  S1/S2 module was modified (lib.rs change is the additive module decl + one
  doc line; main.rs untouched).

## What was built

### 1. `src/tray.rs` (1353 lines: ~316 pure / ~602 win32 / ~376 tests / stub)

**Pure helpers** (`#[cfg(any(windows, test))]`, all host-tested — the
sysproxy/elevate pattern):

| Helper | Purpose |
|---|---|
| `state_word` / `state_line` | lifecycle state → menu/tooltip wording |
| `links_line` | `Links: N (merging)` / `Links: bonding off` / `Links: unavailable` |
| `snippet` | first line of an error, capped, with ellipsis |
| `tooltip_text` | state line + error/bond/probe snippets (blank errors skipped) |
| `tip_array` | NIF_TIP-safe `[u16;128]`: ≤127 UTF-16 units + NUL, never splits a surrogate pair across units 126/127 |
| `ico_entries` / `ico_image_bytes` | ICO directory parse + RT_ICON entry selection (exact → largest-fits → largest overall; empty/out-of-bounds rejected) |
| `status_dump` / `status_dump_path` / `write_status_dump` | snapshot pretty-JSON + raw net-merge status → `state_dir/net-merge-status.txt` |
| `Cmd` / `MenuEntry` / `menu_plan` / `id_for_cmd` / `cmd_from_id` | full menu as a pure plan; Connect/Disconnect enablement per state |

**Windows implementation** (`mod win`, all FFI through windows-sys 0.61.2,
**no new Cargo features** — only the already-enabled
Win32_Foundation/Win32_UI_Shell/Win32_UI_WindowsAndMessaging):

- **Window**: `CreateWindowExW` with the predefined `"STATIC"` system class,
  `HWND_MESSAGE` parent, style 0 (message-only, never shown) — no
  RegisterClass. Subclassed with `SetWindowLongPtrW(GWLP_WNDPROC)`;
  unhandled messages → `DefWindowProcW` (the STATIC proc only matters during
  creation, before the subclass is installed).
- **Tray icon**: `Shell_NotifyIconW` NIM_ADD with NIF_MESSAGE|NIF_ICON|NIF_TIP;
  callback = `WM_APP+1`; tooltip refreshed by `SetTimer` 1 s → WM_TIMER →
  NIM_MODIFY only when the text changed (redundant updates skipped).
- **Menu**: rebuilt FRESH from `supervisor.snapshot()` at every open via
  `CreatePopupMenu`/`AppendMenuW`/`TrackPopupMenuEx(TPM_RIGHTBUTTON |
  TPM_BOTTOMALIGN)`/`DestroyMenu`; KB Q135788 pattern
  (SetForegroundWindow before, WM_NULL after) so the menu dismisses when
  clicking elsewhere.
- **Icon embedding**: `include_bytes!("../assets/icon.ico")` →
  `ico_image_bytes(.., 32)` (RT_ICON payload, NOT the directory) →
  `CreateIconFromResourceEx(.., 0x00030000, 32, 32, LR_DEFAULTCOLOR)`;
  fallback `LoadIconW(NULL, IDI_APPLICATION)` (shared, never destroyed —
  tracked by `icon_owned`).
- **Handler state**: `thread_local RefCell<Option<&'static TrayCtx>>` with a
  `Box::leak`'d context — no GWLP_USERDATA pointer juggling; handlers copy
  the `&'static` out and drop the borrow guard immediately, so re-entrancy
  during the `TrackPopupMenuEx` modal loop is safe.
- **Threading contract honored**: `connect`/`disconnect` ALWAYS run on
  `std::thread` workers (they hold the supervisor op_lock, up to ~15 s);
  nothing blocking ever runs in the window procedure.
- **Exit**: NIM_DELETE first (no ghost icon), best-effort disconnect on a
  worker with a 2 s bounded wait (`mpsc::recv_timeout`), then
  `PostQuitMessage`; idempotent teardown (NIM_DELETE/KillTimer/
  DestroyWindow/DestroyIcon-if-owned) after the loop. Defensive `WM_DESTROY →
  PostQuitMessage` so an externally-destroyed window can't wedge the loop.

### 2. `assets/icon.ico` + `scripts/make_icon.py` (142 lines, stdlib-only, deterministic)

32×32 + 16×16 BMP-format ICO entries (BITMAPINFOHEADER with doubled
biHeight, bottom-up 32-bpp BGRA XOR rows, 1-bpp AND mask with 4-byte row
padding). Motif: dark-navy `#0D1B2A` rounded square with two parallel link
bars `#4FC3F7` (sky) over `#4ADE80` (emerald) — bonded links, pure geometry,
no text/emoji. Entry layout validated byte-by-byte (biSize=40, biWidth=n,
biHeight=2n, comp=BI_RGB) and by a Rust unit test.

### 3. `src/bin/tray.rs` (108 lines) — real GUI binary

`#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]` kept.
Elevation check first (Windows: `!is_elevated()` → `relaunch_elevated(args)`
+ exit 0, original args ride along so `--config` survives). Config: explicit
`-config/--config` arg → beside the exe → cwd fallback; when nothing exists,
template lands beside the exe plus a friendly `config-needed.txt` note
(GUI-app equivalent of the console bin's message) and exit 2. Then
`Arc<Supervisor>` + `tray::run`. Non-Windows: stub `run` returns Err →
stderr line + exit 2, same as the pre-S3 stub.

## Key FFI decisions (for the unsafe reviewer)

1. **RT_ICON bytes, not the file**: `CreateIconFromResourceEx` requires the
   image bytes an ICONDIRENTRY points at; passing the whole ICO file is the
   classic mistake. `ico_image_bytes` extracts+bounds-checks the slice; a
   unit test pins the 32×32 entry's header (biSize=40/biWidth=32/biHeight=64).
2. **STATIC + GWLP_WNDPROC subclass** instead of RegisterClass (fewer moving
   parts, per the stage spec). The previous proc pointer is checked non-null;
   we intentionally call `DefWindowProcW` (not `CallWindowProcW` on the saved
   STATIC proc) for unhandled messages — a message-only window never uses
   static-control behavior after creation.
3. **No GWLP_USERDATA**: state flows through a thread-local `&'static`
   (single window, single message thread). Eliminates the classic
   create-then-SetWindowLongPtrW race and null-deref windows entirely.
4. **Re-entrancy**: `TrackPopupMenuEx` runs a modal loop that re-enters the
   window procedure (WM_TIMER during the menu, etc.) — no `RefCell` borrow
   guard is ever held across it (guards close inside `ctx()`/small blocks).
5. **Icon ownership**: `CreateIconFromResourceEx` HICON is destroyed in
   teardown; `LoadIconW` fallback is shared and never destroyed
   (`icon_owned` flag).
6. **`NOTIFYICONDATAW` init**: struct literal with `..Default::default()`
   (all-zero is the valid Win32 "unused" state); `cbSize` set from
   `size_of`.
7. Every one of the 32 `unsafe` blocks carries a `SAFETY:` comment; error
   paths use `std::io::Error::last_os_error()` (thread-safe errno) rather
   than raw `GetLastError`.

## Test / gate results (all run in-session)

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets` (host) | 0 warnings |
| `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | 0 warnings |
| `cargo test` | **77/77 passed** (was 57; +20 tray tests, ~6 s) |
| `cargo zigbuild --target x86_64-pc-windows-gnu` | both bins link |
| `file .../multitunnel-tray.exe` | **PE32+ GUI subsystem, x86-64** (console bin still PE32+ console) |

New tests: state labels (4 states), links line 3 modes, tooltip
clean/snippets/first-line-cap/blank-skip, tip_array NUL/cap-127/surrogate
pair (incl. boundary at unit 126), ICO embedded-asset parse + DIB header
assertions, ICO preference order (exact → fits → largest), ICO rejection
(bad magic/cursor type/reserved/truncated/len-0/out-of-bounds), status dump
content/placeholder/overwrite, menu enablement per state, menu structure
(11 rows, separators, labels), cmd id roundtrip. Host stub behavior of the
binary verified manually (missing-config template + note beside exe + exit 2;
cwd fallback config loads; Windows-only stub message + exit 2).

## Deviations / notes

- **Tooltip also surfaces the top-level `snapshot.error`** (in addition to
  the specified bond/probe snippets): a Failed state without it would show no
  reason at all. Same snippet/first-line/cap rules; documented in helper docs.
- **`Links: unavailable`** = enabled + not merging (disconnected or bonding
  down) — matches the three-way spec wording; merging with zero parsed links
  renders `Links: 0 (merging)` (count is the truth, never invented).
- Menu placement flags: `TPM_RIGHTBUTTON | TPM_BOTTOMALIGN` (bottom-align is
  standard tray behavior so the menu grows up from the cursor).
- Left-click and right-click both open the menu (task says "on the icon
  click"; no separate double-click action specified).
- `scripts/make_icon.py` committed alongside `assets/icon.ico` per task;
  output is deterministic (re-running reproduces byte-identical file).
- Raw net-merge status in the dump comes from the supervisor snapshot's
  `raw_status` (last poll) rather than shelling out to `net-merge status`
  at click time — a 10 s CLI call would block the menu handler.
- P2 backlog from S2 (config loopback check, temp+rename writes, etc.)
  untouched — out of S3 scope.
