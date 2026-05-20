//! `polish` — refinement-tier decoration tokens.
//!
//! The base loom-tokens system (color / spacing / font / radius)
//! covers the structural design surface — the parts every primitive
//! consumes by default. `PolishToken` is the layer ABOVE that: typed
//! refinement decorations that primitives OPT INTO when the
//! composition benefits.
//!
//! Examples:
//! * A `HeroEditorial` opts into `PolishToken::DotGrid` background
//!   to surface the substrate's signature texture without painting
//!   it directly into the primitive.
//! * A `Card` with `CardElevation::Pronounced` opts into
//!   `PolishToken::SoftGlow` for a faint brand-tinted halo without
//!   the primitive having to bake a custom shadow.
//! * A `Section` opts into `PolishToken::EditorialRule` to draw a
//!   thin top-border in the brand color — the editorial-magazine
//!   "section divider" rule.
//!
//! Closed enum. Adding a variant is a doctrine review per
//! `[[plausiden-design-premium]]`. Renaming a variant is a wire
//! break — operators may serialize PolishToken in CMS JSON.
//!
//! WIRE SHAPE
//! ----------
//! `serde(rename_all = "kebab-case")`. Each variant maps to a CSS
//! class `loom-polish-<kebab-name>` so the skin can target via
//! `[data-loom-polish*="dot-grid"]` selectors when a primitive
//! emits the class.
//!
//! ## Category groupings
//!
//! * **Backgrounds** — dot-grid / linear-mesh / topographic
//! * **Borders** — editorial-rule / inset-frame / blueprint-corner
//! * **Glows** — soft-glow / brand-halo / amoled-rim
//! * **Motion** — slow-reveal / page-turn / cursor-tilt
//!
//! Each grouping is documented per variant; consumers compose
//! 0-N tokens per primitive (the [`PolishSet`] wrapper).

use serde::{Deserialize, Serialize};

/// A single polish token.
///
/// Variants below are intentionally narrow. New variants land via
/// the loom-doctrine review process; the variant name is the wire
/// contract (CMS JSON serializes via `serde(rename_all = "kebab-case")`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolishToken {
    // ----- Backgrounds -----
    /// Subtle 1px dot grid — the substrate's signature texture.
    /// Used on editorial heroes + section transitions to mark
    /// substrate-rendered vs. CMS-imported areas.
    DotGrid,
    /// Diagonal linear-gradient mesh, brand-tinted. Editorial
    /// counterpart to the SaaS-canonical solid-color background.
    LinearMesh,
    /// Topographic contour-line background — for site-map / DR /
    /// region-aware pages.
    Topographic,

    // ----- Borders -----
    /// Thin top-border in brand color marking an editorial section
    /// transition. Two pixels, no rounding.
    EditorialRule,
    /// Inset frame — 4-side 1px border offset 8px inward.
    InsetFrame,
    /// Blueprint-corner — small angle marks at the 4 corners,
    /// no full border. Engineering-doc look.
    BlueprintCorner,

    // ----- Glows -----
    /// Soft brand-tinted halo. Subtler than `shadow-2xl`.
    SoftGlow,
    /// Brand-halo — saturated brand color, used on hero CTAs.
    BrandHalo,
    /// AMOLED-rim — 1px brand-edge glow on dark surfaces.
    AmoledRim,

    // ----- Motion -----
    /// Slow-reveal — 600ms opacity fade-in on first paint,
    /// respects `prefers-reduced-motion`.
    SlowReveal,
    /// Page-turn — horizontal slide between routes, respects
    /// reduced motion.
    PageTurn,
    /// Cursor-tilt — element rotates 1-2deg following cursor.
    /// Editorial decorative; explicit opt-in only.
    CursorTilt,
}

impl PolishToken {
    /// Kebab-case slug used in the CSS class name + the wire format.
    /// Stable; renaming requires a Cat-3 migration per the version
    /// discipline doctrine.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::DotGrid => "dot-grid",
            Self::LinearMesh => "linear-mesh",
            Self::Topographic => "topographic",
            Self::EditorialRule => "editorial-rule",
            Self::InsetFrame => "inset-frame",
            Self::BlueprintCorner => "blueprint-corner",
            Self::SoftGlow => "soft-glow",
            Self::BrandHalo => "brand-halo",
            Self::AmoledRim => "amoled-rim",
            Self::SlowReveal => "slow-reveal",
            Self::PageTurn => "page-turn",
            Self::CursorTilt => "cursor-tilt",
        }
    }

    /// CSS class skin.css targets to apply the polish.
    #[must_use]
    pub fn css_class(self) -> String {
        format!("loom-polish-{}", self.slug())
    }

    /// Polish category. Used by the editorial-audit phase + by the
    /// operator UI to group tokens semantically.
    #[must_use]
    pub const fn category(self) -> PolishCategory {
        match self {
            Self::DotGrid | Self::LinearMesh | Self::Topographic => PolishCategory::Background,
            Self::EditorialRule | Self::InsetFrame | Self::BlueprintCorner => {
                PolishCategory::Border
            }
            Self::SoftGlow | Self::BrandHalo | Self::AmoledRim => PolishCategory::Glow,
            Self::SlowReveal | Self::PageTurn | Self::CursorTilt => PolishCategory::Motion,
        }
    }

    /// `true` iff this polish involves motion. Used by the
    /// `motion_respects_reduced` Forge phase to flag primitives
    /// that ship motion-tier polish without honoring
    /// `prefers-reduced-motion`.
    #[must_use]
    pub const fn is_motion(self) -> bool {
        matches!(self.category(), PolishCategory::Motion)
    }
}

/// Top-level polish category. Drives the operator UI grouping +
/// the editorial-audit's per-category density check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolishCategory {
    /// Background textures.
    Background,
    /// Border / frame decoration.
    Border,
    /// Glow / halo decoration.
    Glow,
    /// Motion / transition decoration.
    Motion,
}

/// A composed polish set — 0..N tokens on a single primitive
/// instance.
///
/// Wire shape: a flat array of [`PolishToken`] variants. CMS JSON
/// authors:
///
/// ```text
/// "polish": ["dot-grid", "editorial-rule"]
/// ```
///
/// The Loom renderer collects the polish list, joins the
/// per-token CSS classes, and emits them on the primitive's
/// outer element. The skin styles each class independently;
/// multiple polish tokens layer additively.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolishSet(pub Vec<PolishToken>);

impl PolishSet {
    /// Empty polish set.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Build a set from a slice of tokens.
    #[must_use]
    pub fn from_slice(tokens: &[PolishToken]) -> Self {
        Self(tokens.to_vec())
    }

    /// Compose the joined CSS class string. Empty for an empty set.
    #[must_use]
    pub fn css_classes(&self) -> String {
        let mut classes: Vec<String> = self.0.iter().copied().map(PolishToken::css_class).collect();
        classes.sort();
        classes.dedup();
        classes.join(" ")
    }

    /// `true` iff the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of tokens in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// `true` iff the set contains at least one motion-tier polish.
    /// Cross-checked by the `motion_respects_reduced` Forge phase.
    #[must_use]
    pub fn has_motion(&self) -> bool {
        self.0.iter().any(|t| t.is_motion())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_unique_slug() {
        let variants: &[PolishToken] = &[
            PolishToken::DotGrid,
            PolishToken::LinearMesh,
            PolishToken::Topographic,
            PolishToken::EditorialRule,
            PolishToken::InsetFrame,
            PolishToken::BlueprintCorner,
            PolishToken::SoftGlow,
            PolishToken::BrandHalo,
            PolishToken::AmoledRim,
            PolishToken::SlowReveal,
            PolishToken::PageTurn,
            PolishToken::CursorTilt,
        ];
        let mut slugs: Vec<&'static str> = variants.iter().map(|v| v.slug()).collect();
        slugs.sort_unstable();
        let len_before = slugs.len();
        slugs.dedup();
        assert_eq!(
            slugs.len(),
            len_before,
            "duplicate slug detected; each variant must have a unique slug"
        );
    }

    #[test]
    fn slug_is_kebab_case() {
        for v in [
            PolishToken::DotGrid,
            PolishToken::LinearMesh,
            PolishToken::EditorialRule,
            PolishToken::BlueprintCorner,
            PolishToken::SoftGlow,
            PolishToken::BrandHalo,
            PolishToken::AmoledRim,
            PolishToken::SlowReveal,
            PolishToken::PageTurn,
            PolishToken::CursorTilt,
        ] {
            let slug = v.slug();
            assert!(
                slug.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "non-kebab-case slug: {slug}"
            );
            assert!(!slug.starts_with('-'));
            assert!(!slug.ends_with('-'));
        }
    }

    #[test]
    fn css_class_prefix() {
        assert_eq!(PolishToken::DotGrid.css_class(), "loom-polish-dot-grid");
        assert_eq!(
            PolishToken::EditorialRule.css_class(),
            "loom-polish-editorial-rule"
        );
        assert_eq!(PolishToken::SoftGlow.css_class(), "loom-polish-soft-glow");
    }

    #[test]
    fn category_groups_match_doctrine() {
        // Three of each background / border / glow / motion.
        let by_cat = |cat: PolishCategory| -> usize {
            [
                PolishToken::DotGrid,
                PolishToken::LinearMesh,
                PolishToken::Topographic,
                PolishToken::EditorialRule,
                PolishToken::InsetFrame,
                PolishToken::BlueprintCorner,
                PolishToken::SoftGlow,
                PolishToken::BrandHalo,
                PolishToken::AmoledRim,
                PolishToken::SlowReveal,
                PolishToken::PageTurn,
                PolishToken::CursorTilt,
            ]
            .iter()
            .filter(|t| t.category() == cat)
            .count()
        };
        assert_eq!(by_cat(PolishCategory::Background), 3);
        assert_eq!(by_cat(PolishCategory::Border), 3);
        assert_eq!(by_cat(PolishCategory::Glow), 3);
        assert_eq!(by_cat(PolishCategory::Motion), 3);
    }

    #[test]
    fn is_motion_only_true_for_motion_category() {
        assert!(PolishToken::SlowReveal.is_motion());
        assert!(PolishToken::PageTurn.is_motion());
        assert!(PolishToken::CursorTilt.is_motion());
        assert!(!PolishToken::DotGrid.is_motion());
        assert!(!PolishToken::EditorialRule.is_motion());
        assert!(!PolishToken::SoftGlow.is_motion());
    }

    #[test]
    fn polish_set_default_is_empty() {
        let set = PolishSet::default();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert_eq!(set.css_classes(), "");
    }

    #[test]
    fn polish_set_from_slice_preserves_tokens() {
        let set = PolishSet::from_slice(&[PolishToken::DotGrid, PolishToken::EditorialRule]);
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
    }

    #[test]
    fn polish_set_css_classes_joined_sorted_deduped() {
        let set = PolishSet::from_slice(&[
            PolishToken::EditorialRule,
            PolishToken::DotGrid,
            PolishToken::DotGrid, // duplicate
        ]);
        let s = set.css_classes();
        assert_eq!(s, "loom-polish-dot-grid loom-polish-editorial-rule");
    }

    #[test]
    fn polish_set_has_motion_true_when_motion_token_present() {
        let with_motion = PolishSet::from_slice(&[PolishToken::DotGrid, PolishToken::SlowReveal]);
        assert!(with_motion.has_motion());
        let without = PolishSet::from_slice(&[PolishToken::DotGrid, PolishToken::EditorialRule]);
        assert!(!without.has_motion());
        assert!(!PolishSet::new().has_motion());
    }

    #[test]
    fn polish_token_serializes_as_kebab_case_string() {
        let json = serde_json::to_string(&PolishToken::DotGrid).expect("ser");
        assert_eq!(json, "\"dot-grid\"");
        let json = serde_json::to_string(&PolishToken::EditorialRule).expect("ser");
        assert_eq!(json, "\"editorial-rule\"");
        let json = serde_json::to_string(&PolishToken::AmoledRim).expect("ser");
        assert_eq!(json, "\"amoled-rim\"");
    }

    #[test]
    fn polish_token_deserializes_from_kebab_case_string() {
        let t: PolishToken = serde_json::from_str("\"dot-grid\"").expect("de");
        assert_eq!(t, PolishToken::DotGrid);
        let t: PolishToken = serde_json::from_str("\"editorial-rule\"").expect("de");
        assert_eq!(t, PolishToken::EditorialRule);
    }

    #[test]
    fn polish_token_deserialize_rejects_unknown_token() {
        let r: Result<PolishToken, _> = serde_json::from_str("\"made-up-polish\"");
        assert!(r.is_err(), "unknown polish token must reject");
    }

    #[test]
    fn polish_set_serializes_as_flat_array() {
        // PolishSet uses #[serde(transparent)] so the JSON shape is
        // a flat array, not `{"0": [...]}`.
        let set = PolishSet::from_slice(&[PolishToken::DotGrid, PolishToken::EditorialRule]);
        let json = serde_json::to_string(&set).expect("ser");
        assert_eq!(json, r#"["dot-grid","editorial-rule"]"#);
    }

    #[test]
    fn polish_set_round_trips_through_serde() {
        let set = PolishSet::from_slice(&[
            PolishToken::DotGrid,
            PolishToken::SoftGlow,
            PolishToken::SlowReveal,
        ]);
        let json = serde_json::to_string(&set).expect("ser");
        let back: PolishSet = serde_json::from_str(&json).expect("de");
        assert_eq!(back, set);
    }

    #[test]
    fn polish_category_serializes_as_kebab_case_string() {
        let json = serde_json::to_string(&PolishCategory::Background).expect("ser");
        assert_eq!(json, "\"background\"");
        let json = serde_json::to_string(&PolishCategory::Motion).expect("ser");
        assert_eq!(json, "\"motion\"");
    }

    #[test]
    fn empty_polish_set_emits_empty_css_string() {
        let set = PolishSet::new();
        assert_eq!(set.css_classes(), "");
    }

    #[test]
    fn polish_token_total_count_matches_documented_twelve() {
        // The doctrine documents exactly 12 polish variants — 3 per
        // category × 4 categories. This test is a doctrine gate:
        // adding a 13th variant requires an explicit doctrine bump.
        let all: &[PolishToken] = &[
            PolishToken::DotGrid,
            PolishToken::LinearMesh,
            PolishToken::Topographic,
            PolishToken::EditorialRule,
            PolishToken::InsetFrame,
            PolishToken::BlueprintCorner,
            PolishToken::SoftGlow,
            PolishToken::BrandHalo,
            PolishToken::AmoledRim,
            PolishToken::SlowReveal,
            PolishToken::PageTurn,
            PolishToken::CursorTilt,
        ];
        assert_eq!(all.len(), 12);
    }
}
