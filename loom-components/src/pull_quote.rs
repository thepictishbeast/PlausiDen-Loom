//! Typed `PullQuote` primitive — editorial inline quote.
//!
//! The preamble names `pull_quote editorial` as a substrate vocabulary
//! gap. This primitive ships the editorial composition that pairs with
//! `HeroEditorial`'s decoration slot, `KvPairCard` grids in the body,
//! and `BodyText` editorial paragraphs.
//!
//! Shape commitments enforced by tests:
//!
//! * Uses a left-border editorial rule, NOT giant decorative
//!   `“...”` quote-mark glyphs above the text.
//! * No italic by default (italic-by-default is a SaaS trope; reserve
//!   italic for true emphasis the operator opted into).
//! * No card / shadow / rounded ornaments.
//! * No "as featured in" / publication-logo / avatar-circle pretense
//!   — attribution is a plain attribution line, not a card.
//! * Renders as semantic `<blockquote>` with optional `cite="..."`
//!   per the HTML living standard.
//! * Honors AMOLED tone per `[[dark-theme-amoled-true-black]]`.

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Visual emphasis tier for the quote body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullQuoteEmphasis {
    /// Inline-with-body voice. `text-xl md:text-2xl`. Use inside a
    /// long editorial body to break up running prose.
    Inline,
    /// Hero-side / decoration-slot voice. `text-2xl md:text-3xl
    /// lg:text-4xl`. Use as a `HeroEditorial.decoration` value or
    /// as a standalone editorial mark.
    Display,
}

/// Color tone. Identical layout across tones; surfaces + text shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullQuoteTone {
    /// Slate text on light surface (default).
    Slate,
    /// Slate-100 on AMOLED true-black surface.
    Amoled,
}

/// Editorial pull quote.
///
/// Example rendered shape:
///
/// ```text
/// │  The substrate cannot ship a hero centered on a single line
/// │  of marketing copy and call that an answer.
/// │
/// │  — paul, 2026-05-20
/// ```
pub struct PullQuote<'a> {
    /// Quote body text. Multiple paragraphs render as separate `<p>`s
    /// when the caller passes `\n\n` separators; this primitive splits
    /// internally to preserve semantic paragraph boundaries.
    pub body: &'a str,
    /// Optional attribution line. Rendered as a separate `<footer>`
    /// element below the quote with an em-dash prefix per editorial
    /// convention.
    pub attribution: Option<&'a str>,
    /// Optional source URL — rendered into the `cite` attribute on
    /// `<blockquote>` per the HTML spec for machine-readable
    /// provenance.
    pub cite_url: Option<&'a str>,
    /// Emphasis tier.
    pub emphasis: PullQuoteEmphasis,
    /// Tone.
    pub tone: PullQuoteTone,
}

impl PullQuote<'_> {
    /// Render as `<blockquote>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let (body_size, padding_left) = match self.emphasis {
            PullQuoteEmphasis::Inline => ("text-xl md:text-2xl", "pl-5 md:pl-6"),
            PullQuoteEmphasis::Display => (
                "text-2xl md:text-3xl lg:text-4xl",
                "pl-6 md:pl-8",
            ),
        };
        let (border_color, body_color, attr_color) = match self.tone {
            PullQuoteTone::Slate => ("border-slate-300", "text-slate-900", "text-slate-600"),
            PullQuoteTone::Amoled => ("border-slate-700", "text-slate-100", "text-slate-400"),
        };
        let outer = format!(
            "border-l-2 {border_color} {padding_left} flex flex-col gap-3"
        );
        let body_class = format!(
            "font-display font-medium leading-snug {body_size} {body_color}"
        );
        let attr_class = format!(
            "text-sm md:text-base leading-relaxed {attr_color}"
        );
        let paragraphs: Vec<&str> = self
            .body
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        html! {
            blockquote class=(outer) cite=[self.cite_url] {
                @for para in &paragraphs {
                    p class=(body_class) {
                        (*para)
                    }
                }
                @if let Some(attr) = self.attribution {
                    footer class=(attr_class) {
                        "— "
                        (attr)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_body_and_attribution() {
        let s = PullQuote {
            body: "The substrate cannot ship a hero centered on a single line of marketing copy and call that an answer.",
            attribution: Some("paul, 2026-05-20"),
            cite_url: None,
            emphasis: PullQuoteEmphasis::Display,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("<blockquote"));
        assert!(s.contains(">The substrate cannot ship"));
        assert!(s.contains(">— paul, 2026-05-20<"));
        assert!(s.contains("<footer"));
    }

    #[test]
    fn omits_footer_when_no_attribution() {
        let s = PullQuote {
            body: "x",
            attribution: None,
            cite_url: None,
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        assert!(!s.contains("<footer"));
        assert!(!s.contains("— "));
    }

    #[test]
    fn cite_url_emits_cite_attribute() {
        let s = PullQuote {
            body: "x",
            attribution: Some("a"),
            cite_url: Some("https://example.org/source"),
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains(r#"cite="https://example.org/source""#));
    }

    #[test]
    fn cite_url_none_omits_cite_attribute() {
        let s = PullQuote {
            body: "x",
            attribution: None,
            cite_url: None,
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        assert!(!s.contains("cite="));
    }

    #[test]
    fn no_saas_trope_ornaments() {
        // The shape guarantee that distinguishes this primitive from
        // testimonial-card SaaS shapes. The preamble specifically
        // calls out avatar circles + "most popular" decorations as
        // tropes to avoid.
        let s = PullQuote {
            body: "Body text here for the trope-check",
            attribution: Some("Name, Role"),
            cite_url: None,
            emphasis: PullQuoteEmphasis::Display,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        assert!(!s.contains("rounded-"));
        assert!(!s.contains("shadow-"));
        assert!(!s.contains("bg-primary"));
        // No decorative italic markup — italic is a stylistic opt-in,
        // not a default for pull-quotes.
        assert!(!s.contains("italic"));
        // No giant decorative quote-mark glyphs as a pseudo-element
        // or inline literal.
        assert!(!s.contains("&ldquo;"));
        assert!(!s.contains("&rdquo;"));
        // Uses a left-border editorial rule, NOT a quote-mark decoration.
        assert!(s.contains("border-l-2"));
    }

    #[test]
    fn inline_emphasis_uses_smaller_sizes() {
        let s = PullQuote {
            body: "x",
            attribution: None,
            cite_url: None,
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("text-xl"));
        assert!(s.contains("md:text-2xl"));
        // Must NOT use the display-tier sizes.
        assert!(!s.contains("lg:text-4xl"));
    }

    #[test]
    fn display_emphasis_uses_larger_sizes() {
        let s = PullQuote {
            body: "x",
            attribution: None,
            cite_url: None,
            emphasis: PullQuoteEmphasis::Display,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("text-2xl"));
        assert!(s.contains("md:text-3xl"));
        assert!(s.contains("lg:text-4xl"));
    }

    #[test]
    fn amoled_tone_uses_true_black_neighborhood_colors() {
        let s = PullQuote {
            body: "x",
            attribution: Some("a"),
            cite_url: None,
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Amoled,
        }
        .render()
        .into_string();
        assert!(s.contains("text-slate-100"));
        // The light tone's slate-900 must NOT appear here.
        assert!(!s.contains("text-slate-900"));
        // Border darkens for AMOLED so it reads against true black.
        assert!(s.contains("border-slate-700"));
    }

    #[test]
    fn splits_paragraph_separators_into_separate_p_tags() {
        let s = PullQuote {
            body: "First paragraph.\n\nSecond paragraph.",
            attribution: None,
            cite_url: None,
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        // Two <p> tags expected — count opens.
        let p_open_count = s.matches("<p").count();
        assert_eq!(p_open_count, 2, "expected 2 <p> tags, got {p_open_count}: {s}");
        assert!(s.contains(">First paragraph.<"));
        assert!(s.contains(">Second paragraph.<"));
    }

    #[test]
    fn single_paragraph_emits_one_p() {
        let s = PullQuote {
            body: "Just one.",
            attribution: None,
            cite_url: None,
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        let p_open_count = s.matches("<p").count();
        assert_eq!(p_open_count, 1);
    }

    #[test]
    fn ignores_empty_paragraphs_between_separators() {
        // Triple newlines or leading/trailing whitespace should NOT
        // emit empty <p> tags.
        let s = PullQuote {
            body: "  \n\n  Only paragraph  \n\n  ",
            attribution: None,
            cite_url: None,
            emphasis: PullQuoteEmphasis::Inline,
            tone: PullQuoteTone::Slate,
        }
        .render()
        .into_string();
        let p_open_count = s.matches("<p").count();
        assert_eq!(p_open_count, 1);
        assert!(s.contains(">Only paragraph<"));
    }
}
