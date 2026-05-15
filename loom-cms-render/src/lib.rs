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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsPage {
    /// JSON Schema reference. Editors with jsonls (VS Code,
    /// Helix, Zed) read this to provide inline autocomplete +
    /// validation. The bridge ignores the value — it's the
    /// editor's contract, not the renderer's. Authors should
    /// generate the schema via `loom cms-schema --out ...` and
    /// reference it here as `"$schema": "../cms-schema.json"`.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none", default)]
    #[schemars(rename = "$schema")]
    pub schema: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
        ///
        /// REGRESSION-GUARD: serde alias `subtitle` accepts the
        /// pre-2026-05 field name. Older fixture files written by
        /// the form-builder still use `subtitle`; without the alias
        /// the renderer 500s on read because `deny_unknown_fields`
        /// rejects unknown keys. Save-path scrubbing handles the
        /// write side; this handles the read side. Cycle 52
        /// /preview-edit/about.html 500 was the trigger.
        #[serde(alias = "subtitle")]
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
    /// Top-of-page persistent banner. Used for "voting closes in
    /// 1h", "maintenance Saturday 2-4am", "PoC build" type notices.
    /// Renders as an `<aside>` with a tone-tinted background and an
    /// optional close button.
    Banner {
        /// Visual tone — info / warn / success / danger.
        tone: CmsBannerTone,
        /// Notice text. Maud auto-escapes.
        text: String,
        /// If true, render a close button with
        /// `data-loom-banner-dismiss` (client JS handles).
        #[serde(default)]
        dismissible: bool,
        /// Optional stable id used by client JS to remember a
        /// dismissal across page loads.
        id: Option<String>,
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
    /// A heading. Level constrained to 2-6 (h1 is owned by the
    /// page-shell template, not section content). T36 (2026-05-14):
    /// `level` is a typed `HeadingLevel` enum so out-of-range
    /// values are uncompilable AND parse-failed at the
    /// JSON boundary — no runtime clamp surface.
    Heading {
        /// Heading text.
        text: String,
        /// Typed heading level. JSON: integer 2..=6 (anything
        /// else fails parse with `deny_unknown_fields`-style
        /// strictness).
        level: HeadingLevel,
    },
}

/// T36: typed heading level for `CmsSection::Heading`.
///
/// `<h1>` is reserved for the page-shell template (every page has
/// exactly one); section headings live at h2..=h6. The variants
/// here mirror the HTML tag set 1:1 — there is no `H1` variant
/// because emitting a second h1 from CMS content would break
/// landmark semantics.
///
/// JSON wire format: integer `2..=6`. `serde` round-trips through
/// `u8`; out-of-range values fail `Deserialize` at the boundary
/// with a clear error, never landing as runtime drift.
///
/// Why typed instead of `u8`:
///   * **Compile-time guarantee** — `HeadingLevel::H7` doesn't
///     compile. Runtime clamps + `data-cms-warn` markers no
///     longer needed; the level is always valid by construction.
///   * **Exhaustive match** — every consumer matches on the
///     enum and the compiler refuses to forget a variant.
///   * **Type-state doctrine** — moves a runtime invariant into
///     the type system, where AVP-2's "no boolean blindness"
///     rule belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingLevel {
    /// `<h2>` — top-level section heading inside a page.
    H2,
    /// `<h3>` — subsection.
    H3,
    /// `<h4>`.
    H4,
    /// `<h5>`.
    H5,
    /// `<h6>`.
    H6,
}

impl HeadingLevel {
    /// Tag name (`"h2"`..`"h6"`).
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::H2 => "h2",
            Self::H3 => "h3",
            Self::H4 => "h4",
            Self::H5 => "h5",
            Self::H6 => "h6",
        }
    }

    /// Numeric level (`2`..`6`).
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::H2 => 2,
            Self::H3 => 3,
            Self::H4 => 4,
            Self::H5 => 5,
            Self::H6 => 6,
        }
    }

    /// Construct from a numeric level. Returns `None` for any
    /// value outside `2..=6`.
    #[must_use]
    pub const fn from_u8(n: u8) -> Option<Self> {
        match n {
            2 => Some(Self::H2),
            3 => Some(Self::H3),
            4 => Some(Self::H4),
            5 => Some(Self::H5),
            6 => Some(Self::H6),
            _ => None,
        }
    }
}

impl Serialize for HeadingLevel {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_u8(self.as_u8())
    }
}

impl<'de> Deserialize<'de> for HeadingLevel {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let n = u8::deserialize(de)?;
        Self::from_u8(n).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "heading level must be 2..=6 (h1 is owned by the page-shell), got {n}"
            ))
        })
    }
}

impl schemars::JsonSchema for HeadingLevel {
    fn schema_name() -> String {
        "HeadingLevel".to_owned()
    }
    fn json_schema(_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        // Integer enum 2..=6 — the editor / IDE autocomplete sees
        // exactly the valid values.
        let mut obj = schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::Integer.into()),
            ..Default::default()
        };
        obj.enum_values = Some(vec![
            serde_json::json!(2),
            serde_json::json!(3),
            serde_json::json!(4),
            serde_json::json!(5),
            serde_json::json!(6),
        ]);
        obj.metadata().description = Some(
            "Heading level (h2-h6). h1 is reserved for the page-shell.".to_owned(),
        );
        schemars::schema::Schema::Object(obj)
    }
}

/// Hero CTA — the single typed primary action attached to a Hero
/// section. URL is validated by `composer::is_safe_url` at render
/// time.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

/// Banner tone — closed enum mirroring the standard color
/// roles. Each maps to a `data-tone` attribute the skin styles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CmsBannerTone {
    /// Neutral, primary-tinted (default).
    Info,
    /// Yellow-tinted; use for time-sensitive / actionable notices.
    Warn,
    /// Green-tinted; use for confirmations.
    Success,
    /// Red-tinted; use for errors / critical alerts.
    Danger,
}

impl CmsBannerTone {
    const fn data_attr(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Success => "success",
            Self::Danger => "danger",
        }
    }
}

/// Submit-row config for [`CmsSection::Form`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
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
            Self::Current => "current",
            Self::Upcoming => "upcoming",
            Self::Done => "done",
        }
    }
}

/// Typed form field. Closed enum — adding a variant requires a
/// renderer arm + test.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

const fn default_textarea_rows() -> u32 {
    4
}

/// One option inside a [`CmsFormField::Select`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsSelectOption {
    /// `value` attribute.
    pub value: String,
    /// Display text.
    pub label: String,
}

/// One sidebar panel inside a [`CmsSection::Sidebar`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsPanel {
    /// Panel heading (rendered as h2).
    pub title: String,
    /// Body content. Discriminated by `kind`.
    pub body: CmsPanelBody,
}

/// Typed panel body. Closed enum — adding a variant requires a
/// renderer arm + test.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsCardStat {
    /// Caption text (e.g. "Votes").
    pub label: String,
    /// Value text (e.g. "78%").
    pub value: String,
}

/// Closed enum mirror of [`PromptAction`] — separated so the wire
/// format is independent of the Loom enum's internals.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
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
            CmsPromptAction::UploadClip => Self::UploadClip,
            CmsPromptAction::ChallengeOpponent => Self::ChallengeOpponent,
            CmsPromptAction::GoLive => Self::GoLive,
            CmsPromptAction::PhotoOnly => Self::PhotoOnly,
        }
    }
}

/// Mirror of [`ComposerSize`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
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
            CmsComposerSize::Compact => Self::Compact,
            CmsComposerSize::Comfortable => Self::Comfortable,
        }
    }
}

/// Wire form of [`ComposerAvatar`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
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
            CmsLoading::Lazy => Self::Lazy,
            CmsLoading::Eager => Self::Eager,
        }
    }
}

/// Mirror of [`PicturePriority`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
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
            CmsPriority::Auto => Self::Auto,
            CmsPriority::High => Self::High,
            CmsPriority::Low => Self::Low,
        }
    }
}

/// Mirror of [`PictureFit`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
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
            CmsFit::Default => Self::Default,
            CmsFit::Cover => Self::Cover,
            CmsFit::Contain => Self::Contain,
        }
    }
}

/// Render a complete CMS page to Loom markup. The output is a
/// `<div class="loom-page">` containing one rendered subtree per
/// section, in order.
///
/// PAGE-SHELL CONTRACT: this function emits NO `<html>`, `<head>`,
/// `<title>`, `<h1>`, or `<main>`. Those belong to the page-shell
/// template (`page_shell` in this crate). The bridge focuses on
/// the body REGION the CMS owns.
///
/// REGRESSION-GUARD T70b-fix (2026-05-14): formerly emitted
/// `<main class="loom-page">` which produced nested `<main>` tags
/// when wrapped by `page_shell` (which emits its own
/// `<main id="content">`). WCAG forbids more than one `<main>`
/// per document. The wrapper is now `<div>`; the landmark stays
/// in `page_shell`.
#[must_use]
pub fn render_page(page: &CmsPage) -> Markup {
    html! {
        div class="loom-page" data-cms-path=(page.path) {
            @for section in &page.sections {
                (render_section(section))
            }
        }
    }
}

/// Render one CMS section to Loom markup.
#[must_use]
#[allow(clippy::too_many_lines)] // single match over every CmsSection variant.
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
                .is_none_or(|c| loom_components::composer::is_safe_url(&c.href));
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
        CmsSection::Banner {
            tone,
            text,
            dismissible,
            id,
        } => html! {
            // SECURITY/A11Y: <aside> already has implicit landmark
            // role 'complementary' — explicit role='status' would
            // be redundant AND axe-rejected (aria-allowed-role).
            // For static-rendered banners, the natural reading
            // order announces the text on first paint; aria-live
            // is unnecessary. If a future variant needs to inject
            // banners post-load, add CmsSection::Toast with
            // role='status' on a <div> wrapper.
            aside
                class="loom-banner"
                data-tone=(tone.data_attr())
                data-loom-banner-id=[id.as_deref()]
            {
                p class="loom-banner__text" { (text) }
                @if *dismissible {
                    button
                        class="loom-banner__dismiss"
                        type="button"
                        data-loom-banner-dismiss
                        aria-label="Dismiss notice"
                    {
                        "×"
                    }
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
            // T36 (2026-05-14): typed HeadingLevel enum makes
            // out-of-range values uncompilable. The runtime clamp
            // + data-cms-warn fallback are gone — invalid levels
            // never reach this match (Deserialize fails first at
            // the JSON boundary).
            //
            // Future: enabling `non_exhaustive` on HeadingLevel
            // would turn this into an explicit-arm match; for
            // now the compiler exhaustiveness check is enough.
            match level {
                HeadingLevel::H2 => html! {
                    h2 class="loom-heading" data-loom-level="2" { (text) }
                },
                HeadingLevel::H3 => html! {
                    h3 class="loom-heading" data-loom-level="3" { (text) }
                },
                HeadingLevel::H4 => html! {
                    h4 class="loom-heading" data-loom-level="4" { (text) }
                },
                HeadingLevel::H5 => html! {
                    h5 class="loom-heading" data-loom-level="5" { (text) }
                },
                HeadingLevel::H6 => html! {
                    h6 class="loom-heading" data-loom-level="6" { (text) }
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

/// Render the visible required-field marker (` *`) — but only the
/// VISIBLE part. Screen readers learn the required state from the
/// `required` attribute on the input/textarea/select itself, so
/// the asterisk gets `aria-hidden="true"` to avoid double-announce
/// ("Challenge title required asterisk").
///
/// CSS hook: `.loom-form-field__required` is styled red via
/// `--loom-color-danger` in skin.css. Closes the
/// `form.required-no-indicator` finding from the
/// 2026-05-14 SkillShots dogfood run (Crawler T76) at the source —
/// every Forge-generated site picks up the indicator automatically.
fn render_required_marker(required: bool) -> Markup {
    html! {
        @if required {
            span class="loom-form-field__required" aria-hidden="true" { " *" }
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
                label class="loom-form-field__label" for=(name) {
                    (label)
                    (render_required_marker(*required))
                }
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
                label class="loom-form-field__label" for=(name) {
                    (label)
                    (render_required_marker(*required))
                }
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
                label class="loom-form-field__label" for=(name) {
                    (label)
                    (render_required_marker(*required))
                }
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
                    @let href_safe = item.href.as_deref().is_some_and(is_safe_url);
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
///
/// # Errors
/// Forwards any `serde_json::Error` raised while deserializing the
/// document — schema mismatch (unknown field, wrong tag), bad
/// types, or malformed JSON.
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
    fn empty_page_renders_div_wrapper() {
        // T70b-fix (2026-05-14): wrapper is now <div>, not <main>.
        // The <main> landmark belongs to page_shell, not render_page,
        // to avoid nested <main> tags in the composed output.
        let p = CmsPage {
            schema: None,
            title: "Home".to_owned(),
            description: "x".to_owned(),
            path: "/".to_owned(),
            nav_links: vec![],
            sections: vec![],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<div class="loom-page""#));
        assert!(!html.contains(r#"<main class="loom-page""#),
            "render_page must NOT emit <main> — page_shell owns the landmark");
        assert!(html.contains(r#"data-cms-path="/""#));
    }

    #[test]
    fn paragraph_renders_loom_prose() {
        let p = CmsPage {
            schema: None,
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
            schema: None,
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
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Heading {
                text: "Section".to_owned(),
                level: HeadingLevel::H2,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<h2 class="loom-heading" data-loom-level="2""#));
        // T36: data-cms-warn no longer emitted — invalid levels
        // can't construct, so no clamp surface to warn about.
        assert!(!html.contains("data-cms-warn"));
    }

    #[test]
    fn heading_level_3_through_6_render_correctly() {
        // T36: full h2-h6 coverage now (was h2-h4 with clamp).
        for (level, expected_tag) in [
            (HeadingLevel::H3, "h3"),
            (HeadingLevel::H4, "h4"),
            (HeadingLevel::H5, "h5"),
            (HeadingLevel::H6, "h6"),
        ] {
            let p = CmsPage {
                schema: None,
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
                "level {level:?} → {expected_tag}: {html}"
            );
        }
    }

    /// T36 REGRESSION-GUARD: out-of-range levels in JSON are now
    /// REJECTED at parse time (was: clamped to h2 with warn).
    /// This test pins the new fail-closed behaviour.
    #[test]
    fn heading_level_out_of_range_fails_deserialize() {
        for bad in [0u8, 1, 7, 99, 255] {
            let json = format!(
                r#"{{"title":"x","description":"x","path":"/","sections":[
                   {{"kind":"heading","level":{bad},"text":"x"}}
                ]}}"#
            );
            let r: Result<CmsPage, _> = serde_json::from_str(&json);
            assert!(
                r.is_err(),
                "level {bad} must fail deserialize, got: {:?}",
                r
            );
            let err_msg = r.unwrap_err().to_string();
            assert!(
                err_msg.contains("2..=6") || err_msg.contains("h1"),
                "error must explain valid range, got: {err_msg}"
            );
        }
    }

    #[test]
    fn heading_level_in_range_round_trips() {
        for n in 2u8..=6 {
            let json = format!(
                r#"{{"title":"x","description":"x","path":"/","sections":[
                   {{"kind":"heading","level":{n},"text":"x"}}
                ]}}"#
            );
            let p: CmsPage = serde_json::from_str(&json).expect("valid level parses");
            // Round-trip the level through the typed enum.
            if let CmsSection::Heading { level, .. } = &p.sections[0] {
                assert_eq!(level.as_u8(), n);
            } else {
                panic!("section is not a heading");
            }
        }
    }

    #[test]
    fn heading_level_serialize_emits_integer() {
        let level = HeadingLevel::H3;
        let s = serde_json::to_string(&level).expect("serialize");
        assert_eq!(s, "3");
    }

    #[test]
    fn heading_level_from_u8_rejects_out_of_range() {
        assert_eq!(HeadingLevel::from_u8(0), None);
        assert_eq!(HeadingLevel::from_u8(1), None);
        assert_eq!(HeadingLevel::from_u8(2), Some(HeadingLevel::H2));
        assert_eq!(HeadingLevel::from_u8(6), Some(HeadingLevel::H6));
        assert_eq!(HeadingLevel::from_u8(7), None);
        assert_eq!(HeadingLevel::from_u8(255), None);
    }

    #[test]
    fn heading_level_tag_strings() {
        assert_eq!(HeadingLevel::H2.tag(), "h2");
        assert_eq!(HeadingLevel::H6.tag(), "h6");
    }

    #[test]
    fn composer_section_renders_loom_composer() {
        let p = CmsPage {
            schema: None,
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
            schema: None,
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
    fn hero_legacy_subtitle_field_alias_to_lede() {
        // REGRESSION-GUARD cycle 52: pre-2026-05 fixtures used
        // `subtitle` on Hero before the field was renamed to
        // `lede`. Without the serde alias, a legacy on-disk
        // cms/about.json 500s the renderer because
        // `deny_unknown_fields` rejects `subtitle`.
        let json = r#"{
            "title": "x",
            "description": "x",
            "path": "/x",
            "sections": [
                {
                    "kind": "hero",
                    "eyebrow": "Hello",
                    "title": "Hi there",
                    "subtitle": "Edited subtitle",
                    "cta": null
                }
            ]
        }"#;
        let r = render_json(json);
        assert!(
            r.is_ok(),
            "legacy `subtitle` field should alias to `lede`: {:?}",
            r.err()
        );
        let html = r.unwrap().0;
        assert!(
            html.contains("loom-section-hero__lede"),
            "lede should render from aliased `subtitle` field"
        );
        assert!(html.contains("Edited subtitle"));
    }

    #[test]
    fn hero_renders_required_title_only() {
        let p = CmsPage {
            schema: None,
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
            schema: None,
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
            schema: None,
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
        assert!(html.contains(r#"data-invalid="true""#));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn hero_text_is_escaped() {
        let p = CmsPage {
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
            schema: None,
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
        assert!(html.contains("<select"));
        assert!(html.contains(r#"value="basketball""#));
        assert!(html.contains(">Basketball<"));
        assert!(html.contains(">Parkour<"));
    }

    // T76 (Crawler dogfood 2026-05-14): required form fields must
    // render a visible required-marker (`*`) AND remain accessible
    // (aria-hidden on the marker, `required` attr on the control,
    // label text intact).
    #[test]
    fn required_text_field_renders_visible_marker() {
        let html = render_to_string(&form_page());
        // The required Text field's label MUST contain a visible
        // marker. The form fixture's `Challenge title` is required.
        let title_label_pos = html
            .find(">Challenge title")
            .expect("'Challenge title' label present");
        let after = &html[title_label_pos..title_label_pos + 200];
        assert!(
            after.contains(r#"class="loom-form-field__required""#),
            "required marker span present after Challenge title label: {after}"
        );
        // Marker text is " *".
        assert!(after.contains(" *"), "marker contains visible '*': {after}");
        // aria-hidden so screen readers don't double-announce.
        assert!(
            after.contains(r#"aria-hidden="true""#),
            "marker is aria-hidden: {after}"
        );
    }

    #[test]
    fn non_required_text_field_omits_marker() {
        // simple_form's "rules" textarea has required=false. The
        // rendered label must NOT carry the marker span.
        let html = render_to_string(&form_page());
        let rules_label_pos = html.find(">Rules<").expect("'Rules' label present");
        let after = &html[rules_label_pos..rules_label_pos + 80];
        assert!(
            !after.contains("loom-form-field__required"),
            "non-required field must NOT render marker: {after}"
        );
    }

    #[test]
    fn required_select_renders_marker() {
        // Reuse the select test's CmsPage with required=true.
        let p = CmsPage {
            schema: None,
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
                        options: vec![CmsSelectOption {
                            value: "a".to_owned(),
                            label: "A".to_owned(),
                        }],
                        required: true,
                    }],
                }],
            }],
        };
        let html = render_to_string(&p);
        let pos = html.find(">Category").expect("Category label");
        let after = &html[pos..pos + 200];
        assert!(after.contains(r#"class="loom-form-field__required""#));
        assert!(after.contains(" *"));
    }

    #[test]
    fn required_textarea_renders_marker() {
        // Build a fresh page with a required Textarea (simple_form's
        // textarea has required=false).
        let p = CmsPage {
            schema: None,
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
                    label: "Bio".to_owned(),
                    state: CmsFormStepState::Current,
                    fields: vec![CmsFormField::Textarea {
                        name: "bio".to_owned(),
                        label: "Your bio".to_owned(),
                        hint: None,
                        placeholder: None,
                        max_length: None,
                        rows: 4,
                        required: true,
                    }],
                }],
            }],
        };
        let html = render_to_string(&p);
        let pos = html.find(">Your bio").expect("Your bio label");
        let after = &html[pos..pos + 200];
        assert!(after.contains(r#"class="loom-form-field__required""#));
        assert!(after.contains(" *"));
    }

    #[test]
    fn form_readonly_field_is_readonly() {
        let p = CmsPage {
            schema: None,
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
            schema: None,
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
            schema: None,
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

    fn banner_page(
        tone: CmsBannerTone,
        text: &str,
        dismissible: bool,
        id: Option<&str>,
    ) -> CmsPage {
        CmsPage {
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            sections: vec![CmsSection::Banner {
                tone,
                text: text.to_owned(),
                dismissible,
                id: id.map(ToOwned::to_owned),
            }],
        }
    }

    #[test]
    fn banner_renders_aside_with_tone_attribute() {
        let html = render_to_string(&banner_page(CmsBannerTone::Info, "Heads up!", false, None));
        assert!(html.contains(r#"class="loom-banner""#));
        // <aside> has implicit role="complementary"; no explicit
        // role attribute is set (axe rejects role="status" on aside).
        assert!(!html.contains(r#"role="status""#));
        assert!(html.contains("<aside"));
        assert!(html.contains(r#"data-tone="info""#));
        assert!(html.contains(">Heads up!<"));
        // Non-dismissible: no close button.
        assert!(!html.contains("loom-banner__dismiss"));
    }

    #[test]
    fn banner_each_tone_emits_correct_data_attr() {
        for (tone, attr) in [
            (CmsBannerTone::Info, "info"),
            (CmsBannerTone::Warn, "warn"),
            (CmsBannerTone::Success, "success"),
            (CmsBannerTone::Danger, "danger"),
        ] {
            let html = render_to_string(&banner_page(tone, "x", false, None));
            assert!(
                html.contains(&format!(r#"data-tone="{attr}""#)),
                "tone {attr}: {html}"
            );
        }
    }

    #[test]
    fn banner_dismissible_emits_close_button() {
        let html = render_to_string(&banner_page(CmsBannerTone::Warn, "x", true, None));
        assert!(html.contains("loom-banner__dismiss"));
        assert!(html.contains("data-loom-banner-dismiss"));
        assert!(html.contains(r#"aria-label="Dismiss notice""#));
        assert!(html.contains(">×<"));
    }

    #[test]
    fn banner_id_emits_data_attribute() {
        let html = render_to_string(&banner_page(
            CmsBannerTone::Info,
            "x",
            true,
            Some("poc-2026"),
        ));
        assert!(html.contains(r#"data-loom-banner-id="poc-2026""#));
    }

    #[test]
    fn banner_no_id_omits_data_attribute() {
        let html = render_to_string(&banner_page(CmsBannerTone::Info, "x", false, None));
        assert!(!html.contains("data-loom-banner-id"));
    }

    #[test]
    fn banner_text_is_escaped() {
        let html = render_to_string(&banner_page(
            CmsBannerTone::Danger,
            "<script>alert(1)</script>",
            false,
            None,
        ));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
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

/// Generate the JSON Schema for the `CmsPage` document type.
///
/// Emitted via schemars 0.8 (Draft 07; supported by every modern
/// editor LSP). Editors that read a `$schema` reference (VS Code,
/// Helix, Zed, Sublime, Neovim with jsonls) provide inline
/// autocomplete + validation when authors put `"$schema": "..."`
/// in their `cms/*.json`. The output is fully self-contained:
/// every nested type expanded inline via `definitions`.
///
/// # Panics
/// Only on a contract violation inside schemars (its
/// `RootSchema → JsonValue` conversion is total for any input it
/// produces). Unreachable in practice.
#[must_use]
pub fn cms_page_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(CmsPage);
    serde_json::to_value(&schema).expect("CmsPage schema serializes")
}

// ============================================================
// T70b: page-shell (moved from loom-cli so Forge can call it
// directly via the public API and inherit the same WCAG-AA
// dual-theme + a11y defaults Loom-rendered pages already enjoy).
// ============================================================

/// Inline style block always emitted by `page_shell`. Carries
/// light + dark colour tokens, focus-visible outlines, skip-link
/// styling, and `prefers-reduced-motion` honour. CSP-pinned by
/// sha256.
///
/// Both palettes verified WCAG 2.1 AA in loom-cli's
/// `base_theme_meets_wcag_aa_in_both_modes` test.
pub const BASE_THEME_CSS: &str = ":root{\
--loom-bg:#FBFAF7;--loom-fg:#1B1F2A;--loom-muted:#6B7280;\
--loom-accent:#4338CA;--loom-accent-2:#E07A5F;\
--loom-border:#E6E2DA;\
--loom-link:#4338CA;--loom-link-hover:#3730A3;--loom-focus:#4338CA;\
--loom-radius:10px;--loom-radius-sm:6px;--loom-radius-lg:18px;\
--loom-shadow-sm:0 1px 2px rgba(20,24,42,.06),0 1px 1px rgba(20,24,42,.04);\
--loom-shadow-md:0 4px 14px rgba(20,24,42,.08),0 2px 4px rgba(20,24,42,.04);\
--loom-shadow-lg:0 18px 40px rgba(20,24,42,.12),0 6px 12px rgba(20,24,42,.06);\
--loom-grad-hero:linear-gradient(135deg,#4338CA 0%,#7C3AED 50%,#E07A5F 100%);\
--loom-grad-soft:linear-gradient(135deg,rgba(67,56,202,.08) 0%,rgba(224,122,95,.08) 100%);\
--loom-font-stack:Inter,ui-sans-serif,system-ui,-apple-system,\"Segoe UI Variable Text\",\"Segoe UI\",Roboto,sans-serif;\
--loom-font-display:\"Outfit\",ui-rounded,Inter,ui-sans-serif,system-ui,sans-serif;\
--loom-font-mono:ui-monospace,\"JetBrains Mono\",SFMono-Regular,\"Cascadia Mono\",monospace;\
--loom-motion-fast:120ms;--loom-motion-base:220ms;--loom-motion-slow:420ms;\
--loom-ease-out:cubic-bezier(.22,1,.36,1);--loom-ease-spring:cubic-bezier(.34,1.56,.64,1);\
--loom-space-0:0;--loom-space-1:.25rem;--loom-space-2:.5rem;--loom-space-3:.75rem;\
--loom-space-4:1rem;--loom-space-5:1.25rem;--loom-space-6:1.5rem;--loom-space-7:1.75rem;\
--loom-space-8:2rem;--loom-space-10:2.5rem;--loom-space-12:3rem;--loom-space-16:4rem;\
--loom-space-20:5rem;--loom-space-24:6rem;\
--loom-font-xs:.75rem;--loom-font-sm:.875rem;--loom-font-base:1rem;--loom-font-lg:1.125rem;\
--loom-font-xl:1.25rem;--loom-font-2xl:1.5rem;--loom-font-3xl:1.875rem;--loom-font-4xl:2.25rem;\
--loom-font-5xl:3rem;--loom-font-6xl:3.75rem;\
--loom-pad-card:1rem;--loom-pad-panel:1.25rem;--loom-pad-band:1.5rem;\
--loom-gap-stack:1rem;--loom-gap-row:.75rem;--loom-gap-grid:1rem;\
--loom-tap-min:44px;--loom-track-tight:-.012em;\
--loom-stroke-thin:1px;--loom-stroke-strong:2px;\
--loom-radius-component:10px;--loom-radius-sm:6px;\
--loom-size-icon-sm:20px;--loom-size-icon-md:24px;\
--loom-size-avatar-sm:40px;--loom-size-avatar-md:48px;\
--loom-break-xl:80rem;\
--loom-border-component:1px solid var(--loom-color-border,var(--loom-border));\
--loom-transition-fast:120ms cubic-bezier(.22,1,.36,1)}\
@media (prefers-color-scheme:dark){:root:not([data-theme=\"light\"]){\
--loom-bg:#0F1019;--loom-fg:#ECEEF6;--loom-muted:#8B92A6;\
--loom-accent:#A5A6FF;--loom-accent-2:#FFA771;\
--loom-border:#25283A;\
--loom-link:#A5A6FF;--loom-link-hover:#DCDDFF;--loom-focus:#A5A6FF;\
--loom-shadow-sm:0 1px 2px rgba(0,0,0,.4),0 1px 1px rgba(0,0,0,.3);\
--loom-shadow-md:0 4px 14px rgba(0,0,0,.45),0 2px 4px rgba(0,0,0,.3);\
--loom-shadow-lg:0 18px 40px rgba(0,0,0,.6),0 6px 12px rgba(0,0,0,.4);\
--loom-grad-hero:linear-gradient(135deg,#5046E5 0%,#8B5CF6 50%,#FFA771 100%);\
--loom-grad-soft:linear-gradient(135deg,rgba(165,166,255,.08) 0%,rgba(255,167,113,.08) 100%)}}\
:root[data-theme=\"dark\"]{\
--loom-bg:#0F1019;--loom-fg:#ECEEF6;--loom-muted:#8B92A6;\
--loom-accent:#A5A6FF;--loom-accent-2:#FFA771;\
--loom-border:#25283A;\
--loom-link:#A5A6FF;--loom-link-hover:#DCDDFF;--loom-focus:#A5A6FF;\
--loom-shadow-sm:0 1px 2px rgba(0,0,0,.4),0 1px 1px rgba(0,0,0,.3);\
--loom-shadow-md:0 4px 14px rgba(0,0,0,.45),0 2px 4px rgba(0,0,0,.3);\
--loom-shadow-lg:0 18px 40px rgba(0,0,0,.6),0 6px 12px rgba(0,0,0,.4);\
--loom-grad-hero:linear-gradient(135deg,#5046E5 0%,#8B5CF6 50%,#FFA771 100%);\
--loom-grad-soft:linear-gradient(135deg,rgba(165,166,255,.08) 0%,rgba(255,167,113,.08) 100%)}\
:root[data-theme=\"light\"]{\
--loom-bg:#FBFAF7;--loom-fg:#1B1F2A;--loom-muted:#6B7280;\
--loom-accent:#4338CA;--loom-accent-2:#E07A5F;\
--loom-border:#E6E2DA;\
--loom-link:#4338CA;--loom-link-hover:#3730A3;--loom-focus:#4338CA;\
--loom-shadow-sm:0 1px 2px rgba(20,24,42,.06),0 1px 1px rgba(20,24,42,.04);\
--loom-shadow-md:0 4px 14px rgba(20,24,42,.08),0 2px 4px rgba(20,24,42,.04);\
--loom-shadow-lg:0 18px 40px rgba(20,24,42,.12),0 6px 12px rgba(20,24,42,.06);\
--loom-grad-hero:linear-gradient(135deg,#4338CA 0%,#7C3AED 50%,#E07A5F 100%);\
--loom-grad-soft:linear-gradient(135deg,rgba(67,56,202,.08) 0%,rgba(224,122,95,.08) 100%)}\
html{background:var(--loom-bg);color:var(--loom-fg);\
font-family:var(--loom-font-stack);line-height:1.55;\
-webkit-font-smoothing:antialiased;-moz-osx-font-smoothing:grayscale;\
font-feature-settings:\"cv11\",\"ss01\",\"ss03\"}\
body{margin:0}\
a{color:var(--loom-link);text-decoration-thickness:.08em;text-underline-offset:.18em;\
transition:color var(--loom-motion-fast) var(--loom-ease-out)}\
a:hover,a:focus{color:var(--loom-link-hover)}\
h1,h2,h3,h4,h5,h6{font-family:var(--loom-font-display);letter-spacing:-.012em;line-height:1.2}\
:focus-visible{outline:2px solid var(--loom-focus);outline-offset:3px;border-radius:var(--loom-radius-sm)}\
.loom-skip{position:absolute;left:-9999px;top:auto;width:1px;height:1px;overflow:hidden}\
.loom-skip:focus{left:1rem;top:1rem;width:auto;height:auto;padding:.5rem 1rem;\
background:var(--loom-bg);color:var(--loom-fg);border:2px solid var(--loom-focus);\
border-radius:var(--loom-radius);z-index:1000;box-shadow:var(--loom-shadow-md)}\
header.loom-page-header{padding:1.25rem 1.5rem;border-bottom:1px solid var(--loom-border);\
background:var(--loom-bg);position:sticky;top:0;z-index:50;\
backdrop-filter:saturate(140%) blur(8px);-webkit-backdrop-filter:saturate(140%) blur(8px)}\
footer.loom-page-footer{padding:2rem 1.5rem;border-top:1px solid var(--loom-border);\
color:var(--loom-muted);margin-top:3rem;font-size:.92rem}\
nav.loom-page-nav{display:flex;gap:1.25rem;align-items:center;flex-wrap:wrap}\
nav.loom-page-nav a{text-decoration:none;color:var(--loom-fg);\
display:inline-flex;align-items:center;min-height:44px;padding:.5rem .75rem;\
border-radius:var(--loom-radius-sm);\
transition:background var(--loom-motion-fast) var(--loom-ease-out),color var(--loom-motion-fast) var(--loom-ease-out)}\
nav.loom-page-nav a:hover{color:var(--loom-link-hover);background:var(--loom-grad-soft)}\
nav.loom-page-nav a[aria-current=\"page\"]{color:var(--loom-accent);font-weight:600}\
.loom-page-title{margin:.5rem 0 0;font-family:var(--loom-font-display);\
font-weight:700;letter-spacing:-.018em;font-size:1.6rem}\
main#content{padding:1.5rem;max-width:64rem;margin:0 auto}\
@media (prefers-reduced-motion:reduce){\
*,*::before,*::after{animation-duration:.001ms !important;animation-iteration-count:1 !important;\
transition-duration:.001ms !important;scroll-behavior:auto !important}\
header.loom-page-header{position:static;backdrop-filter:none;-webkit-backdrop-filter:none}}";

/// Fixed onload event handler for the deferred stylesheet link
/// when `critical_css` is supplied. Hashed at build time + pinned
/// in CSP `script-src 'unsafe-hashes' 'sha256-…'`.
pub const DEFER_ONLOAD_JS: &str = "this.media='all';this.removeAttribute('onload')";

/// T76 (Crawler dogfood 2026-05-14): every page emits a default
/// inline-SVG favicon so browser tabs / bookmarks / history don't
/// render the generic globe glyph. Inline data URL means there's
/// no separate /favicon.ico to deploy or 404 on — every Loom site
/// gets a working icon out of the box.
///
/// Sites that want their own favicon should override this by
/// emitting an additional `<link rel="icon">` AFTER the page_shell
/// — browsers honor the last matching rel="icon" per type. Future
/// page_shell variant could accept a `favicon_override: Option<&str>`.
///
/// The SVG renders a soft-square Loom mark in the brand accent
/// colour. Width=height=16, viewBox 0 0 16 16, single path with a
/// border radius so it reads cleanly at favicon size.
pub const DEFAULT_FAVICON_LINK: &str = "<link rel=\"icon\" href=\"data:image/svg+xml,%3Csvg%20xmlns%3D%27http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%27%20viewBox%3D%270%200%2016%2016%27%3E%3Crect%20width%3D%2716%27%20height%3D%2716%27%20rx%3D%273%27%20fill%3D%27%23111827%27%2F%3E%3Cpath%20d%3D%27M4%204h2v8H4zM10%204h2v8h-2zM6.5%208h3v1.5h-3z%27%20fill%3D%27%23f8fafc%27%2F%3E%3C%2Fsvg%3E\" type=\"image/svg+xml\">";

/// Compute the CSP `'sha256-<base64>'` source-list value for a
/// given inline block (script or style). Same construction
/// browsers use for hash-pinning.
#[must_use]
pub fn csp_sha256(bytes: &[u8]) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let b64 = base64::engine::general_purpose::STANDARD.encode(digest);
    format!("sha256-{b64}")
}

/// Escape a text node (HTML body text or `<title>` content).
#[must_use]
pub fn escape_html_text(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// Escape a value going inside a double-quoted attribute.
#[must_use]
pub fn escape_html_attr(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// Render the `<nav>`'s `<a>` children for a page's primary
/// nav-links. `aria-current="page"` for the link marked `current`.
/// Unsafe hrefs render as `#invalid-nav-link` placeholders carrying
/// `data-invalid="true"` so the build's audit phase can flag the
/// page WITHOUT leaking the bad URL into a real anchor.
#[must_use]
pub fn render_nav_links(links: &[CmsNavLink]) -> String {
    let mut out = String::new();
    for link in links {
        let label = escape_html_text(&link.label);
        let href_safe = loom_components::composer::is_safe_url(&link.href);
        let href = if href_safe {
            escape_html_attr(&link.href)
        } else {
            "#invalid-nav-link".to_owned()
        };
        let invalid_attr = if href_safe { "" } else { " data-invalid=\"true\"" };
        let backend = escape_html_attr(&link.data_backend);
        let current = if link.current { " aria-current=\"page\"" } else { "" };
        out.push_str(&format!(
            "<a class=\"loom-page-nav__link\" href=\"{href}\"{invalid_attr} data-backend=\"{backend}\"{current}>{label}</a>"
        ));
    }
    out
}

/// Wrap rendered body markup in the smallest valid HTML5 page
/// shell that satisfies WCAG 2.1 AA (ISO/IEC 40500), declares
/// dual-theme support (`<meta name="color-scheme">`), and
/// pins every inline style block in CSP via sha256 (never
/// `unsafe-inline`).
///
/// Always emits the `BASE_THEME_CSS` block regardless of
/// `critical_css`. If `critical_css` is supplied, it ALSO
/// emits a deferred-load `<link>` for the `css_href` stylesheet
/// (with a hashed `onload=` handler) so the user's larger
/// stylesheet doesn't block first paint.
#[must_use]
pub fn page_shell(
    page: &CmsPage,
    css_href: &str,
    body: &str,
    critical_css: Option<&str>,
) -> String {
    page_shell_themed(page, css_href, body, critical_css, None)
}

/// T37 v1: explicit-theme variant of `page_shell`. When `theme`
/// is `Some(name)`, emits `<html lang="en" data-theme="<name>">`
/// so explicit picks ("dark" / "light" / future high-contrast
/// variants) override the OS-driven `prefers-color-scheme`
/// auto-applied palette. When `None`, identical to `page_shell`.
///
/// Valid theme values: `"light"` | `"dark"` (today). Future
/// variants (`"hc-dark"`, `"hc-light"`, `"sepia"`) get added to
/// `BASE_THEME_CSS` and the closed enum in `loom-cli` together.
///
/// SECURITY: the `theme` value is HTML-attribute-escaped before
/// interpolation. An attacker-controlled theme string (via cookie
/// or query param) cannot escape the attribute context.
#[must_use]
pub fn page_shell_themed(
    page: &CmsPage,
    css_href: &str,
    body: &str,
    critical_css: Option<&str>,
    theme: Option<&str>,
) -> String {
    let title = escape_html_text(&page.title);
    let description = escape_html_text(&page.description);
    let path = escape_html_attr(&page.path);
    let css = escape_html_attr(css_href);
    let nav_links = render_nav_links(&page.nav_links);
    let base_theme_hash = csp_sha256(BASE_THEME_CSS.as_bytes());
    let base_theme_block = format!("<style>{BASE_THEME_CSS}</style>\n  ");
    #[allow(clippy::option_if_let_else)]
    let (extra_style_block, css_link, csp) = if let Some(crit) = critical_css {
        let style_hash = csp_sha256(crit.as_bytes());
        let onload_hash = csp_sha256(DEFER_ONLOAD_JS.as_bytes());
        let extra_block = format!("<style>{crit}</style>\n  ");
        let css_link = format!(
            "<link rel=\"stylesheet\" href=\"{css}\" media=\"print\" onload=\"{DEFER_ONLOAD_JS}\">\n  <noscript><link rel=\"stylesheet\" href=\"{css}\"></noscript>"
        );
        let csp = format!(
            "default-src 'self'; img-src 'self' data:; style-src 'self' '{base_theme_hash}' '{style_hash}'; script-src 'self' 'unsafe-hashes' '{onload_hash}'; frame-ancestors 'none'"
        );
        (extra_block, css_link, csp)
    } else {
        let css_link = format!("<link rel=\"stylesheet\" href=\"{css}\">");
        let csp = format!(
            "default-src 'self'; img-src 'self' data:; style-src 'self' '{base_theme_hash}'; script-src 'self'; frame-ancestors 'none'"
        );
        (String::new(), css_link, csp)
    };
    let style_block = format!("{base_theme_block}{extra_style_block}");
    // T37 v1: gate the `data-theme` attribute on a closed allow-list
    // ("light" | "dark"). An attacker-controlled value is dropped
    // rather than escaped-and-emitted — defence in depth on top of
    // the attribute-escape.
    let html_open = match theme {
        Some(t) if t == "light" || t == "dark" => {
            format!("<html lang=\"en\" data-theme=\"{t}\">")
        }
        _ => "<html lang=\"en\">".to_owned(),
    };
    format!(
        "<!doctype html>\n\
{html_open}\n\
<head>\n\
  <meta charset=\"utf-8\">\n\
  <meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\n\
  <meta http-equiv=\"X-Content-Type-Options\" content=\"nosniff\">\n\
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
  <meta name=\"color-scheme\" content=\"light dark\">\n\
  <title>{title}</title>\n\
  <meta name=\"description\" content=\"{description}\">\n\
  <link rel=\"canonical\" href=\"{path}\">\n\
  {DEFAULT_FAVICON_LINK}\n\
  {style_block}{css_link}\n\
</head>\n\
<body>\n\
  <a class=\"loom-skip\" href=\"#content\">Skip to content</a>\n\
  <header class=\"loom-page-header\">\n\
    <nav class=\"loom-page-nav\" aria-label=\"Primary\">\n\
      <a class=\"loom-page-brand\" href=\"/\" data-loom-rich-link=\"true\">SkillShots</a>{nav_links}\n\
    </nav>\n\
    <h1 class=\"loom-page-title\">{title}</h1>\n\
  </header>\n\
  <main id=\"content\">\n\
{body}\n\
  </main>\n\
  <footer class=\"loom-page-footer\">\n\
  </footer>\n\
</body>\n\
</html>\n"
    )
}

#[cfg(test)]
mod page_shell_tests {
    use super::*;

    fn empty_page() -> CmsPage {
        CmsPage {
            schema: None,
            title: "T".into(),
            description: "D".into(),
            path: "/".into(),
            nav_links: vec![],
            sections: vec![],
        }
    }

    #[test]
    fn always_emits_base_theme_block_csp_pinned() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        let hash = csp_sha256(BASE_THEME_CSS.as_bytes());
        assert!(s.contains(&hash), "base-theme hash must appear in CSP");
        assert!(!s.contains("'unsafe-inline'"));
        assert!(s.contains("<style>"));
        assert!(s.contains("--loom-bg"));
    }

    #[test]
    fn emits_dual_theme_media_query() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains("prefers-color-scheme:dark"));
    }

    #[test]
    fn emits_color_scheme_meta_and_main_landmark() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains(r#"<meta name="color-scheme" content="light dark">"#));
        assert!(s.contains(r#"<main id="content">"#));
    }

    #[test]
    fn honours_prefers_reduced_motion() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains("prefers-reduced-motion:reduce"));
    }

    #[test]
    fn emits_skip_link_visible_on_focus() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains("<a class=\"loom-skip\" href=\"#content\">"));
        assert!(s.contains(".loom-skip:focus"));
    }

    #[test]
    fn pins_critical_css_with_separate_hash_when_supplied() {
        let crit = "h1{color:red}";
        let s = page_shell(&empty_page(), "/loom-skin.css", "", Some(crit));
        assert!(s.contains(&csp_sha256(BASE_THEME_CSS.as_bytes())));
        assert!(s.contains(&csp_sha256(crit.as_bytes())));
        assert!(s.contains(&csp_sha256(DEFER_ONLOAD_JS.as_bytes())));
        assert!(s.contains("'unsafe-hashes'"));
        assert!(!s.contains("'unsafe-inline'"));
    }

    #[test]
    fn body_landed_inside_main() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "<p>hello</p>", None);
        let main_open = s.find("<main").expect("main");
        let main_close = s.find("</main>").expect("/main");
        let inside = &s[main_open..main_close];
        assert!(inside.contains("<p>hello</p>"), "body must land inside <main>");
    }

    #[test]
    fn escapes_title_to_prevent_xss() {
        let mut p = empty_page();
        p.title = "<script>alert(1)</script>".into();
        let s = page_shell(&p, "/loom-skin.css", "", None);
        assert!(!s.contains("<script>alert(1)</script>"));
        assert!(s.contains("&lt;script&gt;"));
    }

    // ---- T37 v1: explicit-theme `page_shell_themed` ----

    #[test]
    fn page_shell_themed_emits_data_theme_when_dark() {
        let s = page_shell_themed(&empty_page(), "/loom-skin.css", "", None, Some("dark"));
        assert!(s.contains("data-theme=\"dark\""), "missing data-theme=dark: {s}");
    }

    #[test]
    fn page_shell_themed_emits_data_theme_when_light() {
        let s = page_shell_themed(&empty_page(), "/loom-skin.css", "", None, Some("light"));
        assert!(s.contains("data-theme=\"light\""), "missing data-theme=light");
    }

    #[test]
    fn page_shell_themed_with_none_matches_unthemed_shell() {
        let a = page_shell(&empty_page(), "/loom-skin.css", "", None);
        let b = page_shell_themed(&empty_page(), "/loom-skin.css", "", None, None);
        assert_eq!(a, b, "page_shell(...) must equal page_shell_themed(..., None)");
    }

    // T76 (Crawler dogfood 2026-05-14): every page emits a default
    // favicon link via DEFAULT_FAVICON_LINK. Closes
    // favicon.missing-link at the source — SkillShots audit
    // pre-fix produced 7 warns (one per page); post-fix produces 0.
    #[test]
    fn page_shell_emits_default_favicon_link() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(
            s.contains("rel=\"icon\""),
            "page_shell must emit a <link rel=\"icon\"> (default favicon)"
        );
        // It should be an inline data:image/svg+xml URL — no separate
        // /favicon.ico file required to deploy.
        assert!(
            s.contains("data:image/svg+xml"),
            "default favicon should be an inline SVG data URL (not /favicon.ico)"
        );
    }

    #[test]
    fn page_shell_themed_drops_unknown_theme_value() {
        // Defence in depth: a hostile theme value (XSS attempt /
        // typo / future variant not yet in the closed allow-list)
        // gets DROPPED, not emitted as <html data-theme="...">.
        // BASE_THEME_CSS itself contains [data-theme="..."] selectors,
        // so we narrow the assertion to the <html ...> opening tag.
        for hostile in ["evil", "'><script>", "../etc/passwd", "DARK", "Light"] {
            let s = page_shell_themed(
                &empty_page(),
                "/loom-skin.css",
                "",
                None,
                Some(hostile),
            );
            assert!(
                s.contains("<html lang=\"en\">\n"),
                "unknown theme value `{hostile}` must produce bare <html lang=\"en\">"
            );
            assert!(
                !s.contains("<html lang=\"en\" data-theme="),
                "unknown theme value `{hostile}` must NOT emit data-theme on the html tag"
            );
        }
    }

    #[test]
    fn base_theme_css_includes_explicit_data_theme_rules() {
        // T37 v1: explicit picks must win over OS @media. Verifies
        // both `:root[data-theme="dark"]` and `:root[data-theme="light"]`
        // selectors appear in the always-emitted base block. The
        // const stores the raw CSS text (escapes resolved at compile
        // time), so we look for literal real-double-quote strings.
        assert!(
            BASE_THEME_CSS.contains(r#"[data-theme="dark"]"#),
            "missing :root[data-theme=\"dark\"] block in BASE_THEME_CSS"
        );
        assert!(
            BASE_THEME_CSS.contains(r#"[data-theme="light"]"#),
            "missing :root[data-theme=\"light\"] block in BASE_THEME_CSS"
        );
    }

    #[test]
    fn base_theme_css_dark_media_does_not_apply_when_light_is_explicit() {
        // The @media (prefers-color-scheme: dark) block uses
        // :root:not([data-theme="light"]) so an explicit
        // data-theme="light" overrides the OS preference.
        assert!(
            BASE_THEME_CSS.contains(r#":not([data-theme="light"])"#),
            "@media block must use :root:not([data-theme=\"light\"])"
        );
    }

    /// T70b-fix REGRESSION-GUARD: page_shell + render_page composed
    /// must produce EXACTLY ONE `<main>` element. Two `<main>`s
    /// per document is a WCAG violation.
    #[test]
    fn page_shell_with_rendered_body_produces_exactly_one_main() {
        let p = CmsPage {
            schema: None,
            title: "Test".into(),
            description: "T".into(),
            path: "/".into(),
            nav_links: vec![],
            sections: vec![CmsSection::Heading { level: HeadingLevel::H2, text: "x".into() }],
        };
        let body = render_page(&p).into_string();
        let composed = page_shell(&p, "/loom-skin.css", &body, None);
        let main_open_count = composed.matches("<main").count();
        let main_close_count = composed.matches("</main>").count();
        assert_eq!(main_open_count, 1, "exactly one <main> open: composed = {composed}");
        assert_eq!(main_close_count, 1, "exactly one </main> close");
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn cms_page_schema_serializes() {
        let v = cms_page_schema();
        assert!(v.is_object());
    }

    #[test]
    fn schema_documents_section_variants() {
        let v = cms_page_schema();
        let s = serde_json::to_string(&v).expect("ser");
        for tag in [
            "hero",
            "group",
            "card_feed",
            "sidebar",
            "form",
            "composer",
            "picture",
            "paragraph",
            "heading",
            "banner",
        ] {
            assert!(s.contains(&format!("\"{tag}\"")), "missing tag: {tag}");
        }
    }

    #[test]
    fn schema_documents_named_types() {
        let v = cms_page_schema();
        let s = serde_json::to_string(&v).expect("ser");
        for type_name in [
            "CmsSection",
            "CmsCard",
            "CmsCardStat",
            "CmsPanel",
            "CmsPanelBody",
            "CmsPanelListItem",
            "CmsFormField",
            "CmsFormStep",
            "CmsFormSubmit",
            "CmsBannerTone",
            "CmsAvatar",
            "HeroCta",
            "CmsNavLink",
        ] {
            assert!(s.contains(type_name), "missing type: {type_name}");
        }
    }
}
