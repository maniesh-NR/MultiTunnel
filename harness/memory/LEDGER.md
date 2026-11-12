# LEDGER — append-only. One line per event: `<UTC timestamp> | <event>`

2026-11-12T13:18Z | env | Go toolchain probe; proxy.golang.org reachable (pre-pivot)
2026-11-12T13:30Z | pivot | USER DIRECTIVE: rewrite in Rust; harness+memory on github.com/maniesh-NR/MultiTunnel; backup after every stage; parallel adversarial agents w/ consensus gates; releases to GitHub
2026-11-12T13:45Z | env | repo exists, PUBLIC, EMPTY (ls-remote exit 0); no push credentials at that time
2026-11-12T13:50Z | env | Rust 1.99.1 + zig 0.17.1 installed under tools/ (rootless); mingw impossible (no sudo)
2026-11-12T14:05Z | s0 | skeleton: Cargo.toml, error/lib/config/xrayconf/netmerge, main.rs dispatch, bin/tray.rs stub, meta files
2026-11-12T14:20Z | s0 | cargo test green; zigbuild OK: multitunnel.exe (console) + multitunnel-tray.exe (GUI) PE32+ via file(1)
2026-11-12T14:21Z | s0 | backup(s0-bootstrap) commit 2a6b3f9 (kept local — no credentials yet)
2026-11-12T14:22Z | s1 | launched parallel implementers S1-A (config+xrayconf tests) and S1-B (netmerge)
2026-11-12T14:35Z | s1 | S1-A done: 12 tests green. S1-B gateway deadline-crash; retried
2026-11-12T14:45Z | s1 | S1-B done (retry): 13 tests green; fixed real LINK_LINE_RE boundary bug (multi-digit metrics silently rejected)
2026-11-12T14:50Z | INCIDENT | sandbox VM reboot rolled back /home/z/my-project to an older snapshot: S1-A tests, cargo-zigbuild, zig, rustup std target, MultiTunnel/.git, harness memory subtree, .github, LICENSE/.gitignore/config.example.json LOST; netmerge.rs + src lib SURVIVED
2026-11-12T14:55Z | incident | USER UPLOADED push token: upload/'Multitunnel github access token.txt' (93 bytes) — backup.sh now reads it one-shot, scrubs logs, never prints
2026-11-12T15:00Z | incident | toolchain tree rebuilt (rustup+zig+zigbuild+target), env.sh rewritten, harness+meta files restored from protocol; git history re-initialized
2026-11-12T15:02Z | s1 | resuming S1-A to re-apply its 12 tests after the revert
2026-11-12T15:09Z | s1 | S1-A resumed: re-applied 12 tests (had to restore wiped config.rs/xrayconf.rs implementations too) — 25 passed total; pushed b5caf15
2026-11-12T15:12Z | s1 | SCOPE VIOLATION (crashed S1-B session): had replaced src/main.rs + src/bin/tray.rs with minimal "RESTORED-PLACEHOLDER" stubs — originals restored by orchestrator (generate/dispatch + windows_subsystem attr back in place)
2026-11-12T15:12Z | s1 | first successful GitHub push was 64e0e22 (restore-post-revert); implementer state pushed as b5caf15
2026-11-12T15:15Z | s1-review | consensus round: S1-REV-C APPROVE (0P0/2P1/5P2), S1-REV-B APPROVE (0P0/1P1/6P2) — both ran tests+cross-build independently
2026-11-12T15:18Z | s1 | P1+P2 fixes applied by orchestrator: run_strict for on/off, 0600 credential write, #[cfg(unix)] gates, checked_add deadlines, has_root paths, golden tests (proxyOverride + example.json equality), fingerprint/spiderX asserts, release.yml Test-Path guards, stale ROADMAP note removed
2026-11-12T15:20Z | s1 | post-fix suite: 30 passed 0 failed; zigbuild green; S1 CLOSED (harness/memory/consensus/S1-consensus.md)
