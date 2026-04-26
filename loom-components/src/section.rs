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
}

/// A typed page section. Wraps a body with consistent vertical
/// padding, a max-width container, and a typed background theme.
pub struct Section<'a> {
    /// Body markup. Pre-rendered.
    pub body: &'a Markup,
    /// Theme.
    pub theme: SectionTheme,
}

impl Section<'_> {
    /// Render as `<section>` element.
    #[must_use]
    pub fn render(&self) -> Markup {
        let theme = match self.theme {
            SectionTheme::Light => "bg-white text-slate-900",
            SectionTheme::Muted => "bg-slate-50 text-slate-900",
            SectionTheme::Dark => "bg-slate-900 text-white",
            SectionTheme::Tinted => "bg-primary/5 text-slate-900",
        };
        html! {
            section class=(format!("py-20 {theme}")) {
                div class="container mx-auto px-4 md:px-6" {
                    (PreEscaped(self.body.0.clone()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_uses_slate_900() {
        let body = html! { p { "x" } };
        let s = Section {
            body: &body,
            theme: SectionTheme::Dark,
        }
        .render()
        .into_string();
        assert!(s.contains("bg-slate-900"));
        assert!(s.contains("text-white"));
    }

    #[test]
    fn body_is_preserved() {
        let body = html! { h1 { "Hello" } };
        let s = Section {
            body: &body,
            theme: SectionTheme::Light,
        }
        .render()
        .into_string();
        assert!(s.contains("<h1>Hello</h1>"));
    }

    #[test]
    fn every_theme_includes_consistent_padding() {
        for theme in [
            SectionTheme::Light,
            SectionTheme::Muted,
            SectionTheme::Dark,
            SectionTheme::Tinted,
        ] {
            let body = html! {};
            let s = Section { body: &body, theme }.render().into_string();
            assert!(s.contains("py-20"), "padding missing at {theme:?}");
            assert!(s.contains("container mx-auto"), "container missing");
        }
    }
}
