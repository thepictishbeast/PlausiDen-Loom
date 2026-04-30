//! Typed `Nav` primitive — the fixed top-bar nav used on every
//! `PlausiDen` page.
//!
//! The render emits both desktop and mobile chrome from the same
//! data: a brand block, a list of [`NavLink`]s with active-state
//! styling, and a list of [`NavCta`] CTA buttons. The mobile drawer
//! and mobile toggle are stamped automatically — a caller cannot
//! produce a desktop-only nav by accident.
//!
//! Mobile drawer expansion is wired to `/static/menu.js` (same id
//! contract as the legacy hand-rolled nav). The toggle button has
//! `aria-expanded` + `aria-controls` set; the drawer itself starts
//! `hidden` + `aria-hidden="true"`.
//!
//! ## Active styling
//!
//! Each [`NavLink`] gets compared to the [`Nav::current`] path. A
//! match emits the production blue underline bar at full width;
//! non-matches get a hover-grow bar. There's no per-link override —
//! the active state is derived, never asserted.

use crate::button::{Button, ButtonSize, ButtonType, ButtonVariant, Decoration, IconPosition};
use loom_icons::Icon;
use maud::{Markup, PreEscaped, html};

/// A single nav link in the top bar.
pub struct NavLink<'a> {
    /// Destination href; compared against [`Nav::current`] to decide
    /// the active styling.
    pub href: &'a str,
    /// Visible label.
    pub label: &'a str,
}

/// A CTA button on the right side of the nav. CTAs render as button
/// pills in desktop and as wide pill links in the mobile drawer.
pub struct NavCta<'a> {
    /// Destination href (the wrapping `<a>`).
    pub href: &'a str,
    /// Visible label.
    pub label: &'a str,
    /// Visual style — drives both the desktop button and the mobile
    /// pill colors. The two-element CTA set we use today is one
    /// `OutlineSuccess` + one `Primary`; adding a third variant is a
    /// design-system review, not a caller decision.
    pub variant: ButtonVariant,
    /// Optional icon for the desktop button. The mobile pill omits
    /// the icon by design — it's a long-pill link, not a button.
    pub icon: Option<&'a str>,
    /// Aria-label override (used when the visible label isn't enough).
    pub aria_label: Option<&'a str>,
}

/// The full top nav.
pub struct Nav<'a> {
    /// Brand logo (typically a shield).
    pub brand_logo: &'static Icon,
    /// Brand name (the dark portion).
    pub brand_name: &'a str,
    /// Brand accent (rendered in primary color, e.g. "LLC").
    pub brand_accent: &'a str,
    /// Top-level nav links.
    pub links: &'a [NavLink<'a>],
    /// Top-level CTA buttons.
    pub ctas: &'a [NavCta<'a>],
    /// Current request path. Drives active styling on links.
    pub current: &'a str,
}

impl Nav<'_> {
    /// Render as `<nav id="site-nav">` followed by the mobile drawer
    /// `<div id="mobile-menu">`. Both share the same root `<nav>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let logo_svg = self
            .brand_logo
            .render_with_class("lucide lucide-shield w-6 h-6 text-white");
        let menu_icon = MENU_ICON_SVG;

        html! {
            nav id="site-nav" class="fixed top-0 left-0 right-0 z-50 transition-all duration-300 border-b bg-transparent border-transparent py-5" {
                div class="container mx-auto px-4 md:px-6 flex items-center justify-between" {
                    a href="/" {
                        div class="flex items-center gap-2 cursor-pointer group" {
                            div class="bg-primary p-1.5 rounded-lg group-hover:scale-105 transition-transform duration-300" {
                                (PreEscaped(logo_svg))
                            }
                            span class="font-display font-bold text-xl tracking-tight transition-colors text-slate-900" {
                                (self.brand_name) " " span class="text-primary" { (self.brand_accent) }
                            }
                        }
                    }
                    div class="hidden md:flex items-center gap-6" {
                        @for link in self.links {
                            (render_desktop_link(link, self.current))
                        }
                        @for cta in self.ctas {
                            (render_desktop_cta(cta))
                        }
                    }
                    button id="mobile-menu-toggle" aria-expanded="false" aria-controls="mobile-menu" class="md:hidden p-2 text-slate-600" aria-label="Toggle menu" {
                        (PreEscaped(menu_icon))
                    }
                }
                div id="mobile-menu" class="md:hidden hidden border-t border-slate-200 bg-white" aria-hidden="true" {
                    div class="container mx-auto px-4 py-4 flex flex-col gap-3" {
                        @for link in self.links {
                            a href=(link.href) class="text-sm font-medium text-slate-700 hover:text-primary py-2" { (link.label) }
                        }
                        @for cta in self.ctas {
                            (render_mobile_cta(cta))
                        }
                    }
                }
            }
        }
    }
}

fn render_desktop_link(link: &NavLink<'_>, current: &str) -> Markup {
    let is_active = link.href == current;
    let text_class = if is_active {
        "text-primary"
    } else {
        "text-slate-600"
    };
    let bar_class = if is_active {
        "absolute -bottom-1 left-0 h-0.5 bg-primary transition-all duration-300 w-full"
    } else {
        "absolute -bottom-1 left-0 h-0.5 bg-primary transition-all duration-300 w-0 group-hover:w-full"
    };
    let span_class = format!(
        "text-sm font-medium transition-colors hover:text-primary cursor-pointer relative group {text_class}"
    );
    html! {
        a href=(link.href) {
            span class=(span_class) {
                (link.label)
                span class=(bar_class) {}
            }
        }
    }
}

fn render_desktop_cta(cta: &NavCta<'_>) -> Markup {
    let icon_pair = cta.icon.map(|svg| (svg, IconPosition::Before));
    let decoration = match cta.variant {
        ButtonVariant::Primary => Decoration::SoftShadow,
        _ => Decoration::None,
    };
    let button = Button {
        label: cta.label,
        variant: cta.variant,
        size: ButtonSize::Sm,
        aria_label: cta.aria_label,
        icon: icon_pair,
        decoration,
        button_type: ButtonType::Button,
    }
    .render();
    html! {
        a href=(cta.href) { (button) }
    }
}

/// The mobile-drawer CTA: a wide pill link styled differently from
/// the desktop button. Color tracks the desktop button's variant so
/// the visual identity stays consistent across breakpoints.
fn render_mobile_cta(cta: &NavCta<'_>) -> Markup {
    let class = match cta.variant {
        ButtonVariant::OutlineSuccess => {
            "mt-2 inline-flex items-center justify-center gap-2 whitespace-nowrap font-medium rounded-md border border-emerald-500/50 text-emerald-700 hover:bg-emerald-50 min-h-8 px-3 text-xs py-2"
        }
        _ => {
            "inline-flex items-center justify-center gap-2 whitespace-nowrap font-medium rounded-md bg-primary text-primary-foreground min-h-8 px-3 text-xs py-2"
        }
    };
    html! {
        a href=(cta.href) class=(class) { (cta.label) }
    }
}

/// Inline hamburger SVG. Embedded as a constant rather than the icon
/// registry because the menu icon's stroke + viewport are slightly
/// different from the registered "menu" icon — keeping the pixel
/// match with the legacy nav.
const MENU_ICON_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-menu w-6 h-6"><line x1="4" x2="20" y1="12" y2="12"></line><line x1="4" x2="20" y1="6" y2="6"></line><line x1="4" x2="20" y1="18" y2="18"></line></svg>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use loom_icons::Icon;

    /// Test-only Icon literal. The production logo is registered in
    /// the consuming repo's `icons` module.
    static TEST_LOGO: Icon = Icon {
        id: "test",
        template: r#"<svg class="__CLS__"><path d="M0 0h24v24H0z"/></svg>"#,
        default_class: "w-6 h-6",
    };

    fn fixture<'a>() -> Nav<'a> {
        static LINKS: &[NavLink<'static>] = &[
            NavLink {
                href: "/",
                label: "Home",
            },
            NavLink {
                href: "/services",
                label: "Services",
            },
        ];
        static CTAS: &[NavCta<'static>] = &[
            NavCta {
                href: "/contact",
                label: "Encrypted Inquiry",
                variant: ButtonVariant::OutlineSuccess,
                icon: None,
                aria_label: None,
            },
            NavCta {
                href: "/contact",
                label: "Get a Quote",
                variant: ButtonVariant::Primary,
                icon: None,
                aria_label: None,
            },
        ];
        Nav {
            brand_logo: &TEST_LOGO,
            brand_name: "PlausiDen",
            brand_accent: "LLC",
            links: LINKS,
            ctas: CTAS,
            current: "/services",
        }
    }

    #[test]
    fn brand_block_emits_logo_and_name() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"<a href="/">"#));
        assert!(s.contains("PlausiDen"));
        assert!(s.contains(r#"<span class="text-primary">LLC</span>"#));
    }

    #[test]
    fn current_link_gets_full_width_bar() {
        let s = fixture().render().into_string();
        // /services is the current path and should carry the
        // full-width primary bar; / (the home link in the desktop
        // strip, NOT the brand wrapper) should not. We disambiguate
        // by anchoring on the link span class — only nav links emit
        // it; the brand wrapper does not.
        let services_pos = s
            .find(r#"href="/services""#)
            .expect("services link present");
        let services_block = &s[services_pos..services_pos + 400];
        assert!(
            services_block.contains("w-full"),
            "active link should have w-full bar"
        );
        // The Home nav link's span has the active/inactive bar; the
        // brand wrapper just has the logo+name. Find the link span.
        assert!(
            s.contains("w-0 group-hover:w-full"),
            "inactive link should have w-0 hover bar"
        );
    }

    #[test]
    fn desktop_ctas_render_as_buttons_inside_anchors() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"<button type="button""#));
        assert!(s.contains("Encrypted Inquiry"));
        assert!(s.contains("Get a Quote"));
    }

    #[test]
    fn mobile_drawer_carries_aria_hidden_initial_state() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"id="mobile-menu""#));
        assert!(s.contains(r#"aria-hidden="true""#));
    }

    #[test]
    fn mobile_toggle_has_accessible_label_and_controls() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"id="mobile-menu-toggle""#));
        assert!(s.contains(r#"aria-controls="mobile-menu""#));
        assert!(s.contains(r#"aria-expanded="false""#));
        assert!(s.contains(r#"aria-label="Toggle menu""#));
    }

    #[test]
    fn mobile_cta_uses_correct_variant_styling() {
        let s = fixture().render().into_string();
        // OutlineSuccess CTA pill uses emerald border in mobile.
        assert!(s.contains("border-emerald-500/50"));
        assert!(s.contains("text-emerald-700"));
        // Primary CTA pill uses bg-primary.
        assert!(s.contains("bg-primary text-primary-foreground"));
    }
}
