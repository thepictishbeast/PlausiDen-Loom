//! `loom-assets` — typed asset registry for Forge sites.
//!
//! Open-source icons, emoji, photos, gifs, illustrations, and
//! CMS-JSON templates. Every entry is tagged + license-bearing +
//! source-traceable, so:
//!
//! - Clients can search ("show me all line-style arrow icons
//!   suitable for hero CTAs")
//! - Forge can auto-apply by context (a `card_feed` item with
//!   tag `tech` can default-pull a relevant icon)
//! - The supply-chain audit (`forge audit-log verify`) carries
//!   each asset's license for SLSA-style provenance
//!
//! ## What's in the catalogue
//!
//! | Kind        | Source examples                                          | License class      |
//! |-------------|----------------------------------------------------------|--------------------|
//! | `icon`      | Feather, Heroicons, Lucide, Tabler, Phosphor             | MIT / ISC          |
//! | `emoji`     | OpenMoji, Twemoji, Noto Emoji                            | CC-BY-SA / Apache  |
//! | `photo`     | Unsplash, Pexels, Pixabay, Wikimedia Commons (PD)        | open / PD          |
//! | `gif`       | CC0 loops + platform-generated                           | CC0                |
//! | `illustration` | unDraw, Open Peeps, Manypixels                        | CC0                |
//! | `template`  | PlausiDen-authored CMS JSON skeletons                    | platform           |
//!
//! Initial seed ships a small set; the catalogue grows via PRs.
//! See `LICENSES.md` and `SOURCES.md` in the crate root.
//!
//! ## API
//!
//! - [`AssetRegistry`] — typed collection
//! - [`Asset`] — single entry
//! - [`AssetKind`] — closed enum of asset types
//! - [`AssetSearch`] — search builder
//!
//! See the module doc for details.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// Closed enum of asset kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    /// Single-color line/solid icon (SVG).
    Icon,
    /// Color emoji (SVG).
    Emoji,
    /// Photograph (raster).
    Photo,
    /// Looping animation (SVG or APNG).
    Gif,
    /// Vector illustration.
    Illustration,
    /// CMS JSON template — a starting skeleton operators can
    /// customise.
    Template,
}

impl AssetKind {
    /// Stable kebab-case slug.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Icon => "icon",
            Self::Emoji => "emoji",
            Self::Photo => "photo",
            Self::Gif => "gif",
            Self::Illustration => "illustration",
            Self::Template => "template",
        }
    }
}

/// Closed enum of common license classes.
///
/// Keeping a closed enum (rather than free-form strings) means
/// the supply-chain audit can validate every entry has a known
/// compatible license at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LicenseClass {
    /// MIT.
    Mit,
    /// Apache 2.0.
    Apache2,
    /// ISC.
    Isc,
    /// Creative Commons Zero / public domain.
    Cc0,
    /// Creative Commons Attribution 4.0.
    CcBy4,
    /// Creative Commons Attribution-ShareAlike 4.0.
    CcBySa4,
    /// SIL Open Font License 1.1.
    Ofl1_1,
    /// Unsplash License.
    Unsplash,
    /// Pexels License.
    Pexels,
    /// Pixabay Content License.
    Pixabay,
    /// PlausiDen-authored under platform terms.
    Platform,
}

impl LicenseClass {
    /// Stable kebab-case slug.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Mit => "mit",
            Self::Apache2 => "apache-2",
            Self::Isc => "isc",
            Self::Cc0 => "cc0",
            Self::CcBy4 => "cc-by-4",
            Self::CcBySa4 => "cc-by-sa-4",
            Self::Ofl1_1 => "ofl-1-1",
            Self::Unsplash => "unsplash",
            Self::Pexels => "pexels",
            Self::Pixabay => "pixabay",
            Self::Platform => "platform",
        }
    }
    /// Whether attribution is required for downstream use.
    pub fn requires_attribution(&self) -> bool {
        matches!(
            self,
            Self::CcBy4 | Self::CcBySa4 | Self::Apache2 | Self::Mit | Self::Isc | Self::Ofl1_1
        )
    }
}

/// One asset entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Asset {
    /// Stable kebab-case slug — unique within the registry.
    pub slug: String,
    /// What kind of asset.
    pub kind: AssetKind,
    /// One-line human label ("right arrow", "house silhouette").
    pub label: String,
    /// Search tags (kebab-case): `["arrow", "right", "navigation"]`.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Free-text description for client-facing search.
    #[serde(default)]
    pub description: String,
    /// Where this asset lives — typically a path relative to the
    /// registry root.
    pub path: String,
    /// License class.
    pub license: LicenseClass,
    /// Where the asset came from (URL / repo / "platform-authored").
    pub source: String,
    /// Original author or upstream-set name (for attribution).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Optional thumbnail path for UI previews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_path: Option<String>,
    /// Optional dimensions hint (W x H pixels — useful for
    /// photos + illustrations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dim_hint: Option<(u32, u32)>,
}

/// Typed asset registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AssetRegistry {
    /// All entries.
    pub assets: Vec<Asset>,
}

impl AssetRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self { assets: vec![] }
    }

    /// Add an asset.
    pub fn add(&mut self, a: Asset) {
        self.assets.push(a);
    }

    /// Find by exact slug.
    pub fn get(&self, slug: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.slug == slug)
    }

    /// Search entries.
    pub fn search<'a>(&'a self, q: &AssetSearch) -> Vec<&'a Asset> {
        self.assets
            .iter()
            .filter(|a| q.matches(a))
            .collect::<Vec<_>>()
    }

    /// Count by kind.
    pub fn count_by_kind(&self, k: AssetKind) -> usize {
        self.assets.iter().filter(|a| a.kind == k).count()
    }

    /// Distinct licenses present in the registry. Used by the
    /// supply-chain audit to flag any unexpected license class.
    pub fn licenses_used(&self) -> std::collections::BTreeSet<LicenseClass> {
        self.assets.iter().map(|a| a.license).collect()
    }
}

/// Search query.
#[derive(Debug, Clone, Default)]
pub struct AssetSearch {
    /// If Some, restrict to this kind.
    pub kind: Option<AssetKind>,
    /// All these tags must be present (AND).
    pub tags_all: Vec<String>,
    /// Any of these tags present (OR). Empty = match all.
    pub tags_any: Vec<String>,
    /// Free-text — matched (case-insensitive) against label +
    /// description.
    pub text: Option<String>,
    /// Restrict to one license class.
    pub license: Option<LicenseClass>,
}

impl AssetSearch {
    /// Construct empty.
    pub fn new() -> Self {
        Default::default()
    }

    /// Apply.
    pub fn matches(&self, a: &Asset) -> bool {
        if let Some(k) = self.kind {
            if a.kind != k {
                return false;
            }
        }
        if let Some(l) = self.license {
            if a.license != l {
                return false;
            }
        }
        for t in &self.tags_all {
            if !a.tags.iter().any(|x| x == t) {
                return false;
            }
        }
        if !self.tags_any.is_empty()
            && !self.tags_any.iter().any(|t| a.tags.iter().any(|x| x == t))
        {
            return false;
        }
        if let Some(text) = &self.text {
            let needle = text.to_lowercase();
            let hay_l = a.label.to_lowercase();
            let hay_d = a.description.to_lowercase();
            if !hay_l.contains(&needle) && !hay_d.contains(&needle) {
                return false;
            }
        }
        true
    }
}

/// Construct the default seed registry — a small starter set
/// shipped with every loom-assets install.
///
/// This is intentionally tiny — the real catalogue grows via PRs
/// against `loom-assets/seeds/*.json`. Each PR adds + verifies
/// license + source.
pub fn default_seed() -> AssetRegistry {
    let mut r = AssetRegistry::new();
    for a in seed_icons() {
        r.add(a);
    }
    for a in seed_templates() {
        r.add(a);
    }
    r
}

fn seed_icons() -> Vec<Asset> {
    vec![
        Asset {
            slug: "icon-arrow-right".into(),
            kind: AssetKind::Icon,
            label: "Arrow, right".into(),
            tags: vec!["arrow".into(), "right".into(), "navigation".into(), "cta".into()],
            description: "Right-pointing arrow for forward navigation, CTAs, and pagination next-buttons.".into(),
            path: "icons/arrow-right.svg".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: Some((24, 24)),
        },
        Asset {
            slug: "icon-check".into(),
            kind: AssetKind::Icon,
            label: "Check mark".into(),
            tags: vec!["check".into(), "confirm".into(), "success".into(), "form".into()],
            description: "Check / confirmation mark for completed states and feature lists.".into(),
            path: "icons/check.svg".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: Some((24, 24)),
        },
        Asset {
            slug: "icon-x-close".into(),
            kind: AssetKind::Icon,
            label: "X / close".into(),
            tags: vec!["close".into(), "x".into(), "dismiss".into(), "cancel".into()],
            description: "X-shaped close button for modals, banners, and dismissable surfaces.".into(),
            path: "icons/x-close.svg".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: Some((24, 24)),
        },
        Asset {
            slug: "icon-search".into(),
            kind: AssetKind::Icon,
            label: "Magnifying glass".into(),
            tags: vec!["search".into(), "magnify".into(), "find".into()],
            description: "Magnifying-glass icon for search inputs and find-bar buttons.".into(),
            path: "icons/search.svg".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: Some((24, 24)),
        },
        Asset {
            slug: "icon-home".into(),
            kind: AssetKind::Icon,
            label: "House silhouette".into(),
            tags: vec!["home".into(), "house".into(), "navigation".into()],
            description: "Home navigation icon — landing-page link in header navs.".into(),
            path: "icons/home.svg".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: Some((24, 24)),
        },
    ]
}

fn seed_templates() -> Vec<Asset> {
    vec![
        Asset {
            slug: "template-saas-landing".into(),
            kind: AssetKind::Template,
            label: "SaaS landing".into(),
            tags: vec![
                "marketing".into(),
                "landing".into(),
                "saas".into(),
                "hero".into(),
                "features".into(),
                "pricing".into(),
            ],
            description: "Hero + logo-wall + 6-feature card grid + code sample + 3 testimonials + pricing teaser + final CTA. Use as a starting point for any developer-tools marketing site.".into(),
            path: "templates/saas-landing.json".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: None,
        },
        Asset {
            slug: "template-docs-hub".into(),
            kind: AssetKind::Template,
            label: "Docs hub".into(),
            tags: vec!["docs".into(), "marketing".into(), "developer".into()],
            description: "Install / author / build / deploy code-sample sequence + a quick-reference kv table. Pairs with the SaaS landing template.".into(),
            path: "templates/docs-hub.json".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: None,
        },
        Asset {
            slug: "template-pricing-3-tier".into(),
            kind: AssetKind::Template,
            label: "Pricing — 3 tiers".into(),
            tags: vec!["pricing".into(), "marketing".into(), "tiers".into()],
            description: "Three side-by-side tier cards (free / team / enterprise) with stat-based feature lists.".into(),
            path: "templates/pricing-3-tier.json".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: None,
        },
        Asset {
            slug: "template-blog-index".into(),
            kind: AssetKind::Template,
            label: "Blog index".into(),
            tags: vec!["blog".into(), "editorial".into(), "card-feed".into()],
            description: "Blog index card-feed with avatar / title / read-time / topic tag. Hand-edit the items array.".into(),
            path: "templates/blog-index.json".into(),
            license: LicenseClass::Platform,
            source: "platform-authored".into(),
            author: None,
            thumbnail_path: None,
            dim_hint: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_has_icons_and_templates() {
        let r = default_seed();
        assert!(r.count_by_kind(AssetKind::Icon) >= 5);
        assert!(r.count_by_kind(AssetKind::Template) >= 4);
    }

    #[test]
    fn search_filters_by_kind() {
        let r = default_seed();
        let q = AssetSearch {
            kind: Some(AssetKind::Icon),
            ..Default::default()
        };
        let hits = r.search(&q);
        assert!(hits.iter().all(|a| a.kind == AssetKind::Icon));
    }

    #[test]
    fn search_filters_by_tag() {
        let r = default_seed();
        let q = AssetSearch {
            tags_all: vec!["navigation".into()],
            ..Default::default()
        };
        let hits = r.search(&q);
        assert!(hits.iter().all(|a| a.tags.iter().any(|t| t == "navigation")));
    }

    #[test]
    fn search_filters_by_text() {
        let r = default_seed();
        let q = AssetSearch {
            text: Some("arrow".into()),
            ..Default::default()
        };
        let hits = r.search(&q);
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|a| {
            a.label.to_lowercase().contains("arrow")
                || a.description.to_lowercase().contains("arrow")
        }));
    }

    #[test]
    fn license_required_attribution_set() {
        assert!(!LicenseClass::Cc0.requires_attribution());
        assert!(LicenseClass::CcBy4.requires_attribution());
        assert!(LicenseClass::Mit.requires_attribution());
    }
}
