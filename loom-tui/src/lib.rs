//! Loom — TUI renderer.
//!
//! Renders a typed [`loom_cms_render::CmsPage`] into a
//! [`ratatui::text::Text`] so the same typed content can drive
//! a terminal UI without re-authoring the page.
//!
//! ## Scope
//!
//! This crate is a **pure renderer** — given a `CmsPage`, it
//! emits a `Text` that an external TUI binary can plug into a
//! `Paragraph` widget. The crate intentionally does NOT:
//!
//! - own a terminal handle
//! - drive a render loop
//! - handle keystrokes
//! - scroll, paginate, or maintain widget state
//!
//! Those concerns belong to a CLI binary (future
//! `loom-cli tui <page>`). Splitting renderer from runtime
//! keeps the renderer testable as a pure function.
//!
//! ## Coverage
//!
//! Renders the text-bearing Tier-2 editorial primitives plus
//! the most common Tier-1 marketing primitives' textual content.
//! Variants that are inherently graphical (Picture, VideoEmbed,
//! ImageGrid, Lightbox, etc.) emit a single `[asset: <slug>]`
//! placeholder line — the TUI can't show pixels.
//!
//! Closes #120.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use loom_cms_render::{CmsPage, CmsSection, HeadingLevel, HeroCta};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

/// Render a `CmsPage` to a ratatui `Text`.
///
/// The returned `Text` is consumable by `Paragraph::new(...)`.
/// Each section produces one or more `Line`s. Sections are
/// separated by a single blank line.
pub fn render_page(page: &CmsPage) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Page title — emitted once at the top, bold.
    lines.push(Line::from(Span::styled(
        page.title.clone(),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (i, section) in page.sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        for line in render_section_lines(section) {
            lines.push(line);
        }
    }

    Text::from(lines)
}

/// Render a single `CmsSection` to a `Vec<Line>`.
///
/// Public so callers can compose sections out of the page
/// flow (e.g. a preview pane that shows one section at a time).
pub fn render_section_lines(section: &CmsSection) -> Vec<Line<'static>> {
    match section {
        CmsSection::Paragraph { text, .. } => wrap_to_lines(text, Style::default()),
        CmsSection::Heading { text, level, .. } => {
            let prefix = "#".repeat(heading_level_to_int(level));
            vec![Line::from(vec![
                Span::raw(format!("{prefix} ")),
                Span::styled(
                    text.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ])]
        }
        CmsSection::SubHeading { text, level } => {
            let prefix = "#".repeat((*level as usize).clamp(2, 6));
            vec![Line::from(vec![
                Span::raw(format!("{prefix} ")),
                Span::styled(text.clone(), Style::default().add_modifier(Modifier::BOLD)),
            ])]
        }
        CmsSection::Lede { text } => {
            vec![Line::from(Span::styled(
                text.clone(),
                Style::default().add_modifier(Modifier::ITALIC),
            ))]
        }
        CmsSection::DropCap { text } => wrap_to_lines(text, Style::default()),
        CmsSection::PullQuote { body, attribution } => {
            let mut out: Vec<Line<'static>> = wrap_to_lines(body, Style::default().add_modifier(Modifier::ITALIC));
            if let Some(a) = attribution {
                out.push(Line::from(Span::styled(
                    format!("    — {a}"),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
            out
        }
        CmsSection::Epigraph { body, attribution } => {
            let mut out: Vec<Line<'static>> = wrap_to_lines(body, Style::default().add_modifier(Modifier::ITALIC));
            if let Some(a) = attribution {
                out.push(Line::from(Span::styled(
                    format!("    — {a}"),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
            out
        }
        CmsSection::Marginalia { body, .. } => {
            vec![Line::from(Span::styled(
                format!("  ⁂ {body}"),
                Style::default().add_modifier(Modifier::DIM),
            ))]
        }
        CmsSection::Quote { body, attribution, .. } => {
            let mut out: Vec<Line<'static>> = wrap_to_lines(body, Style::default());
            out.push(Line::from(Span::styled(
                format!("— {attribution}"),
                Style::default().add_modifier(Modifier::DIM),
            )));
            out
        }
        CmsSection::Caption { text } => {
            vec![Line::from(Span::styled(
                text.clone(),
                Style::default().add_modifier(Modifier::DIM),
            ))]
        }
        CmsSection::Footnote { number, text } => {
            vec![Line::from(format!("[{number}] {text}"))]
        }
        CmsSection::Citation { text, source } => {
            vec![Line::from(format!("{text} ({source})"))]
        }
        CmsSection::Divider { .. } => {
            vec![Line::from("─".repeat(40))]
        }
        CmsSection::Spacer { .. } => {
            vec![Line::from("")]
        }
        CmsSection::Hero { eyebrow, title, lede, cta } => {
            render_hero_textual(eyebrow.as_deref(), title, lede.as_deref(), cta.as_ref())
        }
        CmsSection::ImageHero { eyebrow, title, lede, cta, .. } => {
            render_hero_textual(eyebrow.as_deref(), title, lede.as_deref(), cta.as_ref())
        }
        CmsSection::Banner { text, .. } => wrap_to_lines(text, Style::default()),
        CmsSection::Code { body, lang, .. } => {
            let mut out: Vec<Line<'static>> = Vec::new();
            out.push(Line::from(Span::styled(
                format!("```{lang}"),
                Style::default().add_modifier(Modifier::DIM),
            )));
            for raw in body.lines() {
                out.push(Line::from(raw.to_owned()));
            }
            out.push(Line::from(Span::styled(
                "```",
                Style::default().add_modifier(Modifier::DIM),
            )));
            out
        }
        CmsSection::Picture { src_stem, alt, .. } => {
            let label = if alt.is_empty() { src_stem } else { alt };
            vec![Line::from(Span::styled(
                format!("[picture: {label}]"),
                Style::default().add_modifier(Modifier::DIM),
            ))]
        }
        CmsSection::Figure { caption, credit, asset_slug } => {
            let mut out: Vec<Line<'static>> = Vec::new();
            let label = asset_slug
                .clone()
                .unwrap_or_else(|| "no-asset".to_owned());
            out.push(Line::from(Span::styled(
                format!("[figure {label}]"),
                Style::default().add_modifier(Modifier::DIM),
            )));
            let cap = if let Some(c) = credit {
                format!("{caption} · {c}")
            } else {
                caption.clone()
            };
            out.push(Line::from(Span::styled(
                cap,
                Style::default().add_modifier(Modifier::DIM),
            )));
            out
        }
        // Unknown / unrendered variants — emit a single
        // bookkeeping line so the section count visible in
        // the TUI matches the source CMS. Future iterations
        // expand this list as variants warrant.
        other => {
            let name = section_kind_label(other);
            vec![Line::from(Span::styled(
                format!("[{name}]"),
                Style::default().add_modifier(Modifier::DIM),
            ))]
        }
    }
}

fn heading_level_to_int(level: &HeadingLevel) -> usize {
    match level {
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn render_hero_textual(
    eyebrow: Option<&str>,
    title: &str,
    lede: Option<&str>,
    cta: Option<&HeroCta>,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    if let Some(e) = eyebrow {
        out.push(Line::from(Span::styled(
            e.to_owned(),
            Style::default().add_modifier(Modifier::DIM),
        )));
    }
    out.push(Line::from(Span::styled(
        title.to_owned(),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    if let Some(l) = lede {
        for line in wrap_to_lines(l, Style::default()) {
            out.push(line);
        }
    }
    if let Some(c) = cta {
        out.push(Line::from(Span::styled(
            format!("[ {} ]", c.label),
            Style::default().add_modifier(Modifier::UNDERLINED),
        )));
    }
    out
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

/// Wrap a plain-text body to a soft column width (78 cols by
/// default) for predictable layout in narrow terminals.
fn wrap_to_lines(text: &str, style: Style) -> Vec<Line<'static>> {
    const COL: usize = 78;
    let mut out: Vec<Line<'static>> = Vec::new();
    for paragraph in text.split("\n\n") {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.len() + 1 + word.len() <= COL {
                current.push(' ');
                current.push_str(word);
            } else {
                out.push(Line::from(Span::styled(
                    std::mem::take(&mut current),
                    style,
                )));
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            out.push(Line::from(Span::styled(current, style)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_with(sections: Vec<CmsSection>) -> CmsPage {
        let json = serde_json::json!({
            "brand": null,
            "theme": null,
            "chrome": null,
            "content_width": null,
            "nav_actions": [],
            "title": "Test Page",
            "description": "desc",
            "path": "/p",
            "nav_links": [],
            "dev_devtools": false,
            "sections": []
        });
        let mut page: CmsPage = serde_json::from_value(json).expect("page parses");
        page.sections = sections;
        page
    }

    #[test]
    fn page_title_appears_at_top() {
        let p = page_with(vec![]);
        let text = render_page(&p);
        // First line should be the bold title.
        assert_eq!(text.lines[0].spans[0].content, "Test Page");
    }

    #[test]
    fn heading_renders_with_hash_prefix() {
        let json = r#"{"kind":"heading","text":"Roadmap","level":2}"#;
        let s: CmsSection = serde_json::from_str(json).unwrap();
        let lines = render_section_lines(&s);
        assert_eq!(lines.len(), 1);
        assert!(format!("{:?}", lines[0]).contains("##"));
        assert!(format!("{:?}", lines[0]).contains("Roadmap"));
    }

    #[test]
    fn paragraph_wraps_long_text() {
        let long = "lorem ipsum ".repeat(40);
        let json = serde_json::json!({ "kind": "paragraph", "text": long });
        let s: CmsSection = serde_json::from_value(json).unwrap();
        let lines = render_section_lines(&s);
        assert!(lines.len() > 1, "long paragraph should wrap to multiple lines");
    }

    #[test]
    fn pull_quote_includes_attribution() {
        let json = r#"{
            "kind":"pull_quote",
            "body":"Hope is the thing with feathers.",
            "attribution":"Dickinson"
        }"#;
        let s: CmsSection = serde_json::from_str(json).unwrap();
        let lines = render_section_lines(&s);
        let last = format!("{:?}", lines.last().unwrap());
        assert!(last.contains("Dickinson"));
    }

    #[test]
    fn divider_renders_horizontal_rule() {
        let json = r#"{"kind":"divider","style":"line"}"#;
        let s: CmsSection = serde_json::from_str(json).unwrap();
        let lines = render_section_lines(&s);
        assert_eq!(lines.len(), 1);
        assert!(format!("{:?}", lines[0]).contains("─"));
    }

    #[test]
    fn code_emits_fenced_block() {
        let json = r#"{
            "kind":"code",
            "lang":"rust",
            "body":"fn main(){}",
            "terminal":false
        }"#;
        let s: CmsSection = serde_json::from_str(json).unwrap();
        let lines = render_section_lines(&s);
        let dump = format!("{lines:?}");
        assert!(dump.contains("```rust"));
        assert!(dump.contains("fn main()"));
    }

    #[test]
    fn image_hero_renders_textual_content_only() {
        let json = r#"{
            "kind":"image_hero",
            "eyebrow":"E",
            "title":"T",
            "lede":"L",
            "cta":{"label":"Click","href":"/x","data_backend":"x"}
        }"#;
        let s: CmsSection = serde_json::from_str(json).unwrap();
        let lines = render_section_lines(&s);
        let dump = format!("{lines:?}");
        assert!(dump.contains("E"));
        assert!(dump.contains("T"));
        assert!(dump.contains("L"));
        assert!(dump.contains("Click"));
    }

    #[test]
    fn unknown_variant_emits_bookkeeping_label() {
        let json = r#"{
            "kind":"newsletter_signup",
            "heading":"H",
            "placeholder":"P",
            "submit_label":"S"
        }"#;
        let s: CmsSection = serde_json::from_str(json).unwrap();
        let lines = render_section_lines(&s);
        // The catch-all arm hits — emit at least one placeholder line.
        assert!(!lines.is_empty());
    }

    #[test]
    fn full_page_render_produces_more_lines_than_sections() {
        let secs: Vec<CmsSection> = vec![
            serde_json::from_value(serde_json::json!({"kind":"heading","text":"Section A","level":2})).unwrap(),
            serde_json::from_value(serde_json::json!({"kind":"paragraph","text":"Body of A."})).unwrap(),
            serde_json::from_value(serde_json::json!({"kind":"heading","text":"Section B","level":2})).unwrap(),
        ];
        let p = page_with(secs);
        let text = render_page(&p);
        // title + blank + headingA + blank + paraA + blank + headingB
        assert!(text.lines.len() >= 7);
    }
}
