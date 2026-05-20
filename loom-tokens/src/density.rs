//! `density` — canonical density-tier vocabulary.
//!
//! Closes part of task #217 / preamble #109 (reference corpus +
//! density tiers).
//!
//! Most existing primitives carry their own density-ish enum
//! (`KvPairDensity { Compact, Comfortable, Spacious }`,
//! `FormDensity { Compact, Comfortable, Spacious }`). They're
//! per-primitive vocabulary. The substrate needs a CANONICAL
//! tier vocabulary that:
//!
//! * Per-primitive density enums can map to (so audit reports
//!   can roll up "this page is mostly Sparse content" or "this
//!   page is mostly Dense content" across primitive types).
//! * The reference-corpus density classification consumes (so
//!   "Stripe is Comfortable density" / "Linear is Dense density"
//!   is a directly-comparable property).
//! * Future detector axes ("page is too sparse for the content
//!   it carries" / "page is too dense for its target audience")
//!   can score against.
//!
//! ## Tiers
//!
//! Four tiers, ordered by visual + information density:
//!
//! * `Sparse` — substantial whitespace, single-column layouts,
//!   1-2 ideas per viewport. Reference exemplars: linear.app
//!   marketing pages, Apple product pages.
//! * `Comfortable` — balanced whitespace, typical SaaS marketing
//!   density, 3-5 ideas per viewport. Reference exemplars:
//!   stripe.com homepage, notion.so marketing.
//! * `Dense` — editorial / dashboard density, 8-15 ideas per
//!   viewport, columns + kv-pairs + grids. Reference exemplars:
//!   the Stripe docs, github.com profile pages, datadoghq.com.
//! * `Extreme` — extreme info-density, every-pixel-earns-its-
//!   place. Reference exemplars: bloomberg.com terminal screens,
//!   hackernews.com, terminal-style displays.
//!
//! Density does NOT correlate with quality — a Sparse marketing
//! page can be excellent for its audience; a Dense dashboard is
//! correct for its operator user. The tier is a CLASSIFICATION,
//! not a quality judgment.
//!
//! ## Char-per-1000sqpx guidance
//!
//! Approximate visible-text density per 1000 CSS pixels² at
//! 1280×800 viewport (empirical from the reference corpus):
//!
//! * `Sparse`     ~30-80 chars per 1000sqpx
//! * `Comfortable` ~80-180 chars per 1000sqpx
//! * `Dense`      ~180-400 chars per 1000sqpx
//! * `Extreme`    >400 chars per 1000sqpx
//!
//! These are loose bands — a single tier can span a 2-3x range
//! because typeface, line-height, and column-width all swing
//! the visible density. The tier classification is for HUMAN
//! intent ("we're targeting Comfortable density"), not for
//! tight numeric bounds.
//!
//! ## Default
//!
//! `Comfortable` is the default — most marketing + editorial
//! sites land here. Sparse is the opt-in for premium / luxe
//! contexts; Dense + Extreme are operator-tool contexts.
//!
//! AVP-2 INVARIANTS
//! ----------------
//! * `unsafe_code = "deny"` (inherited).
//! * Pure enum + impl; no I/O.
//! * `#[non_exhaustive]` on the enum would lock out exhaustive
//!   matches in consumer code, so we deliberately omit it here.
//!   Adding a tier is a doctrine change.

use serde::{Deserialize, Serialize};

/// Canonical density-tier vocabulary.
///
/// See module docs for the per-tier definition + reference
/// exemplars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DensityTier {
    /// Substantial whitespace, 1-2 ideas per viewport. Premium
    /// marketing + product-page default.
    Sparse,
    /// Balanced whitespace, 3-5 ideas per viewport. Substrate
    /// default. SaaS marketing default.
    Comfortable,
    /// Editorial / dashboard density, 8-15 ideas per viewport.
    /// Operator-tool default.
    Dense,
    /// Extreme info-density, every-pixel-earns-its-place.
    /// Terminal-style / pro-trader-tool default.
    Extreme,
}

impl Default for DensityTier {
    /// `Comfortable` — the substrate default. Most marketing +
    /// editorial sites land here.
    fn default() -> Self {
        Self::Comfortable
    }
}

impl DensityTier {
    /// Stable kebab-case wire string. Part of the wire shape;
    /// detector axes + audit reports index by these.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Sparse => "sparse",
            Self::Comfortable => "comfortable",
            Self::Dense => "dense",
            Self::Extreme => "extreme",
        }
    }

    /// CSS class name for the data-driven tier hook. Loom-skin
    /// can target these via `.loom-density-<slug>` cascades.
    #[must_use]
    pub fn css_class(self) -> String {
        format!("loom-density-{}", self.slug())
    }

    /// Approximate char-per-1000sqpx band as `(low, high)`.
    /// Returns the rough numeric range the tier targets at the
    /// 1280×800 baseline viewport. Pure heuristic; future audit
    /// phases can refine.
    #[must_use]
    pub const fn char_per_1000sqpx(self) -> (u32, u32) {
        match self {
            Self::Sparse => (30, 80),
            Self::Comfortable => (80, 180),
            Self::Dense => (180, 400),
            Self::Extreme => (400, u32::MAX),
        }
    }

    /// All tiers ordered by density (sparse first). Useful for
    /// rendering tier-pickers + for iterating all tiers in tests.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [Self::Sparse, Self::Comfortable, Self::Dense, Self::Extreme]
    }

    /// Classify an empirical char-per-1000sqpx measurement to the
    /// nearest tier. Conservative — the boundary char counts map
    /// to the LOWER tier (sparse boundary 80c is still Sparse,
    /// 81c is Comfortable). The Extreme cap is open-ended.
    #[must_use]
    pub const fn classify(chars_per_1000sqpx: u32) -> Self {
        if chars_per_1000sqpx <= 80 {
            Self::Sparse
        } else if chars_per_1000sqpx <= 180 {
            Self::Comfortable
        } else if chars_per_1000sqpx <= 400 {
            Self::Dense
        } else {
            Self::Extreme
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_comfortable() {
        assert_eq!(DensityTier::default(), DensityTier::Comfortable);
    }

    #[test]
    fn all_tiers_have_unique_slugs() {
        let slugs: Vec<_> = DensityTier::all().iter().map(|t| t.slug()).collect();
        let mut sorted = slugs.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(slugs.len(), sorted.len(), "duplicate slugs across tiers");
    }

    #[test]
    fn slug_is_stable_kebab_case() {
        for tier in DensityTier::all() {
            let s = tier.slug();
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "tier slug not kebab-case: {s:?}"
            );
        }
    }

    #[test]
    fn css_class_prefix_is_loom_density() {
        for tier in DensityTier::all() {
            assert!(
                tier.css_class().starts_with("loom-density-"),
                "tier css_class not loom-density-prefixed: {}",
                tier.css_class()
            );
        }
    }

    #[test]
    fn char_per_1000sqpx_bands_are_monotonic_increasing() {
        // Each tier's band starts at the previous tier's ceiling.
        // Lower bound is monotonically increasing tier-to-tier.
        let bands: Vec<(u32, u32)> = DensityTier::all()
            .iter()
            .map(|t| t.char_per_1000sqpx())
            .collect();
        for w in bands.windows(2) {
            let (_, prev_high) = w[0];
            let (next_low, _) = w[1];
            assert!(
                next_low >= prev_high,
                "tier bands overlap: {prev_high} >= {next_low}"
            );
        }
    }

    #[test]
    fn extreme_band_is_open_ended() {
        let (_, hi) = DensityTier::Extreme.char_per_1000sqpx();
        assert_eq!(hi, u32::MAX, "Extreme tier should be open-ended");
    }

    #[test]
    fn classify_returns_expected_tier_at_known_values() {
        assert_eq!(DensityTier::classify(0), DensityTier::Sparse);
        assert_eq!(DensityTier::classify(50), DensityTier::Sparse);
        assert_eq!(DensityTier::classify(80), DensityTier::Sparse);
        assert_eq!(DensityTier::classify(81), DensityTier::Comfortable);
        assert_eq!(DensityTier::classify(180), DensityTier::Comfortable);
        assert_eq!(DensityTier::classify(181), DensityTier::Dense);
        assert_eq!(DensityTier::classify(400), DensityTier::Dense);
        assert_eq!(DensityTier::classify(401), DensityTier::Extreme);
        assert_eq!(DensityTier::classify(10_000), DensityTier::Extreme);
    }

    #[test]
    fn ord_matches_density_intuition() {
        // Sparse < Comfortable < Dense < Extreme.
        assert!(DensityTier::Sparse < DensityTier::Comfortable);
        assert!(DensityTier::Comfortable < DensityTier::Dense);
        assert!(DensityTier::Dense < DensityTier::Extreme);
    }

    #[test]
    fn serde_round_trip_via_snake_case() {
        for (tier, expected_json) in [
            (DensityTier::Sparse, "\"sparse\""),
            (DensityTier::Comfortable, "\"comfortable\""),
            (DensityTier::Dense, "\"dense\""),
            (DensityTier::Extreme, "\"extreme\""),
        ] {
            let j = serde_json::to_string(&tier).unwrap();
            assert_eq!(j, expected_json);
            let back: DensityTier = serde_json::from_str(&j).unwrap();
            assert_eq!(back, tier);
        }
    }
}
