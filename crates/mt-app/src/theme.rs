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
// S16: titlebar window menu (right-click context menu)
// ---------------------------------------------------------------------------

/// The floating menu card: high tonal surface, soft elevation shadow
/// (level-2 menu: the one place a drop shadow earns its keep).
pub fn menu_card(s: M3Scheme) -> impl Fn(&iced::Theme) -> container::Style {
    move |_t| container::Style {
        background: bg(s.surface_container_high),
        border: border_plain(SHAPE.small),
        shadow: Shadow {
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 12.0,
            color: Color {
                a: 0.25,
                ..col(s.on_surface)
            },
        },
        text_color: None,
    }
}

/// One menu row: transparent at rest, secondary-container hover/press —
/// the same selected background the pick-list menus use.
pub fn menu_item(s: M3Scheme) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_t, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => bg(s.secondary_container),
            _ => None,
        },
        text_color: col(s.on_surface),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: SHAPE.small.into(),
        },
        shadow: FLAT,
    }
}

/// The thin separator rule between menu groups (outline-variant hairline).
pub fn menu_rule(s: M3Scheme) -> impl Fn(&iced::Theme) -> container::Style {
    move |_t| container::Style {
        background: Some(Background::Color(Color {
            a: 0.45,
            ..col(s.outline_variant)
        })),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: FLAT,
        text_color: None,
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

// ---------------------------------------------------------------------------
// S18: OS appearance detection (System theme mode)
// ---------------------------------------------------------------------------

/// How often the System mode re-checks the OS preference (Tick is 1 s).
pub const SYSTEM_THEME_POLL_TICKS: u32 = 5;

/// Does the OS prefer a DARK app appearance right now? (S18 System mode.)
///
/// Order of authority:
/// 1. `MT_THEME_SYSTEM_DARK` env override — the rig/test seam (Xvfb has
///    no appearance setting; a deterministic override keeps GUI-rig
///    assertions honest).
/// 2. Windows: `AppsUseLightTheme` in the Personalize registry key (the
///    exact setting the Windows 10/11 "Choose your mode" toggle writes).
/// 3. Linux desktops: the freedesktop `color-scheme` (gsettings/portal),
///    then the GTK3 settings file, then `GTK_THEME` conventions.
/// 4. Unknown → light (the historic default of this app).
pub fn system_prefers_dark() -> bool {
    if let Ok(v) = std::env::var("MT_THEME_SYSTEM_DARK") {
        return matches!(v.trim(), "1" | "true" | "yes" | "dark" | "on");
    }
    sys_prefers_dark_platform()
}

#[cfg(windows)]
fn sys_prefers_dark_platform() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    let sub_key: [u16; 64] = {
        // "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
        const S: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\u{0}";
        let mut a = [0u16; 64];
        for (slot, u) in a.iter_mut().zip(S.encode_utf16()) {
            *slot = u;
        }
        a
    };
    let value: [u16; 20] = {
        const V: &str = "AppsUseLightTheme\u{0}";
        let mut a = [0u16; 20];
        for (slot, u) in a.iter_mut().zip(V.encode_utf16()) {
            *slot = u;
        }
        a
    };
    let mut data: u32 = 1;
    let mut len = std::mem::size_of::<u32>() as u32;
    // SAFETY: RegGetValueW with RRF_RT_REG_DWORD writes at most 4 bytes
    // into `data` (len in/out) and reads the two NUL-terminated UTF-16
    // arrays above; all pointers stay valid for the call. Failure (any
    // return code != 0) means "unknown" → light.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut u32 as *mut core::ffi::c_void,
            &mut len,
        )
    };
    rc == 0 && data == 0 // AppsUseLightTheme=0 → dark
}

#[cfg(not(windows))]
fn sys_prefers_dark_platform() -> bool {
    // gsettings (GNOME/most modern desktops) — prefer-dark is the
    // freedesktop "color-scheme" answer.
    if let Ok(out) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.contains("prefer-dark") {
                return true;
            }
            if s.contains("prefer-light") {
                return false;
            }
        }
    }
    // GTK3 settings.ini fallback.
    if let Some(home) = std::env::var_os("HOME") {
        let ini = std::path::Path::new(&home).join(".config/gtk-3.0/settings.ini");
        if let Ok(raw) = std::fs::read_to_string(ini) {
            for line in raw.lines() {
                let l = line.trim().to_ascii_lowercase();
                if l.starts_with("gtk-application-prefer-dark-theme") {
                    return l.ends_with("=true") || l.ends_with("=1");
                }
            }
        }
    }
    // GTK_THEME=Adwaita:dark style env override.
    matches!(std::env::var("GTK_THEME"), Ok(t) if t.ends_with(":dark") || t.ends_with("-dark"))
}

// ---------------------------------------------------------------------------
// S18: the accent color wheel (HSV disc → seed ARGB)
// ---------------------------------------------------------------------------

/// The embedded 256×256 HSV wheel (RGBA8; `scripts/make_color_wheel.py`).
/// Geometry contract — BOTH sides must stay in sync:
/// - hue    = angle around the center, 0° RED at 12 o'clock, clockwise;
/// - chroma = distance from the center (center ≈ white), full at the rim;
/// - value  = 1.0 (TonalSpot keys on hue+chroma; the tone is normalized
///   away by the palette generator).
pub const COLOR_WHEEL_PX: u32 = 256;
pub const COLOR_WHEEL_BYTES: &[u8] = include_bytes!("../../../assets/color_wheel.rgba");

/// Draw size of the wheel in the Settings card (logical px).
pub const COLOR_WHEEL_DRAW: f32 = 176.0;

/// HSV → RGB (h in degrees 0..360, s/v in 0..1). Returns (r, g, b) 0..255.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// RGB → HSV (h in degrees 0..360, s/v in 0..1).
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if (max - r).abs() < f32::EPSILON {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

/// A point in the wheel's LOCAL square (top-left origin, side = the draw
/// size) → the seed ARGB under it. Outside the rim clamps to the rim at
/// that angle (the center of the edge/corner direction); the exact
/// center (radius 0) is the white seed.
pub fn wheel_seed_at(p: iced::Point, side: f32) -> u32 {
    let cx = p.x - side / 2.0;
    let cy = p.y - side / 2.0;
    // Screen y grows DOWN; the wheel's hue grows CLOCKWISE from 12
    // o'clock, so (dx, dy) already gives clockwise-positive angles.
    let (dx, dy) = (cx, cy);
    let r = (dx * dx + dy * dy).sqrt().clamp(0.0, side / 2.0);
    let sat = if side <= 0.0 {
        0.0
    } else {
        (r / (side / 2.0)).clamp(0.0, 1.0)
    };
    let hue = if r < f32::EPSILON {
        0.0
    } else {
        dy.atan2(dx).to_degrees() + 90.0 // 12 o'clock = 0°
    };
    let (rr, gg, bb) = hsv_to_rgb(hue, sat, 1.0);
    0xFF00_0000 | (rr as u32) << 16 | (gg as u32) << 8 | bb as u32
}

/// The inverse map (for the selection marker): a seed's position in the
/// wheel's local square. Preset swatch seeds land on their wheel spot.
pub fn wheel_marker_position(seed: u32, side: f32) -> iced::Point {
    let (h, s, _) = rgb_to_hsv(
        ((seed >> 16) & 0xFF) as u8,
        ((seed >> 8) & 0xFF) as u8,
        (seed & 0xFF) as u8,
    );
    let r = s * side / 2.0;
    let rad = (h - 90.0).to_radians();
    iced::Point::new(side / 2.0 + rad.cos() * r, side / 2.0 + rad.sin() * r)
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
        let mut t = MtTheme::from_settings(&{
            // S23: one base-line diff off the stock view (the fields
            // from_settings actually reads: seed + mode).
            let mut s = SettingsView::default();
            s.theme_seed = 0x4285F4;
            s.theme_mode = "light".into();
            s.system_proxy = true;
            s.merge_policy = "failover".into();
            s
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

    /// S18: the System env seam decides first (rig determinism).
    #[test]
    fn system_prefers_dark_env_override_wins() {
        // SAFETY of env mutation: tests in this module are single-threaded
        // per process feature set; the var is restored before return.
        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "1") };
        assert!(system_prefers_dark());
        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "true") };
        assert!(system_prefers_dark());
        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "0") };
        assert!(!system_prefers_dark());
        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "") };
        assert!(!system_prefers_dark());
        unsafe { std::env::remove_var("MT_THEME_SYSTEM_DARK") };
    }

    /// S18 wheel geometry: the cardinal spots are the hues the asset
    /// generator draws (0° red at 12 o'clock, clockwise).
    #[test]
    fn wheel_seed_matches_cardinal_hues() {
        let s = COLOR_WHEEL_DRAW;
        let c = s / 2.0;
        // 12 o'clock = red at full saturation.
        assert_eq!(wheel_seed_at(iced::Point::new(c, 0.0), s), 0xFFFF_0000);
        // 3 o'clock = 90° = chartreuse (#80FF00 at v=1).
        assert_eq!(wheel_seed_at(iced::Point::new(s, c), s), 0xFF80_FF00);
        // 6 o'clock = 180° = cyan.
        assert_eq!(wheel_seed_at(iced::Point::new(c, s), s), 0xFF00_FFFF);
        // 9 o'clock = 270° = violet (#8000FF at v=1).
        assert_eq!(wheel_seed_at(iced::Point::new(0.0, c), s), 0xFF80_00FF);
        // center = white.
        assert_eq!(wheel_seed_at(iced::Point::new(c, c), s), 0xFFFF_FFFF);
        // half-way out, 12 o'clock: s=0.5 red → #FF8080.
        assert_eq!(
            wheel_seed_at(iced::Point::new(c, c - s / 4.0), s),
            0xFFFF_8080
        );
    }

    /// Outside the disc clamps to the rim; the seed stays a legal ARGB.
    #[test]
    fn wheel_seed_clamps_outside_points_to_the_rim() {
        let s = COLOR_WHEEL_DRAW;
        let far = wheel_seed_at(iced::Point::new(-50.0, s / 2.0), s);
        assert_eq!(far, 0xFF80_00FF, "9 o'clock rim (full sat at hue 270)");
    }

    /// The inverse map: a picked point's seed lands its marker back at
    /// the same spot, and preset swatches sit inside the disc.
    #[test]
    fn wheel_marker_roundtrips_picked_points() {
        let s = COLOR_WHEEL_DRAW;
        for p in [
            iced::Point::new(s / 2.0, 4.0),
            iced::Point::new(s - 12.0, s / 2.0),
            iced::Point::new(s * 0.3, s * 0.7),
            iced::Point::new(s / 2.0, s / 2.0),
        ] {
            let seed = wheel_seed_at(p, s);
            let back = wheel_marker_position(seed, s);
            assert!(
                (back.x - p.x).abs() < 1.0 && (back.y - p.y).abs() < 1.0,
                "p {p:?} seed {seed:#x} → {back:?}"
            );
        }
        // The Material baseline purple preset (a mid-saturation violet)
        // lands inside the disc, off-center.
        let m = wheel_marker_position(0x6750A4, s);
        let d = ((m.x - s / 2.0).powi(2) + (m.y - s / 2.0).powi(2)).sqrt();
        assert!(d > 4.0 && d < s / 2.0, "marker {m:?} dist {d}");
    }

    /// HSV helpers: pure channel edges and the grayscale center.
    #[test]
    fn hsv_conversions_roundtrip_primaries() {
        for &(r, g, b) in &[
            (255u8, 0u8, 0u8),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
            (255, 255, 255),
            (18, 52, 86),
        ] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert_eq!((r, g, b), (r2, g2, b2), "rgb {r:?} via ({h}, {s}, {v})");
        }
        assert_eq!(rgb_to_hsv(80, 80, 80).1, 0.0, "gray has no saturation");
    }

    /// The embedded wheel asset is the size the contract promises.
    #[test]
    fn wheel_asset_dimensions() {
        assert_eq!(
            COLOR_WHEEL_BYTES.len(),
            (COLOR_WHEEL_PX * COLOR_WHEEL_PX * 4) as usize
        );
    }
}
