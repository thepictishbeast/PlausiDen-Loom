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
}

impl Card<'_> {
    /// Render as a `<div>` wrapper.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = compose_class(self.elevation, self.padding, self.hover);
        html! {
            div class=(class) {
                (PreEscaped(self.body.0.clone()))
            }
        }
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
        let card_class = compose_class(CardElevation::Soft, card_padding, CardHover::Lift);
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
        let class = compose_class(CardElevation::Soft, CardPadding::Roomy, CardHover::Lift);
        html! {
            a href=(self.href) class="group block" {
                article class=(class) {
                    (PreEscaped(self.body.0.clone()))
                }
            }
        }
    }
}

fn compose_class(elev: CardElevation, pad: CardPadding, hover: CardHover) -> String {
    let base = "rounded-xl border bg-white";
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
        }
        .render()
        .into_string();
        assert!(s.contains("<p>hello</p>"));
        assert!(s.contains("rounded-xl"));
        assert!(s.contains("hover:border-primary/40"));
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
        }
        .render()
        .into_string();
        let roomy = Card {
            body: &body,
            elevation: CardElevation::Flat,
            padding: CardPadding::Roomy,
            hover: CardHover::None,
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
        }
        .render()
        .into_string();
        assert!(!s.contains("shadow-"));
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
        }
        .render()
        .into_string();
        assert!(s.contains("shadow-xl"));
    }
}
