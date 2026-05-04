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
    /// Card-feed: an ordered list of feed cards (battle list,
    /// leaderboard rows, vote queue, etc.). Each card carries an
    /// avatar, title, optional host subline, optional stats grid,
    /// and primary link. Maps to a series of
    /// `<article class="loom-card-feed-item">` inside a
    /// `<section class="loom-card-feed">`.
    CardFeed {
        /// Optional heading rendered above the list.
        heading: Option<String>,
        /// List items, top to bottom.
        items: Vec<CmsCard>,
    },
    /// Sidebar: a stack of typed `Panel`s rendered inside an
    /// `<aside>` landmark. Used for index.html's right-rail
    /// (top earners / open votes / house rules / etc.).
    Sidebar {
        /// Optional aria-label for the `<aside>` landmark.
        /// Defaults to "Side panels" if omitted.
        label: Option<String>,
        /// Stack of panels, top to bottom.
        panels: Vec<CmsPanel>,
    },
    /// Multi-step form. Renders as `<form>` with a step indicator
    /// at top + each step's fields below + a submit row at bottom.
    /// Used for post-skill.html upload flow.
    Form {
        /// Form heading (rendered as h2 inside the section).
        legend: String,
        /// Submit-row config.
        submit: CmsFormSubmit,
        /// Ordered steps (multi-page UX rendered single-page in
        /// the SSG output; client JS can swap visibility later).
        steps: Vec<CmsFormStep>,
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

/// Submit-row config for [`CmsSection::Form`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsFormSubmit {
    /// Primary button label (e.g. "Continue → upload").
    pub label: String,
    /// Optional secondary button label (e.g. "Save draft"). None
    /// → no secondary button rendered.
    pub secondary_label: Option<String>,
    /// `<form action>` URL. Validated via `is_safe_url`.
    pub action: String,
    /// Backend key (verified by phantom_button at build time).
    pub data_backend: String,
}

/// One step inside a [`CmsSection::Form`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsFormStep {
    /// Step display label.
    pub label: String,
    /// Visual state (current / upcoming / done).
    pub state: CmsFormStepState,
    /// Fields belonging to this step.
    pub fields: Vec<CmsFormField>,
}

/// Step visual state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmsFormStepState {
    /// User is currently on this step.
    Current,
    /// Step is ahead of the user.
    Upcoming,
    /// Step has been completed.
    Done,
}

impl CmsFormStepState {
    const fn data_attr(self) -> &'static str {
        match self {
            CmsFormStepState::Current => "current",
            CmsFormStepState::Upcoming => "upcoming",
            CmsFormStepState::Done => "done",
        }
    }
}

/// Typed form field. Closed enum — adding a variant requires a
/// renderer arm + test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CmsFormField {
    /// Single-line text input.
    Text {
        /// `name` attribute (form-data key).
        name: String,
        /// Visible label.
        label: String,
        /// Optional hint paragraph below the input.
        hint: Option<String>,
        /// Optional placeholder text.
        placeholder: Option<String>,
        /// `maxlength` attribute (None → unbounded).
        max_length: Option<u32>,
        /// `required` attribute.
        #[serde(default)]
        required: bool,
    },
    /// Multi-line text input.
    Textarea {
        /// `name` attribute.
        name: String,
        /// Visible label.
        label: String,
        /// Optional hint.
        hint: Option<String>,
        /// Optional placeholder.
        placeholder: Option<String>,
        /// `maxlength` attribute.
        max_length: Option<u32>,
        /// `rows` attribute (defaults to 4 if omitted).
        #[serde(default = "default_textarea_rows")]
        rows: u32,
        /// `required` attribute.
        #[serde(default)]
        required: bool,
    },
    /// Dropdown.
    Select {
        /// `name` attribute.
        name: String,
        /// Visible label.
        label: String,
        /// Optional hint.
        hint: Option<String>,
        /// Options list.
        options: Vec<CmsSelectOption>,
        /// `required` attribute.
        #[serde(default)]
        required: bool,
    },
    /// Read-only text display (e.g. "Set automatically: 720p · 30s").
    Readonly {
        /// `name` attribute.
        name: String,
        /// Visible label.
        label: String,
        /// Optional hint.
        hint: Option<String>,
        /// Display value.
        value: String,
    },
}

fn default_textarea_rows() -> u32 {
    4
}

/// One option inside a [`CmsFormField::Select`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsSelectOption {
    /// `value` attribute.
    pub value: String,
    /// Display text.
    pub label: String,
}

/// One sidebar panel inside a [`CmsSection::Sidebar`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsPanel {
    /// Panel heading (rendered as h2).
    pub title: String,
    /// Body content. Discriminated by `kind`.
    pub body: CmsPanelBody,
}

/// Typed panel body. Closed enum — adding a variant requires a
/// renderer arm + test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CmsPanelBody {
    /// Ordered list of `{label, value, href?}` rows. Each row
    /// renders as a `<li>`; if `href` is set + valid, the row
    /// is wrapped in an `<a>`.
    List {
        /// Row entries, top to bottom.
        items: Vec<CmsPanelListItem>,
    },
    /// Plain prose paragraph(s), each entry → one `<p>`.
    Text {
        /// Paragraphs.
        paragraphs: Vec<String>,
    },
}

/// One row inside a [`CmsPanelBody::List`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsPanelListItem {
    /// Left-side label.
    pub label: String,
    /// Right-side value.
    pub value: String,
    /// Optional click target. Validated via `is_safe_url`.
    pub href: Option<String>,
    /// Optional backend key (verified by phantom_button at build).
    pub data_backend: Option<String>,
}

/// One feed card inside a [`CmsSection::CardFeed`]. Self-contained;
/// no further nesting allowed in v1.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsCard {
    /// Avatar slot. Uses the same shape as the Composer avatar so
    /// downstream Loom CSS can share the circle treatment.
    pub avatar: CmsAvatar,
    /// Card title (rendered as h3).
    pub title: String,
    /// Optional sub-line (e.g. "@court_dax · 4d left"). Rendered
    /// as `<p>` below the title.
    pub host: Option<String>,
    /// Stats grid below the body. Empty → no grid emitted.
    #[serde(default)]
    pub stats: Vec<CmsCardStat>,
    /// Primary card link target. Validated by `is_safe_url`.
    pub href: String,
    /// Backend key (verified by phase_phantom_button at build time).
    pub data_backend: String,
    /// Optional category tag (small badge above the title).
    pub tag: Option<String>,
}

/// One {label, value} pair inside a [`CmsCard`]'s stats grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CmsCardStat {
    /// Caption text (e.g. "Votes").
    pub label: String,
    /// Value text (e.g. "78%").
    pub value: String,
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
        CmsSection::CardFeed { heading, items } => html! {
            section class="loom-card-feed" data-loom-card-feed {
                @if let Some(h) = heading {
                    h2 class="loom-card-feed__heading" { (h) }
                }
                @for card in items {
                    (render_card(card))
                }
            }
        },
        CmsSection::Sidebar { label, panels } => {
            let aria = label.as_deref().unwrap_or("Side panels");
            html! {
                aside class="loom-sidebar" aria-label=(aria) {
                    @for panel in panels {
                        (render_panel(panel))
                    }
                }
            }
        }
        CmsSection::Form {
            legend,
            submit,
            steps,
        } => render_form(legend, submit, steps),
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

/// Render one feed card. Helper for `CmsSection::CardFeed`'s arm.
/// Validates the primary `href`; invalid href → `#invalid-card`
/// placeholder + `data-invalid="true"` so forge audits surface it.
fn render_card(card: &CmsCard) -> Markup {
    let href_safe = is_safe_url(&card.href);
    let href_value: &str = if href_safe {
        &card.href
    } else {
        "#invalid-card"
    };
    html! {
        article class="loom-card-feed-item" data-loom-card {
            a
                class="loom-card-feed-item__link"
                href=(href_value)
                data-backend=(card.data_backend)
                data-loom-rich-link="true"
                data-invalid=[(!href_safe).then_some("true")]
            {
                @match &card.avatar {
                    CmsAvatar::None => {}
                    CmsAvatar::Initials { letters } => {
                        div class="loom-card-feed-item__avatar" data-avatar="initials" aria-hidden="true" {
                            (letters)
                        }
                    }
                    CmsAvatar::Image { src, alt } => {
                        @if is_safe_url(src) {
                            img
                                class="loom-card-feed-item__avatar"
                                data-avatar="image"
                                src=(src)
                                alt=(alt)
                                width="48"
                                height="48"
                                loading="lazy"
                                decoding="async";
                        } @else {
                            div class="loom-card-feed-item__avatar" data-avatar="invalid-image" aria-hidden="true" {
                                "?"
                            }
                        }
                    }
                }
                div class="loom-card-feed-item__body" {
                    @if let Some(tag) = &card.tag {
                        span class="loom-card-feed-item__tag" { (tag) }
                    }
                    h3 class="loom-card-feed-item__title" { (card.title) }
                    @if let Some(host) = &card.host {
                        p class="loom-card-feed-item__host" { (host) }
                    }
                    @if !card.stats.is_empty() {
                        ul class="loom-card-feed-item__stats" aria-label="Stats" {
                            @for stat in &card.stats {
                                li class="loom-card-feed-item__stat" {
                                    span class="loom-card-feed-item__stat-label" { (stat.label) }
                                    span class="loom-card-feed-item__stat-value" { (stat.value) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render the multi-step form. Helper for `CmsSection::Form`'s arm.
fn render_form(legend: &str, submit: &CmsFormSubmit, steps: &[CmsFormStep]) -> Markup {
    let action_safe = is_safe_url(&submit.action);
    let action_value: &str = if action_safe {
        &submit.action
    } else {
        "#invalid-form-action"
    };
    html! {
        section class="loom-form-section" {
            h2 class="loom-form-section__legend" { (legend) }
            @if !steps.is_empty() {
                ol class="loom-form-section__steps" aria-label="Form progress" {
                    @for (i, step) in steps.iter().enumerate() {
                        li class="loom-form-section__step" data-state=(step.state.data_attr()) {
                            span class="loom-form-section__step-num" aria-hidden="true" { (i + 1) }
                            span class="loom-form-section__step-label" { (step.label) }
                        }
                    }
                }
            }
            form
                class="loom-form"
                method="post"
                action=(action_value)
                data-backend=(submit.data_backend)
                data-invalid=[(!action_safe).then_some("true")]
            {
                @for step in steps {
                    fieldset class="loom-form__step" data-state=(step.state.data_attr()) {
                        legend class="loom-form__step-legend" { (step.label) }
                        @for field in &step.fields {
                            (render_form_field(field))
                        }
                    }
                }
                div class="loom-form__submit-row" {
                    @if let Some(secondary) = &submit.secondary_label {
                        button
                            class="loom-form__btn"
                            data-variant="ghost"
                            type="button"
                            data-backend=(submit.data_backend)
                            data-loom-rich-link="true"
                        {
                            (secondary)
                        }
                    }
                    button
                        class="loom-form__btn"
                        data-variant="primary"
                        type="submit"
                        data-backend=(submit.data_backend)
                        data-loom-rich-link="true"
                    {
                        (submit.label)
                    }
                }
            }
        }
    }
}

fn render_form_field(field: &CmsFormField) -> Markup {
    match field {
        // SECURITY/A11Y: every form field carries BOTH `<label for>`
        // (programmatic association) AND `aria-label` (explicit
        // accessible name). The redundancy is intentional — Chromium
        // accessibility tree sometimes fails to associate a label
        // sibling inside a <fieldset> with its <legend>, leaving the
        // textbox unnamed in the AT (caught by the crawler's
        // axe-static-a11y axis). aria-label guarantees the name
        // regardless of the for/id binding state.
        CmsFormField::Text {
            name,
            label,
            hint,
            placeholder,
            max_length,
            required,
        } => html! {
            div class="loom-form-field" {
                label class="loom-form-field__label" for=(name) { (label) }
                input
                    class="loom-form-field__input"
                    type="text"
                    id=(name)
                    name=(name)
                    aria-label=(label)
                    placeholder=[placeholder.as_deref()]
                    maxlength=[max_length.map(|m| m.to_string())]
                    required=[required.then_some("required")];
                @if let Some(h) = hint {
                    p class="loom-form-field__hint" { (h) }
                }
            }
        },
        CmsFormField::Textarea {
            name,
            label,
            hint,
            placeholder,
            max_length,
            rows,
            required,
        } => html! {
            div class="loom-form-field" {
                label class="loom-form-field__label" for=(name) { (label) }
                textarea
                    class="loom-form-field__textarea"
                    id=(name)
                    name=(name)
                    aria-label=(label)
                    rows=(rows)
                    placeholder=[placeholder.as_deref()]
                    maxlength=[max_length.map(|m| m.to_string())]
                    required=[required.then_some("required")] {}
                @if let Some(h) = hint {
                    p class="loom-form-field__hint" { (h) }
                }
            }
        },
        CmsFormField::Select {
            name,
            label,
            hint,
            options,
            required,
        } => html! {
            div class="loom-form-field" {
                label class="loom-form-field__label" for=(name) { (label) }
                select
                    class="loom-form-field__select"
                    id=(name)
                    name=(name)
                    aria-label=(label)
                    required=[required.then_some("required")]
                {
                    @for opt in options {
                        option value=(opt.value) { (opt.label) }
                    }
                }
                @if let Some(h) = hint {
                    p class="loom-form-field__hint" { (h) }
                }
            }
        },
        CmsFormField::Readonly {
            name,
            label,
            hint,
            value,
        } => html! {
            div class="loom-form-field" {
                label class="loom-form-field__label" for=(name) { (label) }
                input
                    class="loom-form-field__input"
                    type="text"
                    id=(name)
                    name=(name)
                    aria-label=(label)
                    value=(value)
                    readonly;
                @if let Some(h) = hint {
                    p class="loom-form-field__hint" { (h) }
                }
            }
        },
    }
}

/// Render one sidebar panel. Helper for `CmsSection::Sidebar`'s arm.
fn render_panel(panel: &CmsPanel) -> Markup {
    html! {
        section class="loom-panel" {
            h2 class="loom-panel__title" { (panel.title) }
            (render_panel_body(&panel.body))
        }
    }
}

fn render_panel_body(body: &CmsPanelBody) -> Markup {
    match body {
        CmsPanelBody::List { items } => html! {
            ul class="loom-panel__list" {
                @for item in items {
                    @let href_safe = item.href.as_deref().map(is_safe_url).unwrap_or(false);
                    li class="loom-panel__list-item" {
                        @if let (Some(href), true) = (item.href.as_deref(), href_safe) {
                            a class="loom-panel__list-link" href=(href) data-backend=[item.data_backend.as_deref()] data-loom-rich-link="true" {
                                span class="loom-panel__list-label" { (item.label) }
                                span class="loom-panel__list-value" { (item.value) }
                            }
                        } @else if item.href.is_some() {
                            // href present but failed validation
                            span class="loom-panel__list-link" data-invalid="true" {
                                span class="loom-panel__list-label" { (item.label) }
                                span class="loom-panel__list-value" { (item.value) }
                            }
                        } @else {
                            span class="loom-panel__list-label" { (item.label) }
                            span class="loom-panel__list-value" { (item.value) }
                        }
                    }
                }
            }
        },
        CmsPanelBody::Text { paragraphs } => html! {
            div class="loom-panel__body" {
                @for paragraph in paragraphs {
                    p class="loom-panel__paragraph" { (paragraph) }
                }
            }
        },
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

    fn card(title: &str, href: &str) -> CmsCard {
        CmsCard {
            avatar: CmsAvatar::Initials {
                letters: "DA".to_owned(),
            },
            title: title.to_owned(),
            host: Some("@court_dax · 4d left".to_owned()),
            stats: vec![
                CmsCardStat {
                    label: "Votes".to_owned(),
                    value: "78%".to_owned(),
                },
                CmsCardStat {
                    label: "Pot".to_owned(),
                    value: "$240".to_owned(),
                },
            ],
            href: href.to_owned(),
            data_backend: "view-challenge".to_owned(),
            tag: Some("Parkour".to_owned()),
        }
    }

    #[test]
    fn card_feed_renders_each_item() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: Some("Top battles".to_owned()),
                items: vec![card("Battle A", "/c/a"), card("Battle B", "/c/b")],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"class="loom-card-feed""#));
        assert!(html.contains(">Top battles<"));
        assert!(html.contains(">Battle A<"));
        assert!(html.contains(">Battle B<"));
        // Two items → two article tags.
        assert_eq!(html.matches("<article ").count(), 2);
    }

    #[test]
    fn card_feed_no_heading_omits_h2() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: None,
                items: vec![card("Only", "/c/only")],
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("loom-card-feed__heading"));
        assert!(html.contains(">Only<"));
    }

    #[test]
    fn card_emits_stats_grid_when_present() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: None,
                items: vec![card("X", "/x")],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"aria-label="Stats""#));
        assert!(html.contains(">Votes<"));
        assert!(html.contains(">78%<"));
        assert!(html.contains(">Pot<"));
        assert!(html.contains(">$240<"));
    }

    #[test]
    fn card_omits_stats_grid_when_empty() {
        let mut c = card("X", "/x");
        c.stats.clear();
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: None,
                items: vec![c],
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("loom-card-feed-item__stats"));
    }

    #[test]
    fn card_invalid_href_substitutes_placeholder() {
        let mut c = card("X", "javascript:alert(1)");
        c.tag = None;
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: None,
                items: vec![c],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r##"href="#invalid-card""##));
        assert!(html.contains(r#"data-invalid="true""#));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn card_text_fields_are_escaped() {
        let mut c = card("<script>alert</script>", "/x");
        c.host = Some("<img onerror=x>".to_owned());
        c.tag = Some("<x>".to_owned());
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: None,
                items: vec![c],
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("<script>alert"));
        assert!(!html.contains("<img onerror"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;img"));
    }

    #[test]
    fn card_image_avatar_with_invalid_src_falls_back() {
        let mut c = card("X", "/x");
        c.avatar = CmsAvatar::Image {
            src: "javascript:alert(1)".to_owned(),
            alt: "evil".to_owned(),
        };
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: None,
                items: vec![c],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"data-avatar="invalid-image""#));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn card_feed_empty_items_emits_only_section_wrapper() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::CardFeed {
                heading: Some("Empty list".to_owned()),
                items: vec![],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"class="loom-card-feed""#));
        assert!(html.contains(">Empty list<"));
        assert!(!html.contains("<article "));
    }

    #[test]
    fn sidebar_renders_aside_with_aria_label() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Sidebar {
                label: Some("Right rail".to_owned()),
                panels: vec![],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<aside class="loom-sidebar" aria-label="Right rail">"#));
    }

    #[test]
    fn sidebar_default_label_is_side_panels() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Sidebar {
                label: None,
                panels: vec![],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"aria-label="Side panels""#));
    }

    #[test]
    fn panel_with_list_body_renders_each_row() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Sidebar {
                label: None,
                panels: vec![CmsPanel {
                    title: "Top earners".to_owned(),
                    body: CmsPanelBody::List {
                        items: vec![
                            CmsPanelListItem {
                                label: "@court_dax".to_owned(),
                                value: "$1,840".to_owned(),
                                href: Some("/u/court_dax".to_owned()),
                                data_backend: Some("view-profile".to_owned()),
                            },
                            CmsPanelListItem {
                                label: "@vault_kit".to_owned(),
                                value: "$1,420".to_owned(),
                                href: None,
                                data_backend: None,
                            },
                        ],
                    },
                }],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(">Top earners<"));
        assert!(html.contains(">@court_dax<"));
        assert!(html.contains(">$1,840<"));
        assert!(html.contains(r#"href="/u/court_dax""#));
        assert!(html.contains(r#"data-backend="view-profile""#));
        assert!(html.contains(">@vault_kit<"));
    }

    #[test]
    fn panel_with_text_body_renders_each_paragraph() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Sidebar {
                label: None,
                panels: vec![CmsPanel {
                    title: "House rules".to_owned(),
                    body: CmsPanelBody::Text {
                        paragraphs: vec!["Rule one.".to_owned(), "Rule two.".to_owned()],
                    },
                }],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(">House rules<"));
        assert!(html.contains(">Rule one.<"));
        assert!(html.contains(">Rule two.<"));
    }

    #[test]
    fn panel_list_invalid_href_falls_back_to_span() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Sidebar {
                label: None,
                panels: vec![CmsPanel {
                    title: "x".to_owned(),
                    body: CmsPanelBody::List {
                        items: vec![CmsPanelListItem {
                            label: "evil".to_owned(),
                            value: "x".to_owned(),
                            href: Some("javascript:alert(1)".to_owned()),
                            data_backend: None,
                        }],
                    },
                }],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"data-invalid="true""#));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn panel_text_body_is_escaped() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Sidebar {
                label: None,
                panels: vec![CmsPanel {
                    title: "<script>".to_owned(),
                    body: CmsPanelBody::Text {
                        paragraphs: vec!["<img onerror=x>".to_owned()],
                    },
                }],
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;img"));
    }

    fn simple_form() -> CmsSection {
        CmsSection::Form {
            legend: "Post a skill".to_owned(),
            submit: CmsFormSubmit {
                label: "Continue".to_owned(),
                secondary_label: Some("Save draft".to_owned()),
                action: "/post-skill".to_owned(),
                data_backend: "post-skill".to_owned(),
            },
            steps: vec![CmsFormStep {
                label: "Rules & category".to_owned(),
                state: CmsFormStepState::Current,
                fields: vec![
                    CmsFormField::Text {
                        name: "title".to_owned(),
                        label: "Challenge title".to_owned(),
                        hint: Some("State the SHOT, not the difficulty.".to_owned()),
                        placeholder: Some("e.g. Half-court shot".to_owned()),
                        max_length: Some(120),
                        required: true,
                    },
                    CmsFormField::Textarea {
                        name: "rules".to_owned(),
                        label: "Rules".to_owned(),
                        hint: None,
                        placeholder: None,
                        max_length: None,
                        rows: 6,
                        required: false,
                    },
                ],
            }],
        }
    }

    fn form_page() -> CmsPage {
        CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![simple_form()],
        }
    }

    #[test]
    fn form_renders_legend_action_and_submit() {
        let html = render_to_string(&form_page());
        assert!(html.contains(">Post a skill<"));
        assert!(html.contains(r#"action="/post-skill""#));
        assert!(html.contains(r#"data-backend="post-skill""#));
        assert!(html.contains(r#"type="submit""#));
        assert!(html.contains(">Continue<"));
        // Secondary button.
        assert!(html.contains(">Save draft<"));
    }

    #[test]
    fn form_steps_indicator_emits_state_attribute() {
        let html = render_to_string(&form_page());
        assert!(html.contains(r#"data-state="current""#));
        assert!(html.contains(">Rules &amp; category<"));
    }

    #[test]
    fn form_text_field_with_attrs() {
        let html = render_to_string(&form_page());
        assert!(html.contains(r#"id="title""#));
        assert!(html.contains(r#"name="title""#));
        assert!(html.contains(r#"placeholder="e.g. Half-court shot""#));
        assert!(html.contains(r#"maxlength="120""#));
        assert!(html.contains(r#"required="required""#));
        assert!(html.contains(">Challenge title<"));
        assert!(html.contains(">State the SHOT"));
    }

    #[test]
    fn form_textarea_field_with_default_rows() {
        let html = render_to_string(&form_page());
        assert!(html.contains(r#"rows="6""#));
        assert!(html.contains("loom-form-field__textarea"));
    }

    #[test]
    fn form_select_field() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                steps: vec![CmsFormStep {
                    label: "Pick".to_owned(),
                    state: CmsFormStepState::Current,
                    fields: vec![CmsFormField::Select {
                        name: "category".to_owned(),
                        label: "Category".to_owned(),
                        hint: None,
                        options: vec![
                            CmsSelectOption {
                                value: "basketball".to_owned(),
                                label: "Basketball".to_owned(),
                            },
                            CmsSelectOption {
                                value: "parkour".to_owned(),
                                label: "Parkour".to_owned(),
                            },
                        ],
                        required: true,
                    }],
                }],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<select"#));
        assert!(html.contains(r#"value="basketball""#));
        assert!(html.contains(">Basketball<"));
        assert!(html.contains(">Parkour<"));
    }

    #[test]
    fn form_readonly_field_is_readonly() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                steps: vec![CmsFormStep {
                    label: "Format".to_owned(),
                    state: CmsFormStepState::Done,
                    fields: vec![CmsFormField::Readonly {
                        name: "format".to_owned(),
                        label: "Required video format".to_owned(),
                        hint: Some("Set automatically.".to_owned()),
                        value: "720p · 30s".to_owned(),
                    }],
                }],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"value="720p · 30s""#));
        assert!(html.contains("readonly"));
    }

    #[test]
    fn form_invalid_action_substitutes_placeholder() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "javascript:alert(1)".to_owned(),
                    data_backend: "x".to_owned(),
                },
                steps: vec![],
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r##"action="#invalid-form-action""##));
        assert!(html.contains(r#"data-invalid="true""#));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn form_field_text_is_escaped() {
        let p = CmsPage {
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Form {
                legend: "<script>".to_owned(),
                submit: CmsFormSubmit {
                    label: "<x>".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                steps: vec![CmsFormStep {
                    label: "<step>".to_owned(),
                    state: CmsFormStepState::Current,
                    fields: vec![CmsFormField::Text {
                        name: "n".to_owned(),
                        label: "<lbl>".to_owned(),
                        hint: Some("<hint>".to_owned()),
                        placeholder: None,
                        max_length: None,
                        required: false,
                    }],
                }],
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;step&gt;"));
        assert!(html.contains("&lt;lbl&gt;"));
        assert!(html.contains("&lt;hint&gt;"));
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
