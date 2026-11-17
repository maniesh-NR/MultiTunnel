# S6 — All-in-One MultiTunnel (v0.3): contracts per expert

Stage goal (user directive, 2026-11-17): MultiTunnel becomes an **all-in-one
app with zero user-installed dependencies**, combining the functions of
v2rayN and net-merge out of the box, with a **Material You GUI** and a
**Rust core**, built **Mixture-of-Experts style**: parallel expert agents,
one crate each, orchestrator owns the workspace, integration and reviews.

## Global rules for every expert (read first)

1. **Scope**: touch ONLY files under your own crate directory
   (`crates/<your-crate>/`). Never edit the workspace root `Cargo.toml`,
   `src/`, other crates, CI, or harness files. If you believe a file
   outside your crate must change, REPORT it in your final message instead.
2. **Toolchain**: run `. /home/z/my-project/tools/env.sh` FIRST in every
   shell. Build/test with your own target dir to avoid lock contention:
   `CARGO_TARGET_DIR=/tmp/target-<crate> cargo test -p <crate>`.
   Rust 1.100.0; zigbuild available for cross-checks:
   `cargo zigbuild --target x86_64-pc-windows-gnu -p <crate>`.
3. **Gates** (your crate must pass all before you finish):
   - `cargo fmt --check -p <crate>`
   - `cargo clippy --all-targets -p <crate> -- -D warnings`
   - windows-gnu target clippy (needs the zig shims — one-time setup):
     ```sh
     sh scripts/mingw-zig-shims.sh /tmp/shims
     export CC_x86_64_pc_windows_gnu=/tmp/shims/zig-cc \
            CXX_x86_64_pc_windows_gnu=/tmp/shims/zig-cxx \
            AR_x86_64_pc_windows_gnu=/tmp/shims/zig-ar
     cargo clippy --all-targets -p <crate> --target x86_64-pc-windows-gnu -- -D warnings
     ```
   - `cargo test -p <crate>`
   (Dependencies that cannot cross-compile to windows-gnu must be
   feature-gated or replaced — report if you hit one.)
4. **Security (D9/D10)**: no real credentials anywhere — tests use the
   synthetic vectors in this doc (RFC 5737 addresses, dummy UUIDs). Error
   messages must never echo credential values (uuid/password/pbk), only
   lengths/positions. Files that embed credentials are written 0600 POSIX.
5. **Style**: match the existing repo discipline — module doc comment with
   CONTRACT + IMPLEMENTATION NOTES sections, poison-tolerant locks, no
   `unwrap()` in non-test code paths, `thiserror` for library errors,
   `tracing` for diagnostics (no println! in libs).
6. **Sandbox hazards** (known incidents): tool output is filtered (verify
   bracketed literals with `od -An -c` if a test looks impossible);
   MultiEdit can PARTIALLY apply before reporting failure — re-read the
   file after any failed edit.
7. **Async**: tokio everywhere (`rt-multi-thread`), `async_trait` for
   object-safe traits. Library code must not create its own runtime.
8. Report at the end: public API surface, test count, design decisions
   ("pick one and document it"), deviations from this contract, and
   anything the orchestrator must wire.

## Crate map and dependency graph

```
crates/mt-links     (Expert A)  share links, profiles, subscriptions   [standalone]
crates/mt-net       (Expert B)  SOCKS5/HTTP inbound, router, aggregator [standalone]
crates/mt-engine    (Expert E)  managed Xray engine per link            [depends: mt-net]
crates/mt-m3        (Expert C1) Material You design tokens              [standalone]
crates/mt-platform  (Expert D)  dirs, engine locator, autostart         [depends: mt-engine]
crates/mt-service   (orchestrator, DONE) UI service contract            [standalone]
crates/mt-app       (Expert C2, wave 2) iced Material You GUI           [depends: mt-m3, mt-service, mt-links]
root multitunnel    (orchestrator, wave 3) CLI + RealService wiring     [depends: all]
```

The root package `multitunnel` keeps the existing v0.2 behavior (supervisor,
tray, import, doctor, connect) unchanged and green at every commit.

## Shared synthetic test vectors (safe, format-valid)

```
vless : vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=reality&sni=tunnel.example.org&fp=chrome&pbk=0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde&sid=0123abcd&type=tcp&flow=xtls-rprx-vision#synthetic-reality
vmess : base64url({"v":"2","ps":"synthetic","add":"203.0.113.10","port":"443","id":"aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee","aid":"0","net":"tcp","type":"none","host":"","path":"","tls":"tls","sni":"tunnel.example.org","scy":"auto"})
ss    : ss://YWVzLTI1Ni1nY206c3ludGhldGlj@203.0.113.10:8388#synthetic-ss
trojan: trojan://synthetic-password@203.0.113.10:443?sni=tunnel.example.org#synthetic-trojan
```

## Expert A — `crates/mt-links`

Multi-protocol share links, the multi-server profile store, subscriptions.
v2rayN-import compatibility is the bar. Port the strictness discipline from
`src/vless.rs` (READ it first — its parse policy is D10).

- `Protocol { Vless, Vmess, Trojan, Shadowsocks }`
- `Transport { Tcp, Ws { host: Option<String>, path: String }, Grpc { service: String }, HttpUpgrade { host: Option<String>, path: String } }`
- `Security { None, Tls { sni: Option<String>, alpn: Vec<String>, allow_insecure: bool }, Reality { sni: String, fingerprint: String, public_key: String, short_id: String, spider_x: Option<String> } }`
- `Profile { id: String (uuid v4), tag: String, protocol, address: String, port: u16, identity: String, flow: Option<String>, transport: Transport, security: Security, remark: String }`
- `parse_share_link(&str) -> Result<Profile, LinkError>`: vless:// (accept
  tcp/ws/grpc/httpupgrade × none/tls/reality; keep D10 strictness for what
  we EXPORT, but PARSE liberally and record), vmess:// (v2rayN base64 JSON,
  both standard and padded/unpadded base64), ss:// (SIP002
  `ss://base64(method:password)@host:port#tag` and legacy full-base64),
  trojan://. Unknown params ignored. Errors never echo credentials.
- `render_share_link(&Profile) -> Result<String, LinkError>`: vless:// and
  trojan:// and ss:// (SIP002). Rendering must round-trip through parse.
- `parse_subscription(body: &str) -> (Vec<Profile>, Vec<String>)` — pure:
  body is either base64 (whole) or plain lines; one link per line; returns
  parsed + human-readable failure lines (no credential echo).
- `fetch_subscription(url: &str) -> Result<(Vec<Profile>, Vec<String>), LinkError>`
  behind default feature `fetch` (reqwest, rustls-tls, no default features).
- `ProfileStore` (file `profiles.json`):
  `{ "version": 1, "active": Option<String>, "profiles": [Profile] }`
  load/save; save is atomic (tmp file + rename, 0600 POSIX); upsert dedupes
  by `(protocol, address, port, identity)` (same identity → update tag/etc.);
  `set_active`, `remove`, `list`; missing file → empty store.
- `migrate_legacy_server(v: &serde_json::Value) -> Result<Profile, LinkError>`
  — from the v0.2 `config.json` `server` block
  (address/port/uuid/sni/publicKey/shortId → Vless+Reality profile).
- Tests: round-trips for all four protocols incl. both vectors; dedupe;
  atomic save survives a simulated crash (no partial file); subscription
  plain + base64; migration golden; error messages redact credentials.
- Deps allowed: serde, serde_json, thiserror, base64, uuid (v4+serde),
  url, percent-encoding, reqwest (feature-gated), tracing.

## Expert B — `crates/mt-net`

The native Rust data plane. Everything the external `net-merge` binary did
for connection spreading now happens in-process; the external binary stays
supported by the root crate (S7 decides its fate).

```rust
pub enum Host { Domain(String), Ip(std::net::IpAddr) }
pub struct TargetAddr { pub host: Host, pub port: u16 }

pub trait AsyncStream: tokio::io::AsyncRead + tokio::io::AsyncWrite
    + Unpin + Send {}
// + blanket impl for every T: ...
pub type ProxyStream = Box<dyn AsyncStream>;

#[async_trait::async_trait]
pub trait Outbound: Send + Sync {
    fn name(&self) -> &str;
    async fn connect(&self, target: &TargetAddr) -> anyhow::Result<ProxyStream>;
    async fn probe(&self, target: &TargetAddr) -> anyhow::Result<std::time::Duration>;
}
```

- `DirectOutbound` — plain TCP connect (happy eyeballs not required).
- `Socks5Outbound { addr: SocketAddr }` — SOCKS5 client, no-auth, domain
  ATYP; reuse the wire format from `src/core_proc.rs::socks_probe` (READ
  it) but async and returning the stream instead of dropping it.
- SOCKS5 **server** (`socks5.rs`): RFC 1928, no-auth only, CONNECT only
  (BIND/UDP → proper error replies), domain + IPv4 + IPv6 ATYP, correct
  reply drain; concurrent connections; read timeouts.
- HTTP **proxy server** (`http.rs`): absolute-form requests + CONNECT
  tunneling; 502 on upstream failure; header sizes bounded.
- Router (`router.rs`):
  `RuleMatcher::{DomainSuffix(String), DomainKeyword(String), IpCidr(ipnet::IpNet), Any}`
  → `Action::{Proxy, Direct, Reject}`; ordered first-match; serde
  (camelCase JSON) for the rule list; default action when nothing matches
  (configurable, default Proxy); domain rules match the suffix at label
  boundaries (`evil-example.com` must NOT match suffix `example.com`).
- Aggregator (`merge.rs`): `MergePolicy::{Failover, RoundRobin, LowestLatency}`;
  `LinkHealth { ok: bool, latency: Option<Duration>, ewma_ms: f32,
  consecutive_failures: u32, quarantined_until: Option<Instant> }`;
  quarantine after 3 consecutive failures for 30 s; `pick(&mut healths,
  policy, counter)` pure + testable; per-link background prober task
  (interval configurable) updating healths through `Arc<Mutex<...>>` or a
  small actor — pick one and document it.
- `ProxyServer` (`server.rs`): owns listeners (socks addr + http addr),
  `Vec<Arc<dyn Outbound>>`, `Router`, `MergePolicy`; `run(shutdown:
  tokio::sync::watch::Receiver<bool>)` serves until shutdown flips;
  per-connection flow: parse target → router → pick link (or direct/reject)
  → connect (with one transparent retry on a DIFFERENT link if the first
  connect fails before any byte flows) → bidirectional copy with byte
  counters; emits `ProxyEvent` on a `tokio::sync::broadcast::Sender`:
  `Started`, `Stopped`, `ConnOpened { target: String, link: String }`,
  `ConnClosed { link, up: u64, down: u64, dur_ms: u64 }`,
  `LinkHealth { link, health }`, `Tick { up_bps, down_bps }` (1 s).
- Fail-open at the data plane: if ALL links fail, connections error, but
  the server keeps listening (never unwrap-panics a listener).
- Tests: socks5 client↔server loopback E2E (target echo server), http
  CONNECT E2E, router ordering + label boundary + CIDR, aggregator
  policies incl. quarantine/failover with fake outbounds (one always
  failing), retry-on-fresh-connect, broadcast events observed, graceful
  shutdown closes listeners promptly.
- Deps allowed: tokio (full), async-trait, anyhow, thiserror, bytes,
  futures, ipnet, serde, serde_json, tracing.

## Expert E — `crates/mt-engine`

Managed Xray engine: one child process per outbound link, driven by config
JSON, exposed to mt-net as an `Outbound`. Port the lifecycle discipline
from `src/core_proc.rs` (READ it: waiter semantics, log pumping,
CREATE_NO_WINDOW) into async.

```rust
pub struct VlessRealityEndpoint {
    pub address: String, pub port: u16, pub uuid: String,
    pub sni: String, pub public_key: String, pub short_id: String,
    pub fingerprint: String,           // "chrome"
    pub spider_x: Option<String>,      // default "/"
}
pub struct EngineSpec { pub tag: String, pub endpoint: VlessRealityEndpoint }

#[async_trait::async_trait]
pub trait EngineLocator: Send + Sync {
    /// Path already on disk, if any (never downloads).
    fn locate(&self) -> Option<std::path::PathBuf>;
    /// Path on disk, downloading if necessary.
    async fn ensure(&self) -> anyhow::Result<std::path::PathBuf>;
}
```

- `generate_config(spec: &EngineSpec, socks_listen: SocketAddr) ->
  serde_json::Value` — ONE loopback socks inbound (noauth, udp: false) +
  ONE vless outbound (mux `"enabled": false` — the crate-root doc explains
  WHY, copy the reasoning from `src/xrayconf.rs`), streamSettings reality
  (serverName/fingerprint/publicKey/shortId/spiderX), log warning level.
  No freedom/direct outbound, no routing rules — the Rust router in
  mt-net decides what reaches the engine.
- `ManagedEngine::new(spec, locator: Arc<dyn EngineLocator>, state_dir,
  log_dir)`: lazily spawns on first `connect`; free loopback port picked
  by bind(127.0.0.1:0)-then-drop (retry once if the child fails to bind);
  config written to `state_dir/engine-<tag>.json` 0600 (it embeds the
  UUID); child `xray run -c <path>` with piped stdout/stderr appended to
  `log_dir/engine-<tag>.log` + forwarded to tracing; kill-on-drop;
  unexpected-exit detection (connect failure + dead child) → respawn with
  backoff 1s→2s→…→30s, budget 5, then the outbound errors (fail-open at
  the data plane level is mt-net's business, not ours).
- `#[async_trait] impl Outbound for ManagedEngine` — connects through its
  loopback socks via `Socks5Outbound`; `probe` = full socks CONNECT to a
  given target through the engine and back (measures the real tunnel RTT).
- `with_command(test_hook)`: an alternate binary+args for tests. Tests use
  a tiny fake engine: a python3 script (shipped under `crates/mt-engine/
  tests/fake_engine.py`) that listens on the socks port, accepts + logs to
  stderr, and can be told to exit. NO real xray binary in tests.
- Tests: golden config JSON (mux off, reality fields, loopback-only
  inbound, udp false); lifecycle: spawn → port up → connect succeeds →
  kill child → auto-respawn → budget exhaustion errors; kill-on-drop;
  0600 perms on the written config (unix); config file never appears in
  logs; windows-gnu clippy green (CREATE_NO_WINDOW under cfg(windows)).
- Deps allowed: tokio (process, net, time, sync, fs, macros), async-trait,
  anyhow, thiserror, serde, serde_json, tracing, mt-net (path dep).

## Expert C1 — `crates/mt-m3`

Material You (Material Design 3) design tokens as a pure Rust library +
optional iced bridge. Goal: mt-app can build a fully themed GUI with
dynamic color from a seed, like Android 12+ wallpaper theming.

- Try the `material-colors` crate (a Rust port of Google's
  material-color-utilities). If it is unavailable or a poor fit, implement
  a documented OKLCH-based tonal-palette approximation (mark it as an
  approximation in the module docs). Either way the PUBLIC API is:
- `M3Palette::from_seed(argb: u32) -> M3Palette` (deterministic);
  `palette.scheme(dark: bool) -> M3Scheme`;
- `M3Scheme` — every one of these tokens, `pub struct M3Color { r, g, b }`:
  primary, on_primary, primary_container, on_primary_container,
  secondary, on_secondary, secondary_container, on_secondary_container,
  tertiary, on_tertiary, tertiary_container, on_tertiary_container,
  error, on_error, error_container, on_error_container,
  background, on_background, surface, on_surface,
  surface_variant, on_surface_variant,
  surface_container_lowest, surface_container_low, surface_container,
  surface_container_high, surface_container_highest,
  surface_dim, surface_bright, outline, outline_variant,
  inverse_surface, on_inverse_surface, inverse_primary, scrim, shadow.
- `M3TypeScale` — the M3 roles (display_l/m/s, headline_l/m/s, title_l/m/s,
  body_l/m/s, label_l/m/s) each `{ size: f32, weight: u16, line_height: f32 }`
  per the M3 spec.
- `M3Shape { extra_small: 4.0, small: 8.0, medium: 12.0, large: 16.0,
  extra_large: 28.0 }`.
- `state_layer(base: M3Color, state: StateLayer) -> M3Color` — hover 8%,
  focus 10%, pressed 10% (blend semantics documented).
- Feature `iced` (optional): `impl From<M3Color> for iced::Color` and a
  helper `M3Palette::iced_theme(&self, dark: bool) -> iced::Theme` mapping
  onto iced 0.13's custom palette. Keep the bridge tiny.
- Tests: determinism (same seed → same scheme), all tokens present and
  light/dark differ, on/off pairs have a sane luminance relationship
  (on_primary is legible on primary: relative-luminance contrast ≥ 3.0),
  type scale monotonic (display_l > … > label_s), state layers change the
  color. No network at test time.
- Deps allowed: material-colors (optional), iced 0.13 (optional, feature
  `iced` only), thiserror. Nothing else.

## Expert D — `crates/mt-platform`

Platform integration that makes MultiTunnel self-provisioning: app dirs,
engine locator (locate/verify/download/embed), autostart, update check.
sysproxy/elevate stay in the root crate this stage (do not port them).

- `dirs.rs`: `config_dir()`, `cache_dir()`, `log_dir()`, `state_dir()` ->
  PathBuf (Windows: `%APPDATA%`/`%LOCALAPPDATA%` under a `MultiTunnel`
  folder; Linux: XDG base dirs under `multitunnel`; create on demand;
  env overrides `MULTITUNNEL_CONFIG_DIR` etc. for tests). Pure, tested.
- `engine.rs`: `DefaultEngineLocator` implementing `mt_engine::
  EngineLocator` (READ that trait — signature must match exactly):
  resolution order: (1) `$MULTITUNNEL_ENGINE` env, (2)
  `<exe_dir>/engine/xray(.exe)`, (3) `<config_dir>/engine/xray(.exe)` with
  sha256 verification against `engine/SHA256SUMS` (mismatch → warn by
  default, refuse when `MULTITUNNEL_ENGINE_STRICT=1`), (4) download: query
  `$MULTITUNNEL_ENGINE_URL` (direct zip URL) or GitHub releases latest
  (`XTLS/Xray-core`, api.github.com, `Xray-windows-64.zip` / `Xray-linux-
  x86_64.zip` per OS), extract with the `zip` crate, install binary + any
  geoip.dat/geosite.dat to `<config_dir>/engine/`, write `SHA256SUMS`,
  chmod 0755. HTTP behind a small `trait HttpFetch` so tests inject
  fixtures (reqwest impl behind default feature `net`).
  `locate()` NEVER downloads; `ensure()` may.
- Feature `embedded` + `build.rs`: when `MT_EMBED_ENGINE=<path-to-zip>` is
  set at build time, embed the zip with `include_bytes!` and extract on
  first `ensure()` (documented release path; absent env → feature compiles
  but `embedded()` returns None).
- `autostart.rs`: `enable/disable/is_enabled(app_path)` — Windows: HKCU
  Run key via winreg (cfg-gated; pure "what value would we write" fn
  tested on Linux); Linux: `~/.config/autostart/multitunnel.desktop`
  (content fn pure + tested).
- `update.rs`: `latest_release() -> Result<Option<String>>` — GitHub API
  `releases/latest` tag compare; NO auto-apply, no secrets.
- Tests: dirs env overrides; SHA256SUMS verify happy/mismatch; zip extract
  from an in-test-generated zip; autostart desktop content; locator order
  via temp dirs + env; embedded extraction from a fixture zip. No network
  in tests (HttpFetch fixtures).
- Deps allowed: anyhow, thiserror, serde, serde_json, sha2, hex, zip,
  tracing, reqwest (feature `net`), winreg (cfg windows), mt-engine
  (path dep), dirs crate NOT allowed (hand-rolled).

## Expert C2 — `crates/mt-app` (wave 2; contract published now)

iced 0.13 Material You GUI. Depends on mt-m3 (feature `iced`), mt-service,
mt-links (parse preview). Ships `MockService` for standalone development
(feature `mock`, default on in dev). Screens: Status (state card, links
with latency chips, connect switch, throughput), Servers (profile list,
add-by-paste with parse preview via mt-links, set active, latency test,
delete, export link), Routing (rule table editor), Settings (theme seed
picker + dark toggle, system proxy, merge policy, autostart), Logs
(streaming view with level filter). Left M3 navigation rail with active
pill indicator; M3 buttons (filled/tonal/outlined/text), switches, text
fields, cards on tonal surfaces; live re-theme on seed change. `--smoke`
flag: run 3 s headless then exit 0 (CI). MUST verify
`cargo zigbuild --target x86_64-pc-windows-gnu -p mt-app` green
(prefer the tiny-skia renderer over wgpu for cross-compile robustness;
switch only if the feature set forces wgpu, and then prove it links).
Detailed per-widget contract is issued with the wave-2 prompt.

## Orchestrator (wave 3)

Root crate wiring: `multitunnel gui` launches mt-app with the real
service; `profiles list/add/use/export/test` CLI over mt-links +
mt-platform; RealService (mt-service impl) gluing ProfileStore +
mt-net::ProxyServer + mt-engine + mt-platform + legacy sysproxy; README +
QUICKSTART updates; all gates host+target; two adversarial reviews (D6);
release artifacts.

## Stage acceptance (close)

All crates green on: fmt, clippy host + windows-gnu (-D warnings), cargo
test --workspace, zigbuild --workspace (PE32+ console + tray + gui).
Adversarial review 2×APPROVE per D6. DECISIONS/STATE/LEDGER updated,
backup pushed. The v0.3 backlog P2s from v0.2 remain parked unless free.
