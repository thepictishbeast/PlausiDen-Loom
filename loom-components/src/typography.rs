//! Typed typography primitives — `Heading`, `Lede`, `BodyText`.
//!
//! Replace every `text-3xl font-bold ...` raw class string in views.
//! Adding a level / variant is a doctrine review.

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Heading level. Maps to the corresponding HTML tag.
///
/// Visual variant is decoupled from semantic level so a page can use
/// a `<h2>` styled as the section heading without embedding font
/// sizes inline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadingLevel {
    /// `<h1>` — one per page. Hero headlines.
    H1,
    /// `<h2>` — section-level. "What we cover", "Why firms come to us".
    H2,
    /// `<h3>` — subsection. Capability card titles, FAQ items.
    H3,
}

/// Visual style. May differ from semantic level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadingVariant {
    /// Hero-scale display — biggest. `text-4xl md:text-5xl lg:text-6xl`.
    Display,
    /// Section heading. `text-3xl md:text-4xl`.
    Section,
    /// Card / feature heading. `text-xl`.
    Sub,
    /// Card sub-heading — the smaller heading inside a card body
    /// (e.g. "What we shipped" inside a case-study card,
    /// "What's included" inside a pricing tier). `text-lg`.
    Card,
}

/// Color tone. Mostly determined by surrounding band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadingTone {
    /// Slate-900 (default on light backgrounds).
    Ink,
    /// White (on dark bands).
    OnDark,
}

/// A typed heading.
pub struct Heading<'a> {
    /// Heading text.
    pub text: &'a str,
    /// Semantic level (`h1`/`h2`/`h3`).
    pub level: HeadingLevel,
    /// Visual variant.
    pub variant: HeadingVariant,
    /// Tone.
    pub tone: HeadingTone,
}

impl Heading<'_> {
    /// Render as the appropriate heading tag.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = format!(
            "font-display font-bold leading-tight {variant} {tone}",
            variant = variant_classes(self.variant),
            tone = tone_classes(self.tone),
        );
        match self.level {
            HeadingLevel::H1 => html! { h1 class=(class) { (self.text) } },
            HeadingLevel::H2 => html! { h2 class=(class) { (self.text) } },
            HeadingLevel::H3 => html! { h3 class=(class) { (self.text) } },
        }
    }
}

const fn variant_classes(v: HeadingVariant) -> &'static str {
    match v {
        HeadingVariant::Display => "text-4xl md:text-5xl lg:text-6xl",
        HeadingVariant::Section => "text-3xl md:text-4xl",
        HeadingVariant::Sub => "text-xl",
        HeadingVariant::Card => "text-lg",
    }
}

const fn tone_classes(t: HeadingTone) -> &'static str {
    match t {
        HeadingTone::Ink => "text-slate-900",
        HeadingTone::OnDark => "text-white",
    }
}

/// Subhead lede paragraph — the larger body text directly under a
/// heading.
pub struct Lede<'a> {
    /// Text content.
    pub text: &'a str,
    /// Tone.
    pub tone: HeadingTone,
}

impl Lede<'_> {
    /// Render as `<p>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let tone = match self.tone {
            HeadingTone::Ink => "text-slate-600",
            HeadingTone::OnDark => "text-slate-400",
        };
        let class = format!("text-lg md:text-xl leading-relaxed {tone}");
        html! { p class=(class) { (self.text) } }
    }
}

/// Standard body paragraph.
pub struct BodyText<'a> {
    /// Text content.
    pub text: &'a str,
    /// Tone.
    pub tone: HeadingTone,
}

impl BodyText<'_> {
    /// Render as `<p>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let tone = match self.tone {
            HeadingTone::Ink => "text-slate-700",
            HeadingTone::OnDark => "text-slate-300",
        };
        let class = format!("leading-relaxed {tone}");
        html! { p class=(class) { (self.text) } }
    }
}

/// Helper text — smaller, lighter prose used under inputs, beside
/// buttons, and as form notes. Two sizes: Default (`text-sm`) and
/// Tiny (`text-xs`). Always `text-slate-500` on light, `text-slate-400`
/// on dark.
///
/// Use this in place of raw `class="text-sm text-slate-500"` strings
/// (16 occurrences in plausiden.com at the time this primitive
/// landed).
pub struct HelperText<'a> {
    /// Text content.
    pub text: &'a str,
    /// Size step.
    pub size: HelperSize,
    /// Tone — Ink for light bands, OnDark for dark bands.
    pub tone: HeadingTone,
}

/// Size step for [`HelperText`]. Closed enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperSize {
    /// `text-sm` — under headings, captions.
    Default,
    /// `text-xs` — micro-copy under buttons.
    Tiny,
}

impl HelperText<'_> {
    /// Render as `<p>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let size = match self.size {
            HelperSize::Default => "text-sm",
            HelperSize::Tiny => "text-xs",
        };
        let tone = match self.tone {
            HeadingTone::Ink => "text-slate-500",
            HeadingTone::OnDark => "text-slate-400",
        };
        let class = format!("{size} {tone}");
        html! { p class=(class) { (self.text) } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h1_emits_h1_tag() {
        let s = Heading {
            text: "Hero",
            level: HeadingLevel::H1,
            variant: HeadingVariant::Display,
            tone: HeadingTone::Ink,
        }
        .render()
        .into_string();
        assert!(s.starts_with("<h1"));
        assert!(s.contains(">Hero</h1>"));
    }

    #[test]
    fn h2_section_emits_section_classes() {
        let s = Heading {
            text: "Section",
            level: HeadingLevel::H2,
            variant: HeadingVariant::Section,
            tone: HeadingTone::Ink,
        }
        .render()
        .into_string();
        assert!(s.contains("text-3xl"));
        assert!(s.contains("md:text-4xl"));
        assert!(s.contains("text-slate-900"));
    }

    #[test]
    fn h3_card_emits_text_lg() {
        let s = Heading {
            text: "What we shipped",
            level: HeadingLevel::H3,
            variant: HeadingVariant::Card,
            tone: HeadingTone::Ink,
        }
        .render()
        .into_string();
        assert!(s.starts_with("<h3"));
        assert!(s.contains("text-lg"));
        // Card variant must NOT also emit a larger size — sub vs card divergence
        assert!(!s.contains("text-xl"));
        assert!(!s.contains("text-3xl"));
        assert!(s.contains(">What we shipped</h3>"));
    }

    #[test]
    fn ondark_tone_emits_text_white() {
        let s = Heading {
            text: "x",
            level: HeadingLevel::H2,
            variant: HeadingVariant::Section,
            tone: HeadingTone::OnDark,
        }
        .render()
        .into_string();
        assert!(s.contains("text-white"));
    }

    #[test]
    fn lede_uses_larger_body_text() {
        let s = Lede {
            text: "the lede",
            tone: HeadingTone::Ink,
        }
        .render()
        .into_string();
        assert!(s.contains("text-lg"));
        assert!(s.contains("leading-relaxed"));
        assert!(s.contains(">the lede<"));
    }

    #[test]
    fn body_uses_slate_700_on_ink() {
        let s = BodyText {
            text: "x",
            tone: HeadingTone::Ink,
        }
        .render()
        .into_string();
        assert!(s.contains("text-slate-700"));
    }

    #[test]
    fn helper_default_size_emits_text_sm() {
        let s = HelperText {
            text: "x",
            size: HelperSize::Default,
            tone: HeadingTone::Ink,
        }
        .render()
        .into_string();
        assert!(s.contains("text-sm"));
        assert!(s.contains("text-slate-500"));
    }

    #[test]
    fn helper_tiny_size_emits_text_xs() {
        let s = HelperText {
            text: "fine print",
            size: HelperSize::Tiny,
            tone: HeadingTone::Ink,
        }
        .render()
        .into_string();
        assert!(s.contains("text-xs"));
        assert!(s.contains("fine print"));
    }

    #[test]
    fn helper_ondark_uses_slate_400() {
        let s = HelperText {
            text: "x",
            size: HelperSize::Default,
            tone: HeadingTone::OnDark,
        }
        .render()
        .into_string();
        assert!(s.contains("text-slate-400"));
    }
}
