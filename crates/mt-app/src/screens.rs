//! Screen views: Status / Servers / Adapters / Routing / Settings / Logs +
//! nav rail.
//!
//! Functionality-level fidelity per the wave-2 contract (not pixel-perfect):
//! M3 rail with active pill, cards on tonal surfaces, filled/tonal/outlined/
//! text buttons, chips, switches, text fields, pick lists, checkboxes (S7
//! Adapters screen).

use iced::widget::{
    button, canvas, checkbox, column, container, horizontal_space, image, mouse_area, pick_list,
    row, scrollable, stack, text, text_input, toggler, vertical_space, Canvas,
};
use iced::{Length, Padding};
use mt_service::{ProfileView, ServiceState, ThemeMode};

use crate::app::{
    App, EditField, LatencyState, LogFilter, Message, RuleRow, Screen, TitleMenuItem, BOND_MODES,
    LOG_LEVELS, MERGE_POLICIES, RATE_HISTORY_LEN, RULE_ACTIONS, RULE_KINDS,
};
use crate::graph::{RateGraph, RateGraphStyle};
use crate::theme as t3;
use crate::util;
use crate::Element;

// ---------------------------------------------------------------------------
// Shared scaffolding
// ---------------------------------------------------------------------------

/// The embedded 24×24 titlebar emblem (RGBA8; `scripts/make_logo_assets.py`).
const LOGO_24: &[u8] = include_bytes!("../../../assets/logo_24.rgba");

/// S15 (D25): the app-drawn title bar — the window runs with
/// `decorations: false`, so this strip IS the chrome. Chromium-style: the
/// strip sits on the page surface (no separator, it blends), the logo +
/// title live on the left, the window controls on the right, and every
/// non-button pixel drags the window (double-press maximizes).
pub fn titlebar(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;
    let logo = image(image::Handle::from_rgba(24, 24, LOGO_24.to_vec()))
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(20.0));
    let title = text("MultiTunnel")
        .size(ts.title_m.size)
        .color(s.on_surface_variant);
    // S17: the release version rides next to the name — the same number
    // the release zip carries (workspace-inherited CARGO_PKG_VERSION).
    let version = text(format!("v{}", crate::app::app_version()))
        .size(ts.label_m.size)
        .color(s.on_surface_variant);
    let identity = row![logo, title]
        .spacing(10)
        .align_y(iced::Alignment::Center);
    let identity = row![identity, version]
        .spacing(6)
        .align_y(iced::Alignment::Center);
    // S17: live global rates on the strip's right, just before the window
    // controls — the same 1 s throughput sample the tray tooltip reports
    // (down in tertiary, up in primary; zeros before the first sample).
    let (up_bps, down_bps) = app.throughput.unwrap_or((0, 0));
    let speeds = row![
        text(format!("↓ {}", util::fmt_bps(down_bps)))
            .size(ts.label_m.size)
            .color(s.tertiary),
        text(format!("↑ {}", util::fmt_bps(up_bps)))
            .size(ts.label_m.size)
            .color(s.primary),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);
    // The whole strip (minus the buttons — inner widgets capture first)
    // drags the window; a fast second press maximizes; a RIGHT press
    // opens the window menu (S16). The speeds cluster is plain text —
    // it drags like the rest of the strip.
    let strip = row![identity, horizontal_space().width(Length::Fill), speeds,]
        .align_y(iced::Alignment::Center)
        .width(Length::Fill);
    let drag = mouse_area(
        container(strip)
            .width(Length::Fill)
            .height(Length::Fill)
            // S16: the identity row is Shrink-height; without this it sat
            // flush at the strip's TOP (hidden with the old opaque emblem
            // tile, obvious with the transparent one).
            .align_y(iced::Alignment::Center)
            .padding(Padding::new(0.0).left(14.0).right(12.0)),
    )
    .on_press(Message::TitleDrag)
    .on_right_press(Message::TitleContext);
    let make_ctl = |label: &str, msg: Message| {
        button(text(label.to_string()).size(ts.headline_s.size))
            .style(t3::text_button(s))
            .padding(Padding::new(0.0).left(14.0).right(14.0))
            .height(Length::Fill)
            .on_press(msg)
    };
    let max_glyph = if app.window_maximized { "▣" } else { "□" };
    let controls = row![
        make_ctl("—", Message::TitleMinimize),
        make_ctl(max_glyph, Message::TitleMaximize),
        make_ctl("×", Message::TitleClose),
    ]
    .height(Length::Fill);
    container(row![drag, controls].height(Length::Fill))
        .height(Length::Fixed(40.0))
        .width(Length::Fill)
        .into()
}

/// S16 (D26): the titlebar right-click window menu as a full-window
/// overlay layer (rendered on top of the app via `iced::widget::stack`).
///
/// The layer is a full-size mouse_area: a click ANYWHERE closes the menu
/// — except on the menu rows themselves, whose buttons capture the
/// release first (child-before-parent event order in `mouse_area`), so
/// the picked action fires exactly once. `on_release` (not `on_press`)
/// gives native click semantics and avoids the press-outside/release-
/// inside half-click dismissing.
pub fn title_menu_overlay(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;
    let (mw, _) = App::menu_bounds();
    let anchor = app
        .title_menu
        .expect("overlay rendered only while the menu is open");
    let on_top = app.settings.always_on_top;

    let check = |on: bool| -> Element<'_> {
        text(if on { "✓ " } else { "   " }.to_string())
            .size(ts.body_m.size)
            .color(s.primary)
            .into()
    };
    // Local helpers with EXPLICIT lifetimes: the returned element borrows
    // the glyph (and nothing else — labels are copied into owned text).
    fn item<'a>(
        glyph: Element<'a>,
        label: &str,
        msg: Message,
        s: mt_m3::M3Scheme,
        ts: &mt_m3::M3TypeScale,
    ) -> Element<'a> {
        button(
            row![glyph, text(label.to_string()).size(ts.body_m.size)]
                .spacing(6)
                .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .style(t3::menu_item(s))
        .padding(Padding::new(6.0).left(10.0).right(10.0))
        .on_press(msg)
        .into()
    }
    fn rule<'a>(s: mt_m3::M3Scheme) -> Element<'a> {
        container(horizontal_space().width(Length::Fill))
            .height(Length::Fixed(1.0))
            .width(Length::Fill)
            .style(t3::menu_rule(s))
            .into()
    }
    let max_label = if app.window_maximized {
        "Restore"
    } else {
        "Maximize"
    };
    let menu = column![
        item(
            check(on_top),
            "Always on top",
            Message::TitleMenu(TitleMenuItem::AlwaysOnTop),
            s,
            &ts,
        ),
        rule(s),
        item(
            check(false),
            "Minimize",
            Message::TitleMenu(TitleMenuItem::Minimize),
            s,
            &ts,
        ),
        item(
            check(false),
            max_label,
            Message::TitleMenu(TitleMenuItem::Maximize),
            s,
            &ts,
        ),
        rule(s),
        item(
            check(false),
            "Close",
            Message::TitleMenu(TitleMenuItem::Close),
            s,
            &ts,
        ),
    ]
    .spacing(2)
    .padding(Padding::new(6.0))
    .width(Length::Fixed(mw));

    // Position at the clamped anchor via container padding (top/left).
    let card = container(menu)
        .style(t3::menu_card(s))
        .width(Length::Fixed(mw))
        .height(Length::Shrink);
    let positioned = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::new(0.0).top(anchor.y).left(anchor.x));
    mouse_area(positioned)
        .on_release(Message::TitleContextDismiss)
        .into()
}

/// The left M3 navigation rail with the active pill indicator.
pub fn nav_rail(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;
    let mut rail = column![].spacing(6).padding(Padding::new(10.0));
    for scr in Screen::ALL {
        let active = app.screen == scr;
        let btn = button(text(scr.label()).size(ts.label_m.size))
            .style(t3::rail_button(active, s))
            .padding(Padding::new(10.0).left(18.0).right(18.0))
            .width(Length::Fixed(96.0))
            .on_press(Message::Screen(scr));
        rail = rail.push(btn);
    }
    container(rail)
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fixed(120.0))
        .height(Length::Fill)
        .into()
}

/// Screen scaffold WITHOUT its own scrollable (Status): the body owns
/// Fill heights — a fixed connection frame on top and a page scrollable
/// below it (S13). The graph canvas must scale with the window, so it
/// cannot live inside a scrollable (D19 §3).
fn scaffold_fixed<'a>(app: &'a App, title: &'a str, body: Element<'a>) -> Element<'a> {
    let ts = app.theme.ts;
    let inner = column![
        text(title)
            .size(ts.headline_s.size)
            .color(app.theme.scheme.on_background),
        body,
    ]
    .spacing(16)
    .width(Length::Fill)
    .height(Length::Fill);
    container(inner)
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .max_width(980)
        .into()
}

/// Screen scaffold: headline + body column, scrollable.
fn scaffold<'a>(app: &'a App, title: &'a str, body: Element<'a>) -> Element<'a> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;
    let inner = column![
        text(title).size(ts.headline_s.size).color(s.on_background),
        body,
    ]
    .spacing(16)
    .width(Length::Fill);
    scrollable(
        container(inner)
            .padding(24)
            .width(Length::Fill)
            .max_width(980),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// A color at reduced opacity (graph fills / grid).
fn translucent(mut c: iced::Color, a: f32) -> iced::Color {
    c.a = a;
    c
}

/// A chip: small rounded container with a label.
fn chip<'a>(
    label: impl Into<String>,
    app: &'a App,
    bg: mt_m3::M3Color,
    fg: mt_m3::M3Color,
) -> Element<'a> {
    container(text(label.into()).size(app.theme.ts.label_m.size).color(fg))
        .padding(Padding::new(4.0).left(10.0).right(10.0))
        .style(t3::chip(app.theme.scheme, bg))
        .into()
}

/// Filled / tonal / outlined / text button shorthand.
fn m3_button<'a>(
    label: &str,
    enabled: bool,
    kind: ButtonKind,
    on_press: Message,
    app: &App,
) -> Element<'a> {
    let s = app.theme.scheme;
    let base = text(label.to_string()).size(app.theme.ts.label_l.size);
    let mut b = button(base).padding(Padding::new(8.0).left(16.0).right(16.0));
    b = match kind {
        ButtonKind::Filled => b.style(t3::filled_button(s)),
        ButtonKind::Tonal => b.style(t3::tonal_button(s)),
        ButtonKind::Outlined => b.style(t3::outlined_button(s)),
        ButtonKind::Text => b.style(t3::text_button(s)),
        ButtonKind::ErrorText => b.style(t3::error_text_button(s)),
    };
    if enabled {
        b.on_press(on_press).into()
    } else {
        b.into()
    }
}

#[derive(Debug, Clone, Copy)]
enum ButtonKind {
    Filled,
    Tonal,
    Outlined,
    Text,
    ErrorText,
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Status screen (S9 / D19, D22, S13): ONE connection frame on top —
/// tunnel state text, the connect toggle and the server picker stacked
/// in a fixed-width left column, with the global bandwidth graph filling
/// all remaining frame space — and, below it, ONE page scrollable owning
/// everything else: the exit-probe card, the tunnel links, and the
/// per-adapter rows (press an adapter's name to expand its own traffic
/// graph). The wheel therefore scrolls EVERYTHING under the connection
/// frame (S13: the old inner-scrollable bottom card capped the visible
/// adapter rows at the space left after three stacked cards).
pub fn status(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    let (state, state_bg, state_fg) = match app.status.as_ref().map(|x| x.state) {
        Some(ServiceState::Connected) => ("Connected", s.primary_container, s.on_primary_container),
        Some(ServiceState::Connecting) => (
            "Connecting…",
            s.secondary_container,
            s.on_secondary_container,
        ),
        Some(ServiceState::Failed) => ("Failed", s.error_container, s.on_error_container),
        _ => (
            "Disconnected",
            s.surface_container_high,
            s.on_surface_variant,
        ),
    };

    let snap = app.status.clone();
    let profile_line = snap
        .as_ref()
        .and_then(|x| x.active_profile.clone())
        .map_or_else(|| "no active server".to_string(), |p| format!("via {p}"));
    let meta_line = snap.as_ref().map_or_else(String::new, |x| {
        let since = x.up_since.clone().unwrap_or_else(|| "—".into());
        format!("up since {since}   ·   restarts: {}", x.restarts)
    });

    // --- server picker (D19 §2) -------------------------------------------
    let active_tag = snap
        .as_ref()
        .and_then(|x| x.active_profile.clone())
        .unwrap_or_default();
    let tags: Vec<String> = app.profiles.iter().map(|p| p.tag.clone()).collect();
    let selected = tags.iter().find(|t| **t == active_tag).cloned();
    let picker = pick_list(tags.clone(), selected, |t: String| {
        Message::StatusServerPicked(t)
    })
    .width(Length::Fixed(260.0))
    .padding(6)
    .style(t3::pick_list_style(s))
    .menu_style(t3::menu_style(s));
    let picker_label = if tags.is_empty() {
        "no servers — add one on the Servers tab".to_string()
    } else if active_tag.is_empty() {
        "pick a server".to_string()
    } else {
        String::new()
    };

    // --- connection frame, left column (S13): state text, connect
    // toggle and server picker STACKED in a fixed-width column. The old
    // layout parked the toggle and the picker at opposite ends of a
    // Fill row — a wide dead gap between them — and kept the graph in
    // its own card below; the graph now takes that free space instead.
    let mut left_col = column![
        text("Tunnel state")
            .size(ts.label_m.size)
            .color(s.on_surface_variant),
        text(state).size(ts.headline_m.size).color(state_fg),
        text(profile_line).size(ts.body_m.size).color(state_fg),
        text(meta_line).size(ts.body_s.size).color(state_fg),
        vertical_space().height(Length::Fixed(8.0)),
        toggler(app.is_connected())
            .size(36.0)
            .label("connect")
            .on_toggle(Message::ConnectToggle)
            .style(t3::switch(s)),
    ]
    .spacing(6)
    .width(Length::Fixed(STATUS_FRAME_LEFT_WIDTH));
    if !picker_label.is_empty() {
        left_col = left_col.push(
            text(picker_label)
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
        );
    }
    left_col = left_col.push(picker);

    // --- bandwidth graph (D19 §1, D20 fix + legend) ------------------------
    let graph = RateGraph {
        samples: app.rate_history.iter().copied().collect(),
        style: RateGraphStyle {
            down_stroke: t3::col(s.primary),
            down_fill: translucent(t3::col(s.primary), 0.30),
            up_stroke: t3::col(s.tertiary),
            up_fill: translucent(t3::col(s.tertiary), 0.30),
            grid: translucent(t3::col(s.on_surface_variant), 0.35),
            label: t3::col(s.on_surface_variant),
        },
        font_size: ts.label_s.size,
        legend: true,
    }
    .canvas();

    let connected = app.is_connected();
    let graph_inner: Element<'_> = if connected {
        graph.into()
    } else {
        column![
            text("connect to see live bandwidth")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
            vertical_space().height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };
    // S13: the graph panel lives INSIDE the connection frame and fills
    // all remaining frame space. D22's stable-container rule holds: the
    // height stays FIXED and nothing that can grow (exit probe result,
    // links, adapter rows) lives inside the frame any more — they all
    // scroll in the page scrollable below it. The canvas Fill-scales
    // with the window WIDTH.
    let graph_panel = container(graph_inner)
        .padding(8)
        .style(t3::card(s, s.surface_container_lowest))
        .width(Length::Fill)
        .height(Length::Fixed(STATUS_GRAPH_HEIGHT));
    let _ = util::fmt_bps; // rate labels live in graph.rs; kept for chips/tests

    // --- merged exit & speed panel (S21 ask p; S22 two-column split) ---------
    // The two separate cards each carried a title + subtitle + button +
    // result rows with lots of air between them; ONE panel shares the
    // header and puts both triggers side by side. S22: the results are
    // split into TWO columns — ALL exit-ip outputs on the LEFT side, ALL
    // speed-test outputs on the RIGHT side (user wording, verbatim).
    // Pure view change inside the page scrollable (D24: nothing growable
    // in the frame).
    let has_active = app.profiles.iter().any(|p| p.active);
    // Both triggers live in ONE row (S21 merged panel). Explicit Element
    // annotations: the two branches build different widget types and the
    // row needs a single erased type.
    let exit_ctl: Element = if app.exit_probing {
        container(
            text("probing…")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
        )
        .width(Length::Fill)
        .into()
    } else {
        container(m3_button(
            "Check exit IP",
            has_active && !app.speed_testing,
            ButtonKind::Tonal,
            Message::ExitProbe,
            app,
        ))
        .width(Length::Fill)
        .into()
    };
    let speed_ctl: Element = if app.speed_testing {
        container(
            text("testing… (up to 30 s)")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
        )
        .width(Length::Fill)
        .into()
    } else {
        container(m3_button(
            "Run speed test",
            has_active && !app.exit_probing,
            ButtonKind::Tonal,
            Message::SpeedTest,
            app,
        ))
        .width(Length::Fill)
        .into()
    };
    let mut diag_col = column![
        text("Exit & speed")
            .size(ts.title_m.size)
            .color(s.on_surface),
        text(if has_active {
            "check which IP the internet sees and measure the download rate — the live \
             tunnel when connected, a temporary probe tunnel otherwise (setup is \
             excluded from the rate)"
        } else {
            "check which IP the internet sees and measure the download rate — pick an \
             active server first"
        })
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
    ]
    .spacing(6)
    .push(
        row![exit_ctl, speed_ctl]
            .spacing(12)
            .align_y(iced::Alignment::Center),
    );
    // S22 two-column split: ALL exit-ip outputs on the LEFT side, ALL
    // speed-test outputs on the RIGHT side (user wording, verbatim).
    // Each column: small heading, stacked result rows, quiet placeholder
    // while idle.
    let mut exit_out = column![text("Exit IP")
        .size(ts.label_m.size)
        .color(s.on_surface_variant),]
    .spacing(6);
    if let Some(res) = &app.exit {
        match res {
            Ok(info) => {
                let (verdict, vbg, vfg) = if info.direct_exit {
                    (
                        format!(
                            "single hop — the exit IS your server ({})",
                            info.server_endpoint
                        ),
                        s.tertiary_container,
                        s.on_tertiary_container,
                    )
                } else {
                    (
                        format!(
                            "extra egress hop — the exit differs from your server ({}); the server's own outbound (e.g. a VPN on the VM) is the exit",
                            info.server_endpoint
                        ),
                        s.error_container,
                        s.on_error_container,
                    )
                };
                exit_out = exit_out
                    .push(
                        row![
                            text(info.exit_ip.clone())
                                .size(ts.headline_s.size)
                                .color(s.on_surface),
                            horizontal_space().width(Length::Fill),
                            if info.via_temp_tunnel {
                                chip(
                                    "temporary probe tunnel",
                                    app,
                                    s.surface_container_high,
                                    s.on_surface_variant,
                                )
                            } else {
                                chip(
                                    "live tunnel",
                                    app,
                                    s.tertiary_container,
                                    s.on_tertiary_container,
                                )
                            },
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .push(
                        row![
                            chip(
                                format!("{} {}", info.country, info.city),
                                app,
                                s.secondary_container,
                                s.on_secondary_container
                            ),
                            chip(
                                info.isp.clone(),
                                app,
                                s.secondary_container,
                                s.on_secondary_container
                            ),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .push(
                        container(text(verdict).size(ts.body_s.size).color(vfg))
                            .padding(10)
                            .style(t3::card(s, vbg))
                            .width(Length::Fill),
                    );
            }
            Err(e) => {
                exit_out = exit_out.push(
                    text(format!("exit probe failed: {e}"))
                        .size(ts.body_s.size)
                        .color(s.error),
                );
            }
        }
    } else {
        exit_out = exit_out.push(
            text("no result yet — check the exit IP")
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
        );
    }
    let mut speed_out = column![text("Speed test")
        .size(ts.label_m.size)
        .color(s.on_surface_variant),]
    .spacing(6);
    if let Some(res) = &app.speed {
        match res {
            Ok(info) => {
                speed_out = speed_out
                    .push(
                        row![
                            text(format!("{:.1} Mbps", info.mbps))
                                .size(ts.headline_s.size)
                                .color(s.on_surface),
                            horizontal_space().width(Length::Fill),
                            if info.via_temp_tunnel {
                                chip(
                                    "temporary probe tunnel",
                                    app,
                                    s.surface_container_high,
                                    s.on_surface_variant,
                                )
                            } else {
                                chip(
                                    "live tunnel",
                                    app,
                                    s.tertiary_container,
                                    s.on_tertiary_container,
                                )
                            },
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .push(
                        row![
                            chip(
                                format!("{:.1} MB in {:.1} s", info.mbytes, info.seconds),
                                app,
                                s.secondary_container,
                                s.on_secondary_container
                            ),
                            chip(
                                util::short(&info.profile_tag, 24),
                                app,
                                s.surface_container_high,
                                s.on_surface_variant
                            ),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .push(
                        text(
                            "measures the DOWNLOAD path through this server; the engine's \
                             per-adapter graphs show the live spread while connected",
                        )
                        .size(ts.label_m.size)
                        .color(s.on_surface_variant),
                    );
            }
            Err(e) => {
                speed_out = speed_out.push(
                    text(format!("speed test failed: {e}"))
                        .size(ts.body_s.size)
                        .color(s.error),
                );
            }
        }
    } else {
        speed_out = speed_out.push(
            text("no result yet — run the speed test")
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
        );
    }
    diag_col = diag_col.push(
        row![exit_out.width(Length::Fill), speed_out.width(Length::Fill)].spacing(16),
    );
    let diag_card = container(diag_col.spacing(8).padding(16))
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fill);

    // --- links (scrollable) -------------------------------------------------
    let mut links_col = column![].spacing(8);
    match snap.as_ref().map(|x| x.links.as_slice()) {
        Some([]) => {
            links_col = links_col.push(
                text("no links — connect to populate")
                    .size(ts.body_m.size)
                    .color(s.on_surface_variant),
            );
        }
        Some(links) => {
            for l in links {
                let dot = if l.ok { "●" } else { "○" };
                let dot_color = if l.ok { s.tertiary } else { s.error };
                let latency = l
                    .latency_ms
                    .map_or_else(|| "— ms".to_string(), util::fmt_ms);
                let ewma = l
                    .ewma_ms
                    .map_or_else(|| "—".to_string(), |v| format!("{v:.0} ms"));
                let mut line = row![
                    text(dot.to_string()).size(ts.title_m.size).color(dot_color),
                    text(util::short(&l.name, 32))
                        .size(ts.body_m.size)
                        .color(s.on_surface),
                    horizontal_space().width(Length::Fill),
                    chip(
                        &latency,
                        app,
                        s.secondary_container,
                        s.on_secondary_container
                    ),
                    chip(
                        format!("ewma {ewma}"),
                        app,
                        s.surface_container_high,
                        s.on_surface_variant
                    ),
                    chip(
                        format!("up {}", util::fmt_bytes(l.bytes_up)),
                        app,
                        s.surface_container_high,
                        s.on_surface_variant
                    ),
                    chip(
                        format!("down {}", util::fmt_bytes(l.bytes_down)),
                        app,
                        s.surface_container_high,
                        s.on_surface_variant
                    ),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center);
                if l.quarantined {
                    line = line.push(chip(
                        "quarantined",
                        app,
                        s.error_container,
                        s.on_error_container,
                    ));
                }
                links_col = links_col.push(
                    container(line)
                        .padding(14)
                        .style(t3::card(s, s.surface_container_low))
                        .width(Length::Fill),
                );
            }
        }
        None => {
            links_col = links_col.push(
                text("loading status…")
                    .size(ts.body_m.size)
                    .color(s.on_surface_variant),
            );
        }
    }
    // NOTE: scrollable content must NOT fill the scroll axis (iced
    // layout rule) — the inner column shrinks to its content; the page
    // scrollable itself owns the Fill height.
    //
    // S13: the Links + Adapters card is content-sized now — it lives in
    // the PAGE scrollable together with the exit card, so the wheel
    // scrolls everything below the connection frame and no adapter is
    // stuck behind a short inner viewport.
    let mut bottom_col = column![].spacing(4).width(Length::Fill);
    bottom_col = bottom_col.push(
        text("Links")
            .size(ts.title_s.size)
            .color(s.on_surface_variant),
    );
    bottom_col = bottom_col.push(links_col);
    bottom_col = bottom_col.push(vertical_space().height(Length::Fixed(8.0)));
    bottom_col = bottom_col.push(
        text("Adapters")
            .size(ts.title_s.size)
            .color(s.on_surface_variant),
    );
    bottom_col = bottom_col.push(
        text("press an adapter's name for its live traffic graph")
            .size(ts.label_s.size)
            .color(s.on_surface_variant),
    );
    let mut adapters_col = column![].spacing(8);
    if app.adapters.is_empty() {
        adapters_col = adapters_col.push(
            text("no adapters found — check the Adapters tab")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
        );
    }
    for (i, r) in app.adapters.iter().enumerate() {
        adapters_col = adapters_col.push(status_adapter_row(app, i, r));
    }
    bottom_col = bottom_col.push(adapters_col);

    let bottom_card = container(bottom_col)
        .padding(12)
        .style(t3::card(s, s.surface_container_lowest))
        .width(Length::Fill);

    let _ = RATE_HISTORY_LEN; // (documented window; the App enforces it)

    // --- the connection frame: left column + graph side by side -------
    let frame = container(row![left_col, graph_panel].spacing(12))
        .style(t3::card(s, state_bg))
        .padding(20)
        .width(Length::Fill);

    // --- ONE page scrollable below the frame (S13) ---------------------
    let mut below = column![].spacing(12).width(Length::Fill);
    // S19 (review fix): a KILL SWITCH banner when the tunnel FAILED with
    // the switch armed — the state give_up() leaves the machine in.
    if crate::app::kill_switch_banner_visible(
        snap.as_ref()
            .map_or(ServiceState::Disconnected, |s| s.state),
        app.settings.kill_switch,
    ) {
        below = below.push(
            container(
                column![
                    text("KILL SWITCH ACTIVE — traffic is blocked")
                        .size(ts.title_s.size)
                        .color(s.on_error_container),
                    text(
                        "the tunnel died on its own, so system traffic is being blocked \n                         (fail-closed) instead of leaking outside the tunnel. Connect again \n                         to resume, or turn the kill switch off in Settings → Network.",
                    )
                    .size(ts.body_s.size)
                    .color(s.on_error_container),
                ]
                .spacing(4),
            )
            .padding(14)
            .style(t3::card(s, s.error_container))
            .width(Length::Fill),
        );
    }
    below = below.push(diag_card);
    below = below.push(bottom_card);
    let page_scroll = scrollable(container(below).width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill);

    let head = column![frame, page_scroll].spacing(12).height(Length::Fill);
    scaffold_fixed(app, "Status", head.into())
}

/// Height of the Status screen's global graph panel (D22: a stable,
/// fixed container — S13 moved it INSIDE the connection frame; its
/// height stays fixed so nothing can re-flow the canvas).
pub const STATUS_GRAPH_HEIGHT: f32 = 240.0;

/// Width of the Status connection frame's left column (state text,
/// connect toggle, server picker) — the global graph fills the rest.
pub const STATUS_FRAME_LEFT_WIDTH: f32 = 296.0;

/// One adapter row for the Status screen's bottom card (D22): the NAME is
/// the expander (the user's ask), with live rate chips; expanding reveals
/// that adapter's mini graph, fed by the same 1 s per-adapter sampler the
/// Adapters tab uses.
fn status_adapter_row<'a>(app: &'a App, i: usize, r: &'a crate::app::AdapterRow) -> Element<'a> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;
    let v = &r.view;

    // The pressable name: arrow + name in one text button (no icon font
    // in the zero-dep build — plain glyphs, same as the Adapters tab).
    let arrow = if r.expanded { "▾" } else { "▸" };
    let mut line = row![button(
        row![
            text(arrow.to_string())
                .size(ts.body_m.size)
                .color(s.primary),
            text(v.name.clone()).size(ts.body_m.size).color(s.primary),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .padding(Padding::new(4.0).left(8.0).right(8.0))
    .style(t3::text_button(s))
    .on_press(Message::AdapterExpanded(i)),]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    line = line.push(if v.up {
        chip("up", app, s.secondary_container, s.on_secondary_container)
    } else {
        chip("down", app, s.error_container, s.on_error_container)
    });
    // S11: the adapter's saved priority position (bonded adapters only).
    if let Some(p) = app.priority_position_of(&v.id) {
        line = line.push(chip(
            format!("#{p}"),
            app,
            s.primary_container,
            s.on_primary_container,
        ));
    }
    if !r.selected {
        line = line.push(chip(
            "not bonded",
            app,
            s.surface_container_high,
            s.on_surface_variant,
        ));
    }
    line = line.push(horizontal_space().width(Length::Fill));
    // Live per-adapter rates: the newest 1 s sample, same formatting as
    // the graph's own readout.
    let last = app
        .adapter_rates
        .get(&v.id)
        .and_then(|series| series.back());
    if let Some(&(u, d)) = last {
        line = line.push(chip(
            format!("↑ {}", crate::graph::format_rate(u as f64)),
            app,
            s.surface_container_high,
            s.on_surface_variant,
        ));
        line = line.push(chip(
            format!("↓ {}", crate::graph::format_rate(d as f64)),
            app,
            s.surface_container_high,
            s.on_surface_variant,
        ));
    }

    let mut card_col = column![line].spacing(8);
    if r.expanded {
        let samples: Vec<crate::graph::Sample> = app
            .adapter_rates
            .get(&v.id)
            .map(|series| series.iter().copied().collect())
            .unwrap_or_default();
        let carries_traffic = samples.iter().any(|(u, d)| *u > 0 || *d > 0);
        let mini = RateGraph {
            samples,
            style: RateGraphStyle {
                down_stroke: t3::col(s.primary),
                down_fill: translucent(t3::col(s.primary), 0.25),
                up_stroke: t3::col(s.tertiary),
                up_fill: translucent(t3::col(s.tertiary), 0.25),
                grid: translucent(t3::col(s.on_surface_variant), 0.30),
                label: t3::col(s.on_surface_variant),
            },
            font_size: ts.label_s.size,
            legend: false, // the Status graph owns the legend
        }
        .mini(110.0);
        let hint = if !app.is_connected() {
            "connect to see this adapter's tunnel traffic"
        } else if !r.selected {
            "not bonded — tick it on the Adapters tab to route tunnel traffic through it"
        } else if !carries_traffic {
            "bonded but idle — no tunnel bytes in the last 2 minutes"
        } else {
            "tunnel traffic through this adapter (↓ down / ↑ up, same colors \
             as the combined graph)"
        };
        card_col = card_col.push(
            container(mini)
                .padding(Padding::new(4.0).left(28.0).right(4.0).bottom(4.0))
                .style(t3::card(s, s.surface_container)),
        );
        card_col = card_col.push(text(hint).size(ts.label_s.size).color(s.on_surface_variant));
    }
    container(card_col)
        .padding(10)
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Servers
// ---------------------------------------------------------------------------

/// Servers screen: add-by-paste with parse preview + profile cards.
pub fn servers(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    // --- add by paste card -------------------------------------------------
    let paste_field = text_input(
        "paste a vless:// / vmess:// / ss:// / trojan:// link",
        &app.paste,
    )
    .on_input(Message::PasteChanged)
    .on_submit(Message::PasteAdd)
    .padding(10)
    .style(t3::text_field(s))
    .width(Length::Fill);

    let mut add_col = column![paste_field].spacing(10);
    match app.paste_preview.clone() {
        Some(Ok(p)) => {
            add_col = add_col.push(
                container(
                    column![
                        text(format!("{}  ·  {}", p.protocol, p.tag))
                            .size(ts.title_s.size)
                            .color(s.on_primary_container),
                        text(p.endpoint.clone())
                            .size(ts.body_m.size)
                            .color(s.on_primary_container),
                        text(p.security.clone())
                            .size(ts.body_s.size)
                            .color(s.on_primary_container),
                    ]
                    .spacing(4),
                )
                .padding(12)
                .style(t3::card(s, s.primary_container))
                .width(Length::Fill),
            );
        }
        Some(Err(e)) => {
            add_col = add_col.push(
                container(
                    text(format!("parse error: {e}"))
                        .size(ts.body_m.size)
                        .color(s.on_error_container),
                )
                .padding(12)
                .style(t3::card(s, s.error_container))
                .width(Length::Fill),
            );
        }
        None => {
            add_col = add_col.push(
                text("the parsed server appears here before it is added")
                    .size(ts.body_s.size)
                    .color(s.on_surface_variant),
            );
        }
    }
    add_col = add_col.push(
        row![
            m3_button(
                "Add server",
                !app.paste.trim().is_empty(),
                ButtonKind::Filled,
                Message::PasteAdd,
                app
            ),
            m3_button("Clear", true, ButtonKind::Text, Message::PasteClear, app),
        ]
        .spacing(8),
    );

    let mut col = column![container(add_col)
        .padding(16)
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fill)]
    .spacing(12);

    // --- notice -----------------------------------------------------------
    if let Some(n) = &app.notice {
        col = col.push(
            text(n.clone())
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
        );
    }

    // --- profile cards ----------------------------------------------------
    let mut list = column![].spacing(8);
    if app.profiles.is_empty() {
        list = list.push(
            text("no servers yet — paste a share link above")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
        );
    }
    for p in &app.profiles {
        list = list.push(profile_card(app, p));
    }
    col = col.push(
        container(list)
            .padding(16)
            .style(t3::card(s, s.surface_container_lowest))
            .width(Length::Fill),
    );

    // --- export stub (Service trait gap, wave 3) --------------------------
    col = col.push(
        row![
            m3_button("Export link", false, ButtonKind::Outlined, Message::PasteClear, app),
            text("disabled: MtService has no export_link (ProfileView carries no link material) — wave 3")
                .size(ts.label_m.size)
                .color(s.on_surface_variant),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
    );

    scaffold(app, "Servers", col.into())
}

fn profile_card<'a>(app: &'a App, p: &'a ProfileView) -> Element<'a> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    let latency_chip = match app.latency.get(&p.id) {
        Some(LatencyState::InFlight) => chip(
            "testing…",
            app,
            s.surface_container_high,
            s.on_surface_variant,
        ),
        Some(LatencyState::Done(Ok(ms))) => chip(
            util::fmt_ms(*ms),
            app,
            s.tertiary_container,
            s.on_tertiary_container,
        ),
        Some(LatencyState::Done(Err(e))) => chip(
            format!("test failed: {e}"),
            app,
            s.error_container,
            s.on_error_container,
        ),
        None => chip(
            "untested",
            app,
            s.surface_container_high,
            s.on_surface_variant,
        ),
    };

    let active_badge = if p.active {
        chip("active", app, s.primary_container, s.on_primary_container)
    } else {
        chip(
            p.protocol.clone(),
            app,
            s.surface_container_high,
            s.on_surface_variant,
        )
    };

    let row1 = row![
        text(p.tag.clone())
            .size(ts.title_s.size)
            .color(s.on_surface),
        horizontal_space().width(Length::Fill),
        active_badge,
        latency_chip,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let row2 = row![
        text(format!("{}  ·  {}", p.protocol, p.endpoint))
            .size(ts.body_s.size)
            .color(s.on_surface_variant),
        horizontal_space().width(Length::Fill),
        m3_button(
            "Use",
            !p.active,
            ButtonKind::Tonal,
            Message::SetActive(p.id.clone()),
            app
        ),
        m3_button(
            "Edit",
            true,
            ButtonKind::Text,
            Message::EditServer(p.id.clone()),
            app
        ),
        m3_button(
            "Test latency",
            true,
            ButtonKind::Text,
            Message::LatencyTest(p.id.clone()),
            app
        ),
        m3_button(
            "Delete",
            true,
            ButtonKind::ErrorText,
            Message::RemoveProfile(p.id.clone()),
            app
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    if !p.remark.is_empty() {
        return container(
            column![
                row1,
                text(p.remark.clone())
                    .size(ts.body_s.size)
                    .color(s.on_surface_variant),
                row2,
            ]
            .spacing(6),
        )
        .padding(14)
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fill)
        .into();
    }
    container(column![row1, row2].spacing(6))
        .padding(14)
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// S19: server Edit dialog (full-window modal)
// ---------------------------------------------------------------------------

/// One `[label | text field]` row of the edit form.
fn edit_field_row<'a>(
    app: &'a App,
    label: &'a str,
    value: &'a str,
    field: EditField,
    placeholder: &'a str,
    secret: bool,
) -> iced::widget::Row<'a, Message> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;
    let mut input = text_input(placeholder, value)
        .on_input(move |t| Message::EditField(field, t))
        .padding(6)
        .width(Length::Fixed(260.0))
        .style(t3::text_field(s));
    if secret {
        input = input.on_submit(Message::EditSave).size(ts.body_s.size); // credentials read a touch smaller
    } else {
        input = input.on_submit(Message::EditSave);
    }
    row![
        text(label)
            .size(app.theme.ts.body_m.size)
            .color(s.on_surface_variant)
            .width(Length::Fixed(150.0)),
        input,
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
}

/// S19: the server Edit dialog — a full-window modal layer (the S16
/// title-menu overlay discipline): a dim backdrop that cancels on click,
/// with a centered card carrying the editable fields. REALITY-only fields
/// render only for REALITY profiles (the service ignores them otherwise).
pub fn edit_overlay(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;
    let Some(d) = &app.edit else {
        // Not open (the caller gates on edit.is_some(); render nothing
        // clickable either way).
        return container(vertical_space().height(Length::Shrink))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    };

    let mut form = column![
        text(format!("Edit server — {} · {}", d.protocol, d.security))
            .size(ts.title_m.size)
            .color(s.on_surface),
        edit_field_row(app, "name", &d.tag, EditField::Tag, "server name", false),
        edit_field_row(
            app,
            "remark",
            &d.remark,
            EditField::Remark,
            "free-form note",
            false
        ),
        edit_field_row(
            app,
            "address",
            &d.address,
            EditField::Address,
            "host or IP",
            false
        ),
        edit_field_row(app, "port", &d.port, EditField::Port, "1-65535", false),
        edit_field_row(
            app,
            "identity (UUID)",
            &d.identity,
            EditField::Identity,
            "vless UUID",
            true
        ),
    ]
    .spacing(10);

    if d.is_reality() {
        form = form.push(
            text("REALITY security (vless+reality+tcp)")
                .size(ts.label_l.size)
                .color(s.on_surface_variant),
        );
        form = form.push(edit_field_row(
            app,
            "SNI",
            &d.sni,
            EditField::Sni,
            "server name indication",
            false,
        ));
        form = form.push(edit_field_row(
            app,
            "fingerprint",
            &d.fingerprint,
            EditField::Fingerprint,
            "uTLS fingerprint (chrome…)",
            false,
        ));
        form = form.push(edit_field_row(
            app,
            "public key",
            &d.public_key,
            EditField::PublicKey,
            "pbk",
            true,
        ));
        form = form.push(edit_field_row(
            app,
            "short ID",
            &d.short_id,
            EditField::ShortId,
            "sid",
            false,
        ));
        form = form.push(edit_field_row(
            app,
            "spiderX",
            &d.spider_x,
            EditField::SpiderX,
            "empty = /",
            false,
        ));
    } else {
        form = form.push(
            text("security fields apply to REALITY profiles only")
                .size(ts.label_m.size)
                .color(s.on_surface_variant),
        );
    }

    if let Some(err) = &d.error {
        form = form.push(
            container(
                text(format!("save failed: {err}"))
                    .size(ts.body_s.size)
                    .color(s.on_error_container),
            )
            .padding(10)
            .style(t3::card(s, s.error_container))
            .width(Length::Fill),
        );
    }

    form = form.push(
        row![
            m3_button(
                if d.saving { "saving…" } else { "Save" },
                !d.saving,
                ButtonKind::Filled,
                Message::EditSave,
                app
            ),
            m3_button("Cancel", true, ButtonKind::Text, Message::EditCancel, app),
            text("editing the ACTIVE server applies on the next reconnect")
                .size(ts.label_m.size)
                .color(s.on_surface_variant),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    // Scrollable so the form never overflows short windows.
    let card = container(
        scrollable(column![form].spacing(4).width(Length::Fill))
            .height(Length::Fill)
            .width(Length::Fill),
    )
    .padding(20)
    .style(t3::card(s, s.surface_container_low))
    .width(Length::Fixed(520.0))
    .height(Length::Fill)
    .max_height(560.0);

    // Dim backdrop: click cancels; the card centers.
    let backdrop = mouse_area(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .padding(24)
            .style(move |_t| iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba(
                    0.0, 0.0, 0.0, 0.45,
                ))),
                ..Default::default()
            }),
    )
    .on_press(Message::EditCancel)
    .interaction(iced::mouse::Interaction::Pointer);

    backdrop.into()
}

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

/// Routing screen: ordered rule table editor.
pub fn routing(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    let dirty = app.rules != app.rules_saved;
    let toolbar = row![
        m3_button(
            "Add rule",
            true,
            ButtonKind::Filled,
            Message::RuleAdded,
            app
        ),
        m3_button("Apply", dirty, ButtonKind::Tonal, Message::RulesApply, app),
        m3_button(
            "Revert",
            dirty,
            ButtonKind::Text,
            Message::RulesReverted,
            app
        ),
        horizontal_space().width(Length::Fill),
        match &app.rules_status {
            Some(t) => text(t.clone())
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
            None => text("first match wins, top to bottom")
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
        },
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let mut rows = column![toolbar].spacing(8);

    // --- S10: exclusions import (v2rayN parity) -----------------------------
    let import_row = row![
        text("import exclusions")
            .size(ts.body_m.size)
            .color(s.on_surface_variant)
            .width(Length::Fixed(150.0)),
        text_input(
            "file path — .txt domain list or v2rayN routing .json",
            &app.rules_import_path,
        )
        .on_input(Message::RulesImportPathChanged)
        .padding(8)
        .style(t3::text_field(s))
        .width(Length::Fill),
        m3_button(
            "Import",
            !app.rules_import_path.trim().is_empty(),
            ButtonKind::Tonal,
            Message::RulesImport,
            app
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    let import_hint = text(
        "one domain / IP / CIDR per line (# comments) — or a v2rayN routing-rules \
         JSON; entries join the DRAFT as direct rules, skipped lines land in the \
         log; press Apply to activate",
    )
    .size(ts.body_s.size)
    .color(s.on_surface_variant);
    let import_card = container(column![import_row, import_hint].spacing(6))
        .padding(12)
        .style(t3::card(s, s.surface_container))
        .width(Length::Fill);

    rows = rows.push(import_card);

    if app.rules.is_empty() {
        rows = rows.push(
            text("no rules — everything falls through to the default action")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
        );
    }
    for (i, r) in app.rules.iter().enumerate() {
        rows = rows.push(rule_row(app, i, r));
    }

    let card = container(rows)
        .padding(16)
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fill);

    scaffold(app, "Routing", column![card].width(Length::Fill).into())
}

fn rule_row<'a>(app: &'a App, i: usize, r: &'a RuleRow) -> Element<'a> {
    let s = app.theme.scheme;
    let kind = pick_list(RULE_KINDS, Some(r.kind.as_str()), move |k| {
        Message::RuleKind(i, k.to_string())
    })
    .width(Length::Fixed(170.0))
    .padding(6)
    .style(t3::pick_list_style(s))
    .menu_style(t3::menu_style(s));

    let value = text_input("match value", &r.value)
        .on_input(move |v| Message::RuleValue(i, v))
        .padding(8)
        .style(t3::text_field(s))
        .width(Length::Fill);

    let action = pick_list(RULE_ACTIONS, Some(r.action.as_str()), move |a| {
        Message::RuleAction(i, a.to_string())
    })
    .width(Length::Fixed(130.0))
    .padding(6)
    .style(t3::pick_list_style(s))
    .menu_style(t3::menu_style(s));

    container(
        row![
            text(format!("{}. ", i + 1))
                .size(app.theme.ts.label_l.size)
                .color(s.on_surface_variant),
            kind,
            value,
            action,
            m3_button(
                "Remove",
                true,
                ButtonKind::ErrorText,
                Message::RuleRemoved(i),
                app
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .into()
}

// ---------------------------------------------------------------------------
// Adapters (S7: bonded-adapter selection)
// ---------------------------------------------------------------------------

/// The Adapters screen: one checkbox row per network adapter (name,
/// description, IPv4 chips, up/down chip) + Apply / Revert / Refresh.
///
/// Semantics (mirrors the Routing editor): rows are a DRAFT; Apply pushes
/// the selected ids through `set_bonded_adapters` (the service persists and
/// restarts a live session); Revert drops the draft; Refresh re-enumerates
/// (USB NICs come and go). An adapter without IPv4 can be checked but is
/// skipped by the service at connect time (shown by its "no IPv4" chip).
pub fn adapters(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    let dirty = app.adapters_dirty();
    let selected = app.adapters.iter().filter(|r| r.selected).count();
    let priority_mode = app.settings.merge_policy == "priority";

    let toolbar = row![
        m3_button(
            "Apply",
            dirty,
            ButtonKind::Filled,
            Message::AdaptersApply,
            app
        ),
        m3_button(
            "Revert",
            dirty,
            ButtonKind::Text,
            Message::AdaptersReverted,
            app
        ),
        m3_button(
            "Refresh",
            true,
            ButtonKind::Tonal,
            Message::AdaptersRefresh,
            app
        ),
        horizontal_space().width(Length::Fill),
        match &app.adapters_status {
            Some(t) => text(t.clone())
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
            None => text(format!(
                "{selected} bonded — order below = adapter priority{}",
                if priority_mode {
                    " (priority policy: fills #1 until saturated, then #2…)"
                } else {
                    " (used by the 'priority' policy)"
                }
            ))
            .size(ts.body_s.size)
            .color(s.on_surface_variant),
        },
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let explainer = column![
        text("Bonded adapters")
            .size(ts.title_m.size)
            .color(s.on_surface),
        text(
            "Check the network adapters the tunnel should bond across — each \
             checked adapter gets its own engine bound to that NIC \
             (sendThrough). The ORDER of the checked rows is the ADAPTER \
             PRIORITY: with the \"priority\" merge policy (Settings) traffic \
             fills adapter #1 until its bandwidth is saturated, then the \
             overflow goes to #2, then #3 — reorder with ↑/↓. Other policies \
             ignore the order. Bonding mode \"stream\" (Settings → Advanced) \
             additionally needs mt-merge-server on the tunnel server plus an \
             Xray finalRules allow for the merge port (re-run \
             server/setup-xray-reality.sh on the VM). Expand a row (▸, or \
             press the adapter's name on the Status tab) for that adapter's \
             live traffic graph. No selection = a single link and Windows \
             picks the route.",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
    ]
    .spacing(6);

    let mut rows = column![toolbar, explainer].spacing(10);
    if app.adapters.is_empty() {
        rows = rows.push(
            text("no adapters found — plug one in or press Refresh")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
        );
    }
    // S11: position within the selected block (1-based) — the draft's
    // priority order, i.e. exactly what Apply will persist.
    let mut running_pos = 0usize;
    for (i, r) in app.adapters.iter().enumerate() {
        let v = &r.view;
        let arrow = if r.expanded { "▾" } else { "▸" };
        let pos = if r.selected {
            running_pos += 1;
            Some(running_pos)
        } else {
            None
        };
        let mut line = row![
            // D20: expand/collapse the per-interface graph (a plain text
            // button — no icon font in the zero-dep build).
            button(text(arrow.to_string()).size(ts.body_m.size))
                .padding(Padding::new(2.0).left(6.0).right(6.0))
                .style(t3::text_button(s))
                .on_press(Message::AdapterExpanded(i)),
            checkbox(v.name.clone(), r.selected)
                .size(ts.body_m.size)
                .spacing(8)
                .on_toggle(move |_| Message::AdapterToggled(i))
                .style(t3::checkbox(s)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // S11: priority position chip on every bonded row.
        if let Some(p) = pos {
            line = line.push(chip(
                format!("#{p}"),
                app,
                s.primary_container,
                s.on_primary_container,
            ));
        }

        if !v.description.is_empty() {
            line = line.push(
                text(util::short(&v.description, 40))
                    .size(ts.body_s.size)
                    .color(s.on_surface_variant),
            );
        }
        line = line.push(horizontal_space().width(Length::Fill));
        if v.ipv4.is_empty() {
            line = line.push(chip(
                "no IPv4",
                app,
                s.error_container,
                s.on_error_container,
            ));
        } else {
            for ip in &v.ipv4 {
                line = line.push(chip(
                    ip.clone(),
                    app,
                    s.surface_container_high,
                    s.on_surface_variant,
                ));
            }
        }
        line = line.push(if v.up {
            chip("up", app, s.secondary_container, s.on_secondary_container)
        } else {
            chip("down", app, s.error_container, s.on_error_container)
        });
        // S11: reorder buttons — swap with the previous/next SELECTED row
        // (disabled at the block edges; unselected rows carry no order).
        let can_up = pos.is_some_and(|p| p > 1);
        let can_down = pos.is_some() && app.adapters[i + 1..].iter().any(|r| r.selected);
        line = line.push(
            button(text("↑").size(ts.body_m.size))
                .padding(Padding::new(2.0).left(6.0).right(6.0))
                .style(t3::text_button(s))
                .on_press_maybe(can_up.then_some(Message::AdapterMoved(i, -1))),
        );
        line = line.push(
            button(text("↓").size(ts.body_m.size))
                .padding(Padding::new(2.0).left(6.0).right(6.0))
                .style(t3::text_button(s))
                .on_press_maybe(can_down.then_some(Message::AdapterMoved(i, 1))),
        );

        // D20: the adapter row + its expandable per-interface graph live in
        // ONE card so the graph reads as part of the row.
        let mut card_col = column![line].spacing(8);
        if r.expanded {
            let samples: Vec<crate::graph::Sample> = app
                .adapter_rates
                .get(&v.id)
                .map(|series| series.iter().copied().collect())
                .unwrap_or_default();
            let carries_traffic = samples.iter().any(|(u, d)| *u > 0 || *d > 0);
            let mini = RateGraph {
                samples,
                style: RateGraphStyle {
                    down_stroke: t3::col(s.primary),
                    down_fill: translucent(t3::col(s.primary), 0.25),
                    up_stroke: t3::col(s.tertiary),
                    up_fill: translucent(t3::col(s.tertiary), 0.25),
                    grid: translucent(t3::col(s.on_surface_variant), 0.30),
                    label: t3::col(s.on_surface_variant),
                },
                font_size: ts.label_s.size,
                legend: false, // the Status graph owns the legend
            }
            .mini(110.0);
            let hint = if !app.is_connected() {
                "connect to see this adapter's tunnel traffic".to_string()
            } else if !r.selected {
                "not bonded — tick the adapter to route tunnel traffic through it".to_string()
            } else {
                let prio = match app.priority_position_of(&v.id) {
                    Some(p) => format!("priority #{p}"),
                    None => "bonded".to_string(),
                };
                if !carries_traffic {
                    format!("{prio} — no tunnel bytes in the last 2 minutes")
                } else {
                    format!(
                        "{prio} — tunnel traffic through this adapter \
                         (↓ down / ↑ up, same colors as the Status graph)"
                    )
                }
            };
            card_col = card_col.push(
                container(mini)
                    .padding(Padding::new(4.0).left(28.0).right(4.0).bottom(4.0))
                    .style(t3::card(s, s.surface_container)),
            );
            card_col = card_col.push(text(hint).size(ts.label_s.size).color(s.on_surface_variant));
        }
        rows = rows.push(
            container(card_col)
                .padding(14)
                .style(t3::card(s, s.surface_container_low))
                .width(Length::Fill),
        );
    }

    let card = container(rows)
        .padding(16)
        .style(t3::card(s, s.surface_container_low))
        .width(Length::Fill);

    scaffold(app, "Adapters", column![card].width(Length::Fill).into())
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// S18: the accent wheel's selection marker — a canvas overlay stacked on
/// the wheel image, drawing a filled disc of the CURRENT seed color with
/// an on-surface ring (visible over any wheel hue). The canvas is sized
/// to the same square as the wheel, so [`theme::wheel_marker_position`]'s
/// local coordinates line up with the image underneath.
#[derive(Debug, Clone, Copy)]
pub struct WheelMarker {
    /// The seed whose wheel position the marker shows.
    pub seed: u32,
    /// Ring color (on_surface — theme-dependent).
    pub ring: iced::Color,
}

impl canvas::Program<Message> for WheelMarker {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height).max(1.0);
        let p = crate::theme::wheel_marker_position(self.seed, side);
        let c = mt_m3::M3Color::from_argb(self.seed);
        // Halo + disc + ring: readable over identical hues and at the rim.
        frame.fill(
            &canvas::Path::circle(p, 9.0),
            iced::Color::from(c).scale_alpha(0.35),
        );
        frame.fill(&canvas::Path::circle(p, 6.0), iced::Color::from(c));
        frame.stroke(
            &canvas::Path::circle(p, 6.0),
            canvas::Stroke::default()
                .with_color(self.ring)
                .with_width(2.0),
        );
        vec![frame.into_geometry()]
    }
}

/// Settings screen: accent color wheel + presets + seed, theme mode
/// (system/light/dark), system proxy, merge policy, autostart — all
/// mapped onto SettingsView with immediate save.
pub fn settings(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    // Appearance card ------------------------------------------------------
    //
    // S18: the accent picker is a color WHEEL (click or drag; the seed
    // updates live and persists on release) — the 8 presets and the
    // numeric seed field stay for exact values. Next to it, the
    // appearance MODE picks system / light / dark.
    let wheel_img = image(image::Handle::from_rgba(
        t3::COLOR_WHEEL_PX,
        t3::COLOR_WHEEL_PX,
        t3::COLOR_WHEEL_BYTES.to_vec(),
    ))
    .width(Length::Fixed(t3::COLOR_WHEEL_DRAW))
    .height(Length::Fixed(t3::COLOR_WHEEL_DRAW));
    let marker = Canvas::new(WheelMarker {
        seed: app.settings.theme_seed,
        ring: t3::col(s.on_surface),
    })
    .width(Length::Fixed(t3::COLOR_WHEEL_DRAW))
    .height(Length::Fixed(t3::COLOR_WHEEL_DRAW));
    let wheel = mouse_area(stack![wheel_img, marker])
        .on_press(Message::WheelPressed)
        .on_release(Message::WheelReleased)
        .on_move(Message::WheelMoved)
        .interaction(iced::mouse::Interaction::Crosshair);

    // The current accent: a filled chip + hex readout (also the rig's
    // pixel-assertion target — the chip shows the picked seed verbatim).
    let seed = app.settings.theme_seed;
    let seed_chip = container(vertical_space().height(Length::Fixed(28.0)))
        .width(Length::Fixed(48.0))
        .style(move |_t| iced::widget::container::Style {
            background: t3::bg(mt_m3::M3Color::from_argb(seed)),
            border: t3::border(8.0, s.on_surface_variant, 1.0),
            ..Default::default()
        });
    let hex = format!("#{seed:06X}", seed = seed & 0xFF_FFFF);

    let mut swatches = row![].spacing(8);
    for seed in t3::SEED_SWATCHES {
        let selected = seed == app.settings.theme_seed;
        let sw = button(
            container(vertical_space().height(Length::Fixed(28.0)))
                .width(Length::Fixed(40.0))
                .style(move |_t| iced::widget::container::Style {
                    background: t3::bg(mt_m3::M3Color::from_argb(seed)),
                    border: if selected {
                        t3::border(14.0, s.on_surface, 2.0)
                    } else {
                        t3::border_plain(14.0)
                    },
                    ..Default::default()
                }),
        )
        .style(t3::text_button(s))
        .padding(0)
        .on_press(Message::SeedSwatch(seed));
        swatches = swatches.push(sw);
    }

    let seed_field = text_input("decimal or 0xRRGGBB", &app.seed_text)
        .on_input(Message::SeedFieldChanged)
        .on_submit(Message::SeedCommitted)
        .padding(8)
        .width(Length::Fixed(180.0))
        .style(t3::text_field(s));

    // S18 appearance mode: segmented System / Light / Dark (selected =
    // filled). System shows what it currently resolves to.
    let mode = app.settings.theme_mode();
    let mode_button = |label: &str, m: ThemeMode, msg: Message| {
        m3_button(
            label,
            true,
            if mode == m {
                ButtonKind::Filled
            } else {
                ButtonKind::Outlined
            },
            msg,
            app,
        )
    };
    let mode_caption = match mode {
        ThemeMode::System => format!(
            "system — currently {}",
            if app.theme_dark() { "dark" } else { "light" }
        ),
        ThemeMode::Light => "light — always".to_string(),
        ThemeMode::Dark => "dark — always".to_string(),
    };
    let mode_row = row![
        mode_button(
            "System",
            ThemeMode::System,
            Message::ThemeModePicked("system".into())
        ),
        mode_button(
            "Light",
            ThemeMode::Light,
            Message::ThemeModePicked("light".into())
        ),
        mode_button(
            "Dark",
            ThemeMode::Dark,
            Message::ThemeModePicked("dark".into())
        ),
    ]
    .spacing(8);

    let appearance = column![
        text("Appearance").size(ts.title_m.size).color(s.on_surface),
        row![
            wheel,
            column![
                text("accent color")
                    .size(ts.label_l.size)
                    .color(s.on_surface_variant),
                row![
                    seed_chip,
                    text(hex).size(ts.body_m.size).color(s.on_surface)
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                swatches,
                row![
                    seed_field,
                    m3_button(
                        "Set seed",
                        true,
                        ButtonKind::Tonal,
                        Message::SeedCommitted,
                        app
                    ),
                ]
                .spacing(8),
            ]
            .spacing(10),
        ]
        .spacing(20)
        .align_y(iced::Alignment::Start),
        text("mode")
            .size(ts.label_l.size)
            .color(s.on_surface_variant),
        mode_row,
        text(mode_caption)
            .size(ts.label_m.size)
            .color(s.on_surface_variant),
    ]
    .spacing(10);

    // Network card ---------------------------------------------------------
    let network = column![
        text("Network").size(ts.title_m.size).color(s.on_surface),
        toggler(app.settings.system_proxy)
            .label("set system proxy while connected")
            .size(28.0)
            .on_toggle(Message::SystemProxyToggled)
            .style(t3::switch(s)),
        toggler(app.settings.kill_switch)
            .label("kill switch (block traffic if the tunnel dies)")
            .size(28.0)
            .on_toggle(Message::KillSwitchToggled)
            .style(t3::switch(s)),
        text(
            "on: when the tunnel dies on its own, the system proxy is pointed at a \
             dead local port — apps FAIL instead of silently leaking outside the \
             tunnel (fail-closed). Connect again to resume, or toggle this off to \
             restore direct traffic. A MANUAL disconnect always restores. Guards \
             system-proxy traffic; apps using the SOCKS/HTTP port already fail \
             closed while the tunnel is down. off (default): the proxy is \
             restored on a spontaneous death and traffic goes direct (fail-open).",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
        // S21 (feature o): the fair-share bandwidth scheduler.
        toggler(app.settings.qos_fair)
            .label("QoS fair share (stop one app hogging all bandwidth)")
            .size(28.0)
            .on_toggle(Message::QosToggled)
            .style(t3::switch(s)),
        text(
            "on: connections are grouped by the site they talk to; when two or \
             more groups race for a loaded tunnel, each gets an equal share — \
             a single download cannot starve everything else. One group alone \
             (or an idle tunnel) runs at full speed. Applies immediately, no \
             reconnect. off (default): every connection competes freely and \
             one busy site can take all of it. Note: grouping is per \
             destination — a download manager spreading over many servers \
             counts as several groups.",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
        row![
            text("merge policy")
                .size(ts.body_m.size)
                .color(s.on_surface_variant),
            pick_list(
                MERGE_POLICIES,
                Some(app.settings.merge_policy.as_str()),
                |p| { Message::MergePolicyPicked(p.to_string()) }
            )
            .width(Length::Fixed(200.0))
            .padding(6)
            .style(t3::pick_list_style(s))
            .menu_style(t3::menu_style(s)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
        text(
            "balanced (default): every new connection goes to the adapter \
             carrying the least traffic RIGHT NOW — a fat download stops \
             hogging one NIC and the spread evens out by BYTES. round-robin: \
             strict turn-taking per connection (even by connection count, \
             not by bytes). priority: adapters fill in the Adapters-tab \
             order — #1 takes everything until its bandwidth saturates, \
             then the overflow spills to #2, and so on. failover: everything \
             rides the first healthy adapter (spares stay idle). \
             lowest-latency: each connection picks the adapter with the \
             best smoothed (EWMA) RTT. In stream bonding mode (Advanced) \
             the policy also spreads every connection's BYTES across the \
             bonded adapters.",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
    ]
    .spacing(10);

    // Advanced card (S9: v2rayN + net-merge parity) -------------------------
    fn port_field<'a>(
        s: mt_m3::M3Scheme,
        ts: &mt_m3::M3TypeScale,
        label: &'a str,
        value: u16,
        msg: fn(String) -> Message,
    ) -> iced::widget::Row<'a, Message> {
        row![
            text(label)
                .size(ts.body_m.size)
                .color(s.on_surface_variant)
                .width(Length::Fixed(210.0)),
            text_input("port", &value.to_string())
                .on_input(msg)
                .padding(6)
                .width(Length::Fixed(110.0))
                .style(crate::theme::text_field(s)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center)
    }
    let bond_mode_row = row![
        text("bonding mode")
            .size(ts.body_m.size)
            .color(s.on_surface_variant)
            .width(Length::Fixed(210.0)),
        pick_list(BOND_MODES, Some(app.settings.bond_mode.as_str()), |m| {
            Message::BondModePicked(m.to_string())
        },)
        .width(Length::Fixed(190.0))
        .padding(6)
        .style(t3::pick_list_style(s))
        .menu_style(t3::menu_style(s)),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);
    let log_level_row = row![
        text("engine log level")
            .size(ts.body_m.size)
            .color(s.on_surface_variant)
            .width(Length::Fixed(210.0)),
        pick_list(LOG_LEVELS, Some(app.settings.log_level.as_str()), |l| {
            Message::LogLevelPicked(l.to_string())
        },)
        .width(Length::Fixed(190.0))
        .padding(6)
        .style(t3::pick_list_style(s))
        .menu_style(t3::menu_style(s)),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);
    let advanced = column![
        text("Advanced").size(ts.title_m.size).color(s.on_surface),
        text("mux stays OFF by design — it would collapse bonding into one stream per link")
            .size(ts.body_s.size)
            .color(s.on_surface_variant),
        port_field(s, &ts, "SOCKS port (0 = ephemeral)", app.settings.socks_port, Message::SocksPortChanged),
        port_field(s, &ts, "HTTP port (0 = ephemeral)", app.settings.http_port, Message::HttpPortChanged),
        port_field(s, &ts, "server merge port (stream mode)", app.settings.merge_port, Message::MergePortChanged),
        bond_mode_row,
        text(
            "per-connection (default): each TCP connection rides ONE adapter, \
             connections are spread by the merge policy. stream: the BYTES of \
             every connection are split across ALL ticked adapters at once and \
             re-merged by mt-merge-server on the tunnel server — one download \
             uses every NIC simultaneously. In stream mode the merge policy \
             drives the frame scheduler too: balanced spreads frames by live \
             adapter backpressure (fast NICs take more), priority fills the \
             Adapters-tab order. Needs the merge daemon (setup script in the \
             release zip, port below) plus an Xray finalRules allow for that \
             port on the server (re-run server/setup-xray-reality.sh); falls \
             back to per-connection with a log line when it is unreachable.",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
        log_level_row,
        toggler(app.settings.allow_lan)
            .label("allow LAN clients (bind 0.0.0.0 — exposes the proxy)")
            .size(28.0)
            .on_toggle(Message::AllowLanToggled)
            .style(t3::switch(s)),
        row![
            text("latency test target")
                .size(ts.body_m.size)
                .color(s.on_surface_variant)
                .width(Length::Fixed(210.0)),
            text_input("host:port", &app.settings.probe_target)
                .on_input(Message::ProbeTargetChanged)
                .padding(6)
                .width(Length::Fixed(240.0))
                .style(t3::text_field(s)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
        row![
            text("exit probe URL")
                .size(ts.body_m.size)
                .color(s.on_surface_variant)
                .width(Length::Fixed(210.0)),
            text_input("http://host/path", &app.settings.exit_probe_url)
                .on_input(Message::ExitUrlChanged)
                .padding(6)
                .width(Length::Fixed(240.0))
                .style(t3::text_field(s)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
        row![
            text("speed test URL")
                .size(ts.body_m.size)
                .color(s.on_surface_variant)
                .width(Length::Fixed(210.0)),
            text_input("http://host/path", &app.settings.speed_test_url)
                .on_input(Message::SpeedUrlChanged)
                .padding(6)
                .width(Length::Fixed(240.0))
                .style(t3::text_field(s)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
        text("stream mode splits every connection's bytes across the bonded adapters and merges them on the server (needs the mt-merge-server daemon and an Xray finalRules allow for the merge port on the server — re-run server/setup-xray-reality.sh; falls back to per-connection when unreachable)")
            .size(ts.body_s.size)
            .color(s.on_surface_variant),
    ]
    .spacing(10);

    // Startup card ---------------------------------------------------------
    let startup = column![
        text("Startup").size(ts.title_m.size).color(s.on_surface),
        toggler(app.settings.autostart)
            .label("start with the system")
            .size(28.0)
            .on_toggle(Message::AutostartToggled)
            .style(t3::switch(s)),
    ]
    .spacing(10);

    // Window & tray card (S15, D25; S16 adds always-on-top) -----------------
    let window_tray = column![
        text("Window & tray")
            .size(ts.title_m.size)
            .color(s.on_surface),
        toggler(app.settings.close_to_tray)
            .label("keep running in the tray when closed")
            .size(28.0)
            .on_toggle(Message::CloseToTrayToggled)
            .style(t3::switch(s)),
        text(
            "on: the ✕ button hides the window into the tray icon — the \
             tunnel keeps running; left-click the icon to reopen, \
             right-click it for Quick Settings (connect, servers, exit). \
             off (default): closing really closes. Windows only — on other \
             platforms the window minimizes instead, so it stays reachable.",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
        toggler(app.settings.always_on_top)
            .label("keep the window always on top")
            .size(28.0)
            .on_toggle(Message::AlwaysOnTopToggled)
            .style(t3::switch(s)),
        text(
            "on: the window floats above all other windows. Also \
             switchable from the title bar's right-click menu. off \
             (default): normal window stacking.",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
    ]
    .spacing(10);

    // Diagnostics card (S19) ------------------------------------------------
    let diag_caption = match &app.diag_path {
        Some(p) => format!("currently writing: {p}"),
        None => "the file is created when the mode turns on (its path also lands \
                 in the Logs tab)"
            .to_string(),
    };
    let diagnostics_card = column![
        text("Diagnostics")
            .size(ts.title_m.size)
            .color(s.on_surface),
        toggler(app.settings.diagnostics)
            .label("diagnostics mode (verbose support log)")
            .size(28.0)
            .on_toggle(Message::DiagnosticsToggled)
            .style(t3::switch(s)),
        text(
            "on: one timestamped file per session in the log folder collects \
             connection lifecycle events, engine output (log level forced to \
             debug), link health and a redacted settings/profile summary — \
             paste it into a chat for connection-issue analysis. off: the file \
             stops growing but stays on disk.",
        )
        .size(ts.body_s.size)
        .color(s.on_surface_variant),
        text(diag_caption)
            .size(ts.body_s.size)
            .color(s.on_surface_variant),
        m3_button(
            "Open log folder",
            true,
            ButtonKind::Outlined,
            Message::OpenLogsFolder,
            app
        ),
    ]
    .spacing(10);

    let mut col = column![
        container(appearance)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
        container(network)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
        container(advanced)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
        container(startup)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
        container(window_tray)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
        container(diagnostics_card)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
    ]
    .spacing(12);

    if let Some(t) = &app.settings_status {
        col = col.push(
            text(t.clone())
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
        );
    }

    scaffold(app, "Settings", col.into())
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

/// Logs screen (S17, D27): fixed toolbar + a dedicated viewport that owns
/// the page's scrolling, with floating top/bottom jump buttons and an
/// autoscroll-to-bottom toggle. The toolbar (level filter, autoscroll,
/// Clear) stays put — only the log lines scroll.
///
/// The scrollable's id is shared with `App::update` (programmatic jumps +
/// autoscroll re-anchoring on every fresh line while the toggle is on).
pub fn logs_scroll_id() -> scrollable::Id {
    scrollable::Id::new("logs-lines")
}

pub fn logs(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    let filter = pick_list(LogFilter::LABELS, Some(app.log_filter.label()), |l| {
        Message::LogFilterPicked(l.to_string())
    })
    .width(Length::Fixed(150.0))
    .padding(6)
    .style(t3::pick_list_style(s))
    .menu_style(t3::menu_style(s));

    // S17: the autoscroll toggle doubles as its own state indicator —
    // Tonal (filled) while following the tail, quiet Text while off.
    let (auto_label, auto_kind) = if app.logs_autoscroll {
        ("Auto-scroll: on", ButtonKind::Tonal)
    } else {
        ("Auto-scroll: off", ButtonKind::Text)
    };

    let toolbar = row![
        text("level:")
            .size(ts.body_m.size)
            .color(s.on_surface_variant),
        filter,
        horizontal_space().width(Length::Fill),
        m3_button(
            auto_label,
            true,
            auto_kind,
            Message::LogsAutoscrollToggled,
            app,
        ),
        m3_button("Clear", true, ButtonKind::Text, Message::LogsCleared, app),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let mut lines = column![].spacing(2);
    for line in app.logs.iter().filter(|l| app.log_filter.matches(l)) {
        let color = if line.starts_with("ERROR") {
            s.error
        } else if line.starts_with("WARN") {
            s.tertiary
        } else {
            s.on_surface_variant
        };
        lines = lines.push(
            text(line.clone())
                .size(ts.body_s.size)
                .color(color)
                .width(Length::Fill),
        );
    }
    if app.logs.is_empty() {
        lines = lines.push(
            text("waiting for events…")
                .size(ts.body_s.size)
                .color(s.on_surface_variant),
        );
    }

    // The viewport: the ONLY scrollable on this screen (the page itself is
    // fixed via scaffold_fixed), so wheel/keys always move the log lines.
    let viewport = scrollable(
        container(lines)
            .width(Length::Fill)
            // Right padding keeps the newest line clear of the floating
            // jump buttons anchored over the same corner.
            .padding(Padding::new(2.0).right(8.0)),
    )
    .id(logs_scroll_id())
    .width(Length::Fill)
    .height(Length::Fill);

    // S17: floating navigation — top/bottom jumps overlaid on the
    // viewport's bottom-right corner. The anchor is a PLAIN container
    // (non-interactive): wheel events and clicks between the buttons pass
    // straight through to the scrollable beneath; only the two buttons
    // capture. Same ↑/↓ glyphs as the adapter reorder controls.
    let jump = |glyph: &'static str, msg: Message| {
        button(text(glyph).size(ts.body_m.size))
            .style(t3::tonal_button(s))
            .padding(Padding::new(4.0).left(12.0).right(12.0))
            .on_press(msg)
    };
    let floating = container(
        column![
            jump("↑", Message::LogsScrollTop),
            jump("↓", Message::LogsScrollBottom),
        ]
        .spacing(6),
    )
    .padding(Padding::new(6.0))
    .style(t3::card(s, s.surface_container_high));
    // RIG FINDING (S17): anchored flush to the corner, the card sat inside
    // the scrollable's scrollbar hit zone (iced grabs a margin around the
    // rail) — the rail swallowed the button clicks. Stand off the right
    // edge by more than the rail+grab width so the buttons own their
    // pixels; keep the bottom inset modest (the corner reads best).
    let overlay = container(floating)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::End)
        .align_y(iced::Alignment::End)
        .padding(Padding::new(0.0).right(26.0).bottom(4.0));

    let viewport = stack![viewport].push(overlay);

    let card = container(
        column![
            toolbar,
            vertical_space().height(Length::Fixed(6.0)),
            viewport
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .padding(16)
    .style(t3::card(s, s.surface_container_lowest))
    .width(Length::Fill)
    .height(Length::Fill);

    scaffold_fixed(app, "Logs", card.into())
}
