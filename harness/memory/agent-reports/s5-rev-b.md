# S5-REV-B verdict: APPROVE

Reviewer lens: BUILD / PORTABILITY (windows-gnu vs msvc) / DEPENDENCY+LICENSE HYGIENE /
TEST QUALITY / CI+DOCS ACCURACY. Scope reviewed: `git diff 07ef5c5..40ee65c`
(14 files, +1716/−255) plus the full current state of every touched source file,
both workflows, the server script, the docs, and independent runs of every gate.

No P0. No P1. Eight P2s (doc/robustness nits, all fixable at close or carried).
All 19 contract items verified implemented and tested; all gates reproduce green.

## Findings

### P0 (blocks approval) — file:line + argument + suggested fix

None.

### P1 (should fix before release)

None rose to P1. The closest candidate (snapshot downgrade gap, P2-1 below) was
downgraded: the common-case snapshot round-trips both directions, the failure
mode is non-destructive (v0.1.0 refuses to guess rather than deleting data) and
recoverable by deleting `state/sysproxy.json`.

### P2 (nits)

1. **Snapshot downgrade direction undocumented** — `src/sysproxy.rs:52-56`
   (`SnapValue` doc: "JSON shape (backward compatible with pre-S5 snapshots)")
   and `src/sysproxy.rs:218-227`. The claim is true for the direction it states
   (v0.1.0 file → v0.2.0 binary; verified: the legacy test at
   `src/sysproxy.rs:611` pins the *actual* v0.1.0 on-disk shape — `proxyEnable`
   always a plain number, strings `null`/string, per `git show
   07ef5c5:src/sysproxy.rs` — not a hand-invented format). The reverse
   direction is only partially compatible: a v0.2.0-written snapshot containing
   `"proxyEnable": null` (Absent — new shape; v0.1.0 always wrote a number) or
   any `{"opaqueKind": …}` marker fails v0.1.0's `u32`/`Option<String>` parse
   (`#[serde(default)]` does not rescue a present-but-wrong-typed field), so a
   downgraded v0.1.0 `load_snapshot` errs → `restore` errs → the stale
   snapshot persists and the proxy stays set until the file is deleted. Narrow
   (requires downgrade while a snapshot exists AND ProxyEnable absent/Opaque)
   and recoverable. Suggested fix: one doc sentence on `Snapshot` noting the
   downgrade caveat ("v0.2.0 snapshots containing `null`/`opaqueKind` markers
   will not parse on v0.1.0 — delete state/sysproxy.json after downgrade");
   optionally serialize DWORD `Absent` as `0` (semantically identical:
   `EnableAction::Write(0)`) to keep even that corner downgrade-readable.
   No code change required for approval.

2. **rust-cache on the release tag is effectively a cold cache** —
   `.github/workflows/release.yml:17`. rust-cache's default key includes the
   workflow name ("release" ≠ "ci"), and GitHub cache scoping means caches
   saved on a tag ref are not restorable by later tags (only default-branch
   caches are). Harmless (perf only); the cache step still satisfies contract
   D2 as written ("default key + the workflow name"). Optional: `prefix-key`
   shared with ci.yml, or drop the step from release.yml.

3. **cross-check cache not keyed on the zig version** —
   `.github/workflows/ci.yml:51-56`. rust-cache@v2 stores `target/`
   (including `x86_64-pc-windows-gnu` artifacts) keyed on Cargo.lock + rustc,
   not on the zig 0.17.1 pinned at ci.yml:53. Cargo fingerprints don't include
   the linker, so a future zig bump could in theory serve a stale *linked* exe
   from cache (rustc-produced rlibs are unaffected). rust-cache does NOT touch
   the zig toolchain itself (only cargo home + target — confirmed against the
   action's defaults), so the toolchain can't go stale. Suggested: prefix the
   job's cache key with the zig version or set `cache: false` on this job.
   Theoretical; the cross-check is a sanity leg, not the release artifact.

4. **Packaging hard-required list omits two files the docs promise** —
   `.github/workflows/release.yml:34-49`: `README.md` and `QUICKSTART.md` are
   only best-effort copied (`if (Test-Path …)`), while QUICKSTART step 0 now
   states the zip's "full contents" includes both. They are tracked files, so
   every real tag build packages them; but if either were ever renamed, the
   zip would silently lack a doc that QUICKSTART promises (`fail_on_unmatched_files`
   can't catch it — it matches `dist/*.zip`). Suggested: add both to `$required`.
   (Pre-existing behavior; S5-D's doc change is what makes the promise explicit.)

5. **`join_reader` grace can silently drop captured output** — `src/main.rs:213-222`
   (`READER_GRACE` = 1 s): if a fast-exiting child produced output the reader
   threads haven't drained within 1 s, `run_bounded` returns the output it has
   (empty) and discards the rest. Correct and documented for the `xray version`
   use case (a few hundred bytes drain in ms); a trap if the helper is reused
   for a chatty child. Suggested: note the budget in the `run_bounded` doc, or
   make the grace apply only when a grandchild is suspected.

6. **Slow-child test can orphan a bounded grandchild** —
   `src/main.rs:618-631`: `sh -c "sleep 30"` killed at 250 ms; on hosts where
   `sh` forks rather than execs the single command, the `sleep` survives ~30 s
   (a stray `sleep 30` was observed in the sandbox during my test run — other
   agents may also produce these). Self-terminating, no hang, no scratch files.
   Nit only.

7. **THIRD_PARTY_NOTICES.md zmij row cites itoa's URL** —
   `THIRD_PARTY_NOTICES.md:28`: "itoa, zmij | (serde_json deps) …
   https://crates.io/crates/itoa" — zmij v1.0.23 is a real serde_json dep
   (Cargo.lock:176) and the MIT OR Apache-2.0 license claim is right, but the
   upstream link is itoa's. Pre-existing (file untouched by S5), license-hygiene
   lens nit.

8. **Cargo.lock format silently downgraded v4 → v3** — `Cargo.lock:3`. The only
   lock changes in S5 are the multitunnel 0.1.0→0.2.0 bump and this format
   regression, i.e. the lock was regenerated/edited by something older than the
   sandbox cargo 1.100 (which preserves v4). Harmless — v3 is readable by every
   cargo ≥1.53 (and by the MSRV 1.78 claim), and `git status` stayed clean
   after test/clippy/zigbuild, proving the lock is in sync. No dependency
   additions/removals (diff verified). Informational.

Also observed, outside this lens: the S5 diff rewrites historical LEDGER.md
recovery timestamps (e.g. `harness/memory/LEDGER.md:56-61`, 14:05Z→13:40Z…).
Flagging for the process lens only.

## Contract-item verification (S5-A 1-5, S5-B 1-6, S5-C 1-4, S5-D 1-4)

**S5-A (config.rs, main.rs)** — all 5 verified:
1. ✅ Loopback validation: `ensure_loopback_listen` called in `Config::validate`
   (`src/config.rs:273`); `Config::load` → `validate` (`src/config.rs:178`) so
   generate/doctor/connect all enforce it. Tests:
   `validate_accepts_loopback_listen_values` (127.0.0.1 + localhost),
   `validate_rejects_non_loopback_listen` (message contains the value AND
   "loopback" — matches `supervisor.rs:731-740` wording),
   `validate_rejects_listen_that_is_not_an_ip`.
2. ✅ `--config=PATH` equals form: factored into `split_args`
   (`src/main.rs:75`); 5 tests cover space/equals forms, both spellings,
   missing path, empty `=` value, last-flag-wins, pass-through. Usage text
   updated (`src/main.rs:475-479`) and truthful.
3. ✅ doctor bounded + windowless: `run_bounded` (`src/main.rs:168`) with
   explicit `Duration`; 5 s budget (`main.rs:142`); poll loop `try_wait` every
   50 ms; kill + reap on deadline; CREATE_NO_WINDOW under `#[cfg(windows)]`
   (`main.rs:272-277`) with `CommandExt` imported inside the cfg block — the
   exact `netmerge.rs:144` / `core_proc.rs:178` precedent, so the flag applies
   on BOTH the msvc release leg and the gnu cross leg, and the import can't
   break the unix build. Stdio piped like the old `.output()`; pipes drained on
   threads (no self-deadlock on chatty children). Timeout path prints
   "timed out after 5s and was killed". `cfg(unix)` tests
   `run_bounded_returns_output_for_a_fast_child` /
   `run_bounded_kills_a_slow_child_at_the_deadline` (250 ms budget vs 30 s
   child, elapsed-bounded) — ran green on the host; skipped on the Windows CI
   legs exactly as the contract permits.
4. ✅ `main.rs` `write_private` fully delegated to
   `fsutil::write_private_atomic` (`src/main.rs:130-135`), VLESS-credential
   doc kept; test `write_private_writes_contents_and_leaves_no_temp_file`
   asserts contents + no temp sibling.
5. ✅ proxyOverride verbatim: fallback-to-default branch removed from
   `apply_fallback_defaults` (comment explains the serde container-default
   path, which I traced: absent field → `Behavior::default()` →
   `default_proxy_override()`); 3 tests (absent→default list, set→verbatim,
   ""→stays empty). `config.example.json` ships the full default string (not
   ""), so template users keep the bypass list — and the byte-equality test
   still pins it. Module + field docs updated.

**S5-B (sysproxy.rs, supervisor.rs, core_proc.rs)** — all 6 verified:
1. ✅ Tri-state `SnapValue` (`src/sysproxy.rs:57`): `Absent | Value(T) |
   Opaque{kind}`; `restore` maps Opaque → `Leave` (never delete/overwrite),
   Absent → delete (strings) / write 0 (DWORD, pre-S5 behavior preserved);
   winreg typed reads gated on exact `vtype == REG_SZ/REG_DWORD` so
   REG_EXPAND_SZ/REG_BINARY/unreadable → Opaque, never a false Absent. Mid-set
   half-applied comment present twice (Snapshot doc + inline in `set`). Tests:
   tri-state serde + true legacy v0.1.0 format, per-state restore actions,
   plan mapping, all-opaque round-trip through the file. Forward-compatible
   with real v0.1.0 snapshots (verified against `git show 07ef5c5`). See P2-1
   for the undocumented downgrade corner.
2. ✅ `set` writes ProxyOverride verbatim: `set_string_plan`
   (`src/sysproxy.rs:175`) always includes the override; skip-when-empty branch
   gone; test `set_plan_writes_override_verbatim_even_when_empty`.
3. ✅ supervisor `write_private` → `fsutil::write_private_atomic`
   (`src/supervisor.rs:797`); test `write_private_is_atomic_and_private`
   (contents, 0600 unix, no temp sibling).
4. ✅ IPv6 literals: `parse_probe_target` (`src/supervisor.rs:752`) unwraps
   `[addr]:port` (incl. trimmed, `[::ffff:127.0.0.1]`), returns None for bare
   IPv6 (ambiguity rule, `fe80::1:443` → None) and malformed brackets — 13-case
   table test. `build_connect_request` (`src/core_proc.rs:360`) picks ATYP
   01/04/03 by parsing the host; oversize-domain check preserved. Tests:
   byte-exact per ATYP + wire-level `socks_probe_sends_atyp_matching_host_kind`
   against a capturing stub (verifies the bytes actually sent, not just the
   builder).
5. ✅ Pump interleave: single `Arc<Mutex<File>>` opened once per cycle
   (`open_shared_log`), spawn header + both pumps append through it
   (`pump_pipe_to_log` locks per chunk); silent-drop-on-write-failure behavior
   preserved and documented; open failure now fails `start()` before any pump
   (documented as preserving the old semantics). Test
   `shared_log_handle_keeps_lines_unsplit`: two threads × 50 lines, one line
   per write via a chunked LineReader, asserts 100 intact lines + both pumps
   represented — deterministic, no sleeps.
6. ✅ Exit-race guards: `abort_if_stopped` (`src/supervisor.rs:405`) re-checks
   the stop flag immediately before net-merge (`:371`) and before
   `sysproxy::set` (`:382`); on abort: core stop + best-effort restore + proxy
   flag cleared + neither side effect. `reap_loop_and_clear_stop` at connect
   entry (`:283`, join-before-clear; no deadlock — the state gate guarantees no
   live un-stopped loop when it runs) prevents stale-stop poisoning. Tests:
   two boundary-race tests via a rig whose SOCKS stub flips the stop flag at
   probe completion (deterministic seam; asserts state Failed, error text,
   proxy flag down, bond_error/sys_error clean, merging false) +
   `connect_after_disconnect_is_not_poisoned_by_stale_stop`. All ran green.

**S5-C (tray.rs)** — all 4 verified:
1. ✅ `spawn_detached` logs spawn failures via `log_err` (`src/tray.rs:887-889`),
   return value still ignored.
2. ✅ Icon test pins the asset (`src/tray.rs:1319-1334`): 16×16 DIB biSize 40 /
   width 16 / height 32 (XOR+AND), plus total `ICON_BYTES.len() == 5430` —
   verified against `stat -c %s assets/icon.ico` = 5430; the comment's
   arithmetic (6 + 2×16 + 4264 + 1128, with AND-mask row padding) checks out
   and `scripts/make_icon.py` documents "Deterministic output; stdlib only".
3. ✅ Dump-cleanup: `remove_dir_all` at the end of
   `write_status_dump_creates_dirs_and_overwrites` (`src/tray.rs:1418`).
4. ✅ Exit tooltip: `EXIT_TIP` (`src/tray.rs:129`), one best-effort NIM_MODIFY
   after NIM_DELETE (`src/tray.rs:913-926`), inside the audited ladder without
   touching its ordering/timing; FALSE-after-delete ignored and documented;
   `exit_tip_fits_nif_tip_cap` asserts the NIF_TIP fit + NUL terminator.
   Unsafe audit: 34 `unsafe` blocks / 34 SAFETY comments in tray.rs (rg count),
   +1 block vs pre-S5, no other new unsafe anywhere in the diff; no new raw
   FFI functions (Shell_NotifyIconW already in the audited set), no new
   dependencies.

**S5-D (workflows, server script, docs)** — all 4 verified:
1. ✅ release.yml lints on tag: `components: rustfmt, clippy` added
   (`release.yml:15`); `cargo fmt --check` (`:20`) and
   `cargo clippy --all-targets -- -D warnings` (`:21`) run BEFORE `cargo test`
   (`:24`) and `cargo build --release` (`:25`). Order and component install
   verified; clippy on windows-latest is the MSVC leg.
2. ✅ rust-cache added to ci.yml's three jobs (`:19`, `:35`, `:55`) and the
   release job (`release.yml:17`), default keying, no custom paths, placed
   after the toolchain steps. (Effectiveness caveats: P2-2/P2-3.)
3. ✅ Server config mode: `chmod 0640` + `chown root:nogroup`
   (`server/setup-xray-reality.sh:300-301`) with the verify-note comment
   (`:295-299`, `id -gn nobody` must print nogroup — accurate for Debian/Ubuntu
   where nobody's primary group is nogroup; the unit is `User=nobody` at
   `:322`). With 0640 root:nogroup, nobody retains group-read — the note's
   claim is correct. Final summary line updated (`:389`). Quoting intact on
   all changed lines; `bash -n` clean.
4. ✅ QUICKSTART step 0 lists all 8 zip contents + the `multitunnel-vX.Y.Z`
   folder name (`QUICKSTART.md:13-23`); README zip sentence updated
   (`README.md:253-256`). See crosscheck below — exact match.

## Gate results (tails) + yaml/bash -n results

All gates run independently with `CARGO_TARGET_DIR=/tmp/mt-target-REVB`
(cold target dir, no shared state with implementer runs):

- `cargo fmt --check` → exit 0, no output (clean).
- `cargo test` → `test result: ok. 103 passed; 0 failed; 0 ignored` (lib)
  + `test result: ok. 8 passed; 0 failed` (multitunnel bin: split_args ×5,
  write_private, run_bounded ×2) + `0 tests` (tray bin) + `0` doc-tests.
  Spot-reruns confirm the new tests actually execute:
  `connect_aborts_before_net_merge_when_stop_set_at_boundary`,
  `connect_aborts_before_sysproxy_when_stop_set_at_boundary`,
  `connect_after_disconnect_is_not_poisoned_by_stale_stop`,
  `probe_target_parsing_ipv6`, `snapvalue_serde_tri_state_and_legacy_format` —
  all ok.
- `cargo clippy --all-targets -- -D warnings` →
  `Finished dev profile … in 4.38s`, exit 0.
- `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` →
  `Finished dev profile … in 10.25s`, exit 0 (the gnu leg compiles every
  cfg(windows) change incl. `run_bounded`'s pipe threads and
  `CommandExt::creation_flags`).
- `cargo zigbuild --target x86_64-pc-windows-gnu --release` →
  `Finished release profile [optimized] target(s) in 26.31s`, exit 0; both
  artifacts `PE32+ executable for MS Windows 6.00` — multitunnel.exe
  (console), multitunnel-tray.exe (GUI).
- yaml: `yaml.safe_load` of both workflows → `YAML_OK_BOTH`. (Note: the
  tools/venv python in env.sh lacks PyYAML; used /home/z/.venv/bin/python3 —
  sandbox artifact, not a repo issue. Bracketed YAML content was verified via
  Read/bytewise display rules, not shell echo.)
- `bash -n server/setup-xray-reality.sh` → clean (`BASHN_OK`).
- `git status --short` after all gates → empty (tree clean; Cargo.lock in
  sync — cargo never tried to rewrite the v3 lock).

MSRV note: no 1.78 toolchain exists in the sandbox (only stable 1.100.0) so
MSRV was checked statically: nothing in the S5 diff needs >1.78 (`let-else`
1.65, enum `#[default]` 1.62, `JoinHandle::is_finished` 1.61, const
`Duration::from_secs` — all fine); `rust-version = "1.78"` stays truthful.
Dependency diff: Cargo.toml/lock changes are exactly the 0.2.0 bump + lock
format v3; zero dependency additions/removals; THIRD_PARTY_NOTICES.md
untouched by S5 and still covers the full (unchanged) dependency set (see
P2-7 URL nit).

## Doc-vs-packaging crosscheck (zip list vs QUICKSTART, exact)

release.yml Package step (bsdtar over `dist/multitunnel-$v/`) builds:

| # | workflow packages | QUICKSTART step 0 lists | README sentence |
|---|---|---|---|
| 1 | `multitunnel.exe` (required-check + copy) | ✔ | ✔ |
| 2 | `multitunnel-tray.exe` (required + copy) | ✔ | ✔ |
| 3 | `config.example.json` (required + copy) | ✔ | ✔ |
| 4 | `README.md` (conditional copy) | ✔ | ✔ (self) |
| 5 | `QUICKSTART.md` (conditional copy) | ✔ (self) | ✔ |
| 6 | `LICENSE` (required + copy) | ✔ | ✔ |
| 7 | `THIRD_PARTY_NOTICES.md` (required + copy) | ✔ | ✔ |
| 8 | `server/setup-xray-reality.sh` (required + copy to `$root/server/`) | ✔ (same path form) | ✔ |

Exact 8/8 match, no extras on either side. Folder name: zip root entry is
`multitunnel-$v` (`tar -C dist "multitunnel-$v"`), QUICKSTART says
`multitunnel-vX.Y.Z` with example `multitunnel-v0.3.0` — consistent for a
`v0.2.0` tag. Zip name `multitunnel-<version>-windows-x64.zip` matches the
workflow's `multitunnel-$v-windows-x64.zip` and the released v0.1.0 asset
name pattern. QUICKSTART step 1's `bash server/setup-xray-reality.sh` is
consistent with the script's zip location under `server/`. The only weakness
is items 4-5 being conditional rather than required (P2-4).
