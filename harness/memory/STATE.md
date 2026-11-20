# STATE — rewritten at the end of every stage. Last update: S14 (memory graph + RAG protocol shipped; saturation scheduling WIP preserved)

## Current position
- Stage: S14 IN FLIGHT, two tracks.
  (a) DONE — user directive "store agentic memory as data graphs
  accessible to all agents and use retrieval augmented generation
  (RAG)": typed graph store harness/memory/store/memory_graph.json
  (schema v1; 16 node types / 14 edge rels, evidence-gated; sources =
  PM durable sections, STATE/DECISIONS/LEDGER, agent-reports,
  consensus, git tags + full log, curated/). Retrieval = BM25
  (k1=1.5, b=0.75, title boost 1.5) + graph-neighborhood expansion
  (0.5^hop decay). CLIs: query_memory.py at repo root (--json,
  --prompt paste-ready context packs); harness/memory/write_memory.py
  --node/--edge (curated/ = agent write path, merge-last, failed
  rebuild auto-rolls-back). update_memory.py rebuilds the store.
  18/18 tests green (harness/memory/test_graph.py); builds
  deterministic (byte-identical; as_of from git, no wall-clock).
  (b) SATURATION FIX COMPILED + GATED (release pending) — capacity-
  aware scheduling (task E core, "one adapter maxes out, no spill")
  COMPLETE on top of the cut-off WIP (f4a26ca): peer-reported
  per-carrier delivery rates piggybacked on 1 s PINGs, assign/deliver
  EWMA, carrier_score = assign_bps / max(deliver_bps, PROBE_BPS
  32 KiB/s), Balanced = least relative load (capacity-proportional),
  Priority spills at score >= 1.15, REORDER_CAP 4 -> 16 MiB +
  would_overflow backpressure path. Session closed the WIP gaps:
  unused_mut fix, rustfmt, test helper S14 fields (no-report default
  exercises the S12 fallback), 6 new tests (carrier_score incl. probe
  floor, saturated-adapter bypass, proportional 10:1 spreading
  crossover, priority PRIORITY_SPILL_RATIO spill, mixed-report probe,
  would_overflow boundary). GATES: fmt clean; workspace clippy -D
  warnings clean; 404/0 workspace tests (baseline 398 + 6). Toolchain
  reinstalled per D2: rustc 1.100.0 + rustfmt + clippy under
  /home/z/my-project/tools/{rustup,cargo} (persistent; env.sh updated),
  zig 0.14.1 via python3 -m ziglang intact.
- Task E UI half recorded as PENDING (task:e-ui-logo-titlebar-tray in
  the graph): user's attached image as logo (file not in repo —
  locate/re-request), Chromium-style blended title bar, close-to-tray
  toggle, persistent tray icon, quick settings on tray right-click.
- S13 CLOSED: v0.7.1 released + verified (run 44574748448, CI
  44574711227 green, assets download-verified; status tab = one
  connection frame + one page scrollable, D24).

## Next actions
1. Windows-gnu clippy gate (rustup target add x86_64-pc-windows-gnu +
   cargo/ shims), then cargo zigbuild --release gates (PE32+ x4 + ELF),
   then tag v0.8.0 → release.yml → verify assets → LEDGER + backup.
2. Task E UI half after the core release (logo asset needed from user
   — the attached image is not in the repo; locate or re-request).
3. USER VERIFICATION of v0.7.1 still pending: wheel scrolls everything
   below the frame; graph fills the frame; long adapter lists reachable.
   If stream bonding fails on the VM: sudo bash
   server/setup-xray-reality.sh + server/setup-merge-daemon.sh. After
   v0.8.0: verify saturation spill on the real 3-adapter setup.
4. Backlog: subscription auto-update, vmess validation, profiles.json
   locking, engine log rotation, listener max_conns, upstream iced
   migration, S7 native REALITY core (parked).
