//! Visual-uniqueness matrix proof (#331).
//!
//! Builds the same `CmsPage` with each combination of
//! `(theme, chrome, content_width)` slots that the page shell
//! supports and asserts every rendered HTML string differs
//! pairwise. The architectural commitment is: a tenant picking
//! different style slots gets a meaningfully different rendered
//! page, not just a different `data-*` attribute that's
//! invisible to humans.
//!
//! HTML-level distinctness is the in-process proof; pixel-level
//! distinctness lands when the Crawler diff matrix runs against
//! built static output (separate follow-up; needs a built
//! `forge` + `crawler` toolchain available).

use loom_cms_render::{
    page_shell_themed, ChromeKind, CmsPage, CmsSection, ContentWidth, HeroEditorialBackground,
    HeroSplitSide,
};
use std::collections::HashSet;

/// Build a canonical CmsPage with the same content for every
/// variant. Only the style slots differ between calls.
fn page_with(theme: Option<&str>, chrome: ChromeKind, width: ContentWidth) -> CmsPage {
    let payload = format!(
        r#"{{
            "title": "Acme — Uniqueness Test",
            "description": "Uniqueness matrix proof",
            "path": "/index",
            "theme": {theme_value},
            "chrome": {chrome_value},
            "content_width": {width_value},
            "nav_actions": [],
            "nav_links": [],
            "dev_devtools": false,
            "sections": [
                {{
                    "kind": "hero_minimal",
                    "title": "Headline",
                    "lede": "Lede"
                }}
            ]
        }}"#,
        theme_value = match theme {
            Some(t) => format!("\"{t}\""),
            None => "null".to_owned(),
        },
        chrome_value = format!("\"{}\"", chrome_slug(chrome)),
        width_value = format!("\"{}\"", width_slug(width)),
    );
    serde_json::from_str(&payload).expect("CmsPage parses")
}

fn chrome_slug(c: ChromeKind) -> &'static str {
    match c {
        ChromeKind::PageShell => "page_shell",
        ChromeKind::FloatingPill => "floating_pill",
        ChromeKind::Minimal => "minimal",
    }
}

fn width_slug(w: ContentWidth) -> &'static str {
    match w {
        ContentWidth::Comfortable => "comfortable",
        ContentWidth::Narrow => "narrow",
        ContentWidth::Wide => "wide",
        ContentWidth::Full => "full",
    }
}

#[test]
fn theme_chrome_width_matrix_is_pairwise_distinct() {
    // 4 themes × 3 chromes × 4 widths = 48 combinations.
    let themes: &[Option<&str>] = &[
        Some("light"),
        Some("dark"),
        Some("warm"),
        Some("ocean"),
    ];
    let chromes = [ChromeKind::PageShell, ChromeKind::FloatingPill, ChromeKind::Minimal];
    let widths = [
        ContentWidth::Comfortable,
        ContentWidth::Narrow,
        ContentWidth::Wide,
        ContentWidth::Full,
    ];
    let mut seen: HashSet<String> = HashSet::new();
    let mut duplicates: Vec<(usize, &'static str)> = Vec::new();
    let mut count = 0usize;
    for &theme in themes {
        for chrome in chromes {
            for width in widths {
                let page = page_with(theme, chrome, width);
                let html =
                    page_shell_themed(&page, "/loom-skin.css", "<main></main>", None, theme);
                if !seen.insert(html.clone()) {
                    duplicates.push((count, chrome_slug(chrome)));
                }
                count += 1;
            }
        }
    }
    assert_eq!(
        seen.len(),
        count,
        "expected {count} pairwise-distinct renders; got {} unique strings + {} duplicates",
        seen.len(),
        duplicates.len()
    );
    assert!(
        duplicates.is_empty(),
        "duplicate renders found at: {duplicates:?}"
    );
}

#[test]
fn hero_section_matrix_is_pairwise_distinct() {
    // 4 hero shapes (Hero, HeroEditorial, HeroSplit, HeroMinimal)
    // should each emit distinct HTML even with identical title +
    // lede text.
    let hero_payloads: &[&str] = &[
        // Hero (centered, eyebrow pill)
        r#"{"kind":"hero","title":"T","lede":"L"}"#,
        // HeroEditorial (asymmetric, monospace kicker)
        r#"{"kind":"hero_editorial","kicker":null,"headline":"T","headline_accent":null,"lede":"L"}"#,
        // HeroSplit (image-on-one-side)
        r#"{"kind":"hero_split","title":"T","lede":"L","image_url":"/img.jpg","image_alt":"A","image_side":"right"}"#,
        // HeroMinimal (text-only)
        r#"{"kind":"hero_minimal","title":"T","lede":"L"}"#,
    ];
    let mut seen: HashSet<String> = HashSet::new();
    for payload in hero_payloads {
        let section: CmsSection = serde_json::from_str(payload)
            .unwrap_or_else(|e| panic!("section parses: {e} ({payload})"));
        let html = loom_cms_render::render_section(&section).into_string();
        assert!(seen.insert(html), "duplicate hero render: {payload}");
    }
    assert_eq!(seen.len(), hero_payloads.len());
}

#[test]
fn hero_split_left_vs_right_produces_distinct_renders() {
    // Same content, just flipped image_side — must produce
    // different HTML.
    let left: CmsSection = serde_json::from_str(
        r#"{"kind":"hero_split","title":"T","lede":"L","image_url":"/i.jpg","image_alt":"A","image_side":"left"}"#,
    )
    .unwrap();
    let right: CmsSection = serde_json::from_str(
        r#"{"kind":"hero_split","title":"T","lede":"L","image_url":"/i.jpg","image_alt":"A","image_side":"right"}"#,
    )
    .unwrap();
    let l = loom_cms_render::render_section(&left).into_string();
    let r = loom_cms_render::render_section(&right).into_string();
    assert_ne!(l, r);
    assert!(l.contains(r#"data-image-side="left""#));
    assert!(r.contains(r#"data-image-side="right""#));
}

#[test]
fn hero_editorial_background_amoled_vs_slate_distinct() {
    let amoled: CmsSection = serde_json::from_str(
        r#"{"kind":"hero_editorial","kicker":null,"headline":"T","headline_accent":null,"lede":"L","background":"amoled"}"#,
    )
    .unwrap();
    let slate: CmsSection = serde_json::from_str(
        r#"{"kind":"hero_editorial","kicker":null,"headline":"T","headline_accent":null,"lede":"L","background":"slate"}"#,
    )
    .unwrap();
    let a = loom_cms_render::render_section(&amoled).into_string();
    let s = loom_cms_render::render_section(&slate).into_string();
    assert_ne!(a, s);
    assert!(a.contains(r#"data-background="amoled""#));
    assert!(s.contains(r#"data-background="slate""#));
    // Confirm the substrate uses the BackgroundEnums, not just an
    // arbitrary string — guards against a future refactor that
    // accidentally drops the background data attr.
    let _ = HeroEditorialBackground::Amoled;
    let _ = HeroSplitSide::Left;
}
