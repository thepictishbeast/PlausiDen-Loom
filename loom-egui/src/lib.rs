//! Loom — egui (immediate-mode) renderer.
//!
//! Renders a typed [`loom_cms_render::CmsPage`] into an
//! [`egui::Ui`] so the same typed content can drive a native
//! desktop / mobile / web-canvas app via egui — without
//! re-authoring the page.
//!
//! ## Scope
//!
//! Pure draw function: `show_page(ui, page)` issues the egui
//! widget calls. This crate intentionally does NOT:
//!
//! - own a window
//! - drive eframe / glow / wgpu / winit
//! - handle keystrokes or scroll
//! - lay out beyond what egui's vertical/horizontal helpers
//!   already do
//!
//! A downstream binary picks the window/event backend, then
//! each frame calls `loom_egui::show_page(ui, &page)`.
//!
//! ## Coverage
//!
//! Renders the same Tier-2 editorial primitives + Tier-1
//! marketing primitives' textual content that `loom-tui` covers
//! (#120). Graphical primitives emit a single dimmed
//! placeholder so the UI conveys "an image goes here" without
//! requiring the renderer to know how to load pixels.
//!
//! Closes #121.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use egui::{RichText, Ui};
use loom_cms_render::{CmsPage, CmsSection, HeadingLevel, HeroCta};

/// Render a `CmsPage` into the given egui `Ui`. Call this once
/// per frame from the host app's update closure.
pub fn show_page(ui: &mut Ui, page: &CmsPage) {
    ui.heading(&page.title);
    ui.add_space(8.0);
    for (i, section) in page.sections.iter().enumerate() {
        if i > 0 {
            ui.add_space(12.0);
        }
        show_section(ui, section);
    }
}

/// Render a single section. Public so callers can render
/// out of normal flow (e.g. a preview pane showing one
/// section at a time).
pub fn show_section(ui: &mut Ui, section: &CmsSection) {
    match section {
        CmsSection::Paragraph { text, .. } => {
            ui.label(text);
        }
        CmsSection::Heading { text, level, .. } => {
            ui.label(
                RichText::new(text)
                    .heading()
                    .size(heading_pt(level))
                    .strong(),
            );
        }
        CmsSection::SubHeading { text, .. } => {
            ui.label(RichText::new(text).strong().size(16.0));
        }
        CmsSection::Lede { text } => {
            ui.label(RichText::new(text).italics().size(15.0));
        }
        CmsSection::DropCap { text } => {
            ui.label(text);
        }
        CmsSection::PullQuote {
            body, attribution, ..
        } => {
            ui.label(RichText::new(body).italics().size(15.0));
            if let Some(a) = attribution {
                ui.label(RichText::new(format!("— {a}")).weak());
            }
        }
        CmsSection::Epigraph { body, attribution } => {
            ui.label(RichText::new(body).italics());
            if let Some(a) = attribution {
                ui.label(RichText::new(format!("— {a}")).weak());
            }
        }
        CmsSection::Marginalia { body, .. } => {
            ui.label(RichText::new(format!("⁂ {body}")).weak());
        }
        CmsSection::Quote {
            body, attribution, ..
        } => {
            ui.label(body);
            ui.label(RichText::new(format!("— {attribution}")).weak());
        }
        CmsSection::Caption { text } => {
            ui.label(RichText::new(text).weak());
        }
        CmsSection::Footnote { number, text } => {
            ui.label(format!("[{number}] {text}"));
        }
        CmsSection::Citation { text, source } => {
            ui.label(format!("{text} ({source})"));
        }
        CmsSection::Divider { .. } => {
            ui.separator();
        }
        CmsSection::Spacer { .. } => {
            ui.add_space(12.0);
        }
        CmsSection::Hero {
            eyebrow,
            title,
            lede,
            cta,
        } => {
            show_hero_textual(ui, eyebrow.as_deref(), title, lede.as_deref(), cta.as_ref());
        }
        CmsSection::ImageHero {
            eyebrow,
            title,
            lede,
            cta,
            ..
        } => {
            show_hero_textual(ui, eyebrow.as_deref(), title, lede.as_deref(), cta.as_ref());
        }
        CmsSection::Banner { text, .. } => {
            ui.label(text);
        }
        CmsSection::Code { body, lang, .. } => {
            ui.label(RichText::new(format!("```{lang}")).weak());
            ui.label(RichText::new(body).monospace());
            ui.label(RichText::new("```").weak());
        }
        CmsSection::Picture { src_stem, alt, .. } => {
            let label = if alt.is_empty() { src_stem } else { alt };
            ui.label(RichText::new(format!("[picture: {label}]")).weak());
        }
        CmsSection::Figure {
            caption,
            credit,
            asset_slug,
        } => {
            let label = asset_slug.clone().unwrap_or_else(|| "no-asset".to_owned());
            ui.label(RichText::new(format!("[figure {label}]")).weak());
            if let Some(c) = credit {
                ui.label(RichText::new(format!("{caption} · {c}")).weak());
            } else {
                ui.label(RichText::new(caption).weak());
            }
        }
        // Unknown / unrendered variant — emit a bookkeeping
        // label so the section count matches the source CMS.
        other => {
            let name = section_kind_label(other);
            ui.label(RichText::new(format!("[{name}]")).weak());
        }
    }
}

fn show_hero_textual(
    ui: &mut Ui,
    eyebrow: Option<&str>,
    title: &str,
    lede: Option<&str>,
    cta: Option<&HeroCta>,
) {
    if let Some(e) = eyebrow {
        ui.label(RichText::new(e).weak());
    }
    ui.label(RichText::new(title).heading().size(28.0).strong());
    if let Some(l) = lede {
        ui.label(l);
    }
    if let Some(c) = cta {
        ui.label(RichText::new(format!("[ {} ]", c.label)).underline());
    }
}

const fn heading_pt(level: &HeadingLevel) -> f32 {
    match level {
        HeadingLevel::H2 => 22.0,
        HeadingLevel::H3 => 19.0,
        HeadingLevel::H4 => 17.0,
        HeadingLevel::H5 => 15.0,
        HeadingLevel::H6 => 14.0,
    }
}

fn section_kind_label(s: &CmsSection) -> &'static str {
    match s {
        CmsSection::Paragraph { .. } => "paragraph",
        CmsSection::Heading { .. } => "heading",
        CmsSection::SubHeading { .. } => "sub_heading",
        CmsSection::Lede { .. } => "lede",
        CmsSection::DropCap { .. } => "drop_cap",
        CmsSection::PullQuote { .. } => "pull_quote",
        CmsSection::Epigraph { .. } => "epigraph",
        CmsSection::Marginalia { .. } => "marginalia",
        CmsSection::Quote { .. } => "quote",
        CmsSection::Caption { .. } => "caption",
        CmsSection::Footnote { .. } => "footnote",
        CmsSection::Citation { .. } => "citation",
        CmsSection::Divider { .. } => "divider",
        CmsSection::Spacer { .. } => "spacer",
        CmsSection::Hero { .. } => "hero",
        CmsSection::ImageHero { .. } => "image_hero",
        CmsSection::SplitHero { .. } => "split_hero",
        CmsSection::Banner { .. } => "banner",
        CmsSection::Code { .. } => "code",
        CmsSection::Picture { .. } => "picture",
        CmsSection::Figure { .. } => "figure",
        _ => "section",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::__run_test_ui;

    fn page_with_sections(sections: Vec<CmsSection>) -> CmsPage {
        let mut page: CmsPage = serde_json::from_value(serde_json::json!({
            "brand": null,
            "theme": null,
            "chrome": null,
            "content_width": null,
            "nav_actions": [],
            "title": "Test App",
            "description": "d",
            "path": "/",
            "nav_links": [],
            "dev_devtools": false,
            "sections": []
        }))
        .expect("page parses");
        page.sections = sections;
        page
    }

    #[test]
    fn show_page_does_not_panic_on_empty_page() {
        let page = page_with_sections(vec![]);
        __run_test_ui(|ui| {
            show_page(ui, &page);
        });
    }

    #[test]
    fn show_page_does_not_panic_on_mixed_sections() {
        let sections: Vec<CmsSection> = vec![
            serde_json::from_value(serde_json::json!({"kind":"heading","text":"H","level":2}))
                .unwrap(),
            serde_json::from_value(serde_json::json!({"kind":"paragraph","text":"P"})).unwrap(),
            serde_json::from_value(serde_json::json!({"kind":"divider","style":"line"})).unwrap(),
            serde_json::from_value(serde_json::json!({
                "kind":"image_hero",
                "title":"T",
                "lede":"L",
                "cta":{"label":"Go","href":"/x","data_backend":"x"}
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "kind":"pull_quote",
                "body":"B",
                "attribution":"A"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "kind":"code",
                "lang":"rust",
                "body":"fn main(){}",
                "terminal":false
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "kind":"epigraph",
                "body":"E"
            }))
            .unwrap(),
        ];
        let page = page_with_sections(sections);
        __run_test_ui(|ui| {
            show_page(ui, &page);
        });
    }

    #[test]
    fn show_section_handles_unknown_variant_without_panic() {
        let s: CmsSection = serde_json::from_value(serde_json::json!({
            "kind":"newsletter_signup",
            "heading":"H",
            "placeholder":"P",
            "submit_label":"S"
        }))
        .unwrap();
        __run_test_ui(|ui| {
            show_section(ui, &s);
        });
    }

    #[test]
    fn heading_pt_assigns_decreasing_sizes() {
        assert!(heading_pt(&HeadingLevel::H2) > heading_pt(&HeadingLevel::H3));
        assert!(heading_pt(&HeadingLevel::H3) > heading_pt(&HeadingLevel::H4));
        assert!(heading_pt(&HeadingLevel::H4) > heading_pt(&HeadingLevel::H5));
        assert!(heading_pt(&HeadingLevel::H5) > heading_pt(&HeadingLevel::H6));
    }

    #[test]
    fn section_kind_label_covers_text_bearing_variants() {
        let s: CmsSection =
            serde_json::from_value(serde_json::json!({"kind":"epigraph","body":"x"})).unwrap();
        assert_eq!(section_kind_label(&s), "epigraph");
        let s: CmsSection =
            serde_json::from_value(serde_json::json!({"kind":"paragraph","text":"x"})).unwrap();
        assert_eq!(section_kind_label(&s), "paragraph");
    }
}
