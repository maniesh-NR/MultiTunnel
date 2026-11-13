# STATE — rewritten at the end of every stage. Last update: S3 CLOSED (2026-11-13T07:55Z)

## Current position
- Stage: **S3 CLOSED** (consensus 2×APPROVE + all 3 P1s fixed at 799f6cd).
  The full client is now feature-complete on the Rust side: config, xray
  config generation (mux off), net-merge integration, core supervisor
  (op_lock-serialized), WinINET sysproxy, elevation, doctor/connect CLI,
  raw-win32 tray (32 audited unsafe blocks) + real GUI binary + icon.
  78 tests green; fmt, host clippy, windows-target clippy, zigbuild all
  clean; both PEs link (console + GUI subsystems).

## Next actions
1. S4 — Ship (implementer round): `server/setup-xray-reality.sh` (VLESS +
   REALITY on the VM; REALITY dest borrows the Pangolin gateway TLS on
   127.0.0.1:443; systemd unit; firewall checklist incl. GCP rule + the
   VM's netfilter-persistent; prints the client config block matching
   config.example.json), README.md, QUICKSTART.md (must state: first real
   Windows run is the tray smoke test — menu opens/dismisses, icon survives
   explorer restart), doctor hint updates if needed.
2. S4 review round (D6), then close S4.
3. Release: `git tag v0.1.0 && git push origin v0.1.0` → release.yml builds
   MSVC binaries on windows-latest, runs tests first, packages the zip and
   publishes the GitHub Release. Verify via the GitHub API afterward.
   Also drop local zigbuild artifacts + a source zip into the sandbox
   download/ dir for the user.
4. Final memory update + backup `s4-close` / `v0.1.0`.

## Blockers / open items
- None. Token in upload/ (one-shot use by backup.sh / manual push).
- P2 backlog (from S2+S3 consensus files — do NOT lose): config::validate
  loopback (generate/doctor bypass), sysproxy snapshot .ok() typing,
  temp+rename writes, IPv6 probe literal, doctor xray-version timeout,
  pump interleave, --config= form, ~1 s re-set micro-race in tray Exit,
  CI caching, required status checks. Quickstart must include the
  real-Windows tray smoke test.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main; remote
  github.com/maniesh-NR/MultiTunnel (backup.sh one-shot push).
- Toolchains: `. /home/z/my-project/tools/env.sh` FIRST. Rust 1.100.0,
  zig 0.17.1, cargo-zigbuild, target x86_64-pc-windows-gnu,
  CARGO_TARGET_DIR=/tmp/mt-target.
- Gates (must gate any push): cargo fmt --check; cargo clippy --all-targets;
  cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings;
  cargo test (~6 s); cargo zigbuild --target x86_64-pc-windows-gnu.
