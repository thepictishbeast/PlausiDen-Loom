//! `loom-cms-render` — bridge from CMS page schema to Loom components.
//!
//! ARCHITECTURE
//! ------------
//! The CMS stores pages as serializable [`CmsPage`] documents.
//! Each page is a typed sequence of [`CmsSection`] variants; each
//! variant maps to ONE Loom primitive. The bridge function
//! [`render_page`] walks the document and returns a single
//! `maud::Markup` ready for serialization or further composition
//! into a layout shell.
//!
//! WHY THIS CRATE EXISTS
//! ---------------------
//! Without it, the CMS would either (a) emit raw HTML strings —
//! defeating the design system — or (b) directly construct Loom
//! components via Rust code at request time, coupling the CMS to
//! the component crate. The bridge inverts that: the CMS speaks
//! a stable JSON schema, this crate translates JSON → Loom calls.
//! Future renderers (GTK, Jetpack Compose, terminal) can be
//! added by extending the `render_*` family without changing the
//! schema.
//!
//! SECURITY DOCTRINE
//! -----------------
//! 1. Every text field passes through Maud's auto-escaping. No
//!    `PreEscaped` accepts CMS content — that would let a CMS
//!    editor smuggle HTML.
//! 2. URLs go through a same-origin / `https://` validator at the
//!    Loom-component layer (`composer::is_safe_url`,
//!    `picture::*` paths). The bridge enforces nothing further;
//!    if a component accepts a URL, that component owns the
//!    validation.
//! 3. The schema is `#[serde(deny_unknown_fields)]` everywhere.
//!    A CMS that emits unknown fields fails deserialization at the
//!    boundary — no silent acceptance, no field-name typos that
//!    silently get dropped on the floor.
//! 4. No `unsafe`. No `unwrap`/`expect` in non-test code.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use loom_components::composer::{Composer, ComposerAvatar, ComposerSize, PromptAction};
use loom_components::picture::{Picture, PictureFit, PictureLoading, PicturePriority};

/// Re-export of `loom_components::composer::is_safe_url` so
/// downstream consumers (loom-cli's page-shell) can validate
/// URLs without taking a direct dependency on loom-components.
pub use loom_components::composer::is_safe_url;
use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// A single CMS-managed page. The smallest unit the bridge knows
/// how to render in isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsPage {
    /// `<title>` text. Required.
    pub title: String,
    /// `<meta name="description">` text. Required for SEO.
    pub description: String,
    /// Canonical URL path (e.g. `"/leaderboard"`). Required.
    /// Used by the layout shell to emit `<link rel="canonical">`.
    pub path: String,
    /// Optional primary navigation links. The page-shell renders
    /// these inside `<nav aria-label="Primary">`. Empty/omitted →
    /// shell emits brand-only nav. Each link's `href` is validated
    /// (same-origin path or `https://`); invalid hrefs render as
    /// `#invalid-nav-link` placeholders.
    #[serde(default)]
    pub nav_links: Vec<CmsNavLink>,
    /// Sequence of body sections, top to bottom.
    pub sections: Vec<CmsSection>,
}

/// One primary-nav link.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsNavLink {
    /// Visible label.
    pub label: String,
    /// Same-origin path or `https://` URL.
    pub href: String,
    /// Backend key (verified against `backends.toml` by forge's
    /// phase_phantom_button).
    pub data_backend: String,
    /// Mark this link as the current page (renders
    /// `aria-current="page"`).
    #[serde(default)]
    pub current: bool,
}

/// One section of a page. Adding a variant requires a paired
/// renderer arm in [`render_section`] and a unit test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CmsSection {
    /// Top-of-page hero. Optional eyebrow pill, required title,
    /// optional lede, optional primary CTA. Loom-namespaced
    /// (no Tailwind dependency) so it composes cleanly with the
    /// SkillShots PoC skin.
    Hero {
        /// Optional small pill above the title.
        eyebrow: Option<String>,
        /// Headline text.
        title: String,
        /// Optional subhead paragraph.
        lede: Option<String>,
        /// Optional primary CTA.
        cta: Option<HeroCta>,
    },
    /// Group: a heading + a body paragraph, framed as a `<section>`.
    /// Useful for "How it works" / "Rules" type explainer blocks.
    Group {
        /// Heading text (rendered as h2 inside the section).
        title: String,
        /// Body paragraph(s). Each entry becomes a `<p>`.
        body: Vec<String>,
    },
    /// Feed-top compose box. Maps to [`Composer`].
    Composer {
        /// Visible CTA text.
        prompt: String,
        /// Where the prompt links to.
        submit_endpoint: String,
        /// Up to 3 prompt actions.
        actions: Vec<CmsPromptAction>,
        /// Avatar slot.
        avatar: CmsAvatar,
        /// Density.
        size: CmsComposerSize,
    },
    /// Single image with the full Picture treatment.
    Picture {
        /// Asset stem under `/assets/`.
        src_stem: String,
        /// Required alt text. Empty string only for decorative.
        alt: String,
        /// Intrinsic width (CSS px).
        width: u32,
        /// Intrinsic height.
        height: u32,
        /// Loading strategy.
        loading: CmsLoading,
        /// Resource priority.
        priority: CmsPriority,
        /// Object-fit mode.
        fit: CmsFit,
    },
    /// A paragraph of body text. Maud auto-escapes on render.
    Paragraph {
        /// Plain-text body (no markup).
        text: String,
    },
    /// A heading. Level constrained to 2-4 (h1 is owned by the
    /// page-shell template, not section content).
    Heading {
        /// Heading text.
        text: String,
        /// `2` → `<h2>`, etc. Validation in [`render_section`].
        level: u8,
    },
}

/// Hero CTA — the single typed primary action attached to a Hero
/// section. URL is validated by `composer::is_safe_url` at render
/// time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroCta {
    /// Visible label text.
    pub label: String,
    /// Where the CTA navigates. Same-origin path or https://.
    pub href: String,
    /// Backend key (must match a `[backends.X]` in backends.toml).
    /// Forge's phantom_button phase verifies this at build time.
    pub data_backend: String,
}

/// Closed enum mirror of [`PromptAction`] — separated so the wire
/// format is independent of the Loom enum's internals.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmsPromptAction {
    /// Map → [`PromptAction::UploadClip`].
    UploadClip,
    /// Map → [`PromptAction::ChallengeOpponent`].
    ChallengeOpponent,
    /// Map → [`PromptAction::GoLive`].
    GoLive,
    /// Map → [`PromptAction::PhotoOnly`].
    PhotoOnly,
}

impl From<CmsPromptAction> for PromptAction {
    fn from(c: CmsPromptAction) -> Self {
        match c {
            CmsPromptAction::UploadClip => PromptAction::UploadClip,
            CmsPromptAction::ChallengeOpponent => PromptAction::ChallengeOpponent,
            CmsPromptAction::GoLive => PromptAction::GoLive,
            CmsPromptAction::PhotoOnly => PromptAction::PhotoOnly,
        }
    }
}

/// Mirror of [`ComposerSize`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmsComposerSize {
    /// Compact density.
    Compact,
    /// Comfortable density.
    Comfortable,
}

impl From<CmsComposerSize> for ComposerSize {
    fn from(c: CmsComposerSize) -> Self {
        match c {
            CmsComposerSize::Compact => ComposerSize::Compact,
            CmsComposerSize::Comfortable => ComposerSize::Comfortable,
        }
    }
}

/// Wire form of [`ComposerAvatar`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CmsAvatar {
    /// No avatar slot.
    None,
    /// Display 1-3 letters.
    Initials {
        /// Letters.
        letters: String,
    },
    /// Display an image.
    Image {
        /// Image src.
        src: String,
        /// Required alt.
        alt: String,
    },
}

/// Mirror of [`PictureLoading`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmsLoading {
    /// Lazy load.
    Lazy,
    /// Eager load.
    Eager,
}

impl From<CmsLoading> for PictureLoading {
    fn from(c: CmsLoading) -> Self {
        match c {
            CmsLoading::Lazy => PictureLoading::Lazy,
            CmsLoading::Eager => PictureLoading::Eager,
        }
    }
}

/// Mirror of [`PicturePriority`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmsPriority {
    /// Browser default.
    Auto,
    /// Pre-load high.
    High,
    /// De-prioritize.
    Low,
}

impl From<CmsPriority> for PicturePriority {
    fn from(c: CmsPriority) -> Self {
        match c {
            CmsPriority::Auto => PicturePriority::Auto,
            CmsPriority::High => PicturePriority::High,
            CmsPriority::Low => PicturePriority::Low,
        }
    }
}

/// Mirror of [`PictureFit`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmsFit {
    /// Default.
    Default,
    /// Cover.
    Cover,
    /// Contain.
    Contain,
}

impl From<CmsFit> for PictureFit {
    fn from(c: CmsFit) -> Self {
        match c {
            CmsFit::Default => PictureFit::Default,
            CmsFit::Cover => PictureFit::Cover,
            CmsFit::Contain => PictureFit::Contain,
        }
    }
}

/// Render a complete CMS page to Loom markup. The output is a
/// `<main>` containing one rendered subtree per section, in order.
///
/// PAGE-SHELL CONTRACT: this function emits NO `<html>`, `<head>`,
/// `<title>`, or `<h1>`. Those belong to the page-layout template.
/// The bridge focuses on the body region the CMS owns.
#[must_use]
pub fn render_page(page: &CmsPage) -> Markup {
    html! {
        main class="loom-page" data-cms-path=(page.path) {
            @for section in &page.sections {
                (render_section(section))
            }
        }
    }
}

/// Render one CMS section to Loom markup.
#[must_use]
pub fn render_section(section: &CmsSection) -> Markup {
    match section {
        CmsSection::Hero {
            eyebrow,
            title,
            lede,
            cta,
        } => {
            // CTA href validation: same rule as Composer.
            // Invalid → fallback href + data-invalid (skin.css
            // styles a warning outline, forge audit can detect).
            let cta_href_safe = cta
                .as_ref()
                .map(|c| loom_components::composer::is_safe_url(&c.href))
                .unwrap_or(true);
            html! {
                section class="loom-section-hero" data-loom-hero {
                    @if let Some(e) = eyebrow {
                        span class="loom-section-hero__eyebrow" { (e) }
                    }
                    h2 class="loom-section-hero__title" { (title) }
                    @if let Some(l) = lede {
                        p class="loom-section-hero__lede" { (l) }
                    }
                    @if let Some(c) = cta {
                        a
                            class="loom-section-hero__cta"
                            href=(if cta_href_safe { c.href.as_str() } else { "#invalid-cta" })
                            data-backend=(c.data_backend)
                            data-invalid=[(!cta_href_safe).then_some("true")]
                        {
                            (c.label)
                        }
                    }
                }
            }
        }
        CmsSection::Group { title, body } => html! {
            section class="loom-section-group" {
                h2 class="loom-section-group__title" { (title) }
                @for paragraph in body {
                    p class="loom-section-group__body" { (paragraph) }
                }
            }
        },
        CmsSection::Composer {
            prompt,
            submit_endpoint,
            actions,
            avatar,
            size,
        } => {
            let mapped_actions: Vec<PromptAction> =
                actions.iter().copied().map(Into::into).collect();
            let composer_avatar = match avatar {
                CmsAvatar::None => ComposerAvatar::None,
                CmsAvatar::Initials { letters } => ComposerAvatar::Initials(letters),
                CmsAvatar::Image { src, alt } => ComposerAvatar::Image { src, alt },
            };
            let c = Composer {
                prompt,
                submit_endpoint,
                actions: mapped_actions,
                avatar: composer_avatar,
                size: (*size).into(),
            };
            c.render()
        }
        CmsSection::Picture {
            src_stem,
            alt,
            width,
            height,
            loading,
            priority,
            fit,
        } => {
            let p = Picture {
                src_stem,
                alt,
                width: *width,
                height: *height,
                loading: (*loading).into(),
                priority: (*priority).into(),
                fit: (*fit).into(),
            };
            p.render()
        }
        CmsSection::Paragraph { text } => html! {
            p class="loom-prose" { (text) }
        },
        CmsSection::Heading { text, level } => {
            // Constrain to h2-h4 — h1 is owned by the page-shell.
            // Any out-of-range level falls back to h2 (still
            // semantically valid, just less specific) AND emits a
            // data-cms-warn attribute so the forge audit can spot
            // CMS pages with bad heading levels.
            match level {
                3 => html! { h3 class="loom-heading" data-loom-level="3" { (text) } },
                4 => html! { h4 class="loom-heading" data-loom-level="4" { (text) } },
                _ => html! {
                    h2 class="loom-heading" data-loom-level="2" data-cms-warn=[
                        (*level != 2).then_some("level-clamped")
                    ] { (text) }
                },
            }
        }
    }
}

/// Convenience: render a page directly from a JSON document.
/// Returns the Maud markup OR a serde_json error if the document
/// doesn't satisfy the schema.
///
/// SECURITY: `deny_unknown_fields` on every CmsPage / CmsSection
/// variant makes typos and field-name drift LOUD. A CMS that
/// emits an unrecognized field fails deserialization here rather
/// than silently shipping a missing render.
pub fn render_json(doc: &str) -> Result<Markup, serde_json::Error> {
    let page: CmsPage = serde_json::from_str(doc)?;
    Ok(render_page(&page))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(p: &CmsPage) -> String {
        render_page(p).into_string()
    }

    #[test]
    fn empty_page_renders_main_shell() {
        let p = CmsPage {
            title: "Home".to_owned(),
            description: "x".to_owned(),
            path: "/".to_owned(),
            nav_links: vec![],
            sections: vec![],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<main class="loom-page""#));
        assert!(html.contains(r#"data-cms-path="/""#));
    }

    #[test]
    fn paragraph_renders_loom_prose() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Paragraph {
                text: "Hello world.".to_owned(),
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<p class="loom-prose">Hello world.</p>"#));
    }

    #[test]
    fn paragraph_html_is_escaped() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Paragraph {
                text: "<script>alert(1)</script>".to_owned(),
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("<script>"), "raw script leaked: {html}");
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn heading_level_2_renders_h2() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Heading {
                text: "Section".to_owned(),
                level: 2,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<h2 class="loom-heading" data-loom-level="2""#));
        assert!(!html.contains("data-cms-warn"));
    }

    #[test]
    fn heading_level_3_and_4_render_correctly() {
        for (level, expected_tag) in [(3, "h3"), (4, "h4")] {
            let p = CmsPage {
                title: "x".to_owned(),
                description: "x".to_owned(),
                path: "/x".to_owned(),
                nav_links: vec![],
                sections: vec![CmsSection::Heading {
                    text: "x".to_owned(),
                    level,
                }],
            };
            let html = render_to_string(&p);
            assert!(
                html.contains(&format!("<{expected_tag} ")),
                "level {level} → {expected_tag}: {html}"
            );
        }
    }

    #[test]
    fn heading_level_out_of_range_clamps_with_warn() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Heading {
                text: "x".to_owned(),
                level: 7,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains("<h2 "));
        assert!(html.contains(r#"data-cms-warn="level-clamped""#));
    }

    #[test]
    fn composer_section_renders_loom_composer() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Composer {
                prompt: "What did you nail?".to_owned(),
                submit_endpoint: "/post-skill".to_owned(),
                actions: vec![CmsPromptAction::UploadClip],
                avatar: CmsAvatar::Initials {
                    letters: "DA".to_owned(),
                },
                size: CmsComposerSize::Comfortable,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"class="loom-composer""#));
        assert!(html.contains("What did you nail?"));
        assert!(html.contains(">DA<"));
    }

    #[test]
    fn picture_section_renders_loom_picture() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Picture {
                src_stem: "hero/dragon".to_owned(),
                alt: "A dragon".to_owned(),
                width: 1280,
                height: 720,
                loading: CmsLoading::Eager,
                priority: CmsPriority::High,
                fit: CmsFit::Cover,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains("/assets/hero/dragon.avif"));
        assert!(html.contains("/assets/hero/dragon.webp"));
        assert!(html.contains("/assets/hero/dragon.jpg"));
        assert!(html.contains(r#"alt="A dragon""#));
        assert!(html.contains(r#"loading="eager""#));
    }

    #[test]
    fn json_round_trip() {
        let json = r#"{
            "title": "Home",
            "description": "x",
            "path": "/",
            "sections": [
                { "kind": "heading", "text": "Welcome", "level": 2 },
                { "kind": "paragraph", "text": "Body text." }
            ]
        }"#;
        let markup = render_json(json).expect("renders");
        let html = markup.into_string();
        assert!(html.contains("<h2 "));
        assert!(html.contains("Welcome"));
        assert!(html.contains("Body text."));
    }

    #[test]
    fn json_with_unknown_fields_is_rejected() {
        let json = r#"{
            "title": "x",
            "description": "x",
            "path": "/",
            "sections": [],
            "extra_field_that_should_fail": "evil"
        }"#;
        let r = render_json(json);
        assert!(r.is_err(), "deny_unknown_fields not enforced");
    }

    #[test]
    fn json_section_with_unknown_kind_is_rejected() {
        let json = r#"{
            "title": "x",
            "description": "x",
            "path": "/",
            "sections": [
                { "kind": "unknown_section", "text": "x" }
            ]
        }"#;
        let r = render_json(json);
        assert!(r.is_err(), "unknown section kind silently accepted");
    }

    #[test]
    fn hero_renders_required_title_only() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Hero {
                eyebrow: None,
                title: "Welcome".to_owned(),
                lede: None,
                cta: None,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"class="loom-section-hero""#));
        assert!(html.contains("<h2 class=\"loom-section-hero__title\">Welcome</h2>"));
        assert!(!html.contains("loom-section-hero__eyebrow"));
        assert!(!html.contains("loom-section-hero__lede"));
        assert!(!html.contains("loom-section-hero__cta"));
    }

    #[test]
    fn hero_renders_all_optional_slots() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Hero {
                eyebrow: Some("New".to_owned()),
                title: "Welcome".to_owned(),
                lede: Some("Skill battles, decided by your crew.".to_owned()),
                cta: Some(HeroCta {
                    label: "Sign up".to_owned(),
                    href: "/sign-up".to_owned(),
                    data_backend: "sign-up".to_owned(),
                }),
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(">New<"));
        assert!(html.contains(">Welcome<"));
        assert!(html.contains(">Skill battles"));
        assert!(html.contains(r#"href="/sign-up""#));
        assert!(html.contains(r#"data-backend="sign-up""#));
        assert!(html.contains(">Sign up<"));
    }

    #[test]
    fn hero_invalid_cta_href_substitutes_placeholder() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Hero {
                eyebrow: None,
                title: "x".to_owned(),
                lede: None,
                cta: Some(HeroCta {
                    label: "x".to_owned(),
                    href: "javascript:alert(1)".to_owned(),
                    data_backend: "x".to_owned(),
                }),
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r##"href="#invalid-cta""##));
        assert!(html.contains(r##"data-invalid="true""##));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn hero_text_is_escaped() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Hero {
                eyebrow: Some("<x>".to_owned()),
                title: "<script>".to_owned(),
                lede: None,
                cta: None,
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;x&gt;"));
    }

    #[test]
    fn group_renders_title_and_multiple_body_paragraphs() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Group {
                title: "Rules".to_owned(),
                body: vec!["First rule.".to_owned(), "Second rule.".to_owned()],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"class="loom-section-group""#));
        assert!(html.contains(">Rules<"));
        // Body paragraphs in order.
        let p1 = html.find("First rule.").expect("first");
        let p2 = html.find("Second rule.").expect("second");
        assert!(p1 < p2);
    }

    #[test]
    fn group_with_empty_body_renders_just_title() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Group {
                title: "Empty".to_owned(),
                body: vec![],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(">Empty<"));
        assert!(!html.contains("loom-section-group__body"));
    }

    #[test]
    fn full_page_with_multiple_sections() {
        let json = r#"{
            "title": "Index",
            "description": "Skill battles, voted by your crew.",
            "path": "/",
            "sections": [
                {
                    "kind": "composer",
                    "prompt": "What did you nail today?",
                    "submit_endpoint": "/post-skill",
                    "actions": ["upload_clip", "challenge_opponent"],
                    "avatar": { "kind": "none" },
                    "size": "comfortable"
                },
                { "kind": "heading", "text": "Top battles", "level": 2 },
                { "kind": "paragraph", "text": "Vote on entries below." }
            ]
        }"#;
        let markup = render_json(json).expect("renders");
        let html = markup.into_string();
        // All three sections present, in order.
        let composer_pos = html.find("loom-composer").expect("composer");
        let h2_pos = html.find("Top battles").expect("h2");
        let para_pos = html.find("Vote on entries").expect("paragraph");
        assert!(composer_pos < h2_pos, "composer before heading");
        assert!(h2_pos < para_pos, "heading before paragraph");
    }
}
