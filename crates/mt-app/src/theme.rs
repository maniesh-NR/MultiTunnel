//! M3-derived theme snapshot + widget style helpers for mt-app.
//!
//! Every style closure in the GUI captures an [`M3Scheme`] (Copy) and
//! resolves its colors through these helpers, so a seed change re-themes the
//! whole app by swapping one field in [`App`](crate::app::App).
//!
//! Approximations (documented):
//! * Disabled controls: M3 specifies "content at 38% opacity on a 12%
//!   container"; we approximate with [`mt_m3::state_layer`] blending
//!   `on_surface` over `surface` (10% pressed opacity) — close tonally and
//!   keeps the helper set pure.
//! * 1 M3 sp maps to 1 iced point; no font scaling machinery.

use mt_m3::{state_layer, M3Color, M3Palette, M3Scheme, M3Shape, M3TypeScale, StateLayer};
use mt_service::SettingsView;

use iced::widget::{button, container, text_input, toggler};
use iced::{Background, Border, Color, Shadow};

/// M3 shape scale (const-evaluated from mt-m3).
const SHAPE: M3Shape = mt_m3::shape();

/// The GUI's theme snapshot: seed palette + resolved scheme + type scale.
#[derive(Debug, Clone)]
pub struct MtTheme {
    /// Seed-derived palette (both light and dark schemes pre-resolved).
    pub palette: M3Palette,
    /// The scheme currently applied (`dark` decides light vs dark).
    pub scheme: M3Scheme,
    /// M3 type scale (constant).
    pub ts: M3TypeScale,
    /// M3 shape scale (constant).
    pub shape: M3Shape,
    /// Current dark-mode flag.
    pub dark: bool,
}

/// Seed swatches offered on the Settings screen (Material baseline hues).
pub const SEED_SWATCHES: [u32; 8] = [
    0x67_50_A4, // M3 baseline purple
    0x42_85_F4, // blue
    0x00_79_6B, // teal
    0x34_A8_53, // green
    0xF9_AB_00, // amber
    0xEA_43_35, // red
    0xFF_69_00, // orange
    0x00_00_00, // black
];

impl MtTheme {
    /// Derives a theme from raw settings values.
    pub fn derive(seed: u32, dark: bool) -> Self {
        let palette = M3Palette::from_seed(seed);
        Self {
            scheme: palette.scheme(dark),
            palette,
            ts: mt_m3::type_scale(),
            shape: SHAPE,
            dark,
        }
    }

    /// Derives from the settings view (the usual entry point).
    pub fn from_settings(s: &SettingsView) -> Self {
        Self::derive(s.theme_seed, s.dark_mode)
    }

    /// Live re-theme in place (seed and/or dark mode changed).
    pub fn rederive(&mut self, seed: u32, dark: bool) {
        *self = Self::derive(seed, dark);
    }

    /// Maps the palette onto an iced theme (used by the window-level theme).
    pub fn iced_theme(&self) -> iced::Theme {
        self.palette.iced_theme(self.dark)
    }
}

// ---------------------------------------------------------------------------
// Color plumbing
// ---------------------------------------------------------------------------

/// `M3Color` as a background.
pub fn bg(c: M3Color) -> Option<Background> {
    Some(Color::from(c).into())
}

/// `M3Color` as an iced color.
pub fn col(c: M3Color) -> Color {
    c.into()
}

/// Border with an M3 radius and an outline color.
pub fn border(radius: f32, color: M3Color, width: f32) -> Border {
    Border {
        color: col(color),
        width,
        radius: radius.into(),
    }
}

/// Border with a radius and no visible line.
pub fn border_plain(radius: f32) -> Border {
    Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: radius.into(),
    }
}

/// Flat shadow (M3 cards here use tonal surfaces, not elevation shadows).
const FLAT: Shadow = Shadow {
    offset: iced::Vector::ZERO,
    blur_radius: 0.0,
    color: Color::TRANSPARENT,
};

// ---------------------------------------------------------------------------
// Cards / surfaces
// ---------------------------------------------------------------------------

fn surface_style(surface: M3Color, radius: f32) -> container::Style {
    container::Style {
        background: bg(surface),
        border: border_plain(radius),
        shadow: FLAT,
        text_color: None,
    }
}

/// A card on a tonal surface (rounded, no border).
pub fn card(_s: M3Scheme, surface: M3Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_t| surface_style(surface, SHAPE.large)
}

/// A fully-rounded pill container (navigation rail / badges).
pub fn pill(_s: M3Scheme, surface: M3Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_t| surface_style(surface, SHAPE.extra_large)
}

/// Chip container (small radius, tonal background).
pub fn chip(_s: M3Scheme, surface: M3Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_t| surface_style(surface, SHAPE.small)
}

/// The page background (M3 `background` token).
pub fn page(s: M3Scheme) -> impl Fn(&iced::Theme) -> container::Style {
    move |_t| surface_style(s.background, 0.0)
}

// ---------------------------------------------------------------------------
// Buttons (filled / tonal / outlined / text) — M3 states via state layers
// ---------------------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn button_style(
    base: M3Color,
    on: M3Color,
    outline: Option<M3Color>,
    disabled: (M3Color, M3Color),
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_t, status| {
        let (bg_c, fg_c) = match status {
            button::Status::Active => (base, on),
            button::Status::Hovered => (state_layer(base, on, StateLayer::Hover), on),
            button::Status::Pressed => (state_layer(base, on, StateLayer::Pressed), on),
            button::Status::Disabled => disabled,
        };
        button::Style {
            background: bg(bg_c),
            text_color: col(fg_c),
            border: match outline {
                Some(o) => border(SHAPE.large, o, 1.0),
                None => border_plain(SHAPE.large),
            },
            shadow: FLAT,
        }
    }
}

/// Disabled-container approximation (on_surface blended over surface).
fn disabled_container(s: M3Scheme) -> M3Color {
    state_layer(s.surface, s.on_surface, StateLayer::Pressed)
}

/// M3 filled button (highest emphasis).
pub fn filled_button(s: M3Scheme) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(
        s.primary,
        s.on_primary,
        None,
        (disabled_container(s), s.on_surface_variant),
    )
}

/// M3 tonal button (medium emphasis, secondary container).
pub fn tonal_button(s: M3Scheme) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(
        s.secondary_container,
        s.on_secondary_container,
        None,
        (disabled_container(s), s.on_surface_variant),
    )
}

/// M3 outlined button (low emphasis).
pub fn outlined_button(s: M3Scheme) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let outline = state_layer(s.outline, s.on_surface, StateLayer::Pressed);
    button_style(
        s.surface,
        s.primary,
        Some(outline),
        (s.surface, s.on_surface_variant),
    )
}

/// M3 text button (lowest emphasis).
pub fn text_button(s: M3Scheme) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(
        s.surface,
        s.primary,
        None,
        (disabled_container(s), s.on_surface_variant),
    )
}

/// Error-tinted text button (destructive actions).
pub fn error_text_button(s: M3Scheme) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(
        s.surface,
        s.error,
        None,
        (disabled_container(s), s.on_surface_variant),
    )
}

/// Navigation-rail pill button: filled when active, transparent otherwise.
pub fn rail_button(
    active: bool,
    s: M3Scheme,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let hover = state_layer(s.surface, s.on_surface_variant, StateLayer::Hover);
    let pressed = state_layer(s.surface, s.on_surface_variant, StateLayer::Pressed);
    move |_t, status| match (active, status) {
        (true, _) => button::Style {
            background: bg(s.secondary_container),
            text_color: col(s.on_secondary_container),
            border: border_plain(SHAPE.extra_large),
            shadow: FLAT,
        },
        (false, button::Status::Hovered) => button::Style {
            background: bg(hover),
            text_color: col(s.on_surface_variant),
            border: border_plain(SHAPE.extra_large),
            shadow: FLAT,
        },
        (false, button::Status::Pressed) => button::Style {
            background: bg(pressed),
            text_color: col(s.on_surface_variant),
            border: border_plain(SHAPE.extra_large),
            shadow: FLAT,
        },
        (false, _) => button::Style {
            background: None,
            text_color: col(s.on_surface_variant),
            border: border_plain(SHAPE.extra_large),
            shadow: FLAT,
        },
    }
}

// ---------------------------------------------------------------------------
// Text inputs
// ---------------------------------------------------------------------------

/// M3 text field: filled style on `surface_container_highest`.
pub fn text_field(s: M3Scheme) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    move |_t, status| {
        let (fill, border) = match status {
            text_input::Status::Focused => (
                s.surface_container_highest,
                border(SHAPE.extra_small, s.primary, 2.0),
            ),
            text_input::Status::Disabled => {
                (disabled_container(s), border_plain(SHAPE.extra_small))
            }
            _ => (s.surface_container_highest, border_plain(SHAPE.extra_small)),
        };
        text_input::Style {
            background: bg(fill).unwrap_or(Background::Color(Color::BLACK)),
            border,
            icon: col(s.on_surface_variant),
            placeholder: col(s.on_surface_variant),
            value: col(s.on_surface),
            selection: col(s.inverse_primary),
        }
    }
}

// ---------------------------------------------------------------------------
// Pick list + menu (M3 surfaces)
// ---------------------------------------------------------------------------

/// M3 pick-list surface.
pub fn pick_list_style(
    s: M3Scheme,
) -> impl Fn(&iced::Theme, iced::widget::pick_list::Status) -> iced::widget::pick_list::Style {
    move |_t, status| {
        let base = match status {
            iced::widget::pick_list::Status::Hovered => {
                state_layer(s.surface_container, s.on_surface_variant, StateLayer::Hover)
            }
            _ => s.surface_container,
        };
        iced::widget::pick_list::Style {
            text_color: col(s.on_surface_variant),
            placeholder_color: col(s.on_surface_variant),
            handle_color: col(s.on_surface_variant),
            background: col(base).into(),
            border: border_plain(SHAPE.small),
        }
    }
}

/// M3 dropdown menu surface.
pub fn menu_style(s: M3Scheme) -> impl Fn(&iced::Theme) -> iced::widget::overlay::menu::Style {
    move |_t| iced::widget::overlay::menu::Style {
        background: col(s.surface_container).into(),
        border: border_plain(SHAPE.small),
        text_color: col(s.on_surface),
        selected_text_color: col(s.on_secondary_container),
        selected_background: col(s.secondary_container).into(),
    }
}

// ---------------------------------------------------------------------------
// Switch (iced toggler, styled as an M3 switch)
// ---------------------------------------------------------------------------

/// M3 switch styling for the iced toggler widget.
pub fn switch(s: M3Scheme) -> impl Fn(&iced::Theme, toggler::Status) -> toggler::Style {
    move |_t, status| {
        let (track, thumb) = match status {
            toggler::Status::Active { is_toggled: true }
            | toggler::Status::Hovered { is_toggled: true } => (s.primary, s.on_primary),
            toggler::Status::Active { is_toggled: false }
            | toggler::Status::Hovered { is_toggled: false } => {
                (s.surface_container_highest, s.outline)
            }
            toggler::Status::Disabled => (disabled_container(s), s.on_surface_variant),
        };
        toggler::Style {
            background: col(track),
            background_border_width: 2.0,
            background_border_color: col(track),
            foreground: col(thumb),
            foreground_border_width: 0.0,
            foreground_border_color: col(thumb),
        }
    }
}

/// M3 checkbox (Adapters screen): unchecked = 2 dp `on_surface_variant`
/// outline box on a transparent background; checked = `primary` box with
/// an `on_primary` check icon (M3 spec shape corner ~2 dp).
pub fn checkbox(
    s: M3Scheme,
) -> impl Fn(&iced::Theme, iced::widget::checkbox::Status) -> iced::widget::checkbox::Style {
    move |_t, status| {
        let is_checked = matches!(
            status,
            iced::widget::checkbox::Status::Active { is_checked: true }
                | iced::widget::checkbox::Status::Hovered { is_checked: true }
        );
        let (bg, icon, border_color) = if is_checked {
            (s.primary, s.on_primary, s.primary)
        } else {
            (s.surface, s.on_surface, s.on_surface_variant)
        };
        iced::widget::checkbox::Style {
            background: Background::Color(col(bg)),
            icon_color: col(icon),
            border: border(4.0, border_color, if is_checked { 0.0 } else { 2.0 }),
            text_color: Some(col(s.on_surface)),
        }
    }
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Plain text in the given M3 color (thin wrapper over `text().color`).
pub fn tinted_text<'a, F>(s: &'a str, c: F) -> iced::widget::Text<'a, iced::Theme, iced::Renderer>
where
    F: Into<iced::Color> + Copy,
{
    iced::widget::text(s).color(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::text;

    /// Two different seeds derive visibly different schemes and the palette
    /// remembers the seed; dark/light differ.
    #[test]
    fn theme_derivation_seed_changes_scheme() {
        let a = MtTheme::derive(0x6750A4, false);
        let b = MtTheme::derive(0x4285F4, false);
        assert_ne!(a.scheme.primary, b.scheme.primary);
        assert_eq!(a.palette.seed(), 0x6750A4);
        assert_eq!(b.palette.seed(), 0x4285F4);

        let light = MtTheme::derive(0x6750A4, false);
        let dark = MtTheme::derive(0x6750A4, true);
        assert_ne!(light.scheme.background, dark.scheme.background);
    }

    /// `from_settings` / `rederive` honor the settings view (live re-theme).
    #[test]
    fn theme_from_settings_and_rederive() {
        let mut t = MtTheme::from_settings(&SettingsView {
            theme_seed: 0x4285F4,
            dark_mode: false,
            system_proxy: true,
            merge_policy: "failover".into(),
            autostart: false,
            bonded_adapters: Vec::new(),
            bond_mode: "per-connection".into(),
            merge_port: 7443,
            socks_port: 10808,
            http_port: 10809,
            allow_lan: false,
            log_level: "warning".into(),
            probe_target: String::new(),
            exit_probe_url: String::new(),
            close_to_tray: false,
        });
        assert_eq!(t.palette.seed(), 0x4285F4);
        assert!(!t.dark);
        let before = t.scheme.primary;
        t.rederive(0xEA4335, true);
        assert_eq!(t.palette.seed(), 0xEA4335);
        assert!(t.dark);
        assert_ne!(t.scheme.primary, before);
    }

    /// State-layer styling actually changes the button background vs active.
    #[test]
    fn button_states_use_state_layers() {
        let t = MtTheme::derive(0x6750A4, false);
        let st = filled_button(t.scheme);
        let active = st(&t.iced_theme(), button::Status::Active);
        let hovered = st(&t.iced_theme(), button::Status::Hovered);
        let disabled = st(&t.iced_theme(), button::Status::Disabled);
        match (active.background, hovered.background) {
            (Some(Background::Color(a)), Some(Background::Color(h))) => assert_ne!(a, h),
            _ => panic!("expected colored backgrounds"),
        }
        match disabled.background {
            Some(Background::Color(c)) => assert_ne!(c, Color::from(t.scheme.primary)),
            _ => panic!("expected disabled background"),
        }
    }

    /// The toggler switch styles checked vs unchecked differently.
    #[test]
    fn switch_styles_differ_by_state() {
        let t = MtTheme::derive(0x6750A4, false);
        let sw = switch(t.scheme);
        let checked = sw(
            &t.iced_theme(),
            toggler::Status::Active { is_toggled: true },
        );
        let unchecked = sw(
            &t.iced_theme(),
            toggler::Status::Active { is_toggled: false },
        );
        assert_ne!(checked.background, unchecked.background);
    }

    /// Text color plumbing: `.color(M3Color)` compiles through the From impl.
    #[test]
    fn text_color_from_m3() {
        let t = MtTheme::derive(0x6750A4, false);
        let _el: iced::widget::Text<'_, iced::Theme, iced::Renderer> =
            text("hello").color(t.scheme.primary);
        let _el2 = tinted_text("hi", t.scheme.on_background);
    }
}
