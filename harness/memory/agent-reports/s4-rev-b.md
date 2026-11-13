# S4 Reviewer-B Report — SHIP / RELEASE-READINESS AUDIT (build / release / portability / licensing)

- Stage: S4 adversarial review, Reviewer-B. Stage S4 = Ship: `server/setup-xray-reality.sh` + `README.md` + `QUICKSTART.md` (commit 56340a5). This is the last gate before tagging **v0.1.0**.
- Repo: /home/z/my-project/MultiTunnel, branch main, HEAD **56340a5**, working tree clean. S4 diff: exactly the 4 new files (+1027 lines) — no src/, .github/, Cargo.*, config.example.json change (verified: `git diff 06ad6aa..56340a5 -- Cargo.toml Cargo.lock .github/` is empty).
- Context verified: the S3 review round closed with my P1 (message-only window) fixed at 799f6cd — hidden TOP-LEVEL `WS_OVERLAPPED` window, checked `SetForegroundWindow` return, `TaskbarCreated` re-registration (REV-C P1), `Supervisor::emergency_stop` + the 78th test (op-lock-free exit teardown).

## 1. Verification log (all run by me on 56340a5)

| Command | Result |
|---|---|
| `cargo test` | **78/78 ok**, 5.95 s, 0 ignored (config 9, core_proc 7, elevate 4, netmerge 16, supervisor 11, sysproxy 6, tray 20, xrayconf 5) |
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets` (forced rebuild) | clean |
| `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | **clean** — full windows cfg surface (incl. tray FFI) type-checked |
| `cargo zigbuild --target x86_64-pc-windows-gnu` (debug) | green |
| `cargo zigbuild --release --target x86_64-pc-windows-gnu` | **green** (1m23s) — release-profile cross build, the closest local analog of the release job |
| `file …/debug/*.exe` and `…/release/*.exe` | `multitunnel.exe` **PE32+ console**, `multitunnel-tray.exe` **PE32+ GUI** in BOTH profiles (release: 3.20 MB / 3.18 MB) |
| `git show 56340a5 --stat` | QUICKSTART.md 153+, README.md 291+, s4-implementation.md, server/setup-xray-reality.sh 437+ — docs+script only |
| `bash -n server/setup-xray-reality.sh` | clean; script is 0755 in-tree |
| link liveness | github.com/net-merge/net-merge → 200; github.com/XTLS/Xray-core/releases → 200 (maintained upstreams, links in QUICKSTART accurate). api.github.com is 403 rate-limited from this IP — exactly the condition the script's redirect fallback exists for |
| no-arg console binary (host build) | prints `error: tray lands in stage S3 — build multitunnel-tray`, exit 1 (→ P1-2) |
| packaging paths | `config.example.json`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `README.md`, `QUICKSTART.md` all exist at repo root NOW; exes are built by the workflow's own `cargo build --release` |

## 2. Release-readiness audit

### 2.1 release.yml trace — tag `v0.1.0` pushed hypothetically NOW
windows-latest → checkout (Cargo.lock committed → reproducible deps) → stable toolchain → **`cargo test`** (65 tests on MSVC, see 2.7) → `cargo build --release` (both bins at `target/release/`, default target dir on the runner — paths in the Copy-Item match) → package: unconditional copy of `multitunnel.exe`, `multitunnel-tray.exe`, `config.example.json`, `LICENSE`, `THIRD_PARTY_NOTICES.md` (all exist — a missing one would fail the step loudly, which is the desired behavior), guarded copy of `README.md` + `QUICKSTART.md` (both exist → included) → `Compress-Archive` → `dist/multitunnel-v0.1.0-windows-x64.zip` → upload-artifact → softprops release attach with `fail_on_unmatched_files: true` (the glob will match). **No missing path, no silent-degradation path: the release would build, test, package and attach successfully.** One content gap → P1-3 (server script not in the zip).

### 2.2 ci.yml on the tag push — is release-only testing enough?
ci.yml does not run on tags (`branches: [main]`); release.yml carries `cargo test`. fmt/clippy are NOT run on the tag. Judgment: **acceptable** — in this project's flow every main push is CI-green (test on both OSes + lint + cross-check incl. windows-target clippy), and tags are cut from main HEAD; the release job re-runs the full suite on the exact tag commit, so the artifact is always *tested*. The residual (a tag of a never-pushed commit ships unlinted code) is a lint-regression risk, not a functional one → P2-1 optional belt-and-braces.

### 2.3 The zip a Windows user downloads
Contents: `multitunnel.exe`, `multitunnel-tray.exe`, `config.example.json`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `README.md`, `QUICKSTART.md`. Client-side self-sufficient: QUICKSTART step 3 links XTLS/Xray-core releases (`Xray-windows-64.zip` — correct asset name; contains `xray.exe`, `geoip.dat`, `geosite.dat`; both maintained upstreams, no unmaintained sources) and step 2 links net-merge (live repo). The `bin\xray.exe` default matches `paths.xray` resolution (exe-dir-relative), and the geo-dat-next-to-exe note matches xray's asset lookup. **Gap: QUICKSTART step 1's server script is NOT in the zip** — a zip-only user must fetch the repo (no direct raw URL is given) → P1-3. Trivial nit: step 0 lists five items, the zip actually also carries README/QUICKSTART (P2-3).

### 2.4 MSVC-vs-gnu parity (shipped = MSVC, local proof = gnu)
Risk judged **low**: windows-sys 0.61.2 and winreg are the same crate versions with identical FFI declarations for both ABIs (win32 is a stable C ABI; windows-link import shims are toolchain-agnostic); the gnu side is proven by zigbuild (debug + release) + clippy `-D warnings` + PE import inspection; the MSVC side compiles and runs 65 tests in CI. Rust MSVC release binaries carry the CRT statically (no vcruntime DLL prerequisite). No printf/CRT-dependent behavior exists in the code paths. The genuinely untested-until-now part — the tray UI at runtime — is exactly what QUICKSTART step 6 turns into the documented first-run smoke test (with 7 checkpoints and a "report it" escape hatch). Acceptable for v0.1.0 with that backstop.

### 2.5 Licensing chain for the release artifact
- Cargo.toml/Cargo.lock byte-identical since S2-close → the S2-round-2-verified THIRD_PARTY_NOTICES.md (all 19 dep crates, permissive MIT / MIT-OR-Apache-2.0 / MIT-OR-Unlicense; full MIT text) is still current. **Zero new dependencies in S3/S4.**
- The zip redistributes only project code (MIT) + statically-linked permissive crates + docs. Xray-core (MPL-2.0) and net-merge (MIT) are downloaded by the user from their own upstreams (QUICKSTART links; server script downloads from XTLS releases) — never bundled → **no copyleft contamination**. The setup script similarly downloads rather than embeds.
- The tray icon is original generated artwork (project's own committed script) → project MIT. No third-party assets.
- SmartScreen/unsigned-binary warning: covered in README (Security notes) and QUICKSTART step 6.1. UAC/admin expectations documented in both.

### 2.6 Docs build claims
`cargo build --release` produces both binaries (proven locally in the release profile via zigbuild); the zigbuild cross instructions match the exact commands this harness runs; MSRV 1.78 matches Cargo.toml (verified exact in S2 r2: code max 1.70, deps ≤1.71, lockfile v4 ⇒ 1.78); "Linux build compiles stubs" is true; CI description matches the actual workflows (test matrix, lint, cross-check incl. windows-target clippy); harness pointer present. Version: Cargo.toml `0.1.0` ↔ planned tag `v0.1.0` ↔ zip name `multitunnel-v0.1.0-windows-x64.zip` — consistent. **Two doc-accuracy defects → P1-1 (restartMax semantics), P1-2 (stale S3 string vs README description).**

### 2.7 Windows-leg test count and safety
Host runs 78. `#[cfg(unix)]`-gated tests (exhaustively enumerated by multiline grep): supervisor 6 (op-lock wait, emergency stop, 4 rig tests), core_proc 2 (start-twice, drip deadline), netmerge 7 (process tests) = **15 excluded**; `#[cfg(windows)]`-only sysproxy tests: 2 (mapping helpers). **Windows leg = 78 − 15 + 2 = 65.** Safety: all 65 are pure / temp-dir file IO / loopback binds; the two loopback-binding core_proc stub tests carry the S2-round-2 `set_nonblocking(false)` hardening; every timing-sensitive test (drip 350–900 ms window, op-lock 300 ms, start-twice <1 s, emergency-stop <2 s, core-death 15 s deadline, 20 s recv bounds) is unix-gated, so the Windows leg carries no timing risk. Windows runners handle temp dirs, loopback binds, and the file-IO dump tests without issue (same tests have been green on the windows-latest CI leg since S2). **Low risk of a Windows-leg failure at tag time.**

### 2.8 Server script (portability/lens notes; deep security is REV-C's)
`set -euo pipefail`; validation-before-root; no curl|bash (download → `unzip -t` → extract → smoke test → `install -D`); mktemp 0700 with EXIT trap; ETXTBSY handled (stop service before replacing binary); `xray run -test` gate on the generated config; both `xray x25519` output generations parsed; JSON built from charset-validated variables (no injection through DOMAIN/DEST/keys); GitHub-API rate-limit fallback to the `releases/latest` redirect (verified live: this sandbox's IP is currently rate-limited on api.github.com — the fallback path is real-world exercised); pipefail/SIGPIPE analyzed for every `sed | head` pipeline (each sed emits at most one line → no SIGPIPE-141 under `set -e`). GNU-isms (`sed s///I`, `install -D`) are unreachable on non-systemd hosts (earlier clean die "targets Ubuntu/Debian"). Config written 0644 root:root because `User=nobody` must read it → P2-2 (0640 root:nogroup would be tighter). systemd unit hardening (NoNewPrivileges/ProtectSystem=strict/ProtectHome/PrivateTmp) as documented in README.

## 3. Findings by severity

### P0 — none
The tagged release would not fail: every workflow path exists, both exes build and test green on the exact commands CI uses (host + windows-gnu + release profile), licensing is clean, the zip is complete for the client side, and the Windows CI leg carries no identified failure risk.

### P1 — 3

**P1-1. README config table inverts `restartMax` semantics — ships in the zip.**
README.md:153 — "`behavior.restartMax` | 5 | core restart budget after failures **(0 = unlimited)**". Actual behavior: `restart_campaign` (supervisor.rs) treats `max == 0` as *give up immediately — never restart* (the S2 contract: "restart_max (0 = never)"), and `apply_fallback_defaults` (config.rs:174-212) deliberately does NOT 0-fallback `restartMax`, so a JSON `0` reaches the supervisor as "never". Also the blanket claim at README.md:155-156 ("numbers set to `0` fall back to these defaults on load") is false for this one field. A user setting `restartMax: 0` expecting unlimited retries gets a fail-fast tunnel that disables the system proxy on the first core death. Fix the table line to "0 = never restart (fail immediately)" and scope the 0-fallback sentence.

**P1-2. Stale stage-3 string in the shipped console binary contradicts the README.**
`multitunnel` with no args prints `error: tray lands in stage S3 — build multitunnel-tray` and exits 1 (verified against the built binary). README.md:170 documents "(none) print a pointer to multitunnel-tray (the tray app is the GUI binary)". In v0.1.0, "stage S3" is meaningless jargon for an end user and the wording/behavior doesn't match the docs that ship next to it. One-string fix in `src/main.rs:49-51` (frozen file — the implementer flagged it for orchestrator arbitration; as release reviewer I escalate: fix before tagging).

**P1-3. The release zip lacks `server/setup-xray-reality.sh` — QUICKSTART step 1's prerequisite.**
release.yml:23 copies exes + config.example.json + LICENSE + notices (+ guarded README/QUICKSTART). QUICKSTART step 1 tells the user to "copy `server/setup-xray-reality.sh` there (or fetch the repo)" — a zip-only user has no script and no direct URL. One line (`Copy-Item server/setup-xray-reality.sh` after creating the dir, or adding `server/setup-xray-reality.sh` to the unconditional list) makes the zip self-sufficient for the whole 10-minute path; alternatively add the raw-GitHub URL to QUICKSTART step 1. Fix before tagging (re-tagging after the fact is worse than one more line now).

### P2 — 3

**P2-1. release.yml runs `cargo test` but no fmt/clippy on the tag.** Acceptable today (tags cut from CI-green main; the artifact is fully tested), but adding the two lint steps to the release job (~4 lines) closes the "tag of an untested commit ships unlinted" residual permanently.

**P2-2. Server config file is world-readable (0644 root:root) and holds the REALITY private key.** Deliberate (the service runs as `User=nobody` and must read it; documented), but `install -m 0640 -o root -g nogroup` (Ubuntu's nobody group) is tighter while staying service-readable. Note: `nobody`'s group is `nogroup` on Debian/Ubuntu — worth a quick verification on the target VM before switching.

**P2-3. QUICKSTART step 0 under-lists the zip contents** (5 items; README.md and QUICKSTART.md are also inside). Trivial; also consider mentioning that extracting yields a `multitunnel-v0.1.0` folder (Compress-Archive keeps the directory) so users know what "extract into C:\Tools\MultiTunnel" produces.

## 4. Verdict

**APPROVE** — 0 P0. The release pipeline is mechanically sound end-to-end (verified path-by-path, plus a local release-profile cross build as the closest analog of the runner's build), licensing is clean with zero dependency drift since the audited S2 state, and the Windows CI leg (65 tests) carries no identified failure risk. The three P1s are documentation/shipping-completeness fixes that do not affect whether the release builds and attaches — but all three ship user-visible wrongness in v0.1.0 (an inverted config-table claim, stale stage jargon in the binary, and a missing step-1 prerequisite in the zip), and each is a one-to-three-line fix. **Recommendation: apply the three P1s, re-run the gates, then tag.**
