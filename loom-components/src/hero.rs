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

// ----------------------------------------------------------------
// `HeroEditorial` — the asymmetric, non-SaaS-trope sibling primitive.
//
// Per the loop preamble's Priority 6 directive (find new culprits;
// hardcoded SaaS-flavor shapes; centered + gradient + animate-fade
// patterns) — `Hero` ships the canonical centered SaaS shape with an
// eyebrow pill, brand-tinted skewed gradient overlay, and three
// stacked fade-in animations. That shape has a place, but it cannot
// be the only shape an operator can compose. `HeroEditorial` is its
// substrate-aware editorial counterpart: 2-column grid on md+,
// headline on the left, caller-supplied decoration slot on the right
// (intended for `Code` terminal blocks, `KvPair` dense info panels,
// `PullQuote` editorial inserts, or other typed primitives). No
// gradient, no eyebrow pill, no fake-fade animations. Kicker is a
// plain uppercase line of metadata, not a pill.
// ----------------------------------------------------------------

/// Decorative background variant for `HeroEditorial`. Minimal by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeroEditorialBackground {
    /// Plain slate-50. The recommended default — lets the content carry the page.
    Slate,
    /// Plain white. Use when the hero leads into a band that has its own surface.
    Plain,
    /// AMOLED true-black (`#000`). Per `[[dark-theme-amoled-true-black]]`
    /// memory — pixels off on OLED, deep editorial.
    Amoled,
}

/// Asymmetric editorial hero.
///
/// The 2-column composition is the substrate's editorial-design
/// answer to the centered SaaS hero — left column carries the
/// language (kicker + headline + lede), right column carries a
/// typed decoration (code shell, kv pair panel, pull quote,
/// picture; the caller composes whichever typed primitive matches
/// the editorial intent).
pub struct HeroEditorial<'a> {
    /// Plain uppercase kicker line above the headline (e.g.,
    /// "DISPATCH · 2026-05-20"). No pill, no border, just metadata.
    pub kicker: Option<&'a str>,
    /// Headline. Renders as the dominant typographic mass on the page.
    pub headline: &'a str,
    /// Optional accent fragment rendered after the headline in the
    /// brand primary color. Use sparingly — accent should be a noun
    /// the page is *about*, not adjectival sales copy.
    pub headline_accent: Option<&'a str>,
    /// Lede paragraph. Encourages long-form editorial copy — there
    /// is no `max-w-2xl` cap, lede sets its own measure.
    pub lede: &'a str,
    /// Optional CTA cluster. Caller composes typed `Button`s.
    pub cta: Option<&'a Markup>,
    /// Right-column decoration slot. Caller composes a typed
    /// primitive — `Code`, `KvPair`, `PullQuote`, `Picture`, etc.
    /// `None` collapses the right column on md+ and the hero
    /// becomes single-column editorial body.
    pub decoration: Option<&'a Markup>,
    /// Background style.
    pub background: HeroEditorialBackground,
}

impl HeroEditorial<'_> {
    /// Render as `<section>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let bg_class = match self.background {
            HeroEditorialBackground::Slate => "bg-slate-50 text-slate-900",
            HeroEditorialBackground::Plain => "bg-white text-slate-900",
            HeroEditorialBackground::Amoled => "bg-black text-slate-100",
        };
        let kicker_tone = match self.background {
            HeroEditorialBackground::Amoled => "text-slate-400",
            _ => "text-slate-500",
        };
        let lede_tone = match self.background {
            HeroEditorialBackground::Amoled => "text-slate-300",
            _ => "text-slate-700",
        };
        let grid_cols = if self.decoration.is_some() {
            // 2-column asymmetric grid on md+. The 3:2 ratio
            // (60/40) keeps the headline dominant.
            "md:grid-cols-[3fr_2fr] md:gap-12"
        } else {
            "md:grid-cols-1"
        };
        html! {
            section class=(format!("relative pt-24 pb-12 md:pt-36 md:pb-20 {bg_class}")) {
                div class="container mx-auto px-4 md:px-6 max-w-6xl" {
                    div class=(format!("grid grid-cols-1 {grid_cols} items-start")) {
                        div class="flex flex-col gap-4 md:gap-6" {
                            @if let Some(kicker) = self.kicker {
                                p class=(format!("text-xs md:text-sm font-mono uppercase tracking-widest {kicker_tone}")) {
                                    (kicker)
                                }
                            }
                            h1 class="font-display text-4xl md:text-5xl lg:text-6xl font-semibold leading-[1.05] tracking-tight" {
                                (self.headline)
                                @if let Some(accent) = self.headline_accent {
                                    " "
                                    span class="text-primary" { (accent) }
                                }
                            }
                            p class=(format!("text-base md:text-lg leading-relaxed max-w-prose {lede_tone}")) {
                                (self.lede)
                            }
                            @if let Some(cta) = self.cta {
                                div class="flex flex-col sm:flex-row gap-3 mt-2" {
                                    (PreEscaped(cta.0.clone()))
                                }
                            }
                        }
                        @if let Some(deco) = self.decoration {
                            div class="mt-8 md:mt-0" {
                                (PreEscaped(deco.0.clone()))
                            }
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

    // ----- HeroEditorial -----

    #[test]
    fn editorial_renders_kicker_when_present() {
        let s = HeroEditorial {
            kicker: Some("DISPATCH · 2026-05-20"),
            headline: "The substrate carries the page",
            headline_accent: None,
            lede: "Editorial composition replaces the centered SaaS shape.",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains(">DISPATCH · 2026-05-20<"));
        assert!(s.contains("font-mono"));
        assert!(s.contains("uppercase"));
        // The kicker is NOT a pill — must not carry rounded-full /
        // bg-primary/10 / border ornaments.
        assert!(!s.contains("rounded-full"));
        assert!(!s.contains("bg-primary/10"));
    }

    #[test]
    fn editorial_omits_kicker_when_none() {
        let s = HeroEditorial {
            kicker: None,
            headline: "Hello",
            headline_accent: None,
            lede: "World",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Plain,
        }
        .render()
        .into_string();
        assert!(!s.contains("uppercase tracking-widest"));
        assert!(s.contains(">Hello"));
    }

    #[test]
    fn editorial_two_column_when_decoration_present() {
        let deco = html! { pre { "code shell here" } };
        let s = HeroEditorial {
            kicker: None,
            headline: "x",
            headline_accent: None,
            lede: "y",
            cta: None,
            decoration: Some(&deco),
            background: HeroEditorialBackground::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("md:grid-cols-[3fr_2fr]"));
        assert!(s.contains(">code shell here<"));
    }

    #[test]
    fn editorial_single_column_without_decoration() {
        let s = HeroEditorial {
            kicker: None,
            headline: "x",
            headline_accent: None,
            lede: "y",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("md:grid-cols-1"));
        assert!(!s.contains("md:grid-cols-[3fr_2fr]"));
    }

    #[test]
    fn editorial_amoled_background_uses_true_black() {
        let s = HeroEditorial {
            kicker: Some("k"),
            headline: "h",
            headline_accent: None,
            lede: "l",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Amoled,
        }
        .render()
        .into_string();
        assert!(s.contains("bg-black"));
        assert!(s.contains("text-slate-100"));
        // Kicker tone shifts darker for AMOLED contrast.
        assert!(s.contains("text-slate-400"));
    }

    #[test]
    fn editorial_no_fade_in_animations() {
        // The SaaS Hero ships `animate-fade-in-up` with delay-1/2/3;
        // HeroEditorial deliberately omits them so reduced-motion
        // users + editorial design intent win by default.
        let s = HeroEditorial {
            kicker: Some("k"),
            headline: "h",
            headline_accent: Some("accent"),
            lede: "l",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Slate,
        }
        .render()
        .into_string();
        assert!(!s.contains("animate-fade-in-up"));
        assert!(!s.contains("delay-1"));
    }

    #[test]
    fn editorial_no_gradient_overlay() {
        // The SaaS Hero emits `linear-gradient(...)` and a skewed
        // brand accent band; HeroEditorial must NOT.
        let s = HeroEditorial {
            kicker: None,
            headline: "x",
            headline_accent: None,
            lede: "y",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Slate,
        }
        .render()
        .into_string();
        assert!(!s.contains("linear-gradient"));
        assert!(!s.contains("skew-x"));
        assert!(!s.contains("from-primary/5"));
    }

    #[test]
    fn editorial_headline_accent_renders_in_primary() {
        let s = HeroEditorial {
            kicker: None,
            headline: "Substrate carries",
            headline_accent: Some("the page"),
            lede: "x",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Plain,
        }
        .render()
        .into_string();
        assert!(s.contains(">Substrate carries"));
        assert!(s.contains("text-primary"));
        assert!(s.contains(">the page<"));
    }

    #[test]
    fn editorial_cta_markup_preserved() {
        let cta = html! { a href="/dispatch" { "Read the dispatch" } };
        let s = HeroEditorial {
            kicker: None,
            headline: "x",
            headline_accent: None,
            lede: "y",
            cta: Some(&cta),
            decoration: None,
            background: HeroEditorialBackground::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains(r#"href="/dispatch""#));
        assert!(s.contains(">Read the dispatch<"));
    }

    #[test]
    fn editorial_lede_uses_max_w_prose_not_max_w_2xl() {
        // SaaS Hero clamps lede width with `max-w-2xl`; the editorial
        // sibling lets prose set its own measure via `max-w-prose`.
        let s = HeroEditorial {
            kicker: None,
            headline: "x",
            headline_accent: None,
            lede: "Long-form editorial prose should set its own measure.",
            cta: None,
            decoration: None,
            background: HeroEditorialBackground::Slate,
        }
        .render()
        .into_string();
        assert!(s.contains("max-w-prose"));
        assert!(!s.contains("max-w-2xl"));
    }
}
