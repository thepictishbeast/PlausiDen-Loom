//! Typed `Hero` primitive — the signature top-of-page band used by
//! every `PlausiDen` landing page (home, vertical landings, blog index,
//! how-we-work, pricing).
//!
//! The shape is intentionally narrow: eyebrow pill, headline (with
//! one optional accent span), subheadline lede, optional CTA cluster
//! beneath. The grid pattern + brand-tinted skewed accent are emitted
//! automatically. A reviewer accepting a new variant of this shape
//! should add a new typed slot, not a `class=` field.

use maud::{Markup, PreEscaped, html};
use serde::{Deserialize, Serialize};

/// Background style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroBackground {
    /// Slate-50 with a faint dot grid + brand-tinted skewed band.
    /// Default for every page that has a hero.
    GridLight,
    /// Plain white. Used when the hero leads into a band that already
    /// has its own decoration.
    Plain,
}

/// A hero section.
pub struct Hero<'a> {
    /// Eyebrow pill text (e.g., "For law firms"). `None` to omit.
    pub eyebrow: Option<&'a str>,
    /// Headline text *before* the optional accent span.
    pub headline_lead: &'a str,
    /// Optional accent span (rendered in primary color). Sits AFTER
    /// `headline_lead` separated by a space.
    pub headline_accent: Option<&'a str>,
    /// Subheadline lede paragraph.
    pub subheadline: &'a str,
    /// Optional CTA cluster — pre-rendered Markup so callers can
    /// compose typed Buttons.
    pub cta: Option<&'a Markup>,
    /// Background style.
    pub background: HeroBackground,
}

impl Hero<'_> {
    /// Render as `<section>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let bg_class = match self.background {
            HeroBackground::GridLight => "bg-slate-50",
            HeroBackground::Plain => "bg-white",
        };
        let show_decoration = matches!(self.background, HeroBackground::GridLight);
        html! {
            section class=(format!("relative pt-32 pb-16 md:pt-44 md:pb-24 overflow-hidden {bg_class}")) {
                @if show_decoration {
                    div class="absolute inset-0 bg-[linear-gradient(to_right,#80808012_1px,transparent_1px),linear-gradient(to_bottom,#80808012_1px,transparent_1px)] bg-[size:24px_24px]" {}
                    div class="absolute top-0 right-0 w-1/3 h-full bg-gradient-to-l from-primary/5 to-transparent skew-x-12 transform origin-top-right translate-x-32" {}
                }
                div class="container relative mx-auto px-4 md:px-6 z-10 max-w-4xl" {
                    @if let Some(eyebrow) = self.eyebrow {
                        span class="inline-block px-4 py-1.5 rounded-full bg-primary/10 text-primary font-semibold text-sm mb-6 border border-primary/20 animate-fade-in-up" {
                            (eyebrow)
                        }
                    }
                    h1 class="font-display text-4xl md:text-5xl lg:text-6xl font-bold text-slate-900 leading-[1.1] mb-6 animate-fade-in-up delay-1" {
                        (self.headline_lead)
                        @if let Some(accent) = self.headline_accent {
                            " "
                            span class="text-primary" { (accent) }
                        }
                    }
                    p class="text-lg md:text-xl text-slate-600 mb-4 max-w-2xl leading-relaxed animate-fade-in-up delay-2" {
                        (self.subheadline)
                    }
                    @if let Some(cta) = self.cta {
                        div class="flex flex-col sm:flex-row gap-4 mt-4 animate-fade-in-up delay-3" {
                            (PreEscaped(cta.0.clone()))
                        }
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
    fn renders_eyebrow_when_present() {
        let s = Hero {
            eyebrow: Some("For law firms"),
            headline_lead: "IT infrastructure",
            headline_accent: Some("you can rest on"),
            subheadline: "Built for the obligation that shapes your practice.",
            cta: None,
            background: HeroBackground::GridLight,
        }
        .render()
        .into_string();
        assert!(s.contains(">For law firms<"));
        assert!(s.contains(">IT infrastructure"));
        assert!(s.contains("text-primary"));
        assert!(s.contains(">you can rest on<"));
    }

    #[test]
    fn omits_eyebrow_when_none() {
        let s = Hero {
            eyebrow: None,
            headline_lead: "Hello",
            headline_accent: None,
            subheadline: "World",
            cta: None,
            background: HeroBackground::Plain,
        }
        .render()
        .into_string();
        assert!(!s.contains("rounded-full"));
        assert!(s.contains(">Hello"));
    }

    #[test]
    fn plain_background_skips_grid_decoration() {
        let s = Hero {
            eyebrow: None,
            headline_lead: "x",
            headline_accent: None,
            subheadline: "y",
            cta: None,
            background: HeroBackground::Plain,
        }
        .render()
        .into_string();
        assert!(!s.contains("linear-gradient"));
        assert!(s.contains("bg-white"));
    }

    #[test]
    fn grid_background_emits_decoration() {
        let s = Hero {
            eyebrow: None,
            headline_lead: "x",
            headline_accent: None,
            subheadline: "y",
            cta: None,
            background: HeroBackground::GridLight,
        }
        .render()
        .into_string();
        assert!(s.contains("linear-gradient"));
        assert!(s.contains("bg-slate-50"));
    }

    #[test]
    fn cta_markup_is_preserved() {
        let cta = html! { a href="/contact" { "Schedule" } };
        let s = Hero {
            eyebrow: None,
            headline_lead: "x",
            headline_accent: None,
            subheadline: "y",
            cta: Some(&cta),
            background: HeroBackground::GridLight,
        }
        .render()
        .into_string();
        assert!(s.contains(r#"href="/contact""#));
        assert!(s.contains(">Schedule<"));
    }

    #[test]
    fn animation_classes_present() {
        let s = Hero {
            eyebrow: Some("x"),
            headline_lead: "y",
            headline_accent: None,
            subheadline: "z",
            cta: None,
            background: HeroBackground::GridLight,
        }
        .render()
        .into_string();
        assert!(s.contains("animate-fade-in-up"));
        assert!(s.contains("delay-1"));
        assert!(s.contains("delay-2"));
    }
}
