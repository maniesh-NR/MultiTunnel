//! mt-m3 — Material You (Material Design 3) design tokens.
//!
//! CONTRACT (frozen by the orchestrator; see harness/CONTRACTS-S6.md
//! "Expert C1"): `M3Palette::from_seed(argb)` is deterministic and yields
//! the full M3 tonal scheme for light AND dark; `M3Scheme` carries every
//! token listed below (the field set is frozen — mt-app codes against
//! it); `M3TypeScale`/`M3Shape`/`state_layer` complete the system.
//! Prefer the `material-colors` crate; a documented OKLCH approximation is
//! the accepted fallback. No network at build or test time.

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

/// A seed-derived tonal palette (the "dynamic color" source).
#[derive(Debug, Clone)]
pub struct M3Palette {
    _private: (),
}

impl M3Palette {
    /// Deterministic palette from an ARGB seed (like Android wallpaper
    /// theming). Same seed, same palette — guaranteed.
    pub fn from_seed(argb: u32) -> Self {
        let _ = argb;
        todo!("Expert C1: implement per harness/CONTRACTS-S6.md")
    }

    /// The full scheme for light or dark mode.
    pub fn scheme(&self, dark: bool) -> M3Scheme {
        let _ = dark;
        todo!("Expert C1: implement per harness/CONTRACTS-S6.md")
    }
}

/// The M3 type scale constants.
pub fn type_scale() -> M3TypeScale {
    todo!("Expert C1: implement per harness/CONTRACTS-S6.md")
}

/// The M3 shape scale constants.
pub fn shape() -> M3Shape {
    todo!("Expert C1: implement per harness/CONTRACTS-S6.md")
}

/// Blend a state layer onto a base color (hover 8%, focus 10%,
/// pressed 10% of the ON color per M3).
pub fn state_layer(base: M3Color, on: M3Color, state: StateLayer) -> M3Color {
    let _ = (base, on, state);
    todo!("Expert C1: implement per harness/CONTRACTS-S6.md")
}
