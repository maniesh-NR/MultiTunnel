//! Screen views: Status / Servers / Routing / Settings / Logs + nav rail.
//!
//! Functionality-level fidelity per the wave-2 contract (not pixel-perfect):
//! M3 rail with active pill, cards on tonal surfaces, filled/tonal/outlined/
//! text buttons, chips, switches, text fields, pick lists.

use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, text, text_input,
    toggler, vertical_space,
};
use iced::{Length, Padding};
use mt_service::{ProfileView, ServiceState};

use crate::app::{
    App, LatencyState, LogFilter, Message, RuleRow, Screen, MERGE_POLICIES, RULE_ACTIONS,
    RULE_KINDS,
};
use crate::theme as t3;
use crate::util;
use crate::Element;

// ---------------------------------------------------------------------------
// Shared scaffolding
// ---------------------------------------------------------------------------

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

/// Status screen: big state card + connect switch + links + throughput.
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

    let state_card = column![
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
    .padding(20)
    .width(Length::Fill);

    let mut head = column![container(state_card)
        .style(t3::card(s, state_bg))
        .width(Length::Fill)]
    .spacing(12);

    if let Some((up, down)) = app.throughput {
        head = head.push(
            row![
                chip(
                    format!("up {}", util::fmt_bps(up)),
                    app,
                    s.tertiary_container,
                    s.on_tertiary_container
                ),
                chip(
                    format!("down {}", util::fmt_bps(down)),
                    app,
                    s.tertiary_container,
                    s.on_tertiary_container
                ),
            ]
            .spacing(8),
        );
    }

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

    head = head.push(
        container(links_col)
            .style(t3::card(s, s.surface_container_lowest))
            .width(Length::Fill)
            .padding(12),
    );
    scaffold(app, "Status", head.into())
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
// Settings
// ---------------------------------------------------------------------------

/// Settings screen: theme seed picker + dark, system proxy, merge policy,
/// autostart — all mapped onto SettingsView with immediate save.
pub fn settings(app: &App) -> Element<'_> {
    let s = app.theme.scheme;
    let ts = app.theme.ts;

    // Appearance card ------------------------------------------------------
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

    let appearance = column![
        text("Appearance").size(ts.title_m.size).color(s.on_surface),
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
        toggler(app.settings.dark_mode)
            .label("dark mode")
            .size(28.0)
            .on_toggle(Message::DarkToggled)
            .style(t3::switch(s)),
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

    let mut col = column![
        container(appearance)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
        container(network)
            .padding(16)
            .style(t3::card(s, s.surface_container_low))
            .width(Length::Fill),
        container(startup)
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

/// Logs screen: streaming buffer fed by ServiceEvent::LogLine + filter.
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

    let toolbar = row![
        text("level:")
            .size(ts.body_m.size)
            .color(s.on_surface_variant),
        filter,
        horizontal_space().width(Length::Fill),
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

    let card = container(
        column![toolbar, vertical_space().height(Length::Fixed(6.0)), lines].width(Length::Fill),
    )
    .padding(16)
    .style(t3::card(s, s.surface_container_lowest))
    .width(Length::Fill);

    scaffold(app, "Logs", card.into())
}
