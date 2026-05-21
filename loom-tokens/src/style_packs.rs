//! Style packs — named bundles of `AestheticTuple` values + the
//! generative grammar they imply.
//!
//! Per PlausiDen-Forge/docs/ARCHITECTURE_PRINCIPLES.md §5
//! "Style packs as first-class artifacts". A style pack is a
//! coherent bundle of token settings + primitive preferences +
//! motion language + asset treatments + generative grammar.
//! Curated by human designers, stored as data, signed, versioned.
//!
//! The AI bridge picks a pack (or blends two with declared
//! weights) rather than inventing aesthetics from scratch — this
//! is how you get visual range without slop. Inventiveness is
//! front-loaded into curation; runtime composition is bounded.
//!
//! The packs here are deliberately curated reference points
//! across the established design-history vocabulary:
//!
//! - **Swiss editorial** — strict baseline grid, two type sizes,
//!   asymmetric layouts, generous whitespace, no shadows. The
//!   Bauhaus/Müller-Brockmann/Vignelli tradition continued.
//! - **90s zine** — broken grids, mixed scripts, deliberate
//!   chaos, polychrome saturation. Riot Grrrl, post-punk
//!   editorial.
//! - **Y2K chrome** — glassmorphic surfaces, neon saturation,
//!   geometric type, technical formality. iMac-G3 / Matrix
//!   aesthetic.
//! - **Bauhaus poster** — geometric type, primary palette,
//!   strict regular grid, layered surfaces. Foundational
//!   modernist register.
//! - **Brutalist** — system fonts, harsh contrast, broken
//!   grids, raw HTML aesthetic. Designer portfolios, post-
//!   digital editorial.
//! - **Memphis revival** — playful formality, triadic mood,
//!   pastel energy, asymmetric organic layouts. 80s revival,
//!   creator-economy.
//! - **Japanese minimal** — display-serif typography,
//!   monochromatic palette, spacious density, regular grid.
//!   Wabi-sabi minimalism.
//! - **Dieter Rams** — flat surfaces, muted color, humanist
//!   type, technical formality, airy density. Reference
//!   industrial-design aesthetic continued.
//!
//! Adding a pack:
//! 1. Add a variant to `StylePack`.
//! 2. Implement the `aesthetic` method case for it.
//! 3. Add rustdoc covering the historical reference + intent.
//! 4. Update the test asserting per-pack tuple invariants.
//! 5. (Future) provide an example site at `examples/<pack>/`
//!    so the AI bridge has a concrete reference to retrieve
//!    against.

use serde::{Deserialize, Serialize};

use crate::axes::{
    AestheticTuple, ColorEnergy, ColorMood, Density, Formality, GridCharacter, MotionIntensity,
    Texture, TypePersonality,
};

/// A named, curated bundle of aesthetic-axis values + generative
/// grammar. Sites declare ONE pack (or blend two) instead of
/// declaring 8 individual axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StylePack {
    /// Swiss editorial — Müller-Brockmann / Vignelli /
    /// magazine-grid tradition. Asymmetric within a strict
    /// baseline grid; two type sizes; flat surfaces; generous
    /// whitespace; no shadows.
    SwissEditorial,
    /// 90s zine — Riot Grrrl / post-punk editorial. Broken
    /// grids, mixed scripts, polychrome saturation, deliberate
    /// kinetic energy.
    NinetiesZine,
    /// Y2K chrome — iMac-G3 / Matrix-poster aesthetic.
    /// Glassmorphic surfaces, neon saturation, geometric type,
    /// technical voice.
    Y2kChrome,
    /// Bauhaus poster — foundational modernism. Geometric type,
    /// primary triadic palette, strict regular grid, layered
    /// surfaces.
    BauhausPoster,
    /// Brutalist — system fonts, harsh contrast, broken grids,
    /// raw HTML aesthetic. Post-digital editorial, designer
    /// portfolios.
    Brutalist,
    /// Memphis revival — 80s-Memphis-Group continuation.
    /// Playful formality, triadic mood, pastel energy,
    /// asymmetric-organic grid.
    MemphisRevival,
    /// Japanese minimal — wabi-sabi register. Display-serif
    /// type, monochromatic palette, spacious density, regular
    /// grid.
    JapaneseMinimal,
    /// Dieter Rams — Braun-industrial-design tradition. Flat
    /// surfaces, muted color, humanist type, technical
    /// formality, airy density.
    DieterRams,
    /// Newspaper editorial — broadsheet / The New Yorker
    /// register. Display-serif type, columnar dense layout,
    /// monochromatic palette, regular grid, still motion.
    NewspaperEditorial,
    /// Cyberpunk neon — dark base + neon accents + glitch
    /// register. Mono type, polychrome, kinetic motion,
    /// glassmorphic texture, asymmetric grid.
    CyberpunkNeon,
    /// Pastel soft — Notion / Linear register. Rounded
    /// surfaces, pastel triadic palette, humanist type,
    /// subtle motion, airy density.
    PastelSoft,
    /// Technical documentation — operations / engineering
    /// register. Mono headings, technical formality, tight
    /// regular grids, monochromatic muted palette, still
    /// motion.
    TechnicalDocumentation,
}

impl StylePack {
    /// All shipped packs in declaration order. Useful for
    /// iteration / `loom site templates --list-packs` /
    /// AI-bridge corpus indexing.
    pub const ALL: &'static [Self] = &[
        Self::SwissEditorial,
        Self::NinetiesZine,
        Self::Y2kChrome,
        Self::BauhausPoster,
        Self::Brutalist,
        Self::MemphisRevival,
        Self::JapaneseMinimal,
        Self::DieterRams,
        Self::NewspaperEditorial,
        Self::CyberpunkNeon,
        Self::PastelSoft,
        Self::TechnicalDocumentation,
    ];

    /// Kebab-case identifier (the serde representation).
    pub fn slug(self) -> &'static str {
        match self {
            Self::SwissEditorial => "swiss-editorial",
            Self::NinetiesZine => "nineties-zine",
            Self::Y2kChrome => "y2k-chrome",
            Self::BauhausPoster => "bauhaus-poster",
            Self::Brutalist => "brutalist",
            Self::MemphisRevival => "memphis-revival",
            Self::JapaneseMinimal => "japanese-minimal",
            Self::DieterRams => "dieter-rams",
            Self::NewspaperEditorial => "newspaper-editorial",
            Self::CyberpunkNeon => "cyberpunk-neon",
            Self::PastelSoft => "pastel-soft",
            Self::TechnicalDocumentation => "technical-documentation",
        }
    }

    /// The 8-axis aesthetic tuple this pack sets.
    pub fn aesthetic(self) -> AestheticTuple {
        match self {
            Self::SwissEditorial => AestheticTuple {
                density: Density::Spacious,
                formality: Formality::Editorial,
                motion_intensity: MotionIntensity::Subtle,
                texture: Texture::Flat,
                type_personality: TypePersonality::Humanist,
                grid_character: GridCharacter::Asymmetric,
                color_mood: ColorMood::Monochromatic,
                color_energy: ColorEnergy::Muted,
            },
            Self::NinetiesZine => AestheticTuple {
                density: Density::Tight,
                formality: Formality::Brutalist,
                motion_intensity: MotionIntensity::Kinetic,
                texture: Texture::Grainy,
                type_personality: TypePersonality::Condensed,
                grid_character: GridCharacter::Broken,
                color_mood: ColorMood::Polychrome,
                color_energy: ColorEnergy::Saturated,
            },
            Self::Y2kChrome => AestheticTuple {
                density: Density::Airy,
                formality: Formality::Technical,
                motion_intensity: MotionIntensity::Expressive,
                texture: Texture::Glassmorphic,
                type_personality: TypePersonality::Geometric,
                grid_character: GridCharacter::Regular,
                color_mood: ColorMood::Complementary,
                color_energy: ColorEnergy::Neon,
            },
            Self::BauhausPoster => AestheticTuple {
                density: Density::Spacious,
                formality: Formality::Editorial,
                motion_intensity: MotionIntensity::Still,
                texture: Texture::Layered,
                type_personality: TypePersonality::Geometric,
                grid_character: GridCharacter::Regular,
                color_mood: ColorMood::Triadic,
                color_energy: ColorEnergy::Saturated,
            },
            Self::Brutalist => AestheticTuple {
                density: Density::Tight,
                formality: Formality::Brutalist,
                motion_intensity: MotionIntensity::Still,
                texture: Texture::Flat,
                type_personality: TypePersonality::Mono,
                grid_character: GridCharacter::Broken,
                color_mood: ColorMood::Duotone,
                color_energy: ColorEnergy::Muted,
            },
            Self::MemphisRevival => AestheticTuple {
                density: Density::Airy,
                formality: Formality::Playful,
                motion_intensity: MotionIntensity::Expressive,
                texture: Texture::Layered,
                type_personality: TypePersonality::Geometric,
                grid_character: GridCharacter::Asymmetric,
                color_mood: ColorMood::Triadic,
                color_energy: ColorEnergy::Pastel,
            },
            Self::JapaneseMinimal => AestheticTuple {
                density: Density::Spacious,
                formality: Formality::Editorial,
                motion_intensity: MotionIntensity::Subtle,
                texture: Texture::Flat,
                type_personality: TypePersonality::DisplaySerif,
                grid_character: GridCharacter::Regular,
                color_mood: ColorMood::Monochromatic,
                color_energy: ColorEnergy::Muted,
            },
            Self::DieterRams => AestheticTuple {
                density: Density::Airy,
                formality: Formality::Technical,
                motion_intensity: MotionIntensity::Subtle,
                texture: Texture::Flat,
                type_personality: TypePersonality::Humanist,
                grid_character: GridCharacter::Regular,
                color_mood: ColorMood::Monochromatic,
                color_energy: ColorEnergy::Muted,
            },
            Self::NewspaperEditorial => AestheticTuple {
                density: Density::Tight,
                formality: Formality::Editorial,
                motion_intensity: MotionIntensity::Still,
                texture: Texture::Flat,
                type_personality: TypePersonality::DisplaySerif,
                grid_character: GridCharacter::Regular,
                color_mood: ColorMood::Monochromatic,
                color_energy: ColorEnergy::Muted,
            },
            Self::CyberpunkNeon => AestheticTuple {
                density: Density::Tight,
                formality: Formality::Technical,
                motion_intensity: MotionIntensity::Kinetic,
                texture: Texture::Glassmorphic,
                type_personality: TypePersonality::Mono,
                grid_character: GridCharacter::Asymmetric,
                color_mood: ColorMood::Polychrome,
                color_energy: ColorEnergy::Neon,
            },
            Self::PastelSoft => AestheticTuple {
                density: Density::Airy,
                formality: Formality::Playful,
                motion_intensity: MotionIntensity::Subtle,
                texture: Texture::Layered,
                type_personality: TypePersonality::Humanist,
                grid_character: GridCharacter::Regular,
                color_mood: ColorMood::Triadic,
                color_energy: ColorEnergy::Pastel,
            },
            Self::TechnicalDocumentation => AestheticTuple {
                density: Density::Tight,
                formality: Formality::Technical,
                motion_intensity: MotionIntensity::Still,
                texture: Texture::Flat,
                type_personality: TypePersonality::Mono,
                grid_character: GridCharacter::Regular,
                color_mood: ColorMood::Monochromatic,
                color_energy: ColorEnergy::Muted,
            },
        }
    }

    /// Human-readable label for UIs (loom site templates output,
    /// editor pack-picker dropdown, etc.).
    pub fn label(self) -> &'static str {
        match self {
            Self::SwissEditorial => "Swiss editorial",
            Self::NinetiesZine => "90s zine",
            Self::Y2kChrome => "Y2K chrome",
            Self::BauhausPoster => "Bauhaus poster",
            Self::Brutalist => "Brutalist",
            Self::MemphisRevival => "Memphis revival",
            Self::JapaneseMinimal => "Japanese minimal",
            Self::DieterRams => "Dieter Rams",
            Self::NewspaperEditorial => "Newspaper editorial",
            Self::CyberpunkNeon => "Cyberpunk neon",
            Self::PastelSoft => "Pastel soft",
            Self::TechnicalDocumentation => "Technical documentation",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_packs_have_unique_slugs() {
        let slugs: HashSet<&'static str> = StylePack::ALL.iter().map(|p| p.slug()).collect();
        assert_eq!(slugs.len(), StylePack::ALL.len(), "slugs must be unique");
    }

    #[test]
    fn all_packs_have_unique_aesthetic_tuples() {
        let tuples: HashSet<AestheticTuple> =
            StylePack::ALL.iter().map(|p| p.aesthetic()).collect();
        assert_eq!(
            tuples.len(),
            StylePack::ALL.len(),
            "each pack must encode a distinct aesthetic — if two packs collide, one is redundant"
        );
    }

    #[test]
    fn serde_round_trip_via_slug() {
        for &pack in StylePack::ALL {
            let json = serde_json::to_string(&pack).unwrap();
            let back: StylePack = serde_json::from_str(&json).unwrap();
            assert_eq!(pack, back, "round-trip failed for {pack:?}");
            // serde value matches manual slug
            assert_eq!(json, format!("\"{}\"", pack.slug()));
        }
    }

    #[test]
    fn swiss_editorial_is_monochromatic_muted_humanist() {
        // Anchor test: if these invariants drift (e.g. someone
        // accidentally swaps Swiss to Polychrome), we'll catch
        // it before a designer review does.
        let t = StylePack::SwissEditorial.aesthetic();
        assert_eq!(t.color_mood, ColorMood::Monochromatic);
        assert_eq!(t.color_energy, ColorEnergy::Muted);
        assert_eq!(t.type_personality, TypePersonality::Humanist);
        assert_eq!(t.grid_character, GridCharacter::Asymmetric);
    }

    #[test]
    fn brutalist_is_mono_broken_duotone() {
        let t = StylePack::Brutalist.aesthetic();
        assert_eq!(t.type_personality, TypePersonality::Mono);
        assert_eq!(t.grid_character, GridCharacter::Broken);
        assert_eq!(t.color_mood, ColorMood::Duotone);
        assert_eq!(t.motion_intensity, MotionIntensity::Still);
    }

    #[test]
    fn memphis_is_playful_pastel_triadic() {
        let t = StylePack::MemphisRevival.aesthetic();
        assert_eq!(t.formality, Formality::Playful);
        assert_eq!(t.color_energy, ColorEnergy::Pastel);
        assert_eq!(t.color_mood, ColorMood::Triadic);
    }

    #[test]
    fn nineties_zine_is_kinetic_polychrome_grainy() {
        let t = StylePack::NinetiesZine.aesthetic();
        assert_eq!(t.motion_intensity, MotionIntensity::Kinetic);
        assert_eq!(t.color_mood, ColorMood::Polychrome);
        assert_eq!(t.texture, Texture::Grainy);
    }

    #[test]
    fn all_iterates_twelve_packs() {
        // If we ship a 13th pack, this test forces the doctrine
        // doc + tests to stay in sync.
        assert_eq!(StylePack::ALL.len(), 12);
    }

    #[test]
    fn label_is_distinct_per_pack() {
        let labels: HashSet<&'static str> = StylePack::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(labels.len(), StylePack::ALL.len());
    }

    #[test]
    fn dieter_rams_is_flat_humanist_muted_airy() {
        let t = StylePack::DieterRams.aesthetic();
        assert_eq!(t.texture, Texture::Flat);
        assert_eq!(t.type_personality, TypePersonality::Humanist);
        assert_eq!(t.color_energy, ColorEnergy::Muted);
        assert_eq!(t.density, Density::Airy);
    }
}
