//! Gradient pool — curated set of brand-grade gradient pairs with
//! identity-aware deterministic selection.
//!
//! Closes the "ugly gradient appears on every site" failure mode
//! named in `docs/SUBSTRATE_REFRAME_2026_05_21.md` (Forge fix 4 —
//! default fragmentation). When a tenant doesn't explicitly
//! specify a gradient, the substrate selects one from this pool
//! deterministically on site identity (slug + tenant). Two sites
//! that don't specify gradients land on different gradients
//! because the pool is broad and selection considers identity.
//!
//! ## Why a pool, not a single default
//!
//! A single canonical default is the path-of-least-resistance
//! for every site that doesn't override. The same default appears
//! everywhere; the substrate is structurally producing
//! convergence. A pool with deterministic identity-aware
//! selection turns defaults into a force *for* variation rather
//! than against it.
//!
//! ## Selection is deterministic
//!
//! Given the same `site_id` + `tenant`, [`select_for_identity`]
//! always returns the same gradient. This preserves reproducible
//! builds (same inputs → same output) while still producing
//! variation across sites with different identities.
//!
//! ## How to extend
//!
//! Add a new [`GradientPair`] to [`GRADIENT_POOL`]. Each pair
//! requires a `name` (kebab-case, stable wire identifier), the
//! two endpoint CSS colors, a direction in degrees, and a
//! [`GradientMood`] tag. The tag lets downstream selection
//! filter on aesthetic intent ("give me a warm gradient for an
//! editorial site"). Pool growth is design-led work — each new
//! pair should be reviewed for aesthetic quality and coherence
//! with existing pairs.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Aesthetic mood tag for a gradient pair. Lets downstream
/// selection filter on intent without enumerating every pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum GradientMood {
    /// Cool tones (blue, purple, teal). Default for tech /
    /// SaaS / modern aesthetics.
    Cool,
    /// Warm tones (orange, red, amber). Default for editorial /
    /// hospitality / consumer aesthetics.
    Warm,
    /// Monochromatic — same hue at different lightnesses.
    /// Default for editorial / publication aesthetics.
    Monochrome,
    /// Two contrasting hues. Default for vibrant / playful
    /// aesthetics.
    Duotone,
    /// Near-neutral tones. Default for minimal / brutalist /
    /// technical-dense aesthetics.
    Neutral,
    /// Photographic — emulates a sky / sunset / sea gradient.
    /// Default for atmospheric / lifestyle aesthetics.
    Photographic,
    /// Solid-no-gradient signal. When a tenant identity hashes
    /// to this, the substrate emits a solid color instead of a
    /// linear-gradient. Lets the pool include "no gradient at
    /// all" as a valid option.
    Solid,
}

/// One gradient pair entry in the curated pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GradientPair {
    /// Stable wire identifier (kebab-case). Used for telemetry +
    /// fingerprint registry entries so downstream phases can ask
    /// "what gradient did this site land on?" without parsing
    /// CSS.
    pub name: &'static str,
    /// First endpoint as a CSS color literal (hex / hsl / oklch).
    pub a: &'static str,
    /// Second endpoint as a CSS color literal.
    pub b: &'static str,
    /// Direction in degrees (0 = top to bottom; 90 = left to
    /// right; 135 = top-left to bottom-right).
    pub direction_deg: u16,
    /// Aesthetic mood tag.
    pub mood: GradientMood,
}

impl GradientPair {
    /// Emit the CSS `linear-gradient(...)` value string.
    #[must_use]
    pub fn to_css_value(&self) -> String {
        if matches!(self.mood, GradientMood::Solid) {
            self.a.to_owned()
        } else {
            format!(
                "linear-gradient({}deg, {}, {})",
                self.direction_deg, self.a, self.b
            )
        }
    }
}

/// Curated pool of brand-grade gradient pairs. Hand-tuned for
/// aesthetic coherence within each pair AND for breadth across
/// the pool. Pool size targets 20-30 per
/// `docs/SUBSTRATE_REFRAME_2026_05_21.md` § "default fragmentation";
/// initial commit ships 24.
///
/// Add new pairs as design-reviewed additions; pool growth is a
/// substrate-doctrine change, not a casual edit. Adding a pair
/// is free; removing or reordering breaks the identity-based
/// selection's stability across substrate versions and must be
/// treated as a substrate migration event.
pub const GRADIENT_POOL: &[GradientPair] = &[
    // Cool — modern / SaaS / tech aesthetics
    GradientPair {
        name: "cool-indigo-violet",
        a: "#4F46E5",
        b: "#7C3AED",
        direction_deg: 135,
        mood: GradientMood::Cool,
    },
    GradientPair {
        name: "cool-teal-cyan",
        a: "#0F766E",
        b: "#0891B2",
        direction_deg: 120,
        mood: GradientMood::Cool,
    },
    GradientPair {
        name: "cool-slate-blue",
        a: "#1E293B",
        b: "#1D4ED8",
        direction_deg: 110,
        mood: GradientMood::Cool,
    },
    GradientPair {
        name: "cool-aqua-mint",
        a: "#06B6D4",
        b: "#10B981",
        direction_deg: 100,
        mood: GradientMood::Cool,
    },
    // Warm — editorial / hospitality / consumer
    GradientPair {
        name: "warm-amber-rust",
        a: "#F59E0B",
        b: "#B91C1C",
        direction_deg: 135,
        mood: GradientMood::Warm,
    },
    GradientPair {
        name: "warm-peach-coral",
        a: "#FED7AA",
        b: "#EA580C",
        direction_deg: 120,
        mood: GradientMood::Warm,
    },
    GradientPair {
        name: "warm-rose-magenta",
        a: "#E11D48",
        b: "#A21CAF",
        direction_deg: 135,
        mood: GradientMood::Warm,
    },
    GradientPair {
        name: "warm-sand-terracotta",
        a: "#FBBF24",
        b: "#9A3412",
        direction_deg: 125,
        mood: GradientMood::Warm,
    },
    // Monochrome — editorial / publication
    GradientPair {
        name: "mono-slate-fade",
        a: "#0F172A",
        b: "#475569",
        direction_deg: 180,
        mood: GradientMood::Monochrome,
    },
    GradientPair {
        name: "mono-cream-fade",
        a: "#FAF7F2",
        b: "#D6CFC2",
        direction_deg: 180,
        mood: GradientMood::Monochrome,
    },
    GradientPair {
        name: "mono-indigo-fade",
        a: "#312E81",
        b: "#6366F1",
        direction_deg: 180,
        mood: GradientMood::Monochrome,
    },
    GradientPair {
        name: "mono-emerald-fade",
        a: "#064E3B",
        b: "#34D399",
        direction_deg: 180,
        mood: GradientMood::Monochrome,
    },
    // Duotone — vibrant / playful
    GradientPair {
        name: "duo-violet-pink",
        a: "#7C3AED",
        b: "#EC4899",
        direction_deg: 135,
        mood: GradientMood::Duotone,
    },
    GradientPair {
        name: "duo-emerald-yellow",
        a: "#059669",
        b: "#FCD34D",
        direction_deg: 135,
        mood: GradientMood::Duotone,
    },
    GradientPair {
        name: "duo-blue-orange",
        a: "#1D4ED8",
        b: "#F97316",
        direction_deg: 110,
        mood: GradientMood::Duotone,
    },
    GradientPair {
        name: "duo-cyan-magenta",
        a: "#06B6D4",
        b: "#D946EF",
        direction_deg: 135,
        mood: GradientMood::Duotone,
    },
    // Neutral — minimal / brutalist / technical-dense
    GradientPair {
        name: "neutral-stone-paper",
        a: "#F5F5F4",
        b: "#A8A29E",
        direction_deg: 180,
        mood: GradientMood::Neutral,
    },
    GradientPair {
        name: "neutral-graphite",
        a: "#27272A",
        b: "#52525B",
        direction_deg: 180,
        mood: GradientMood::Neutral,
    },
    GradientPair {
        name: "neutral-bone-ash",
        a: "#FAFAF9",
        b: "#78716C",
        direction_deg: 165,
        mood: GradientMood::Neutral,
    },
    // Photographic — atmospheric / lifestyle
    GradientPair {
        name: "photo-dawn",
        a: "#FB923C",
        b: "#7C3AED",
        direction_deg: 200,
        mood: GradientMood::Photographic,
    },
    GradientPair {
        name: "photo-dusk",
        a: "#1E1B4B",
        b: "#F472B6",
        direction_deg: 200,
        mood: GradientMood::Photographic,
    },
    GradientPair {
        name: "photo-ocean",
        a: "#0C4A6E",
        b: "#7DD3FC",
        direction_deg: 200,
        mood: GradientMood::Photographic,
    },
    GradientPair {
        name: "photo-forest",
        a: "#14532D",
        b: "#FACC15",
        direction_deg: 200,
        mood: GradientMood::Photographic,
    },
    // Solid — the "no gradient" option as a first-class choice
    GradientPair {
        name: "solid-ink",
        a: "#0F172A",
        b: "#0F172A",
        direction_deg: 0,
        mood: GradientMood::Solid,
    },
];

/// Deterministic identity-aware selection from the pool. Given
/// the same `site_id` + `tenant`, always returns the same pair.
/// Different identities map to different pairs across the pool's
/// breadth.
///
/// Selection is SHA-256-based to spread identities uniformly
/// across pool indices. The hash is folded to `u64` (taking the
/// first 8 bytes) and reduced modulo pool length.
///
/// `recently_used` is a hint set: if the selection lands on a
/// recently-used pair, the function walks forward through the
/// pool until it finds an unused pair, wrapping at the end. When
/// every pair has been used, it falls back to the unmodified
/// hashed selection (the recency constraint is best-effort, not
/// a hard refusal — a tenant with many sites must eventually
/// reuse SOMETHING in the pool).
#[must_use]
pub fn select_for_identity(
    site_id: &str,
    tenant: &str,
    recently_used: &[&str],
) -> &'static GradientPair {
    let mut hasher = Sha256::new();
    hasher.update(b"gradient-pool/v1\0");
    hasher.update(tenant.as_bytes());
    hasher.update(b"\0");
    hasher.update(site_id.as_bytes());
    let digest = hasher.finalize();
    let mut idx_bytes = [0u8; 8];
    idx_bytes.copy_from_slice(&digest[..8]);
    let hashed_idx = (u64::from_be_bytes(idx_bytes) as usize) % GRADIENT_POOL.len();

    if recently_used.is_empty() {
        return &GRADIENT_POOL[hashed_idx];
    }

    // Walk forward until we find an unused pair, wrapping at pool
    // length. If every pair is used, fall back to hashed_idx.
    for offset in 0..GRADIENT_POOL.len() {
        let candidate = &GRADIENT_POOL[(hashed_idx + offset) % GRADIENT_POOL.len()];
        if !recently_used.contains(&candidate.name) {
            return candidate;
        }
    }
    &GRADIENT_POOL[hashed_idx]
}

/// Filtered selection: same as [`select_for_identity`] but
/// restricts to pairs matching a specific mood. Used when the
/// tenant's identity declares a target mood (`editorial` →
/// `Monochrome`, `tech` → `Cool`, `lifestyle` → `Photographic`).
/// Falls back to the unrestricted selection if no pairs match
/// the requested mood (defensive — should never happen with the
/// current pool but protects against future mood additions that
/// could leave a mood empty).
#[must_use]
pub fn select_for_identity_mood(
    site_id: &str,
    tenant: &str,
    mood: GradientMood,
    recently_used: &[&str],
) -> &'static GradientPair {
    let filtered: Vec<&'static GradientPair> = GRADIENT_POOL
        .iter()
        .filter(|p| p.mood == mood)
        .collect();
    if filtered.is_empty() {
        return select_for_identity(site_id, tenant, recently_used);
    }
    let mut hasher = Sha256::new();
    hasher.update(b"gradient-pool-mood/v1\0");
    hasher.update(tenant.as_bytes());
    hasher.update(b"\0");
    hasher.update(site_id.as_bytes());
    let digest = hasher.finalize();
    let mut idx_bytes = [0u8; 8];
    idx_bytes.copy_from_slice(&digest[..8]);
    let hashed_idx = (u64::from_be_bytes(idx_bytes) as usize) % filtered.len();

    for offset in 0..filtered.len() {
        let candidate = filtered[(hashed_idx + offset) % filtered.len()];
        if !recently_used.contains(&candidate.name) {
            return candidate;
        }
    }
    filtered[hashed_idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_has_expected_breadth() {
        assert!(
            GRADIENT_POOL.len() >= 20,
            "pool should ship at least 20 pairs per substrate reframe spec"
        );
    }

    #[test]
    fn pool_names_are_unique() {
        let mut names: Vec<&str> = GRADIENT_POOL.iter().map(|p| p.name).collect();
        names.sort_unstable();
        let count_before = names.len();
        names.dedup();
        assert_eq!(
            names.len(),
            count_before,
            "pool names must be unique — identity-aware selection assumes stable names"
        );
    }

    #[test]
    fn pool_covers_every_mood_except_solid() {
        for mood in [
            GradientMood::Cool,
            GradientMood::Warm,
            GradientMood::Monochrome,
            GradientMood::Duotone,
            GradientMood::Neutral,
            GradientMood::Photographic,
        ] {
            assert!(
                GRADIENT_POOL.iter().any(|p| p.mood == mood),
                "pool missing mood: {mood:?}"
            );
        }
    }

    #[test]
    fn selection_is_deterministic() {
        let a = select_for_identity("alpha-site", "tenant-x", &[]);
        let b = select_for_identity("alpha-site", "tenant-x", &[]);
        assert_eq!(a.name, b.name);
    }

    #[test]
    fn different_identities_can_land_on_different_pairs() {
        // Probabilistic property: across a sample of 20 distinct
        // site IDs in the same tenant, we should see >1 distinct
        // pair selected. With 24 pool entries this is
        // overwhelmingly likely; the test pins the property.
        let picks: std::collections::BTreeSet<&str> = (0..20)
            .map(|i| {
                let site = format!("site-{i}");
                select_for_identity(&site, "tenant-x", &[]).name
            })
            .collect();
        assert!(
            picks.len() > 1,
            "20 distinct site ids should land on >1 gradient — pool selection is broken"
        );
    }

    #[test]
    fn recently_used_skipped_when_alternative_exists() {
        // First selection for an identity.
        let first = select_for_identity("repeat-site", "tenant-x", &[]).name;
        // Re-select with the first marked as recently used — must
        // pick something else (pool has 23 other options).
        let second = select_for_identity("repeat-site", "tenant-x", &[first]).name;
        assert_ne!(second, first);
    }

    #[test]
    fn mood_filter_restricts_to_requested_mood() {
        let pair = select_for_identity_mood("site-y", "tenant-x", GradientMood::Warm, &[]);
        assert_eq!(pair.mood, GradientMood::Warm);
    }

    #[test]
    fn css_value_emits_linear_gradient_for_non_solid() {
        let pair = &GRADIENT_POOL[0];
        let css = pair.to_css_value();
        assert!(css.starts_with("linear-gradient("));
        assert!(css.contains(pair.a));
        assert!(css.contains(pair.b));
    }

    #[test]
    fn css_value_emits_solid_for_solid_mood() {
        let solid = GRADIENT_POOL
            .iter()
            .find(|p| p.mood == GradientMood::Solid)
            .expect("pool must include a solid option");
        let css = solid.to_css_value();
        assert!(!css.starts_with("linear-gradient("));
        assert_eq!(css, solid.a);
    }
}
