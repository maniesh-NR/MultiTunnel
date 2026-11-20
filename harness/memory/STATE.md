# STATE — rewritten at the end of every stage. Last update: S16 (repo polish + window menu + state-aware tray, v0.10.0)

## Current position
- Stage: S16 IN FLIGHT — implementation + gates COMPLETE, release v0.10.0
  re-running (first attempt: two independent GitHub-runner flakes in the
  mt-bond integration leg — `carrier_death_mid_stream_fails_over_without_
  data_loss` failed on windows (second-half write: "no live bond carrier")
  and the linux leg hung inside `cargo test -p mt-bond`; the SAME commit
  passed CI on BOTH OSes (run 44717917257) minutes earlier, 25x solo +
  12x full-suite stress runs under 8-way CPU load locally = 408 executions
  0 failures, and the identical suite shipped S14/S15 releases — verdict
  runner flake, precedent v0.5.1 cancel+rerun; run 44719389267 cancelled
  + re-run).
- S16 user asks ALL implemented (D26): (1) GitHub About section rewritten
  (was a stale description of an unrelated project) + homepage → releases;
  20 relevant topic tags set via API; (2) Attribution section in README
  (Xray-core, iced + vendored patches, Material You, v2rayN, net-merge,
  BondTunnel predecessor, own logo/banner artwork, cargo-zigbuild/zig);
  (3) README banner (assets/banner.png, 1280x360 lockup on the logo's own
  canvas color) + transparent logo in the identity section; (4) titlebar
  emblem on a TRANSPARENT background (flood-fill key-out + area-averaged
  resample with premultiplied alpha — bilinear punched holes at 20:1);
  (5) tray icon state-aware: colored logo while connected, MONOCHROME
  transparent variant in every not-connected state (NIM_MODIFY swap,
  TaskbarCreated re-adds the current style, boots Mono); (6) tray hover =
  live ↓/↑ speeds + Σ totals (session_bytes from link-counter deltas,
  counter-reset-safe, wiped on leaving Connected; tooltip < 128 NIF_TIP
  units); (7) titlebar RIGHT-CLICK window menu (stack overlay layer,
  cursor-anchored + clamped, dismiss on outside release): Always on top
  (checkable, serde-default settings field, applied cross-platform via
  iced window::change_level, honored at startup) / Minimize / Maximize /
  Close. ALSO FIXED: latent S15 bug — the titlebar identity cluster sat
  flush at the strip top (container align_y(Center) added; was hidden by
  the old opaque emblem tile).
- Memory graph + RAG (S14) remain live: query_memory.py / write_memory.py;
  graph rebuilt after every change (18/18 tests).

## Next actions
1. Watch release run 44719389267 (re-run); on success verify assets by
   download+unzip (11-entry manifest, PE32+ x2, README/QUICKSTART carried);
   then LEDGER close entry + backup push.
2. If the mt-bond flake recurs on the runner: do NOT blind-rerun again —
   capture the hung job's logs FIRST (jobs API), then decide (test-harness
   hardening: bound read_to_end with tokio timeout is the leading option,
   but only with a reproduced defect in hand).
3. USER VERIFICATION of v0.9/v0.10 asks on the real Windows box: tray mono
   ↔ colored transition, hover speeds, titlebar right-click → always-on-top
   actually floating above other windows, transparent emblem on both light
   and dark titlebars.
4. Backlog: subscription auto-update, vmess validation, profiles.json
   locking, engine log rotation, listener max_conns, upstream iced
   migration, S7 native REALITY core (parked).
