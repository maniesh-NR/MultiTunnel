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

## D21 — S10: rendering fixes as vendored iced patches + exclusions import

User report: graphs occasionally out of frame / not loading; exclusions
import like v2rayN; better-than-round-robin scheduling; "Check exit IP"
panics on a non-total comparator.

1. VENDORED ICED PATCHES over an iced upgrade: iced_graphics and
   iced_tiny_skia are at their TERMINAL 0.13.0 releases (no 0.13.x patch
   line exists), and 0.14/1.0 is a breaking migration. The three defects
   (local-vs-absolute clip space in damage + render mask; INFINITY marker
   bounds on canvas text → NaN damage; non-total sort comparator) are
   fixed as minimal vendored patches under vendor/ with
   [patch.crates-io], inline comments, per-patch READMEs and regression
   tests inside the vendored crates (`cargo test -p iced_graphics -p
   iced_tiny_skia`). LESSON: before upgrading iced wholesale, diff the
   canvas/damage paths — these fixes must be re-based or dropped
   deliberately when the dependency moves.
2. CANVAS TEXT BOUNDS are now MEASURED (one no-wrap cosmic-text layout
   per label per frame, ~µs) instead of INFINITY markers. The anchor
   stays the alignment position and the size is the measured line — both
   consumers (visible_bounds for damage, draw_cached for rendering)
   apply the same alignment arithmetic to the same rectangle. A first
   attempt that reused the frame clip as conservative bounds BROKE the
   draw anchor (draw_cached re-anchors via `..bounds`), pushing the
   right-aligned rate label to the window edge — caught by the Xvfb rig
   and fixed by measuring instead.
3. EXCLUSIONS SEMANTICS: every plain-list entry imports as action
   `direct`; v2rayN JSON keeps its own outbound tags where they map
   (direct/freedom, proxy, block/blackhole). `full:` domains WIDEN to
   domainSuffix (no exact matcher exists; widening is the safe direction
   for a bypass list) — documented in the UI hint, README and parser
   docs. geosite:/geoip:/regexp: skip with per-entry reasons (zero-dep
   build carries no geo databases). Imports join the GUI DRAFT (Apply
   activates); the CLI variant (`routing import`) persists immediately —
   power users read diffs, GUI users get an undo.
4. GUI/backend vocabulary unified on the REAL service's camelCase rule
   kinds ("domainSuffix"/"domainKeyword"/"ipCidr"/"any") — the kebab-case
   pick-list had made every real-backend save fail and loaded rules
   render blank kinds since v0.5.0 (invisible in smoke tests because
   MockService is permissive). Same class of bug: hyphenated merge-policy
   names now parse; "lowest-latency" had silently meant round-robin.
5. SCHEDULING: no new policy — lowest-latency (EWMA) already existed in
   mt-net; the work was making it reachable and explaining the trade.
   "Both connections at once" = stream bonding (needs mt-merge-server on
   the VM, D17/D18); the Advanced card now says exactly that.

## D23 — S11: even byte spreading (Balanced) + adapter priority order (Priority), v0.7.0

User report: with 3+ bonded adapters, one carries most traffic, the
second less, the third rarely any. Root cause: round-robin evens
CONNECTION COUNT, not BYTES — a single fat flow dominates its adapter.
(D22 was the S10.1 HiDPI fix — recorded in the ledger, no separate
section here.)

1. BALANCED (new stock default) is load-aware spreading: each new
   connection goes to the link with the least live pressure
   (`rate_bps + active_conns × 128 KiB/s`). The per-connection reserve
   exists specifically so a burst of connections inside one 1 s counter
   tick spreads instead of stacking on the one link whose byte counters
   have not ticked yet. Ties rotate via the shared cursor, so an idle
   table degrades to exactly round-robin. Settings migration v2 moves
   the stock round-robin to balanced ONCE (D20 precedent: a version<2
   file carrying the stock value is indistinguishable from an explicit
   choice; an explicit re-choice persists).
2. SATURATION IS A PLATEAU, NOT A LEVEL. The learned capacity rises to
   meet any new rate, so "rate ≈ capacity" is true during a ramp too —
   a naive threshold would mark a link saturated on its first fat flow.
   Priority therefore requires the capacity to have STOPPED rising
   (≥2%/tick resets the streak; the rate EWMA damps jitter below that)
   for 3+ ticks AND rate ≥ 0.85×capacity AND capacity ≥ 256 KiB/s.
   Consequences: a still-climbing link is never saturated (priority
   keeps feeding #1 while it ramps); a pinned link saturates within
   seconds of plateauing; when demand drops below the band the link
   un-saturates immediately (priority returns traffic to #1 long before
   the capacity decays).
3. PRIORITY ORDER = the bonded_adapters LIST ORDER (ids persisted in
   order since S8; links and stream carriers are built in that order,
   so link index IS priority). No new settings field — the Adapters tab
   became the ordering UI: selected rows render first with #n chips and
   ↑/↓ buttons; ticking an adapter appends at the END (lowest priority).
   All-saturated fallback = least relative load (unknown capacity ranks
   as most-headroom).
4. STREAM MODE uses WRITER-QUEUE BACKPRESSURE as the saturation signal
   (no capacity estimation needed — TCP already measured it): carriers
   track queued bytes (send_bytes +, writer_task −); Balanced picks the
   least-queued carrier (capacity-proportional: fast NICs drain quicker
   and take more frames), Priority fills carriers in order until one
   exceeds 512 KiB queued (≈8 max frames — healthy carriers hover near
   zero, bottlenecked ones grow without bound). The daemon keeps
   round-robin; priority is a client-side concern.
5. HONESTY about what per-connection Balanced can do: it places NEW
   connections; an already-running fat flow stays on its adapter. Byte
   evenness at the FLOW level is stream mode's job (per-connection
   balanced + stream balanced now compose).

## D24 — S13: Status tab = one connection frame + one page scrollable (2026-11-19)

User report (v0.7.0): the Status scrollbars only scrolled the adapter
list, so long adapter lists were effectively unusable; wide dead
horizontal gap between the connect switch and the server dropdown; the
combined graph should use that free space.

1. STRUCTURE: ONE fixed connection frame at the top — tunnel state
   text, the connect toggler and the server pick_list STACKED in a
   fixed-width left column (STATUS_FRAME_LEFT_WIDTH 296) — and the
   combined bandwidth graph in a fixed-height panel (STATUS_GRAPH_HEIGHT
   240, D22's stable-container rule intact) that Fill-scales into ALL
   remaining frame width. Below the frame: ONE page scrollable owning
   everything (exit card + the Links/Adapters card). The wheel therefore
   scrolls the whole page; the frame never moves.
2. WHY the graph may Fill WIDTH inside the frame but keep FIXED height:
   nothing that can grow lives in the frame any more (the exit-probe
   result, links and adapter rows all scroll below) — the D22 reflow
   triggers are gone by construction.
3. MOCK FIDELITY: seed_adapters now carries EIGHT adapters (the 3-NIC
   bond + Bluetooth, vEthernet (WSL), Tailscale, Loopback, VirtualBox —
   a realistic Windows enumeration); only the 3 wired ones are bonded.
   This is what exposed the stale test idiom: the toggle test's apply
   step used to send ALL row ids to set_bonded_adapters (the real
   AdaptersApply filters to selected rows; the all-bonded 3-seed made
   the two indistinguishable). Test now mirrors production.
4. RIG PROOF (pixel-verified): frame anchors byte-stable across scrolls
   at sf=1.0 AND sf=1.5; all 8 adapter rows render in priority order and
   the last row is visible at max scroll; per-adapter expansion, the
   exit-probe verdict card and the resize round-trip all still work;
   0 panics.

## D25 — S15: in-app tray + Chromium-style title bar + the user's logo (2026-11-20)

User asks (task E UI half): use the attached image as the logo; title bar
blends with the window like Chromium; toggleable close-to-tray; a
persistent tray icon while the app is open or minimized; Quick Settings
on tray right-click.

1. LOGO PIPELINE (assets, all runtime — no exe resource embedding):
   `scripts/make_logo_assets.py` (stdlib-only; deterministic) decodes the
   user's PNG (zlib + unfilter, no image crate) and emits:
   `assets/logo.png` (verbatim source, ~197 KB), `assets/logo_24.rgba`
   (titlebar emblem, 24x24), `assets/logo_64.rgba` (window icon 64x64 —
   iced `window::Settings::icon` via `Icon::from_rgba`), and regenerates
   `assets/icon.ico` (BMP-format entries 16/32/48 = emblem crop, 256 =
   full lockup — small sizes use the emblem so the mark stays legible;
   the legacy `src/tray.rs` ICO parser and the new mt-app tray both
   consume it). The window/taskbar icon is set at RUNTIME (Settings::icon
   + WM_SETICON) — Explorer's .exe file icon stays default (a resource
   object would need windres/llvm-rc, neither cross-compiles in the
   sandbox; D4-style hand-rolled COFF .rsrc deferred until asked).
2. TITLE BAR: `decorations: false` + an in-app strip ABOVE the nav rail
   (emblem + "MultiTunnel" + drag area + – □ × buttons) on the SAME
   surface color as the page (the Chromium blend). Drag = `window::drag`
   on press; double-click-to-maximize = time-based (450 ms) second-press
   detect (mouse_area has no double-click in iced 0.13.4); maximize glyph
   state via `window::get_maximized`; min/max/close via window Tasks.
3. CLOSE-TO-TRAY: `exit_on_close_request: false` ALWAYS (the app owns
   both paths); `close_to_tray: bool` settings field (serde default
   false — version-free migration, D8/D20 precedent). CloseRequested
   with close_to_tray: Windows + tray alive → hide via
   `window::run_with_handle` (HWND → ShowWindow SW_HIDE); non-Windows →
   minimize + log line (honest fallback; a hidden window with NO tray on
   Linux would be unreachable). Without close_to_tray → `window::close`.
   iced 0.13 has NO hide Action — RunWithHandle is the escape hatch (no
   vendored patch needed).
4. IN-APP TRAY (mt-app/src/tray.rs, cfg(windows), raw Win32 per D4 —
   the S3 patterns reused: hidden top-level STATIC window subclassed via
   GWLP_WNDPROC (NOT message-only — TaskbarCreated must reach it),
   Shell_NotifyIconW, TrackPopupMenuEx with TPM_RETURNCMD +
   SetForegroundWindow, icon from the embedded ICO, TaskbarCreated
   re-add): dedicated thread + 50 ms WM_TIMER command poll; commands
   (tooltip/model/shutdown) via std mpsc IN, events via tokio mpsc OUT
   (bridged into iced by the existing EVENTS_SUB_ID-style stream
   subscription). Left click = open window; right click = menu; the
   tray spawns in App::init (both mock and real backends — run_app is
   shared with `multitunnel gui`), tears down (NIM_DELETE + bounded
   join) on every real exit path. Non-Windows: stub (None) — the Linux
   GUI keeps working, covered by the rig; the win32 half is covered by
   windows clippy + zigbuild + the S3-proven FFI shapes.
5. QUICK SETTINGS MENU = the tray right-click menu, built FRESH from a
   pure, host-tested model (menu_model: Vec of items with id/label/
   checked/disabled + submenu): Connect/Disconnect (state-reflecting),
   Server → radio list of profiles, Open MultiTunnel, Exit. Selections
   ride TrayEvent → existing Message paths (ConnectToggle/SetActive) —
   no new service calls. Tooltip: "MultiTunnel — <state> [<profile>]",
   refreshed on tick only when changed.

## D28 — S18: tray tooltip/icon rework, color wheel + theme modes, literature-driven scheduler refinements (2026-11-21)

User asks: (1) tray hover = name+version / total volume / current speeds,
three lines; (2) tray+taskbar icons with CLEAR backgrounds and a larger
mark; (3) a color wheel in Settings for custom accent colors; (4) system /
light / dark mode selection; (5) explore bonding performance from
prominent scientific papers; (6) explore UI improvements from comparable
apps' changelogs.

1. TOOLTIP = 3 lines with CRLF separators (the Win32 tooltip control
   breaks lines on \r\n — the SpeedFan/qBittorrent multi-line tray
   precedent): line 1 `MultiTunnel v<CARGO_PKG_VERSION>`, line 2
   `Total ↓x ↑y`, line 3 `↓x/s ↑y/s` (the S16 single-line form is gone;
   not-connected states are 2 lines: name+version / state word). Worst
   case ≈70 UTF-16 units — safely inside the 128-unit NIF_TIP budget,
   pinned by tests.
2. ICON ASSETS (make_logo_assets.py): all RUNTIME icons — icon.ico
   (16/32/48 emblem + 256 lockup), icon_mono.ico, logo_64.rgba (the
   window/taskbar icon) and logo_24.rgba (titlebar emblem) — now share
   ONE treatment: flood-fill keyed-out TRANSPARENT background,
   `contrast_boost` (hue-preserving luminance remap onto [56,240],
   percentile-normalized like desaturate but per-channel scaled so hue
   and chroma survive) and a tighter margin (0.08 → 0.04, the glyph
   fills ~8% more of the tile). Rationale: the old connected icon was a
   nearly-white opaque tile (92% of pixels >200 luminance —
   indistinguishable from a blank square); the emblem itself carries
   dark strokes (16% below 110), which the boost deepens for light
   taskbars while the pastel body still pops on dark ones. Verified by
   pixel stats (33% opaque, corners alpha 0, dark-stroke bands) and the
   rig (titlebar emblem lum spread 114 on light / 212 on dark).
3. COLOR WHEEL = precomputed 256×256 HSV disc asset
   (make_color_wheel.py; hue = clockwise angle from 12 o'clock,
   saturation = radius, V=1) shown at 176 logical px via
   image::Handle; hit-testing + marker placement are the INVERSE pair
   `wheel_seed_at`/`wheel_marker_position` in theme.rs (unit-tested
   roundtrip, cardinal hues pinned). Interaction: mouse_area
   on_press/on_move(Point)/on_release; press picks the color under the
   cursor (the move that got the cursor there recorded the position),
   drag re-derives the theme LIVE (no disk I/O), release persists via
   the shared save_settings path; a global left-release listener
   (listen_with, same subscription as the S16 cursor tracking) ends a
   drag released OUTSIDE the wheel. The marker is a canvas overlay
   (seed disc + white halo + on-surface ring). Presets + numeric seed
   field stay.
4. THEME MODES = `SettingsView::theme_mode: String` ("system"|"light"|
   "dark", serde default "") + `dark_mode: bool` KEPT as the
   forward-compat shadow (written at the effective state on every save,
   so a v0.11 binary reading a v0.12 file still lands on the appearance
   the user sees; an absent theme_mode parses from the legacy bool —
   D8 version-free migration, pinned by tests). System resolution:
   `MT_THEME_SYSTEM_DARK` env override (the rig seam) → Windows
   AppsUseLightTheme registry (windows-sys Win32_System_Registry — same
   crate, one feature) → gsettings color-scheme → GTK3 settings.ini →
   GTK_THEME suffix → light. Re-polled every 5 ticks while in System
   mode (a flip logs + re-themes live). The Settings UI replaces the
   dark toggler with System/Light/Dark segmented buttons (selected =
   filled) + a caption showing what System currently resolves to.
5. SCHEDULER (papers → code, docs/research/2026-11-21-bonding-
   performance-survey.md): (a) BLEST (Ferlin et al., IFIP Networking
   2016) — among carriers TIED at the min relative-load score, one
   whose queue drains slower than 250 ms at its reported rate is
   skipped (HoL avoidance at the merge daemon's reorder buffer). The
   guard ONLY narrows the tied set: the score leader always stays
   eligible, because queue depth alone is not a saturation signal (the
   S14 lesson — the deep-queue-but-headroom carrier in the S14 test
   must keep winning; a naive BLEST filter would have regressed it and
   DID fail its first test run). (b) ECF (Lim et al. 2017) —
   LowestLatency with delivery reports = earliest completion
   (rtt + 1000·queued/deliver): a 20 ms carrier with a 500 ms backlog
   loses to an empty 60 ms one; without reports the S11 sampled-RTT
   behavior stands. Roadmap (redundant mode, FEC parity, reinjection,
   wVegas-style delay signal) documented with mechanisms in the survey.
6. UI SURVEY (docs/research/2026-11-21-ui-changelog-survey.md): Clash
   Verge Rev / v2rayN / FlClash / Hiddify / Speedify changelogs mapped
   to shipped vs proposed (next: per-link live status badges, exit-IP
   auto-refresh, empty states).

## D29 — S21: P0 reconnect fix — tracked force-close at drain end + bind retry (2026-11-24)

User report: "The connection works for some time, but it stops working
when a second connection is added. The application must be restart for it
to work again."

1. ROOT CAUSE (H1): connection tasks spawned per accepted proxy
   connection were UNTRACKED; a shutdown drained for 3 s and returned
   WITH LIVE CONNECTIONS. Each lingering socket pins the listener port's
   local endpoint; the next connect re-binds 10808/10809. Linux tolerates
   a listen re-bind over non-listening sockets (proven with a standalone
   std probe — scripts/rebind_probe.rs); WINDOWS does NOT
   (WSAEADDRINUSE), so every picker switch after a busy session landed
   Failed and only process death (RST) freed the ports. Reproduced
   first: the regression test fails pre-fix (EOF never arrives) and
   passes post-fix (bug-first contract).
2. FIX: track every connection task's AbortHandle in `Shared.conns`
   (pruned at 4096 pushes; aborting finished tasks is a no-op), and at
   the END of the drain grace FORCE-CLOSE whatever wait_idle could not
   reap. Listeners re-bind after a `bind_with_retry` (5 s budget / 250 ms
   interval; BIND_WAIT 2→8 s) absorbing the kernel teardown race.
3. PUMP DETACHMENT: `tunnel` spawns two pump tasks; aborting the outer
   connection task merely DROPS their JoinHandles (detach), leaving the
   socket halves open — the EOF assertion caught it. `AbortPumpsOnDrop`
   aborts the pumps whenever the tunnel future dies (normal completion
   too: no-op).
4. H2 (same report family): engine config/log filenames keyed by tag
   alone collided across profiles sharing a `#fragment` — a temp
   probe/speed-test engine REWROTE the live engine's config and its next
   respawn dialed the wrong server into DEGRADED while the GUI showed
   Connected. `engine_file_stem` appends a deterministic FNV-1a hash of
   the full endpoint identity (same profile → same file across
   reconnects; different profiles → different files). Wire files gain a
   hash suffix; only mt-engine's lifecycle-test helper had to follow.
5. H3: profiles.json load→mutate→save cycles are now serialized by
   `Core::store_mtx` (GUI iced tasks run concurrently; the CLI can race
   the GUI; shared per-PID temp name collided on Windows).

## D30 — S21: QoS fair share by destination + live toggle; merged Status panel (2026-11-24)

User asks (verbatim): "Include a QoS toggle that can be turned on to
prevent a single application from dominating all the bandwidth." / "Have
the 'check exit ip' and 'speed test' in the same UI block, as there is a
lot of blank space in their individual blocks."

1. GROUPING: local flows all originate from 127.0.0.1 ephemeral ports —
   per-PROCESS attribution is impossible at the proxy layer. Flows group
   by TARGET (`host:port`): one site/app = one group regardless of how
   many connections it opens (a download manager spreading over many
   servers counts as several groups — documented in the Settings copy).
2. SHAPING: the existing 1 s ticker measures aggregate up+down and
   refills per-group token buckets at `aggregate/groups` ONLY under real
   contention (≥2 active groups AND aggregate ≥ 2 MiB/s floor). Pumps
   acquire a `PUMP_BUF`-sized budget before each read (20 ms retry until
   the next refill). One group alone / idle tunnels are UNLIMITED — no
   throughput tax without contention.
3. LIVE TOGGLE: the scheduler is ALWAYS attached at connect (initialized
   from settings), so `set_settings` flips an AtomicBool — NO reconnect
   (unlike diagnostics/kill-switch, nothing in the data plane changes).
   Disabling unblocks waiters and clears the table (tested).
4. MERGED PANEL: exit-IP + speed-test share ONE Status card (shared
   header, both triggers side-by-side Fill-width, result rows stacked
   with label prefixes "exit"/"speed"); all messages/state unchanged
   (pure view merge inside the D24 page scrollable).
5. P1 PARITY (same batch): TCP_NODELAY on all four carrier legs; ACK
   coalescing (8 frames / 20 ms, CLOSE+PING flush, cumulative semantics
   unchanged); PONG echoes on the receiving carrier (RTT was
   cross-contaminated); send-path copies 5→3 per frame (single-pass
   encoder, wire-identity test); carrier writer batches ≤16 frames per
   write_vectored; quarantine last-resort fallback (a probe can no
   longer blackhole the only link); production prober uses the
   user-configured probe target.
