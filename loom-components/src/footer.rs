//! Typed `Footer` primitive — the four-column site footer used by every
//! `PlausiDen` page.
//!
//! Column shape is uniform: heading + list of items. Items can be
//! plain-text labels, navigation links, or contact rows (icon +
//! label, optionally a href). Adding a new item shape is a doctrine
//! review — the four current variants cover every footer use case
//! we've seen.

use loom_icons::Icon;
use maud::{Markup, PreEscaped, html};

/// One item inside a footer column.
pub enum FooterItem<'a> {
    /// Hyperlinked text — used in Company column.
    Link {
        /// Destination href.
        href: &'a str,
        /// Visible label.
        label: &'a str,
    },
    /// Plain text — used in Solutions column for non-clickable
    /// capability names.
    Text {
        /// Visible text.
        text: &'a str,
    },
    /// Icon + label, optionally a href — used in Contact column for
    /// phone / email / location rows.
    Contact {
        /// Loom icon constant.
        icon: &'static Icon,
        /// Display label (phone number, email, address text).
        label: &'a str,
        /// Optional href (`tel:`, `mailto:`, or `None` for plain text).
        href: Option<&'a str>,
    },
}

/// One column.
pub struct FooterColumn<'a> {
    /// Column heading (e.g., "Company").
    pub heading: &'a str,
    /// Column items.
    pub items: &'a [FooterItem<'a>],
}

/// One link in the bottom legal-links row.
pub struct FooterLegalLink<'a> {
    /// Destination href.
    pub href: &'a str,
    /// Visible label.
    pub label: &'a str,
}

/// Visual chrome — drives the brand logo badge style.
///
/// `Standard` keeps the legacy `rounded-lg` logo badge; `Editorial`
/// switches to `rounded-none` for the flat editorial register that
/// pairs with `NavStyle::Editorial`, `ButtonShape::Square`,
/// `CardShape::Square`, etc. A page composing the editorial
/// vocabulary should ship a footer that doesn't break it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FooterStyle {
    /// `rounded-lg` logo badge. Back-compat default.
    #[default]
    Standard,
    /// `rounded-none` logo badge — editorial flat register.
    Editorial,
}

/// The full site footer.
pub struct Footer<'a> {
    /// Brand logo icon (typically a shield).
    pub brand_logo: &'static Icon,
    /// Brand name (`"PlausiDen"`).
    pub brand_name: &'a str,
    /// Brand accent (`"LLC"`) rendered in primary color.
    pub brand_accent: &'a str,
    /// One-paragraph brand tagline.
    pub brand_tagline: &'a str,
    /// Footer columns. Typical: Company / Solutions / Contact.
    pub columns: &'a [FooterColumn<'a>],
    /// Bottom-band copyright text.
    pub copyright: &'a str,
    /// Bottom-band legal links (Privacy, Terms, etc.).
    pub legal_links: &'a [FooterLegalLink<'a>],
    /// Visual chrome style. Defaults to [`FooterStyle::Standard`].
    pub style: FooterStyle,
}

impl Footer<'_> {
    /// Render as `<footer>`.
    #[must_use]
    pub fn render(&self) -> Markup {
        let logo_svg = self.brand_logo.render_with_class("w-6 h-6 text-white");
        let logo_class = match self.style {
            FooterStyle::Standard => "bg-primary p-1.5 rounded-lg",
            FooterStyle::Editorial => "bg-primary p-1.5 rounded-none",
        };
        let style_attr = match self.style {
            FooterStyle::Standard => "standard",
            FooterStyle::Editorial => "editorial",
        };
        html! {
            footer class="bg-slate-900 text-slate-300 py-16" data-loom-footer-style=(style_attr) {
                div class="container mx-auto px-4 md:px-6" {
                    div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-12" {
                        // Brand column
                        div class="space-y-6" {
                            div class="flex items-center gap-3 text-white" {
                                div class=(logo_class) {
                                    (PreEscaped(logo_svg))
                                }
                                span class="font-display font-bold text-xl tracking-tight whitespace-nowrap" {
                                    (self.brand_name) " "
                                    span class="text-primary" { (self.brand_accent) }
                                }
                            }
                            p class="text-slate-400 text-sm leading-relaxed max-w-xs" {
                                (self.brand_tagline)
                            }
                        }

                        // Other columns
                        @for col in self.columns {
                            (column(col))
                        }
                    }

                    // Bottom band
                    div class="mt-16 pt-8 border-t border-slate-800 flex flex-col md:flex-row justify-between items-center gap-4 text-xs text-slate-500" {
                        p { (self.copyright) }
                        div class="flex gap-6" {
                            @for link in self.legal_links {
                                a href=(link.href) {
                                    span class="hover:text-white transition-colors cursor-pointer" {
                                        (link.label)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn column(col: &FooterColumn<'_>) -> Markup {
    html! {
        div class="space-y-6" {
            h3 class="text-white font-display font-semibold text-lg" { (col.heading) }
            ul class={ "space-y-3" @if any_contact(col.items) { " space-y-4" } } {
                @for item in col.items {
                    (item_li(item))
                }
            }
        }
    }
}

fn any_contact(items: &[FooterItem<'_>]) -> bool {
    items
        .iter()
        .any(|i| matches!(i, FooterItem::Contact { .. }))
}

fn item_li(item: &FooterItem<'_>) -> Markup {
    match item {
        FooterItem::Link { href, label } => html! {
            li {
                a href=(*href) {
                    span class="text-sm hover:text-white transition-colors cursor-pointer whitespace-nowrap" {
                        (*label)
                    }
                }
            }
        },
        FooterItem::Text { text } => html! {
            li class="text-sm whitespace-nowrap" { (*text) }
        },
        FooterItem::Contact { icon, label, href } => {
            let svg = icon.render_with_class("w-5 h-5 text-primary shrink-0 mt-0.5");
            html! {
                li class="flex items-start gap-3" {
                    (PreEscaped(svg))
                    @match href {
                        Some(h) => a href=(*h) {
                            span class="text-sm hover:text-white transition-colors" { (*label) }
                        },
                        None => span class="text-sm" { (*label) },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture<'a>() -> Footer<'a> {
        const COMPANY: &[FooterItem<'static>] = &[
            FooterItem::Link {
                href: "/",
                label: "Home",
            },
            FooterItem::Link {
                href: "/about",
                label: "About",
            },
        ];
        const SOLUTIONS: &[FooterItem<'static>] = &[
            FooterItem::Text {
                text: "IT Operations",
            },
            FooterItem::Text {
                text: "Cyber Security",
            },
        ];
        static CONTACT: &[FooterItem<'static>] = &[
            FooterItem::Contact {
                icon: &loom_icons::PHONE,
                label: "555-1234",
                href: Some("tel:5551234"),
            },
            FooterItem::Contact {
                icon: &loom_icons::MAP_PIN,
                label: "Massachusetts, USA",
                href: None,
            },
        ];
        static COLS: &[FooterColumn<'static>] = &[
            FooterColumn {
                heading: "Company",
                items: COMPANY,
            },
            FooterColumn {
                heading: "Solutions",
                items: SOLUTIONS,
            },
            FooterColumn {
                heading: "Contact",
                items: CONTACT,
            },
        ];
        static LEGAL: &[FooterLegalLink<'static>] = &[
            FooterLegalLink {
                href: "/privacy",
                label: "Privacy",
            },
            FooterLegalLink {
                href: "/terms",
                label: "Terms",
            },
        ];
        Footer {
            brand_logo: &loom_icons::SHIELD,
            brand_name: "PlausiDen",
            brand_accent: "LLC",
            brand_tagline: "Test tagline.",
            columns: COLS,
            copyright: "© PlausiDen LLC.",
            legal_links: LEGAL,
            style: FooterStyle::default(),
        }
    }

    #[test]
    fn renders_brand_block() {
        let s = fixture().render().into_string();
        assert!(s.contains("<footer"));
        assert!(s.contains("PlausiDen"));
        assert!(s.contains(">LLC<"));
        assert!(s.contains("Test tagline"));
    }

    #[test]
    fn link_items_emit_anchors() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"<a href="/""#));
        assert!(s.contains(r#"<a href="/about""#));
    }

    #[test]
    fn text_items_emit_plain_li() {
        let s = fixture().render().into_string();
        // Plain solutions text — IT Operations should be in an li but
        // not wrapped in an <a>.
        let pos = s.find("IT Operations").expect("IT Operations present");
        // Look backward for the nearest <a — must NOT be the most
        // recent open tag wrapping IT Operations.
        let preceding = &s[..pos];
        let last_a = preceding.rfind("<a ");
        let last_li = preceding.rfind("<li").expect("li open");
        if let Some(a_pos) = last_a {
            assert!(a_pos < last_li, "Text item must not be wrapped in <a>");
        }
    }

    #[test]
    fn contact_items_with_href_emit_anchor() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"href="tel:5551234""#));
    }

    #[test]
    fn contact_items_without_href_emit_span() {
        let s = fixture().render().into_string();
        assert!(s.contains(">Massachusetts, USA<"));
        // Massachusetts label should not be inside an anchor.
        let pos = s.find("Massachusetts").expect("present");
        let preceding = &s[..pos];
        let last_a_open = preceding.rfind("<a ");
        let last_a_close = preceding.rfind("</a>");
        // If there is an unmatched <a> before our text, fail.
        if let Some(open) = last_a_open {
            let close = last_a_close.unwrap_or(0);
            assert!(close > open, "Massachusetts must not be inside <a>");
        }
    }

    #[test]
    fn legal_links_render_in_bottom_band() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"href="/privacy""#));
        assert!(s.contains(r#"href="/terms""#));
        assert!(s.contains("PlausiDen LLC"));
    }

    #[test]
    fn default_style_emits_rounded_logo_badge() {
        let s = fixture().render().into_string();
        assert!(s.contains("bg-primary p-1.5 rounded-lg"));
        assert!(s.contains(r#"data-loom-footer-style="standard""#));
    }

    #[test]
    fn editorial_style_emits_rounded_none_logo_badge() {
        let s = Footer {
            style: FooterStyle::Editorial,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(s.contains("bg-primary p-1.5 rounded-none"));
        assert!(!s.contains("bg-primary p-1.5 rounded-lg"));
        assert!(s.contains(r#"data-loom-footer-style="editorial""#));
    }

    #[test]
    fn footer_style_default_is_standard() {
        assert!(matches!(FooterStyle::default(), FooterStyle::Standard));
    }
}
