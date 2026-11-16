# S5-REV-C verdict: APPROVE

Reviewed `git diff 07ef5c5..40ee65c` (S5 work) on top of full-file reads of every
touched module, the pre-S5 baseline (`git show 07ef5c5:…` / `827e655:…` for the
v0.1.0 snapshot format), all 5 gates, and 3× full-suite flake runs. S5-B (the
agent that crashed before reporting) got the hardest scrutiny per instructions:
every one of its six items was re-derived from the contract and the code, not
from any implementer claim.

No P0s. One P1 (contract-mandated design gap with an exotic trigger — should fix
at close), seven P2s. Sandbox output-filter rule respected: the icon byte
length was verified with `stat -c %s` (5430), not with filtered cat/rg output.

## Findings

### P0 (blocks approval)

None.

### P1 (should fix before release)

1. **`sysproxy.rs:153-154, 418-421, 440-445` — Opaque tri-state leaves OUR
   values, not the user's, in place after a connect/disconnect cycle.**
   Two facets of one root gap (the contract's "Opaque → leave untouched, that
   is the user's data" ignores that `set()` overwrites those very values
   first):
   - **(a) Silent proxy-enable flip via type-mismatched `ProxyEnable`** — the
     exact scenario the review brief called out. With `ProxyEnable` present as
     a non-DWORD (e.g. REG_SZ `"0"`, written by some third-party proxy
     switchers): `set()` unconditionally writes DWORD 1 (`sysproxy.rs:418`);
     `snap_dword` records `Opaque` (`:344-348`); `restore` takes
     `EnableAction::Leave` (`:444`) and writes nothing. Result: after
     disconnect, `ProxyEnable` is still DWORD 1 pointing at the now-dead local
     HTTP inbound — every WinINET consumer is blackholed until the user
     manually disables the proxy. This is a *regression vs v0.1.0*, where the
     failed typed read snapshotted `0` and restore wrote 0 (disabled =
     fail-safe). Failure scenario is concrete; trigger is rare (Windows itself
     always writes DWORD there).
   - **(b) Exotic-typed REG_SZ values are destroyed by `set()` itself** — for a
     REG_EXPAND_SZ/REG_BINARY `ProxyServer`/`ProxyOverride`/`AutoConfigURL`,
     the unconditional `reg_set_string` loop (`:419-421`) replaces the user's
     value with our REG_SZ string before restore ever runs; `Opaque → Leave`
     then preserves *our* tunnel address, not the user's data. The S2-REV-C
     P2-5 goal is only half-achieved (restore no longer deletes — but `set`
     already clobbered).
   **Suggested fix:** `EnableAction::Opaque → Write(0)` (restores the pre-S5
   fail-safe; the user's original bytes are unrecoverable at that point anyway)
   plus either refuse-to-run in `set()` when a value it must overwrite is
   Opaque, or snapshot raw `winreg::RegValue` bytes+type and write them back on
   restore. Note: the contract text mandated `Leave` for Opaque without
   distinguishing `ProxyEnable`, so this is a contract gap, not an
   implementer deviation — but it is a real correctness regression vs v0.1.0
   and should be fixed before v0.2.0 ships.

### P2 (nits)

1. **`supervisor.rs:371,382` — residual check-then-act window in the exit-race
   fix.** The gates narrow the ~1 s re-set window to the gate→side-effect span,
   but a stop landing *after* the net-merge gate still lets `netmerge::on`
   complete up to `BOND_WAIT` = 15 s after an `emergency_stop` restored the
   proxy (and `sysproxy::set` for ~ms). Suggested: re-check `stop` *after* each
   side effect and best-effort undo (`sysproxy::restore` / `netmerge::off`).
2. **`sysproxy.rs:193-216` — `save_snapshot` is not atomic** (create_new +
   write_all, no fsync/rename). A crash mid-write leaves a truncated
   `sysproxy.json`; `load_snapshot` then errors and every caller drops the
   error (`let _ =` at `supervisor.rs:304,335,410,494,550,653`), so all future
   restores silently no-op — proxy stays enabled across reboots until the file
   is hand-deleted. Reuse the `fsutil` temp+fsync+rename pattern; log restore
   errors somewhere visible.
3. **`supervisor.rs:728` + `core_proc.rs:249,286` — `listen: "localhost"`
   passes `ensure_loopback_listen` (and now `Config::validate`) but
   `wait_for_port`/`socks_probe` parse `"localhost:10808"` as a `SocketAddr`
   → connect always fails at step 2 with "invalid address".** Pre-existing
   inconsistency (not an S5 regression); consider resolving localhost at load
   or rejecting with a precise message.
4. **`main.rs:213-222` — `join_reader` discards already-collected output when
   the 1 s grace expires** (grandchild inheriting the pipe), and the TimedOut
   path detaches readers entirely (documented). Bounded, no hang possible;
   doctor may show an empty version line. `main.rs:178` uses unchecked
   `Instant::now() + timeout` (safe with the fixed 5 s constant; `core_proc`
   uses `checked_add`).
5. **`core_proc.rs:39-41,420-423` — pump doc wording slightly
   overpromises.** The mutex makes each `write_all` exclusive, but a logical
   line spanning two pipe reads can still be interleaved with the other
   pump's write (log cosmetics only; per-stream order and all bytes
   preserved). Soften "can never split/interleave lines" to per-write
   exclusivity.
6. **`fsutil.rs:40-44` — the AlreadyExists retry deletes the temp
   unconditionally**; two concurrent same-path writers in one process could
   remove each other's in-flight temp (unreachable today: single-writer call
   sites — `generate` is single-threaded, supervisor writes under op_lock).
7. **Minor hygiene:** (a) downgrade incompatibility — a v0.2.0 snapshot
   containing `{"opaqueKind":…}` fails to parse under v0.1.0 ("corrupt proxy
   snapshot") → an old binary won't restore it (forward compat was not
   required; worth a release-notes line). (b) `tray.rs:910-925` — the exit
   tooltip can only render when the preceding NIM_DELETE failed; in the normal
   path the icon is already gone so nothing shows (in-code documented; ladder
   untouched as the contract required). (c) `QUICKSTART.md` uses
   "e.g. `multitunnel-v0.3.0`" as the folder example while the pending release
   is v0.2.0 — mildly confusing.

## Contract-item verification

### S5-A (config + main CLI)

1. **Loopback validation in `Config::validate` — VERIFIED.**
   `config.rs:273` calls `ensure_loopback_listen` inside `validate()`;
   `Config::load` → `validate` covers generate/doctor/connect uniformly
   (connect keeps its own in-sequence check). Tests:
   `validate_accepts_loopback_listen_values` (127.0.0.1 + localhost),
   `validate_rejects_non_loopback_listen` (192.168.1.5, message names the
   value AND "loopback"), `validate_rejects_listen_that_is_not_an_ip`.
   (See P2-3 for the pre-existing localhost/SocketAddr tension.)
2. **`--config=PATH` equals form — VERIFIED.** `split_args`
   (`main.rs:75-101`) accepts space and both equals forms, errors on missing/
   empty path, last flag wins; usage text updated (`main.rs:475-478`); five
   parsing tests incl. edge cases.
3. **doctor `xray version` bounded + windowless — VERIFIED.** `run_bounded`
   (`main.rs:168-197`): explicit `Duration`, 50 ms `try_wait` slices, kill+
   reap on timeout, pipes drained on reader threads (no chatty-child false
   timeout); CREATE_NO_WINDOW added for Windows (`main.rs:270-276`); timeout
   reported as `[fail] … timed out and was killed`. Unix tests exercise both
   the fast-child and the 250 ms-budget-kills-sleep-30 paths.
4. **`main.rs` `write_private` → fsutil — VERIFIED.** `main.rs:134-136`
   delegates; credential doc retained; test asserts exact contents + no temp
   sibling left.
5. **Empty `proxyOverride` semantics — VERIFIED.** Absent field → serde
   container default (full LAN bypass list); explicit value verbatim; explicit
   `""` stays `""` (`config.rs:184-188` documents the deliberate
   non-defaulting). Three dedicated tests + golden-string default test.

### S5-B (runtime files — the agent that never reported; re-derived from scratch)

1. **Snapshot tri-state, no silent deletes — VERIFIED with one P1.**
   Checked: `SnapValue` untagged serde round-trips all three states distinctly
   (`null` / plain value / `{"opaqueKind":…}`); malformed markers are errors,
   never silent Absent; `str_action`/`enable_action`/`string_write_plan`
   mappings (Absent→delete/0, Value→write, Opaque→Leave) with an all-Opaque
   snapshot test incl. file round-trip; mid-set half-applied comment present
   (Snapshot doc + set site). **Backward compatibility verified against the
   REAL v0.1.0 on-disk format** (read `git show 827e655:src/sysproxy.rs`: old
   struct = `proxyEnable: u32` always a number + `Option<String>` →
   null/string for the other three; same field names/casing): the legacy test
   (`snapvalue_serde_tri_state_and_legacy_format`) parses exactly that shape,
   and old-number→Value / old-null→Absent reproduce v0.1.0 restore behavior
   exactly — it is not a self-referential roundtrip. **P1-1 above** is the
   gap: `Opaque ProxyEnable` → proxy left ENABLED after restore (silent
   flip), and `set()` clobbers exotic-typed REG_SZ values so `Leave`
   preserves our data, not the user's.
2. **`ProxyOverride` verbatim incl. empty — VERIFIED.** No skip-when-empty
   branch remains in `set`; `set_string_plan` (`sysproxy.rs:175-180`) always
   emits the override; test `set_plan_writes_override_verbatim_even_when_empty`.
3. **supervisor `write_private` → fsutil — VERIFIED.** `supervisor.rs:796-799`
   delegates; test `write_private_is_atomic_and_private` (contents, unix 0600,
   no temp sibling).
4. **IPv6 probe literals + ATYP dispatch — VERIFIED.** `parse_probe_target`
   (`supervisor.rs:752-777`): `[::1]:443`/`[2001:db8::1]:80` unwrap correctly,
   bare IPv6 (even port-looking tails) → None, legacy host:port forms
   unchanged; 16-case test table incl. malformed brackets.
   `build_connect_request` (`core_proc.rs:360-378`) is byte-exact: IPv4 →
   `05 01 00 01` + 4 octets, IPv6 → `05 01 00 04` + 16 octets, domain →
   `05 01 00 03` + 1-byte len (≤255 enforced), port big-endian; reply drain
   reads 4+2 / 1+len+2 / 16+2 per ATYP with unknown-ATYP error (no length
   confusion). Verified both as pure byte tests AND end-to-end against a
   capturing stub for all three ATYPs
   (`socks_probe_sends_atyp_matching_host_kind`).
5. **Pump interleave — VERIFIED.** One `Arc<Mutex<File>>` opened once per
   cycle in `start()` (header + both pumps); per-chunk lock with no nested
   locks anywhere in the module (deadlock impossible); open-failure path
   checked against pre-S5: the header open was already fatal then, so
   failing `start()` on the single shared open is *identical* prior behavior,
   and write failures still silently drop as documented; one handle per cycle
   (freed at pipe EOF after kill/reap) — restart campaigns cannot accumulate
   handles; test `shared_log_handle_keeps_lines_unsplit` (2 joined threads ×
   50 lines, every line intact, both pumps represented). P2-5 notes the doc
   wording nuance.
6. **Exit re-set micro-race — VERIFIED.** `abort_if_stopped` gates BOTH
   windows: immediately before net-merge (`supervisor.rs:371`) and immediately
   before `sysproxy::set` (`:382`); abort stops the core (idempotent),
   restores the proxy best-effort, clears `system_proxy`, and errors with a
   boundary-naming message; the two boundary tests (9a/9b) set the stop flag
   deterministically *during the probe* (stub flips it before the reply —
   guaranteed visible at both gates) and assert coherent aftermath (Failed,
   error set, not merging, bond_error/sys_error clean, proxy flag down). Stop
   lifecycle: `connect()` reaps+joins the previous loop and clears the flag
   BEFORE the sequence (`:283`, `reap_loop_and_clear_stop` `:470-476`) and
   again in `spawn_loop`; test (10) proves a normal second connect after a
   disconnect works (stale stop not poisoning). disconnect/emergency_stop
   interleavings walked: op_lock serializes connect/disconnect; emergency_stop
   is the designed lock-free path; a post-emergency connect reaps the dying
   loop (bounded by one probe/port window) and clears the flag. Residual
   check-then-act window: P2-1.

### S5-C (tray polish)

1. **`spawn_detached` logs failures — VERIFIED.** `tray.rs:887-891` maps the
   spawn Result through `log_err` (`:476`, stderr writeln, never eprintln);
   fire-and-forget preserved.
2. **Icon test pins the asset — VERIFIED.** 16×16 DIB header asserted (biSize
   40, width 16, height 32) and total length pinned at 5430; true byte length
   confirmed with `stat -c %s assets/icon.ico` = **5430** (output-filter rule);
   the comment's arithmetic (6 + 2·16 + 4264 + 1128, AND-mask rows padded to
   32 bits) checks out.
3. **Status-dump test cleans up — VERIFIED.** `tray.rs:1416-1418`
   `remove_dir_all` at the end.
4. **Exit tooltip — VERIFIED (additive, with caveat).** EXIT_TIP NIM_MODIFY
   inserted between the NIM_DELETE and the disconnect worker; diff-confirmed
   the ladder's ordering/timing untouched; SAFETY comments remain 1:1 with
   unsafe blocks — counted 34 `unsafe {` vs 34 `SAFETY:` (was 33/33; the diff
   adds exactly one of each); `exit_tip_fits_nif_tip_cap` test. Caveat (P2-7b):
   it can only render when the NIM_DELETE failed.

### S5-D (ship hygiene)

1. **release.yml lints on tag — VERIFIED.** `cargo fmt --check` +
   `cargo clippy --all-targets -- -D warnings` added BEFORE test/build;
   toolchain step now installs `components: rustfmt, clippy`.
2. **CI caching — VERIFIED.** `Swatinem/rust-cache@v2` on all three ci.yml
   jobs and the release job, default keying, no custom paths.
3. **Server config mode — VERIFIED.** 0640 root:nogroup + the
   User=nobody/`id -gn nobody` verify note; `bash -n` green; the script's
   summary line updated; its own `xray run -test` runs as root during setup so
   it can still read the file.
4. **QUICKSTART zip list — VERIFIED.** All 8 contents + folder-name note; the
   README sentence matches the ACTUAL release.yml Package step (verified it
   really copies server/setup-xray-reality.sh into the zip). P2-7c: the
   "multitunnel-v0.3.0" example reads oddly for the v0.2.0 release.

Cross-cutting: no test touches the real registry (sysproxy tests are pure
helpers + temp dirs; windows-only registry code is cfg-gated and never
exercised by tests) or any non-loopback network; all stub listeners use the
quit-flag + nonblocking-accept pattern with bounded joins (the S2 hang class);
no panics on garbage input found in non-test code (poison-tolerant locks, no
unwrap/expect outside tests); `netmerge.rs`/`elevate.rs`/`xrayconf.rs`/
`error.rs`/`lib.rs`/`src/bin/tray.rs` untouched by S5 as expected.

## Gate results (tails)

- `cargo fmt --check` → clean, exit 0 (no output).
- `cargo test` →
  `test result: ok. 103 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.06s`
  (lib) · `test result: ok. 8 passed; 0 failed; … finished in 0.25s`
  (multitunnel bin) · `0 passed` (tray bin) · `0 passed` (doc-tests).
- `cargo clippy --all-targets -- -D warnings` →
  `Checking multitunnel v0.2.0 … Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 4.08s` (zero warnings).
- `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` →
  `Checking multitunnel v0.2.0 … Finished \`dev\` profile … in 9.99s` (zero warnings).
- `cargo zigbuild --target x86_64-pc-windows-gnu --release` →
  `Finished \`release\` profile [optimized] target(s) in 26.87s`; both artifacts
  verified PE32+ (`multitunnel.exe` console 3,243,008 B, `multitunnel-tray.exe`
  GUI 3,210,752 B).

## Flake check (3x suite runs, timings)

| run | wall (incl. build) | suite time (lib) | result |
|-----|--------------------|------------------|--------|
| 1 | 18.5 s (cold compile) | 6.06 s | 103 + 8 passed |
| 2 | 6.5 s | 6.11 s | 103 + 8 passed |
| 3 | 7.1 s | 6.66 s | 103 + 8 passed |

No flakes, no hangs — every run finished far inside the 300 s kill threshold;
the slowest individual tests are the bounded supervisor lifecycle tests
(core-death/give-up, exit-race boundaries), all deterministic by design.
