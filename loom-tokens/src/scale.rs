//! Numeric scales: spacing, breakpoints, font sizes.
//!
//! Each scale is a fixed enumeration. Components consume scale steps
//! through typed enums; a request to add a new step is a doctrine
//! change.

use serde::{Deserialize, Serialize};

/// Spacing scale step. Maps to Tailwind `{prefix}-{step}` utilities,
/// e.g. `Spacing::S4` → `"4"` → `px-4`, `py-4`, `gap-4`.
///
/// The value is the raw Tailwind step number, not pixels. That keeps
/// the scale composable across dark/light themes and breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spacing {
    /// 0
    S0,
    /// 0.25rem
    S1,
    /// 0.5rem
    S2,
    /// 0.75rem
    S3,
    /// 1rem
    S4,
    /// 1.5rem
    S6,
    /// 2rem
    S8,
    /// 3rem
    S12,
    /// 4rem
    S16,
    /// 6rem
    S24,
}

impl Spacing {
    /// Tailwind step number as a string slice.
    #[must_use]
    pub const fn tailwind(self) -> &'static str {
        match self {
            Self::S0 => "0",
            Self::S1 => "1",
            Self::S2 => "2",
            Self::S3 => "3",
            Self::S4 => "4",
            Self::S6 => "6",
            Self::S8 => "8",
            Self::S12 => "12",
            Self::S16 => "16",
            Self::S24 => "24",
        }
    }

    /// Logical rem value of this step.
    ///
    /// Tailwind's spacing scale is `0.25rem * step` for the standard
    /// steps; this matches that. Used by the cross-platform token
    /// generators (CSS custom properties, egui px constants).
    #[must_use]
    pub const fn rem(self) -> f32 {
        match self {
            Self::S0 => 0.0,
            Self::S1 => 0.25,
            Self::S2 => 0.5,
            Self::S3 => 0.75,
            Self::S4 => 1.0,
            Self::S6 => 1.5,
            Self::S8 => 2.0,
            Self::S12 => 3.0,
            Self::S16 => 4.0,
            Self::S24 => 6.0,
        }
    }

    /// Logical pixel value at the design root font size (16 px).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub const fn px(self) -> u32 {
        // Cast through u32: every rem step lands on a whole non-negative
        // pixel at 16px root, so truncation / sign-loss can't fire.
        (self.rem() * 16.0) as u32
    }

    /// Every defined step. Used by tests + JSON export.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::S0,
            Self::S1,
            Self::S2,
            Self::S3,
            Self::S4,
            Self::S6,
            Self::S8,
            Self::S12,
            Self::S16,
            Self::S24,
        ]
    }
}

/// Responsive breakpoints. Tailwind-aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Breakpoint {
    /// `640px` — `sm:`
    Sm,
    /// `768px` — `md:`
    Md,
    /// `1024px` — `lg:`
    Lg,
    /// `1280px` — `xl:`
    Xl,
}

impl Breakpoint {
    /// Tailwind prefix without the trailing colon.
    #[must_use]
    pub const fn tailwind(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Xl => "xl",
        }
    }

    /// Pixel width at which the breakpoint activates.
    #[must_use]
    pub const fn px(self) -> u32 {
        match self {
            Self::Sm => 640,
            Self::Md => 768,
            Self::Lg => 1024,
            Self::Xl => 1280,
        }
    }

    /// Every defined breakpoint.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Sm, Self::Md, Self::Lg, Self::Xl]
    }
}

/// Font size step. Tailwind-aligned, with semantic aliases at the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontSize {
    /// `0.75rem` (12px)
    Xs,
    /// `0.875rem` (14px)
    Sm,
    /// `1rem` (16px) — default body.
    Base,
    /// `1.125rem` (18px)
    Lg,
    /// `1.25rem` (20px)
    Xl,
    /// `1.5rem` (24px)
    H3,
    /// `1.875rem` (30px)
    H2,
    /// `2.25rem` (36px) — page heading desktop.
    H1,
    /// `3rem` (48px) — hero headline.
    Hero,
}

impl FontSize {
    /// Tailwind class suffix, e.g. `"sm"` → `text-sm`.
    #[must_use]
    pub const fn tailwind(self) -> &'static str {
        match self {
            Self::Xs => "xs",
            Self::Sm => "sm",
            Self::Base => "base",
            Self::Lg => "lg",
            Self::Xl => "xl",
            Self::H3 => "2xl",
            Self::H2 => "3xl",
            Self::H1 => "4xl",
            Self::Hero => "6xl",
        }
    }

    /// CSS-shaped value used by the cross-platform token emitters.
    #[must_use]
    pub const fn css_size(self) -> &'static str {
        match self {
            Self::Xs => "0.75rem",
            Self::Sm => "0.875rem",
            Self::Base => "1rem",
            Self::Lg => "1.125rem",
            Self::Xl => "1.25rem",
            Self::H3 => "1.5rem",
            Self::H2 => "1.875rem",
            Self::H1 => "2.25rem",
            Self::Hero => "3rem",
        }
    }

    /// Every defined size. Used by tests + JSON export.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Xs,
            Self::Sm,
            Self::Base,
            Self::Lg,
            Self::Xl,
            Self::H3,
            Self::H2,
            Self::H1,
            Self::Hero,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_step_4_is_what_we_expect() {
        assert_eq!(Spacing::S4.tailwind(), "4");
    }

    #[test]
    fn breakpoints_in_ascending_pixel_order() {
        let mut prev = 0;
        for bp in Breakpoint::all() {
            assert!(bp.px() > prev, "breakpoint regressed at {bp:?}");
            prev = bp.px();
        }
    }

    #[test]
    fn font_sizes_are_unique_tailwind_strings() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for fs in FontSize::all() {
            assert!(seen.insert(fs.tailwind()), "duplicate fs tailwind: {fs:?}");
        }
    }

    #[test]
    fn spacing_serializes_to_snake_case() {
        let json = serde_json::to_string(&Spacing::S4).unwrap();
        assert_eq!(json, "\"s4\"");
    }
}
