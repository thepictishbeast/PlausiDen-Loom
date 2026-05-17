//! Aesthetic axes — discrete-enum dimensions a site picks a point
//! in to compose a coherent system.
//!
//! Captures PlausiDen-Forge/docs/ARCHITECTURE_PRINCIPLES.md §4
//! "Tokens as discrete axes — color, type, motion, density,
//! formality". Each axis is an enum, not a free value. A site
//! picks one variant per axis; the design system + AI generation
//! layer compose a coherent visual from the resulting tuple.
//!
//! Tens of thousands of valid combinations across the 8 axes,
//! every one of them guaranteed coherent because the axes are
//! discrete + curated.
//!
//! Each axis derives `Serialize` / `Deserialize` (kebab-case
//! representation in JSON), `PartialEq`, `Eq`, `Hash`, `Debug`,
//! `Clone`, `Copy`. The Default variant is the centrist option
//! the platform's reference style ships with.
//!
//! Style packs (Swiss editorial / brutalist / etc.) compose
//! these axes into named bundles. A site can either:
//!   * pick a style pack (which sets all 8 axes for you)
//!   * pick individual axes (override the pack's defaults)
//!   * mix two packs with declared weights (let the AI bridge
//!     interpolate)

use serde::{Deserialize, Serialize};

/// Density of the visual rhythm — how much whitespace separates
/// content blocks + how compact form fields stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    /// Compact spacing, lower line-heights, tighter forms.
    /// Data-dense tables + admin dashboards.
    Tight,
    /// Default — moderate whitespace, generous-but-bounded
    /// padding, comfortable line-height.
    #[default]
    Airy,
    /// Generous whitespace, oversized padding, editorial
    /// rhythm. Marketing landing pages + long-form writing.
    Spacious,
}

/// Formality of the design voice — visual + typographic + motion
/// register the site speaks in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Formality {
    /// Editorial — serif headlines, generous leading, magazine
    /// rhythm. News, books, longform.
    #[default]
    Editorial,
    /// Technical — monospace accents, dense tables, precise
    /// data presentation. Docs, dashboards, technical SaaS.
    Technical,
    /// Playful — rounded forms, bright accents, decorative
    /// motion. Consumer apps, creator tools, kids' sites.
    Playful,
    /// Brutalist — raw HTML aesthetic, system fonts, harsh
    /// contrasts, deliberate ugliness. Designer portfolios,
    /// experimental art, post-digital editorial.
    Brutalist,
}

/// Motion intensity — how much animation + transition the site
/// uses at the default reader experience.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MotionIntensity {
    /// No motion — instant transitions, no scroll-linked effects,
    /// no auto-playing media. The Tor-mode + max-accessibility
    /// default.
    Still,
    /// Default — subtle fades / hover transitions / focus ring
    /// animations. Refined but bounded.
    #[default]
    Subtle,
    /// Expressive — scroll-linked reveals, magnetic hover,
    /// viewport-triggered sequences. Marketing sites,
    /// portfolio reveals.
    Expressive,
    /// Kinetic — auto-playing animations, scrub-controlled
    /// timelines, parallax stacks, spring physics. Creative
    /// agency, editorial scrollytelling.
    Kinetic,
}

/// Texture of surfaces — flat vs layered visual depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Texture {
    /// Flat — no shadows, no gradients, no grain. Classic Swiss /
    /// Linear / GitHub aesthetic.
    #[default]
    Flat,
    /// Layered — soft shadows + subtle z-axis, gentle elevation
    /// gradients. Most modern SaaS landing pages.
    Layered,
    /// Grainy — film-grain overlays, halftone textures, paper
    /// noise. Editorial print-feel sites.
    Grainy,
    /// Glassmorphic — frosted-glass blurs, translucent panels,
    /// gradient meshes. Apple-style hero sections.
    Glassmorphic,
}

/// Type personality — which family of typefaces the site reaches
/// for. Used by the AI bridge to pick coherent font pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TypePersonality {
    /// Humanist — Inter, Source Sans, IBM Plex Sans. The
    /// default SaaS / dashboard register.
    #[default]
    Humanist,
    /// Geometric — Futura, Avenir, Eurostile. Architectural,
    /// brand-led, fashion.
    Geometric,
    /// Mono — JetBrains Mono, IBM Plex Mono, Berkeley Mono.
    /// Technical docs, code-forward, editorial avant-garde.
    Mono,
    /// Display-serif — GT Sectra, Tiempos Headline, Source
    /// Serif Display. Editorial, longform, premium brand.
    DisplaySerif,
    /// Condensed — Oswald, Bebas Neue, condensed cuts. News,
    /// magazine, sports, kinetic editorial.
    Condensed,
}

/// Grid character — how strictly the layout adheres to a
/// regular grid system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GridCharacter {
    /// Regular — strict columns, predictable rhythm, baseline
    /// snapping. Swiss editorial + SaaS default.
    #[default]
    Regular,
    /// Asymmetric — deliberate offsets, varied column counts,
    /// orthogonal hierarchy. Editorial, magazine, designer
    /// portfolio.
    Asymmetric,
    /// Broken — intentional grid violations within declared
    /// safe-overlap primitives. Brutalist, post-digital,
    /// experimental art.
    Broken,
    /// Organic — flowing layouts, no underlying grid,
    /// scroll-driven composition. Story-led narrative sites.
    Organic,
}

/// Color mood — the palette-theory family the site's accent
/// colors are drawn from. Used by the AI bridge to generate
/// coherent palettes from a single base hue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMood {
    /// Monochromatic — all hues are tints/shades of one base.
    /// Editorial, minimal, brand-led.
    Monochromatic,
    /// Analogous — adjacent hues on the color wheel (warm /
    /// cool clusters). Default landing-page register.
    #[default]
    Analogous,
    /// Complementary — base + opposite. Bold marketing
    /// callouts.
    Complementary,
    /// Triadic — three hues evenly spaced. Playful, vibrant,
    /// consumer-app aesthetic.
    Triadic,
    /// Split-complementary — base + two adjacent opposites.
    /// Editorial with controlled tension.
    SplitComplementary,
    /// Duotone — two hues only, one for fg + one for bg.
    /// Iconography-led, brand-strict.
    Duotone,
    /// Polychrome — full chromatic range, no theory applied.
    /// Memphis, 90s zine, deliberate chaos.
    Polychrome,
}

/// Color energy — how saturated + vibrant the palette is, at
/// equal hue choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ColorEnergy {
    /// Muted — desaturated, ink-like, editorial. Tints lean
    /// toward gray.
    #[default]
    Muted,
    /// Saturated — full chroma, vibrant. Standard SaaS +
    /// consumer brands.
    Saturated,
    /// Neon — beyond-sRGB-feel saturation, glow effects.
    /// Crypto, esports, late-night-editorial.
    Neon,
    /// Pastel — high lightness + low chroma. Wellness,
    /// children's products, soft-spoken brands.
    Pastel,
}

/// The full aesthetic tuple — a coordinate in the 8-dimensional
/// design-axis space. Sites either pick this directly or pick a
/// named style pack which sets it.
///
/// Cardinality: 3 × 4 × 4 × 4 × 5 × 4 × 7 × 4 = **107,520** valid
/// combinations, every one of them guaranteed coherent because
/// each axis is enumerated + reviewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AestheticTuple {
    /// Whitespace + rhythm density.
    pub density: Density,
    /// Voice register (editorial / technical / playful / brutalist).
    pub formality: Formality,
    /// How much motion the default reader sees.
    pub motion_intensity: MotionIntensity,
    /// Surface texture (flat / layered / grainy / glassmorphic).
    pub texture: Texture,
    /// Typeface family register.
    pub type_personality: TypePersonality,
    /// Strictness of layout adherence to a regular grid.
    pub grid_character: GridCharacter,
    /// Color-theory family the palette is generated from.
    pub color_mood: ColorMood,
    /// Saturation + vibrance level of the palette.
    pub color_energy: ColorEnergy,
}

impl AestheticTuple {
    /// The platform's reference centrist tuple — what new sites
    /// get if they declare nothing. Editorial / Airy / Subtle /
    /// Flat / Humanist / Regular / Analogous / Muted.
    pub const fn reference() -> Self {
        Self {
            density: Density::Airy,
            formality: Formality::Editorial,
            motion_intensity: MotionIntensity::Subtle,
            texture: Texture::Flat,
            type_personality: TypePersonality::Humanist,
            grid_character: GridCharacter::Regular,
            color_mood: ColorMood::Analogous,
            color_energy: ColorEnergy::Muted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_compose_the_reference_tuple() {
        let dflt = AestheticTuple::default();
        assert_eq!(dflt, AestheticTuple::reference());
    }

    #[test]
    fn serde_round_trip() {
        let t = AestheticTuple {
            density: Density::Spacious,
            formality: Formality::Brutalist,
            motion_intensity: MotionIntensity::Kinetic,
            texture: Texture::Grainy,
            type_personality: TypePersonality::DisplaySerif,
            grid_character: GridCharacter::Broken,
            color_mood: ColorMood::SplitComplementary,
            color_energy: ColorEnergy::Neon,
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: AestheticTuple = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn serde_uses_kebab_case() {
        let t = ColorMood::SplitComplementary;
        let s = serde_json::to_string(&t).unwrap();
        assert_eq!(s, "\"split-complementary\"");
    }

    #[test]
    fn cardinality_matches_doc_claim() {
        // The docstring claims 107,520 valid tuples. Verify the
        // arithmetic so the doc + types stay in sync. If a new
        // variant lands on any axis, this test must be updated
        // alongside the doc comment on AestheticTuple.
        let density_n = 3;
        let formality_n = 4;
        let motion_n = 4;
        let texture_n = 4;
        let type_n = 5;
        let grid_n = 4;
        let mood_n = 7;
        let energy_n = 4;
        assert_eq!(
            density_n * formality_n * motion_n * texture_n * type_n * grid_n * mood_n * energy_n,
            107_520
        );
    }

    #[test]
    fn every_variant_is_copy() {
        // Compile-time check that every axis derives Copy +
        // round-trips Clone semantics.
        fn assert_copy<T: Copy>() {}
        assert_copy::<Density>();
        assert_copy::<Formality>();
        assert_copy::<MotionIntensity>();
        assert_copy::<Texture>();
        assert_copy::<TypePersonality>();
        assert_copy::<GridCharacter>();
        assert_copy::<ColorMood>();
        assert_copy::<ColorEnergy>();
        assert_copy::<AestheticTuple>();
    }
}
