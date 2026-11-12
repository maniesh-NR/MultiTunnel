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
