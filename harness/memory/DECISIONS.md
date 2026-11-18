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

## D11 — All-in-One pivot (user directive, 2026-11-17)
v0.3 north star: one app, zero user-installed dependencies, v2rayN +
net-merge functions out of the box, Material You GUI, Rust core. The user
never manually installs an Xray core or net-merge: the engine is located,
verified, downloaded (or embedded at build time) by mt-platform; link
spreading moves in-process into mt-net. The managed Xray engine remains
the REALITY specialist until S7's native port (stock rustls cannot do the
REALITY ClientHello auth; a faithful port needs a custom TLS 1.3 stack —
tracked, not rushed).

## D12 — S6 execution: Mixture of Experts + strangler workspace (2026-11-17)
Stage S6 runs as parallel expert agents, one crate each (mt-links, mt-net,
mt-engine, mt-m3, mt-platform; then mt-app), contracts frozen in
harness/CONTRACTS-S6.md; the orchestrator owns the workspace root, wiring
and integration. The root package keeps v0.2 behavior green at EVERY
commit (branch protection requires green CI on main pushes) — new code
lands beside it in crates/, migration of root modules happens gradually.

## D13 — GUI: iced + mt-m3 Material You system (2026-11-17)
iced 0.13 (MIT, pure Rust, no system webview — keeps the zero-dependency
promise) with a hand-built Material Design 3 token system (mt-m3: dynamic
color from a seed, light/dark, type scale, shape, state layers). The
existing raw-win32 tray stays (D4) as the always-running companion; the
iced window app (mt-app) is the full v2rayN-class GUI. Prefer the
tiny-skia renderer for cross-compile robustness.

## D14 — Engine self-provisioning, never silent (2026-11-17)
Engine resolution order: $MULTITUNNEL_ENGINE → exe-dir/engine →
config-dir/engine (sha256-verified against SHA256SUMS; strict mode refuses
mismatch) → build-time embedded zip (MT_EMBED_ENGINE) → explicit
user-triggered download (GitHub latest, URL overridable). `locate()` never
downloads; only `ensure()` may, and the GUI/CLI surfaces it.

## D10 — vless:// importer is REALITY-only and strict (2026-11-17)
MultiTunnel generates exactly one protocol shape (xrayconf: VLESS/TCP/
REALITY/xtls-rprx-vision, chrome fp, spiderX /). `multitunnel import`
therefore accepts only links it can faithfully represent: security=reality,
type=tcp, flow absent or xtls-rprx-vision; uuid 8-4-4-4-12 hex; pbk exactly
43 unpadded base64url chars; sid empty or 1-16 hex. Unknown query params are
ignored (other clients append extras such as headerType, path, alpn,
allowInsecure). No percent-decoding of stored fields (uuid/host/port/sni/
pbk/sid never carry escapes; the remark fragment is display-only, not
stored). Error messages never echo uuid/pbk values (D9). config.json is
patched as RAW JSON — only the server block is replaced — so user-customized
ports/paths/behavior survive and relative paths are not re-rooted by a
resolve() round-trip. Writes go through write_private (fsutil atomic,
S5). Missing config → template first, then patch. (Feature built on a
stale-tree session and salvaged onto the frontier after the 7th rollback.)

## D15 — Bare launch = GUI; a GUI app never dies invisibly (2026-11-18)
User report on the v0.3.0 release: "multitunnel.exe does not show a GUI
when opened. There does not appear to be any launch activity." Root cause:
the no-args default was the `tray` command, which only printed a pointer to
the separate multitunnel-tray.exe and exited 1 — a console-subsystem
double-click flashes a console for a fraction of a second. The build was
never broken; the product's front door was. v0.3.1 rules:
1. Bare launch defaults to `gui` (`command_from`, regression-tested).
2. Windows console policy (`console_mode`): a console-subsystem GUI launch
   detaches its console via FreeConsole ONLY when the process is the sole
   console user (GetConsoleProcessList == 1 — Explorer double-click,
   autostart Run key, `start`); a terminal launch shares the console and
   keeps it. Call failures conservatively read as "shared" — never detach a
   console we might share. Side effect: the parked C-P2-10 autostart
   concern is largely resolved (autostarted console exe now opens the GUI
   with no lingering console).
3. Never-die-invisibly: GUI launch errors surface as a MessageBoxW when no
   shared console exists (`alert_error`), and a panic hook
   (`install_panic_dialog_hook`) shows panics the same way in flash-death
   processes. stderr remains the primary channel whenever a console
   survives the process.
4. `gui_args()`: the `gui` dispatch arm must accept an EMPTY argv tail —
   `&cmd_args[1..]` panicked on the bare launch (a hidden precondition the
   v0.3.0 `tray` default masked). Caught only by running the host binary
   bare: unit tests and `gui --smoke` never exercise empty-argv dispatch.
   LESSON: after changing a dispatch default, run the binary with no args
   on the host before shipping.

## D16 — Adapter bonding via per-adapter engines + Xray `sendThrough` (S8, 2026-11-18)
User report: "There is no menu to select the network adapters that are
being bonded for the vless connection." Root cause: the merge aggregator,
policies and probers existed, but `EngineLinkFactory` built exactly ONE
unbound engine — bonding had no adapter surface at all. S8 rules:
1. Enumeration lives in mt-platform (`adapters` module): Windows
   `GetAdaptersAddresses` FFI (windows-sys, AF_INET, loopback excluded,
   buffer-retry, 16 MiB cap, fail-open to an empty vec); non-Windows is a
   pure-std /proc+/sys dev path (IPv4 attribution documented-limited).
   `NetAdapter.id` = the stable machine id (Windows ANSI AdapterName GUID
   / Linux ifname) — settings persist IDS, never display names.
2. Binding model: ONE `ManagedEngine` per selected adapter, its Xray config
   carrying `"sendThrough": <adapter IPv4>` (top-level outbound field;
   Windows source-address binding selects the egress interface). Engine
   tags embed the adapter name (`"profile [Wi-Fi]"`, deduplicated) so
   config/log files never collide; each engine keeps its own ephemeral
   loopback SOCKS port. Mux stays OFF — the aggregator spreads individual
   TCP connections across the per-adapter links (net-merge model).
3. Selection is id-validated (`set_bonded_adapters` rejects unknown ids),
   persisted in `settings.json` (`SettingsView.bonded_adapters`,
   `#[serde(default)]` so v0.3 files load), and applied on the NEXT
   connect; a live session is restarted immediately on Apply. Unknown ids
   or IPv4-less adapters at connect time are SKIPPED with a warning
   (fail-open — an unplugged USB modem must not black-hole the tunnel).
4. `set_settings` never writes `bonded_adapters` (the Adapters screen owns
   that field; a stale GUI settings copy must not clobber a fresh
   selection).
5. Empty selection = v0.3 behavior (single unbound link, OS routing table
   decides). GUI: new "Adapters" screen (checkbox rows + IPv4/up chips,
   Apply/Revert/Refresh — the Routing-editor draft idiom); CLI:
   `multitunnel adapters` prints the same view for headless diagnosis.
## D17 — Stream bonding protocol (S9, 2026-11-18): mt-bond framing over N REALITY carriers
User want: one logical connection, bytes split across the bonded adapters,
re-merged at the server; reverse symmetric. Design:
1. TRANSPORT: each adapter keeps its per-adapter engine (D16); a CARRIER is
   one long-lived connection dialed through one engine whose VLESS target is
   `127.0.0.1:<merge_port>` ON THE SERVER — server Xray (freedom) pipes it to
   the `mt-merge-server` daemon. REALITY/vision unchanged per carrier.
2. FRAMING: fixed 24-byte little-endian header (magic 0x4D54, version 1,
   kind, bond_id u32, session u32, seq u64, len u32) + payload; kinds
   HELLO/OPEN/DATA/CLOSE/ACK/PING/PONG; DATA ≤ 16 KiB. Carriers are plain
   byte streams — framing is end-to-end between client bond engine and the
   daemon, invisible to Xray.
3. RELIABILITY: per-session cumulative ACKs; sender keeps unacked frames;
   carrier death (write err / EOF / pong timeout) replays unacked frames on
   live carriers; receiver reorders (drop dupes, buffer gaps, stall-close at
   10 s). Backpressure is TCP-native: bounded demux channels → reader blocks
   → carrier socket buffers fill → sender stalls (no windowing protocol).
4. SCHEDULING: round-robin across live carriers (predictable even split —
   exactly what the user asked); per-carrier PING RTT EWMA powers health +
   the Status rows; weighted scheduling is future work.
5. FALLBACK: `bond_mode=stream` with <2 usable carriers → per-connection
   mode + LogLine (never black-hole).
6. DAEMON: loopback-only (127.0.0.1) — authentication is REALITY+UUID at
   the Xray edge; no extra secret in v0.5, documented.

## D18 — Exit probe: plain HTTP inside the tunnel (S9, 2026-11-18)
`exit_probe()` rides the LIVE session: SOCKS CONNECT (through the local
listener) to `ip-api.com:80` + hand-rolled HTTP/1.1 GET, parse
{query,country,city,isp,as}. Plain HTTP is deliberate: the client→server leg
is REALITY-encrypted, and the last hop carries only the echo payload — no
TLS stack enters the tree (zero-dep rule). The Status card compares the
measured exit against the profile endpoint and calls out mismatches (the
GCP-VM-VPN-egress case: exit ISP ≠ Google → server-side egress explanation,
NOT a hidden MultiTunnel hop).

## D19 — Status tab becomes a live dashboard (S9, 2026-11-18)
1. Bandwidth graph = iced canvas, 120 s ring of (up_bps, down_bps) 1 s
   samples from ProxyEvent::Tick; Fill-sized (grows with the window, Task
   Manager style); auto-scaled axes; theme-aware; down=primary, up=tertiary.
2. Server picker = pick_list over profile tags; changing it while connected
   = set_active + automatic reconnect.
3. Status screen leaves the global scrollable (fixed state card + graph
   Fill + links scrollable) so the graph can scale with the window.

## D20 — v0.5.1: visibility fixes driven by the first real two-adapter run (S9.1, 2026-11-18)

User report on v0.5.0 with Ethernet+Wi-Fi ticked: (1) all traffic rode
Ethernet only; (2) the Status graph never appeared; (3) wanted one
combined color-coded graph + per-interface graphs under an arrow.

1. GRAPH COORDINATES: iced 0.13's Canvas draws through
   `renderer.with_translation(bounds.x, bounds.y)` — a Program's frame is
   in LOCAL space. v0.5.0's RateGraph computed every point from
   `bounds.x/y`, double-offsetting the plot outside the card (invisible,
   "graph does not load"). Fix: draw into `Rectangle::new(ORIGIN,
   bounds.size())`; regression test pins points inside the local rect for
   a realistically-offset widget rect. Unit tests never caught it because
   they used origin bounds. LESSON: a canvas program's helpers should
   never see widget-space coordinates at all.
2. MERGE POLICY DEFAULT: v0.5.0 shipped `merge_policy = "failover"`, and
   Failover always picks the FIRST healthy link — with Ethernet enumerated
   first, Wi-Fi stayed idle until Ethernet died. Users read that as broken
   bonding (it is failover semantics). v0.5.1: default round-robin
   (connections alternate), unknown-string fallback also round-robin, and
   a `settings_version` field (default 0 = v0.5.0 file) migrates the STOCK
   failover value to round-robin on load (best-effort persist). An
   explicit failover on a version>=1 file is respected verbatim — the
   migration is once, ever.
3. CARRIER VISIBILITY: ensure_carriers now returns a CarrierReport
   {live, lines} — one human line per link ("live" / "down (reason)") —
   and connect_flow emits them as ServiceEvent::LogLine so the GUI log
   shows per-carrier verdicts; <all-live emits a WARN with the share
   semantics. Skipped bonded adapters (absent / no IPv4 at connect) also
   LogLine (were tracing-only). Dead carriers were INVISIBLE in v0.5.0 —
   the exact conditions that produced "only Ethernet".
4. PER-INTERFACE GRAPHS: the Adapters tab rows get an arrow (▸/▾ text
   button — no icon font in a zero-dep build) expanding a mini RateGraph
   (110 px fixed, same two color-coded series as the combined graph; the
   legend lives only on the Status graph). Data: the GUI samples each
   StatusSnapshot (1 s) — links/carriers report CUMULATIVE wire bytes
   under engine tags `"<profile> [<adapter>]"`; `link_matches_adapter`
   matches the bracketed adapter name (rsplit on " [", accepts the `#n`
   dedup suffix, unbound links match nothing), deltas become bytes/s
   samples per adapter id (saturating on counter resets, vanished links
   dropped from the delta base, zero samples appended to unbonded rows —
   a flat line is the honest "this NIC carries nothing" signal).
   Combined graph gained an in-canvas legend (color swatch + down/up).
