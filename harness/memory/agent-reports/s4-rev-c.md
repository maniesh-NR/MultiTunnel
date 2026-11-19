# S4 Review — Reviewer-C (Correctness & Security; server script + docs)

- **Reviewer:** Reviewer-C, adversarial round, stage S4 (Ship)
- **Date:** 2026-11-13, repo HEAD `56340a5` (S4 = `56340a5` on base `06ad6aa`), branch main, clean tree — no files modified; only this report written.
- **Scope:** `server/setup-xray-reality.sh` audited as hostile root-running software; `README.md` + `QUICKSTART.md` cross-checked line-by-line against `src/main.rs`, `src/config.rs`, `src/tray.rs`, `config.example.json`, `.github/workflows/*`; implementer claims re-proven where possible.

## Verification log (run myself, trusted nothing)

| Gate | Result |
|---|---|
| `cargo test` | **78/78 passed, 0 failed** in 5.95 s (77 at S3 close + 1 from the S3 P1 fixes) |
| `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | **clean** — exit 0 |
| `bash -n server/setup-xray-reality.sh` | **clean** |
| `git show 56340a5 --stat` | exactly 4 new files (script, README, QUICKSTART, impl. report); no src/workflow/Cargo/config.example.json change |
| Script validations re-proven by me (unprivileged runs) | no-DOMAIN → clean `[setup] ERROR:` exit 1; JSON-injection DOMAIN (`a".evil`) → rejected; no-dot / leading-dot / trailing-dot / leading-hyphen DOMAIN → rejected; PORT `abc`/`0`/`65536`/`-1` → rejected (`08` parses as decimal 8 — no bash integer error); DEST port `0`/`99999`, bare host, `host:name`, malformed `[bad v6]` → rejected; valid `[::1]:443` and `127.0.0.1:0443` pass to the root check; INSTALL_ROOT relative/whitespace → rejected; `FORCE=2` → rejected; stray positional arg → rejected; `--help` → exit 0 |
| `xray x25519` parse re-proven | both claimed output formats extracted correctly by the script's seds: new `PrivateKey:` / `Password (PublicKey):` **and** old `Private key:` / `Public key:` (private + public both correct); capture class `[A-Za-z0-9_-]+` matches base64url keys and cannot admit quotes → no config injection |
| Latest-tag parse re-proven | compact JSON → `v26.11.9`; spaced JSON → `v25.12.30`; **rate-limit body (no `tag_name`) → empty → the `^v[0-9]…` guard dies loudly** — garbage can never be installed |
| `xray run -test` heredoc validation | **skipped** — the implementer's /tmp xray tooling was cleaned up (per their report) and no binary remains on this host; the script's built-in `-test` gate (config written → validate → **die before the service is ever restarted** if invalid) plus my byte-level heredoc review stand in for it |
| Icon/asset n/a; `config.example.json` | server block field names AND order match the script's client block exactly (`address, port, uuid, sni, publicKey, shortId`) — paste-safe |

---

## Script audit (root-running software standard)

**`set -euo pipefail` discipline — holds.** Every expansion is double-quoted; all interpolations into heredocs are charset-validated before use (DOMAIN `[A-Za-z0-9.-]` with edge rules, PORT all-digits + range, DEST regex `host:port`/`[v6]:port` + port range, INSTALL_ROOT absolute + whitespace-free, keys `[A-Za-z0-9_-]+`, shortId/uuid generator-produced) → **no JSON or ExecStart injection is possible** (re-proven with a `a".evil` DOMAIN attempt). The three `cmd | head -n 1` pipelines either output ≤1 line (no SIGPIPE possible under pipefail) or are `|| true`-guarded (`xray version | head`); command-substitution failures land in `TAG`-empty → guarded die. `WORKDIR="$(mktemp -d)"` + `chmod 0700` + `trap cleanup EXIT` → temp dir always removed.

**No curl|sh; download failure handled.** Zip → temp file → `-s` empty check → `unzip -t` integrity → extract exactly `xray geoip.dat geosite.dat` → per-file existence checks → `chmod 0755` → `xray version` smoke test (catches wrong architecture loudly) → `install -D` into `$INSTALL_ROOT`. `curl -fSL --retry 3 … || die` on the download. The GitHub-API rate-limit case degrades to the `releases/latest` redirect, and both paths terminate in the `^v[0-9]…` tag guard — re-proven above.

**Credentials.** `xray uuid`; `xray x25519` parsed for BOTH output generations (re-proven above); `openssl rand -hex 8` shortId (64 bits — adequate for REALITY's anti-probe salt). The private key is never printed (client block carries `publicKey` only; the `log` line prints uuid/shortId which are client-side values anyway) and never leaves the VM.

**REALITY config validity.** Field set (`show:false, dest, xver:0, serverNames, privateKey, shortIds`) is the current documented shape; `dest` (not the legacy `target` spelling) is what current Xray accepts; VLESS `decryption:"none"` + client `flow:"xtls-rprx-vision"` correct; and the script hard-fails via `xray run -test` **before** touching the service if anything is wrong. Existing config + no FORCE → refusal with regeneration warning (no silent credential rotation). (Implementer's real-binary `-test` pass is credible; tooling no longer present here — noted, not re-run.)

**systemd unit.** Sound for Ubuntu 22.04+: `User=nobody` + `Environment=XRAY_LOCATION_ASSET=$ASSET_DIR` (assets 0644 in 0755 dirs — readable), `ExecStart` path from whitespace-validated INSTALL_ROOT (systemd takes it verbatim — no shell, so no metachar risk), `Restart=always`+`RestartSec=3`, `NoNewPrivileges/ProtectSystem=strict/ProtectHome/PrivateTmp` (xray needs no writes outside stdout → journald). `daemon-reload` → `enable` (guarded) → `restart` → `sleep 2` → `is-active --quiet`; on failure dumps 30 journal lines and dies naming the state — a crash-looping service reads as not-active, so the check cannot pass a broken endpoint. A bind failure (e.g. port taken) exits xray → crash-loop → caught. Good.

**Firewall guidance.** Non-destructive: `iptables -I INPUT -p tcp --dport $PORT -j ACCEPT` (insert, **no `-F` anywhere in the repo**) + `netfilter-persistent save`; explicit "check existing rules first" steps; the gcloud command is syntactically correct with a narrower-source-ranges suggestion. Print-only (script never touches the firewall itself) — correct choice.

**Public IP.** GCP metadata (correct endpoint + `Metadata-Flavor` header, IPv4-validated) preferred over ipify with an explicit VPN-egress warning on the ipify path; placeholder + guidance when neither works. Sound; the metadata preference is the right call for a VM whose egress may be VPN-routed.

**Gaps found** → findings P1-1/P2-1/2/3 below (teardown-on-failure story, config mode, privileged-port validation).

## Docs cross-check (README/QUICKSTART vs code)

Verified accurate: architecture diagram & design contracts (mux off, loopback-only, geoip:private direct, fail-open, op_lock + emergency_stop, stale-proxy self-heal, connect order, backoff 1→30 s) — all match lib.rs/supervisor.rs/xrayconf.rs; doctor check list & order (QUICKSTART step 5 = main.rs exactly, incl. "all checks passed"); `generate` 0600; `connect` UAC/elevation/summary/Ctrl+C semantics; tray menu items, `State:` wording, Links line variants, `state\net-merge-status.txt` dump path, 1 s tooltip, TaskbarCreated re-registration and the emergency-teardown Exit — **both S3 P1 features exist in code** (`RegisterWindowMessageW` re-add, `Supervisor::emergency_stop` after the 2 s exit bound) so the QUICKSTART smoke-test checkpoints are truthful; release.yml description (test-first, zip contents, `fail_on_unmatched_files`); .gitignore really contains `config.json` (+ `xray-client.json`, `*.exe`, logs/state/bin); MSRV 1.78; **smoke-test warning present and prominent**; **no emoji** (programmatic scan); all 23 config-table defaults match `config.rs` **except the restartMax semantics and server.address fallback noted below**; geoip.dat/geosite.dat instruction in QUICKSTART step 3 is **genuinely required** (the generated client config's `geoip:private` routing rule fails xray startup without geoip.dat; xray finds assets next to its exe; the server script mirrors the same convention).

Inaccurate (findings): `restartMax (0 = unlimited)`, net-merge "on your PATH", `server.address` default/fallback, and the CI push-trigger claim — details below.

---

## Findings

### P0 — 0
The script is careful root-running software: injection-proof, download-verified, loudly-failing, secret-preserving; nothing that breaks a VM, installs a broken endpoint, or opens a security hole. (The `-test`-before-restart gate in particular means a bad generated config can never take the endpoint down.)

### P1 — 4
1. **`README.md:153`** — "`behavior.restartMax` … (0 = unlimited)" is **factually wrong**: per the supervisor contract and `restart_campaign` (`if max == 0 || attempt > max` → give up), **0 = never restart** (immediate give-up, proxy restored, `Failed`). A user following the README to get "unlimited" resilience gets zero. One-word doc fix.
2. **`QUICKSTART.md:47`** — "Make sure `net-merge` is on your PATH **or** sits next to `multitunnel.exe`": with the default config (`"netMerge": "net-merge"`), `config.rs::resolve` joins the relative name onto the **executable's directory**, and a directory-qualified program is not PATH-searched — so a PATH-only install fails `doctor`/bonding (empirically re-proven: relative paths resolve against the exe dir in my doctor run). Only "next to the exe" (or an absolute `paths.netMerge`) works; the sentence's first disjunct misleads users into a broken step 2.
3. **`README.md:134` + `:155`** — the config table lists `server.address` with default `35.208.99.240`, and the conventions paragraph claims "strings left empty … fall back to these defaults on load". **False for `server.address`**: it has no load-time fallback (only `server.port` and the local/paths/behavior fields do); an empty address is rejected by `validate()` (empirically re-proven: `doctor -config` → "incomplete config — missing: server.address"). The 35.208.99.240 value is a template/example prefill; the table should mark address required like uuid/sni/publicKey, and the convention sentence should be scoped.
4. **`.github/workflows/ci.yml:4` + `README.md:241`** — the push trigger reads `branches: ain]` (a corrupted `[main]`, present since the `64e0e22` post-revert restore — predates S4, but this is the ship stage): it matches no branch, so **CI has never run on any push to main** (the project's entire flow — only `pull_request` events trigger it, and backup.sh pushes directly). The new README claims ci.yml "runs on every push to `main` and on pull requests" — false on the push half. Releases stay protected (release.yml runs tests on tags), but the documented quality gate does not exist. Fix: `branches: [main]` (one line) — the README then becomes true.

### P2 — 6
1. `server/setup-xray-reality.sh:214-337` — no failure-rollback: after `systemctl stop xray` (upgrade path), any failure before `systemctl restart` (install/credential/validation/unit error) leaves the old endpoint STOPPED with no restart attempt and only a raw `set -e` exit (`install`/`chmod`/`chown`/`systemctl restart` have no `|| die` diagnostics); the subsequent re-run then demands `FORCE=1`, which regenerates all credentials (killing the old client block) even though the old config is still valid. Suggest a failure trap that best-effort `systemctl start xray`, plus a "config unchanged — `systemctl start xray` recovers" hint.
2. `server/setup-xray-reality.sh:297` — config written `0644 root:root`; `chown root:nogroup` + `chmod 0640` would keep it readable by `User=nobody` (nobody's group is nogroup on Debian/Ubuntu) while hiding the REALITY private key from other local users; also create under `umask 077` to close the pre-chmod window. The implementer's "local read = already on the box" argument is acceptable for a dedicated VM — noted as defense-in-depth, not a blocker.
3. `server/setup-xray-reality.sh:95-100` — PORT < 1024 (e.g. `PORT=443`, or `PORT=08` → 8) passes validation but `User=nobody` cannot bind privileged ports → the service fails at start (caught loudly by is-active, but with a cryptic journal). Pre-validate `PORT ≥ 1024` or warn.
4. `README.md:170` — "(none) print a pointer to multitunnel-tray" describes intent; the actual no-arg behavior prints the stale S3-era string "tray lands in stage S3 — build multitunnel-tray" and exits FAILURE. Implementer-flagged residual (frozen `src/main.rs`); the orchestrator should arbitrate the one-line src fix vs. the doc's generous phrasing.
5. `server/setup-xray-reality.sh:186` — the tag guard is charset-only (`^v[0-9][A-Za-z0-9.-]*$` accepts e.g. `v99.FOO-BAR`); harmless in practice (GitHub's own tag; a wrong tag 404s loudly at download), but `^v[0-9]+(\.[0-9]+)+$` would be tighter. (Also verified: the `| head -n 1` constructs cannot SIGPIPE under pipefail here — ≤1 line of sed output.)
6. `QUICKSTART.md:90-94` — "Every line should read `[ ok ]`" assumes bonding enabled; with `"netMerge": { "enable": false }` the net-merge line is a designed `[warn]` and doctor still prints "all checks passed". One clause would cover the disabled case.

### Deviation judgments requested
- **geoip/geosite in QUICKSTART**: justified — the generated client config's `geoip:private` routing rule hard-requires geoip.dat at xray startup; geosite.dat is not strictly needed (no geosite rules) but ships in the same zip and costs nothing to copy. The "next to the executable" location is where xray looks absent `XRAY_LOCATION_ASSET` (the supervisor sets none).
- **Config 0644 root-owned**: acceptable for a single-user VM per the implementer's argument, with the 0640 root:nogroup improvement noted as P2-2.
- **GCP-metadata-over-ipify**: correct deviation — on a VPN-egress VM, ipify reports the VPN exit IP and the client block would be wrong; metadata reports the true external IP; ipify remains the fallback with a warning.
- **`xray run -test` verification**: not re-runnable here (tooling cleaned from /tmp); the script's built-in gate order (test config → die before touching the service) plus the implementer's real-binary validation record give adequate confidence.

## Verdict

**APPROVE.** No P0: the installer is injection-proof, verifies everything it downloads and generates, fails loudly rather than half-working, never prints the private key, and cannot replace a working endpoint with a broken config (the `-test` gate). The four P1s are documentation/CI accuracy issues — three false claims in the new docs (restartMax=0 semantics, net-merge PATH resolution, server.address fallback) and one pre-existing-but-now-documented CI trigger corruption (`branches: ain]`) — all one-to-few-line fixes that should land before the v0.1.0 tag, but none breaks a user's VM, ships a broken endpoint, or opens a security hole.

*Repo untouched (clean tree at `56340a5`); the only write is this report. Doc-accuracy proofs run via the pre-built console binary in /tmp/mt-target against throwaway configs in /tmp/s4-doc-check.*
