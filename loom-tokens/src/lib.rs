//! `loom-tokens` — typed design tokens.
//!
//! Every constant here is a *unit of trust*: a designer or doctrine
//! reviewer signs off on it once, and every component thereafter
//! consumes it instead of inventing its own value. Adding a token is
//! a doctrine change (review required); using a token is free.
//!
//! Tokens are emitted in two shapes:
//!   * As Tailwind class strings (`spacing(4)` → `"4"` → `"px-4"`)
//!     consumed by `loom-components` to build typed components.
//!   * As JSON (`tokens_json()`) so future non-web generators
//!     (GTK, Jetpack Compose, etc.) can consume the same tokens
//!     without re-implementing them.

#![doc(html_no_source)]

pub mod color;
pub mod radius;
pub mod scale;

pub use color::{Color, ColorRole};
pub use radius::Radius;
pub use scale::{Breakpoint, FontSize, Spacing};

use serde::Serialize;

/// Top-level export of every token as JSON. Stable wire format.
///
/// Future cross-platform generators (GTK theme, Jetpack Compose Theme
/// builder, etc.) consume this to ensure pixel-identical sizing and
/// color across platforms.
#[derive(Debug, Serialize)]
pub struct AllTokens {
    /// Palette by semantic role (`primary`, `slate-900`, etc.) →
    /// value (CSS color string). Light theme.
    pub colors: Vec<ColorRole>,
    /// Same role list, dark-theme resolutions. Cross-platform
    /// generators that target a dark surface (GTK dark, Material
    /// You dynamic) consume this slice.
    pub colors_dark: Vec<ColorRole>,
    /// Spacing scale steps.
    pub spacing: Vec<Spacing>,
    /// Breakpoints in pixels.
    pub breakpoints: Vec<Breakpoint>,
    /// Font sizes.
    pub font_sizes: Vec<FontSize>,
    /// Border radii.
    pub radii: Vec<Radius>,
}

/// Serialize every token to a JSON string. Used by cross-platform
/// theme generators and by the doctrine doc check that the surface
/// is still in sync.
///
/// # Panics
/// Never panics in practice — the token tree is finite and every
/// type derives `Serialize`. The `expect` exists so a future-broken
/// derive would fail the build, not silently corrupt output.
#[must_use]
pub fn tokens_json() -> String {
    let all = AllTokens {
        colors: ColorRole::all().to_vec(),
        colors_dark: ColorRole::dark_all().to_vec(),
        spacing: Spacing::all().to_vec(),
        breakpoints: Breakpoint::all().to_vec(),
        font_sizes: FontSize::all().to_vec(),
        radii: Radius::all().to_vec(),
    };
    serde_json::to_string_pretty(&all).expect("token tree is finite + serde-clean")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_json_round_trips() {
        let s = tokens_json();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert!(v.get("colors").is_some());
        assert!(v.get("colors_dark").is_some());
        assert!(v.get("spacing").is_some());
        assert!(v.get("breakpoints").is_some());
        assert!(v.get("font_sizes").is_some());
        assert!(v.get("radii").is_some());
    }

    /// Reviewer guard: if the token surface ships fewer than the
    /// minimums, something has been deleted that probably should
    /// not have been. Bumps to these numbers are intentional.
    #[test]
    fn token_surface_is_at_least_minimum() {
        assert!(ColorRole::all().len() >= 8, "palette shrunk");
        assert!(Spacing::all().len() >= 10, "spacing scale shrunk");
        assert!(Breakpoint::all().len() >= 4, "breakpoints shrunk");
        assert!(FontSize::all().len() >= 7, "font scale shrunk");
        assert!(Radius::all().len() >= 4, "radius scale shrunk");
    }
}
