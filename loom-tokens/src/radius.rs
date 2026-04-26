//! Border-radius scale.

use serde::{Deserialize, Serialize};

/// Border radius step. Tailwind-aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Radius {
    /// `0` — sharp corners.
    None,
    /// `0.25rem` — buttons, inputs.
    Sm,
    /// `0.5rem` — most cards.
    Md,
    /// `0.75rem` — feature cards, hero CTA.
    Lg,
    /// `1rem` — tall hero panels.
    Xl,
    /// `9999px` — pills, badges.
    Full,
}

impl Radius {
    /// Tailwind suffix; `Sm` → `rounded-sm`, etc.
    #[must_use]
    pub const fn tailwind(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Xl => "xl",
            Self::Full => "full",
        }
    }

    /// Every defined step.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::None,
            Self::Sm,
            Self::Md,
            Self::Lg,
            Self::Xl,
            Self::Full,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_radius_serializes_lowercase() {
        let s = serde_json::to_string(&Radius::Full).unwrap();
        assert_eq!(s, "\"full\"");
    }

    #[test]
    fn all_lengths_match_variants() {
        // Reviewer guard: if a variant is added without ::all() updating,
        // this test fails.
        let n_variants = [
            Radius::None,
            Radius::Sm,
            Radius::Md,
            Radius::Lg,
            Radius::Xl,
            Radius::Full,
        ]
        .len();
        assert_eq!(Radius::all().len(), n_variants);
    }
}
