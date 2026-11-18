# S6-REV-B verdict: APPROVE

Reviewer lens: BUILD / PORTABILITY (windows-gnu cross) / DEPENDENCY + LICENSE
HYGIENE / CI + RELEASE PACKAGING / DOCS ACCURACY / TEST QUALITY. Scope:
`git diff d721cba..HEAD` (S6 waves 1-3, 49 files, +15319/−216) read as FULL
files (all 7 new crates, root wiring src/{real_service,profiles,main,lib}.rs,
both workflows, scripts/mingw-zig-shims.sh, Cargo.toml/lock,
THIRD_PARTY_NOTICES.md, README/QUICKSTART), plus independent runs of every
gate. Frontier verified at 6df8767.

Zero P0. Three P1 (release-gate scope regression, stale MSRV claim, stale
THIRD_PARTY_NOTICES). Eight P2 notes + 7 backlog confirmations. All 8 gates
green; all three exes verified PE32+ with the correct subsystems; no
copyleft license anywhere in the 391-package lockfile.

## Gates

All gates run by me, in order, with `CARGO_TARGET_DIR=/tmp/target-rev-b`
(cold dir; removed at the end):

| # | Gate | Result |
|---|------|--------|
| 1 | `cargo fmt --all --check` | **PASS** (no output, exit 0) |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` (host) | **PASS** (exit 0; one non-fatal cargo manifest warning, see P2-2) |
| 3 | `sh scripts/mingw-zig-shims.sh /tmp/shims-b` + `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | **PASS** (exit 0; same non-fatal manifest warning) |
| 4 | `cargo test --workspace` | **PASS** — 321 passed / 0 failed (mt-app 18, mt-engine 4+2 golden+3 lifecycle, mt-links 64, mt-m3 13, mt-net 44, mt-platform 25, root 140 lib + 8 bin + 0 tray, mt-service 0) |
| 5 | `cargo zigbuild --release --target x86_64-pc-windows-gnu` | **PASS** — multitunnel.exe **PE32+ console**, multitunnel-tray.exe **PE32+ GUI** (this command builds the root package only; mt-app.exe comes from gate 6 — verified by deleting mt-app.exe and re-running: not rebuilt) |
| 6 | `cargo zigbuild --release --target x86_64-pc-windows-gnu -p mt-app` | **PASS** — mt-app.exe **PE32+ console** (the iced window app binary) |
| 7 | `cargo check -p mt-app --no-default-features` | **PASS** (the real-service path linked into multitunnel.exe) |
| 8 | `cargo check -p mt-platform --features embedded` | **PASS** (repeated with a real `MT_EMBED_ENGINE=<zip>` fixture: also PASS — the documented release embed path compiles) |

Extra verifications beyond the required gates: `cargo check --features
mock-gui` PASS; `cargo check -p mt-app` (default mock) PASS; workspace YAML
parses (both workflows, `yaml.safe_load`); trigger lines verified **bytewise**
(`od -An -c`: ci.yml `branches: [main]` + `pull_request:`, release.yml
`tags: ["v*"]` — no S4-style phantom). One output-filter scare
(`rg -rn` = replace-with-"n" mangled "smoke" → "n") was self-inflicted and
resolved by re-reading source: `--smoke` exists as documented at
crates/mt-app/src/app.rs:177/5052/5410.

## Findings

### P0 (blocks approval)

None.

- All builds and gates pass (above), including windows-gnu release of all
  three exes with the correct PE subsystems — no host-only dependency found
  (reqwest+rustls+ring compile under the zig shims; iced tiny-skia renderer
  links cleanly with no wgpu in the tree).
- **License sweep: no GPL/AGPL/LGPL/MPL/CDDL/SSPL/EUPL anywhere.** All 391
  external packages in Cargo.lock verified against the actual `license`
  field: 291 from the local registry tarballs
  (`/home/z/my-project/tools/cargo/registry/cache`), the remaining 100
  (wasm/android/objc target-gated deps never extracted on this host) via the
  crates.io API at their **exact locked versions**. Distinct expressions are
  all permissive (MIT, Apache-2.0, BSD-2/3, ISC, Zlib, 0BSD, BSL-1.0,
  Unicode-3.0, Unlicense, CDLA-Permissive-2.0). Two dual/triple-OR crates
  carry a copyleft *option* that we do not elect: `self_cell 1.3.0`
  ("Apache-2.0 OR GPL-2.0-only", pulled by cosmic-text) and `r-efi`
  ("MIT OR Apache-2.0 OR LGPL-2.1-or-later", UEFI-only target) — OR-licensed
  means the permissive term is chosen; no contamination, but the elections
  should be recorded in the notices refresh (folded into P1-3).

### P1 (should fix before release)

1. **NEW — .github/workflows/release.yml:20-25 — release gate still scoped
   to the root package; the 7 new S6 crates are untested at tag time.**
   `cargo fmt --check` (no `--all`), `cargo clippy --all-targets` (no
   `--workspace`) and `cargo test` (no `--workspace`) cover only the root
   package = 148 of the 321 workspace tests; none of mt-links/mt-net/
   mt-engine/mt-m3/mt-platform/mt-service/mt-app's own tests, lints or fmt
   run on a `v*` tag, and the workflow's own comments (S2-REV-B P1-1, "a v*
   tag must never ship an untested build — tag pushes bypass ci.yml") state
   exactly the guarantee that no longer holds — and all seven crates are
   *linked into* the shipped multitunnel.exe. ci.yml's test/lint/cross-check
   legs are all correctly `--workspace`, so the exposure is a tag cut from a
   commit whose main-push CI hasn't completed or was skipped; still a
   regression of a documented invariant. Fix: add `--workspace` /
   `--all` to the three gate lines (cheap, no downside on windows-latest).

2. **NEW — Cargo.toml:8 (echoed at README.md:218) — `rust-version = "1.78"`
   is now false; the effective MSRV is 1.97.** `material-colors 0.5.0`
   (mt-m3's color engine) declares `rust-version = "1.97.0"`; uuid 1.29.3
   needs 1.89, tokio 1.54.1 needs 1.85, iced 0.13.1 needs 1.80. Cargo
   hard-fails dependency rust-version at build time, so a 1.78 toolchain
   cannot resolve the workspace at all — the field (and README's "Stable
   Rust (MSRV 1.78)") is misleading for packagers. The new member crates
   declare no `rust-version` at all. Fix: bump the root field to the true
   floor (≥ 1.97, or simply drop the claim / pin to the toolchain CI uses),
   update README, and consider `workspace.package.rust-version` inheritance
   for the members. Build is green on the sandbox's 1.100.0, so this is
   metadata/doc accuracy, not a broken build.

3. **NEW — THIRD_PARTY_NOTICES.md:14-28 — notices not updated for the S6
   dependency explosion; the file's own rule ("Update it whenever Cargo.lock
   gains a new dependency") is violated by ~375 additions.** The table still
   lists only the v0.2 set (serde/regex/windows-sys/winreg + subcrates).
   Missing entirely: iced + its tree (winit, tiny-skia, softbuffer,
   cosmic-text, iced_*), tokio, reqwest + rustls + ring + hyper + quinn,
   zip/sha2/hex/base64/uuid/url/percent-encoding/thiserror/tracing/
   async-trait/anyhow/futures/ipnet/bytes and everything else new. Also
   missing, non-crate but license-relevant: **iced_graphics 0.13.0 embeds
   `fonts/FiraSans-Regular.ttf` (SIL OFL-1.1) and `fonts/Iced-Icons.ttf`
   compiled into every shipped GUI binary** — OFL is permissive but requires
   its notice on redistribution; and the permissive elections for `self_cell`
   (Apache-2.0 OR GPL-2.0-only → Apache-2.0) and `r-efi` (→ MIT) should be
   recorded (see P0 section — verified, nothing contaminating). The Xray-core
   row's framing ("downloaded by the user … it only launches processes the
   user installed") is also stale for v0.3, where mt-platform downloads and
   installs the engine itself (D14) — downloading from XTLS's releases at
   user trigger is still not redistribution, but the sentence should be
   re-worded. The pre-existing zmij/itoa-URL nit (S5-REV-B P2-7) rides along
   in the same refresh. No license incompatibility exists (P0 sweep above);
   this P1 is the completeness obligation before the v0.3 zip ships the
   notices file into end-user hands.

### P2 (notes)

1. **NEW — .github/workflows/release.yml:25,34-43 — mt-app.exe is never
   built or packaged by the release job, and nothing documents that
   decision.** `cargo build --release` at the workspace root builds only the
   root package (verified empirically: deleting mt-app.exe and re-running the
   equivalent zigbuild does not recreate it), so the MSVC release leg never
   produces the third exe, and the `$required` list / zip contains only
   multitunnel.exe + multitunnel-tray.exe. Functionally the zip is fine: the
   all-in-one GUI is reachable as `multitunnel gui` inside multitunnel.exe
   (real service, mt-app default-features off), and the standalone mt-app.exe
   is MockService-only (crates/mt-app/src/main.rs:14-46) — shipping it as-is
   would give users a fake-backend GUI. But the stage acceptance ("PE32+
   console + tray + gui") is only proven by the sandbox gate + ci.yml's
   *debug* cross-check, never in release config on MSVC. Decide and
   document: either (a) declare mt-app.exe dev-only and keep it out of the
   zip (my recommendation), or (b) build `-p mt-app --release` too and give
   it a real-backend story. One sentence in release.yml/README suffices.

2. **NEW — crates/mt-m3/Cargo.toml:11 — unused `thiserror = "2"` dependency
   (cargo emits a manifest warning on every clippy run).** The crate's own
   docs (crates/mt-m3/src/lib.rs:61) record it as "frozen but currently
   unused". The contract's allowed-deps list permits thiserror but does not
   require it; the warning is noise today but any future cargo promoting
   `cargo::unused_dependencies` past `warn` (or a stricter CI lint pass)
   turns it red. Remove the dep (and the doc sentence), or use it.

3. **NEW — Cargo.lock — thiserror 1.0.69 + 2.0.21 duplication is forced, not
   fixable (informational).** All iced 0.13.x crates, calloop, quinn,
   smithay-client-toolkit, window_clipboard etc. still pin thiserror 1, so
   the workspace's thiserror-2 crates cannot collapse it. Same story for
   base64 0.22/0.23 (mt-links vs reqwest/hyper-util), getrandom 0.2/0.3/0.4,
   redox_syscall ×4, windows-sys 0.52/0.59/0.61 — normal transitive
   duplication, no action available short of downgrades. Answering focus #4
   explicitly: the two thiserror majors are needed.

4. **NEW — docs statements now wrong (docs refresh is a planned separate
   step; listing only what is FALSE, not demanding new v0.3 docs):**
   README.md:8 "One crate, two binaries" (now a workspace of 8 crates / 3
   exes); README.md:169-177 CLI reference omits `import` (stale since v0.2)
   and the new `profiles`/`gui` (the binary's own usage at src/main.rs:563-
   571 is correct); README.md:218 MSRV 1.78 (see P1-2); QUICKSTART.md steps
   2-3 (install net-merge; manually download/place `xray.exe`) now describe
   the legacy v0.2 path — v0.3 bonds in-process (mt-net) and self-provisions
   the engine on first connect (D14); QUICKSTART's zip-contents list (lines
   17-20) still matches release.yml exactly. Also src/main.rs:564 usage
   prints "(none) start the tray app (Windows)" while no-args actually
   errors with a pointer to multitunnel-tray.exe — pre-existing nit.

5. **NEW — harness/CONTRACTS-S6.md:328-329 vs .github/workflows/ci.yml — the
   `--smoke` flag is described as the CI verification of the GUI, but no
   workflow ever runs it.** The smoke *code path* is unit-covered
   (crates/mt-app/src/app.rs run_smoke test; the orchestrator ran
   `multitunnel gui --smoke` manually per LEDGER), but the "run 3 s headless
   then exit 0 (CI)" promise is unwired. Cheap add to the ubuntu test leg
   (`cargo run -q --bin multitunnel -- gui --smoke` or `cargo run -q -p
   mt-app -- --smoke`) to catch GUI-side regressions (view construction,
   theme derive) on every push. Optional, since the in-process test covers
   the same code.

6. **NEW — crates/mt-engine/tests/lifecycle.rs:10 — the whole child-process
   lifecycle suite is `#![cfg(unix)]`,** so spawn/port-up/kill/respawn/
   budget-exhaustion/kill-on-drop never execute on the windows-latest test
   leg (they run on ubuntu, and the CREATE_NO_WINDOW surface compiles on
   windows-gnu clippy). Accepted-by-contract (the fake engine is python3),
   but worth one line in the crate docs' known-limitations so nobody assumes
   Windows CI exercises process supervision.

7. **NEW — .github/workflows/ci.yml:66 — cross-check zigbuilds in debug
   profile only.** The release-profile cross-link (LTO-ish codegen, real
   optimization paths) is proven only in the sandbox gate. Harmless; bump to
   `--release` if a cheap belt is wanted. (Disk/time cost on the runner.)

8. **Backlog confirmations (all unchanged, all in v0.2 root code untouched
   by S6 — parked as accepted):** localhost listen inconsistency (src/
   real_service.rs:589-590 binds fixed 127.0.0.1; sysproxy override literal
   "localhost" at real_service.rs:698 — same shape as v0.2); join_reader
   1 s grace (src/main.rs, untouched); restore-error surfacing (sysproxy,
   untouched); snapshot downgrade doc (sysproxy.rs, untouched); **CI cache
   keying** (Swatinem default keys at ci.yml:19/39/65 + release.yml:17; the
   zig-using legs — lint *and* cross-check — still don't key on zig 0.17.1;
   same theoretical staleness S5-REV-B P2-3 described, no worse); 
   no-snapshot-disable UX; disconnect-latency tray note. Positive deltas
   since S5: release.yml now hard-requires README.md and QUICKSTART.md in
   `$required` (S5-REV-B P2-4 is FIXED, release.yml:34-43); the zmij URL
   nit remains (rides with P1-3).

## Verdict rationale

Every build and lint gate reproduces green on a cold target dir, including
the full windows-gnu matrix (fmt, host clippy, target clippy, tests, release
zigbuild) and both feature-gate corners (mt-app real-service path,
mt-platform embedded with and without a real MT_EMBED_ENGINE zip); all three
exes are PE32+ with the intended subsystems and no dependency is host-only.
The license sweep — every one of the 391 locked packages checked against its
actual license, plus the fonts embedded inside iced_graphics — finds no
copyleft anywhere, so there is no contamination to block on. The three P1s
are all release-hygiene, not correctness: the release workflow's gate scope
silently stopped covering the new crates, the MSRV metadata is now untrue,
and the third-party notices are stale by their own standard — each is a
small, mechanical fix that should land before the v0.3 tag is cut. The
packaging question (mt-app.exe in the zip or not) is a decision to document
rather than a defect, since the shipped multitunnel.exe already carries the
real-service GUI. Nothing here breaks the build, corrupts licensing, or
ships a wrong artifact, so per the D6 criteria (zero P0) I APPROVE, with the
P1s queued for the close-out fixes.
