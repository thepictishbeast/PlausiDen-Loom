//! Typed `Button` primitive.

use maud::{Markup, PreEscaped, html};
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

/// Where an icon sits relative to the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconPosition {
    /// Icon precedes the label.
    Before,
    /// Icon follows the label.
    After,
}

/// Optional visual decoration. Typed slot so callers can ask for a
/// shadow without reaching for raw class strings. Adding a variant
/// requires a doctrine review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decoration {
    /// No extra decoration.
    None,
    /// Soft brand-tinted shadow — used on hero CTAs.
    SoftShadow,
}

/// Corner / chrome shape. Adding a variant requires a doctrine review.
///
/// `Rounded` is the SaaS-canonical back-compat default (rounded-md
/// for Sm/Md, rounded-xl for Lg). `Square` strips all corner radius
/// — the button reads as a flat editorial-style chip rather than a
/// pill or rounded panel. Pairs with the editorial composition
/// vocabulary (HeroEditorial / KvPairCard / PullQuote display) and
/// with `FormStyle::Editorial` so an editorial page can ship CTAs
/// that don't break the visual register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonShape {
    /// `rounded-md` / `rounded-xl` based on size. Back-compat default.
    #[default]
    Rounded,
    /// `rounded-none` — flat editorial chip across all sizes.
    Square,
}

/// HTML form-association role for a `<button>` element.
///
/// Distinguishes a plain action button (default) from a form submit
/// or form reset. Form submission needs `type="submit"` to fire the
/// surrounding form; CTA buttons that don't sit inside a form should
/// stay `type="button"` so an accidental Enter on a sibling input
/// doesn't double-fire.
///
/// SECURITY: this is a closed enum on purpose. Custom button types
/// don't exist in HTML; if a caller wants a fourth value, the caller
/// is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonType {
    /// `type="button"` — plain action. Default.
    Button,
    /// `type="submit"` — submits the surrounding `<form>`.
    Submit,
    /// `type="reset"` — clears the surrounding `<form>`.
    Reset,
}

impl ButtonType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Submit => "submit",
            Self::Reset => "reset",
        }
    }
}

/// A typed button.
///
/// SECURITY: There is no `extra_classes` field. If you find yourself
/// wanting one, the design system has a real gap; extend it (variant,
/// size, decoration, or icon slot), don't route around it.
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
    /// Optional inline SVG markup, pre-escaped, plus its position
    /// relative to the label. `None` = no icon. The SVG is trusted
    /// content (constants from a `loom-icons` registry, or vetted
    /// inline SVG); never accept this from user input.
    pub icon: Option<(&'a str, IconPosition)>,
    /// Optional visual decoration (shadows etc.).
    pub decoration: Decoration,
    /// HTML form role. Defaults to [`ButtonType::Button`] via the
    /// [`Button::new`] constructor; form submit buttons must set
    /// this to [`ButtonType::Submit`] so the surrounding `<form>`
    /// fires on click.
    pub button_type: ButtonType,
    /// Corner / chrome shape. Defaults to [`ButtonShape::Rounded`].
    pub shape: ButtonShape,
}

impl<'a> Button<'a> {
    /// Convenience constructor — minimal config, no icon, no decoration.
    #[must_use]
    pub const fn new(label: &'a str, variant: ButtonVariant, size: ButtonSize) -> Self {
        Self {
            label,
            variant,
            size,
            aria_label: None,
            icon: None,
            decoration: Decoration::None,
            button_type: ButtonType::Button,
            shape: ButtonShape::Rounded,
        }
    }

    /// Render as `<button>` with the configured `type` attribute.
    #[must_use]
    pub fn render(&self) -> Markup {
        let aria = self.aria_label.unwrap_or(self.label);
        let class = format!(
            "{base} {size} {shape} {variant} {deco}",
            base = base_classes(),
            size = size_classes(self.size),
            shape = shape_classes(self.size, self.shape),
            variant = variant_classes(self.variant),
            deco = decoration_classes(self.decoration),
        );
        let btype = self.button_type.as_str();
        html! {
            button type=(btype) class=(class.trim()) aria-label=(aria) data-loom-button-shape=(shape_attr(self.shape)) {
                @if let Some((svg, IconPosition::Before)) = self.icon {
                    (PreEscaped(svg))
                }
                (self.label)
                @if let Some((svg, IconPosition::After)) = self.icon {
                    (PreEscaped(svg))
                }
            }
        }
    }
}

const fn shape_attr(s: ButtonShape) -> &'static str {
    match s {
        ButtonShape::Rounded => "rounded",
        ButtonShape::Square => "square",
    }
}

/// Radius class based on combined (size, shape). Rounded matches the
/// legacy size-based radius (Sm/Md = `rounded-md`, Lg = `rounded-xl`).
/// Square always strips to `rounded-none`.
const fn shape_classes(s: ButtonSize, shape: ButtonShape) -> &'static str {
    match shape {
        ButtonShape::Rounded => match s {
            ButtonSize::Sm | ButtonSize::Md => "rounded-md",
            ButtonSize::Lg => "rounded-xl",
        },
        ButtonShape::Square => "rounded-none",
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
    // Radius is now owned by shape_classes — see ButtonShape.
    match s {
        ButtonSize::Sm => "h-8 px-3 text-xs",
        ButtonSize::Md => "h-10 px-4 text-sm",
        ButtonSize::Lg => "h-12 px-8 py-6 text-lg",
    }
}

const fn variant_classes(v: ButtonVariant) -> &'static str {
    match v {
        ButtonVariant::Primary => {
            "bg-primary text-primary-foreground border border-primary-border hover:bg-primary/90"
        }
        ButtonVariant::Outline => {
            "bg-white border border-slate-200 text-slate-900 hover:bg-slate-50"
        }
        ButtonVariant::OutlineSuccess => {
            "bg-white border border-emerald-500/50 text-emerald-700 hover:bg-emerald-50"
        }
        ButtonVariant::Ghost => {
            "bg-transparent text-white border border-white/20 hover:bg-white/10"
        }
    }
}

const fn decoration_classes(d: Decoration) -> &'static str {
    match d {
        Decoration::None => "",
        Decoration::SoftShadow => "shadow-lg shadow-primary/20 hover:shadow-xl",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_md_renders_with_expected_classes() {
        let s = Button::new("Get a Quote", ButtonVariant::Primary, ButtonSize::Md)
            .render()
            .into_string();
        assert!(s.contains("bg-primary"));
        assert!(s.contains("h-10"));
        assert!(s.contains(">Get a Quote<"));
        assert!(s.contains(r#"aria-label="Get a Quote""#));
    }

    #[test]
    fn outline_success_uses_emerald() {
        let s = Button::new(
            "Encrypted Inquiry",
            ButtonVariant::OutlineSuccess,
            ButtonSize::Sm,
        )
        .render()
        .into_string();
        assert!(s.contains("emerald"));
    }

    #[test]
    fn aria_label_overrides_visible_label() {
        let btn = Button {
            label: "→",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            aria_label: Some("Continue"),
            icon: None,
            decoration: Decoration::None,
            button_type: ButtonType::Button,
            shape: ButtonShape::default(),
        };
        let s = btn.render().into_string();
        assert!(s.contains(r#"aria-label="Continue""#));
        assert!(s.contains(">→<"));
    }

    #[test]
    fn focus_ring_classes_present_in_every_size() {
        for size in [ButtonSize::Sm, ButtonSize::Md, ButtonSize::Lg] {
            let s = Button::new("x", ButtonVariant::Primary, size)
                .render()
                .into_string();
            assert!(
                s.contains("focus-visible:ring-2"),
                "missing focus ring at {size:?}"
            );
        }
    }

    #[test]
    fn icon_before_renders_before_label() {
        let btn = Button {
            label: "Send",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            aria_label: None,
            icon: Some(("<svg data-test=\"X\"></svg>", IconPosition::Before)),
            decoration: Decoration::None,
            button_type: ButtonType::Button,
            shape: ButtonShape::default(),
        };
        let s = btn.render().into_string();
        // Anchor on inner-content positions, not the aria-label attribute.
        let svg_pos = s.find("svg data-test=\"X\"").unwrap();
        let inner_label_pos = s.find(">Send<").expect("inner label boundary");
        assert!(
            svg_pos < inner_label_pos,
            "svg pos {svg_pos} not before inner label pos {inner_label_pos}"
        );
    }

    #[test]
    fn icon_after_renders_after_label() {
        let btn = Button {
            label: "Next",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            aria_label: None,
            icon: Some(("<svg data-test=\"Y\"></svg>", IconPosition::After)),
            decoration: Decoration::None,
            button_type: ButtonType::Button,
            shape: ButtonShape::default(),
        };
        let s = btn.render().into_string();
        let svg_pos = s.find("svg data-test=\"Y\"").unwrap();
        let inner_label_pos = s.find(">Next<").expect("inner label boundary");
        assert!(
            inner_label_pos < svg_pos,
            "inner label pos {inner_label_pos} not before svg pos {svg_pos}"
        );
    }

    #[test]
    fn soft_shadow_decoration_emits_shadow_classes() {
        let btn = Button {
            label: "x",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Lg,
            aria_label: None,
            icon: None,
            decoration: Decoration::SoftShadow,
            button_type: ButtonType::Button,
            shape: ButtonShape::default(),
        };
        let s = btn.render().into_string();
        assert!(s.contains("shadow-lg"));
        assert!(s.contains("shadow-primary/20"));
    }

    #[test]
    fn no_decoration_emits_no_shadow() {
        let s = Button::new("x", ButtonVariant::Primary, ButtonSize::Md)
            .render()
            .into_string();
        assert!(!s.contains("shadow-lg"));
    }

    #[test]
    fn default_button_type_renders_type_button() {
        let s = Button::new("x", ButtonVariant::Primary, ButtonSize::Md)
            .render()
            .into_string();
        assert!(s.contains(r#"type="button""#));
    }

    #[test]
    fn submit_button_renders_type_submit() {
        let btn = Button {
            label: "Send",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Lg,
            aria_label: None,
            icon: None,
            decoration: Decoration::SoftShadow,
            button_type: ButtonType::Submit,
            shape: ButtonShape::default(),
        };
        let s = btn.render().into_string();
        assert!(
            s.contains(r#"type="submit""#),
            "submit form-action button missing type=submit: {s}"
        );
    }

    #[test]
    fn reset_button_renders_type_reset() {
        let btn = Button {
            label: "Clear",
            variant: ButtonVariant::Outline,
            size: ButtonSize::Md,
            aria_label: None,
            icon: None,
            decoration: Decoration::None,
            button_type: ButtonType::Reset,
            shape: ButtonShape::default(),
        };
        let s = btn.render().into_string();
        assert!(s.contains(r#"type="reset""#));
    }

    // ----- ButtonShape -----

    #[test]
    fn rounded_default_matches_legacy_size_radius() {
        // Sm/Md → rounded-md, Lg → rounded-xl. Back-compat.
        let sm = Button::new("x", ButtonVariant::Primary, ButtonSize::Sm)
            .render()
            .into_string();
        let md = Button::new("x", ButtonVariant::Primary, ButtonSize::Md)
            .render()
            .into_string();
        let lg = Button::new("x", ButtonVariant::Primary, ButtonSize::Lg)
            .render()
            .into_string();
        assert!(sm.contains("rounded-md"));
        assert!(md.contains("rounded-md"));
        assert!(lg.contains("rounded-xl"));
        // Default shape attribute is "rounded".
        assert!(sm.contains(r#"data-loom-button-shape="rounded""#));
    }

    #[test]
    fn square_shape_strips_radius_across_all_sizes() {
        for size in [ButtonSize::Sm, ButtonSize::Md, ButtonSize::Lg] {
            let btn = Button {
                label: "x",
                variant: ButtonVariant::Primary,
                size,
                aria_label: None,
                icon: None,
                decoration: Decoration::None,
                button_type: ButtonType::Button,
                shape: ButtonShape::Square,
            };
            let s = btn.render().into_string();
            assert!(
                s.contains("rounded-none"),
                "missing rounded-none at {size:?}"
            );
            assert!(
                !s.contains("rounded-md"),
                "stale rounded-md at {size:?}: {s}"
            );
            assert!(
                !s.contains("rounded-xl"),
                "stale rounded-xl at {size:?}: {s}"
            );
            assert!(s.contains(r#"data-loom-button-shape="square""#));
        }
    }

    #[test]
    fn square_keeps_size_padding_and_height() {
        // Square only affects radius; padding + height stay
        // size-driven.
        let btn = Button {
            label: "x",
            variant: ButtonVariant::Primary,
            size: ButtonSize::Lg,
            aria_label: None,
            icon: None,
            decoration: Decoration::None,
            button_type: ButtonType::Button,
            shape: ButtonShape::Square,
        };
        let s = btn.render().into_string();
        assert!(s.contains("h-12"));
        assert!(s.contains("px-8"));
        assert!(s.contains("text-lg"));
    }

    #[test]
    fn button_shape_default_is_rounded() {
        assert!(matches!(ButtonShape::default(), ButtonShape::Rounded));
    }
}
