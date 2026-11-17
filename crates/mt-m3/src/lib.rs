//! mt-m3 — Material You (Material Design 3) design tokens.
//!
//! # CONTRACT
//!
//! Frozen by the orchestrator (see `harness/CONTRACTS-S6.md` "Expert C1"):
//!
//! * [`M3Palette::from_seed(argb)`] is deterministic and yields the full M3
//!   tonal scheme for light AND dark via [`M3Palette::scheme(dark)`].
//! * [`M3Scheme`] carries every token listed in the contract — the field set
//!   is FROZEN and mt-app codes against it. Note: the contract prose says
//!   "35 tokens" but both the prose list and the frozen skeleton enumerate
//!   **36** fields (primary..shadow); the skeleton is authoritative.
//! * [`M3TypeScale`] — the 15 M3 type roles, each `{ size, weight,
//!   line_height }` per the official M3 typesetting spec.
//! * [`shape()`] — the frozen M3 shape scale (4/8/12/16/28).
//! * [`state_layer(base, on, state)`] — hover 8%, focus 10%, pressed 10%
//!   blend semantics (documented on the function). The `on` parameter comes
//!   from the frozen skeleton signature: M3 state layers are the content
//!   ("on") color drawn at fixed opacity over the component base color.
//! * Optional feature `iced` (off by default): `impl From<M3Color> for
//!   iced::Color` and [`M3Palette::iced_theme`] mapping onto iced 0.13's
//!   custom palette. Never enabled by default builds, so `cargo test -p
//!   mt-m3` does not compile iced.
//! * No network at build or test time; deterministic pure functions; no
//!   `unwrap()` in non-test code.
//!
//! # IMPLEMENTATION NOTES
//!
//! **Color engine: `material-colors` 0.5.0** (chosen, not the OKLCH
//! fallback). It is an up-to-date port of Google's `material-color-utilities`:
//! a real HCT color space (CAM16 + CIE L*), tonal palettes and the dynamic
//! color scheme machinery, so `from_seed` reproduces Android 12+ "Material
//! You" wallpaper theming bit-for-bit (TonalSpot variant, spec 2021, phone
//! platform, contrast level 0 — the exact defaults the platform uses).
//! Accepted over the OKLCH approximation because it is the *actual* Google
//! algorithm (not an approximation), is pure Rust with **zero required
//! dependencies** (we build it `no_std` + `libm`), has MSRV 1.97 ≤ our
//! toolchain 1.100, and does no I/O. The `libm` float backend gives
//! platform-independent floating point results, so a seed produces the same
//! colors on every target (with the default `std` backend only last-bit
//! platform variance would be possible). Cross-checks performed on 0.5.0:
//! its `Scheme` struct carries all 36 frozen tokens (fixed-role extras such
//! as `*_fixed` / `surface_tint` are simply not mapped).
//!
//! Determinism: `from_seed` is a pure function (no I/O, no RNG, no global
//! state). Both light and dark schemes are fully computed once in
//! `from_seed`; `scheme()` is a copy accessor. The alpha byte of the seed
//! ARGB is ignored (MCU source colors are opaque), matching Android.
//!
//! Blend semantics ([`state_layer`]): alpha compositing of the `on` color
//! over the opaque sRGB `base`, `out = round(on·a + base·(1−a))` per 8-bit
//! channel, a ∈ {0.08 hover, 0.10 focus, 0.10 pressed} per the M3 spec.
//!
//! Type scale: official M3 typesetting baseline (m3.material.io,
//! "Typesetting" role table). The official values are NOT one strictly
//! decreasing sequence across families (e.g. title small 14 < body large
//! 16), so the monotonic test asserts (a) strict decrease within every
//! family, (b) display large = global maximum, label small = global minimum,
//! (c) the display > headline > title > body > label top-role chain.
//!
//! `thiserror` stays in `Cargo.toml` as frozen, but is currently unused:
//! the frozen API is infallible (`from_seed` accepts any ARGB).
//!
//! Serde derives are kept exactly as frozen. (No serializer is available in
//! the allowed dependency set, so serde is exercised by compilation of the
//! derives rather than a JSON round-trip test.)

#![forbid(unsafe_code)]

use material_colors::{color::Rgb, hct::Hct, scheme::variant::SchemeTonalSpot, scheme::Scheme};
use serde::{Deserialize, Serialize};

/// An 8-bit sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// The complete M3 color scheme (light or dark). Field set is FROZEN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3Scheme {
    pub primary: M3Color,
    pub on_primary: M3Color,
    pub primary_container: M3Color,
    pub on_primary_container: M3Color,
    pub secondary: M3Color,
    pub on_secondary: M3Color,
    pub secondary_container: M3Color,
    pub on_secondary_container: M3Color,
    pub tertiary: M3Color,
    pub on_tertiary: M3Color,
    pub tertiary_container: M3Color,
    pub on_tertiary_container: M3Color,
    pub error: M3Color,
    pub on_error: M3Color,
    pub error_container: M3Color,
    pub on_error_container: M3Color,
    pub background: M3Color,
    pub on_background: M3Color,
    pub surface: M3Color,
    pub on_surface: M3Color,
    pub surface_variant: M3Color,
    pub on_surface_variant: M3Color,
    pub surface_container_lowest: M3Color,
    pub surface_container_low: M3Color,
    pub surface_container: M3Color,
    pub surface_container_high: M3Color,
    pub surface_container_highest: M3Color,
    pub surface_dim: M3Color,
    pub surface_bright: M3Color,
    pub outline: M3Color,
    pub outline_variant: M3Color,
    pub inverse_surface: M3Color,
    pub on_inverse_surface: M3Color,
    pub inverse_primary: M3Color,
    pub scrim: M3Color,
    pub shadow: M3Color,
}

/// One type-scale role (size pt / weight / line-height per M3 spec).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct M3TypeRole {
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
}

/// The M3 type scale (15 roles).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct M3TypeScale {
    pub display_l: M3TypeRole,
    pub display_m: M3TypeRole,
    pub display_s: M3TypeRole,
    pub headline_l: M3TypeRole,
    pub headline_m: M3TypeRole,
    pub headline_s: M3TypeRole,
    pub title_l: M3TypeRole,
    pub title_m: M3TypeRole,
    pub title_s: M3TypeRole,
    pub body_l: M3TypeRole,
    pub body_m: M3TypeRole,
    pub body_s: M3TypeRole,
    pub label_l: M3TypeRole,
    pub label_m: M3TypeRole,
    pub label_s: M3TypeRole,
}

/// M3 shape scale corner radii.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct M3Shape {
    pub extra_small: f32,
    pub small: f32,
    pub medium: f32,
    pub large: f32,
    pub extra_large: f32,
}

/// Interaction state for state layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateLayer {
    Hover,
    Focus,
    Pressed,
}

/// Opacity of the hover state layer (M3 spec: 8%).
const HOVER_OPACITY: f32 = 0.08;
/// Opacity of the focus state layer (M3 spec: 10%).
const FOCUS_OPACITY: f32 = 0.10;
/// Opacity of the pressed state layer (M3 spec: 10%).
const PRESSED_OPACITY: f32 = 0.10;

/// A seed-derived tonal palette (the "dynamic color" source).
///
/// Holds both fully resolved schemes; every accessor is a pure copy.
#[derive(Debug, Clone)]
pub struct M3Palette {
    /// Original seed ARGB (alpha byte ignored), kept for introspection.
    seed: u32,
    light: M3Scheme,
    dark: M3Scheme,
}

impl M3Color {
    /// Builds from an `0xAARRGGBB` integer (the alpha byte is ignored,
    /// matching how [`M3Palette::from_seed`] treats the seed).
    #[must_use]
    pub const fn from_argb(argb: u32) -> Self {
        Self {
            r: ((argb >> 16) & 0xFF) as u8,
            g: ((argb >> 8) & 0xFF) as u8,
            b: (argb & 0xFF) as u8,
        }
    }
}

impl M3Palette {
    /// Deterministic palette from an ARGB seed (like Android wallpaper
    /// theming). Same seed, same palette — guaranteed.
    ///
    /// Uses the Material You dynamic color algorithm: the TonalSpot variant
    /// of Google's material-color-utilities (spec 2021, phone platform,
    /// standard contrast), i.e. exactly what Android 12+ applies to
    /// wallpaper colors. The alpha byte of `argb` is ignored.
    #[must_use]
    pub fn from_seed(argb: u32) -> Self {
        // Source colors are opaque; `Rgb::from_u32` only reads bits 0..24.
        let source = Hct::new(Rgb::from_u32(argb));

        let light = to_m3_scheme(SchemeTonalSpot::new(source, false, None).scheme.into());
        let dark = to_m3_scheme(SchemeTonalSpot::new(source, true, None).scheme.into());

        Self {
            seed: argb,
            light,
            dark,
        }
    }

    /// The seed this palette was built from (alpha byte as passed in).
    #[must_use]
    pub const fn seed(&self) -> u32 {
        self.seed
    }

    /// The full scheme for light or dark mode.
    #[must_use]
    pub const fn scheme(&self, dark: bool) -> M3Scheme {
        if dark {
            self.dark
        } else {
            self.light
        }
    }
}

/// Maps a material-colors resolved scheme onto the frozen [`M3Scheme`].
///
/// Tokens absent from the frozen set (`*_fixed*`, `surface_tint`) are not
/// mapped; MCU's `inverse_on_surface` is the frozen `on_inverse_surface`.
fn to_m3_scheme(s: Scheme) -> M3Scheme {
    fn c(rgb: Rgb) -> M3Color {
        M3Color {
            r: rgb.red,
            g: rgb.green,
            b: rgb.blue,
        }
    }

    M3Scheme {
        primary: c(s.primary),
        on_primary: c(s.on_primary),
        primary_container: c(s.primary_container),
        on_primary_container: c(s.on_primary_container),
        secondary: c(s.secondary),
        on_secondary: c(s.on_secondary),
        secondary_container: c(s.secondary_container),
        on_secondary_container: c(s.on_secondary_container),
        tertiary: c(s.tertiary),
        on_tertiary: c(s.on_tertiary),
        tertiary_container: c(s.tertiary_container),
        on_tertiary_container: c(s.on_tertiary_container),
        error: c(s.error),
        on_error: c(s.on_error),
        error_container: c(s.error_container),
        on_error_container: c(s.on_error_container),
        background: c(s.background),
        on_background: c(s.on_background),
        surface: c(s.surface),
        on_surface: c(s.on_surface),
        surface_variant: c(s.surface_variant),
        on_surface_variant: c(s.on_surface_variant),
        surface_container_lowest: c(s.surface_container_lowest),
        surface_container_low: c(s.surface_container_low),
        surface_container: c(s.surface_container),
        surface_container_high: c(s.surface_container_high),
        surface_container_highest: c(s.surface_container_highest),
        surface_dim: c(s.surface_dim),
        surface_bright: c(s.surface_bright),
        outline: c(s.outline),
        outline_variant: c(s.outline_variant),
        inverse_surface: c(s.inverse_surface),
        on_inverse_surface: c(s.inverse_on_surface),
        inverse_primary: c(s.inverse_primary),
        scrim: c(s.scrim),
        shadow: c(s.shadow),
    }
}

/// The M3 type scale constants (official typesetting baseline values:
/// size in pt, font weight, line height in pt).
#[must_use]
pub const fn type_scale() -> M3TypeScale {
    M3TypeScale {
        display_l: M3TypeRole {
            size: 57.0,
            weight: 400,
            line_height: 64.0,
        },
        display_m: M3TypeRole {
            size: 45.0,
            weight: 400,
            line_height: 52.0,
        },
        display_s: M3TypeRole {
            size: 36.0,
            weight: 400,
            line_height: 44.0,
        },
        headline_l: M3TypeRole {
            size: 32.0,
            weight: 400,
            line_height: 40.0,
        },
        headline_m: M3TypeRole {
            size: 28.0,
            weight: 400,
            line_height: 36.0,
        },
        headline_s: M3TypeRole {
            size: 24.0,
            weight: 400,
            line_height: 32.0,
        },
        title_l: M3TypeRole {
            size: 22.0,
            weight: 400,
            line_height: 28.0,
        },
        title_m: M3TypeRole {
            size: 16.0,
            weight: 500,
            line_height: 24.0,
        },
        title_s: M3TypeRole {
            size: 14.0,
            weight: 500,
            line_height: 20.0,
        },
        body_l: M3TypeRole {
            size: 16.0,
            weight: 400,
            line_height: 24.0,
        },
        body_m: M3TypeRole {
            size: 14.0,
            weight: 400,
            line_height: 20.0,
        },
        body_s: M3TypeRole {
            size: 12.0,
            weight: 400,
            line_height: 16.0,
        },
        label_l: M3TypeRole {
            size: 14.0,
            weight: 500,
            line_height: 20.0,
        },
        label_m: M3TypeRole {
            size: 12.0,
            weight: 500,
            line_height: 16.0,
        },
        label_s: M3TypeRole {
            size: 11.0,
            weight: 500,
            line_height: 16.0,
        },
    }
}

/// The M3 shape scale constants (corner radii in dp).
#[must_use]
pub const fn shape() -> M3Shape {
    M3Shape {
        extra_small: 4.0,
        small: 8.0,
        medium: 12.0,
        large: 16.0,
        extra_large: 28.0,
    }
}

/// Blend a state layer onto a base color (hover 8%, focus 10%,
/// pressed 10% of the ON color per M3).
///
/// # Blend semantics
///
/// M3 state layers are drawn as the content color (`on`) at a fixed
/// opacity over the component's base color. This composites alpha-over on
/// an opaque sRGB base:
///
/// ```text
/// out_channel = round(on_channel * a + base_channel * (1 - a))
/// ```
///
/// with `a` = 0.08 for [`StateLayer::Hover`], 0.10 for
/// [`StateLayer::Focus`], 0.10 for [`StateLayer::Pressed`] (M3 spec).
/// The result is opaque and stays within `0..=255` (convex combination),
/// so no clamping is needed. Blending a color over itself is the identity.
#[must_use]
pub fn state_layer(base: M3Color, on: M3Color, state: StateLayer) -> M3Color {
    let alpha = match state {
        StateLayer::Hover => HOVER_OPACITY,
        StateLayer::Focus => FOCUS_OPACITY,
        StateLayer::Pressed => PRESSED_OPACITY,
    };
    blend_over(base, on, alpha)
}

/// Alpha-composite `overlay` over `base` at opacity `alpha` (0.0..=1.0).
fn blend_over(base: M3Color, overlay: M3Color, alpha: f32) -> M3Color {
    fn mix(base: u8, overlay: u8, alpha: f32) -> u8 {
        let b = f32::from(base);
        let o = f32::from(overlay);
        // Convex combination of two u8 values stays in 0..=255.
        (o * alpha + b * (1.0 - alpha)).round() as u8
    }
    M3Color {
        r: mix(base.r, overlay.r, alpha),
        g: mix(base.g, overlay.g, alpha),
        b: mix(base.b, overlay.b, alpha),
    }
}

#[cfg(feature = "iced")]
impl From<M3Color> for iced::Color {
    fn from(c: M3Color) -> Self {
        iced::Color::from_rgb8(c.r, c.g, c.b)
    }
}

#[cfg(feature = "iced")]
impl M3Palette {
    /// Maps this palette onto iced 0.13's custom palette.
    ///
    /// iced's five palette slots are filled as: `background` ← background,
    /// `text` ← on_background, `primary` ← primary, `success` ← tertiary
    /// (M3 has no success role; tertiary is the customary stand-in), and
    /// `danger` ← error. The theme is a [`iced::Theme::Custom`] carrying
    /// the generated extended palette, so iced widgets resolve through it.
    #[must_use]
    pub fn iced_theme(&self, dark: bool) -> iced::Theme {
        let s = self.scheme(dark);
        let palette = iced::theme::Palette {
            background: s.background.into(),
            text: s.on_background.into(),
            primary: s.primary.into(),
            success: s.tertiary.into(),
            danger: s.error.into(),
        };
        let name = if dark { "mt-m3-dark" } else { "mt-m3-light" };
        iced::Theme::custom(String::from(name), palette)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Representative seeds: M3 baseline purple, Google blue/green/red,
    /// black, white. Deterministic, no network.
    const SEEDS: [u32; 6] = [
        0xFF_67_50_A4, // baseline M3 purple
        0xFF_42_85_F4, // blue
        0xFF_34_A8_53, // green
        0xFF_EA_43_35, // red
        0xFF_00_00_00, // black
        0xFF_FF_FF_FF, // white
    ];

    /// Every frozen token with its name — compile-checked to be exactly 36
    /// entries (a missing/renamed `M3Scheme` field breaks this function).
    fn tokens(s: &M3Scheme) -> [(&'static str, M3Color); 36] {
        [
            ("primary", s.primary),
            ("on_primary", s.on_primary),
            ("primary_container", s.primary_container),
            ("on_primary_container", s.on_primary_container),
            ("secondary", s.secondary),
            ("on_secondary", s.on_secondary),
            ("secondary_container", s.secondary_container),
            ("on_secondary_container", s.on_secondary_container),
            ("tertiary", s.tertiary),
            ("on_tertiary", s.on_tertiary),
            ("tertiary_container", s.tertiary_container),
            ("on_tertiary_container", s.on_tertiary_container),
            ("error", s.error),
            ("on_error", s.on_error),
            ("error_container", s.error_container),
            ("on_error_container", s.on_error_container),
            ("background", s.background),
            ("on_background", s.on_background),
            ("surface", s.surface),
            ("on_surface", s.on_surface),
            ("surface_variant", s.surface_variant),
            ("on_surface_variant", s.on_surface_variant),
            ("surface_container_lowest", s.surface_container_lowest),
            ("surface_container_low", s.surface_container_low),
            ("surface_container", s.surface_container),
            ("surface_container_high", s.surface_container_high),
            ("surface_container_highest", s.surface_container_highest),
            ("surface_dim", s.surface_dim),
            ("surface_bright", s.surface_bright),
            ("outline", s.outline),
            ("outline_variant", s.outline_variant),
            ("inverse_surface", s.inverse_surface),
            ("on_inverse_surface", s.on_inverse_surface),
            ("inverse_primary", s.inverse_primary),
            ("scrim", s.scrim),
            ("shadow", s.shadow),
        ]
    }

    /// The on/off contrast pairs the contract requires to be legible.
    fn on_off_pairs(s: &M3Scheme) -> [(&'static str, M3Color, M3Color); 12] {
        [
            ("primary/on_primary", s.primary, s.on_primary),
            ("secondary/on_secondary", s.secondary, s.on_secondary),
            ("tertiary/on_tertiary", s.tertiary, s.on_tertiary),
            ("error/on_error", s.error, s.on_error),
            (
                "primary_container/on_primary_container",
                s.primary_container,
                s.on_primary_container,
            ),
            (
                "secondary_container/on_secondary_container",
                s.secondary_container,
                s.on_secondary_container,
            ),
            (
                "tertiary_container/on_tertiary_container",
                s.tertiary_container,
                s.on_tertiary_container,
            ),
            (
                "error_container/on_error_container",
                s.error_container,
                s.on_error_container,
            ),
            ("background/on_background", s.background, s.on_background),
            ("surface/on_surface", s.surface, s.on_surface),
            (
                "surface_variant/on_surface_variant",
                s.surface_variant,
                s.on_surface_variant,
            ),
            (
                "inverse_surface/on_inverse_surface",
                s.inverse_surface,
                s.on_inverse_surface,
            ),
        ]
    }

    fn roles(scale: &M3TypeScale) -> [(&'static str, M3TypeRole); 15] {
        [
            ("display_l", scale.display_l),
            ("display_m", scale.display_m),
            ("display_s", scale.display_s),
            ("headline_l", scale.headline_l),
            ("headline_m", scale.headline_m),
            ("headline_s", scale.headline_s),
            ("title_l", scale.title_l),
            ("title_m", scale.title_m),
            ("title_s", scale.title_s),
            ("body_l", scale.body_l),
            ("body_m", scale.body_m),
            ("body_s", scale.body_s),
            ("label_l", scale.label_l),
            ("label_m", scale.label_m),
            ("label_s", scale.label_s),
        ]
    }

    /// WCAG 2.x relative luminance of an 8-bit sRGB color.
    fn relative_luminance(c: M3Color) -> f64 {
        fn linear(u: u8) -> f64 {
            let s = f64::from(u) / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    }

    /// WCAG contrast ratio between two colors.
    fn contrast(a: M3Color, b: M3Color) -> f64 {
        let (x, y) = (relative_luminance(a), relative_luminance(b));
        let (hi, lo) = if x >= y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn palette_is_deterministic() {
        for &seed in &SEEDS {
            let a = M3Palette::from_seed(seed);
            let b = M3Palette::from_seed(seed);
            assert_eq!(
                a.scheme(false),
                b.scheme(false),
                "light differs for {seed:#x}"
            );
            assert_eq!(a.scheme(true), b.scheme(true), "dark differs for {seed:#x}");
            assert_eq!(a.seed(), b.seed());
        }
    }

    #[test]
    fn seed_alpha_is_ignored() {
        let with_alpha = M3Palette::from_seed(0xFF_67_50_A4);
        let no_alpha = M3Palette::from_seed(0x00_67_50_A4);
        assert_eq!(with_alpha.scheme(false), no_alpha.scheme(false));
        assert_eq!(with_alpha.scheme(true), no_alpha.scheme(true));
    }

    #[test]
    fn different_seeds_produce_different_schemes() {
        let purple = M3Palette::from_seed(0xFF_67_50_A4).scheme(false);
        let blue = M3Palette::from_seed(0xFF_42_85_F4).scheme(false);
        let green = M3Palette::from_seed(0xFF_34_A8_53).scheme(false);
        assert_ne!(purple.primary, blue.primary);
        assert_ne!(purple.primary, green.primary);
        assert_ne!(blue.primary, green.primary);
    }

    #[test]
    fn all_36_tokens_present_with_unique_names() {
        for &seed in &SEEDS {
            let p = M3Palette::from_seed(seed);
            for dark in [false, true] {
                let t = tokens(&p.scheme(dark));
                assert_eq!(t.len(), 36, "frozen token count changed");
                for (i, (name, _)) in t.iter().enumerate() {
                    for (j, (other, _)) in t.iter().enumerate() {
                        if i < j {
                            assert_ne!(name, other, "duplicate token name {name}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn light_and_dark_differ_meaningfully() {
        for &seed in &SEEDS {
            let p = M3Palette::from_seed(seed);
            let light = tokens(&p.scheme(false));
            let dark = tokens(&p.scheme(true));

            let differing = light
                .iter()
                .zip(dark.iter())
                .filter(|(l, d)| l.1 != d.1)
                .count();
            assert!(
                differing >= 25,
                "only {differing}/36 tokens differ light vs dark for {seed:#x}"
            );

            // Key roles must visibly differ between modes.
            assert_ne!(
                light[0].1, dark[0].1,
                "primary identical across modes for {seed:#x}"
            );
            assert_ne!(
                light[16].1, dark[16].1,
                "background identical for {seed:#x}"
            );
            assert_ne!(
                light[19].1, dark[19].1,
                "on_surface identical for {seed:#x}"
            );
        }
    }

    #[test]
    fn scheme_ground_roles_are_sane() {
        for &seed in &SEEDS {
            let p = M3Palette::from_seed(seed);

            for dark in [false, true] {
                let s = p.scheme(dark);
                // Scrim and shadow are tone-0 neutrals: pure black everywhere.
                assert_eq!(s.scrim, M3Color { r: 0, g: 0, b: 0 });
                assert_eq!(s.shadow, M3Color { r: 0, g: 0, b: 0 });
            }

            let light = p.scheme(false);
            // Light on-primary is tone 100 (white); background is tone ~98.
            assert!(
                light.on_primary.r >= 250 && light.on_primary.g >= 250 && light.on_primary.b >= 250
            );
            assert!(
                light.background.r >= 240 && light.background.g >= 240 && light.background.b >= 240
            );

            let dark = p.scheme(true);
            // Dark surfaces are very dark (tones 4..=24).
            assert!(dark.background.r <= 30 && dark.background.g <= 30 && dark.background.b <= 30);
            assert!(
                dark.on_background.r >= 200
                    && dark.on_background.g >= 200
                    && dark.on_background.b >= 200,
                "dark on_background must be light"
            );
        }
    }

    #[test]
    fn on_off_pairs_meet_contrast_3() {
        for &seed in &SEEDS {
            let p = M3Palette::from_seed(seed);
            for dark in [false, true] {
                for (name, bg, fg) in on_off_pairs(&p.scheme(dark)) {
                    let ratio = contrast(bg, fg);
                    assert!(
                        ratio >= 3.0,
                        "{name} contrast {ratio:.2} < 3.0 (seed {seed:#x}, dark={dark})"
                    );
                }
            }
        }
    }

    #[test]
    fn type_scale_matches_official_spec() {
        const EXPECTED: [(&str, f32, u16, f32); 15] = [
            ("display_l", 57.0, 400, 64.0),
            ("display_m", 45.0, 400, 52.0),
            ("display_s", 36.0, 400, 44.0),
            ("headline_l", 32.0, 400, 40.0),
            ("headline_m", 28.0, 400, 36.0),
            ("headline_s", 24.0, 400, 32.0),
            ("title_l", 22.0, 400, 28.0),
            ("title_m", 16.0, 500, 24.0),
            ("title_s", 14.0, 500, 20.0),
            ("body_l", 16.0, 400, 24.0),
            ("body_m", 14.0, 400, 20.0),
            ("body_s", 12.0, 400, 16.0),
            ("label_l", 14.0, 500, 20.0),
            ("label_m", 12.0, 500, 16.0),
            ("label_s", 11.0, 500, 16.0),
        ];

        for (got, expected) in roles(&type_scale()).iter().zip(EXPECTED.iter()) {
            let (name, role) = *got;
            let (ename, size, weight, line_height) = *expected;
            assert_eq!(name, ename);
            assert_eq!(
                (role.size, role.weight, role.line_height),
                (size, weight, line_height),
                "role {name}"
            );
        }
    }

    #[test]
    fn type_scale_is_monotonic() {
        let scale = type_scale();
        let by_name: std::collections::HashMap<&str, f32> =
            roles(&scale).iter().map(|(n, r)| (*n, r.size)).collect();

        // (1) Strict decrease within every family.
        let families: [(&str, [&str; 3]); 5] = [
            ("display", ["display_l", "display_m", "display_s"]),
            ("headline", ["headline_l", "headline_m", "headline_s"]),
            ("title", ["title_l", "title_m", "title_s"]),
            ("body", ["body_l", "body_m", "body_s"]),
            ("label", ["label_l", "label_m", "label_s"]),
        ];
        for (family, [l, m, s]) in families {
            let (l, m, s) = (by_name[l], by_name[m], by_name[s]);
            assert!(
                l > m && m > s,
                "{family} family not decreasing: {l} {m} {s}"
            );
        }

        // (2) Global extremes: display_l is the max, label_s the min.
        let mut sizes: Vec<f32> = by_name.values().copied().collect();
        sizes.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(
            by_name["display_l"], sizes[14],
            "display_l must be the largest size"
        );
        assert_eq!(
            by_name["label_s"], sizes[0],
            "label_s must be the smallest size"
        );

        // (3) The top-role chain display > headline > title > body > label.
        assert!(by_name["display_l"] > by_name["headline_l"]);
        assert!(by_name["headline_l"] > by_name["title_l"]);
        assert!(by_name["title_l"] > by_name["body_l"]);
        assert!(by_name["body_l"] > by_name["label_s"]);
    }

    #[test]
    fn shape_scale_has_frozen_values() {
        let s = shape();
        assert_eq!(
            (s.extra_small, s.small, s.medium, s.large, s.extra_large),
            (4.0, 8.0, 12.0, 16.0, 28.0)
        );
    }

    #[test]
    fn state_layers_change_the_color() {
        let black = M3Color { r: 0, g: 0, b: 0 };
        let white = M3Color {
            r: 255,
            g: 255,
            b: 255,
        };

        // Exact alpha compositing: 8% white over black -> 20.4 -> 20;
        // 10% -> 25.5 -> 26 (deterministic IEEE f32, round half away).
        assert_eq!(
            state_layer(black, white, StateLayer::Hover),
            M3Color {
                r: 20,
                g: 20,
                b: 20
            }
        );
        assert_eq!(
            state_layer(black, white, StateLayer::Focus),
            M3Color {
                r: 26,
                g: 26,
                b: 26
            }
        );
        assert_eq!(
            state_layer(black, white, StateLayer::Pressed),
            state_layer(black, white, StateLayer::Focus)
        );

        // Hover (8%) is strictly weaker than focus/pressed (10%).
        let hover = state_layer(black, white, StateLayer::Hover);
        let focus = state_layer(black, white, StateLayer::Focus);
        assert!(hover.r < focus.r);

        // Blending a color over itself is the identity for every state.
        let base = M3Color {
            r: 42,
            g: 128,
            b: 213,
        };
        for state in [StateLayer::Hover, StateLayer::Focus, StateLayer::Pressed] {
            assert_eq!(state_layer(base, base, state), base);
        }

        // A different on-color always changes the base.
        for state in [StateLayer::Hover, StateLayer::Focus, StateLayer::Pressed] {
            assert_ne!(state_layer(base, white, state), base);
        }

        // Symmetric check on an arbitrary base: 10% black over 128 gray.
        let dimmed = state_layer(
            M3Color {
                r: 128,
                g: 128,
                b: 128,
            },
            black,
            StateLayer::Focus,
        );
        assert_eq!(
            dimmed,
            M3Color {
                r: 115,
                g: 115,
                b: 115
            }
        );
    }

    #[test]
    fn color_from_argb_ignores_alpha() {
        assert_eq!(
            M3Color::from_argb(0xFF_12_34_56),
            M3Color {
                r: 0x12,
                g: 0x34,
                b: 0x56
            }
        );
        assert_eq!(
            M3Color::from_argb(0x00_12_34_56),
            M3Color {
                r: 0x12,
                g: 0x34,
                b: 0x56
            }
        );
    }

    #[cfg(feature = "iced")]
    #[test]
    fn iced_theme_maps_the_scheme() {
        let p = M3Palette::from_seed(0xFF_67_50_A4);
        for dark in [false, true] {
            let s = p.scheme(dark);
            let palette = p.iced_theme(dark).palette();
            assert_eq!(palette.background, s.background.into());
            assert_eq!(palette.text, s.on_background.into());
            assert_eq!(palette.primary, s.primary.into());
            assert_eq!(palette.success, s.tertiary.into());
            assert_eq!(palette.danger, s.error.into());
        }
        // Light and dark produce distinct custom themes.
        assert_ne!(p.iced_theme(false), p.iced_theme(true));
    }
}
