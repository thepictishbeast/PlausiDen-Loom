//! Typed `Picture` primitive — responsive image with modern-format
//! fallback chain (AVIF → WebP → JPEG).
//!
//! The web platform's `<picture>` element gives the browser an
//! ordered list of candidate sources; it picks the FIRST that it
//! can decode. AVIF wins where supported (best compression), WebP
//! catches Safari < 16, JPEG is the universal fallback. Every
//! visitor sees the smallest format their browser can render —
//! no JavaScript negotiation, no layout shift.
//!
//! AVP-2 INVARIANTS BAKED INTO THE TYPE
//! ------------------------------------
//! - `alt` is REQUIRED. Decorative images pass `""` explicitly so
//!   the developer makes the call instead of forgetting.
//! - `width` + `height` are REQUIRED. The intrinsic ratio reserves
//!   layout space → zero CLS contribution from this image.
//! - There is NO `extra_classes` slot. Layout / object-fit choices
//!   go through typed enums; new visual treatment is a doctrine
//!   change, not a per-call escape hatch.
//! - The asset stem path is interpolated into URLs WITHOUT
//!   user-supplied schemes — see `src_stem` doc for the contract.
//!
//! USAGE
//! -----
//! ```no_run
//! use loom_components::picture::{
//!     Picture, PictureFit, PictureLoading, PicturePriority,
//! };
//!
//! let p = Picture {
//!     src_stem: "hero/skill-vault",
//!     alt: "A vault door inscribed with skill marks",
//!     width: 1280,
//!     height: 720,
//!     loading: PictureLoading::Eager,
//!     priority: PicturePriority::High,
//!     fit: PictureFit::Cover,
//! };
//! let _markup = p.render();
//! ```

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Loading strategy. Defaults to `Lazy` when constructed via
/// `Picture::below_fold`; explicit on direct struct construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PictureLoading {
    /// `loading="lazy"` — fetched only when near viewport. Use for
    /// any image below the fold.
    Lazy,
    /// `loading="eager"` — fetched immediately. Use for images that
    /// are above the fold AND are the LCP candidate (one per page,
    /// ideally).
    Eager,
}

impl PictureLoading {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Lazy => "lazy",
            Self::Eager => "eager",
        }
    }
}

/// Resource priority hint. Lets the browser fetch LCP-critical
/// images ahead of less important resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PicturePriority {
    /// Browser default — no `fetchpriority` attribute emitted.
    Auto,
    /// `fetchpriority="high"` — pair with `Eager` for the LCP image.
    High,
    /// `fetchpriority="low"` — for decorative or below-fold tiles.
    Low,
}

impl PicturePriority {
    /// Return the attribute value, or `None` if `Auto` (omit attr).
    const fn as_attr(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::High => Some("high"),
            Self::Low => Some("low"),
        }
    }
}

/// `object-fit` selection — typed instead of allowing arbitrary CSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PictureFit {
    /// No `object-fit` modifier. Image fills its width/height box;
    /// aspect is preserved via the intrinsic dimensions.
    Default,
    /// `object-fit: cover;` — fill the box, crop overflow.
    Cover,
    /// `object-fit: contain;` — letterbox to fit, no crop.
    Contain,
}

impl PictureFit {
    const fn modifier_class(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Cover => Some("loom-picture--cover"),
            Self::Contain => Some("loom-picture--contain"),
        }
    }
}

/// Responsive image with modern-format fallback chain.
///
/// SECURITY: `src_stem` is interpolated into three URL paths
/// (`{stem}.avif`, `{stem}.webp`, `{stem}.jpg`) under the
/// `/assets/` prefix at render time. The component does NOT
/// accept absolute URLs, schemes, or query strings — the stem is
/// a path segment relative to the assets root. Callers MUST
/// validate that `src_stem` contains no `..` traversal, no `://`,
/// no leading slash. The Loom build pipeline enforces this; if
/// you're calling `Picture::render` outside that pipeline (e.g.
/// from a CMS bridge) you must validate yourself first.
pub struct Picture<'a> {
    /// Path stem under `/assets/`, no extension. e.g. `hero/dragon`
    /// yields `/assets/hero/dragon.avif` etc.
    pub src_stem: &'a str,
    /// Text alternative for screen readers + when image fails to
    /// load. Pass `""` ONLY for purely decorative images that add
    /// nothing for an assistive-tech user.
    pub alt: &'a str,
    /// Intrinsic width in CSS pixels. Required for CLS prevention.
    pub width: u32,
    /// Intrinsic height in CSS pixels. Required.
    pub height: u32,
    /// Loading strategy.
    pub loading: PictureLoading,
    /// Resource priority hint.
    pub priority: PicturePriority,
    /// `object-fit` mode.
    pub fit: PictureFit,
}

impl Picture<'_> {
    /// Render the `<picture>` element with AVIF + WebP + JPG sources.
    #[must_use]
    pub fn render(&self) -> Markup {
        let avif = format!("/assets/{}.avif", self.src_stem);
        let webp = format!("/assets/{}.webp", self.src_stem);
        let jpg = format!("/assets/{}.jpg", self.src_stem);
        let class = compose_class(self.fit);
        let priority = self.priority.as_attr();
        let loading = self.loading.as_str();
        html! {
            picture {
                source srcset=(avif) type="image/avif";
                source srcset=(webp) type="image/webp";
                @if let Some(p) = priority {
                    img
                        src=(jpg)
                        alt=(self.alt)
                        width=(self.width)
                        height=(self.height)
                        loading=(loading)
                        decoding="async"
                        fetchpriority=(p)
                        class=(class);
                } @else {
                    img
                        src=(jpg)
                        alt=(self.alt)
                        width=(self.width)
                        height=(self.height)
                        loading=(loading)
                        decoding="async"
                        class=(class);
                }
            }
        }
    }
}

/// Compose the `<img>` class string from the fit selection.
/// Always emits the base `loom-picture` class so skin.css can
/// target it; appends `loom-picture--cover` / `--contain` when set.
fn compose_class(fit: PictureFit) -> String {
    fit.modifier_class().map_or_else(
        || "loom-picture".to_owned(),
        |m| format!("loom-picture {m}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(p: &Picture<'_>) -> String {
        p.render().into_string()
    }

    #[test]
    fn renders_three_format_chain_in_order() {
        let p = Picture {
            src_stem: "hero/dragon",
            alt: "A dragon",
            width: 1280,
            height: 720,
            loading: PictureLoading::Eager,
            priority: PicturePriority::High,
            fit: PictureFit::Cover,
        };
        let html = render_to_string(&p);
        let avif_pos = html.find("/assets/hero/dragon.avif").expect("avif present");
        let webp_pos = html.find("/assets/hero/dragon.webp").expect("webp present");
        let jpg_pos = html.find("/assets/hero/dragon.jpg").expect("jpg present");
        assert!(avif_pos < webp_pos, "avif must precede webp");
        assert!(webp_pos < jpg_pos, "webp must precede jpg");
    }

    #[test]
    fn emits_intrinsic_dimensions_for_cls() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 800,
            height: 600,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"width="800""#));
        assert!(html.contains(r#"height="600""#));
    }

    #[test]
    fn lazy_loading_is_default_explicit() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"loading="lazy""#));
        assert!(html.contains(r#"decoding="async""#));
    }

    #[test]
    fn eager_loading_serializes_as_eager() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Eager,
            priority: PicturePriority::High,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"loading="eager""#));
        assert!(html.contains(r#"fetchpriority="high""#));
    }

    #[test]
    fn priority_auto_omits_attribute() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        assert!(!html.contains("fetchpriority"), "auto must omit attr");
    }

    #[test]
    fn priority_low_serializes_as_low() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Low,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"fetchpriority="low""#));
    }

    #[test]
    fn empty_alt_for_decorative() {
        let p = Picture {
            src_stem: "x",
            alt: "",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        // The alt attribute must still be present (empty string),
        // not absent — that's the WCAG-compliant decorative shape.
        assert!(html.contains(r#"alt="""#));
    }

    #[test]
    fn fit_cover_emits_modifier_class() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Cover,
        };
        let html = render_to_string(&p);
        assert!(html.contains("loom-picture--cover"));
        assert!(html.contains("loom-picture "));
    }

    #[test]
    fn fit_contain_emits_modifier_class() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Contain,
        };
        let html = render_to_string(&p);
        assert!(html.contains("loom-picture--contain"));
    }

    #[test]
    fn fit_default_emits_only_base_class() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"class="loom-picture""#));
        assert!(!html.contains("loom-picture--"));
    }

    #[test]
    fn picture_element_wraps_sources_and_img() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        let p_open = html.find("<picture>").expect("opens picture");
        let p_close = html.rfind("</picture>").expect("closes picture");
        let img_pos = html.find("<img").expect("img present");
        let avif_pos = html.find(r#"type="image/avif""#).expect("avif type");
        assert!(p_open < avif_pos);
        assert!(avif_pos < img_pos);
        assert!(img_pos < p_close);
    }

    #[test]
    fn type_attributes_match_sources() {
        let p = Picture {
            src_stem: "x",
            alt: "x",
            width: 1,
            height: 1,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Default,
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"type="image/avif""#));
        assert!(html.contains(r#"type="image/webp""#));
    }
}
