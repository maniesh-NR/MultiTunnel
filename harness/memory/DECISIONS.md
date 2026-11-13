# DECISIONS — immutable once written; append new ones at the end

## D1 — Rust instead of Go (user directive, 2026-11-12)
The Go prototype is superseded. All production code is Rust. Design contracts
(mux off, loopback inbounds, ordering, fail-open) carry over unchanged.

## D2 — Cross-compilation via zig + cargo-zigbuild
No root in the sandbox (sudo needs a password) → apt gcc-mingw-w64 impossible.
Chosen: zig 0.17.x as linker driver via cargo-zigbuild, target
x86_64-pc-windows-gnu, toolchains rooted under /home/z/my-project/tools/.
CI (release.yml) builds MSVC natively on windows-latest for official
releases; local zigbuild is the sandbox fallback and cross-check.
REINSTALL RECIPE (used twice already — keep for next recycle):
  1. curl rustup-init from static.rust-lang.org (direct binary, NOT curl|sh),
     run with RUSTUP_HOME=/home/z/my-project/tools/rustup
     CARGO_HOME=/home/z/my-project/tools/cargo -y --profile minimal
     --default-toolchain stable --no-modify-path
  2. rustup target add x86_64-pc-windows-gnu
  3. zig tarball from ziglang.org/download/index.json (pick newest stable,
     key containing 'linux' and 'x86_64') → extract to tools/zig
  4. cargo install -q cargo-zigbuild
  5. write tools/env.sh exporting RUSTUP_HOME, CARGO_HOME, PATH(+zig)

## D3 — Single crate, two bin targets
One `multitunnel` crate: lib (all logic) + `multitunnel` (console) +
`multitunnel-tray` (GUI, windows_subsystem via cfg_attr).

## D4 — Tray via raw win32 (windows-sys), not the tray-icon crate
Dependency risk under cross-compile (tray-icon pulls the muda/menu stack) is
higher than ~200 lines of well-understood FFI (Shell_NotifyIconW + popup
menu). Feature-gated windows-sys only.

## D5 — Fail-open proxy policy
If the core dies and restarts are exhausted, the system proxy is RESTORED
(cleared) so normal browsing keeps working. True fail-close is out of scope
for v0.1 and documented as such.

## D6 — Consensus protocol
Two distinct-lens adversarial reviewers must both APPROVE before a stage
closes; BLOCK → fix → re-review, max 3 rounds, then recorded orchestrator
arbitration. Verdicts land in harness/memory/consensus/.

## D7 — GitHub-first resilience (updated after the first incident)
Sandbox snapshots can lag: the first VM reboot rolled back recent writes.
Therefore backups must PUSH as early as possible, not only at stage ends:
commit+push immediately after each implementer round and each review round.
Local commits remain the fallback when no credentials exist.

## D8 — JSON config schema is camelCase and stable
Matches the docs exactly. serde rename_all = "camelCase". Defaults prefill
the known VM IP (35.208.99.240); every credential comes from the server
script output.

## D9 — No secrets in the repo
config.json is gitignored; only config.example.json (empty credentials) is
committed. The GitHub token lives in upload/ (outside the repo), is read
one-shot by backup.sh, never printed, and push logs are scrubbed of it.
