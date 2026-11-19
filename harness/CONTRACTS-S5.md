# S5 contracts — v0.2.0 P2 backlog (per-agent, fixed by the orchestrator)

Scope: clear the carried P2 backlog from S2–S4 reviews. Every item below
traces to a recorded reviewer finding. File ownership is DISJOINT per agent;
shared interfaces are pre-staged by the orchestrator (fsutil.rs atomic
writes; supervisor::ensure_loopback_listen now pub(crate)) and MUST NOT be
re-designed by agents.

## Common rules (all agents)

- Repo `/home/z/my-project/MultiTunnel`, branch `main`. Never `git commit`
  or push — the orchestrator backs up after every agent round.
- FIRST: `. /home/z/my-project/tools/env.sh`
- Gates (all must pass before reporting done):
  `cargo fmt` · `cargo test` ·
  `cargo clippy --all-targets -- -D warnings` ·
  `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
  (run the target leg whenever you touch any `cfg(windows)` code or a file
  compiled for both targets).
- SANDBOX OUTPUT FILTER: tool output rewrites bracketed literals — a literal
  `[main]` DISPLAYS as `ain]`. Never "fix" bracketed text based on
  cat/rg/echo output; verify bytes with `od -An -c FILE | head`. Files on
  disk and git objects are NOT affected.
- Never read, print, or move anything under `upload/` (credentials live
  there). No new dependencies without orchestrator approval.
- Keep doc comments truthful: every behavior change updates its doc. New
  behavior = new test. No test may touch the real registry, network, or
  leave files outside `std::env::temp_dir()` scratch dirs it removes itself.
- Report format: files touched, per-item completion, tests added
  (names), gate output tail, deviations/notes.

## S5-A — config + main CLI (files: `src/config.rs`, `src/main.rs`)

1. **Loopback validation in `Config::validate`** (S2-REV-B P2-B;
   `config.rs:234`): call `crate::supervisor::ensure_loopback_listen`
   (pre-staged `pub(crate)`, `supervisor.rs:662`) on `self.local.listen`
   inside `validate()` so `generate`, `doctor` (both go through
   `Config::load` → `validate`) and `connect` enforce contract #2 uniformly.
   Add tests: `127.0.0.1` ok, `localhost` ok, `192.168.1.5` rejected with a
   message naming the listen value, `not-an-ip` rejected.
2. **`--config=PATH` equals form** (S2-REV-C P2-9; `main.rs:23`): accept
   `-config=PATH` and `--config=PATH` in addition to the space-separated
   form. Update the usage text. Tests for both forms (pure parsing — factor
   the parse into a testable fn if the current inline loop is not testable).
3. **doctor `xray version` bounded + windowless** (S2-REV-C P2-8;
   `main.rs:168`): replace unbounded `.output()` with a bounded run:
   spawn, then poll `try_wait` on a short slice up to a 5 s deadline; on
   timeout, kill and report `[fail] ... timed out`. On Windows add
   `CREATE_NO_WINDOW (0x0800_0000)` via `creation_flags` (precedent:
   `netmerge.rs:144`, `core_proc.rs:168`). Structure the wait as a helper
   taking an explicit `Duration` so a test can exercise the timeout path
   with a tiny budget against a deliberately slow child (host-side, e.g.
   `sh -c` sleep; keep it `cfg(unix)`-gated if needed for the windows leg).
4. **`main.rs` `write_private`** (`main.rs:105`): delegate the whole body to
   `crate::fsutil::write_private_atomic` (pre-staged; keep the fn + its doc
   about the VLESS credential, delete the duplicated truncate logic).
5. **Empty `proxyOverride` semantics, config side** (S2-REV-C P2-8/10;
   `config.rs:200` area): today an empty string silently falls back to the
   default list, so the override can never be emptied. New semantics: the
   configured value is used VERBATIM — empty string means "no local
   exceptions" and is a legal, meaningful choice; absence keeps the default.
   Update the doc comment, `config.example.json` comment context if any, and
   tests: default when the field is absent; exact value when set; empty
   stays empty. (S5-B makes `sysproxy::set` write it verbatim.)

## S5-B — runtime files (files: `src/sysproxy.rs`, `src/supervisor.rs`, `src/core_proc.rs`)

1. **Snapshot typing — no silent deletes** (S2-REV-C P2-5; `sysproxy.rs:229-232`):
   replace `.ok()` snapshot reads with a tri-state per value:
   `Absent | Value(T) | Opaque` (present but not the expected type, e.g.
   `REG_EXPAND_SZ`/`REG_BINARY` `ProxyServer`). `restore` must NEVER delete
   an `Opaque` value (leave it untouched — that is the user's data);
   `Absent` deletes as today; `Value` writes back. Add the mid-`set`
   half-applied comment (ProxyEnable=1 set, ProxyServer not yet —
   self-heals via snapshot on disconnect). Tests with a mock key if the
   module has one; otherwise pure tri-state transition tests.
2. **`sysproxy::set` writes `ProxyOverride` verbatim** (`sysproxy.rs:239-241`):
   remove the skip-when-empty branch — an empty override is written as an
   empty string (WinINET: no exceptions), matching S5-A's config semantics.
   Test: set() path with empty override writes the value (host stub or
   pure-logic level as the module allows).
3. **`write_private` atomic** (S2-REV-C P2-6; `supervisor.rs:698`): delegate
   to `crate::fsutil::write_private_atomic` like S5-A does in main.rs.
4. **IPv6 probe literals** (S2-REV-C P2-7; `supervisor.rs:677`
   `parse_probe_target`): support bracketed IPv6: `[::1]:443` →
   (`::1`, 443), `[2001:db8::1]:80` → (`2001:db8::1`, 80). Unbracketed
   IPv6 without a port stays `None` (ambiguous). Existing forms unchanged:
   `example.com:80`, `127.0.0.1:1080`. ALSO extend `CoreProc::socks_probe`
   (`core_proc.rs:272-345`): pick ATYP by parsing the host — `Ipv4Addr` →
   ATYP 0x01 (4 bytes), `Ipv6Addr` → ATYP 0x04 (16 bytes), else domain
   ATYP 0x03 (today it always sends 0x03, which is wrong for literals).
   Tests: parser table + probe request bytes for all three ATYPs (the
   module's existing request-building tests show the pattern).
5. **Pump interleave** (S2-REV-C P2-4; `core_proc.rs:391` `pump_to_log`):
   the stdout and stderr pump threads append to the same log through
   separate handles; `write_all` on one handle can split a line written by
   the other. Serialize appends: both pumps share one
   `Arc<Mutex<File>>` (open once, append mode) so each write is exclusive;
   keep the existing silent-drop-on-open-failure behavior documented.
   Test: two threads pumping interleaved lines through the helper; assert
   every line in the log is one of the written lines unsplit (run
   single-threaded or with small line counts to keep it deterministic).
6. **Exit re-set micro-race** (S4-consensus carried backlog;
   `supervisor.rs:277-348` `connect_sequence`): an in-flight
   `connect_sequence` that passed its last stop check can still apply
   `net-merge on` + `sysproxy::set` AFTER `emergency_stop` restored the
   proxy during Exit (the ~1 s re-set window). Fix: re-check
   `shared.stop` immediately BEFORE the net-merge step and again
   immediately BEFORE `sysproxy::set`; if set, abort the sequence with a
   clear error and DO NOT apply either. Test: with stop pre-set at the
   right moment (design the seam so it is testable — e.g. an optional
   hook/parameter or by pre-setting stop and asserting the sequence errors
   before the proxy flag flips in the snapshot).

## S5-C — tray polish (files: `src/tray.rs` only)

1. **`spawn_detached` logs failures** (S3-REV-B P2-4; `tray.rs:874`):
   replace the silent `let _ = ...spawn()` with a failure path through the
   module's `log_err` (`tray.rs:466`) — spawn Result mapped, error logged,
   return value still ignored (fire-and-forget stays).
2. **Icon test pins the asset** (S3-REV-B P2-3; `tray.rs:1272`): extend
   `ico_embedded_asset_parses_with_32_entry` to also assert the 16×16 DIB
   header (biSize 40, width 16, height 32) and the TOTAL embedded
   ICON_BYTES length. Determine the true byte length of
   `assets/icon.ico` with `od -An -c` / `stat -c %s` (output-filter rule!)
   and pin that exact number with a comment that make_icon.py's output is
   deterministic.
3. **Status-dump test cleans up** (S3-REV-C P2-1; `tray.rs:1349`):
   `write_status_dump_creates_dirs_and_overwrites` must
   `remove_dir_all` its unique dir at the end (pattern: `tray.rs:1118`).
4. **Exit tooltip (optional, small)** (S3-REV-B P2-2): while `exit_app`
   waits (normal then emergency ladder), one best-effort
   `NIM_MODIFY` tooltip update to "Shutting down…" so the user sees the
   wait is intentional. Skip if it risks the audited exit ladder — say so
   in the report; do NOT change the ladder's timing or ordering.

## S5-D — ship hygiene (files: `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `server/setup-xray-reality.sh`, `QUICKSTART.md`, `README.md` zip-list section only)

1. **release.yml lints on tag** (S4-REV-C P2-1): add `cargo fmt --check`
   and `cargo clippy --all-targets -- -D warnings` steps to the release job
   BEFORE build/test (rust-toolchain already installs components? — check
   the job's toolchain step and add `components: rustfmt, clippy` if
   needed).
2. **CI caching** (S3-REV-C P2-7): add `Swatinem/rust-cache@v2` to ci.yml
   jobs and the release job. Keep it simple: default key + the workflow
   name; no custom paths.
3. **Server config mode** (S4-REV-C P2-2; `server/setup-xray-reality.sh`):
   the generated xray config holds the REALITY private key — install it
   `0640 root:nogroup` instead of 0644 root:root (Debian/Ubuntu `nobody`
   group is `nogroup`), with a short comment noting the service runs as
   `User=nobody` and that the group must be verified on the target VM.
   Keep `bash -n` green; update the script's self-checks if they assert
   the mode.
4. **QUICKSTART step 0 zip list** (S4-REV-B P2-3): list ALL zip contents
   (both exes, config.example.json, README.md, QUICKSTART.md,
   THIRD_PARTY_NOTICES.md, server/setup-xray-reality.sh) and note that
   extraction yields a `multitunnel-vX.Y.Z` folder. Update the README
   sentence that describes the zip contents too (only that sentence).

## Orchestrator-owned (not agent work)

- `Cargo.toml` version → 0.2.0 + lockfile refresh, after agents land.
- Branch protection / required status checks: attempted via API at close;
  documented as a user step if the token lacks admin rights.
- Gates, review rounds, consensus file, tag v0.2.0, release verification,
  download/ artifact restoration, memory updates, backups.
