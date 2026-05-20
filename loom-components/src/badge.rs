//! Typed `Badge` (eyebrow pill / inline label) primitive.
//!
//! Used everywhere a small "category" or status pill needs to appear:
//! eyebrows above headings, inline tags on blog cards, the post
//! category eyebrow on `/blog/<slug>`, the dark-band "Why we exist"
//! signal pills.

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Visual tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeTone {
    /// Brand-colored pill on light surface — default for eyebrows.
    Primary,
    /// White-on-dark pill with translucent backdrop. For dark bands.
    OnDark,
    /// Slate-tinted neutral pill. For category tags on cards.
    Neutral,
    /// Emerald-tinted success pill.
    Success,
}

/// Size step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeSize {
    /// Compact (text-xs) — used inline on cards.
    Sm,
    /// Default (text-sm) — eyebrow above headings.
    Md,
}

/// Shape — controls the chrome envelope around the label.
///
/// `Pill` is the SaaS-canonical `rounded-full` shape (back-compat
/// default). `Square` softens to `rounded-md` for less marketing-flavor
/// look. `EditorialKicker` strips the chrome entirely — no border, no
/// rounded corners, no surface — and rerenders as monospace
/// uppercase tracked text in the brand color. Pairs with the editorial
/// composition vocabulary (HeroEditorial.kicker / KvPairCard.label /
/// PullQuote display).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeShape {
    /// `rounded-full` SaaS pill — back-compat default.
    #[default]
    Pill,
    /// `rounded-md` square-with-soft-corners.
    Square,
    /// No chrome; monospace uppercase tracked text in tone color.
    /// Pairs with the editorial composition vocabulary.
    EditorialKicker,
}

/// A typed badge.
pub struct Badge<'a> {
    /// Visible label.
    pub label: &'a str,
    /// Tone.
    pub tone: BadgeTone,
    /// Size.
    pub size: BadgeSize,
    /// Shape.
    pub shape: BadgeShape,
}

impl Badge<'_> {
    /// Render as `<span>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = compose_class(self.tone, self.size, self.shape);
        html! {
            span class=(class) data-loom-badge-shape=(shape_attr(self.shape)) { (self.label) }
        }
    }
}

fn compose_class(tone: BadgeTone, size: BadgeSize, shape: BadgeShape) -> String {
    match shape {
        BadgeShape::Pill => format!(
            "inline-block rounded-full font-semibold border {tone} {size}",
            tone = tone_classes(tone),
            size = size_classes(size),
        ),
        BadgeShape::Square => format!(
            "inline-block rounded-md font-semibold border {tone} {size}",
            tone = tone_classes(tone),
            size = size_classes(size),
        ),
        BadgeShape::EditorialKicker => format!(
            "inline-block font-mono uppercase tracking-widest font-medium {tone} {size}",
            tone = editorial_kicker_tone_classes(tone),
            size = editorial_kicker_size_classes(size),
        ),
    }
}

fn shape_attr(shape: BadgeShape) -> &'static str {
    match shape {
        BadgeShape::Pill => "pill",
        BadgeShape::Square => "square",
        BadgeShape::EditorialKicker => "editorial-kicker",
    }
}

/// Editorial-kicker tone classes — text color only, no surface / border.
const fn editorial_kicker_tone_classes(t: BadgeTone) -> &'static str {
    match t {
        BadgeTone::Primary => "text-primary",
        BadgeTone::OnDark => "text-slate-300",
        BadgeTone::Neutral => "text-slate-500",
        BadgeTone::Success => "text-emerald-700",
    }
}

/// Editorial-kicker sizes — no horizontal padding (the kicker hugs
/// the surrounding flow), small text sizes since it's metadata.
const fn editorial_kicker_size_classes(s: BadgeSize) -> &'static str {
    match s {
        BadgeSize::Sm => "text-[0.7rem]",
        BadgeSize::Md => "text-xs",
    }
}

const fn tone_classes(t: BadgeTone) -> &'static str {
    match t {
        BadgeTone::Primary => "bg-primary/10 text-primary border-primary/20",
        BadgeTone::OnDark => "bg-white/10 text-white border-white/10 backdrop-blur-sm",
        BadgeTone::Neutral => "bg-slate-100 text-slate-700 border-slate-200",
        BadgeTone::Success => "bg-emerald-50 text-emerald-700 border-emerald-200",
    }
}

const fn size_classes(s: BadgeSize) -> &'static str {
    match s {
        BadgeSize::Sm => "px-2.5 py-0.5 text-xs",
        BadgeSize::Md => "px-4 py-1.5 text-sm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_md_emits_brand_classes() {
        let s = Badge {
            label: "Field Notes",
            tone: BadgeTone::Primary,
            size: BadgeSize::Md,
            shape: BadgeShape::default(),
        }
        .render()
        .into_string();
        assert!(s.contains("bg-primary/10"));
        assert!(s.contains("text-primary"));
        assert!(s.contains(">Field Notes<"));
        // Default shape is Pill — rounded-full.
        assert!(s.contains("rounded-full"));
        assert!(s.contains(r#"data-loom-badge-shape="pill""#));
    }

    #[test]
    fn ondark_uses_white_backdrop() {
        let s = Badge {
            label: "x",
            tone: BadgeTone::OnDark,
            size: BadgeSize::Sm,
            shape: BadgeShape::default(),
        }
        .render()
        .into_string();
        assert!(s.contains("bg-white/10"));
        assert!(s.contains("text-white"));
    }

    #[test]
    fn sm_uses_compact_padding() {
        let s = Badge {
            label: "x",
            tone: BadgeTone::Neutral,
            size: BadgeSize::Sm,
            shape: BadgeShape::default(),
        }
        .render()
        .into_string();
        assert!(s.contains("text-xs"));
        assert!(s.contains("px-2.5"));
    }

    #[test]
    fn square_shape_softens_radius() {
        let s = Badge {
            label: "x",
            tone: BadgeTone::Primary,
            size: BadgeSize::Md,
            shape: BadgeShape::Square,
        }
        .render()
        .into_string();
        assert!(s.contains("rounded-md"));
        assert!(!s.contains("rounded-full"));
        assert!(s.contains(r#"data-loom-badge-shape="square""#));
    }

    #[test]
    fn editorial_kicker_strips_chrome() {
        // The shape that pairs with the editorial composition
        // vocabulary. No border, no rounded, no background surface.
        let s = Badge {
            label: "DISPATCH",
            tone: BadgeTone::Primary,
            size: BadgeSize::Md,
            shape: BadgeShape::EditorialKicker,
        }
        .render()
        .into_string();
        assert!(!s.contains("rounded-full"));
        assert!(!s.contains("rounded-md"));
        assert!(!s.contains("border "));
        assert!(!s.contains("bg-primary/10"));
        assert!(!s.contains("bg-white/10"));
        // Editorial-kicker shape uses monospace uppercase tracked.
        assert!(s.contains("font-mono"));
        assert!(s.contains("uppercase"));
        assert!(s.contains("tracking-widest"));
        // Tone color survives (text-primary).
        assert!(s.contains("text-primary"));
        assert!(s.contains(r#"data-loom-badge-shape="editorial-kicker""#));
        assert!(s.contains(">DISPATCH<"));
    }

    #[test]
    fn editorial_kicker_tone_map_does_not_use_brand_surface() {
        // Editorial-kicker primary uses `text-primary` color but
        // NO `bg-primary/10` surface chrome.
        let s = Badge {
            label: "x",
            tone: BadgeTone::Primary,
            size: BadgeSize::Md,
            shape: BadgeShape::EditorialKicker,
        }
        .render()
        .into_string();
        assert!(s.contains("text-primary"));
        assert!(!s.contains("bg-primary/10"));
        assert!(!s.contains("border-primary/20"));
    }

    #[test]
    fn editorial_kicker_ondark_uses_slate_300() {
        // On dark surfaces, kicker text is slate-300 (mid-gray ink
        // against dark band), not white.
        let s = Badge {
            label: "x",
            tone: BadgeTone::OnDark,
            size: BadgeSize::Sm,
            shape: BadgeShape::EditorialKicker,
        }
        .render()
        .into_string();
        assert!(s.contains("text-slate-300"));
        assert!(!s.contains("backdrop-blur-sm"));
    }

    #[test]
    fn badge_shape_default_is_pill() {
        // Back-compat guarantee: BadgeShape::default() yields the
        // legacy SaaS-pill shape so existing callers via
        // ..Default::default() keep rendering identically.
        assert!(matches!(BadgeShape::default(), BadgeShape::Pill));
    }
}
