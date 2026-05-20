//! `loom state-matrix` subcommand — emit the component state-matrix
//! HTML files. Renders one representative instance of every
//! `CmsSection` variant + named state into a single page, then emits
//! one file per theme so visual review covers the full cascade.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3). Body kept byte-for-byte identical to the
//! pre-extraction implementation; visibility on `cmd_state_matrix`
//! was widened from module-private to `pub` so the entrypoint can
//! call it through the `mod state_matrix;` boundary.

use anyhow::Result;

use crate::WriteCapability;

/// T34: emit the component state-matrix HTML files. Renders one
/// representative instance of every CmsSection variant + named
/// state into a single page, then emits one file per theme so
/// visual review covers the full cascade.
///
/// Layout: each section in the page is an instance of a
/// CmsSection variant; the page-shell wraps them with a section
/// header above each ("Heading H2", "Banner: warn", etc.) so a
/// reviewer can scan the grid without parsing source.
pub fn cmd_state_matrix(out: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(out)
        .map_err(|e| anyhow::anyhow!("create out dir {}: {e}", out.display()))?;
    let cap = WriteCapability::for_dir(out)
        .map_err(|_| anyhow::anyhow!("scope capability to {}", out.display()))?;
    let page = build_state_matrix_page();
    let body = loom_cms_render::render_page(&page).into_string();
    let mut written = 0usize;
    // T34/cycle-38 fix: page_shell_themed emits a `<link
    // rel="stylesheet" href="loom-skin.css">` tag. Without the
    // sibling CSS file, every state-matrix-*.html load 404s and
    // PlausiDen-Crawler grades the output Grade C with reliability=F
    // (6 strict console-errors + 10 strict failed-requests across
    // the 3 themes). Emit loom-skin.css alongside the HTML so the
    // matrix is self-contained and a designer/agent can open
    // state-matrix-light.html directly without spinning up a server.
    let skin_css = loom_tokens::tokens_css();
    cap.write_file(std::path::Path::new("loom-skin.css"), skin_css.as_bytes())
        .map_err(|_| anyhow::anyhow!("write loom-skin.css"))?;
    for theme in [None, Some("light"), Some("dark")] {
        let label = theme.unwrap_or("auto");
        // REGRESSION-GUARD cycle 62: each theme variant gets its own
        // title + description so the Crawler's crossPageTitle and
        // crossPageMetaDescription detectors don't flag the trio
        // as duplicate. Each file IS a distinct page (different
        // theme, different visual output); the metadata should
        // reflect that. The CmsPage is otherwise identical so the
        // section grid stays consistent across theme snapshots.
        let mut themed_page = page.clone();
        let theme_label_display = match theme {
            Some("light") => "Light theme",
            Some("dark") => "Dark theme",
            _ => "Auto (OS preference)",
        };
        themed_page.title = format!("{} — {}", page.title, theme_label_display);
        themed_page.description =
            format!("{} Variant: {}.", page.description, theme_label_display,);
        let html = loom_cms_render::page_shell_themed(
            &themed_page,
            // Use the relative path so the file works from `file://`
            // URLs (designers email these around) AND from
            // `python3 -m http.server` (PlausiDen-Crawler audits).
            // The previous "/loom-skin.css" absolute path worked
            // ONLY when served from the document root, which the
            // state-matrix never is.
            "loom-skin.css",
            &body,
            None,
            theme,
        );
        let rel = std::path::PathBuf::from(format!("state-matrix-{label}.html"));
        cap.write_file(&rel, html.as_bytes())
            .map_err(|_| anyhow::anyhow!("write state-matrix-{label}.html"))?;
        written += 1;
    }
    println!("loom state-matrix:");
    println!(
        "  ok  rendered {} CmsSection variant(s)",
        page.sections.len()
    );
    println!("  ok  loom-skin.css written ({} bytes)", skin_css.len());
    println!("  ok  {written} HTML file(s) written to {}/", out.display());
    println!("       state-matrix-auto.html   (OS prefers-color-scheme)");
    println!("       state-matrix-light.html  (explicit light)");
    println!("       state-matrix-dark.html   (explicit dark)");
    println!();
    println!("Open in a browser, or feed into PlausiDen-Crawler for");
    println!("visual-regression / pixel-diff against a baseline.");
    Ok(())
}

/// Build the canonical CmsPage that exercises every variant +
/// named state. Adding a new CmsSection variant SHOULD extend
/// this page so every shipped state stays visible to designer
/// review + visual-regression.
fn build_state_matrix_page() -> loom_cms_render::CmsPage {
    use loom_cms_render::{
        CmsBannerTone, CmsCard, CmsNavLink, CmsPage, CmsPanel, CmsPanelBody, CmsSection,
        HeadingLevel, HeroCta,
    };
    CmsPage {
        brand: None,
        theme: None,
        chrome: None,
        content_width: None,
        nav_actions: vec![],
        schema: None,
            version: None,
        title: "Loom state matrix".into(),
        description: "Every CmsSection variant + named state, on one page.".into(),
        path: "/state-matrix".into(),
        nav_links: vec![
            CmsNavLink {
                label: "Home".into(),
                href: "/".into(),
                data_backend: "list-pages".into(),
                current: true,
            },
            CmsNavLink {
                label: "Other".into(),
                href: "/other.html".into(),
                data_backend: "list-pages".into(),
                current: false,
            },
        ],
        dev_devtools: false,
        footer: None,
        site_origin: None,
        social_image: None,
        sections: vec![
            // Heading — every level h2..h6.
            CmsSection::Heading {
                level: HeadingLevel::H2,
                text: "Heading H2 — top-level section".into(),
                id: None,
                polish: Vec::new(),
                },
            CmsSection::Heading {
                level: HeadingLevel::H3,
                text: "Heading H3 — subsection".into(),
                id: None,
                polish: Vec::new(),
                },
            CmsSection::Heading {
                level: HeadingLevel::H4,
                text: "Heading H4".into(),
                id: None,
                polish: Vec::new(),
                },
            CmsSection::Heading {
                level: HeadingLevel::H5,
                text: "Heading H5".into(),
                id: None,
                polish: Vec::new(),
                },
            CmsSection::Heading {
                level: HeadingLevel::H6,
                text: "Heading H6 — deepest content heading".into(),
                id: None,
                polish: Vec::new(),
                },
            // Paragraph — single + with longer prose.
            CmsSection::Paragraph {
                text: "Paragraph — short prose. Tests body typography, line-height, max-width.".into(),
                decoration: loom_cms_render::ParagraphDecoration::Body,
            },
            CmsSection::Paragraph {
                text: "Paragraph — longer prose to test line-length wrapping. \
                       Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
                       eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                       Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.".into(),
                decoration: loom_cms_render::ParagraphDecoration::Body,
            },
            // Hero — minimal + full (eyebrow + lede + cta).
            CmsSection::Hero {
                eyebrow: None,
                title: "Hero — minimal".into(),
                lede: None,
                cta: None,
            },
            CmsSection::Hero {
                eyebrow: Some("Eyebrow tag".into()),
                title: "Hero — full".into(),
                lede: Some("Lede paragraph — sets the scene. Longer than a tagline, shorter than a paragraph.".into()),
                cta: Some(HeroCta {
                    label: "Primary action".into(),
                    href: "/cta-target".into(),
                    data_backend: "cta-target".into(),
                }),
            },
            // Group — heading + N body paragraphs.
            CmsSection::Group {
                title: "Group — heading + body paragraphs".into(),
                body: vec![
                    "First paragraph in the group.".into(),
                    "Second paragraph — tests vertical rhythm + spacing.".into(),
                    "Third paragraph — multiple paragraphs in one group should stack cleanly.".into(),
                ],
            },
            // Banner — every tone.
            CmsSection::Banner {
                tone: CmsBannerTone::Info,
                text: "Banner (info) — neutral notice. Maintenance window, schedule change.".into(),
                dismissible: false,
                id: None,
            },
            CmsSection::Banner {
                tone: CmsBannerTone::Success,
                text: "Banner (success) — confirmation. Saved, published, deployed.".into(),
                dismissible: false,
                id: None,
            },
            CmsSection::Banner {
                tone: CmsBannerTone::Warn,
                text: "Banner (warn) — actionable warning. Approaching budget, voting closes soon.".into(),
                dismissible: true,
                id: Some("matrix-warn-demo".into()),
            },
            CmsSection::Banner {
                tone: CmsBannerTone::Danger,
                text: "Banner (danger) — error / critical alert. Failed deploy, signature mismatch.".into(),
                dismissible: false,
                id: None,
            },
            // CardFeed — small list + stat-bearing cards.
            CmsSection::CardFeed {
                heading: Some("CardFeed — sample feed".into()),
                items: vec![
                    CmsCard {
                        avatar: loom_cms_render::CmsAvatar::None,
                        title: "Card 1 — minimal".into(),
                        host: None,
                        stats: vec![],
                        href: "/card-1".into(),
                        data_backend: "card-target".into(),
                        tag: None,
                        tone: None,
                        media: None,
                    },
                    CmsCard {
                        avatar: loom_cms_render::CmsAvatar::None,
                        title: "Card 2 — with host".into(),
                        host: Some("Host name".into()),
                        stats: vec![],
                        href: "/card-2".into(),
                        data_backend: "card-target".into(),
                        tag: Some("featured".into()),
                        tone: None,
                        media: None,
                    },
                ],
            },
            // Sidebar — single panel for the matrix; a real page
            // would have N panels.
            CmsSection::Sidebar {
                label: Some("Side panels".into()),
                panels: vec![CmsPanel {
                    title: "Panel heading".into(),
                    body: CmsPanelBody::Text {
                        paragraphs: vec![
                            "Sidebar panel body — single paragraph.".into(),
                        ],
                    },
                }],
            },
            // Heading marker for the matrix end so reviewers know
            // they've seen the full set.
            CmsSection::Heading {
                level: HeadingLevel::H2,
                text: "End of state matrix".into(),
                id: None,
                polish: Vec::new(),
                },
            CmsSection::Paragraph {
                text: "Every CmsSection variant + every named state should appear above. \
                       If you see a variant missing, extend `build_state_matrix_page` in loom-cli.".into(),
                decoration: loom_cms_render::ParagraphDecoration::Body,
            },
        ],
    }
}
