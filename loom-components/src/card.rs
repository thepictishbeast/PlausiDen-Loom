//! Typed `Card` primitive — content container with consistent
//! border / radius / padding / hover treatment.
//!
//! Three composition shapes:
//! - [`Card`] — generic content wrapper. Body Markup is yours.
//! - [`FeatureCard`] — feature card with icon + title + description
//!   (the shape used on `/solutions/legal` capability grid).
//! - [`LinkCard`] — clickable card that wraps an `<a>`. Used on
//!   the blog index for post previews.

use maud::{Markup, PreEscaped, html};
use serde::{Deserialize, Serialize};

/// Visual elevation. Maps to a fixed shadow + hover-shadow pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardElevation {
    /// No shadow, just a border. Used in dense grids.
    Flat,
    /// Subtle shadow that grows on hover. Default for feature cards.
    Soft,
    /// Pronounced shadow. Used for hero CTAs / form panels.
    Pronounced,
}

/// Padding density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardPadding {
    /// Compact (1rem) — list rows, badges.
    Tight,
    /// Default (1.5rem) — most cards.
    Comfortable,
    /// Generous (2rem) — feature cards.
    Roomy,
}

/// Hover behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardHover {
    /// No hover treatment.
    None,
    /// Border tints, slight lift, deeper shadow.
    Lift,
}

/// Corner / chrome shape. `Rounded` is the SaaS-canonical back-compat
/// default (`rounded-xl`); `Square` strips to `rounded-none` for the
/// flat editorial composition that pairs with `ButtonShape::Square`,
/// `ModalShape::Square`, `ToastShape::Square`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardShape {
    /// `rounded-xl` SaaS card. Back-compat default.
    #[default]
    Rounded,
    /// `rounded-none` flat editorial panel.
    Square,
}

/// Content card — pass arbitrary inner markup.
pub struct Card<'a> {
    /// Inner content. Pre-rendered.
    pub body: &'a Markup,
    /// Elevation tier.
    pub elevation: CardElevation,
    /// Padding density.
    pub padding: CardPadding,
    /// Hover behavior.
    pub hover: CardHover,
    /// Corner shape. Defaults to [`CardShape::Rounded`].
    pub shape: CardShape,
}

impl Card<'_> {
    /// Render as a `<div>` wrapper.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = compose_class(self.elevation, self.padding, self.hover, self.shape);
        let shape_attr = card_shape_attr(self.shape);
        html! {
            div class=(class) data-loom-card-shape=(shape_attr) {
                (PreEscaped(self.body.0.clone()))
            }
        }
    }
}

const fn card_shape_attr(s: CardShape) -> &'static str {
    match s {
        CardShape::Rounded => "rounded",
        CardShape::Square => "square",
    }
}

/// Visual style for [`FeatureCard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureCardStyle {
    /// Compact icon tile (`w-12 h-12 bg-primary/10`), comfortable
    /// padding. Used on vertical landing pages, in dense capability
    /// grids.
    Subtle,
    /// Larger icon tile (`w-14 h-14 bg-primary/5`) that flips to
    /// brand color on `group-hover`. Used on the home services grid
    /// and the services page where the surface invites interaction.
    Bold,
}

/// Feature card: icon + title + description. Common shape on
/// vertical landing pages and "Everything Your Business Needs"
/// home grid.
pub struct FeatureCard<'a> {
    /// Inline SVG (`PreEscaped`, trusted source).
    pub icon_svg: &'a str,
    /// Title text.
    pub title: &'a str,
    /// Body description.
    pub description: &'a str,
}

impl FeatureCard<'_> {
    /// Render with the default subtle style.
    #[must_use]
    pub fn render(&self) -> Markup {
        self.render_with_style(FeatureCardStyle::Subtle)
    }

    /// Render with explicit style.
    #[must_use]
    #[allow(clippy::similar_names)] // tile_class / title_class / body_class are clear in context
    pub fn render_with_style(&self, style: FeatureCardStyle) -> Markup {
        let (card_padding, tile_class, title_class, body_class) = match style {
            FeatureCardStyle::Subtle => (
                CardPadding::Comfortable,
                "bg-primary/10 w-12 h-12 rounded-lg flex items-center justify-center mb-4",
                "font-display text-xl font-bold text-slate-900 mb-2",
                "text-slate-600 text-sm leading-relaxed",
            ),
            FeatureCardStyle::Bold => (
                CardPadding::Roomy,
                "bg-primary/5 w-14 h-14 rounded-2xl flex items-center justify-center mb-6 \
                 group-hover:bg-primary group-hover:text-white transition-colors duration-300",
                "font-display text-xl font-bold text-slate-900 mb-3",
                "text-slate-600 leading-relaxed",
            ),
        };
        let card_class = compose_class(
            CardElevation::Soft,
            card_padding,
            CardHover::Lift,
            CardShape::Rounded,
        );
        // Bold style hooks on `.group` so its child can `group-hover:`.
        let outer_class = match style {
            FeatureCardStyle::Subtle => card_class,
            FeatureCardStyle::Bold => format!("{card_class} group"),
        };
        html! {
            div class=(outer_class) {
                div class=(tile_class) {
                    (PreEscaped(self.icon_svg))
                }
                h3 class=(title_class) {
                    (self.title)
                }
                p class=(body_class) {
                    (self.description)
                }
            }
        }
    }
}

/// Clickable card that wraps an `<a href>`. Used on blog index +
/// case-study lists.
pub struct LinkCard<'a> {
    /// Destination href.
    pub href: &'a str,
    /// Inner content.
    pub body: &'a Markup,
}

impl LinkCard<'_> {
    /// Render as `<a><div>...</div></a>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = compose_class(
            CardElevation::Soft,
            CardPadding::Roomy,
            CardHover::Lift,
            CardShape::Rounded,
        );
        html! {
            a href=(self.href) class="group block" {
                article class=(class) {
                    (PreEscaped(self.body.0.clone()))
                }
            }
        }
    }
}

fn compose_class(
    elev: CardElevation,
    pad: CardPadding,
    hover: CardHover,
    shape: CardShape,
) -> String {
    // Radius is now shape-driven; the base keeps only border + surface.
    let base = "border bg-white";
    let radius = match shape {
        CardShape::Rounded => "rounded-xl",
        CardShape::Square => "rounded-none",
    };
    let border_color = "border-slate-200";
    let shadow = match elev {
        CardElevation::Flat => "",
        CardElevation::Soft => "shadow-sm",
        CardElevation::Pronounced => "shadow-xl",
    };
    let padding = match pad {
        CardPadding::Tight => "p-4",
        CardPadding::Comfortable => "p-6",
        CardPadding::Roomy => "p-6 md:p-8",
    };
    let hover_classes = match hover {
        CardHover::None => "",
        CardHover::Lift => {
            "transition-all hover:border-primary/40 hover:shadow-lg hover:-translate-y-0.5"
        }
    };
    let mut s = String::with_capacity(160);
    s.push_str(radius);
    s.push(' ');
    s.push_str(base);
    s.push(' ');
    s.push_str(border_color);
    if !shadow.is_empty() {
        s.push(' ');
        s.push_str(shadow);
    }
    s.push(' ');
    s.push_str(padding);
    if !hover_classes.is_empty() {
        s.push(' ');
        s.push_str(hover_classes);
    }
    s
}

// ----------------------------------------------------------------
// `KvPairCard` — editorial dense info-panel sibling of `FeatureCard`.
//
// The loop preamble's Priority 2 calls out 3-column `FeatureCard`
// grids (icon + title + description) as a canonical SaaS trope.
// `FeatureCard` keeps its place for sites whose voice is properly
// "feature-led product marketing." `KvPairCard` is the editorial
// alternative: monospace label up top, dominant fact value in the
// middle, optional source / footnote underneath. Repeating these
// 3-across reads as a data dispatch rather than a feature spotlight.
//
// Shape guarantees enforced by tests:
// * No icon tile (no `w-12 h-12 bg-primary/10 rounded-lg`).
// * No `rounded-2xl` ornament — uses the standard card radius.
// * No `group-hover` color-flip animation.
// * Monospace label uppercase + tracked.
// * Value carries the typographic mass; label and source are subordinate.
// ----------------------------------------------------------------

/// Layout density for [`KvPairCard`]. Affects vertical rhythm
/// inside the card; horizontal sizing is the grid's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KvPairDensity {
    /// Tight rhythm — for grids of 4-6 cards with short values.
    Compact,
    /// Default — most KV grids.
    Comfortable,
    /// Generous — for grids of 2-3 cards with long values.
    Spacious,
}

/// Visual tone for [`KvPairCard`]. Affects label / value / source
/// color treatment per theme; the layout shape is identical
/// across tones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KvPairTone {
    /// Slate text on white / slate-50 surface (default).
    Slate,
    /// Slate-100 text on AMOLED true-black surface. Per
    /// `[[dark-theme-amoled-true-black]]` memory.
    Amoled,
}

/// Dense editorial info card. Label / value / optional source.
///
/// Repeats N-across in a grid to compose a data dispatch:
///
/// ```text
/// │ STARTED        │ JURISDICTION   │ INSTRUMENTS  │
/// │ 2019           │ Federal+47     │ ML-DSA 65,87 │
/// │ — incorp. CT   │ — extraterr.   │ — postquant. │
/// ```
pub struct KvPairCard<'a> {
    /// Small uppercase monospace label (e.g., "JURISDICTION").
    pub label: &'a str,
    /// The dominant fact. Renders as the card's typographic mass.
    /// Long strings will wrap; the grid clamps width.
    pub value: &'a str,
    /// Optional footnote / source / annotation. Renders as a
    /// subdued line beneath value, prefixed visually with an
    /// em-dash to mark it as commentary.
    pub source: Option<&'a str>,
    /// Layout density.
    pub density: KvPairDensity,
    /// Color tone.
    pub tone: KvPairTone,
}

impl KvPairCard<'_> {
    /// Render as a `<div>` cell suitable for grid composition.
    #[must_use]
    pub fn render(&self) -> Markup {
        let (gap, padding) = match self.density {
            KvPairDensity::Compact => ("gap-1", "p-4"),
            KvPairDensity::Comfortable => ("gap-2", "p-5"),
            KvPairDensity::Spacious => ("gap-3", "p-6 md:p-8"),
        };
        let (surface, border, label_tone, value_tone, source_tone) = match self.tone {
            KvPairTone::Slate => (
                "bg-white",
                "border-slate-200",
                "text-slate-500",
                "text-slate-900",
                "text-slate-600",
            ),
            KvPairTone::Amoled => (
                "bg-black",
                "border-slate-800",
                "text-slate-400",
                "text-slate-100",
                "text-slate-400",
            ),
        };
        let outer = format!("flex flex-col {gap} border {border} {surface} {padding}");
        let label_class = format!("text-xs font-mono uppercase tracking-widest {label_tone}");
        let value_class =
            format!("font-display text-2xl md:text-3xl font-semibold leading-tight {value_tone}");
        let source_class = format!("text-sm leading-snug {source_tone}");
        html! {
            div class=(outer) {
                p class=(label_class) {
                    (self.label)
                }
                p class=(value_class) {
                    (self.value)
                }
                @if let Some(source) = self.source {
                    p class=(source_class) {
                        "— "
                        (source)
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
    fn content_card_preserves_body() {
        let body = html! { p { "hello" } };
        let s = Card {
            body: &body,
            elevation: CardElevation::Soft,
            padding: CardPadding::Comfortable,
            hover: CardHover::Lift,
            shape: CardShape::default(),
        }
        .render()
        .into_string();
        assert!(s.contains("<p>hello</p>"));
        assert!(s.contains("rounded-xl"));
        assert!(s.contains("hover:border-primary/40"));
        // Default shape is Rounded — data attribute reflects that.
        assert!(s.contains(r#"data-loom-card-shape="rounded""#));
    }

    #[test]
    fn feature_card_emits_icon_title_description() {
        let s = FeatureCard {
            icon_svg: "<svg data-test=\"f\"></svg>",
            title: "Confidential email",
            description: "Self-hosted mail with TLS-required transport.",
        }
        .render()
        .into_string();
        assert!(s.contains("svg data-test=\"f\""));
        assert!(s.contains(">Confidential email<"));
        assert!(s.contains(">Self-hosted mail"));
    }

    #[test]
    fn link_card_wraps_in_anchor() {
        let body = html! { h2 { "Post title" } };
        let s = LinkCard {
            href: "/blog/x",
            body: &body,
        }
        .render()
        .into_string();
        assert!(s.contains(r#"<a href="/blog/x" class="group block""#));
        assert!(s.contains("<article"));
        assert!(s.contains("<h2>Post title</h2>"));
    }

    #[test]
    fn padding_levels_produce_distinct_classes() {
        let body = html! {};
        let tight = Card {
            body: &body,
            elevation: CardElevation::Flat,
            padding: CardPadding::Tight,
            hover: CardHover::None,
            shape: CardShape::default(),
        }
        .render()
        .into_string();
        let roomy = Card {
            body: &body,
            elevation: CardElevation::Flat,
            padding: CardPadding::Roomy,
            hover: CardHover::None,
            shape: CardShape::default(),
        }
        .render()
        .into_string();
        assert!(tight.contains("p-4"));
        assert!(roomy.contains("p-6 md:p-8"));
    }

    #[test]
    fn flat_elevation_has_no_shadow() {
        let body = html! {};
        let s = Card {
            body: &body,
            elevation: CardElevation::Flat,
            padding: CardPadding::Comfortable,
            hover: CardHover::None,
            shape: CardShape::default(),
        }
        .render()
        .into_string();
        assert!(!s.contains("shadow-"));
    }

    #[test]
    fn square_shape_strips_radius() {
        let body = html! {};
        let s = Card {
            body: &body,
            elevation: CardElevation::Flat,
            padding: CardPadding::Comfortable,
            hover: CardHover::None,
            shape: CardShape::Square,
        }
        .render()
        .into_string();
        assert!(s.contains("rounded-none"));
        assert!(!s.contains("rounded-xl"));
        assert!(s.contains(r#"data-loom-card-shape="square""#));
    }

    #[test]
    fn square_keeps_border_and_surface() {
        // Square only affects radius — border + bg-white stay.
        let body = html! {};
        let s = Card {
            body: &body,
            elevation: CardElevation::Soft,
            padding: CardPadding::Comfortable,
            hover: CardHover::None,
            shape: CardShape::Square,
        }
        .render()
        .into_string();
        assert!(s.contains("border"));
        assert!(s.contains("bg-white"));
        assert!(s.contains("shadow-sm"));
    }

    #[test]
    fn card_shape_default_is_rounded() {
        assert!(matches!(CardShape::default(), CardShape::Rounded));
    }

    #[test]
    fn feature_card_bold_uses_larger_tile_with_group_hover() {
        let s = FeatureCard {
            icon_svg: "<svg/>",
            title: "T",
            description: "D",
        }
        .render_with_style(FeatureCardStyle::Bold)
        .into_string();
        assert!(s.contains("w-14 h-14"));
        assert!(s.contains("bg-primary/5"));
        assert!(s.contains("group-hover:bg-primary"));
        assert!(
            s.contains(" group"),
            "outer card needs `group` for group-hover children"
        );
    }

    #[test]
    fn feature_card_subtle_uses_compact_tile() {
        let s = FeatureCard {
            icon_svg: "<svg/>",
            title: "T",
            description: "D",
        }
        .render_with_style(FeatureCardStyle::Subtle)
        .into_string();
        assert!(s.contains("w-12 h-12"));
        assert!(s.contains("bg-primary/10"));
        assert!(!s.contains("group-hover:bg-primary"));
    }

    #[test]
    fn pronounced_elevation_emits_shadow_xl() {
        let body = html! {};
        let s = Card {
            body: &body,
            elevation: CardElevation::Pronounced,
            padding: CardPadding::Roomy,
            hover: CardHover::None,
            shape: CardShape::default(),
        }
        .render()
        .into_string();
        assert!(s.contains("shadow-xl"));
    }

    // ----- KvPairCard -----

    #[test]
    fn kvpair_renders_label_value_no_source() {
        let s = KvPairCard {
            label: "JURISDICTION",
            value: "Federal+47",
            source: None,
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains(">JURISDICTION<"));
        assert!(s.contains(">Federal+47<"));
        // Label is monospace-uppercase-tracked, never a pill / border-pill.
        assert!(s.contains("font-mono"));
        assert!(s.contains("uppercase"));
        assert!(s.contains("tracking-widest"));
    }

    #[test]
    fn kvpair_renders_source_when_present() {
        let s = KvPairCard {
            label: "STARTED",
            value: "2019",
            source: Some("incorp. CT"),
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains(">— incorp. CT<"));
    }

    #[test]
    fn kvpair_omits_source_when_none() {
        let s = KvPairCard {
            label: "L",
            value: "V",
            source: None,
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        // The em-dash prefix only appears when source is rendered.
        assert!(!s.contains("— "));
    }

    #[test]
    fn kvpair_no_saas_trope_ornaments() {
        // Shape guarantee: no icon tile, no rounded-2xl flourish,
        // no group-hover color flip. Repeating this primitive across
        // a grid must NOT look like FeatureCard.
        let s = KvPairCard {
            label: "L",
            value: "V",
            source: Some("s"),
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        assert!(!s.contains("rounded-2xl"));
        assert!(!s.contains("rounded-lg"));
        assert!(!s.contains("bg-primary/10"));
        assert!(!s.contains("bg-primary/5"));
        assert!(!s.contains("group-hover"));
        assert!(!s.contains("w-12 h-12"));
        assert!(!s.contains("w-14 h-14"));
        assert!(!s.contains("shadow-"));
    }

    #[test]
    fn kvpair_compact_density_smaller_gap() {
        let s = KvPairCard {
            label: "L",
            value: "V",
            source: None,
            density: KvPairDensity::Compact,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("gap-1"));
        assert!(s.contains("p-4"));
    }

    #[test]
    fn kvpair_spacious_density_larger_padding() {
        let s = KvPairCard {
            label: "L",
            value: "V",
            source: None,
            density: KvPairDensity::Spacious,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("gap-3"));
        assert!(s.contains("p-6 md:p-8"));
    }

    #[test]
    fn kvpair_amoled_uses_true_black_surface() {
        let s = KvPairCard {
            label: "L",
            value: "V",
            source: None,
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Amoled,
        }
        .render()
        .into_string();
        assert!(s.contains("bg-black"));
        // Slate-100 for value text on dark background per
        // [[dark-theme-amoled-true-black]] palette.
        assert!(s.contains("text-slate-100"));
        // Slate-100 should NOT appear in light tone.
        let light = KvPairCard {
            label: "L",
            value: "V",
            source: None,
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        assert!(!light.contains("bg-black"));
        assert!(light.contains("bg-white"));
    }

    #[test]
    fn kvpair_value_dominates_typographically() {
        // The value carries the typographic mass; label + source are
        // subordinate text sizes. Verify the sizes match the design.
        let s = KvPairCard {
            label: "JURISDICTION",
            value: "Federal+47",
            source: Some("extraterr."),
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Slate,
        }
        .render()
        .into_string();
        // Value: text-2xl on mobile, text-3xl on md+.
        assert!(s.contains("text-2xl"));
        assert!(s.contains("md:text-3xl"));
        // Label: small (text-xs).
        assert!(s.contains("text-xs"));
        // Source: medium (text-sm).
        assert!(s.contains("text-sm"));
    }
}
