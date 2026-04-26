//! Typed `Button` primitive.

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Visual style. Adding a variant requires a doctrine review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    /// Filled primary CTA.
    Primary,
    /// Outlined secondary CTA.
    Outline,
    /// Outlined CTA with the `success` color (e.g. "Encrypted Inquiry").
    OutlineSuccess,
    /// Ghost (transparent) — used inside dark bands.
    Ghost,
}

/// Button size. Maps to a fixed spacing-scale step + font-size step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonSize {
    /// Small — used in nav.
    Sm,
    /// Medium — most CTAs.
    Md,
    /// Large — hero CTA only.
    Lg,
}

/// A typed button.
///
/// SECURITY: There is no `extra_classes` field. If you find yourself
/// wanting one, the design system has a real gap; extend it, don't
/// route around it.
pub struct Button<'a> {
    /// Visible label text.
    pub label: &'a str,
    /// Visual style.
    pub variant: ButtonVariant,
    /// Physical size.
    pub size: ButtonSize,
    /// Required for `<button>` accessibility — what does this button
    /// announce to assistive tech if its label isn't sufficient?
    /// Pass `None` to use `label` verbatim.
    pub aria_label: Option<&'a str>,
}

impl Button<'_> {
    /// Render as `<button type="button">`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let aria = self.aria_label.unwrap_or(self.label);
        let class = format!(
            "{base} {size} {variant}",
            base = base_classes(),
            size = size_classes(self.size),
            variant = variant_classes(self.variant),
        );
        html! {
            button type="button" class=(class) aria-label=(aria) {
                (self.label)
            }
        }
    }
}

const fn base_classes() -> &'static str {
    // Stable across every button — focus ring, layout, transition.
    "inline-flex items-center justify-center gap-2 whitespace-nowrap font-medium \
     focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring \
     focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 \
     transition-colors"
}

const fn size_classes(s: ButtonSize) -> &'static str {
    match s {
        ButtonSize::Sm => "h-8 px-3 text-xs rounded-md",
        ButtonSize::Md => "h-10 px-4 text-sm rounded-md",
        ButtonSize::Lg => "h-12 px-8 py-6 text-lg rounded-xl",
    }
}

const fn variant_classes(v: ButtonVariant) -> &'static str {
    match v {
        ButtonVariant::Primary => {
            "bg-primary text-primary-foreground border border-primary-border \
             shadow-lg shadow-primary/20 hover:bg-primary/90"
        }
        ButtonVariant::Outline => {
            "bg-white border border-slate-200 text-slate-900 hover:bg-slate-50"
        }
        ButtonVariant::OutlineSuccess => {
            "bg-white border border-emerald-500/50 text-emerald-700 hover:bg-emerald-50"
        }
        ButtonVariant::Ghost => "bg-transparent text-white border border-white/20 hover:bg-white/10",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_md_renders_with_expected_classes() {
        let btn = Button {
            label: "Get a Quote",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            aria_label: None,
        };
        let s = btn.render().into_string();
        assert!(s.contains("bg-primary"));
        assert!(s.contains("h-10"));
        assert!(s.contains(">Get a Quote<"));
        assert!(s.contains(r#"aria-label="Get a Quote""#));
    }

    #[test]
    fn outline_success_uses_emerald() {
        let btn = Button {
            label: "Encrypted Inquiry",
            variant: ButtonVariant::OutlineSuccess,
            size: ButtonSize::Sm,
            aria_label: None,
        };
        let s = btn.render().into_string();
        assert!(s.contains("emerald"));
    }

    #[test]
    fn aria_label_overrides_visible_label() {
        let btn = Button {
            label: "→",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            aria_label: Some("Continue"),
        };
        let s = btn.render().into_string();
        assert!(s.contains(r#"aria-label="Continue""#));
        assert!(s.contains(">→<"));
    }

    #[test]
    fn focus_ring_classes_present_in_every_size() {
        for size in [ButtonSize::Sm, ButtonSize::Md, ButtonSize::Lg] {
            let btn = Button {
                label: "x",
                variant: ButtonVariant::Primary,
                size,
                aria_label: None,
            };
            let s = btn.render().into_string();
            assert!(
                s.contains("focus-visible:ring-2"),
                "missing focus ring at {size:?}"
            );
        }
    }
}
