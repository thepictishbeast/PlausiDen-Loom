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

/// Helper text — smaller, lighter prose for inputs and form notes.
///
/// Used under inputs, beside buttons, and as form notes. Two sizes:
/// Default (`text-sm`) and Tiny (`text-xs`). Always
/// `text-slate-500` on light, `text-slate-400` on dark.
///
/// Use this in place of raw `class="text-sm text-slate-500"` strings
/// (16 occurrences in plausiden.com at the time this primitive
/// landed).
pub struct HelperText<'a> {
    /// Text content.
    pub text: &'a str,
    /// Size step.
    pub size: HelperSize,
    /// Tone — Ink for light bands, `OnDark` for dark bands.
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

/// Size step for [`Eyebrow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EyebrowSize {
    /// Above a section heading. 10px, wide tracking.
    #[default]
    Section,
    /// Labels a block *inside* a section. 11px, slightly tighter tracking,
    /// and its own bottom margin.
    ///
    /// Renders as `<h3>`, not `<span>`. This size is used where the label is
    /// the only heading its block has, so emitting a span would delete it
    /// from the document outline and leave a screen-reader user unable to
    /// navigate to it. `Section` sits above a real `<h2>` and is decorative,
    /// so a span is correct there. The size therefore also decides the
    /// element, because in practice the two are the same decision.
    Subhead,
    /// `Section` metrics in the brand navy instead of grey, for a label that
    /// introduces a block the page wants read first.
    ///
    /// This is a named variant rather than a colour prop on purpose. The
    /// component refuses a free colour knob because that is how the eyebrow
    /// drifted below the contrast threshold one page at a time; a fixed second
    /// colour that has been measured keeps the guarantee. `text-primary` is
    /// `#073288`, which sweeps at 11.5:1 on white and 10.6:1 on the
    /// `bg-primary/5` tint it is used over — both far above the 4.5 floor,
    /// and unlike the greys it has headroom to spare.
    Accent,
}

impl EyebrowSize {
    /// Every variant, for tests that must cover all of them.
    ///
    /// Callers iterate this instead of writing their own list. The previous
    /// contrast test hard-coded `[Section, Subhead]`, so a third variant would
    /// have gone untested while the test still reported green.
    pub const ALL: &'static [Self] = &[Self::Section, Self::Subhead, Self::Accent];
}

/// The small capitalised label that introduces a section.
///
/// Extracted because the same class string appeared seventeen times across
/// four pages of plausiden.com, in two sizes, written out by hand each time.
/// That is the shape of a missing primitive: repeated, identical, and load
/// bearing for how the site reads.
///
/// The grey is deliberate and not a free choice. A contrast sweep measured
/// `text-slate-400` at 2.45:1 against a 4.5 requirement at these sizes;
/// `text-slate-500` measures 4.76:1 and is the lightest step that passes.
/// Callers get no colour knob, which is the point — the eyebrow cannot drift
/// back below the threshold one page at a time.
#[derive(Debug, Clone, Copy)]
pub struct Eyebrow<'a> {
    /// Label text. Rendered as written; capitalisation is applied by CSS so
    /// screen readers announce the original casing rather than initialese.
    pub text: &'a str,
    /// Size step.
    pub size: EyebrowSize,
}

impl Eyebrow<'_> {
    /// Render as a `<span>` (Section, Accent) or an `<h3>` (Subhead).
    #[must_use]
    pub fn render(&self) -> Markup {
        match self.size {
            EyebrowSize::Section => {
                let class = "text-[10px] uppercase tracking-[0.2em] font-semibold text-slate-500";
                html! { span class=(class) { (self.text) } }
            }
            EyebrowSize::Accent => {
                let class = "text-[10px] uppercase tracking-[0.2em] font-semibold text-primary";
                html! { span class=(class) { (self.text) } }
            }
            EyebrowSize::Subhead => {
                let class =
                    "text-[11px] uppercase tracking-[0.18em] font-semibold text-slate-500 mb-6";
                html! { h3 class=(class) { (self.text) } }
            }
        }
    }
}

/// One term-and-description row in a hairline definition table.
///
/// The pattern behind the standards table on /services, the report anatomy
/// and finding metadata on /sample-report, and the rate card on /pricing:
/// a term in the first column, a description spanning the rest, separated by
/// a hairline. Nine hand-written copies before this existed.
///
/// Renders `<div><dt><dd>`, so the caller supplies the wrapping `<dl>` and
/// its top border. Keeping the list element outside the row means a caller
/// can put a heading between groups of rows without nesting invalid markup.
#[derive(Debug, Clone, Copy)]
pub struct DefinitionRow<'a> {
    /// The term. Rendered in the first column.
    pub term: &'a str,
    /// The description. Spans the remaining columns from the `md` breakpoint.
    pub description: &'a str,
}

impl DefinitionRow<'_> {
    /// Render one `<div>` containing a `<dt>`/`<dd>` pair.
    #[must_use]
    pub fn render(&self) -> Markup {
        html! {
            div class="grid grid-cols-1 md:grid-cols-3 gap-2 md:gap-7 py-5 border-b border-slate-200" {
                dt class="font-semibold text-slate-900" { (self.term) }
                dd class="md:col-span-2 text-slate-600 text-[15px] md:text-base leading-relaxed font-light" {
                    (self.description)
                }
            }
        }
    }
}

#[cfg(test)]
mod eyebrow_and_definition_tests {
    use super::*;

    /// Every size must carry a colour that has been measured against the 4.5:1
    /// floor. A caller cannot change it, so the only way it regresses is an
    /// edit here.
    ///
    /// The match is deliberately exhaustive and has no catch-all arm: adding a
    /// variant stops this test compiling until whoever added it writes down
    /// which colour it uses. That is the guarantee worth having — not that the
    /// list is complete, but that no variant can reach the site without its
    /// contrast having been stated.
    #[test]
    fn every_eyebrow_colour_has_been_measured() {
        for &size in EyebrowSize::ALL {
            let required = match size {
                // slate-500 measures 4.76:1 on white — the lightest grey that
                // clears the floor at these sizes.
                EyebrowSize::Section | EyebrowSize::Subhead => "text-slate-500",
                // #073288 measures 11.5:1 on white, 10.6:1 on bg-primary/5.
                EyebrowSize::Accent => "text-primary",
            };
            let s = Eyebrow {
                text: "Method",
                size,
            }
            .render()
            .into_string();
            assert!(
                s.contains(required),
                "{size:?} eyebrow dropped its measured colour {required}: {s}"
            );
            assert!(
                !s.contains("text-slate-400") && !s.contains("text-slate-300"),
                "{size:?} eyebrow uses a grey that cannot reach 4.5:1 at this size: {s}"
            );
            assert!(
                s.contains("uppercase"),
                "{size:?} eyebrow is no longer capitalised"
            );
        }
    }

    /// The two sizes must stay distinguishable, or there is one primitive
    /// pretending to be two.
    #[test]
    fn the_two_eyebrow_sizes_differ() {
        let a = Eyebrow {
            text: "x",
            size: EyebrowSize::Section,
        }
        .render()
        .into_string();
        let b = Eyebrow {
            text: "x",
            size: EyebrowSize::Subhead,
        }
        .render()
        .into_string();
        assert_ne!(a, b);
    }

    /// Subhead must stay a heading. It is often the only heading its block
    /// has, so demoting it to a span deletes the block from the document
    /// outline — invisible on screen, and the difference between a
    /// screen-reader user being able to navigate the page or not.
    #[test]
    fn subhead_is_a_heading_and_section_is_not() {
        let sub = Eyebrow {
            text: "Worked example",
            size: EyebrowSize::Subhead,
        }
        .render()
        .into_string();
        assert!(
            sub.starts_with("<h3"),
            "Subhead left the document outline: {sub}"
        );

        let sec = Eyebrow {
            text: "Method",
            size: EyebrowSize::Section,
        }
        .render()
        .into_string();
        assert!(
            sec.starts_with("<span"),
            "Section sits above a real <h2>; a second heading here would \
             duplicate the outline entry: {sec}"
        );
    }

    /// The description column must span, or every row leaves a dead third
    /// column on desktop.
    #[test]
    fn definition_row_description_spans_the_remaining_columns() {
        let s = DefinitionRow {
            term: "PTES",
            description: "Sets the phases.",
        }
        .render()
        .into_string();
        assert!(
            s.contains("md:grid-cols-3"),
            "row is not a three-column grid"
        );
        assert!(s.contains("md:col-span-2"), "description does not span");
        assert!(s.contains("<dt"), "term is not a <dt>");
        assert!(s.contains("<dd"), "description is not a <dd>");
    }
}

/// A section heading inside long-form article prose.
///
/// Distinct from [`Heading`], which sizes headings for marketing pages where
/// a section heading is a landmark. Inside an article the same level needs
/// less weight and a rhythm tuned for reading: space above to separate it
/// from the paragraph before, less below because the paragraph after belongs
/// to it.
///
/// Extracted because all five blog posts wrote the identical class string
/// thirty-four times. `loom report` counts the posts as the largest single
/// cluster of design drift in the site, and this is the biggest share of it.
#[derive(Debug, Clone, Copy)]
pub struct ArticleHeading<'a> {
    /// Heading text.
    pub text: &'a str,
}

impl ArticleHeading<'_> {
    /// Render as `<h2>`.
    ///
    /// Always an `h2`: a post's `<h1>` is its title, rendered by the blog
    /// chrome, so every heading the author writes inside the body sits one
    /// level below it. Making that structural rather than a caller's choice
    /// is what stops a post from skipping a level.
    #[must_use]
    pub fn render(&self) -> Markup {
        html! {
            h2 class="font-display text-2xl md:text-3xl font-bold text-slate-900 mt-12 mb-4" {
                (self.text)
            }
        }
    }
}

#[cfg(test)]
mod article_heading_tests {
    use super::*;

    #[test]
    fn article_heading_is_an_h2_with_reading_rhythm() {
        let s = ArticleHeading {
            text: "What a doctrine is",
        }
        .render()
        .into_string();
        assert!(s.starts_with("<h2"), "article headings must stay h2: {s}");
        assert!(
            s.contains("mt-12"),
            "lost the space that separates it from the previous paragraph"
        );
        assert!(
            s.contains("mb-4"),
            "lost the tighter space binding it to the paragraph it introduces"
        );
        assert!(s.contains("What a doctrine is"));
    }

    /// The rhythm is asymmetric on purpose — more space above than below —
    /// because that is what visually attaches a heading to the text it
    /// introduces rather than the text it follows.
    #[test]
    fn spacing_above_exceeds_spacing_below() {
        let s = ArticleHeading { text: "x" }.render().into_string();
        assert!(
            s.contains("mt-12") && s.contains("mb-4"),
            "heading rhythm changed; above must exceed below: {s}"
        );
    }
}
