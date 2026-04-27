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

/// A typed badge.
pub struct Badge<'a> {
    /// Visible label.
    pub label: &'a str,
    /// Tone.
    pub tone: BadgeTone,
    /// Size.
    pub size: BadgeSize,
}

impl Badge<'_> {
    /// Render as `<span>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = format!(
            "inline-block rounded-full font-semibold border {tone} {size}",
            tone = tone_classes(self.tone),
            size = size_classes(self.size),
        );
        html! {
            span class=(class) { (self.label) }
        }
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
        }
        .render()
        .into_string();
        assert!(s.contains("bg-primary/10"));
        assert!(s.contains("text-primary"));
        assert!(s.contains(">Field Notes<"));
    }

    #[test]
    fn ondark_uses_white_backdrop() {
        let s = Badge {
            label: "x",
            tone: BadgeTone::OnDark,
            size: BadgeSize::Sm,
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
        }
        .render()
        .into_string();
        assert!(s.contains("text-xs"));
        assert!(s.contains("px-2.5"));
    }
}
