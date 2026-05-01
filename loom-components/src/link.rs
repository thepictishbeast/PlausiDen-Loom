//! Typed `TextLink` primitive — inline anchor element with a
//! constrained palette of visual variants.
//!
//! Sixteen sites in plausiden.com (across views, the inquiry
//! handler, admin chrome, and 5 solutions pages) shipped raw
//! `<a class="text-primary font-semibold">` strings before this
//! primitive landed. Promote them to a typed call so the styling
//! lives behind one struct.

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Visual style for a [`TextLink`]. Closed enum.
///
/// Adding a variant requires a doctrine review; if a caller wants
/// "just one custom style," extend the design system rather than
/// adding an `extra_classes` slot here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextLinkVariant {
    /// `text-primary` — most inline links inside a paragraph.
    Primary,
    /// `text-primary font-medium` — mailto / quiet-emphasis links
    /// (5 sites in plausiden.com use this exact shape).
    PrimaryMedium,
    /// `text-primary font-semibold` — CTAs ("Read more →",
    /// "Back home").
    PrimaryBold,
    /// `text-primary underline` — table-row links / admin email
    /// addresses where bold would be visually heavy.
    PrimaryUnderlined,
    /// `text-primary font-semibold underline` — the underlined
    /// CTAs (export links, in-text "how we work" anchors).
    Underlined,
    /// `text-slate-600 hover:text-primary transition-colors` —
    /// subtle inline links inside dense prose (footer rows,
    /// contact-card list items).
    Subtle,
}

/// Size step for [`TextLink`]. Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextLinkSize {
    /// Inherit ambient font-size (no class added).
    Default,
    /// `text-sm` — when the link sits inside a helper / footer.
    Small,
}

/// A typed inline anchor.
///
/// SECURITY: there is no `extra_classes` field. If you find
/// yourself wanting one, the design system has a real gap; extend
/// it (variant, size) rather than routing around.
pub struct TextLink<'a> {
    /// Visible text.
    pub label: &'a str,
    /// `href` attribute (the value is rendered verbatim — caller
    /// is responsible for escaping when interpolating from user
    /// input).
    pub href: &'a str,
    /// Visual variant.
    pub variant: TextLinkVariant,
    /// Size step.
    pub size: TextLinkSize,
}

impl<'a> TextLink<'a> {
    /// Convenience constructor — `Primary` variant at `Default`
    /// size.
    #[must_use]
    pub const fn new(label: &'a str, href: &'a str) -> Self {
        Self {
            label,
            href,
            variant: TextLinkVariant::Primary,
            size: TextLinkSize::Default,
        }
    }

    /// Render as `<a>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = format!(
            "{variant}{size_sep}{size}",
            variant = variant_classes(self.variant),
            size_sep = if matches!(self.size, TextLinkSize::Default) {
                ""
            } else {
                " "
            },
            size = size_classes(self.size),
        );
        html! {
            a href=(self.href) class=(class.trim()) { (self.label) }
        }
    }
}

const fn variant_classes(v: TextLinkVariant) -> &'static str {
    match v {
        TextLinkVariant::Primary => "text-primary",
        TextLinkVariant::PrimaryMedium => "text-primary font-medium",
        TextLinkVariant::PrimaryBold => "text-primary font-semibold",
        TextLinkVariant::PrimaryUnderlined => "text-primary underline",
        TextLinkVariant::Underlined => "text-primary font-semibold underline",
        TextLinkVariant::Subtle => "text-slate-600 hover:text-primary transition-colors",
    }
}

const fn size_classes(s: TextLinkSize) -> &'static str {
    match s {
        TextLinkSize::Default => "",
        TextLinkSize::Small => "text-sm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_variant_emits_text_primary() {
        let s = TextLink::new("x", "/y").render().into_string();
        assert!(s.contains("text-primary"));
        assert!(!s.contains("font-semibold"));
        assert!(s.contains(r#"href="/y""#));
        assert!(s.contains(">x</a>"));
    }

    #[test]
    fn primary_bold_emits_semibold() {
        let s = TextLink {
            label: "x",
            href: "/y",
            variant: TextLinkVariant::PrimaryBold,
            size: TextLinkSize::Default,
        }
        .render()
        .into_string();
        assert!(s.contains("text-primary font-semibold"));
    }

    #[test]
    fn underlined_emits_underline() {
        let s = TextLink {
            label: "x",
            href: "/y",
            variant: TextLinkVariant::Underlined,
            size: TextLinkSize::Default,
        }
        .render()
        .into_string();
        assert!(s.contains("underline"));
    }

    #[test]
    fn subtle_emits_hover_primary() {
        let s = TextLink {
            label: "x",
            href: "/y",
            variant: TextLinkVariant::Subtle,
            size: TextLinkSize::Default,
        }
        .render()
        .into_string();
        assert!(s.contains("hover:text-primary"));
        assert!(s.contains("text-slate-600"));
    }

    #[test]
    fn small_size_adds_text_sm() {
        let s = TextLink {
            label: "x",
            href: "/y",
            variant: TextLinkVariant::Underlined,
            size: TextLinkSize::Small,
        }
        .render()
        .into_string();
        assert!(s.contains("text-sm"));
    }

    #[test]
    fn default_size_omits_text_sm() {
        let s = TextLink::new("x", "/y").render().into_string();
        assert!(!s.contains("text-sm"));
    }
}
