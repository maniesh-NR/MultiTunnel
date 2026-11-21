# UI survey — changelogs of similar applications (S18, 2026-11-21)

Scope: what recent releases of comparable proxy/bonding clients added or
fixed on the UI/UX surface, and how each item maps to MultiTunnel
(already shipped / shipped in v0.12.0 / proposed). Sources are the
projects' own release notes; every claim below is anchored to the linked
page as it read on 2026-11-21.

## Sources reviewed

- **Clash Verge Rev** (Tauri-based Clash client) — releases page,
  v2.5.9 / v2.5.10 (Nov 2026):
  https://github.com/clash-verge-rev/clash-verge-rev/releases
- **v2rayN** (Xray/sing-box GUI, the import-compatibility reference for
  this project) — https://v2rayn.app + community changelog mirror.
- **FlClash** (multi-platform Clash client) — releases page:
  https://github.com/chen08209/FlClash/releases
- **Hiddify** (sing-box GUI, mobile-first) — Google Play release notes:
  https://play.google.com/store/apps/details?id=app.hiddify.com
- **Speedify** (commercial channel-bonding VPN — the closest analog to
  MultiTunnel's bonding) — Speedify 17 release notes:
  https://speedify.com/blog/release-notes/introducing-speedify-17

## Findings → MultiTunnel mapping

### Already shipped in MultiTunnel (parity confirmed)

| Feature (source) | MultiTunnel state |
|---|---|
| Latency tests per node; auto-select fastest (CVR, FlClash) | Servers screen per-profile latency test (S9) |
| Live traffic speeds surfaced, incl. tray (CVR tray speed discussions, v2rayN traffic stats) | Titlebar live global rates + tray hover tooltip (S17/S18) |
| Theme customization (FlClash themes, CVR custom CSS) | M3 seed theming; v0.12.0 adds the color WHEEL + system/light/dark mode |
| Dark/light mode (all clients) | v0.12.0 adds System mode following the OS |
| Log viewing with tail tracking (v2rayN, CVR) | Logs viewport + autoscroll + floating jump buttons (S17) |
| Adapter/link visibility (Speedify connection list) | Status links card + per-adapter graphs (S10/S13) |

### Shipped in v0.12.0 (this survey's direct asks)

- **Tray hover = name+version / totals / live speeds** (v2rayN and
  Speedify both surface session traffic in one glance; our previous
  tooltip was a single dense line).
- **Clear-background, larger tray/taskbar icons** (Speedify's icon
  legibility at small sizes; the old opaque tile read as a blank square).
- **Color wheel accent picker** (FlClash's theme work moved toward free
  color choice rather than fixed palettes).
- **System / Light / Dark mode** (universal in the 2025-2026 releases of
  all five clients).

### Proposed next (prioritized)

1. **Per-link live status + Signal-style ping quality badges**
   (Hiddify's headline home-screen rework: "live status for every proxy,
   Signal-style ping indicators"). MultiTunnel has per-profile latency
   on demand; a continuous 3-level badge (good/ok/bad) on Status link
   rows + Servers rows is a small poll loop away, and directly answers
   "which carrier is degraded" mid-session.
2. **Exit-IP auto-refresh toggle** (CVR v2.5.9 added an "IP Info
   auto-refresh" toggle). Our exit probe is manual; a Settings toggle +
   10-min refresh while Connected is cheap and mirrors theirs.
3. **Empty states** (FlClash: "new app glyphs, empty states"). Servers
   and Logs render bare cards when empty; a centered hint + primary
   action ("paste your first vless:// link") would orient new users.
4. **Streaming/AI unlock detection** (CVR's test suite incl. "Muse AI
   unlock detection") — out of scope for a private tunnel, but a
   "does the tunnel actually reach X" one-click URL test list belongs in
   a Diagnostics card; the exit probe already proves the model.
5. **Update policy controls** (v2rayN: pick release channel/version;
   CVR installer polish). mt-platform's update skeleton exists
   (`latest_release_for(CARGO_PKG_VERSION)`); a Settings "check for
   updates" button + channel choice is the UI half of that.
6. **Traffic statistics durability** (v2rayN repeatedly fixing traffic
   stats display). Our session_bytes reset each connect; persisting
   cumulative per-profile totals in settings.json would give a
   "lifetime" row on Servers without schema risk (new optional field).
7. **Bufferbloat guard visibility** (Speedify 17 fixed over-aggressive
   bufferbloat protection). When 2.4(a) of the bonding survey lands
   (RTT trend in the score), surface a "latency rising" chip on the
   affected adapter row — invisible protection is undervalued by users.

### Deliberately not adopted

- **TUN mode / virtual adapters** (CVR, FlClash): a system-wide packet
  layer is a different risk class (driver-level, admin, AV friction);
  the SOCKS/HTTP listeners + system proxy cover the use cases this
  project targets.
- **Unlock-test catalogs** (CVR's streaming/AI matrix): content-service
  fingerprints rot quickly and imply a maintenance stream this
  one-developer project should not take on.
- **Custom CSS** (CVR): the M3 theme engine already exposes the levers
  users actually change (accent, mode).
