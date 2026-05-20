//! Typed `Section` primitive — vertical band with consistent spacing.

use maud::{Markup, PreEscaped, html};
use serde::{Deserialize, Serialize};

/// Visual theme for a section band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionTheme {
    /// White background, dark text.
    Light,
    /// Slate-50 (subtle gray) background.
    Muted,
    /// Slate-900 background, white text — used for high-contrast bands.
    Dark,
    /// `primary/5` background — used for the final CTA before footer.
    Tinted,
    /// Press-tier monochrome editorial palette — `slate-100`
    /// background, `slate-900` text. Use for editorial bands
    /// inside a press-themed page (theme="press"). Distinct from
    /// `Muted` because it sits BETWEEN `Light` and `Muted` in
    /// elevation, matching the editorial publication feel
    /// (newspaper-stock background, not corporate-marketing tint).
    Editorial,
    /// True-black AMOLED-friendly dark band. Per memory
    /// [[dark-theme-amoled-true-black]] — bg `#000000` on dark
    /// theme so OLED pixels are off (battery + contrast). Use
    /// for hero bands on dark theme where the substrate doctrine
    /// is "OLED-pixels-off." Distinct from `Dark` (slate-900)
    /// because `Dark` is corporate-tech dark; `Amoled` is true
    /// black for OLED-screen reading economy.
    Amoled,
}

/// Inner-container max-width. Closed enum so callers can't sneak
/// arbitrary widths in.
///
/// Picked from the actual usage spread across plausiden.com today:
/// 2xl for forms + concise text columns, 3xl for article bodies +
/// long prose, 4xl for landing-band copy that wants to feel wider,
/// Default for grids of cards and hero-shaped layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionWidth {
    /// No max-width past the Tailwind default container — the wide
    /// hero / grid shape. The historical default.
    Default,
    /// `max-w-4xl` — landing-band copy.
    Wide,
    /// `max-w-3xl` — article body + long prose.
    Article,
    /// `max-w-2xl` — forms + concise content columns.
    Narrow,
}

/// Vertical padding step. Closed enum mapping to a fixed
/// scale-stop, never an arbitrary px value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionPadding {
    /// `py-8` — tight insets, used for inline-card bands.
    Compact,
    /// `py-10` — editorial-density bands. Sits between Compact
    /// and Tight; designed for editorial pages where the
    /// substrate doctrine is high-density (per
    /// `DensityTier::Dense`). Choose this when the page declares
    /// `[composition] target_density = "dense"` in forge.toml.
    Dense,
    /// `py-12` — form bodies sitting under a hero.
    Tight,
    /// `py-16` — most content sections.
    Default,
    /// `py-20` — landing bands. The historical Loom default.
    Loose,
}

/// A typed page section. Wraps a body with a typed vertical padding,
/// a typed max-width inner container, and a typed background theme.
pub struct Section<'a> {
    /// Body markup. Pre-rendered.
    pub body: &'a Markup,
    /// Theme.
    pub theme: SectionTheme,
    /// Inner container width.
    pub width: SectionWidth,
    /// Vertical padding step.
    pub padding: SectionPadding,
}

impl<'a> Section<'a> {
    /// Convenience constructor — picks the historical default
    /// (Loose padding, Default width). Use the struct-literal form
    /// when overriding either.
    #[must_use]
    pub const fn new(body: &'a Markup, theme: SectionTheme) -> Self {
        Self {
            body,
            theme,
            width: SectionWidth::Default,
            padding: SectionPadding::Loose,
        }
    }

    /// Render as `<section>` element.
    #[must_use]
    pub fn render(&self) -> Markup {
        let theme = theme_classes(self.theme);
        let padding = padding_classes(self.padding);
        let width = width_classes(self.width);
        let outer = format!("{padding} {theme}");
        let inner = format!("container mx-auto px-4 md:px-6 {width}");
        html! {
            section class=(outer.trim()) {
                div class=(inner.trim()) {
                    (PreEscaped(self.body.0.clone()))
                }
            }
        }
    }
}

const fn theme_classes(t: SectionTheme) -> &'static str {
    match t {
        SectionTheme::Light => "bg-white text-slate-900",
        SectionTheme::Muted => "bg-slate-50 text-slate-900",
        SectionTheme::Dark => "bg-slate-900 text-white",
        SectionTheme::Tinted => "bg-primary/5 text-slate-900",
        SectionTheme::Editorial => "bg-slate-100 text-slate-900",
        SectionTheme::Amoled => "bg-black text-slate-100",
    }
}

const fn padding_classes(p: SectionPadding) -> &'static str {
    match p {
        SectionPadding::Compact => "py-8",
        SectionPadding::Dense => "py-10",
        SectionPadding::Tight => "py-12",
        SectionPadding::Default => "py-16",
        SectionPadding::Loose => "py-20",
    }
}

const fn width_classes(w: SectionWidth) -> &'static str {
    match w {
        SectionWidth::Default => "",
        SectionWidth::Wide => "max-w-4xl",
        SectionWidth::Article => "max-w-3xl",
        SectionWidth::Narrow => "max-w-2xl",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_uses_slate_900() {
        let body = html! { p { "x" } };
        let s = Section::new(&body, SectionTheme::Dark)
            .render()
            .into_string();
        assert!(s.contains("bg-slate-900"));
        assert!(s.contains("text-white"));
    }

    #[test]
    fn body_is_preserved() {
        let body = html! { h1 { "Hello" } };
        let s = Section::new(&body, SectionTheme::Light)
            .render()
            .into_string();
        assert!(s.contains("<h1>Hello</h1>"));
    }

    #[test]
    fn default_constructor_emits_loose_padding_and_default_width() {
        let body = html! {};
        let s = Section::new(&body, SectionTheme::Light)
            .render()
            .into_string();
        assert!(s.contains("py-20"), "default padding lost: {s}");
        assert!(!s.contains("max-w-"), "default width should be open: {s}");
    }

    #[test]
    fn narrow_width_emits_max_w_2xl() {
        let body = html! {};
        let s = Section {
            body: &body,
            theme: SectionTheme::Light,
            width: SectionWidth::Narrow,
            padding: SectionPadding::Tight,
        }
        .render()
        .into_string();
        assert!(s.contains("max-w-2xl"), "narrow width missing: {s}");
        assert!(s.contains("py-12"), "tight padding missing: {s}");
    }

    #[test]
    fn editorial_theme_uses_slate_100_bg() {
        let body = html! { p { "x" } };
        let s = Section::new(&body, SectionTheme::Editorial)
            .render()
            .into_string();
        assert!(s.contains("bg-slate-100"), "editorial bg wrong: {s}");
        assert!(s.contains("text-slate-900"), "editorial text wrong: {s}");
        // Verify it's DISTINCT from Muted (slate-50) and Light (white).
        assert!(!s.contains("bg-slate-50"));
        assert!(!s.contains("bg-white"));
    }

    #[test]
    fn amoled_theme_uses_pure_black_for_oled() {
        // Per memory [[dark-theme-amoled-true-black]] — bg must be
        // `bg-black` (resolves to #000000), not `bg-slate-900`
        // (which is a near-black gray). OLED pixels are OFF only
        // for true black.
        let body = html! { p { "x" } };
        let s = Section::new(&body, SectionTheme::Amoled)
            .render()
            .into_string();
        assert!(s.contains("bg-black"), "amoled bg wrong: {s}");
        // Verify it's NOT bg-slate-900 (that's the Dark variant).
        assert!(!s.contains("bg-slate-900"));
        // Text should be slate-100 (high contrast on black).
        assert!(s.contains("text-slate-100"));
    }

    #[test]
    fn dense_padding_is_py_10() {
        // Dense sits between Compact (py-8) and Tight (py-12).
        // Maps semantically to DensityTier::Dense from loom-tokens.
        let body = html! {};
        let s = Section {
            body: &body,
            theme: SectionTheme::Light,
            width: SectionWidth::Default,
            padding: SectionPadding::Dense,
        }
        .render()
        .into_string();
        assert!(s.contains("py-10"), "dense padding missing: {s}");
        // Verify it's distinct from Compact and Tight.
        assert!(!s.contains("py-8"));
        assert!(!s.contains("py-12"));
    }

    #[test]
    fn every_theme_padding_pair_is_consistent() {
        for theme in [
            SectionTheme::Light,
            SectionTheme::Muted,
            SectionTheme::Dark,
            SectionTheme::Tinted,
            SectionTheme::Editorial,
            SectionTheme::Amoled,
        ] {
            for padding in [
                SectionPadding::Compact,
                SectionPadding::Dense,
                SectionPadding::Tight,
                SectionPadding::Default,
                SectionPadding::Loose,
            ] {
                let body = html! {};
                let s = Section {
                    body: &body,
                    theme,
                    width: SectionWidth::Default,
                    padding,
                }
                .render()
                .into_string();
                assert!(
                    s.contains("container mx-auto"),
                    "container missing at {theme:?} / {padding:?}",
                );
            }
        }
    }
}
