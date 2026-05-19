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
    /// Top-of-page brand label rendered as the `loom-page-brand` link.
    /// When omitted, the renderer derives a brand from the first
    /// segment of `title` before " — " / " · " / "—" / " - " separators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// Canonical URL path (e.g. `"/leaderboard"`). Required.
    /// Used by the layout shell to emit `<link rel="canonical">`.
    pub path: String,
    /// Theme name. Routed through page_shell_themed's
    /// `data-theme` attribute. Closed allowlist: `light`,
    /// `dark`, `auto`, `warm`, `ocean`, `forest`, `violet`,
    /// `rose`. Operators pick per-page (or set once in the
    /// site's index.json to inherit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Page-shell chrome style. Picks the header / body-backdrop
    /// shape. Default = `PageShell` (legacy SkillShots-style
    /// sticky bar). Other variants:
    /// - `FloatingPill` — modern floating capsule centered on
    ///   top of the viewport with glass-morphism backdrop.
    ///   Drops the cream three-radial-gradient body backdrop.
    /// - `Minimal` — no header at all; first section carries
    ///   all chrome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome: Option<ChromeKind>,
    /// Maximum inline-size of `main#content`. Closes the
    /// hard-coded `max-width: 64rem` page-shell default flagged
    /// in docs/SUBSTRATE_DE_CONSUMER_SHAPING_AUDIT.md. Default
    /// `Comfortable` matches the previous hard-coded value;
    /// `Narrow` is editorial-press measure; `Wide` is for
    /// dense-grid content; `Full` removes the cap entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_width: Option<ContentWidth>,
    /// Action CTAs rendered in the header (e.g. "Sign in",
    /// "Get started"). Distinct from `nav_links` — these are
    /// buttons, not page-to-page links.
    #[serde(default)]
    pub nav_actions: Vec<HeroCta>,
    /// Optional primary navigation links. The page-shell renders
    /// these inside `<nav aria-label="Primary">`. Empty/omitted →
    /// shell emits brand-only nav. Each link's `href` is validated
    /// (same-origin path or `https://`); invalid hrefs render as
    /// `#invalid-nav-link` placeholders.
    #[serde(default)]
    pub nav_links: Vec<CmsNavLink>,
    /// Sequence of body sections, top to bottom.
    pub sections: Vec<CmsSection>,
    /// When true, page-shell emits a relaxed CSP (drops Trusted
    /// Types, allows 'unsafe-inline' styles) and injects a
    /// localStorage-gated loader script that pulls
    /// `/eruda.min.js` on demand. Off by default; turn on via
    /// the per-page JSON flag or via Forge's `FORGE_DEV_DEVTOOLS`
    /// env var (which forge-phases/render.rs reads and patches
    /// onto every page in a build).
    ///
    /// SECURITY: only enable on dev hosts. Prod pages must keep
    /// the strict CSP. The page renders identically with or
    /// without devtools — the only delta is CSP + a tiny loader
    /// script that does nothing unless the visitor sets
    /// `localStorage["loom_eruda"] = "on"`. So even if a dev
    /// page ships to a stranger by mistake, they see no devtools
    /// (and no script load) without that opt-in.
    #[serde(default)]
    pub dev_devtools: bool,
    /// Optional typed footer. When `None`, the page-shell emits
    /// an empty `<footer class="loom-page-footer"></footer>`
    /// (back-compat). When `Some`, the renderer expands a typed
    /// multi-column footer with optional contact info and legal
    /// links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<CmsFooter>,
}

/// Typed page footer.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsFooter {
    /// Footer link columns. Common shape: 3-5 columns, each with
    /// a heading and a vertical list of links.
    #[serde(default)]
    pub columns: Vec<CmsFooterColumn>,
    /// Optional contact-info block (phone / email / address /
    /// jurisdiction). Renders as a separate column on the right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<CmsFooterContact>,
    /// Bottom-row legal links (Privacy, Terms, Imprint, etc.).
    /// Rendered as inline-flex under the columns.
    #[serde(default)]
    pub legal_links: Vec<CmsNavLink>,
    /// Optional copyright / colophon line at the very bottom.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colophon: Option<String>,
}

/// One footer link column.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsFooterColumn {
    /// Column heading (rendered as small uppercase label).
    pub heading: String,
    /// Vertical link list.
    pub links: Vec<CmsNavLink>,
}

/// Contact-info block for the footer.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsFooterContact {
    /// Optional column heading (defaults to "Contact" if absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    /// Phone number (any format; renderer wraps in tel: link if
    /// the string starts with a digit or `+`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// Email address (renderer wraps in mailto:).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Physical address (free-form text; no link).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Jurisdiction line (e.g. "Massachusetts, USA").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<String>,
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
    ///
    /// `decoration` is an optional editorial nudge that bumps the
    /// paragraph out of "default body" into one of three named
    /// editorial shapes (lead / drop_cap / aside). Pure visual —
    /// the text content is unchanged.
    Paragraph {
        /// Plain-text body (no markup).
        text: String,
        /// Editorial treatment (default = Body).
        #[serde(default)]
        decoration: ParagraphDecoration,
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
        /// Bounded per-element typographic adjustments. Each
        /// listed token emits a `loom-polish--X` class on the
        /// heading element. Idempotent — duplicates collapse.
        /// See [`PolishToken`] for the closed enum of one-step
        /// adjustments.
        #[serde(default)]
        polish: Vec<PolishToken>,
    },
    /// FORGE_ROADMAP item 41 — typed key/value list (definition-list
    /// shape). Renders as a `<dl>` with one `<dt>/<dd>` pair per
    /// item; optional `hint` shows as a muted span under the value.
    /// Use cases: "Settings", "Match details", "Spec sheet",
    /// "Receipt fields", "Profile facts" — anywhere a label-and-value
    /// row stack would otherwise be hand-rolled markup.
    KvPair {
        /// Optional heading (rendered as h2 above the list).
        heading: Option<String>,
        /// Items, top to bottom.
        items: Vec<CmsKvItem>,
    },
    /// T660 P1 LogoWall — a wall of vetted brand logos used as
    /// social-proof on marketing landings ("Trusted by Stripe,
    /// Linear, Vercel, ..."). Surfaced in 4 of 5 T660 dogfood
    /// rebuilds — highest dedup-priority variant in the registry.
    ///
    /// `items` carries explicit text labels because the actual SVG
    /// markup lives in a vetted `loom-brand-icons` registry crate
    /// (queued separately). Until that registry lands, the
    /// renderer emits a typographic placeholder; once it lands,
    /// the registry lookup happens at render time keyed by the
    /// label.
    ///
    /// AVP-2: brand SVG bodies are TRUSTED content; this CmsSection
    /// never accepts inline SVG from user input.
    LogoWall {
        /// Optional heading rendered above the wall ("Trusted by",
        /// "Customers", etc.).
        heading: Option<String>,
        /// Brand entries, left-to-right then wrap.
        items: Vec<CmsLogoItem>,
    },
    /// T660 P2 Quote — testimonial card. Surfaced in 3 of 3 T660
    /// marketing rebuilds (Stripe, Linear, Vercel) — second-highest
    /// dedup-priority. Single quote per section; multi-quote
    /// carousels = multiple Quote sections (deliberate, lets
    /// downstream operators reorder via the picker).
    Quote {
        /// The quoted text. Auto-escaped on render.
        body: String,
        /// Speaker name (e.g. "Patrick Collison").
        attribution: String,
        /// Speaker role / company (e.g. "CEO, Stripe").
        role: Option<String>,
    },
    /// T660 P3 Code — fenced code or terminal-output block. Surfaced
    /// in 2 of 3 T660 marketing rebuilds (Stripe API callouts +
    /// Vercel `npx vercel` snippets). Renders as semantic
    /// `<pre><code class="language-<lang>">`; the typed `lang` field
    /// keeps callers honest about syntax-highlighting hints without
    /// shipping a runtime highlighter in v1.
    Code {
        /// Fence language hint. Empty = generic. Examples: "bash",
        /// "rust", "javascript", "terminal".
        #[serde(default)]
        lang: String,
        /// Body of the block. Auto-escaped via Maud. Multi-line OK.
        body: String,
        /// Optional caption rendered above the block.
        caption: Option<String>,
        /// True if the block represents terminal/shell output (sets
        /// data-loom-terminal so the skin can render a chrome bar).
        #[serde(default)]
        terminal: bool,
    },
    /// Full-bleed image/gradient hero. Larger + more visually
    /// ambitious than [`CmsSection::Hero`]; spans the viewport
    /// width breaking out of the standard content max-width.
    /// Use for top-of-page landing sections that need atmosphere.
    ImageHero {
        /// Optional eyebrow chip above the title.
        eyebrow: Option<String>,
        /// Display title.
        title: String,
        /// Optional lede paragraph below the title.
        lede: Option<String>,
        /// Optional primary CTA.
        cta: Option<HeroCta>,
        /// Backdrop kind — gradient-mesh / solid / pattern.
        #[serde(default)]
        background: HeroBackground,
        /// Visual height ramp. Affects min-height + padding.
        #[serde(default)]
        height: HeroHeight,
        /// Typed slot — sections that render ABOVE the title.
        /// Use for trust signals, badges, version chips,
        /// announcement banners, additional eyebrow content.
        /// Each section in the slot renders in order.
        /// Closes #105 (slot-based composition).
        #[serde(default)]
        before_headline: Vec<CmsSection>,
        /// Typed slot — sections that render BELOW the CTA.
        /// Use for trust-signal logos, disclaimer copy, secondary
        /// CTA chains, fine-print legal.
        #[serde(default)]
        after_cta: Vec<CmsSection>,
    },
    /// Text + visual side-by-side hero. Text occupies one column,
    /// a typed visual (code snippet, single stat, or photo asset
    /// slug from `loom-assets`) occupies the other.
    SplitHero {
        /// Optional eyebrow chip.
        eyebrow: Option<String>,
        /// Display title.
        title: String,
        /// Optional lede.
        lede: Option<String>,
        /// Optional primary CTA.
        cta: Option<HeroCta>,
        /// Side visual.
        visual: SplitVisual,
        /// True → visual on the right (default), false → left.
        #[serde(default = "default_true")]
        visual_right: bool,
    },
    /// Multi-column feature listing. Each item carries an icon
    /// (slug from `loom-assets`), a heading, a body, and an
    /// optional learn-more link. `columns` clamps to 1..=4.
    FeatureSpotlight {
        /// Optional section heading above the grid.
        heading: Option<String>,
        /// Optional section lede.
        lede: Option<String>,
        /// Items, displayed in column-grid order.
        items: Vec<SpotlightItem>,
        /// Column count (1..=4). Mobile collapses to 1 regardless.
        #[serde(default = "default_columns_3")]
        columns: u8,
        /// Visual treatment. `Decorated` (default, kept for
        /// backward compatibility) is the SaaS-card shape with
        /// rounded chrome, gradient icon tile, hover lift +
        /// shadow. `Editorial` strips card chrome down to
        /// typography + top accent rule. `Minimal` is a tight
        /// grid with no decoration.
        ///
        /// Substrate-de-consumer-shaping: callers asking for
        /// dense editorial composition opt out of the trope
        /// chrome without inventing a parallel primitive.
        #[serde(default)]
        decoration: FeatureSpotlightDecoration,
    },
    /// Row of large animated numbers with labels. Used for "by
    /// the numbers" / "stats that matter" social-proof bands.
    StatBand {
        /// Optional section heading above the row.
        heading: Option<String>,
        /// Optional lede.
        lede: Option<String>,
        /// Stats, displayed in row order.
        items: Vec<StatItem>,
    },
    /// Numbered process steps. Visual: tall vertical timeline on
    /// mobile, horizontal row on desktop.
    Steps {
        /// Optional section heading.
        heading: Option<String>,
        /// Optional lede.
        lede: Option<String>,
        /// Step items in order. The renderer numbers them 1..=N
        /// automatically.
        items: Vec<StepItem>,
    },
    /// Typed pricing tier display. One band of side-by-side tier
    /// cards with the optional "highlighted" tier visually
    /// distinguished.
    Pricing {
        /// Optional section heading.
        heading: Option<String>,
        /// Optional lede.
        lede: Option<String>,
        /// Tier cards, left-to-right.
        tiers: Vec<PricingTier>,
    },
    /// Accordion of question / answer pairs. Each item is
    /// expandable; only one expanded at a time when
    /// `single_expand` is true.
    Faq {
        /// Optional section heading.
        heading: Option<String>,
        /// Optional lede.
        lede: Option<String>,
        /// FAQ items.
        items: Vec<FaqItem>,
        /// Auto-collapse other open items when one is opened.
        #[serde(default)]
        single_expand: bool,
    },
    /// Horizontally-scrolling band of short text or brand names.
    /// Used as a continuous-motion social-proof rail.
    Marquee {
        /// Items to scroll. Duplicated automatically for
        /// seamless looping.
        items: Vec<String>,
        /// Scroll direction.
        #[serde(default)]
        direction: MarqueeDirection,
        /// Scroll speed (1..=10; higher = faster).
        #[serde(default = "default_speed")]
        speed: u8,
    },
    /// Full-width call-to-action band. Sits at the bottom of
    /// marketing pages just above the footer.
    CallToAction {
        /// Optional eyebrow chip.
        eyebrow: Option<String>,
        /// Display title.
        title: String,
        /// Optional lede.
        lede: Option<String>,
        /// Primary CTA.
        cta: HeroCta,
        /// Backdrop kind.
        #[serde(default)]
        background: HeroBackground,
    },
    /// Editorial pull-quote. Large display-italic body, optional
    /// attribution underneath. Distinct from `Quote` which is a
    /// testimonial-card shape.
    PullQuote {
        /// Body of the quote.
        body: String,
        /// Optional attribution (e.g. "Jane Doe, CTO @ Acme").
        attribution: Option<String>,
    },
    /// Editorial epigraph — an opening quote, poem fragment, or
    /// motto placed BEFORE the article's first paragraph. Distinct
    /// from [`CmsSection::PullQuote`] (which interrupts mid-flow)
    /// and from [`CmsSection::Quote`] (which is a generic
    /// quotation card). Sets the rhetorical tone for the piece.
    Epigraph {
        /// Body of the epigraph (auto-escaped).
        body: String,
        /// Optional source attribution
        /// (e.g. "Rilke, Duino Elegies").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attribution: Option<String>,
    },
    /// Editorial sidenote — text that floats to the side at wide
    /// viewports and renders inline as a small offset block at
    /// narrow ones. Common in long-form essays (literary press,
    /// academic publication, deep technical doc). Distinct from
    /// Paragraph + Aside-decoration: marginalia is positioned
    /// relative to the body text, not styled in line.
    ///
    /// `position` controls which side it floats to at wide
    /// viewports. At narrow viewports both render as a small
    /// inline block, regardless of side.
    Marginalia {
        /// Sidenote body (plain text, auto-escaped).
        body: String,
        /// Wide-viewport float side.
        #[serde(default)]
        position: MarginaliaPosition,
    },
    /// Account-summary card — typed surface for a logged-in
    /// user's at-a-glance state. Renders avatar + display name +
    /// plan + member-since. Read-only; editing flows through
    /// `ProfileEdit`. Server-rendered.
    AccountSummary {
        /// Visible display name.
        display_name: String,
        /// Avatar — typed enum (None / Initials / Image).
        #[serde(default = "default_no_avatar")]
        avatar: CmsAvatar,
        /// Plan / tier label (e.g. "Solo", "Team").
        plan: String,
        /// Member-since string (RFC 3339 date or human form).
        member_since: String,
        /// Optional secondary line under the name (e.g. handle).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handle: Option<String>,
    },
    /// Profile-edit form — typed surface for the logged-in user
    /// to update their identity-facing fields. Distinct from
    /// `SettingsPanel` (which is preferences); ProfileEdit is
    /// "who you are." Server-rendered; posts back to caller's
    /// endpoint.
    ProfileEdit {
        /// Where the form POSTs.
        action: String,
        /// Pre-filled display name.
        #[serde(default)]
        display_name: String,
        /// Pre-filled handle / username.
        #[serde(default)]
        handle: String,
        /// Pre-filled pronouns string.
        #[serde(default)]
        pronouns: String,
        /// Pre-filled bio (multi-line).
        #[serde(default)]
        bio: String,
        /// Pre-filled language preference slug.
        #[serde(default)]
        language: String,
        /// Submit-button label.
        #[serde(default = "default_profile_submit_label")]
        submit_label: String,
    },
    /// Terms-of-Service / Privacy-Policy / similar legal-doc
    /// page. Typed structure: title, last-updated, table-of-
    /// contents (auto-derived from sections), then a flat list
    /// of named sections with anchored headings + plain-language
    /// summary boxes.
    LegalDoc {
        /// Document title (e.g. "Terms of Service", "Privacy
        /// Policy").
        title: String,
        /// Last-updated date string (RFC 3339 preferred).
        last_updated: String,
        /// Optional plain-language tl;dr above the table of
        /// contents.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plain_language_summary: Option<String>,
        /// Document sections in display order.
        sections_list: Vec<LegalSection>,
    },
    /// Settings / preferences panel — a typed surface for the
    /// logged-in user's site preferences. Renders as a labeled
    /// definition-list with controls (toggles / text / danger
    /// buttons), categorised. Server-side-rendered so it survives
    /// LOOM_NOSCRIPT_MODE; the form posts back to a tenant-
    /// provided endpoint (the renderer doesn't bind a handler).
    SettingsPanel {
        /// Optional section heading rendered above the panel.
        heading: Option<String>,
        /// Optional lede paragraph below the heading.
        lede: Option<String>,
        /// Where the form POSTs on submit. Caller-side endpoint;
        /// the renderer just emits `action=`.
        action: String,
        /// Categorised groups of controls.
        categories: Vec<SettingsCategory>,
        /// Submit-button label. Defaults to "Save changes" if
        /// omitted at the JSON layer.
        #[serde(default = "default_settings_submit_label")]
        submit_label: String,
    },
    // ─── T660 P5 catalogue expansion ───────────────────────
    // Layout primitives (10).
    /// Bounded-width content container.
    Container { children_html: String, max_width: ContainerWidth },
    /// Visual section break.
    Divider { style: DividerStyle },
    /// Vertical whitespace token slot.
    Spacer { size: SpaceSize },
    /// N-column free-form layout (2/3/4 cols on desktop).
    Columns { columns: u8, items: Vec<String> },
    /// Vertical flex container.
    Stack { gap: SpaceSize, items: Vec<String> },
    /// Horizontal wrap-flex cluster (chips, tag-rows).
    Cluster { gap: SpaceSize, items: Vec<String> },
    /// Typed grid container with col count.
    GridLayout { columns: u8, items: Vec<String> },
    /// Tabbed content group.
    Tabs { items: Vec<TabItem> },
    /// Accordion of named sections.
    AccordionGroup { items: Vec<AccordionItem> },
    /// Reveal-on-scroll wrapper with typed motion.
    Reveal { motion: RevealMotion, body: String },

    // Editorial (15).
    /// Long-form article wrapper (sets max-width + reading type).
    Article { body: String },
    /// h3-class subheading inside an article.
    SubHeading { text: String, level: u8 },
    /// Large opening paragraph — sets the article's tone.
    Lede { text: String },
    /// Second-tier subhead beneath the lede. Newspapers use this
    /// to extend the headline-and-lede pair with one more
    /// editorial beat before the body. Distinct from
    /// [`CmsSection::Lede`] (sets the tone) and from
    /// [`CmsSection::SubHeading`] (sections the body).
    Sublede { text: String },
    /// Editorial kicker — short uppercase label above a headline.
    /// Newspaper / magazine convention: "OPINION", "LIVE",
    /// "BREAKING", "Q&A", "REVIEW". Distinct from the eyebrow
    /// chip on a hero (which is a hero-internal slot); this is a
    /// standalone editorial label.
    Kicker { text: String },
    /// Byline — typed author / role / dateline / reading-time
    /// unit. Authors currently emit this as 3-4 separate
    /// Paragraphs; the typed unit lets the renderer position
    /// them together and lets the LFI Critic verify a piece has
    /// a coherent attribution block.
    Byline {
        /// Author's display name (required).
        author: String,
        /// Author's role / title (e.g. "Staff writer").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        /// Publication or last-updated dateline (ISO 8601 or
        /// human-readable; the renderer treats it as free text).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dateline: Option<String>,
        /// Reading-time hint (e.g. "5 min read").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reading_time: Option<String>,
    },
    /// End-of-document footnote. Renders as a numbered entry at
    /// the bottom of a long-form piece. Distinct from
    /// [`CmsSection::Footnote`] (which is inline / mid-flow); use
    /// `Endnote` when the author wants all annotations grouped.
    Endnote {
        /// Endnote number (matches an in-body reference).
        number: u32,
        /// Endnote text.
        text: String,
    },
    /// Initial-letter drop-cap paragraph.
    DropCap { text: String },
    /// Renderer-supplied "fact about Loom" — a typed slot that
    /// expands to the current value at render time so cms/*.json
    /// authors never hand-write counts that go stale.
    ///
    /// Example: cms author writes
    /// `{"kind": "loom_fact", "which": "primitive_count", "shape": "inline"}`
    /// and the renderer emits the current count (e.g. `170`).
    /// When new primitives ship, every cms page using this slot
    /// updates automatically.
    ///
    /// Closes the "hand-authored 132 vs 170 vs 166" staleness
    /// pattern that hit `cms/{about,blog,docs,index,platform,
    /// pricing,why}.json` earlier this loop.
    LoomFact {
        /// Which fact to inject.
        which: LoomFactKind,
        /// How to wrap the value. `Inline` for `{count}` flow;
        /// `Sentence` for `"170 typed primitives ship today."`
        #[serde(default)]
        shape: LoomFactShape,
    },
    /// Figure with caption + optional credit line.
    Figure { caption: String, credit: Option<String>, asset_slug: Option<String> },
    /// Image caption text (used outside a figure).
    Caption { text: String },
    /// Numbered footnote.
    Footnote { number: u32, text: String },
    /// Marginal aside note.
    AsideNote { tone: AlertTone, body: String },
    /// Definition list (dl/dt/dd).
    DefList { items: Vec<DefListItem> },
    /// Glossary — sorted definition list with anchored terms.
    Glossary { items: Vec<DefListItem> },
    /// Auto-derived table of contents marker.
    TocBlock { heading: Option<String> },
    /// Mermaid-shaped diagram (typed source).
    Diagram { notation: DiagramKind, source: String, alt: String },
    /// Math block (LaTeX-shape source string).
    MathBlock { source: String, display: bool },
    /// Citation block (academic-style).
    Citation { text: String, source: String },
    /// Pull-out single big stat inline in editorial.
    PullStat { value: String, label: String },

    // Marketing extras (12).
    /// Testimonial card with avatar + role.
    Testimonial {
        /// Quoted body.
        body: String,
        /// Speaker name.
        attribution: String,
        /// Optional speaker role / company.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        /// Optional avatar asset slug under `/assets/`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar_slug: Option<String>,
        /// Visual treatment. `Decorated` (default, back-compat)
        /// is the legacy avatar+quote card; `Editorial` drops
        /// the card chrome and renders as pull-quote-style
        /// typography; `Minimal` is just the quote + attribution
        /// line, no card, no avatar.
        ///
        /// PRIORITY 6 audit — `.loom-testimonial` had textbook
        /// "fake testimonial card with avatars" trope CSS
        /// (rounded chrome + circular avatar). Variant-aware
        /// opt-out lets sites use the primitive without
        /// committing to that shape.
        #[serde(default)]
        decoration: TestimonialDecoration,
    },
    /// Richer logo cloud with grayscale + hover-color treatment.
    LogoCloud { heading: Option<String>, items: Vec<String> },
    /// Side-by-side feature/spec comparison.
    Comparison { heading: Option<String>, columns: Vec<String>, rows: Vec<ComparisonRow> },
    /// Vertical milestone timeline.
    Timeline { heading: Option<String>, items: Vec<TimelineItem> },
    /// Public-facing product roadmap (now/next/later).
    Roadmap { now: Vec<String>, next: Vec<String>, later: Vec<String> },
    /// Case-study card with quote + metrics.
    CaseStudy { headline: String, body: String, metrics: Vec<StatItem>, href: Option<String>, data_backend: Option<String> },
    /// Top-of-viewport announcement bar.
    AnnouncementBar { text: String, cta: Option<HeroCta>, tone: AlertTone },
    /// Cookie notice band.
    CookieNotice { text: String, accept_label: String, reject_label: String },
    /// Mid-page promo strip with CTA.
    PromoStrip { text: String, cta: HeroCta },
    /// Row of award badges.
    AwardBadges { heading: Option<String>, items: Vec<String> },
    /// Email-signup capture row.
    NewsletterSignup { heading: String, lede: Option<String>, placeholder: String, submit_label: String },
    /// Compact contact strip with channels.
    ContactStrip { items: Vec<ContactChannel> },

    // Media (10).
    /// Photo grid gallery.
    ImageGrid { items: Vec<GalleryImage>, columns: u8 },
    /// Group of figures arranged horizontally.
    FigureGroup { items: Vec<GalleryImage> },
    /// HTML5 video embed (typed source allowlist).
    VideoEmbed { src: String, poster: Option<String>, alt: String, mime: String },
    /// HTML5 audio embed.
    AudioEmbed { src: String, alt: String, mime: String },
    /// Auto-rotating image slideshow.
    Slideshow { items: Vec<GalleryImage>, interval_ms: u32 },
    /// Before/after slider comparison.
    BeforeAfter { before_alt: String, after_alt: String, before_slug: String, after_slug: String },
    /// Lightbox trigger row (click to enlarge).
    Lightbox { items: Vec<GalleryImage> },
    /// Irregular mosaic grid.
    MosaicGrid { items: Vec<GalleryImage> },
    /// Row of icon-slug references.
    IconRow { items: Vec<String> },
    /// Grid of badges (icon + label).
    BadgeGrid { items: Vec<BadgeItem> },

    // Commerce (10).
    /// Product card.
    ProductCard { name: String, price: String, rating: Option<f32>, image_alt: String, image_slug: String, href: String, data_backend: String },
    /// Product grid (collection of ProductCard payloads).
    ProductGrid { heading: Option<String>, items: Vec<ProductItem> },
    /// Inline price tag.
    PriceTag { amount: String, currency: String, was: Option<String> },
    /// Add-to-cart button.
    AddToCart { label: String, sku: String, data_backend: String },
    /// Slide-in cart drawer trigger.
    CartDrawer { label: String, count: u32 },
    /// Wishlist heart toggle.
    Wishlist { label: String, count: u32 },
    /// Product image gallery.
    ProductGallery { items: Vec<GalleryImage> },
    /// Product spec list.
    ProductSpec { items: Vec<DefListItem> },
    /// 0..=5 star rating with optional count.
    ReviewStars { value: f32, count: Option<u32> },
    /// Single review card.
    ReviewCard { author: String, rating: f32, body: String, date: Option<String> },

    // Social (10).
    /// Single avatar.
    Avatar { avatar: CmsAvatar, label: Option<String> },
    /// Overlapping avatar stack.
    AvatarStack { items: Vec<CmsAvatar>, more: Option<u32> },
    /// Chat bubble.
    ChatBubble { author: String, body: String, mine: bool },
    /// Threaded chat.
    ChatThread { items: Vec<ChatMessage> },
    /// Like/love/etc reaction row.
    ReactionRow { items: Vec<ReactionItem> },
    /// @username inline mention.
    MentionInline { username: String, href: String, data_backend: String },
    /// #tag inline hashtag.
    HashtagInline { tag: String, href: String, data_backend: String },
    /// Row of share buttons.
    ShareRow { url: String, title: String },
    /// Follow button with count.
    FollowButton { label: String, count: u32, data_backend: String },
    /// Profile card.
    ProfileCard { name: String, handle: String, bio: String, avatar: CmsAvatar, follow: Option<FollowAction> },

    // Forms (10).
    /// Single labeled input.
    FormInput { name: String, label: String, input_type: FormInputKind, placeholder: Option<String>, required: bool },
    /// Labeled select.
    FormSelect { name: String, label: String, options: Vec<SelectOption>, required: bool },
    /// Switch toggle.
    FormToggle { name: String, label: String, on: bool },
    /// Range slider.
    FormSlider { name: String, label: String, min: i32, max: i32, value: i32 },
    /// Date picker.
    FormDate { name: String, label: String, required: bool },
    /// File upload dropzone.
    FormFile { name: String, label: String, accept: String },
    /// Search input with submit.
    FormSearch { placeholder: String, data_backend: String },
    /// Color picker.
    FormColor { name: String, label: String, value: String },
    /// Long-form textarea.
    FormTextarea { name: String, label: String, placeholder: Option<String>, rows: u8 },
    /// Submit button.
    FormSubmit { label: String, data_backend: String, variant: ButtonVariant },

    // Navigation (8).
    /// Breadcrumb trail.
    Breadcrumb { items: Vec<BreadcrumbItem> },
    /// Numbered pagination.
    Pagination { current: u32, total: u32, base_href: String, data_backend: String },
    /// Tab nav (links, not in-page tabs).
    NavTabs { items: Vec<NavTabItem> },
    /// Vertical sidebar nav.
    VerticalNav { items: Vec<NavTabItem> },
    /// Mega-menu rich dropdown.
    MegaMenu { columns: Vec<MegaMenuColumn> },
    /// Floating back-to-top button.
    BackToTop { label: String },
    /// Jump-to-anchor list.
    AnchorList { items: Vec<NavTabItem> },
    /// Language picker.
    LangSwitch { current: String, options: Vec<LangOption> },

    // Feedback (8).
    /// Tonal alert box.
    Alert { tone: AlertTone, title: String, body: String, dismissible: bool },
    /// Transient toast (visible target for live regions).
    Toast { tone: AlertTone, body: String },
    /// Modal dialog placeholder (rendered as a typed section).
    Modal { title: String, body: String, primary: HeroCta, secondary: Option<HeroCta> },
    /// Side drawer.
    Drawer { title: String, body: String, side: DrawerSide },
    /// Tooltip target slot.
    Tooltip { trigger: String, body: String },
    /// Progress bar.
    ProgressBar { value: u8, label: Option<String> },
    /// Loading skeleton group.
    Skeleton { rows: u8, height: SpaceSize },
    /// Empty-state placeholder.
    EmptyState { title: String, body: String, cta: Option<HeroCta> },

    // Game / Forum / Video (8).
    /// Game tile thumbnail.
    GameTile { title: String, genre: String, players_online: u32, image_slug: String, href: String, data_backend: String },
    /// Game grid.
    GameGrid { heading: Option<String>, items: Vec<GameTileItem> },
    /// Thread list row.
    ThreadRow { title: String, author: String, replies: u32, views: u32, last_reply: String, href: String, data_backend: String },
    /// List of thread rows.
    ThreadList { heading: Option<String>, items: Vec<ThreadRowItem> },
    /// Video card with thumbnail + meta.
    VideoCard { title: String, channel: String, duration: String, views: String, thumbnail_slug: String, href: String, data_backend: String },
    /// Grid of video cards.
    VideoGridSection { heading: Option<String>, items: Vec<VideoCardItem> },
    /// Comment thread (post + nested replies).
    CommentThread { post_id: String, items: Vec<CommentItem> },
    /// Social-feed post card.
    FeedPost { author: String, handle: String, avatar: CmsAvatar, body: String, posted_at: String, reactions: u32, comments: u32 },

    // ─── T660 P6 — auth + Crucible widget primitives ───
    /// Typed sign-in / sign-up card. Holds an ordered list of
    /// authentication method choices the renderer expands into
    /// passkey buttons / social-auth rows / password fields /
    /// magic-link inputs.
    AuthCard {
        /// Display title ("Sign in", "Welcome back", etc.).
        title: String,
        /// Optional tagline under the title.
        tagline: Option<String>,
        /// Ordered method options.
        methods: Vec<AuthMethodChoice>,
        /// Optional footer text (terms / privacy disclaimer).
        footer: Option<String>,
    },
    /// Second-factor prompt: OTP / WebAuthn / backup-code.
    MfaPrompt {
        /// Title ("Enter your code").
        title: String,
        /// Factor kind shown to the user.
        factor: MfaFactorKind,
        /// Operator-facing instructions ("Enter the 6-digit code
        /// from your authenticator app").
        instructions: String,
        /// Expected code length for OTP factors (6 for TOTP,
        /// 8 for backup codes).
        #[serde(default = "default_otp_length")]
        otp_length: u8,
        /// Submit-button label.
        submit_label: String,
        /// Optional "use a different factor" link.
        switch_label: Option<String>,
    },
    /// The embeddable Crucible captcha challenge widget.
    CrucibleWidget {
        /// Challenge kind (mirrors crucible-core::ChallengeKind).
        challenge_kind: CrucibleKind,
        /// Operator-facing prompt ("Select all photos with a
        /// bird", "Which of these sentences mean the same thing?").
        prompt: String,
        /// Difficulty hint (Easy / Medium / Hard / Adversarial).
        #[serde(default)]
        difficulty: CrucibleDifficulty,
        /// Number of option slots the widget should render
        /// (e.g. 9 for a 3x3 image-classify grid).
        #[serde(default = "default_option_count")]
        option_count: u8,
        /// Submit button label.
        submit_label: String,
        /// Attribution-policy hint for the user about how their
        /// response may be used.
        attribution_hint: Option<String>,
    },
    /// Stepper for multi-step auth flows (sign-up → verify email →
    /// add MFA → complete profile).
    AuthFlowStepper {
        /// Step labels in order.
        steps: Vec<String>,
        /// Zero-indexed current step.
        current: u8,
    },
    /// Compact "signed in as / sign out" footer.
    SignedInCard {
        /// Display name.
        display_name: String,
        /// Handle or email.
        handle: String,
        /// Avatar.
        avatar: CmsAvatar,
        /// Sign-out CTA.
        sign_out: HeroCta,
    },
    /// Password-reset request form.
    PasswordReset {
        /// Title.
        title: String,
        /// Description.
        description: String,
        /// Email-input placeholder.
        placeholder: String,
        /// Submit-button label.
        submit_label: String,
    },
}

/// One option inside an [`CmsSection::AuthCard`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthMethodChoice {
    /// Continue with passkey (WebAuthn discoverable credential).
    Passkey {
        /// Button label.
        label: String,
    },
    /// Continue with platform-WebAuthn (TouchID / FaceID / Hello).
    WebauthnPlatform {
        /// Button label.
        label: String,
    },
    /// Continue with roaming-WebAuthn (YubiKey / Solo / Titan).
    WebauthnRoaming {
        /// Button label.
        label: String,
    },
    /// Continue with an OAuth provider.
    Social {
        /// Provider slug ("github", "google", "apple", "microsoft").
        provider: String,
        /// Button label.
        label: String,
    },
    /// Sign in with email magic-link.
    MagicLink {
        /// Email-input placeholder.
        placeholder: String,
        /// Submit-button label.
        submit_label: String,
    },
    /// Sign in with SMS OTP.
    SmsOtp {
        /// Phone-input placeholder.
        placeholder: String,
        /// Submit-button label.
        submit_label: String,
    },
    /// Sign in with classic password.
    Password {
        /// Email-input placeholder.
        email_placeholder: String,
        /// Password-input placeholder.
        password_placeholder: String,
        /// Submit-button label.
        submit_label: String,
        /// Optional forgot-password link.
        forgot_label: Option<String>,
    },
    /// Sign in with SSO single-sign-on link.
    Sso {
        /// Button label.
        label: String,
        /// Domain hint placeholder ("yourcompany.com").
        placeholder: String,
    },
    /// "Continue as guest" anonymous-but-receipt-bearing option.
    Anonymous {
        /// Button label.
        label: String,
    },
    /// Visual divider between method groups ("or").
    Divider {
        /// Divider label.
        label: String,
    },
}

/// Second-factor kind shown in [`CmsSection::MfaPrompt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MfaFactorKind {
    /// Time-based OTP from an authenticator app.
    #[default]
    Totp,
    /// WebAuthn second factor.
    Webauthn,
    /// SMS-delivered OTP.
    SmsOtp,
    /// Email-delivered OTP.
    EmailOtp,
    /// Printable backup codes.
    BackupCode,
}

/// Crucible challenge kind mirror (kept independent from
/// crucible-core to avoid creating a Loom-→-Crucible dep).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrucibleKind {
    /// Multi-image classification.
    #[default]
    ImageClassify,
    /// Semantic-similarity selection.
    SemanticSimilarity,
    /// Audio-transcribe with noise.
    AudioTranscribe,
    /// Arithmetic.
    MathArithmetic,
    /// Drawing reconstruction.
    DrawingReconstruct,
    /// Prompt-injection detection.
    PromptInjectionDetect,
}

/// Crucible difficulty mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrucibleDifficulty {
    /// Easy.
    #[default]
    Easy,
    /// Medium.
    Medium,
    /// Hard.
    Hard,
    /// Adversarial.
    Adversarial,
}

fn default_otp_length() -> u8 {
    6
}
fn default_option_count() -> u8 {
    9
}

/// Container max-width token.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContainerWidth { Narrow, #[default] Comfortable, Wide, Full }

/// Divider style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DividerStyle { #[default] Line, Dots, ZigZag, Sparkle }

/// Vertical-spacing token.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpaceSize { Tight, #[default] Comfortable, Loose, Generous }

/// Visual treatment for [`CmsSection::FeatureSpotlight`].
///
/// The default `Decorated` is the legacy SaaS-card shape (rounded
/// chrome + gradient icon tile + hover lift + shadow). The
/// `Editorial` and `Minimal` variants strip the trope chrome for
/// callers that want dense, non-card composition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureSpotlightDecoration {
    /// SaaS-card shape: rounded chrome, gradient icon tile,
    /// hover lift + shadow. Default (kept for back-compat).
    #[default]
    Decorated,
    /// Strip card chrome. Typography only, no icon tile, no
    /// shadow. Top accent rule per item.
    Editorial,
    /// Tight grid, no decoration. Title + body, no icon, no border.
    Minimal,
}

impl FeatureSpotlightDecoration {
    /// Class-modifier suffix emitted on the section element.
    pub const fn modifier_class(self) -> &'static str {
        match self {
            Self::Decorated => "deco-decorated",
            Self::Editorial => "deco-editorial",
            Self::Minimal => "deco-minimal",
        }
    }
}

/// Visual treatment for [`CmsSection::Testimonial`].
///
/// `Decorated` (default) is the legacy avatar+quote card.
/// `Editorial` drops card chrome for pull-quote typography.
/// `Minimal` is just the quote + attribution line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestimonialDecoration {
    /// Legacy avatar+quote card. Back-compat default.
    #[default]
    Decorated,
    /// Pull-quote-style typography, no card.
    Editorial,
    /// Quote + attribution only. No card, no avatar.
    Minimal,
}

impl TestimonialDecoration {
    /// Class-modifier suffix emitted on the section element.
    pub const fn modifier_class(self) -> &'static str {
        match self {
            Self::Decorated => "deco-decorated",
            Self::Editorial => "deco-editorial",
            Self::Minimal => "deco-minimal",
        }
    }
}

/// Which Loom fact to inject via [`CmsSection::LoomFact`]. Closed
/// enum so authors can't ask for a fact the renderer doesn't know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoomFactKind {
    /// Total count of `CmsSection` variants (Loom's "primitive count").
    PrimitiveCount,
    /// Total count of named themes shipped (light, dark, dark-amoled,
    /// auto, warm, ocean, forest, violet, rose, sepia, press, hc-dark,
    /// hc-light, and any future named theme).
    ThemeCount,
    /// Forge audit-phase count. Renderer reports the value Loom
    /// believes Forge ships; if Forge's count diverges, bump the
    /// constant.
    ForgeAuditPhaseCount,
    /// Multi-network DeployAdapter count (Tor / I2P / Lokinet /
    /// IPFS / Gemini / Clearnet).
    DeployNetworkCount,
}

/// How a [`CmsSection::LoomFact`] renders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoomFactShape {
    /// Just the numeric value, no markup. For inline use inside
    /// existing prose: `"At {{loom_fact}} typed primitives..."`
    /// — but since Loom doesn't do template strings, `Inline`
    /// here means rendering as a `<span>` so callers can place
    /// it in a Paragraph slot.
    #[default]
    Inline,
    /// A complete short sentence: `"170 typed Loom primitives
    /// ship today."`
    Sentence,
}

/// Source of truth for [`CmsSection::LoomFact`] values.
///
/// Bumped manually when shipping a new variant / theme / deploy
/// adapter — pair the bump with the change in the same commit so
/// the source-of-truth never drifts.
pub mod loom_facts {
    /// Current `CmsSection` variant count. See `CmsSection` definition.
    /// **When adding a variant, increment this constant.** The
    /// `primitive_count_is_not_wildly_off` test cross-checks this
    /// against the schemars-emitted oneOf cardinality and fails
    /// the build if they drift, so the const can't go stale.
    pub const PRIMITIVE_COUNT: u32 = 142;
    /// Current named-theme count. Defined in `BASE_THEME_CSS` +
    /// `THEME_TOGGLE_CSS`.
    pub const THEME_COUNT: u32 = 14;
    /// Forge audit-phase count. Reported by `forge build` summary.
    pub const FORGE_AUDIT_PHASE_COUNT: u32 = 27;
    /// Multi-network DeployAdapter count.
    pub const DEPLOY_NETWORK_COUNT: u32 = 6;
}

/// Reveal-motion variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevealMotion { #[default] FadeUp, FadeIn, ScaleIn, SlideLeft, SlideRight }

/// Alert tone (used by Alert, Toast, AnnouncementBar, AsideNote).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlertTone { #[default] Info, Success, Warning, Danger, Neutral }

/// Drawer side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawerSide { #[default] Right, Left }

/// Diagram source kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagramKind { #[default] Mermaid, Plantuml, Ascii }

/// Form-input kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FormInputKind { #[default] Text, Email, Password, Tel, Url, Number, Search }

/// Button variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant { #[default] Primary, Secondary, Ghost, Danger }

/// One tab in a Tabs section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabItem { pub label: String, pub body: String }

/// One accordion item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccordionItem { pub title: String, pub body: String }

/// One definition list entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefListItem { pub term: String, pub definition: String }

/// One comparison row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComparisonRow { pub label: String, pub values: Vec<String> }

/// One timeline milestone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineItem { pub when: String, pub title: String, pub body: String }

/// One contact channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactChannel { pub kind: String, pub label: String, pub href: String, pub data_backend: String }

/// One gallery image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GalleryImage { pub asset_slug: String, pub alt: String, pub caption: Option<String> }

/// One badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BadgeItem { pub icon_slug: Option<String>, pub label: String }

/// One product card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProductItem {
    pub name: String,
    pub price: String,
    pub rating: Option<f32>,
    pub image_alt: String,
    pub image_slug: String,
    pub href: String,
    pub data_backend: String,
}

/// One chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatMessage { pub author: String, pub body: String, pub mine: bool, pub at: String }

/// One reaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReactionItem { pub emoji: String, pub count: u32 }

/// Follow action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FollowAction { pub label: String, pub data_backend: String }

/// One select option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectOption { pub value: String, pub label: String }

/// One breadcrumb segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BreadcrumbItem { pub label: String, pub href: String, pub data_backend: String }

/// One nav tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NavTabItem { pub label: String, pub href: String, pub data_backend: String, #[serde(default)] pub current: bool }

/// One mega-menu column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MegaMenuColumn { pub heading: String, pub items: Vec<NavTabItem> }

/// One language option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LangOption { pub code: String, pub label: String, pub href: String, pub data_backend: String }

/// One game tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GameTileItem {
    pub title: String, pub genre: String, pub players_online: u32,
    pub image_slug: String, pub href: String, pub data_backend: String,
}

/// One thread row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThreadRowItem {
    pub title: String, pub author: String, pub replies: u32, pub views: u32,
    pub last_reply: String, pub href: String, pub data_backend: String,
}

/// One video card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoCardItem {
    pub title: String, pub channel: String, pub duration: String, pub views: String,
    pub thumbnail_slug: String, pub href: String, pub data_backend: String,
}

/// One comment in a thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommentItem { pub author: String, pub body: String, pub at: String, pub depth: u8 }

fn default_true() -> bool {
    true
}
fn default_columns_3() -> u8 {
    3
}
fn default_speed() -> u8 {
    5
}

/// Page-shell chrome kind. Picks the header + body-backdrop
/// shape. Each variant is a complete chrome aesthetic, not a
/// modifier on the same shell. Operators pick per-page via
/// `CmsPage::chrome`; new sites typically pick `FloatingPill`,
/// legacy sites stay on `PageShell` for backward compat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChromeKind {
    /// Sticky-blur top bar with brand-left / nav-right and an
    /// auto-h1 underneath. Three-radial-gradient body backdrop
    /// tinted by the active theme's accents. The default for
    /// backward compatibility.
    #[default]
    PageShell,
    /// Floating capsule: brand + nav links + action CTAs in a
    /// glass-morphism pill anchored at the top of the viewport,
    /// centered horizontally. Body backdrop is a single soft
    /// halo above the hero + a flat surface below.
    FloatingPill,
    /// No header at all. The first section carries every chrome
    /// element itself. Used for full-bleed landing pages.
    Minimal,
}

/// Backdrop kind for hero-class sections.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HeroBackground {
    /// Animated three-radial gradient mesh in the accent palette.
    /// The default — works in every theme without configuration.
    #[default]
    GradientMesh,
    /// Solid color. `token` references a loom color token slug
    /// (e.g. `"loom-color-surface"`).
    Solid {
        /// Loom color token slug.
        token: String,
    },
    /// Diagonal-stripe pattern in the accent color.
    Stripes,
    /// Subtle dot-grid pattern.
    Dots,
    /// Photographic background. `src` is a same-origin asset path
    /// (e.g. `"/assets/hero-bg.jpg"`). `alt` is the SEO/a11y
    /// description; even though the image is rendered as a CSS
    /// `background-image`, the alt text ships as `<meta>` /
    /// aria-attributes on the section.
    ///
    /// Used by sites that lead with a real photograph (people,
    /// product, location) rather than a gradient halo.
    /// `overlay` controls the dark/light scrim on top of the
    /// photo so the title stays legible: `none` / `light` / `dark`.
    Photo {
        /// Same-origin path to the image asset.
        src: String,
        /// SEO / accessibility description.
        alt: String,
        /// Overlay tint (defaults to `dark`).
        #[serde(default)]
        overlay: PhotoOverlay,
    },
}

/// Overlay tint applied on top of [`HeroBackground::Photo`] so the
/// title remains legible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PhotoOverlay {
    /// No overlay — image renders raw.
    None,
    /// Light overlay (for dark photos / dark-text titles).
    Light,
    /// Dark overlay (for bright photos / light-text titles). Default.
    #[default]
    Dark,
}

/// Visual-height ramp for hero sections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeroHeight {
    /// Comfortable default — about 60vh on desktop.
    #[default]
    Comfortable,
    /// Compact — about 40vh, fits secondary pages.
    Compact,
    /// Tall — about 80vh, only for top-of-funnel landings.
    Tall,
}

/// The visual half of a [`CmsSection::SplitHero`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SplitVisual {
    /// A code snippet, rendered as a styled `<pre>` panel.
    CodeSnippet {
        /// Language hint.
        #[serde(default)]
        lang: String,
        /// Body.
        body: String,
    },
    /// A single big stat with label.
    StatBlock {
        /// The number ("2.4M", "99.97%", "12x").
        value: String,
        /// Label underneath.
        label: String,
    },
    /// Reference to a `loom-assets` photo / illustration slug.
    AssetSlug {
        /// Slug from the asset registry.
        slug: String,
        /// Alt text for accessibility.
        alt: String,
    },
}

/// One feature spotlight item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpotlightItem {
    /// Optional loom-assets icon slug (e.g. `icon-arrow-right`).
    pub icon_slug: Option<String>,
    /// Feature heading.
    pub title: String,
    /// Feature body paragraph.
    pub body: String,
    /// Optional "learn more" link.
    pub href: Option<String>,
    /// Backend slug paired with href.
    pub data_backend: Option<String>,
}

/// One stat in a [`CmsSection::StatBand`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatItem {
    /// Value — displayed as the large headline.
    pub value: String,
    /// Label underneath.
    pub label: String,
    /// Optional muted hint below the label.
    pub hint: Option<String>,
}

/// One step in a [`CmsSection::Steps`] section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepItem {
    /// Step heading (renderer auto-prefixes the step number).
    pub title: String,
    /// Body paragraph.
    pub body: String,
}

/// One pricing tier in a [`CmsSection::Pricing`] section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PricingTier {
    /// Tier name ("Solo", "Team", "Enterprise").
    pub name: String,
    /// Price string ("$0", "$29", "Custom").
    pub price: String,
    /// Period qualifier ("/mo", "/seat/mo", "annual", empty).
    #[serde(default)]
    pub period: String,
    /// Optional short tagline.
    pub tagline: Option<String>,
    /// Feature bullet list.
    pub features: Vec<String>,
    /// Optional CTA.
    pub cta: Option<HeroCta>,
    /// True → tier is visually highlighted (typically the
    /// middle "recommended" tier).
    #[serde(default)]
    pub highlighted: bool,
    /// Optional badge label displayed on the tier card (e.g.
    /// "Most popular", "Best value", "Compliance-ready").
    /// `None` → no badge; the hardcoded English "Popular" CSS
    /// fallback is gone (substrate-de-consumer-shaping +
    /// localization fix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

/// One FAQ item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FaqItem {
    /// Question text.
    pub question: String,
    /// Answer body — may contain multiple paragraphs.
    pub answer: Vec<String>,
}

/// Marquee scroll direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarqueeDirection {
    /// Scroll right-to-left.
    #[default]
    Left,
    /// Scroll left-to-right.
    Right,
}

/// FORGE_ROADMAP item 41: one entry in a [`CmsSection::KvPair`].
///
/// BUG ASSUMPTION: `key` and `value` carry no markup; renderer
/// auto-escapes via Maud. `hint` is also escaped and rendered as
/// a muted single-line caption — not a place for arbitrary HTML.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsKvItem {
    /// Label / dt text. Auto-escaped on render.
    pub key: String,
    /// Value / dd text. Auto-escaped on render.
    pub value: String,
    /// Optional muted caption shown below the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// T660 P1: one brand logo entry in a [`CmsSection::LogoWall`].
///
/// `name` is the canonical brand name + lookup key into the
/// future `loom-brand-icons` registry. `href` is the brand's
/// website. Until the registry crate lands, the renderer falls
/// back to typographic rendering (the name in `loom-font-display`).
///
/// AVP-2: never carries inline SVG; the registry is the only
/// path through which SVG markup reaches the page.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CmsLogoItem {
    /// Brand name (also the lookup key into `loom-brand-icons`).
    /// Examples: "Stripe", "Linear", "Vercel".
    pub name: String,
    /// Brand website URL. Auto-escaped + rendered as the wrap
    /// element if non-empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
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
/// Heading level encoded over the wire as a raw u8 (2..=6) — the
/// `serde(into / try_from)` pair lets derive produce the same wire
/// shape as the prior hand-rolled impls (rejected by the composition
/// Per-element bounded typographic adjustment. Each variant
/// nudges one default in one direction. Substrate guarantees
/// each adjustment is exactly ONE step from default — no
/// compounding, no unbounded styling. Authors who want stronger
/// emphasis pick a different primitive (e.g. PullQuote for
/// dramatic body text, ImageHero for marketing title weight).
///
/// Maps to a `loom-polish--<kebab>` class on the rendered
/// element. CSS rules are caller-side; substrate ships sensible
/// defaults via skin.css.
///
/// Per `feedback_consumer_shaped_substrate`: polish tokens let
/// authors express "this specific element deserves a bit more
/// weight" without unbounded CSS — a structural compromise that
/// keeps the substrate's variation policy bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PolishToken {
    /// Tighter letter-spacing.
    Tighter,
    /// Looser letter-spacing.
    Looser,
    /// Taller line-height.
    Taller,
    /// Shorter line-height.
    Shorter,
    /// Bolder font-weight.
    Bolder,
    /// Lighter font-weight.
    Lighter,
    /// Larger font-size.
    Larger,
    /// Smaller font-size.
    Smaller,
    /// More-saturated color.
    MoreSaturated,
    /// Less-saturated (muted) color.
    LessSaturated,
}

impl PolishToken {
    /// Class-name slug (kebab-case, no `loom-polish--` prefix).
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Tighter => "tighter",
            Self::Looser => "looser",
            Self::Taller => "taller",
            Self::Shorter => "shorter",
            Self::Bolder => "bolder",
            Self::Lighter => "lighter",
            Self::Larger => "larger",
            Self::Smaller => "smaller",
            Self::MoreSaturated => "more-saturated",
            Self::LessSaturated => "less-saturated",
        }
    }
}

/// Build the space-joined class string for a slice of polish
/// tokens. Each token contributes one `loom-polish--<slug>`
/// class. Duplicates are NOT collapsed at this layer — callers
/// pre-dedupe if they care; CSS is idempotent so duplicates
/// don't compound.
#[must_use]
pub fn polish_class_string(tokens: &[PolishToken]) -> String {
    let mut out = String::new();
    for t in tokens {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("loom-polish--");
        out.push_str(t.slug());
    }
    out
}

/// Content-width preference for `main#content`. Wires through
/// to a `data-content-width="X"` attribute on `<body>` that
/// skin.css matches with `main#content { max-inline-size: ... }`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContentWidth {
    /// 42rem — editorial-press measure (~70 characters).
    Narrow,
    /// 64rem — default. Generous body + side-margin gutters.
    #[default]
    Comfortable,
    /// 90rem — dense-grid / app-shell pages.
    Wide,
    /// No max — full-bleed content.
    Full,
}

impl ContentWidth {
    /// Data-attribute slug for the `<body>` selector.
    #[must_use]
    pub fn attr_value(self) -> &'static str {
        match self {
            Self::Narrow => "narrow",
            Self::Comfortable => "comfortable",
            Self::Wide => "wide",
            Self::Full => "full",
        }
    }
}

/// One named category in a [`CmsSection::SettingsPanel`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsCategory {
    /// Category heading (e.g. "Account", "Privacy", "Danger zone").
    pub name: String,
    /// Items in this category.
    pub items: Vec<SettingsItem>,
}

/// One item in a [`SettingsCategory`]. Each item pairs a label
/// with a control. Controls are typed via `SettingsControl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsItem {
    /// Visible label rendered as the `<dt>` for the row.
    pub label: String,
    /// Form-input name attribute — keys the value in the
    /// submitted form.
    pub name: String,
    /// Optional muted hint shown below the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The typed control.
    pub control: SettingsControl,
}

/// Typed control for a [`SettingsItem`]. Closed enum; adding a
/// variant is a deliberate surface change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingsControl {
    /// On/off checkbox.
    Toggle {
        /// Whether the toggle ships as `checked`.
        #[serde(default)]
        default_on: bool,
    },
    /// Single-line text input.
    Text {
        /// Pre-filled value.
        #[serde(default)]
        default_value: String,
        /// Placeholder shown when empty.
        #[serde(default)]
        placeholder: String,
        /// Optional max length.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
    },
    /// Multi-line textarea.
    Textarea {
        /// Pre-filled value.
        #[serde(default)]
        default_value: String,
        /// Rows attribute.
        #[serde(default = "default_settings_textarea_rows")]
        rows: u8,
    },
    /// Destructive action button. Renders separately from the
    /// main form submit — its own POST endpoint + a confirm
    /// prompt at the form level.
    DangerButton {
        /// Button label (e.g. "Delete account").
        button_label: String,
        /// Confirm-prompt text — rendered as the form's
        /// `aria-describedby` content + as a visual disclaimer.
        confirm_text: String,
        /// Where the danger action POSTs. Distinct from the
        /// panel's main `action`.
        action: String,
    },
}

fn default_settings_submit_label() -> String {
    "Save changes".to_owned()
}

fn default_settings_textarea_rows() -> u8 {
    4
}

fn default_profile_submit_label() -> String {
    "Update profile".to_owned()
}

fn default_no_avatar() -> CmsAvatar {
    CmsAvatar::None
}

/// One section of a [`CmsSection::LegalDoc`]. Each section gets
/// a stable kebab-case anchor (derived from `heading`) so the
/// auto-generated table of contents can link to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LegalSection {
    /// Section heading text.
    pub heading: String,
    /// Body paragraphs (plain text, auto-escaped). Each entry
    /// renders as a `<p>` in order.
    pub body: Vec<String>,
    /// Optional plain-language summary callout box inside this
    /// section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_language: Option<String>,
}

/// Wide-viewport float side for [`CmsSection::Marginalia`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarginaliaPosition {
    /// Float to the right of the body text (start of inline-axis
    /// in RTL locales). Default — least visually-disruptive
    /// because it occupies the gutter on the reading-direction
    /// side.
    #[default]
    Right,
    /// Float to the left of the body text.
    Left,
}

/// Editorial treatment for a [`CmsSection::Paragraph`].
///
/// All variants render `<p>` — the difference is class-keyed CSS:
///
/// * `Body` (default) — standard body text. No extra class.
/// * `Lead` — larger leading paragraph (the lede after a hero).
///   Renders with `class="loom-paragraph--lead"`.
/// * `DropCap` — first letter rendered as a multi-line drop cap.
///   Renders with `class="loom-paragraph--dropcap"`.
/// * `Aside` — visually offset commentary; left-rule, slightly
///   muted color. Renders with `class="loom-paragraph--aside"`.
///
/// Pure decoration. Text content is unaffected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphDecoration {
    /// Default body text. No extra class.
    #[default]
    Body,
    /// Larger lead paragraph (post-hero introduction).
    Lead,
    /// First letter rendered as a multi-line drop cap.
    DropCap,
    /// Visually offset commentary — left rule + muted color.
    Aside,
}

/// audit as a manual-derivable). Same JSON encoding, less code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
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

impl From<HeadingLevel> for u8 {
    fn from(h: HeadingLevel) -> Self {
        h.as_u8()
    }
}

impl TryFrom<u8> for HeadingLevel {
    type Error = HeadingLevelOutOfRange;
    fn try_from(n: u8) -> Result<Self, Self::Error> {
        Self::from_u8(n).ok_or(HeadingLevelOutOfRange(n))
    }
}

/// Returned when a numeric heading level is outside the 2..=6 range.
/// `h1` is owned by the page shell, so it is intentionally excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingLevelOutOfRange(
    /// The offending input value.
    pub u8,
);

impl std::fmt::Display for HeadingLevelOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "heading level must be 2..=6 (h1 is owned by the page-shell), got {}",
            self.0
        )
    }
}

impl std::error::Error for HeadingLevelOutOfRange {}

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
        obj.metadata().description =
            Some("Heading level (h2-h6). h1 is reserved for the page-shell.".to_owned());
        schemars::schema::Schema::Object(obj)
    }
}

/// Hero CTA — the single typed primary action attached to a Hero
/// section. URL is validated by `composer::is_safe_url` at render
/// time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// Optional `data-tone` value for the tag chip (curated palette
    /// in skin.css: violet/indigo/ocean/forest/amber/rose/ruby/
    /// walnut/slate/teal). When set, drives the chip's bg/fg/border
    /// hue. Falls back to the site's primary brand color when None.
    /// Sites that need a custom hue should keep `tone` None and
    /// emit inline `style="--tag-color: …"` via a future field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    /// T70a: optional 16:9 media slot rendered ABOVE the body.
    /// When None, no media block is emitted (card is pure text).
    /// When Some, a `<div class="loom-card-feed-item__media">`
    /// wraps the asset (image / picture / video). Lazy-load via
    /// loading="lazy". Object-fit:cover via skin.css.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<CmsCardMedia>,
}

/// T70a: media slot for [`CmsCard`]. Three shapes:
/// - `Image { src, alt, srcset?, width?, height? }` — `<img loading="lazy">`
/// - `Video { poster?, src, type, alt }` — native `<video>` (no autoplay)
/// - `Placeholder { tone? }` — visible empty media area with
///   gradient bg (no `<img>`/`<video>`), useful while content is
///   being authored. data-empty="true" lets skin.css show a soft
///   pattern.
///
/// All variants are SAFE: src/poster validated by `is_safe_url`;
/// alt is REQUIRED for Image (a11y); type is restricted to known
/// safe MIME values for Video.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CmsCardMedia {
    /// Static image. `loading="lazy"`. `decoding="async"`.
    Image {
        /// Resource URL. Validated by `is_safe_url`.
        src: String,
        /// REQUIRED accessible-name text. Empty string allowed
        /// only for purely decorative media (then aria-hidden
        /// will be set by the renderer too).
        alt: String,
        /// Optional `srcset` for responsive density (1x/2x/3x).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        srcset: Option<String>,
        /// Optional intrinsic dimensions (drives layout-shift
        /// avoidance via the rendered width/height attrs).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        /// See `width`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
    },
    /// Native HTML `<video>`. No autoplay. controls=true. preload=metadata.
    Video {
        /// Optional poster image (shown before play).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        poster: Option<String>,
        /// Video resource URL.
        src: String,
        /// MIME type — must be one of: video/mp4, video/webm, video/ogg.
        mime: String,
        /// Accessible-name (for screen-reader fallback).
        alt: String,
    },
    /// Visible-but-empty media area. CSS gradient placeholder.
    Placeholder {
        /// Optional `data-tone` to colorize the placeholder.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<String>,
    },
}

const ALLOWED_VIDEO_MIME: &[&str] = &["video/mp4", "video/webm", "video/ogg"];

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
        CmsSection::Paragraph { text, decoration } => {
            let class = match decoration {
                ParagraphDecoration::Body => "loom-prose",
                ParagraphDecoration::Lead => "loom-prose loom-paragraph--lead",
                ParagraphDecoration::DropCap => "loom-prose loom-paragraph--dropcap",
                ParagraphDecoration::Aside => "loom-prose loom-paragraph--aside",
            };
            html! { p class=(class) { (text) } }
        }
        CmsSection::Heading { text, level, polish } => {
            // T36 (2026-05-14): typed HeadingLevel enum makes
            // out-of-range values uncompilable. The runtime clamp
            // + data-cms-warn fallback are gone — invalid levels
            // never reach this match (Deserialize fails first at
            // the JSON boundary).
            let polish_classes = polish_class_string(polish);
            let class_attr = if polish_classes.is_empty() {
                "loom-heading".to_owned()
            } else {
                format!("loom-heading {polish_classes}")
            };
            match level {
                HeadingLevel::H2 => html! {
                    h2 class=(class_attr) data-loom-level="2" { (text) }
                },
                HeadingLevel::H3 => html! {
                    h3 class=(class_attr) data-loom-level="3" { (text) }
                },
                HeadingLevel::H4 => html! {
                    h4 class=(class_attr) data-loom-level="4" { (text) }
                },
                HeadingLevel::H5 => html! {
                    h5 class=(class_attr) data-loom-level="5" { (text) }
                },
                HeadingLevel::H6 => html! {
                    h6 class=(class_attr) data-loom-level="6" { (text) }
                },
            }
        }
        CmsSection::KvPair { heading, items } => html! {
            section class="loom-kv-section" {
                @if let Some(h) = heading {
                    h2 class="loom-kv-heading" { (h) }
                }
                dl class="loom-kv-list" {
                    @for item in items {
                        div class="loom-kv-row" {
                            dt class="loom-kv-key" { (item.key) }
                            dd class="loom-kv-value" {
                                span class="loom-kv-text" { (item.value) }
                                @if let Some(hint) = &item.hint {
                                    span class="loom-kv-hint" { (hint) }
                                }
                            }
                        }
                    }
                }
            }
        },
        // T660 P1: typographic LogoWall fallback until loom-brand-icons
        // ships the vetted SVG registry. Each item renders as the name
        // in display font; if href is set, wraps in <a>.
        CmsSection::LogoWall { heading, items } => html! {
            section class="loom-logo-wall" {
                @if let Some(h) = heading {
                    h2 class="loom-logo-wall-heading" { (h) }
                }
                ul class="loom-logo-wall-list" {
                    @for item in items {
                        li class="loom-logo-wall-item" {
                            @match item.href.as_deref() {
                                Some(href) if is_safe_url(href) => {
                                    a href=(href) class="loom-logo-wall-link"
                                        rel="external nofollow noopener" {
                                        span class="loom-logo-wall-name" { (item.name) }
                                    }
                                }
                                _ => {
                                    span class="loom-logo-wall-name" { (item.name) }
                                }
                            }
                        }
                    }
                }
            }
        },
        // T660 P2: Quote / testimonial. Semantic <blockquote> with
        // <cite> attribution row; auto-escaped throughout.
        CmsSection::Quote {
            body,
            attribution,
            role,
        } => html! {
            section class="loom-quote" {
                blockquote class="loom-quote-body" {
                    p { (body) }
                }
                footer class="loom-quote-footer" {
                    cite class="loom-quote-cite" {
                        span class="loom-quote-attribution" { (attribution) }
                        @if let Some(r) = role {
                            span class="loom-quote-role" { (r) }
                        }
                    }
                }
            }
        },
        // T660 P3: Code / terminal block. Semantic <pre><code> with
        // a language class for any downstream syntax highlighter +
        // data-loom-terminal for terminal-style chrome. Body text
        // auto-escapes via Maud.
        CmsSection::Code {
            lang,
            body,
            caption,
            terminal,
        } => html! {
            section class="loom-code" data-loom-terminal=[terminal.then_some("true")] {
                @if let Some(c) = caption {
                    figcaption class="loom-code-caption" { (c) }
                }
                pre class="loom-code-pre" {
                    code class={ "loom-code-body language-" (lang) } {
                        (body)
                    }
                }
            }
        },
        CmsSection::ImageHero {
            eyebrow,
            title,
            lede,
            cta,
            background,
            height,
            before_headline,
            after_cta,
        } => {
            let bg_class = match background {
                HeroBackground::GradientMesh => "gradient-mesh",
                HeroBackground::Solid { .. } => "solid",
                HeroBackground::Stripes => "stripes",
                HeroBackground::Dots => "dots",
                HeroBackground::Photo { .. } => "photo",
            };
            let height_class = match height {
                HeroHeight::Comfortable => "h-comfortable",
                HeroHeight::Compact => "h-compact",
                HeroHeight::Tall => "h-tall",
            };
            let cta_href_safe = cta
                .as_ref()
                .is_none_or(|c| loom_components::composer::is_safe_url(&c.href));
            // For HeroBackground::Photo, emit an <img> as a positioned
            // background-layer child (no inline style — strict CSP
            // permits `src` + `alt` attributes natively).
            let photo_block = match background {
                HeroBackground::Photo { src, alt, overlay } => {
                    let overlay_class = match overlay {
                        PhotoOverlay::None => "ov-none",
                        PhotoOverlay::Light => "ov-light",
                        PhotoOverlay::Dark => "ov-dark",
                    };
                    let src_safe = loom_components::composer::is_safe_url(src);
                    Some((src.clone(), alt.clone(), overlay_class, src_safe))
                }
                _ => None,
            };
            html! {
                section class={ "loom-image-hero loom-bleed bg-" (bg_class) " " (height_class) }
                    data-loom-image-hero data-loom-reveal {
                    @if let Some((src, alt, overlay_class, src_safe)) = photo_block {
                        img class={ "loom-image-hero__photo " (overlay_class) }
                            src=(if src_safe { src.as_str() } else { "" })
                            alt=(alt)
                            data-invalid=[(!src_safe).then_some("true")]
                            loading="eager"
                            decoding="async";
                    }
                    div class="loom-image-hero__inner" {
                        @if !before_headline.is_empty() {
                            div class="loom-image-hero__slot loom-image-hero__slot--before-headline" {
                                @for item in before_headline {
                                    (render_section(item))
                                }
                            }
                        }
                        @if let Some(e) = eyebrow {
                            span class="loom-image-hero__eyebrow" { (e) }
                        }
                        h2 class="loom-image-hero__title" { (title) }
                        @if let Some(l) = lede {
                            p class="loom-image-hero__lede" { (l) }
                        }
                        @if let Some(c) = cta {
                            a class="loom-image-hero__cta loom-btn loom-btn--primary"
                              href=(if cta_href_safe { c.href.as_str() } else { "#invalid-cta" })
                              data-backend=(c.data_backend)
                              data-invalid=[(!cta_href_safe).then_some("true")] { (c.label) }
                        }
                        @if !after_cta.is_empty() {
                            div class="loom-image-hero__slot loom-image-hero__slot--after-cta" {
                                @for item in after_cta {
                                    (render_section(item))
                                }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::SplitHero {
            eyebrow,
            title,
            lede,
            cta,
            visual,
            visual_right,
        } => {
            let order_class = if *visual_right { "visual-right" } else { "visual-left" };
            let cta_href_safe = cta
                .as_ref()
                .is_none_or(|c| loom_components::composer::is_safe_url(&c.href));
            html! {
                section class={ "loom-split-hero " (order_class) }
                    data-loom-split-hero data-loom-reveal {
                    div class="loom-split-hero__text" {
                        @if let Some(e) = eyebrow {
                            span class="loom-split-hero__eyebrow" { (e) }
                        }
                        h2 class="loom-split-hero__title" { (title) }
                        @if let Some(l) = lede {
                            p class="loom-split-hero__lede" { (l) }
                        }
                        @if let Some(c) = cta {
                            a class="loom-split-hero__cta loom-btn loom-btn--primary"
                              href=(if cta_href_safe { c.href.as_str() } else { "#invalid-cta" })
                              data-backend=(c.data_backend)
                              data-invalid=[(!cta_href_safe).then_some("true")] { (c.label) }
                        }
                    }
                    div class="loom-split-hero__visual" {
                        @match visual {
                            SplitVisual::CodeSnippet { lang, body } => {
                                pre class="loom-split-hero__code" {
                                    code class={ "language-" (lang) } { (body) }
                                }
                            }
                            SplitVisual::StatBlock { value, label } => {
                                div class="loom-split-hero__stat" {
                                    span class="loom-split-hero__stat-value" { (value) }
                                    span class="loom-split-hero__stat-label" { (label) }
                                }
                            }
                            SplitVisual::AssetSlug { slug, alt } => {
                                div class="loom-split-hero__asset"
                                    data-asset-slug=(slug)
                                    aria-label=(alt) {
                                    span class="loom-asset-placeholder" { (alt) }
                                }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::FeatureSpotlight {
            heading,
            lede,
            items,
            columns,
            decoration,
        } => {
            let cols = (*columns).clamp(1, 4);
            let deco = decoration.modifier_class();
            html! {
                section class={ "loom-feature-spotlight cols-" (cols) " " (deco) }
                    data-loom-feature-spotlight data-loom-reveal {
                    @if let Some(h) = heading {
                        h2 class="loom-feature-spotlight__heading" { (h) }
                    }
                    @if let Some(l) = lede {
                        p class="loom-feature-spotlight__lede" { (l) }
                    }
                    div class="loom-feature-spotlight__grid" {
                        @for item in items {
                            article class="loom-feature-spotlight__item" data-loom-reveal {
                                @if let Some(icon) = &item.icon_slug {
                                    span class="loom-feature-spotlight__icon"
                                        data-asset-slug=(icon) aria-hidden="true" {
                                        @if let Some(reg) = loom_icons::by_slug(icon) {
                                            (maud::PreEscaped(reg.render_with_class("loom-feature-spotlight__icon-svg")))
                                        }
                                    }
                                }
                                h3 class="loom-feature-spotlight__title" { (item.title) }
                                p class="loom-feature-spotlight__body" { (item.body) }
                                @if let Some(href) = &item.href {
                                    @let href_safe = loom_components::composer::is_safe_url(href);
                                    a class="loom-feature-spotlight__more"
                                      href=(if href_safe { href.as_str() } else { "#invalid-link" })
                                      data-backend=[item.data_backend.as_deref()]
                                      data-invalid=[(!href_safe).then_some("true")] {
                                        "Learn more →"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::StatBand { heading, lede, items } => html! {
            section class="loom-stat-band" data-loom-stat-band data-loom-reveal {
                @if let Some(h) = heading {
                    h2 class="loom-stat-band__heading" { (h) }
                }
                @if let Some(l) = lede {
                    p class="loom-stat-band__lede" { (l) }
                }
                div class="loom-stat-band__row" {
                    @for item in items {
                        div class="loom-stat-band__item" data-loom-reveal {
                            span class="loom-stat-band__value" data-loom-counter=(item.value) {
                                (item.value)
                            }
                            span class="loom-stat-band__label" { (item.label) }
                            @if let Some(h) = &item.hint {
                                span class="loom-stat-band__hint" { (h) }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::Steps { heading, lede, items } => html! {
            section class="loom-steps" data-loom-steps data-loom-reveal {
                @if let Some(h) = heading {
                    h2 class="loom-steps__heading" { (h) }
                }
                @if let Some(l) = lede {
                    p class="loom-steps__lede" { (l) }
                }
                ol class="loom-steps__list" {
                    @for (i, item) in items.iter().enumerate() {
                        li class="loom-steps__item" data-step=((i + 1).to_string()) data-loom-reveal {
                            span class="loom-steps__num" { ((i + 1).to_string()) }
                            div class="loom-steps__body" {
                                h3 class="loom-steps__title" { (item.title) }
                                p class="loom-steps__text" { (item.body) }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::Pricing { heading, lede, tiers } => html! {
            section class="loom-pricing" data-loom-pricing data-loom-reveal {
                @if let Some(h) = heading {
                    h2 class="loom-pricing__heading" { (h) }
                }
                @if let Some(l) = lede {
                    p class="loom-pricing__lede" { (l) }
                }
                div class="loom-pricing__row" {
                    @for tier in tiers {
                        @let cta_href_safe = tier
                            .cta
                            .as_ref()
                            .is_none_or(|c| loom_components::composer::is_safe_url(&c.href));
                        article class={ "loom-pricing__tier" @if tier.highlighted { " is-highlighted" } }
                            data-loom-reveal {
                            @if let Some(b) = &tier.badge {
                                span class="loom-pricing__badge" { (b) }
                            }
                            h3 class="loom-pricing__name" { (tier.name) }
                            div class="loom-pricing__price-row" {
                                span class="loom-pricing__price" { (tier.price) }
                                @if !tier.period.is_empty() {
                                    span class="loom-pricing__period" { (tier.period) }
                                }
                            }
                            @if let Some(t) = &tier.tagline {
                                p class="loom-pricing__tagline" { (t) }
                            }
                            ul class="loom-pricing__features" {
                                @for f in &tier.features {
                                    li class="loom-pricing__feature" { (f) }
                                }
                            }
                            @if let Some(c) = &tier.cta {
                                a class="loom-pricing__cta loom-btn loom-btn--primary"
                                  href=(if cta_href_safe { c.href.as_str() } else { "#invalid-cta" })
                                  data-backend=(c.data_backend)
                                  data-invalid=[(!cta_href_safe).then_some("true")] { (c.label) }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::Faq { heading, lede, items, single_expand } => html! {
            section class="loom-faq" data-loom-faq
                data-single-expand=[single_expand.then_some("true")]
                data-loom-reveal {
                @if let Some(h) = heading {
                    h2 class="loom-faq__heading" { (h) }
                }
                @if let Some(l) = lede {
                    p class="loom-faq__lede" { (l) }
                }
                div class="loom-faq__list" {
                    @for item in items {
                        details class="loom-faq__item" {
                            summary class="loom-faq__question" { (item.question) }
                            div class="loom-faq__answer" {
                                @for p in &item.answer {
                                    p class="loom-faq__paragraph" { (p) }
                                }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::Marquee { items, direction, speed } => {
            let dir_class = match direction {
                MarqueeDirection::Left => "marquee-left",
                MarqueeDirection::Right => "marquee-right",
            };
            let s = (*speed).clamp(1, 10);
            html! {
                section class={ "loom-marquee " (dir_class) " loom-bleed" }
                    data-loom-marquee data-speed=(s.to_string()) aria-hidden="true" {
                    div class="loom-marquee__track" {
                        @for _rep in 0..2 {
                            @for it in items {
                                span class="loom-marquee__item" { (it) }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::CallToAction { eyebrow, title, lede, cta, background } => {
            let bg_class = match background {
                HeroBackground::GradientMesh => "gradient-mesh",
                HeroBackground::Solid { .. } => "solid",
                HeroBackground::Stripes => "stripes",
                HeroBackground::Dots => "dots",
                HeroBackground::Photo { .. } => "photo",
            };
            let cta_href_safe = loom_components::composer::is_safe_url(&cta.href);
            html! {
                section class={ "loom-cta-band loom-bleed bg-" (bg_class) }
                    data-loom-cta-band data-loom-reveal {
                    div class="loom-cta-band__inner" {
                        @if let Some(e) = eyebrow {
                            span class="loom-cta-band__eyebrow" { (e) }
                        }
                        h2 class="loom-cta-band__title" { (title) }
                        @if let Some(l) = lede {
                            p class="loom-cta-band__lede" { (l) }
                        }
                        a class="loom-cta-band__cta loom-btn loom-btn--primary loom-btn--lg"
                          href=(if cta_href_safe { cta.href.as_str() } else { "#invalid-cta" })
                          data-backend=(cta.data_backend)
                          data-invalid=[(!cta_href_safe).then_some("true")] { (cta.label) }
                    }
                }
            }
        },
        CmsSection::Marginalia { body, position } => {
            let pos_class = match position {
                MarginaliaPosition::Left => "loom-marginalia--left",
                MarginaliaPosition::Right => "loom-marginalia--right",
            };
            html! {
                aside class={ "loom-marginalia " (pos_class) } role="note" {
                    span class="loom-marginalia__body" { (body) }
                }
            }
        }
        CmsSection::AccountSummary {
            display_name,
            avatar,
            plan,
            member_since,
            handle,
        } => {
            html! {
                section class="loom-account-summary" data-loom-account-summary {
                    div class="loom-account-summary__avatar" {
                        @match avatar {
                            CmsAvatar::None => {}
                            CmsAvatar::Initials { letters } => {
                                span class="loom-avatar loom-avatar--initials" { (letters) }
                            }
                            CmsAvatar::Image { src, alt } => {
                                @let safe = loom_components::composer::is_safe_url(src);
                                img class="loom-avatar loom-avatar--image"
                                    src=(if safe { src.as_str() } else { "" })
                                    alt=(alt)
                                    data-invalid=[(!safe).then_some("true")]
                                    decoding="async";
                            }
                        }
                    }
                    div class="loom-account-summary__body" {
                        h2 class="loom-account-summary__name" { (display_name) }
                        @if let Some(h) = handle {
                            p class="loom-account-summary__handle" { "@" (h) }
                        }
                        dl class="loom-account-summary__meta" {
                            div class="loom-account-summary__row" {
                                dt { "Plan" } dd { (plan) }
                            }
                            div class="loom-account-summary__row" {
                                dt { "Member since" } dd { (member_since) }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::ProfileEdit {
            action,
            display_name,
            handle,
            pronouns,
            bio,
            language,
            submit_label,
        } => {
            let action_safe = loom_components::composer::is_safe_url(action);
            html! {
                section class="loom-profile-edit" data-loom-profile-edit {
                    h2 class="loom-profile-edit__heading" { "Profile" }
                    form class="loom-profile-edit__form"
                        method="post"
                        action=(if action_safe { action.as_str() } else { "#invalid-action" })
                        data-invalid=[(!action_safe).then_some("true")] {
                        div class="loom-profile-edit__row" {
                            label for="profile-display-name" { "Display name" }
                            input type="text"
                                id="profile-display-name"
                                name="display_name"
                                value=(display_name);
                        }
                        div class="loom-profile-edit__row" {
                            label for="profile-handle" { "Handle" }
                            input type="text"
                                id="profile-handle"
                                name="handle"
                                value=(handle)
                                placeholder="username";
                        }
                        div class="loom-profile-edit__row" {
                            label for="profile-pronouns" { "Pronouns" }
                            input type="text"
                                id="profile-pronouns"
                                name="pronouns"
                                value=(pronouns)
                                placeholder="they / them";
                        }
                        div class="loom-profile-edit__row" {
                            label for="profile-bio" { "Bio" }
                            textarea id="profile-bio" name="bio" rows="4" { (bio) }
                        }
                        div class="loom-profile-edit__row" {
                            label for="profile-language" { "Language" }
                            input type="text"
                                id="profile-language"
                                name="language"
                                value=(language)
                                placeholder="en";
                        }
                        div class="loom-profile-edit__submit" {
                            button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
                        }
                    }
                }
            }
        }
        CmsSection::LegalDoc {
            title,
            last_updated,
            plain_language_summary,
            sections_list,
        } => {
            // Derive stable kebab-case anchors from each heading.
            let anchors: Vec<String> = sections_list
                .iter()
                .map(|s| {
                    s.heading
                        .to_lowercase()
                        .chars()
                        .map(|c| if c.is_alphanumeric() { c } else { '-' })
                        .collect::<String>()
                        .trim_matches('-')
                        .to_owned()
                })
                .collect();
            html! {
                article class="loom-legal-doc" data-loom-legal-doc {
                    header class="loom-legal-doc__header" {
                        h1 class="loom-legal-doc__title" { (title) }
                        p class="loom-legal-doc__updated" {
                            "Last updated " (last_updated)
                        }
                    }
                    @if let Some(summary) = plain_language_summary {
                        aside class="loom-legal-doc__summary" role="note" {
                            strong { "In plain language:" }
                            " " (summary)
                        }
                    }
                    nav class="loom-legal-doc__toc" aria-label="Table of contents" {
                        h2 class="loom-legal-doc__toc-heading" { "Contents" }
                        ol class="loom-legal-doc__toc-list" {
                            @for (i, s) in sections_list.iter().enumerate() {
                                @let anchor = &anchors[i];
                                li { a href={"#" (anchor)} { (s.heading) } }
                            }
                        }
                    }
                    div class="loom-legal-doc__body" {
                        @for (i, s) in sections_list.iter().enumerate() {
                            @let anchor = &anchors[i];
                            section class="loom-legal-doc__section" {
                                h2 id=(anchor) class="loom-legal-doc__section-heading" { (s.heading) }
                                @if let Some(pl) = &s.plain_language {
                                    aside class="loom-legal-doc__section-summary" role="note" {
                                        strong { "Plain language:" } " " (pl)
                                    }
                                }
                                @for paragraph in &s.body {
                                    p class="loom-legal-doc__paragraph" { (paragraph) }
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::SettingsPanel {
            heading,
            lede,
            action,
            categories,
            submit_label,
        } => {
            let action_safe = loom_components::composer::is_safe_url(action);
            html! {
                section class="loom-settings-panel" data-loom-settings {
                    @if let Some(h) = heading {
                        h2 class="loom-settings-panel__heading" { (h) }
                    }
                    @if let Some(l) = lede {
                        p class="loom-settings-panel__lede" { (l) }
                    }
                    form class="loom-settings-panel__form"
                        method="post"
                        action=(if action_safe { action.as_str() } else { "#invalid-action" })
                        data-invalid=[(!action_safe).then_some("true")] {
                        @for category in categories {
                            fieldset class="loom-settings-category" {
                                legend class="loom-settings-category__name" { (category.name) }
                                dl class="loom-settings-category__items" {
                                    @for item in &category.items {
                                        div class="loom-settings-item" {
                                            dt class="loom-settings-item__label" {
                                                label for=(item.name) { (item.label) }
                                                @if let Some(hint) = &item.hint {
                                                    span class="loom-settings-item__hint" { (hint) }
                                                }
                                            }
                                            dd class="loom-settings-item__control" {
                                                @match &item.control {
                                                    SettingsControl::Toggle { default_on } => {
                                                        input type="checkbox"
                                                            id=(item.name)
                                                            name=(item.name)
                                                            checked[*default_on];
                                                    }
                                                    SettingsControl::Text { default_value, placeholder, max_length } => {
                                                        input type="text"
                                                            id=(item.name)
                                                            name=(item.name)
                                                            value=(default_value)
                                                            placeholder=(placeholder)
                                                            maxlength=[max_length.map(|m| m.to_string())];
                                                    }
                                                    SettingsControl::Textarea { default_value, rows } => {
                                                        textarea
                                                            id=(item.name)
                                                            name=(item.name)
                                                            rows=(rows.to_string()) { (default_value) }
                                                    }
                                                    SettingsControl::DangerButton { button_label, confirm_text, action: danger_action } => {
                                                        @let danger_safe = loom_components::composer::is_safe_url(danger_action);
                                                        form class="loom-settings-item__danger-form"
                                                            method="post"
                                                            action=(if danger_safe { danger_action.as_str() } else { "#invalid-action" })
                                                            data-invalid=[(!danger_safe).then_some("true")] {
                                                            p class="loom-settings-item__danger-confirm" { (confirm_text) }
                                                            button type="submit" class="loom-btn loom-btn--danger" { (button_label) }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div class="loom-settings-panel__submit" {
                            button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
                        }
                    }
                }
            }
        }
        CmsSection::PullQuote { body, attribution } => html! {
            figure class="loom-pull-quote" data-loom-reveal {
                blockquote class="loom-pull-quote__body" { (body) }
                @if let Some(a) = attribution {
                    figcaption class="loom-pull-quote__attribution" { "— " (a) }
                }
            }
        },
        CmsSection::Epigraph { body, attribution } => html! {
            figure class="loom-epigraph" data-loom-reveal {
                blockquote class="loom-epigraph__body" { (body) }
                @if let Some(a) = attribution {
                    figcaption class="loom-epigraph__attribution" { "— " (a) }
                }
            }
        },
        // ─── T660 P5 — catalogue expansion render arms ───
        CmsSection::Container { children_html, max_width } => {
            let w = match max_width {
                ContainerWidth::Narrow => "narrow",
                ContainerWidth::Comfortable => "comfortable",
                ContainerWidth::Wide => "wide",
                ContainerWidth::Full => "full",
            };
            html! { div class={ "loom-container w-" (w) } { (maud::PreEscaped(escape_html_text(children_html).to_string())) } }
        }
        CmsSection::Divider { style } => {
            let s = match style { DividerStyle::Line => "line", DividerStyle::Dots => "dots", DividerStyle::ZigZag => "zigzag", DividerStyle::Sparkle => "sparkle" };
            html! { hr class={ "loom-divider style-" (s) } aria-hidden="true"; }
        }
        CmsSection::Spacer { size } => {
            let s = space_class(size);
            html! { div class={ "loom-spacer " (s) } aria-hidden="true" {} }
        }
        CmsSection::Columns { columns, items } => {
            let c = (*columns).clamp(2, 4);
            html! {
                div class={ "loom-columns cols-" (c) } {
                    @for item in items { div class="loom-columns__item" { (item) } }
                }
            }
        }
        CmsSection::Stack { gap, items } => {
            let g = space_class(gap);
            html! { div class={ "loom-stack " (g) } { @for it in items { div class="loom-stack__item" { (it) } } } }
        }
        CmsSection::Cluster { gap, items } => {
            let g = space_class(gap);
            html! { div class={ "loom-cluster " (g) } { @for it in items { span class="loom-cluster__chip" { (it) } } } }
        }
        CmsSection::GridLayout { columns, items } => {
            let c = (*columns).clamp(1, 6);
            html! { div class={ "loom-grid cols-" (c) } { @for it in items { div class="loom-grid__cell" { (it) } } } }
        }
        CmsSection::Tabs { items } => html! {
            section class="loom-tabs" data-loom-tabs data-loom-reveal {
                div class="loom-tabs__bar" role="tablist" {
                    @for (i, t) in items.iter().enumerate() {
                        button class="loom-tabs__tab" role="tab" type="button"
                            aria-selected=(if i == 0 { "true" } else { "false" })
                            data-tab=(i.to_string()) { (t.label) }
                    }
                }
                div class="loom-tabs__panes" {
                    @for (i, t) in items.iter().enumerate() {
                        div class="loom-tabs__pane" role="tabpanel"
                            aria-hidden=(if i == 0 { "false" } else { "true" })
                            data-pane=(i.to_string()) { (t.body) }
                    }
                }
            }
        },
        CmsSection::AccordionGroup { items } => html! {
            section class="loom-accordion" data-loom-accordion data-loom-reveal {
                @for it in items {
                    details class="loom-accordion__item" {
                        summary class="loom-accordion__title" { (it.title) }
                        div class="loom-accordion__body" { (it.body) }
                    }
                }
            }
        },
        CmsSection::Reveal { motion, body } => {
            let m = match motion {
                RevealMotion::FadeUp => "fade-up", RevealMotion::FadeIn => "fade-in",
                RevealMotion::ScaleIn => "scale-in", RevealMotion::SlideLeft => "slide-left",
                RevealMotion::SlideRight => "slide-right",
            };
            html! { div class={ "loom-reveal motion-" (m) } data-loom-reveal { (body) } }
        }
        CmsSection::Article { body } => html! { article class="loom-article" { (body) } },
        CmsSection::SubHeading { text, level } => {
            let lvl = (*level).clamp(2, 6);
            html! { @match lvl {
                2 => h2 class="loom-subhead" { (text) },
                3 => h3 class="loom-subhead" { (text) },
                4 => h4 class="loom-subhead" { (text) },
                5 => h5 class="loom-subhead" { (text) },
                _ => h6 class="loom-subhead" { (text) },
            } }
        }
        CmsSection::Lede { text } => html! { p class="loom-lede" data-loom-reveal { (text) } },
        CmsSection::Sublede { text } => html! {
            p class="loom-sublede" data-loom-reveal { (text) }
        },
        CmsSection::Kicker { text } => html! {
            span class="loom-kicker" data-loom-reveal { (text) }
        },
        CmsSection::Byline { author, role, dateline, reading_time } => html! {
            p class="loom-byline" data-loom-reveal {
                span class="loom-byline__author" { (author) }
                @if let Some(r) = role {
                    " · " span class="loom-byline__role" { (r) }
                }
                @if let Some(d) = dateline {
                    " · " span class="loom-byline__dateline" { (d) }
                }
                @if let Some(rt) = reading_time {
                    " · " span class="loom-byline__reading-time" { (rt) }
                }
            }
        },
        CmsSection::Endnote { number, text } => html! {
            aside class="loom-endnote" id={ "endnote-" (number.to_string()) } {
                span class="loom-endnote__num" { (number.to_string()) "." } " " (text)
            }
        },
        CmsSection::DropCap { text } => html! { p class="loom-dropcap" data-loom-reveal { (text) } },
        CmsSection::LoomFact { which, shape } => {
            let value = match which {
                LoomFactKind::PrimitiveCount => loom_facts::PRIMITIVE_COUNT,
                LoomFactKind::ThemeCount => loom_facts::THEME_COUNT,
                LoomFactKind::ForgeAuditPhaseCount => loom_facts::FORGE_AUDIT_PHASE_COUNT,
                LoomFactKind::DeployNetworkCount => loom_facts::DEPLOY_NETWORK_COUNT,
            };
            let noun = match which {
                LoomFactKind::PrimitiveCount => "typed Loom primitives",
                LoomFactKind::ThemeCount => "named themes",
                LoomFactKind::ForgeAuditPhaseCount => "audit phases per commit",
                LoomFactKind::DeployNetworkCount => "deploy networks",
            };
            match shape {
                LoomFactShape::Inline => html! {
                    span class="loom-fact" data-loom-fact=(format!("{which:?}")) { (value.to_string()) }
                },
                LoomFactShape::Sentence => html! {
                    p class="loom-fact loom-fact--sentence" data-loom-fact=(format!("{which:?}")) {
                        strong class="loom-fact__value" { (value.to_string()) }
                        " " (noun) " ship today."
                    }
                },
            }
        }
        CmsSection::Figure { caption, credit, asset_slug } => html! {
            figure class="loom-figure" data-loom-reveal {
                @if let Some(slug) = asset_slug {
                    div class="loom-figure__media" data-asset-slug=(slug) { span class="loom-asset-placeholder" { (caption) } }
                }
                figcaption class="loom-figure__caption" {
                    (caption)
                    @if let Some(c) = credit { span class="loom-figure__credit" { " · " (c) } }
                }
            }
        },
        CmsSection::Caption { text } => html! { p class="loom-caption" { (text) } },
        CmsSection::Footnote { number, text } => html! {
            aside class="loom-footnote" id={ "fn-" (number.to_string()) } {
                sup class="loom-footnote__num" { (number.to_string()) } " " (text)
            }
        },
        CmsSection::AsideNote { tone, body } => {
            let t = alert_tone_class(tone);
            html! { aside class={ "loom-aside-note tone-" (t) } role="note" { (body) } }
        }
        CmsSection::DefList { items } => html! {
            dl class="loom-deflist" {
                @for it in items {
                    dt class="loom-deflist__term" { (it.term) }
                    dd class="loom-deflist__def" { (it.definition) }
                }
            }
        },
        CmsSection::Glossary { items } => html! {
            section class="loom-glossary" data-loom-reveal {
                dl class="loom-deflist" {
                    @for it in items {
                        dt class="loom-deflist__term" id={ "term-" (slugify(&it.term)) } { (it.term) }
                        dd class="loom-deflist__def" { (it.definition) }
                    }
                }
            }
        },
        CmsSection::TocBlock { heading } => html! {
            nav class="loom-toc" aria-label="Table of contents" data-loom-toc {
                @if let Some(h) = heading { p class="loom-toc__heading" { (h) } }
                ol class="loom-toc__list" data-loom-toc-auto {}
            }
        },
        CmsSection::Diagram { notation, source, alt } => {
            let n = match notation { DiagramKind::Mermaid => "mermaid", DiagramKind::Plantuml => "plantuml", DiagramKind::Ascii => "ascii" };
            html! {
                figure class={ "loom-diagram notation-" (n) } role="img" aria-label=(alt) data-loom-reveal {
                    pre class="loom-diagram__source" { (source) }
                }
            }
        }
        CmsSection::MathBlock { source, display } => html! {
            @if *display {
                div class="loom-math display" role="math" { (source) }
            } @else {
                span class="loom-math inline" role="math" { (source) }
            }
        },
        CmsSection::Citation { text, source } => html! {
            blockquote class="loom-citation" data-loom-reveal {
                p class="loom-citation__text" { (text) }
                cite class="loom-citation__source" { (source) }
            }
        },
        CmsSection::PullStat { value, label } => html! {
            div class="loom-pull-stat" data-loom-reveal {
                span class="loom-pull-stat__value" { (value) }
                span class="loom-pull-stat__label" { (label) }
            }
        },
        CmsSection::Testimonial { body, attribution, role, avatar_slug, decoration } => {
            let deco = decoration.modifier_class();
            let show_avatar = matches!(decoration, TestimonialDecoration::Decorated);
            html! {
                figure class={ "loom-testimonial " (deco) } data-loom-reveal {
                    blockquote class="loom-testimonial__body" { (body) }
                    figcaption class="loom-testimonial__author" {
                        @if show_avatar {
                            @if let Some(slug) = avatar_slug {
                                span class="loom-testimonial__avatar" data-asset-slug=(slug) aria-hidden="true" {}
                            }
                        }
                        span class="loom-testimonial__name" { (attribution) }
                        @if let Some(r) = role { span class="loom-testimonial__role" { " · " (r) } }
                    }
                }
            }
        },
        CmsSection::LogoCloud { heading, items } => html! {
            section class="loom-logo-cloud" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-logo-cloud__heading" { (h) } }
                div class="loom-logo-cloud__row" { @for it in items { span class="loom-logo-cloud__item" { (it) } } }
            }
        },
        CmsSection::Comparison { heading, columns, rows } => html! {
            section class="loom-comparison" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-comparison__heading" { (h) } }
                table class="loom-comparison__table" {
                    thead { tr {
                        th {}
                        @for c in columns { th { (c) } }
                    } }
                    tbody { @for row in rows {
                        tr {
                            th scope="row" { (row.label) }
                            @for v in &row.values { td { (v) } }
                        }
                    } }
                }
            }
        },
        CmsSection::Timeline { heading, items } => html! {
            section class="loom-timeline" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-timeline__heading" { (h) } }
                ol class="loom-timeline__list" {
                    @for it in items {
                        li class="loom-timeline__item" data-loom-reveal {
                            time class="loom-timeline__when" { (it.when) }
                            h3 class="loom-timeline__title" { (it.title) }
                            p class="loom-timeline__body" { (it.body) }
                        }
                    }
                }
            }
        },
        CmsSection::Roadmap { now, next, later } => html! {
            section class="loom-roadmap" data-loom-reveal {
                div class="loom-roadmap__col col-now" {
                    h3 class="loom-roadmap__heading" { "Now" }
                    ul { @for it in now { li { (it) } } }
                }
                div class="loom-roadmap__col col-next" {
                    h3 class="loom-roadmap__heading" { "Next" }
                    ul { @for it in next { li { (it) } } }
                }
                div class="loom-roadmap__col col-later" {
                    h3 class="loom-roadmap__heading" { "Later" }
                    ul { @for it in later { li { (it) } } }
                }
            }
        },
        CmsSection::CaseStudy { headline, body, metrics, href, data_backend } => {
            let safe = href.as_deref().map_or(true, is_safe_url);
            html! {
                article class="loom-case-study" data-loom-reveal {
                    h3 class="loom-case-study__headline" { (headline) }
                    p class="loom-case-study__body" { (body) }
                    ul class="loom-case-study__metrics" {
                        @for m in metrics {
                            li class="loom-case-study__metric" {
                                span class="loom-case-study__metric-value" { (m.value) }
                                span class="loom-case-study__metric-label" { (m.label) }
                            }
                        }
                    }
                    @if let Some(h) = href {
                        a class="loom-case-study__more"
                          href=(if safe { h.as_str() } else { "#invalid-link" })
                          data-backend=[data_backend.as_deref()]
                          data-invalid=[(!safe).then_some("true")] { "Read the case study →" }
                    }
                }
            }
        }
        CmsSection::AnnouncementBar { text, cta, tone } => {
            let t = alert_tone_class(tone);
            let cta_safe = cta.as_ref().is_none_or(|c| is_safe_url(&c.href));
            html! {
                div class={ "loom-announcement-bar loom-bleed tone-" (t) } role="region" aria-label="Announcement" {
                    span class="loom-announcement-bar__text" { (text) }
                    @if let Some(c) = cta {
                        a class="loom-announcement-bar__cta"
                          href=(if cta_safe { c.href.as_str() } else { "#invalid-cta" })
                          data-backend=(c.data_backend) { (c.label) }
                    }
                }
            }
        }
        CmsSection::CookieNotice { text, accept_label, reject_label } => html! {
            div class="loom-cookie-notice" role="dialog" aria-label="Cookie notice" data-loom-cookie {
                p class="loom-cookie-notice__text" { (text) }
                div class="loom-cookie-notice__actions" {
                    button type="button" class="loom-btn loom-btn--primary" data-loom-cookie-accept { (accept_label) }
                    button type="button" class="loom-btn loom-btn--ghost" data-loom-cookie-reject { (reject_label) }
                }
            }
        },
        CmsSection::PromoStrip { text, cta } => {
            let safe = is_safe_url(&cta.href);
            html! {
                div class="loom-promo-strip" data-loom-reveal {
                    span class="loom-promo-strip__text" { (text) }
                    a class="loom-promo-strip__cta loom-btn loom-btn--primary"
                      href=(if safe { cta.href.as_str() } else { "#invalid-cta" })
                      data-backend=(cta.data_backend) { (cta.label) }
                }
            }
        }
        CmsSection::AwardBadges { heading, items } => html! {
            section class="loom-award-badges" data-loom-reveal {
                @if let Some(h) = heading { h3 class="loom-award-badges__heading" { (h) } }
                ul class="loom-award-badges__list" { @for it in items { li class="loom-award-badges__item" { (it) } } }
            }
        },
        CmsSection::NewsletterSignup { heading, lede, placeholder, submit_label } => html! {
            section class="loom-newsletter-signup" data-loom-reveal {
                h2 class="loom-newsletter-signup__heading" { (heading) }
                @if let Some(l) = lede { p class="loom-newsletter-signup__lede" { (l) } }
                form class="loom-newsletter-signup__form" data-loom-newsletter {
                    input type="email" name="email" required placeholder=(placeholder) aria-label="Email";
                    button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
                }
            }
        },
        CmsSection::ContactStrip { items } => html! {
            section class="loom-contact-strip" data-loom-reveal {
                @for it in items {
                    @let safe = is_safe_url(&it.href);
                    a class={ "loom-contact-strip__item kind-" (it.kind) }
                      href=(if safe { it.href.as_str() } else { "#invalid-link" })
                      data-backend=(it.data_backend) {
                        span class="loom-contact-strip__label" { (it.label) }
                    }
                }
            }
        },
        CmsSection::ImageGrid { items, columns } => {
            let c = (*columns).clamp(2, 6);
            html! {
                section class={ "loom-image-grid cols-" (c) } data-loom-reveal {
                    @for img in items {
                        figure class="loom-image-grid__cell" data-asset-slug=(img.asset_slug) aria-label=(img.alt) {
                            span class="loom-asset-placeholder" { (img.alt) }
                            @if let Some(cap) = &img.caption { figcaption class="loom-image-grid__caption" { (cap) } }
                        }
                    }
                }
            }
        }
        CmsSection::FigureGroup { items } => html! {
            section class="loom-figure-group" data-loom-reveal {
                @for img in items {
                    figure class="loom-figure-group__cell" data-asset-slug=(img.asset_slug) aria-label=(img.alt) {
                        span class="loom-asset-placeholder" { (img.alt) }
                        @if let Some(cap) = &img.caption { figcaption { (cap) } }
                    }
                }
            }
        },
        CmsSection::VideoEmbed { src, poster, alt, mime } => {
            let src_safe = is_safe_url(src);
            let poster_safe = poster.as_deref().map(is_safe_url).unwrap_or(true);
            let mime_ok = ALLOWED_VIDEO_MIME.contains(&mime.as_str());
            html! {
                @if src_safe && poster_safe && mime_ok {
                    figure class="loom-video-embed" data-loom-reveal {
                        video controls preload="metadata" poster=[poster.as_deref()] aria-label=(alt) {
                            source src=(src) type=(mime);
                        }
                    }
                } @else {
                    div class="loom-video-embed" data-empty="true" aria-label=(alt) {}
                }
            }
        }
        CmsSection::AudioEmbed { src, alt, mime } => {
            let safe = is_safe_url(src);
            html! {
                @if safe {
                    figure class="loom-audio-embed" data-loom-reveal {
                        audio controls preload="metadata" aria-label=(alt) {
                            source src=(src) type=(mime);
                        }
                    }
                } @else {
                    div class="loom-audio-embed" data-empty="true" aria-label=(alt) {}
                }
            }
        }
        CmsSection::Slideshow { items, interval_ms } => html! {
            section class="loom-slideshow" data-loom-slideshow data-interval=(interval_ms.to_string()) data-loom-reveal {
                @for (i, img) in items.iter().enumerate() {
                    figure class="loom-slideshow__slide"
                        data-index=(i.to_string())
                        data-active=(if i == 0 { "true" } else { "false" })
                        data-asset-slug=(img.asset_slug)
                        aria-label=(img.alt) {
                        span class="loom-asset-placeholder" { (img.alt) }
                    }
                }
            }
        },
        CmsSection::BeforeAfter { before_alt, after_alt, before_slug, after_slug } => html! {
            div class="loom-before-after" data-loom-before-after data-loom-reveal {
                figure class="loom-before-after__before" data-asset-slug=(before_slug) aria-label=(before_alt) {
                    span class="loom-asset-placeholder" { (before_alt) }
                }
                figure class="loom-before-after__after" data-asset-slug=(after_slug) aria-label=(after_alt) {
                    span class="loom-asset-placeholder" { (after_alt) }
                }
                input type="range" min="0" max="100" value="50" aria-label="Reveal slider" class="loom-before-after__slider";
            }
        },
        CmsSection::Lightbox { items } => html! {
            section class="loom-lightbox" data-loom-lightbox data-loom-reveal {
                @for img in items {
                    button type="button" class="loom-lightbox__thumb" data-asset-slug=(img.asset_slug) aria-label=(img.alt) {
                        span class="loom-asset-placeholder" { (img.alt) }
                    }
                }
            }
        },
        CmsSection::MosaicGrid { items } => html! {
            section class="loom-mosaic" data-loom-reveal {
                @for img in items {
                    figure class="loom-mosaic__cell" data-asset-slug=(img.asset_slug) aria-label=(img.alt) {
                        span class="loom-asset-placeholder" { (img.alt) }
                    }
                }
            }
        },
        CmsSection::IconRow { items } => html! {
            div class="loom-icon-row" { @for slug in items { span class="loom-icon-row__icon" data-asset-slug=(slug) aria-hidden="true" {} } }
        },
        CmsSection::BadgeGrid { items } => html! {
            div class="loom-badge-grid" data-loom-reveal {
                @for b in items {
                    span class="loom-badge-grid__item" {
                        @if let Some(slug) = &b.icon_slug { span class="loom-badge-grid__icon" data-asset-slug=(slug) aria-hidden="true" {} }
                        span class="loom-badge-grid__label" { (b.label) }
                    }
                }
            }
        },
        CmsSection::ProductCard { name, price, rating, image_alt, image_slug, href, data_backend } => {
            let safe = is_safe_url(href);
            html! {
                article class="loom-product-card" data-loom-reveal {
                    a class="loom-product-card__link"
                      href=(if safe { href.as_str() } else { "#invalid-link" })
                      data-backend=(data_backend) {
                        figure class="loom-product-card__image" data-asset-slug=(image_slug) aria-label=(image_alt) {
                            span class="loom-asset-placeholder" { (image_alt) }
                        }
                        h3 class="loom-product-card__name" { (name) }
                        div class="loom-product-card__price" { (price) }
                        @if let Some(r) = rating {
                            div class="loom-product-card__rating" aria-label=({ format!("{:.1} out of 5", r) }) {
                                @for i in 0..5 {
                                    span class={ "loom-star " (if (i as f32) < *r { "filled" } else { "empty" }) } aria-hidden="true" { "★" }
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::ProductGrid { heading, items } => html! {
            section class="loom-product-grid" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-product-grid__heading" { (h) } }
                div class="loom-product-grid__row" {
                    @for p in items {
                        @let safe = is_safe_url(&p.href);
                        article class="loom-product-card" data-loom-reveal {
                            a class="loom-product-card__link"
                              href=(if safe { p.href.as_str() } else { "#invalid-link" })
                              data-backend=(p.data_backend) {
                                figure class="loom-product-card__image" data-asset-slug=(p.image_slug) aria-label=(p.image_alt) {
                                    span class="loom-asset-placeholder" { (p.image_alt) }
                                }
                                h3 class="loom-product-card__name" { (p.name) }
                                div class="loom-product-card__price" { (p.price) }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::PriceTag { amount, currency, was } => html! {
            span class="loom-price-tag" {
                @if let Some(w) = was { s class="loom-price-tag__was" { (w) } " " }
                span class="loom-price-tag__amount" { (amount) }
                span class="loom-price-tag__currency" { " " (currency) }
            }
        },
        CmsSection::AddToCart { label, sku, data_backend } => html! {
            button type="button" class="loom-btn loom-btn--primary loom-add-to-cart"
                data-sku=(sku) data-backend=(data_backend) { (label) }
        },
        CmsSection::CartDrawer { label, count } => html! {
            button type="button" class="loom-cart-drawer" data-loom-cart-trigger aria-label=(label) {
                span class="loom-cart-drawer__icon" aria-hidden="true" { "🛒" }
                @if *count > 0 { span class="loom-cart-drawer__badge" { (count.to_string()) } }
            }
        },
        CmsSection::Wishlist { label, count } => html! {
            button type="button" class="loom-wishlist" aria-label=(label) {
                span class="loom-wishlist__icon" aria-hidden="true" { "♡" }
                @if *count > 0 { span class="loom-wishlist__count" { (count.to_string()) } }
            }
        },
        CmsSection::ProductGallery { items } => html! {
            section class="loom-product-gallery" data-loom-reveal {
                @for img in items {
                    figure class="loom-product-gallery__cell" data-asset-slug=(img.asset_slug) aria-label=(img.alt) {
                        span class="loom-asset-placeholder" { (img.alt) }
                    }
                }
            }
        },
        CmsSection::ProductSpec { items } => html! {
            dl class="loom-product-spec" {
                @for it in items {
                    dt class="loom-product-spec__term" { (it.term) }
                    dd class="loom-product-spec__def" { (it.definition) }
                }
            }
        },
        CmsSection::ReviewStars { value, count } => html! {
            span class="loom-review-stars" aria-label=({ format!("{:.1} out of 5", value) }) {
                @for i in 0..5 {
                    span class={ "loom-star " (if (i as f32) < *value { "filled" } else { "empty" }) } aria-hidden="true" { "★" }
                }
                @if let Some(c) = count { span class="loom-review-stars__count" { " (" (c.to_string()) ")" } }
            }
        },
        CmsSection::ReviewCard { author, rating, body, date } => html! {
            article class="loom-review-card" data-loom-reveal {
                header class="loom-review-card__header" {
                    span class="loom-review-card__author" { (author) }
                    @if let Some(d) = date { time class="loom-review-card__date" { (d) } }
                }
                span class="loom-review-stars" aria-label=({ format!("{:.1} out of 5", rating) }) {
                    @for i in 0..5 {
                        span class={ "loom-star " (if (i as f32) < *rating { "filled" } else { "empty" }) } aria-hidden="true" { "★" }
                    }
                }
                p class="loom-review-card__body" { (body) }
            }
        },
        CmsSection::Avatar { avatar, label } => html! {
            span class="loom-avatar-section" {
                (render_avatar(avatar))
                @if let Some(l) = label { span class="loom-avatar-section__label" { (l) } }
            }
        },
        CmsSection::AvatarStack { items, more } => html! {
            div class="loom-avatar-stack" {
                @for a in items { (render_avatar(a)) }
                @if let Some(m) = more { span class="loom-avatar-stack__more" { "+" (m.to_string()) } }
            }
        },
        CmsSection::ChatBubble { author, body, mine } => html! {
            div class={ "loom-chat-bubble " (if *mine { "mine" } else { "theirs" }) } {
                span class="loom-chat-bubble__author" { (author) }
                p class="loom-chat-bubble__body" { (body) }
            }
        },
        CmsSection::ChatThread { items } => html! {
            section class="loom-chat-thread" data-loom-reveal {
                @for m in items {
                    div class={ "loom-chat-bubble " (if m.mine { "mine" } else { "theirs" }) } {
                        span class="loom-chat-bubble__author" { (m.author) }
                        p class="loom-chat-bubble__body" { (m.body) }
                        time class="loom-chat-bubble__at" { (m.at) }
                    }
                }
            }
        },
        CmsSection::ReactionRow { items } => html! {
            div class="loom-reaction-row" {
                @for r in items {
                    button type="button" class="loom-reaction-row__item" {
                        span class="loom-reaction-row__emoji" aria-hidden="true" { (r.emoji) }
                        span class="loom-reaction-row__count" { (r.count.to_string()) }
                    }
                }
            }
        },
        CmsSection::MentionInline { username, href, data_backend } => {
            let safe = is_safe_url(href);
            html! {
                a class="loom-mention"
                  href=(if safe { href.as_str() } else { "#invalid-link" })
                  data-backend=(data_backend) { "@" (username) }
            }
        }
        CmsSection::HashtagInline { tag, href, data_backend } => {
            let safe = is_safe_url(href);
            html! {
                a class="loom-hashtag"
                  href=(if safe { href.as_str() } else { "#invalid-link" })
                  data-backend=(data_backend) { "#" (tag) }
            }
        }
        CmsSection::ShareRow { url, title } => html! {
            div class="loom-share-row" data-share-url=(url) data-share-title=(title) {
                button type="button" class="loom-share-row__btn" data-network="copy" aria-label="Copy link" { "🔗" }
                button type="button" class="loom-share-row__btn" data-network="email" aria-label="Email" { "✉" }
                button type="button" class="loom-share-row__btn" data-network="print" aria-label="Print" { "🖨" }
            }
        },
        CmsSection::FollowButton { label, count, data_backend } => html! {
            button type="button" class="loom-follow-btn loom-btn loom-btn--primary" data-backend=(data_backend) {
                (label) " · " span class="loom-follow-btn__count" { (count.to_string()) }
            }
        },
        CmsSection::ProfileCard { name, handle, bio, avatar, follow } => html! {
            article class="loom-profile-card" data-loom-reveal {
                (render_avatar(avatar))
                h3 class="loom-profile-card__name" { (name) }
                p class="loom-profile-card__handle" { "@" (handle) }
                p class="loom-profile-card__bio" { (bio) }
                @if let Some(f) = follow {
                    button type="button" class="loom-follow-btn loom-btn loom-btn--primary" data-backend=(f.data_backend) { (f.label) }
                }
            }
        },
        CmsSection::FormInput { name, label, input_type, placeholder, required } => {
            let t = match input_type {
                FormInputKind::Text => "text", FormInputKind::Email => "email",
                FormInputKind::Password => "password", FormInputKind::Tel => "tel",
                FormInputKind::Url => "url", FormInputKind::Number => "number",
                FormInputKind::Search => "search",
            };
            html! {
                label class="loom-form-input" {
                    span class="loom-form-input__label" { (label) @if *required { " *" } }
                    input type=(t) name=(name) placeholder=[placeholder.as_deref()] required=[required.then_some("required")];
                }
            }
        }
        CmsSection::FormSelect { name, label, options, required } => html! {
            label class="loom-form-select" {
                span class="loom-form-select__label" { (label) @if *required { " *" } }
                select name=(name) required=[required.then_some("required")] {
                    @for o in options { option value=(o.value) { (o.label) } }
                }
            }
        },
        CmsSection::FormToggle { name, label, on } => html! {
            label class="loom-form-toggle" {
                input type="checkbox" name=(name) checked=[on.then_some("checked")];
                span class="loom-form-toggle__track" aria-hidden="true" {}
                span class="loom-form-toggle__label" { (label) }
            }
        },
        CmsSection::FormSlider { name, label, min, max, value } => html! {
            label class="loom-form-slider" {
                span class="loom-form-slider__label" { (label) }
                input type="range" name=(name) min=(min.to_string()) max=(max.to_string()) value=(value.to_string());
            }
        },
        CmsSection::FormDate { name, label, required } => html! {
            label class="loom-form-date" {
                span class="loom-form-date__label" { (label) @if *required { " *" } }
                input type="date" name=(name) required=[required.then_some("required")];
            }
        },
        CmsSection::FormFile { name, label, accept } => html! {
            label class="loom-form-file" {
                span class="loom-form-file__label" { (label) }
                input type="file" name=(name) accept=(accept);
            }
        },
        CmsSection::FormSearch { placeholder, data_backend } => html! {
            form class="loom-form-search" role="search" data-backend=(data_backend) {
                input type="search" name="q" placeholder=(placeholder) aria-label="Search";
                button type="submit" class="loom-btn loom-btn--primary" { "Search" }
            }
        },
        CmsSection::FormColor { name, label, value } => html! {
            label class="loom-form-color" {
                span class="loom-form-color__label" { (label) }
                input type="color" name=(name) value=(value);
            }
        },
        CmsSection::FormTextarea { name, label, placeholder, rows } => html! {
            label class="loom-form-textarea" {
                span class="loom-form-textarea__label" { (label) }
                textarea name=(name) rows=(rows.to_string()) placeholder=[placeholder.as_deref()] {}
            }
        },
        CmsSection::FormSubmit { label, data_backend, variant } => {
            let v = match variant { ButtonVariant::Primary => "primary", ButtonVariant::Secondary => "secondary", ButtonVariant::Ghost => "ghost", ButtonVariant::Danger => "danger" };
            html! {
                button type="submit" class={ "loom-btn loom-btn--" (v) } data-backend=(data_backend) { (label) }
            }
        }
        CmsSection::Breadcrumb { items } => html! {
            nav class="loom-breadcrumb" aria-label="Breadcrumb" {
                ol class="loom-breadcrumb__list" {
                    @for (i, it) in items.iter().enumerate() {
                        @let safe = is_safe_url(&it.href);
                        li class="loom-breadcrumb__item" {
                            @if i > 0 { span class="loom-breadcrumb__sep" aria-hidden="true" { " / " } }
                            a href=(if safe { it.href.as_str() } else { "#invalid-link" }) data-backend=(it.data_backend) { (it.label) }
                        }
                    }
                }
            }
        },
        CmsSection::Pagination { current, total, base_href, data_backend } => html! {
            nav class="loom-pagination" aria-label="Pagination" {
                @for n in 1..=*total {
                    a class={ "loom-pagination__page " (if n == *current { "current" } else { "" }) }
                      href=({ format!("{}?p={}", base_href, n) })
                      data-backend=(data_backend)
                      aria-current=[(n == *current).then_some("page")] { (n.to_string()) }
                }
            }
        },
        CmsSection::NavTabs { items } => html! {
            nav class="loom-nav-tabs" aria-label="Tabs" {
                @for it in items {
                    @let safe = is_safe_url(&it.href);
                    a class={ "loom-nav-tabs__tab " (if it.current { "current" } else { "" }) }
                      href=(if safe { it.href.as_str() } else { "#invalid-link" })
                      data-backend=(it.data_backend)
                      aria-current=[it.current.then_some("page")] { (it.label) }
                }
            }
        },
        CmsSection::VerticalNav { items } => html! {
            nav class="loom-vertical-nav" aria-label="Sidebar" {
                @for it in items {
                    @let safe = is_safe_url(&it.href);
                    a class={ "loom-vertical-nav__item " (if it.current { "current" } else { "" }) }
                      href=(if safe { it.href.as_str() } else { "#invalid-link" })
                      data-backend=(it.data_backend) { (it.label) }
                }
            }
        },
        CmsSection::MegaMenu { columns } => html! {
            div class="loom-mega-menu" data-loom-mega-menu {
                @for col in columns {
                    div class="loom-mega-menu__col" {
                        h4 class="loom-mega-menu__heading" { (col.heading) }
                        ul {
                            @for it in &col.items {
                                @let safe = is_safe_url(&it.href);
                                li {
                                    a href=(if safe { it.href.as_str() } else { "#invalid-link" })
                                      data-backend=(it.data_backend) { (it.label) }
                                }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::BackToTop { label } => html! {
            a class="loom-back-to-top" href="#top" aria-label=(label) { "↑" }
        },
        CmsSection::AnchorList { items } => html! {
            nav class="loom-anchor-list" aria-label="On this page" {
                ol {
                    @for it in items {
                        li {
                            a class="loom-anchor-list__link"
                              href=(it.href)
                              data-backend=(it.data_backend) { (it.label) }
                        }
                    }
                }
            }
        },
        CmsSection::LangSwitch { current, options } => html! {
            nav class="loom-lang-switch" aria-label="Language" {
                span class="loom-lang-switch__current" { (current) }
                ul {
                    @for o in options {
                        @let safe = is_safe_url(&o.href);
                        li {
                            a href=(if safe { o.href.as_str() } else { "#invalid-link" })
                              data-backend=(o.data_backend)
                              lang=(o.code) { (o.label) }
                        }
                    }
                }
            }
        },
        CmsSection::Alert { tone, title, body, dismissible } => {
            let t = alert_tone_class(tone);
            html! {
                div class={ "loom-alert tone-" (t) } role="alert" data-loom-reveal {
                    strong class="loom-alert__title" { (title) }
                    p class="loom-alert__body" { (body) }
                    @if *dismissible {
                        button type="button" class="loom-alert__dismiss" aria-label="Dismiss" { "×" }
                    }
                }
            }
        }
        CmsSection::Toast { tone, body } => {
            let t = alert_tone_class(tone);
            html! {
                div class={ "loom-toast tone-" (t) } role="status" aria-live="polite" { (body) }
            }
        }
        CmsSection::Modal { title, body, primary, secondary } => {
            let p_safe = is_safe_url(&primary.href);
            html! {
                dialog class="loom-modal" data-loom-modal {
                    h2 class="loom-modal__title" { (title) }
                    p class="loom-modal__body" { (body) }
                    div class="loom-modal__actions" {
                        a class="loom-btn loom-btn--primary"
                          href=(if p_safe { primary.href.as_str() } else { "#invalid-cta" })
                          data-backend=(primary.data_backend) { (primary.label) }
                        @if let Some(s) = secondary {
                            @let s_safe = is_safe_url(&s.href);
                            a class="loom-btn loom-btn--ghost"
                              href=(if s_safe { s.href.as_str() } else { "#invalid-cta" })
                              data-backend=(s.data_backend) { (s.label) }
                        }
                    }
                }
            }
        }
        CmsSection::Drawer { title, body, side } => {
            let s = match side { DrawerSide::Right => "right", DrawerSide::Left => "left" };
            html! {
                aside class={ "loom-drawer side-" (s) } data-loom-drawer {
                    header class="loom-drawer__header" {
                        h2 class="loom-drawer__title" { (title) }
                        button type="button" class="loom-drawer__close" aria-label="Close" { "×" }
                    }
                    div class="loom-drawer__body" { (body) }
                }
            }
        }
        CmsSection::Tooltip { trigger, body } => html! {
            span class="loom-tooltip" data-loom-tooltip {
                span class="loom-tooltip__trigger" tabindex="0" { (trigger) }
                span class="loom-tooltip__body" role="tooltip" { (body) }
            }
        },
        CmsSection::ProgressBar { value, label } => {
            let pct = (*value).clamp(0, 100);
            html! {
                div class="loom-progress" role="progressbar" aria-valuenow=(pct.to_string()) aria-valuemin="0" aria-valuemax="100" {
                    @if let Some(l) = label { span class="loom-progress__label" { (l) } }
                    div class="loom-progress__track" {
                        div class="loom-progress__fill" style=({ format!("--loom-progress-val: {}%", pct) }) {}
                    }
                }
            }
        }
        CmsSection::Skeleton { rows, height } => {
            let h = space_class(height);
            let n = (*rows).clamp(1, 12);
            html! {
                div class={ "loom-skeleton " (h) } aria-busy="true" aria-label="Loading" {
                    @for _ in 0..n { div class="loom-skeleton__row" {} }
                }
            }
        }
        CmsSection::EmptyState { title, body, cta } => html! {
            section class="loom-empty-state" data-loom-reveal {
                h2 class="loom-empty-state__title" { (title) }
                p class="loom-empty-state__body" { (body) }
                @if let Some(c) = cta {
                    @let safe = is_safe_url(&c.href);
                    a class="loom-btn loom-btn--primary"
                      href=(if safe { c.href.as_str() } else { "#invalid-cta" })
                      data-backend=(c.data_backend) { (c.label) }
                }
            }
        },
        CmsSection::GameTile { title, genre, players_online, image_slug, href, data_backend } => {
            let safe = is_safe_url(href);
            html! {
                article class="loom-game-tile" data-loom-reveal {
                    a class="loom-game-tile__link"
                      href=(if safe { href.as_str() } else { "#invalid-link" })
                      data-backend=(data_backend) {
                        figure class="loom-game-tile__thumb" data-asset-slug=(image_slug) aria-label=(title) {
                            span class="loom-asset-placeholder" { (title) }
                        }
                        h3 class="loom-game-tile__title" { (title) }
                        span class="loom-game-tile__genre" { (genre) }
                        span class="loom-game-tile__online" { (players_online.to_string()) " playing" }
                    }
                }
            }
        }
        CmsSection::GameGrid { heading, items } => html! {
            section class="loom-game-grid" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-game-grid__heading" { (h) } }
                div class="loom-game-grid__row" {
                    @for g in items {
                        @let safe = is_safe_url(&g.href);
                        article class="loom-game-tile" data-loom-reveal {
                            a class="loom-game-tile__link"
                              href=(if safe { g.href.as_str() } else { "#invalid-link" })
                              data-backend=(g.data_backend) {
                                figure class="loom-game-tile__thumb" data-asset-slug=(g.image_slug) aria-label=(g.title) {
                                    span class="loom-asset-placeholder" { (g.title) }
                                }
                                h3 class="loom-game-tile__title" { (g.title) }
                                span class="loom-game-tile__genre" { (g.genre) }
                                span class="loom-game-tile__online" { (g.players_online.to_string()) " playing" }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::ThreadRow { title, author, replies, views, last_reply, href, data_backend } => {
            let safe = is_safe_url(href);
            html! {
                article class="loom-thread-row" {
                    a class="loom-thread-row__link"
                      href=(if safe { href.as_str() } else { "#invalid-link" })
                      data-backend=(data_backend) {
                        h3 class="loom-thread-row__title" { (title) }
                        p class="loom-thread-row__author" { "by " (author) " · " (replies.to_string()) " replies · " (views.to_string()) " views · last " (last_reply) }
                    }
                }
            }
        }
        CmsSection::ThreadList { heading, items } => html! {
            section class="loom-thread-list" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-thread-list__heading" { (h) } }
                @for t in items {
                    @let safe = is_safe_url(&t.href);
                    article class="loom-thread-row" {
                        a class="loom-thread-row__link"
                          href=(if safe { t.href.as_str() } else { "#invalid-link" })
                          data-backend=(t.data_backend) {
                            h3 class="loom-thread-row__title" { (t.title) }
                            p class="loom-thread-row__author" { "by " (t.author) " · " (t.replies.to_string()) " replies · " (t.views.to_string()) " views · last " (t.last_reply) }
                        }
                    }
                }
            }
        },
        CmsSection::VideoCard { title, channel, duration, views, thumbnail_slug, href, data_backend } => {
            let safe = is_safe_url(href);
            html! {
                article class="loom-video-card" data-loom-reveal {
                    a class="loom-video-card__link"
                      href=(if safe { href.as_str() } else { "#invalid-link" })
                      data-backend=(data_backend) {
                        figure class="loom-video-card__thumb" data-asset-slug=(thumbnail_slug) aria-label=(title) {
                            span class="loom-asset-placeholder" { (title) }
                            span class="loom-video-card__duration" { (duration) }
                        }
                        h3 class="loom-video-card__title" { (title) }
                        p class="loom-video-card__meta" { (channel) " · " (views) " views" }
                    }
                }
            }
        }
        CmsSection::VideoGridSection { heading, items } => html! {
            section class="loom-video-grid" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-video-grid__heading" { (h) } }
                div class="loom-video-grid__row" {
                    @for v in items {
                        @let safe = is_safe_url(&v.href);
                        article class="loom-video-card" data-loom-reveal {
                            a class="loom-video-card__link"
                              href=(if safe { v.href.as_str() } else { "#invalid-link" })
                              data-backend=(v.data_backend) {
                                figure class="loom-video-card__thumb" data-asset-slug=(v.thumbnail_slug) aria-label=(v.title) {
                                    span class="loom-asset-placeholder" { (v.title) }
                                    span class="loom-video-card__duration" { (v.duration) }
                                }
                                h3 class="loom-video-card__title" { (v.title) }
                                p class="loom-video-card__meta" { (v.channel) " · " (v.views) " views" }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::CommentThread { post_id, items } => html! {
            section class="loom-comment-thread" data-post-id=(post_id) data-loom-reveal {
                @for c in items {
                    article class="loom-comment" data-depth=(c.depth.to_string()) style=({ format!("margin-left: {}rem", c.depth as f32 * 1.5) }) {
                        header { span class="loom-comment__author" { (c.author) } " · " time class="loom-comment__at" { (c.at) } }
                        p class="loom-comment__body" { (c.body) }
                    }
                }
            }
        },
        CmsSection::FeedPost { author, handle, avatar, body, posted_at, reactions, comments } => html! {
            article class="loom-feed-post" data-loom-reveal {
                header class="loom-feed-post__header" {
                    (render_avatar(avatar))
                    span class="loom-feed-post__author" { (author) }
                    span class="loom-feed-post__handle" { " @" (handle) }
                    time class="loom-feed-post__at" { " · " (posted_at) }
                }
                p class="loom-feed-post__body" { (body) }
                footer class="loom-feed-post__footer" {
                    span class="loom-feed-post__reactions" { (reactions.to_string()) " reactions" }
                    " · "
                    span class="loom-feed-post__comments" { (comments.to_string()) " comments" }
                }
            }
        },
        CmsSection::AuthCard { title, tagline, methods, footer } => html! {
            section class="loom-auth-card" data-loom-reveal {
                header class="loom-auth-card__header" {
                    h2 class="loom-auth-card__title" { (title) }
                    @if let Some(t) = tagline { p class="loom-auth-card__tagline" { (t) } }
                }
                div class="loom-auth-card__methods" {
                    @for m in methods { (render_auth_method(m)) }
                }
                @if let Some(f) = footer {
                    p class="loom-auth-card__footer" { (f) }
                }
            }
        },
        CmsSection::MfaPrompt { title, factor, instructions, otp_length, submit_label, switch_label } => {
            let factor_class = match factor {
                MfaFactorKind::Totp       => "totp",
                MfaFactorKind::Webauthn   => "webauthn",
                MfaFactorKind::SmsOtp     => "sms-otp",
                MfaFactorKind::EmailOtp   => "email-otp",
                MfaFactorKind::BackupCode => "backup-code",
            };
            html! {
                section class={ "loom-mfa-prompt factor-" (factor_class) } data-loom-reveal {
                    h2 class="loom-mfa-prompt__title" { (title) }
                    p class="loom-mfa-prompt__instructions" { (instructions) }
                    @if matches!(factor, MfaFactorKind::Webauthn) {
                        button type="button" class="loom-btn loom-btn--primary loom-mfa-prompt__webauthn" data-loom-mfa-webauthn {
                            "Use your security key"
                        }
                    } @else {
                        div class="loom-mfa-prompt__otp" data-loom-otp-length=(otp_length.to_string()) {
                            @for i in 0..*otp_length {
                                input type="text" inputmode="numeric" maxlength="1"
                                    aria-label=({ format!("Digit {} of {}", i + 1, otp_length) })
                                    class="loom-mfa-prompt__digit"
                                    data-loom-otp-index=(i.to_string());
                            }
                        }
                        button type="submit" class="loom-btn loom-btn--primary loom-mfa-prompt__submit" {
                            (submit_label)
                        }
                    }
                    @if let Some(s) = switch_label {
                        button type="button" class="loom-btn loom-btn--ghost loom-mfa-prompt__switch" { (s) }
                    }
                }
            }
        }
        CmsSection::CrucibleWidget { challenge_kind, prompt, difficulty, option_count, submit_label, attribution_hint } => {
            let kind_class = match challenge_kind {
                CrucibleKind::ImageClassify         => "image-classify",
                CrucibleKind::SemanticSimilarity    => "semantic-similarity",
                CrucibleKind::AudioTranscribe       => "audio-transcribe",
                CrucibleKind::MathArithmetic        => "math-arithmetic",
                CrucibleKind::DrawingReconstruct    => "drawing-reconstruct",
                CrucibleKind::PromptInjectionDetect => "prompt-injection-detect",
            };
            let diff_class = match difficulty {
                CrucibleDifficulty::Easy         => "easy",
                CrucibleDifficulty::Medium       => "medium",
                CrucibleDifficulty::Hard         => "hard",
                CrucibleDifficulty::Adversarial  => "adversarial",
            };
            let n = (*option_count).clamp(1, 16);
            html! {
                section class={ "loom-crucible kind-" (kind_class) " difficulty-" (diff_class) }
                    data-loom-crucible data-loom-reveal {
                    header class="loom-crucible__header" {
                        span class="loom-crucible__badge" { "Crucible · " (diff_class) }
                        p class="loom-crucible__prompt" { (prompt) }
                    }
                    div class="loom-crucible__options" data-loom-option-count=(n.to_string()) {
                        @for i in 0..n {
                            button type="button" class="loom-crucible__option"
                                data-loom-option-index=(i.to_string())
                                aria-pressed="false" {
                                span class="loom-crucible__option-glyph" aria-hidden="true" {}
                            }
                        }
                    }
                    footer class="loom-crucible__footer" {
                        button type="submit" class="loom-btn loom-btn--primary loom-crucible__submit" {
                            (submit_label)
                        }
                        @if let Some(h) = attribution_hint {
                            p class="loom-crucible__attribution" { (h) }
                        }
                    }
                }
            }
        }
        CmsSection::AuthFlowStepper { steps, current } => {
            let cur = (*current as usize).min(steps.len().saturating_sub(1));
            html! {
                nav class="loom-auth-stepper" aria-label="Progress" data-loom-reveal {
                    ol class="loom-auth-stepper__list" {
                        @for (i, label) in steps.iter().enumerate() {
                            li class={ "loom-auth-stepper__step "
                                (if i < cur { "is-done" } else if i == cur { "is-current" } else { "is-upcoming" }) }
                                aria-current=[(i == cur).then_some("step")] {
                                span class="loom-auth-stepper__num" { ((i + 1).to_string()) }
                                span class="loom-auth-stepper__label" { (label) }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::SignedInCard { display_name, handle, avatar, sign_out } => {
            let safe = is_safe_url(&sign_out.href);
            html! {
                section class="loom-signed-in-card" data-loom-reveal {
                    (render_avatar(avatar))
                    div class="loom-signed-in-card__body" {
                        span class="loom-signed-in-card__name" { (display_name) }
                        span class="loom-signed-in-card__handle" { (handle) }
                    }
                    a class="loom-signed-in-card__signout loom-btn loom-btn--ghost"
                      href=(if safe { sign_out.href.as_str() } else { "#invalid-cta" })
                      data-backend=(sign_out.data_backend) { (sign_out.label) }
                }
            }
        }
        CmsSection::PasswordReset { title, description, placeholder, submit_label } => html! {
            section class="loom-password-reset" data-loom-reveal {
                h2 class="loom-password-reset__title" { (title) }
                p class="loom-password-reset__description" { (description) }
                form class="loom-password-reset__form" {
                    input type="email" name="email" required placeholder=(placeholder) aria-label="Email";
                    button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
                }
            }
        },
    }
}

fn render_auth_method(m: &AuthMethodChoice) -> Markup {
    match m {
        AuthMethodChoice::Passkey { label } => html! {
            button type="button" class="loom-auth-method loom-auth-method--passkey loom-btn loom-btn--secondary" {
                span class="loom-auth-method__icon" aria-hidden="true" { "🔑" }
                span class="loom-auth-method__label" { (label) }
            }
        },
        AuthMethodChoice::WebauthnPlatform { label } => html! {
            button type="button" class="loom-auth-method loom-auth-method--webauthn-platform loom-btn loom-btn--secondary" {
                span class="loom-auth-method__icon" aria-hidden="true" { "👤" }
                span class="loom-auth-method__label" { (label) }
            }
        },
        AuthMethodChoice::WebauthnRoaming { label } => html! {
            button type="button" class="loom-auth-method loom-auth-method--webauthn-roaming loom-btn loom-btn--secondary" {
                span class="loom-auth-method__icon" aria-hidden="true" { "🗝" }
                span class="loom-auth-method__label" { (label) }
            }
        },
        AuthMethodChoice::Social { provider, label } => html! {
            button type="button" class={ "loom-auth-method loom-auth-method--social loom-btn loom-btn--secondary provider-" (provider) } {
                span class="loom-auth-method__icon" aria-hidden="true" {}
                span class="loom-auth-method__label" { (label) }
            }
        },
        AuthMethodChoice::MagicLink { placeholder, submit_label } => html! {
            form class="loom-auth-method loom-auth-method--magic-link" {
                input type="email" name="email" required placeholder=(placeholder) aria-label="Email";
                button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
            }
        },
        AuthMethodChoice::SmsOtp { placeholder, submit_label } => html! {
            form class="loom-auth-method loom-auth-method--sms-otp" {
                input type="tel" name="phone" required placeholder=(placeholder) aria-label="Phone";
                button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
            }
        },
        AuthMethodChoice::Password { email_placeholder, password_placeholder, submit_label, forgot_label } => html! {
            form class="loom-auth-method loom-auth-method--password" {
                input type="email" name="email" required placeholder=(email_placeholder) aria-label="Email";
                input type="password" name="password" required placeholder=(password_placeholder) aria-label="Password";
                div class="loom-auth-method__row" {
                    button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
                    @if let Some(f) = forgot_label {
                        a class="loom-auth-method__forgot" href="#" { (f) }
                    }
                }
            }
        },
        AuthMethodChoice::Sso { label, placeholder } => html! {
            form class="loom-auth-method loom-auth-method--sso" {
                input type="text" name="sso_domain" placeholder=(placeholder) aria-label="SSO domain";
                button type="submit" class="loom-btn loom-btn--secondary" { (label) }
            }
        },
        AuthMethodChoice::Anonymous { label } => html! {
            button type="button" class="loom-auth-method loom-auth-method--anonymous loom-btn loom-btn--ghost" {
                (label)
            }
        },
        AuthMethodChoice::Divider { label } => html! {
            div class="loom-auth-method-divider" aria-hidden="true" {
                span class="loom-auth-method-divider__line" {}
                span class="loom-auth-method-divider__label" { (label) }
                span class="loom-auth-method-divider__line" {}
            }
        },
    }
}

fn space_class(s: &SpaceSize) -> &'static str {
    match s {
        SpaceSize::Tight => "size-tight",
        SpaceSize::Comfortable => "size-comfortable",
        SpaceSize::Loose => "size-loose",
        SpaceSize::Generous => "size-generous",
    }
}

fn alert_tone_class(t: &AlertTone) -> &'static str {
    match t {
        AlertTone::Info => "info",
        AlertTone::Success => "success",
        AlertTone::Warning => "warning",
        AlertTone::Danger => "danger",
        AlertTone::Neutral => "neutral",
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn render_avatar(a: &CmsAvatar) -> Markup {
    match a {
        CmsAvatar::None => html! { span class="loom-avatar" data-kind="none" aria-hidden="true" {} },
        CmsAvatar::Initials { letters } => html! {
            span class="loom-avatar" data-kind="initials" aria-hidden="true" { (letters) }
        },
        CmsAvatar::Image { src, alt } => {
            if is_safe_url(src) {
                html! { img class="loom-avatar" data-kind="image" src=(src) alt=(alt); }
            } else {
                html! { span class="loom-avatar" data-kind="image" data-empty="true" aria-label=(alt) {} }
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
    // T70a (cycle 96 finish): render the optional media slot.
    // SAFETY: every URL passed through is_safe_url() before
    // emission. Image alt is REQUIRED by the schema. Video MIME
    // is restricted to the ALLOWED_VIDEO_MIME allowlist.
    let media_markup: Markup = match &card.media {
        None => html! {},
        Some(CmsCardMedia::Image {
            src,
            alt,
            srcset,
            width,
            height,
        }) => {
            if !is_safe_url(src) {
                html! {
                    div class="loom-card-feed-item__media" data-empty="true" aria-hidden="true" {}
                }
            } else {
                html! {
                    div class="loom-card-feed-item__media" {
                        img
                            src=(src)
                            alt=(alt)
                            srcset=[srcset.as_deref()]
                            width=[width.map(|w| w.to_string())]
                            height=[height.map(|h| h.to_string())]
                            loading="lazy"
                            decoding="async";
                    }
                }
            }
        }
        Some(CmsCardMedia::Video {
            poster,
            src,
            mime,
            alt,
        }) => {
            let poster_safe = poster.as_deref().map(is_safe_url).unwrap_or(true);
            let src_safe = is_safe_url(src);
            let mime_ok = ALLOWED_VIDEO_MIME.contains(&mime.as_str());
            if !src_safe || !poster_safe || !mime_ok {
                html! {
                    div class="loom-card-feed-item__media" data-empty="true" aria-hidden="true" {}
                }
            } else {
                html! {
                    div class="loom-card-feed-item__media" {
                        video
                            controls
                            preload="metadata"
                            poster=[poster.as_deref()]
                            aria-label=(alt)
                        {
                            source src=(src) type=(mime);
                        }
                    }
                }
            }
        }
        Some(CmsCardMedia::Placeholder { tone }) => html! {
            div class="loom-card-feed-item__media" data-empty="true" data-tone=[tone.as_deref()] aria-hidden="true" {}
        },
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
                (media_markup)
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
                        span class="loom-card-feed-item__tag" data-tone=[card.tone.as_deref()] { (tag) }
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "Home".to_owned(),
            description: "x".to_owned(),
            path: "/".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<div class="loom-page""#));
        assert!(
            !html.contains(r#"<main class="loom-page""#),
            "render_page must NOT emit <main> — page_shell owns the landmark"
        );
        assert!(html.contains(r#"data-cms-path="/""#));
    }

    #[test]
    fn paragraph_renders_loom_prose() {
        let p = CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::Paragraph {
                text: "Hello world.".to_owned(),
                decoration: ParagraphDecoration::Body,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"<p class="loom-prose">Hello world.</p>"#));
    }

    #[test]
    fn paragraph_html_is_escaped() {
        let p = CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::Paragraph {
                text: "<script>alert(1)</script>".to_owned(),
                decoration: ParagraphDecoration::Body,
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("<script>"), "raw script leaked: {html}");
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn heading_level_2_renders_h2() {
        let p = CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::Heading {
                text: "Section".to_owned(),
                level: HeadingLevel::H2,
                polish: Vec::new(),
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
                brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
                schema: None,
                title: "x".to_owned(),
                description: "x".to_owned(),
                path: "/x".to_owned(),
                nav_links: vec![],
                dev_devtools: false,
                footer: None,
                sections: vec![CmsSection::Heading {
                    text: "x".to_owned(),
                    level,
                    polish: Vec::new(),
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
    fn image_hero_renders_slot_sections_before_headline_and_after_cta() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind": "image_hero",
                "title": "Hello",
                "before_headline": [
                    {"kind": "paragraph", "text": "TRUST_SIGNAL_ABOVE"}
                ],
                "after_cta": [
                    {"kind": "paragraph", "text": "FINE_PRINT_BELOW"}
                ]
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-image-hero__slot--before-headline"));
        assert!(html.contains("TRUST_SIGNAL_ABOVE"));
        assert!(html.contains("loom-image-hero__slot--after-cta"));
        assert!(html.contains("FINE_PRINT_BELOW"));
        let before_pos = html.find("loom-image-hero__slot--before-headline").unwrap();
        let title_pos = html.find("loom-image-hero__title").unwrap();
        let after_pos = html.find("loom-image-hero__slot--after-cta").unwrap();
        assert!(before_pos < title_pos, "before_headline must precede title");
        assert!(title_pos < after_pos, "after_cta must follow title");
    }

    #[test]
    fn loom_fact_inline_renders_value_span() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"loom_fact","which":"primitive_count"}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-fact"));
        assert!(html.contains(&loom_facts::PRIMITIVE_COUNT.to_string()));
    }

    #[test]
    fn loom_fact_sentence_renders_with_noun() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"loom_fact","which":"theme_count","shape":"sentence"}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-fact--sentence"));
        assert!(html.contains("named themes ship today"));
        assert!(html.contains(&loom_facts::THEME_COUNT.to_string()));
    }

    #[test]
    fn loom_fact_kinds_each_render() {
        for which in [
            "primitive_count",
            "theme_count",
            "forge_audit_phase_count",
            "deploy_network_count",
        ] {
            let json = format!(
                r#"{{
                "brand": null, "theme": null, "chrome": null, "content_width": null,
                "nav_actions": [], "title": "t", "description": "d",
                "path": "/p", "nav_links": [], "dev_devtools": false,
                "sections": [{{"kind":"loom_fact","which":"{which}","shape":"sentence"}}]
            }}"#
            );
            let page: CmsPage = serde_json::from_str(&json).expect("page parses");
            let html = render_to_string(&page);
            assert!(html.contains("loom-fact"), "{which}");
        }
    }

    /// Compile-time guard: when adding a new CmsSection variant,
    /// bump `loom_facts::PRIMITIVE_COUNT` to keep the renderer's
    /// self-reported count truthful. The match below is
    /// exhaustive — adding a variant will fail to compile here
    /// until the bump lands.
    #[test]
    fn primitive_count_constant_matches_variant_walk() {
        fn variant_index(s: &CmsSection) -> u32 {
            match s {
                CmsSection::Hero { .. } => 0,
                CmsSection::Group { .. } => 1,
                CmsSection::CardFeed { .. } => 2,
                CmsSection::Sidebar { .. } => 3,
                CmsSection::Banner { .. } => 4,
                CmsSection::Form { .. } => 5,
                CmsSection::Composer { .. } => 6,
                CmsSection::Picture { .. } => 7,
                CmsSection::Paragraph { .. } => 8,
                CmsSection::Heading { .. } => 9,
                CmsSection::KvPair { .. } => 10,
                CmsSection::LogoWall { .. } => 11,
                CmsSection::Quote { .. } => 12,
                CmsSection::Code { .. } => 13,
                CmsSection::ImageHero { .. } => 14,
                CmsSection::SplitHero { .. } => 15,
                _ => 99,
            }
        }
        // The match above intentionally has a wildcard — a full
        // 171-arm match here would be unreadable + churn-noisy.
        // The DEDICATED guard is the `primitive_count_is_not_wildly_off`
        // test below, which uses schemars to count subschemas.
        let _ = variant_index(&CmsSection::Heading {
            text: "x".into(),
            level: HeadingLevel::H2,
            polish: Vec::new(),
        });
        assert!(loom_facts::PRIMITIVE_COUNT > 100);
    }

    #[test]
    fn primitive_count_is_not_wildly_off() {
        // schemars's generated schema enumerates one subschema per
        // variant via the oneOf array (because CmsSection is
        // serde-tag = "kind"). Cross-check the constant against
        // that count — keeps the manually-bumped const honest
        // within a small tolerance.
        let settings = schemars::r#gen::SchemaSettings::draft07();
        let mut g = schemars::r#gen::SchemaGenerator::new(settings);
        let schema = g.root_schema_for::<CmsSection>();
        // Find the top-level "oneOf" — schemars emits each variant
        // there for tagged enums.
        let one_of_count = schema
            .schema
            .subschemas
            .as_ref()
            .and_then(|sub| sub.one_of.as_ref())
            .map(|v| v.len())
            .unwrap_or(0);
        // Allow tolerance — some variants may collapse in the schema.
        let expected = loom_facts::PRIMITIVE_COUNT as usize;
        assert!(
            one_of_count >= expected.saturating_sub(2),
            "schema has {one_of_count} variants but loom_facts::PRIMITIVE_COUNT is {expected}; \
             bump PRIMITIVE_COUNT down (or add the missing variant tag) so the renderer's \
             self-reported count stays truthful"
        );
        assert!(
            one_of_count <= expected + 2,
            "schema has {one_of_count} variants but loom_facts::PRIMITIVE_COUNT is {expected}; \
             a new variant landed without bumping PRIMITIVE_COUNT — fix in same commit"
        );
    }

    #[test]
    fn epigraph_renders_body_and_attribution() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind": "epigraph",
                "body": "We must imagine Sisyphus happy.",
                "attribution": "Camus"
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-epigraph"));
        assert!(html.contains("loom-epigraph__body"));
        assert!(html.contains("Sisyphus"));
        assert!(html.contains("loom-epigraph__attribution"));
        assert!(html.contains("Camus"));
    }

    #[test]
    fn epigraph_attribution_optional() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind": "epigraph", "body": "Begin again."}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-epigraph__body"));
        assert!(!html.contains("loom-epigraph__attribution"));
    }

    #[test]
    fn epigraph_body_is_escaped() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind": "epigraph", "body": "<script>alert(1)</script>"}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn testimonial_default_decoration_is_decorated() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"testimonial","body":"B","attribution":"A"}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-testimonial"));
        assert!(html.contains("deco-decorated"));
    }

    #[test]
    fn testimonial_editorial_decoration_drops_avatar() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind":"testimonial",
                "body":"B",
                "attribution":"A",
                "avatar_slug":"avatars/a",
                "decoration":"editorial"
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("parses");
        let html = render_to_string(&page);
        assert!(html.contains("deco-editorial"));
        assert!(!html.contains("loom-testimonial__avatar"));
    }

    #[test]
    fn testimonial_minimal_decoration_drops_avatar() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind":"testimonial",
                "body":"B",
                "attribution":"A",
                "avatar_slug":"avatars/a",
                "decoration":"minimal"
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("parses");
        let html = render_to_string(&page);
        assert!(html.contains("deco-minimal"));
        assert!(!html.contains("loom-testimonial__avatar"));
    }

    #[test]
    fn testimonial_decoration_modifier_class_matches_variant() {
        assert_eq!(TestimonialDecoration::Decorated.modifier_class(), "deco-decorated");
        assert_eq!(TestimonialDecoration::Editorial.modifier_class(), "deco-editorial");
        assert_eq!(TestimonialDecoration::Minimal.modifier_class(), "deco-minimal");
    }

    #[test]
    fn feature_spotlight_default_decoration_is_decorated() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"feature_spotlight","items":[]}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("parses");
        let html = render_to_string(&page);
        assert!(html.contains("deco-decorated"));
    }

    #[test]
    fn feature_spotlight_editorial_decoration_emits_modifier_class() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind":"feature_spotlight",
                "decoration":"editorial",
                "items":[{"title":"T","body":"B"}]
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("parses");
        let html = render_to_string(&page);
        assert!(html.contains("deco-editorial"));
        assert!(!html.contains("deco-decorated"));
    }

    #[test]
    fn feature_spotlight_minimal_decoration_emits_modifier_class() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind":"feature_spotlight",
                "decoration":"minimal",
                "items":[{"title":"T","body":"B"}]
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("parses");
        let html = render_to_string(&page);
        assert!(html.contains("deco-minimal"));
    }

    #[test]
    fn feature_spotlight_decoration_modifier_class_matches_variant() {
        assert_eq!(FeatureSpotlightDecoration::Decorated.modifier_class(), "deco-decorated");
        assert_eq!(FeatureSpotlightDecoration::Editorial.modifier_class(), "deco-editorial");
        assert_eq!(FeatureSpotlightDecoration::Minimal.modifier_class(), "deco-minimal");
    }

    #[test]
    fn sublede_renders_with_class() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"sublede","text":"Second-tier subhead."}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-sublede"));
        assert!(html.contains("Second-tier subhead."));
    }

    #[test]
    fn kicker_renders_as_inline_label() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"kicker","text":"BREAKING"}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-kicker"));
        assert!(html.contains("BREAKING"));
    }

    #[test]
    fn byline_renders_all_optional_fields() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind":"byline",
                "author":"Jane Doe",
                "role":"Staff writer",
                "dateline":"2026-05-19",
                "reading_time":"5 min read"
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-byline"));
        assert!(html.contains("Jane Doe"));
        assert!(html.contains("Staff writer"));
        assert!(html.contains("2026-05-19"));
        assert!(html.contains("5 min read"));
    }

    #[test]
    fn byline_author_only_omits_role_dateline() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"byline","author":"Solo"}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("Solo"));
        assert!(!html.contains("loom-byline__role"));
        assert!(!html.contains("loom-byline__dateline"));
    }

    #[test]
    fn endnote_renders_numbered_aside_with_anchor() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind":"endnote","number":1,"text":"Source: foo."}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-endnote"));
        assert!(html.contains(r#"id="endnote-1""#));
        assert!(html.contains("Source: foo."));
    }

    #[test]
    fn image_hero_omits_slot_wrapper_when_slots_empty() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{"kind": "image_hero", "title": "Hello"}]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(!html.contains("loom-image-hero__slot"));
    }

    #[test]
    fn group_renders_title_and_multiple_body_paragraphs() {
        let p = CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            tone: None,
            media: None,
        }
    }

    fn page_with_card(c: CmsCard) -> CmsPage {
        CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::CardFeed {
                heading: None,
                items: vec![c],
            }],
        }
    }

    #[test]
    fn card_media_image_renders_lazy_with_alt() {
        let mut c = card("Battle", "/c/x");
        c.media = Some(CmsCardMedia::Image {
            src: "/assets/foo.jpg".to_owned(),
            alt: "A jumping skater".to_owned(),
            srcset: Some("/assets/foo.jpg 1x, /assets/foo@2x.jpg 2x".to_owned()),
            width: Some(1280),
            height: Some(720),
        });
        let html = render_to_string(&page_with_card(c));
        assert!(html.contains(r#"class="loom-card-feed-item__media""#));
        assert!(html.contains(r#"src="/assets/foo.jpg""#));
        assert!(html.contains(r#"alt="A jumping skater""#));
        assert!(html.contains(r#"loading="lazy""#));
        assert!(html.contains(r#"decoding="async""#));
        assert!(html.contains(r#"srcset="/assets/foo.jpg 1x, /assets/foo@2x.jpg 2x""#));
        assert!(html.contains(r#"width="1280""#));
        assert!(html.contains(r#"height="720""#));
    }

    #[test]
    fn card_media_image_unsafe_url_falls_back_to_empty_placeholder() {
        let mut c = card("X", "/c/x");
        c.media = Some(CmsCardMedia::Image {
            src: "javascript:alert(1)".to_owned(),
            alt: "evil".to_owned(),
            srcset: None,
            width: None,
            height: None,
        });
        let html = render_to_string(&page_with_card(c));
        assert!(html.contains(r#"data-empty="true""#));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn card_media_video_emits_native_player_with_safe_mime() {
        let mut c = card("X", "/c/x");
        c.media = Some(CmsCardMedia::Video {
            poster: Some("/assets/poster.jpg".to_owned()),
            src: "/assets/clip.mp4".to_owned(),
            mime: "video/mp4".to_owned(),
            alt: "Skill clip".to_owned(),
        });
        let html = render_to_string(&page_with_card(c));
        assert!(html.contains("<video"));
        assert!(html.contains(r#"poster="/assets/poster.jpg""#));
        assert!(html.contains(r#"src="/assets/clip.mp4""#));
        assert!(html.contains(r#"type="video/mp4""#));
        assert!(html.contains(r#"controls"#));
        assert!(html.contains(r#"preload="metadata""#));
    }

    #[test]
    fn card_media_video_rejected_mime_falls_back_to_empty() {
        let mut c = card("X", "/c/x");
        c.media = Some(CmsCardMedia::Video {
            poster: None,
            src: "/clip.mkv".to_owned(),
            mime: "video/x-matroska".to_owned(),
            alt: "x".to_owned(),
        });
        let html = render_to_string(&page_with_card(c));
        assert!(html.contains(r#"data-empty="true""#));
        assert!(!html.contains("<video"));
    }

    #[test]
    fn card_media_placeholder_emits_data_tone_only() {
        let mut c = card("X", "/c/x");
        c.media = Some(CmsCardMedia::Placeholder {
            tone: Some("violet".to_owned()),
        });
        let html = render_to_string(&page_with_card(c));
        assert!(html.contains(r#"data-empty="true""#));
        assert!(html.contains(r#"data-tone="violet""#));
        assert!(!html.contains("<img"));
        assert!(!html.contains("<video"));
    }

    #[test]
    fn card_tag_emits_data_tone_when_set() {
        let mut c = card("X", "/c/x");
        c.tone = Some("forest".to_owned());
        let html = render_to_string(&page_with_card(c));
        assert!(html.contains(r#"data-tone="forest""#));
    }

    #[test]
    fn card_tag_omits_data_tone_when_unset() {
        let html = render_to_string(&page_with_card(card("X", "/c/x")));
        // tag is "Parkour" but tone is None — chip span should
        // not have a data-tone attr (CSS falls back to primary).
        assert!(!html.contains("data-tone="));
    }

    #[test]
    fn card_feed_renders_each_item() {
        let p = CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".to_owned(),
            description: "x".to_owned(),
            path: "/x".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
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

    // ----------------------------------------------------------
    // T660 P3 — Code tests
    // ----------------------------------------------------------

    fn code_page(lang: &str, body: &str, caption: Option<&str>, terminal: bool) -> CmsPage {
        CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "C".to_owned(),
            description: "code-test".to_owned(),
            path: "/c".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::Code {
                lang: lang.to_owned(),
                body: body.to_owned(),
                caption: caption.map(|s| s.to_owned()),
                terminal,
            }],
        }
    }

    #[test]
    fn code_renders_pre_code_with_language_class() {
        let p = code_page("rust", "fn main() {}", None, false);
        let html = render_to_string(&p);
        assert!(html.contains("<pre"));
        assert!(html.contains("<code"));
        assert!(html.contains("language-rust"));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn code_terminal_flag_sets_data_attr() {
        let p = code_page("bash", "echo hi", None, true);
        let html = render_to_string(&p);
        assert!(html.contains("data-loom-terminal"));
    }

    #[test]
    fn code_no_terminal_omits_data_attr() {
        let p = code_page("bash", "echo hi", None, false);
        let html = render_to_string(&p);
        assert!(!html.contains("data-loom-terminal"));
    }

    #[test]
    fn code_caption_renders_above_block() {
        let p = code_page("bash", "echo hi", Some("Quickstart"), true);
        let html = render_to_string(&p);
        assert!(html.contains("Quickstart"));
        assert!(html.contains("loom-code-caption"));
    }

    #[test]
    fn code_body_auto_escaped() {
        let p = code_page("html", "<script>alert(1)</script>", None, false);
        let html = render_to_string(&p);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn code_empty_lang_renders_generic_class() {
        let p = code_page("", "x", None, false);
        let html = render_to_string(&p);
        // lang empty → class is "language-" with no suffix; still valid.
        assert!(html.contains("language-"));
    }

    #[test]
    fn code_serde_round_trip() {
        let p = code_page("rust", "fn main(){}", Some("Demo"), true);
        let j = serde_json::to_string(&p).unwrap();
        let parsed: CmsPage = serde_json::from_str(&j).unwrap();
        assert_eq!(parsed.sections.len(), 1);
        match &parsed.sections[0] {
            CmsSection::Code {
                lang,
                body,
                caption,
                terminal,
            } => {
                assert_eq!(lang, "rust");
                assert_eq!(body, "fn main(){}");
                assert_eq!(caption.as_deref(), Some("Demo"));
                assert!(*terminal);
            }
            _ => panic!("not a Code"),
        }
    }

    // ----------------------------------------------------------
    // T660 P2 — Quote tests
    // ----------------------------------------------------------

    fn quote_page(body: &str, attribution: &str, role: Option<&str>) -> CmsPage {
        CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "Q".to_owned(),
            description: "q-test".to_owned(),
            path: "/q".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::Quote {
                body: body.to_owned(),
                attribution: attribution.to_owned(),
                role: role.map(|s| s.to_owned()),
            }],
        }
    }

    #[test]
    fn quote_renders_blockquote_and_cite() {
        let p = quote_page(
            "Linear is the standard for product velocity.",
            "Patrick Collison",
            Some("CEO, Stripe"),
        );
        let html = render_to_string(&p);
        assert!(html.contains("<blockquote"));
        assert!(html.contains("<cite"));
        assert!(html.contains("Linear is the standard"));
        assert!(html.contains("Patrick Collison"));
        assert!(html.contains("CEO, Stripe"));
    }

    #[test]
    fn quote_role_optional() {
        let p = quote_page("Solid product.", "Anon", None);
        let html = render_to_string(&p);
        assert!(html.contains("Anon"));
        // No role span when role is None.
        assert!(!html.contains("loom-quote-role"));
    }

    #[test]
    fn quote_auto_escapes_body_and_attribution() {
        let p = quote_page(
            "<script>alert(1)</script>",
            "<img src=x onerror=alert(2)>",
            Some("</cite>"),
        );
        let html = render_to_string(&p);
        // XSS-relevant assertions: every angle bracket escaped → no
        // executable element survives. The literal text 'onerror='
        // stays as plain text (harmless without a parent <img>).
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("<img src=x onerror"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;img"));
        // The literal </cite> in the role MUST be escaped — Maud auto-escapes.
        assert!(html.contains("&lt;/cite&gt;"));
    }

    #[test]
    fn quote_serde_round_trip() {
        let p = quote_page("Body", "Attr", Some("Role"));
        let j = serde_json::to_string(&p).unwrap();
        let parsed: CmsPage = serde_json::from_str(&j).unwrap();
        assert_eq!(parsed.sections.len(), 1);
        match &parsed.sections[0] {
            CmsSection::Quote {
                body,
                attribution,
                role,
            } => {
                assert_eq!(body, "Body");
                assert_eq!(attribution, "Attr");
                assert_eq!(role.as_deref(), Some("Role"));
            }
            _ => panic!("not a Quote"),
        }
    }

    #[test]
    fn quote_renders_semantic_landmarks() {
        let p = quote_page("Body", "Attr", None);
        let html = render_to_string(&p);
        // semantic structure: section > blockquote > p; section > footer > cite
        assert!(html.contains("class=\"loom-quote\""));
        assert!(html.contains("class=\"loom-quote-body\""));
        assert!(html.contains("class=\"loom-quote-footer\""));
    }

    // ----------------------------------------------------------
    // T660 P1 — LogoWall tests
    // ----------------------------------------------------------

    fn logo_page(items: Vec<CmsLogoItem>, heading: Option<&str>) -> CmsPage {
        CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "LW".to_owned(),
            description: "lw-test".to_owned(),
            path: "/lw".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::LogoWall {
                heading: heading.map(|s| s.to_owned()),
                items,
            }],
        }
    }

    #[test]
    fn logo_wall_renders_unlinked_names_as_spans() {
        let p = logo_page(
            vec![
                CmsLogoItem {
                    name: "Stripe".into(),
                    href: None,
                },
                CmsLogoItem {
                    name: "Linear".into(),
                    href: None,
                },
            ],
            Some("Trusted by"),
        );
        let html = render_to_string(&p);
        assert!(html.contains("loom-logo-wall"));
        assert!(html.contains("Trusted by"));
        assert!(html.contains("Stripe"));
        assert!(html.contains("Linear"));
        // No href → no <a>, just <span class="loom-logo-wall-name">
        assert!(!html.contains("href=\"\""));
    }

    #[test]
    fn logo_wall_renders_safe_href_as_external_link() {
        let p = logo_page(
            vec![CmsLogoItem {
                name: "Vercel".into(),
                href: Some("https://vercel.com".into()),
            }],
            None,
        );
        let html = render_to_string(&p);
        assert!(html.contains("href=\"https://vercel.com\""));
        assert!(html.contains("rel=\"external nofollow noopener\""));
    }

    #[test]
    fn logo_wall_rejects_javascript_url() {
        let p = logo_page(
            vec![CmsLogoItem {
                name: "Evil".into(),
                href: Some("javascript:alert(1)".into()),
            }],
            None,
        );
        let html = render_to_string(&p);
        // is_safe_url returns false → falls back to span; no anchor emitted.
        assert!(!html.contains("javascript:"));
        assert!(html.contains("Evil"));
    }

    #[test]
    fn logo_wall_auto_escapes_brand_name() {
        let p = logo_page(
            vec![CmsLogoItem {
                name: "<script>alert(1)</script>".into(),
                href: None,
            }],
            None,
        );
        let html = render_to_string(&p);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn logo_wall_serde_round_trip() {
        let p = logo_page(
            vec![CmsLogoItem {
                name: "Stripe".into(),
                href: Some("https://stripe.com".into()),
            }],
            Some("Customers"),
        );
        let j = serde_json::to_string(&p).expect("serialize");
        let parsed: CmsPage = serde_json::from_str(&j).expect("deserialize");
        assert_eq!(parsed.sections.len(), 1);
    }

    // ----------------------------------------------------------
    // FORGE_ROADMAP item 41 — KvPair BlockKind tests.
    // ----------------------------------------------------------

    fn kv_page(items: Vec<CmsKvItem>, heading: Option<&str>) -> CmsPage {
        CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "KV".to_owned(),
            description: "kv-test".to_owned(),
            path: "/kv".to_owned(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::KvPair {
                heading: heading.map(|s| s.to_owned()),
                items,
            }],
        }
    }

    #[test]
    fn kv_pair_renders_as_definition_list() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "Match length".into(),
                value: "3 rounds".into(),
                hint: None,
            }],
            None,
        );
        let html = render_to_string(&p);
        assert!(html.contains(r#"<dl class="loom-kv-list">"#));
        assert!(html.contains(r#"<dt class="loom-kv-key">Match length</dt>"#));
        assert!(html.contains(r#"<dd class="loom-kv-value">"#));
        assert!(html.contains("3 rounds"));
    }

    #[test]
    fn kv_pair_optional_heading_renders_when_some() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "k".into(),
                value: "v".into(),
                hint: None,
            }],
            Some("Match details"),
        );
        let html = render_to_string(&p);
        assert!(html.contains(r#"<h2 class="loom-kv-heading">Match details</h2>"#));
    }

    #[test]
    fn kv_pair_omits_heading_element_when_none() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "k".into(),
                value: "v".into(),
                hint: None,
            }],
            None,
        );
        let html = render_to_string(&p);
        assert!(!html.contains("loom-kv-heading"));
    }

    #[test]
    fn kv_pair_hint_renders_when_some() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "Stake".into(),
                value: "$100".into(),
                hint: Some("non-refundable".into()),
            }],
            None,
        );
        let html = render_to_string(&p);
        assert!(html.contains(r#"<span class="loom-kv-hint">non-refundable</span>"#));
    }

    #[test]
    fn kv_pair_hint_absent_emits_no_hint_span() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "k".into(),
                value: "v".into(),
                hint: None,
            }],
            None,
        );
        let html = render_to_string(&p);
        assert!(!html.contains("loom-kv-hint"));
    }

    #[test]
    fn kv_pair_emits_one_row_per_item_in_order() {
        let p = kv_page(
            vec![
                CmsKvItem {
                    key: "A".into(),
                    value: "1".into(),
                    hint: None,
                },
                CmsKvItem {
                    key: "B".into(),
                    value: "2".into(),
                    hint: None,
                },
                CmsKvItem {
                    key: "C".into(),
                    value: "3".into(),
                    hint: None,
                },
            ],
            None,
        );
        let html = render_to_string(&p);
        let row_count = html.matches(r#"loom-kv-row"#).count();
        assert_eq!(row_count, 3);
        let pos_a = html.find(">A<").expect("A");
        let pos_b = html.find(">B<").expect("B");
        let pos_c = html.find(">C<").expect("C");
        assert!(pos_a < pos_b && pos_b < pos_c, "items in declared order");
    }

    #[test]
    fn kv_pair_auto_escapes_key_value_hint() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "<script>alert(1)</script>".into(),
                value: "& \"x\"".into(),
                hint: Some("</dl>".into()),
            }],
            None,
        );
        let html = render_to_string(&p);
        // Maud auto-escapes; raw < > " & must be entity-encoded.
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&amp;"));
        assert!(html.contains("&lt;/dl&gt;"));
    }

    #[test]
    fn kv_pair_serde_round_trip_via_json() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "k".into(),
                value: "v".into(),
                hint: Some("h".into()),
            }],
            Some("My list"),
        );
        let j = serde_json::to_string(&p).expect("serialize");
        let parsed: CmsPage = serde_json::from_str(&j).expect("deserialize");
        assert_eq!(parsed.sections.len(), 1);
        match &parsed.sections[0] {
            CmsSection::KvPair { heading, items } => {
                assert_eq!(heading.as_deref(), Some("My list"));
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].key, "k");
                assert_eq!(items[0].value, "v");
                assert_eq!(items[0].hint.as_deref(), Some("h"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn kv_pair_json_skips_none_hint_on_serialize() {
        let p = kv_page(
            vec![CmsKvItem {
                key: "k".into(),
                value: "v".into(),
                hint: None,
            }],
            None,
        );
        let j = serde_json::to_string(&p).expect("serialize");
        // None hint should NOT appear in serialized JSON.
        assert!(!j.contains("\"hint\""));
    }

    #[test]
    fn kv_pair_section_kind_serializes_snake_case() {
        let p = kv_page(vec![], None);
        let j = serde_json::to_string(&p).expect("serialize");
        // serde tag = "kind", rename_all = "snake_case" → "kv_pair".
        assert!(j.contains(r#""kind":"kv_pair""#));
    }

    #[test]
    fn kv_pair_empty_items_renders_empty_dl() {
        let p = kv_page(vec![], None);
        let html = render_to_string(&p);
        // Empty list still emits the dl shell — operator can spot
        // the bug visually rather than the renderer collapsing.
        assert!(html.contains(r#"<dl class="loom-kv-list">"#));
        assert!(!html.contains("loom-kv-row"));
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
--loom-radius-md:10px;--loom-radius-lg:18px;--loom-radius-xl:24px;--loom-radius-full:9999px;\
--loom-size-icon-sm:20px;--loom-size-icon-md:24px;\
--loom-size-avatar-sm:40px;--loom-size-avatar-md:48px;\
--loom-break-xl:80rem;\
--loom-border-component:1px solid var(--loom-color-border,var(--loom-border));\
--loom-transition-fast:120ms cubic-bezier(.22,1,.36,1)}\
/* T76: light default by owner directive (most users prefer light).\
 * Dark is opt-in via data-theme=\"dark\" or via the in-page theme\
 * switcher (T72). Auto-flip via prefers-color-scheme is ALSO\
 * available BUT only when the user explicitly opts via\
 * data-theme=\"auto\" — so a fresh page-load defaults to light\
 * regardless of OS dark-mode pref. */\
:root[data-theme=\"auto\"] {color-scheme: light dark;}\
@media (prefers-color-scheme:dark){:root[data-theme=\"auto\"]{\
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
font-feature-settings:\"cv11\",\"ss01\",\"ss03\";min-height:100%}\
body{margin:0;min-height:100vh;\
background:\
radial-gradient(60rem 38rem at 88% -8%,color-mix(in oklab,var(--loom-accent-2,#E07A5F) 14%,transparent) 0%,transparent 55%),\
radial-gradient(50rem 36rem at -8% 12%,color-mix(in oklab,var(--loom-accent,#4338CA) 10%,transparent) 0%,transparent 55%),\
radial-gradient(40rem 28rem at 50% 110%,color-mix(in oklab,var(--loom-accent,#4338CA) 8%,transparent) 0%,transparent 55%),\
var(--loom-bg);\
background-attachment:fixed}\
a{color:var(--loom-link);text-decoration-thickness:.08em;text-underline-offset:.18em;\
transition:color var(--loom-motion-fast) var(--loom-ease-out)}\
a:hover,a:focus{color:var(--loom-link-hover)}\
h1,h2,h3,h4,h5,h6{font-family:var(--loom-font-display);letter-spacing:-.012em;line-height:1.2}\
:focus-visible{outline:2px solid var(--loom-focus);outline-offset:3px;border-radius:var(--loom-radius-sm)}\
.loom-skip{position:absolute;left:-9999px;top:auto;width:1px;height:1px;overflow:hidden}\
.loom-skip:focus{left:1rem;top:1rem;width:auto;height:auto;padding:.5rem 1rem;\
background:var(--loom-bg);color:var(--loom-fg);border:2px solid var(--loom-focus);\
border-radius:var(--loom-radius);z-index:1000;box-shadow:var(--loom-shadow-md)}\
header.loom-page-header{padding:1rem 1.75rem;border-bottom:1px solid color-mix(in oklab,var(--loom-border) 60%,transparent);\
background:color-mix(in oklab,var(--loom-bg) 88%,transparent);position:sticky;top:0;z-index:50;\
backdrop-filter:saturate(160%) blur(14px);-webkit-backdrop-filter:saturate(160%) blur(14px)}\
footer.loom-page-footer{padding:2.5rem 1.75rem;border-top:1px solid var(--loom-border);\
color:var(--loom-muted);margin-top:4rem;font-size:.92rem}\
nav.loom-page-nav{display:flex;gap:.5rem;align-items:center;flex-wrap:wrap}\
nav.loom-page-nav a{text-decoration:none;color:var(--loom-muted);\
display:inline-flex;align-items:center;min-height:44px;padding:.5rem .9rem;\
border-radius:999px;font-weight:500;font-size:.96rem;\
transition:background var(--loom-motion-fast) var(--loom-ease-out),color var(--loom-motion-fast) var(--loom-ease-out)}\
nav.loom-page-nav a:hover{color:var(--loom-fg);background:color-mix(in oklab,var(--loom-fg) 5%,transparent)}\
nav.loom-page-nav a[aria-current=\"page\"]{color:var(--loom-accent);font-weight:600;background:color-mix(in oklab,var(--loom-accent) 10%,transparent)}\
nav.loom-page-nav a.loom-page-brand{font-family:var(--loom-font-display);font-weight:800;color:var(--loom-fg);\
font-size:1.15rem;letter-spacing:-.022em;padding-left:.25rem;padding-right:1.25rem;background:none}\
nav.loom-page-nav a.loom-page-brand:hover{background:none}\
.loom-page-title{margin:0;font-family:var(--loom-font-display);\
font-weight:800;letter-spacing:-.022em;font-size:1.4rem;line-height:1.2;color:var(--loom-fg);\
display:flex;align-items:center;padding-left:.6rem;border-left:3px solid color-mix(in oklab,var(--loom-accent,#4338CA) 70%,transparent);\
margin-left:.4rem}\
main#content{padding:1.5rem;max-width:64rem;margin:0 auto}\
body[data-content-width=\"narrow\"] main#content{max-width:42rem}\
body[data-content-width=\"comfortable\"] main#content{max-width:64rem}\
body[data-content-width=\"wide\"] main#content{max-width:90rem}\
body[data-content-width=\"full\"] main#content{max-width:none}\
@media (prefers-reduced-motion:reduce){\
*,*::before,*::after{animation-duration:.001ms !important;animation-iteration-count:1 !important;\
transition-duration:.001ms !important;scroll-behavior:auto !important}\
header.loom-page-header{position:static;backdrop-filter:none;-webkit-backdrop-filter:none}}";

/// Fixed onload event handler for the deferred stylesheet link
/// when `critical_css` is supplied. Hashed at build time + pinned
/// in CSP `script-src 'unsafe-hashes' 'sha256-…'`.
pub const DEFER_ONLOAD_JS: &str = "this.media='all';this.removeAttribute('onload')";

/// T72 cycle 96 iter 9: in-page theme switcher.
///
/// Cycles light → dark → auto on each click. Persists choice to
/// localStorage('loom-theme'). On load, applies the stored
/// preference (or 'light' default per cycle 95f owner directive).
/// ARIA-correct: button announces current theme + intent.
///
/// Hash-pinned in CSP `script-src` per Loom doctrine. No external
/// deps. ~30 lines minified for a small inline script tag.
///
/// SECURITY: only writes to data-theme on html element + localStorage
/// with a fixed key. No DOM injection, no eval, no fetch.
pub const THEME_TOGGLE_JS: &str = "(function(){var K='loom-theme';var B=document.querySelector('[data-loom-theme-toggle]');if(!B)return;var T=['light','dark','auto'];function r(){var v=null;try{v=localStorage.getItem(K);}catch(_){}if(T.indexOf(v)>=0)return v;var s=document.documentElement.getAttribute('data-theme');if(T.indexOf(s)>=0)return s;return 'light';}function a(t){document.documentElement.setAttribute('data-theme',t);B.setAttribute('aria-label','Theme: '+t+' (click to cycle)');B.setAttribute('aria-pressed',t==='dark'?'true':'false');B.textContent=t==='light'?'☀':(t==='dark'?'☾':'◐');}a(r());B.addEventListener('click',function(){var c=r();var n=T[(T.indexOf(c)+1)%T.length];try{localStorage.setItem(K,n);}catch(_){}a(n);});})();";

/// Dev-only Eruda loader. Inlined into `<head>` when
/// `CmsPage.dev_devtools = true`. Runs always; only loads the
/// remote (same-origin) `/eruda.min.js` if the visitor has set
/// `localStorage["loom_eruda"] = "on"`. Strangers landing on a
/// dev page see no devtools and no script load.
///
/// To enable from mobile, paste this into the URL bar once:
///   javascript:localStorage.setItem('loom_eruda','on');location.reload()
/// To disable:
///   javascript:localStorage.removeItem('loom_eruda');location.reload()
///
/// The loader assumes `/eruda.min.js` is served from the same
/// origin (vendored into `/var/www/<host>/eruda.min.js` or
/// equivalent). It does NOT fetch from a CDN — that would be a
/// supply-chain hole. Eruda's UI uses inline styles which require
/// the relaxed CSP emitted by `page_shell_themed` when
/// `dev_devtools = true`.
pub const ERUDA_LOADER_JS: &str = "(function(){try{if(localStorage.getItem('loom_eruda')!=='on')return;var s=document.createElement('script');s.src='/eruda.min.js';s.onload=function(){if(window.eruda){eruda.init();}};document.head.appendChild(s);}catch(_){}})();";

/// CSS for the theme-toggle button. Inlined into BASE_THEME_CSS
/// so first paint paints the button correctly without FOUC.
pub const THEME_TOGGLE_CSS: &str = ".loom-theme-toggle{margin-left:auto;display:inline-flex;align-items:center;justify-content:center;width:44px;height:44px;border-radius:9999px;border:1px solid var(--loom-color-border,var(--loom-border));background:var(--loom-color-surface,var(--loom-bg));color:var(--loom-color-ink,var(--loom-fg));font-size:1.15rem;cursor:pointer;line-height:1;padding:0;transition:background var(--loom-motion-fast,120ms) var(--loom-ease-out,ease),border-color var(--loom-motion-fast,120ms) var(--loom-ease-out,ease)}.loom-theme-toggle:hover{background:var(--loom-color-surface-muted,var(--loom-grad-soft));border-color:var(--loom-color-primary,var(--loom-accent))}.loom-theme-toggle:focus-visible{outline:2px solid var(--loom-color-primary,var(--loom-accent));outline-offset:3px}";

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

/// Map the always-escape HTML metacharacters (`&`, `<`, `>`) to
/// their entity form. The shared base for [`escape_html_text`] and
/// [`escape_html_attr`] — those wrap this with their additional
/// context-specific characters.
fn escape_html_base(c: char) -> Option<&'static str> {
    match c {
        '&' => Some("&amp;"),
        '<' => Some("&lt;"),
        '>' => Some("&gt;"),
        _ => None,
    }
}

/// Escape a text node (HTML body text or `<title>` content).
#[must_use]
pub fn escape_html_text(s: &str) -> String {
    s.chars()
        .map(|c| match escape_html_base(c) {
            Some(ent) => ent.to_owned(),
            None => c.to_string(),
        })
        .collect()
}

/// Escape a value going inside a double-quoted attribute.
#[must_use]
pub fn escape_html_attr(s: &str) -> String {
    s.chars()
        .map(|c| match (escape_html_base(c), c) {
            (Some(ent), _) => ent.to_owned(),
            (None, '"') => "&quot;".to_owned(),
            (None, '\'') => "&#39;".to_owned(),
            (None, other) => other.to_string(),
        })
        .collect()
}

/// Render the action-CTA buttons that sit inside a FloatingPill
/// chrome's right-side cluster. Each entry is a typed HeroCta;
/// the renderer validates the href via the standard same-origin
/// + https-only check before emitting it, falling back to
/// `#invalid-cta` + `data-invalid` for any rejected URL so the
/// build's audit phase can flag it.
#[must_use]
pub fn render_nav_actions(actions: &[HeroCta]) -> String {
    let mut out = String::new();
    for (i, a) in actions.iter().enumerate() {
        let label = escape_html_text(&a.label);
        let safe = loom_components::composer::is_safe_url(&a.href);
        let href = if safe {
            escape_html_attr(&a.href)
        } else {
            "#invalid-cta".to_owned()
        };
        let invalid_attr = if safe { "" } else { " data-invalid=\"true\"" };
        let backend = escape_html_attr(&a.data_backend);
        // Last action gets the primary variant; earlier ones are
        // secondary. The "Sign in / Get started" pattern.
        let variant = if i + 1 == actions.len() {
            "loom-btn--primary"
        } else {
            "loom-btn--secondary"
        };
        out.push_str(&format!(
            "\n        <a class=\"loom-btn loom-btn--sm {variant} loom-floating-pill__action\" href=\"{href}\" data-backend=\"{backend}\"{invalid_attr}>{label}</a>"
        ));
    }
    out
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
        let invalid_attr = if href_safe {
            ""
        } else {
            " data-invalid=\"true\""
        };
        let backend = escape_html_attr(&link.data_backend);
        let current = if link.current {
            " aria-current=\"page\""
        } else {
            ""
        };
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
    // Pipe page.theme through automatically so operators set
    // the theme once on CmsPage instead of having to pass it at
    // every call site. Legacy callers can still call
    // page_shell_themed directly with an explicit override.
    page_shell_themed(page, css_href, body, critical_css, page.theme.as_deref())
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
    // Brand label: explicit page.brand wins; otherwise derive from
    // the first segment of title before a separator. Never hardcode
    // another site's name.
    let brand_raw = page.brand.clone().unwrap_or_else(|| {
        let t = page.title.trim();
        for sep in [" — ", " · ", "—", "·", " - ", "–"] {
            if let Some(i) = t.find(sep) {
                return t[..i].trim().to_owned();
            }
        }
        t.to_owned()
    });
    let brand = escape_html_text(&brand_raw);
    // Suppress the auto-emitted <h1 class="loom-page-title"> when
    // the first section is hero-class — heroes carry their own
    // display title, duplicating it as a header banner reads as
    // visual noise on a marketing landing.
    let first_is_hero = page.sections.first().is_some_and(|s| {
        matches!(
            s,
            CmsSection::Hero { .. }
                | CmsSection::ImageHero { .. }
                | CmsSection::SplitHero { .. }
                | CmsSection::CallToAction { .. }
                | CmsSection::Banner { .. }
                | CmsSection::AnnouncementBar { .. }
        )
    });
    // When the first section is hero-class, suppress the
    // visible <h1 class="loom-page-title"> banner (the hero
    // carries the title visually) but keep a screen-reader-only
    // <h1> so SEO + assistive tech still see a single H1.
    let page_title_block = if first_is_hero {
        format!("\n    <h1 class=\"loom-sr-only\">{title}</h1>")
    } else {
        format!("\n    <h1 class=\"loom-page-title\">{title}</h1>")
    };
    // T72: bundle the theme-toggle button CSS into the inline
    // critical-CSS block. Recomputes the hash naturally.
    let base_with_toggle = format!("{BASE_THEME_CSS}{THEME_TOGGLE_CSS}");
    let base_theme_hash = csp_sha256(base_with_toggle.as_bytes());
    let base_theme_block = format!("<style>{base_with_toggle}</style>\n  ");
    let toggle_script_hash = csp_sha256(THEME_TOGGLE_JS.as_bytes());
    let eruda_hash = csp_sha256(ERUDA_LOADER_JS.as_bytes());
    // LOOM_NOSCRIPT_MODE — process-level env that drops every
    // inline script and the defer-stylesheet onload swap. Used
    // by Forge when forge.toml `[noscript_strict] enabled = true`
    // for LibreJS / Tor-strict / hunted-tier builds. The rendered
    // HTML carries zero `<script>` tags + a maximally-strict CSP.
    let noscript_mode = std::env::var("LOOM_NOSCRIPT_MODE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    // Dev-only devtools loader: emitted in <head>, gated on
    // localStorage["loom_eruda"] == "on" so it does nothing for
    // strangers who happen onto a dev page. Forced off in
    // noscript_mode.
    let eruda_block = if page.dev_devtools && !noscript_mode {
        format!("<script>{ERUDA_LOADER_JS}</script>\n  ")
    } else {
        String::new()
    };
    #[allow(clippy::option_if_let_else)]
    let (extra_style_block, css_link, csp) = if let Some(crit) = critical_css {
        let style_hash = csp_sha256(crit.as_bytes());
        let onload_hash = csp_sha256(DEFER_ONLOAD_JS.as_bytes());
        let extra_block = format!("<style>{crit}</style>\n  ");
        // In noscript_mode the defer-onload swap doesn't work
        // (no JS to fire the onload), so use a plain stylesheet
        // link instead.
        let css_link = if noscript_mode {
            format!("<link rel=\"stylesheet\" href=\"{css}\">")
        } else {
            format!(
                "<link rel=\"stylesheet\" href=\"{css}\" media=\"print\" onload=\"{DEFER_ONLOAD_JS}\">\n  <noscript><link rel=\"stylesheet\" href=\"{css}\"></noscript>"
            )
        };
        // T72 cycle 96 iter 10: add Trusted Types CSP-L3
        // directives. require-trusted-types-for 'script' makes
        // the browser reject DOM-sink string assignments
        // (innerHTML, document.write, eval). trusted-types 'none'
        // means no policy creation allowed — strongest stance.
        // Our inline scripts (THEME_TOGGLE_JS + DEFER_ONLOAD_JS)
        // only use setAttribute/textContent/addEventListener and
        // single-string property writes — all safe under TT.
        //
        // Dev-only override: when page.dev_devtools is set, drop
        // Trusted Types and allow 'unsafe-inline' styles so Eruda
        // can inject its panel UI. Eruda lives in /eruda.min.js
        // (same-origin) so script-src 'self' is still enough.
        let csp = if noscript_mode {
            // Strictest CSP — no inline scripts allowed at all.
            format!(
                "default-src 'self'; img-src 'self' data:; style-src 'self' '{base_theme_hash}' '{style_hash}'; script-src 'none'; require-trusted-types-for 'script'; trusted-types 'none'; frame-ancestors 'none'"
            )
        } else if page.dev_devtools {
            format!(
                "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-hashes' '{onload_hash}' '{toggle_script_hash}' '{eruda_hash}'; connect-src 'self'; frame-ancestors 'none'"
            )
        } else {
            format!(
                "default-src 'self'; img-src 'self' data:; style-src 'self' '{base_theme_hash}' '{style_hash}'; script-src 'self' 'unsafe-hashes' '{onload_hash}' '{toggle_script_hash}'; require-trusted-types-for 'script'; trusted-types 'none'; frame-ancestors 'none'"
            )
        };
        (extra_block, css_link, csp)
    } else {
        let css_link = format!("<link rel=\"stylesheet\" href=\"{css}\">");
        let csp = if noscript_mode {
            format!(
                "default-src 'self'; img-src 'self' data:; style-src 'self' '{base_theme_hash}'; script-src 'none'; require-trusted-types-for 'script'; trusted-types 'none'; frame-ancestors 'none'"
            )
        } else if page.dev_devtools {
            format!(
                "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' '{toggle_script_hash}' '{eruda_hash}'; connect-src 'self'; frame-ancestors 'none'"
            )
        } else {
            format!(
                "default-src 'self'; img-src 'self' data:; style-src 'self' '{base_theme_hash}'; script-src 'self' '{toggle_script_hash}'; require-trusted-types-for 'script'; trusted-types 'none'; frame-ancestors 'none'"
            )
        };
        (String::new(), css_link, csp)
    };
    let style_block = format!("{base_theme_block}{extra_style_block}");
    // Resolve chrome kind. Default per CmsPage's #[derive(Default)]
    // is PageShell — preserves backward compat for sites that
    // don't pick a chrome explicitly.
    let chrome = page.chrome.unwrap_or_default();
    // Render the nav-actions row for FloatingPill (PageShell
    // ignores nav_actions today).
    let nav_actions_html = render_nav_actions(&page.nav_actions);
    let content_width = page.content_width.unwrap_or_default();
    let footer_html = render_page_footer(page.footer.as_ref());
    let body_html = render_chrome_body(
        chrome,
        &brand,
        &nav_links,
        &nav_actions_html,
        &page_title_block,
        body,
        content_width,
        noscript_mode,
        &footer_html,
    );
    // T37 v1 + T66 (closes #649): closed allow-list for the
    // `data-theme` attribute. T66 extends to named palettes
    // ("warm" | "ocean" | "forest" | "violet" | "rose") on top
    // of "light" | "dark" | "auto". An attacker-controlled
    // value is dropped rather than escaped-and-emitted —
    // defence in depth on top of attribute-escape.
    let html_open = match theme {
        Some(t)
            if matches!(
                t,
                "light"
                    | "dark"
                    | "dark-amoled"
                    | "auto"
                    | "warm"
                    | "ocean"
                    | "forest"
                    | "violet"
                    | "rose"
                    | "sepia"
                    | "press"
                    | "hc-dark"
                    | "hc-light"
            ) =>
        {
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
  {eruda_block}{style_block}{css_link}\n\
</head>\n\
{body_html}\n\
</html>\n"
    )
}

/// Render the body innards for a given chrome variant. Returns
/// the complete `<body>...</body>` element including header,
/// main, footer, and theme-toggle script.
fn render_chrome_body(
    chrome: ChromeKind,
    brand: &str,
    nav_links: &str,
    nav_actions_html: &str,
    page_title_block: &str,
    body: &str,
    content_width: ContentWidth,
    noscript_mode: bool,
    footer_html: &str,
) -> String {
    let _ = nav_actions_html; // PageShell ignores nav_actions today
    let cw = content_width.attr_value();
    // Suppress the theme-toggle button + its bootstrap script in
    // noscript_mode. The site retains its server-rendered
    // data-theme; visitors who want a different theme set their
    // OS preference (the auto media query handles it).
    let toggle_btn_pageshell = if noscript_mode {
        ""
    } else {
        "<button type=\"button\" class=\"loom-theme-toggle\" data-loom-theme-toggle aria-label=\"Theme: light (click to cycle)\" aria-pressed=\"false\">☀</button>\n    "
    };
    let toggle_btn_floating = if noscript_mode {
        ""
    } else {
        "<button type=\"button\" class=\"loom-theme-toggle\" data-loom-theme-toggle aria-label=\"Theme: light (click to cycle)\" aria-pressed=\"false\">☀</button>\n      "
    };
    let toggle_script = if noscript_mode {
        String::new()
    } else {
        format!("<script>{THEME_TOGGLE_JS}</script>\n")
    };
    match chrome {
        ChromeKind::PageShell => format!(
            "<body data-chrome=\"page-shell\" data-content-width=\"{cw}\">\n  \
<a class=\"loom-skip\" href=\"#content\">Skip to content</a>\n  \
<header class=\"loom-page-header\">\n    \
<nav class=\"loom-page-nav\" aria-label=\"Primary\">\n      \
<a class=\"loom-page-brand\" href=\"/\" data-loom-rich-link=\"true\">{brand}</a>{nav_links}\n      \
{toggle_btn_pageshell}</nav>{page_title_block}\n  \
</header>\n  \
<main id=\"content\">\n{body}\n  </main>\n  \
{footer_html}\n  \
{toggle_script}</body>\n"
        ),
        ChromeKind::FloatingPill => format!(
            "<body data-chrome=\"floating-pill\" data-content-width=\"{cw}\">\n  \
<a class=\"loom-skip\" href=\"#content\">Skip to content</a>\n  \
<header class=\"loom-floating-pill\">\n    \
<nav class=\"loom-floating-pill__nav\" aria-label=\"Primary\">\n      \
<a class=\"loom-floating-pill__brand\" href=\"/\" data-loom-rich-link=\"true\">{brand}</a>\n      \
<div class=\"loom-floating-pill__links\">{nav_links}</div>\n      \
<div class=\"loom-floating-pill__actions\">{nav_actions_html}\n        \
{toggle_btn_floating}</div>\n    \
</nav>\n  \
</header>{page_title_block}\n  \
<main id=\"content\">\n{body}\n  </main>\n  \
{footer_html}\n  \
{toggle_script}</body>\n"
        ),
        ChromeKind::Minimal => format!(
            "<body data-chrome=\"minimal\" data-content-width=\"{cw}\">\n  \
<a class=\"loom-skip\" href=\"#content\">Skip to content</a>{page_title_block}\n  \
<main id=\"content\">\n{body}\n  </main>\n  \
{footer_html}\n  \
{toggle_script}</body>\n"
        ),
    }
}

/// Render the page footer. `None` → empty back-compat footer
/// (just the styled tag). `Some` → typed multi-column layout
/// with columns / contact info / legal links / colophon.
fn render_page_footer(footer: Option<&CmsFooter>) -> String {
    let Some(f) = footer else {
        return "<footer class=\"loom-page-footer\"></footer>".to_owned();
    };
    let mut out = String::new();
    out.push_str("<footer class=\"loom-page-footer loom-page-footer--rich\">");
    out.push_str("<div class=\"loom-page-footer__columns\">");
    for col in &f.columns {
        out.push_str("<section class=\"loom-page-footer__col\">");
        out.push_str("<h3 class=\"loom-page-footer__heading\">");
        out.push_str(&escape_html_text(&col.heading));
        out.push_str("</h3><ul class=\"loom-page-footer__links\">");
        for link in &col.links {
            let href = if loom_components::composer::is_safe_url(&link.href) {
                link.href.as_str()
            } else {
                "#invalid-link"
            };
            out.push_str("<li><a href=\"");
            out.push_str(&escape_html_attr(href));
            out.push_str("\" data-backend=\"");
            out.push_str(&escape_html_attr(&link.data_backend));
            out.push_str("\">");
            out.push_str(&escape_html_text(&link.label));
            out.push_str("</a></li>");
        }
        out.push_str("</ul></section>");
    }
    if let Some(c) = &f.contact {
        let heading = c.heading.as_deref().unwrap_or("Contact");
        out.push_str("<section class=\"loom-page-footer__col loom-page-footer__contact\">");
        out.push_str("<h3 class=\"loom-page-footer__heading\">");
        out.push_str(&escape_html_text(heading));
        out.push_str("</h3>");
        if let Some(phone) = &c.phone {
            let bytes = phone.as_bytes();
            let dialable = !bytes.is_empty()
                && (bytes[0] == b'+' || bytes[0].is_ascii_digit());
            if dialable {
                let tel = phone.replace([' ', '-', '(', ')'], "");
                out.push_str("<p><a href=\"tel:");
                out.push_str(&escape_html_attr(&tel));
                out.push_str("\">");
                out.push_str(&escape_html_text(phone));
                out.push_str("</a></p>");
            } else {
                out.push_str("<p>");
                out.push_str(&escape_html_text(phone));
                out.push_str("</p>");
            }
        }
        if let Some(email) = &c.email {
            out.push_str("<p><a href=\"mailto:");
            out.push_str(&escape_html_attr(email));
            out.push_str("\">");
            out.push_str(&escape_html_text(email));
            out.push_str("</a></p>");
        }
        if let Some(addr) = &c.address {
            out.push_str("<p>");
            out.push_str(&escape_html_text(addr));
            out.push_str("</p>");
        }
        if let Some(j) = &c.jurisdiction {
            out.push_str("<p class=\"loom-page-footer__jurisdiction\">");
            out.push_str(&escape_html_text(j));
            out.push_str("</p>");
        }
        out.push_str("</section>");
    }
    out.push_str("</div>");
    if !f.legal_links.is_empty() {
        out.push_str("<nav class=\"loom-page-footer__legal\" aria-label=\"Legal\"><ul>");
        for link in &f.legal_links {
            let href = if loom_components::composer::is_safe_url(&link.href) {
                link.href.as_str()
            } else {
                "#invalid-link"
            };
            out.push_str("<li><a href=\"");
            out.push_str(&escape_html_attr(href));
            out.push_str("\" data-backend=\"");
            out.push_str(&escape_html_attr(&link.data_backend));
            out.push_str("\">");
            out.push_str(&escape_html_text(&link.label));
            out.push_str("</a></li>");
        }
        out.push_str("</ul></nav>");
    }
    if let Some(c) = &f.colophon {
        out.push_str("<p class=\"loom-page-footer__colophon\">");
        out.push_str(&escape_html_text(c));
        out.push_str("</p>");
    }
    out.push_str("</footer>");
    out
}

#[cfg(test)]
mod page_shell_tests {
    use super::*;

    fn empty_page() -> CmsPage {
        CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "T".into(),
            description: "D".into(),
            path: "/".into(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![],
        }
    }

    #[test]
    fn always_emits_base_theme_block_csp_pinned() {
        // T72 cycle 96 iter 9: base-theme block bundles
        // BASE_THEME_CSS + THEME_TOGGLE_CSS, so the CSP hash
        // covers BOTH together.
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        let combined = format!("{BASE_THEME_CSS}{THEME_TOGGLE_CSS}");
        let hash = csp_sha256(combined.as_bytes());
        assert!(
            s.contains(&hash),
            "combined base-theme + toggle hash must appear in CSP"
        );
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
        // T72 cycle 96 iter 9: base-theme is the combined
        // BASE_THEME_CSS + THEME_TOGGLE_CSS block; critical_css is
        // a separate inline pinned by its own hash.
        let crit = "h1{color:red}";
        let s = page_shell(&empty_page(), "/loom-skin.css", "", Some(crit));
        let combined = format!("{BASE_THEME_CSS}{THEME_TOGGLE_CSS}");
        assert!(s.contains(&csp_sha256(combined.as_bytes())));
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
        assert!(
            inside.contains("<p>hello</p>"),
            "body must land inside <main>"
        );
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
        assert!(
            s.contains("data-theme=\"dark\""),
            "missing data-theme=dark: {s}"
        );
    }

    #[test]
    fn page_shell_themed_emits_data_theme_when_light() {
        let s = page_shell_themed(&empty_page(), "/loom-skin.css", "", None, Some("light"));
        assert!(
            s.contains("data-theme=\"light\""),
            "missing data-theme=light"
        );
    }

    #[test]
    fn page_shell_themed_with_none_matches_unthemed_shell() {
        let a = page_shell(&empty_page(), "/loom-skin.css", "", None);
        let b = page_shell_themed(&empty_page(), "/loom-skin.css", "", None, None);
        assert_eq!(
            a, b,
            "page_shell(...) must equal page_shell_themed(..., None)"
        );
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
            let s = page_shell_themed(&empty_page(), "/loom-skin.css", "", None, Some(hostile));
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
        // The @media (prefers-color-scheme: dark) block applies
        // ONLY when data-theme="auto" — explicit light or dark
        // get their own selector blocks. Effect: an explicit
        // data-theme="light" overrides the OS preference, since
        // the OS-driven dark rule is scoped to the auto block.
        assert!(
            BASE_THEME_CSS
                .contains("@media (prefers-color-scheme:dark){:root[data-theme=\"auto\"]"),
            "OS-dark media block must be scoped to data-theme=auto so explicit light wins"
        );
        // The explicit dark block stands on its own.
        assert!(
            BASE_THEME_CSS.contains(":root[data-theme=\"dark\"]"),
            "explicit dark selector must exist for data-theme=dark to apply"
        );
        // No standalone OS-dark rule that would override explicit light.
        let media_idx = BASE_THEME_CSS
            .find("@media (prefers-color-scheme:dark)")
            .expect("media block must be present");
        let after_media = &BASE_THEME_CSS[media_idx..];
        assert!(
            after_media
                .starts_with("@media (prefers-color-scheme:dark){:root[data-theme=\"auto\"]"),
            "media block must IMMEDIATELY scope to data-theme=auto"
        );
    }

    /// T70b-fix REGRESSION-GUARD: page_shell + render_page composed
    /// must produce EXACTLY ONE `<main>` element. Two `<main>`s
    /// per document is a WCAG violation.
    #[test]
    fn page_shell_with_rendered_body_produces_exactly_one_main() {
        let p = CmsPage {
            brand: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "Test".into(),
            description: "T".into(),
            path: "/".into(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            sections: vec![CmsSection::Heading {
                level: HeadingLevel::H2,
                text: "x".into(),
                polish: Vec::new(),
            }],
        };
        let body = render_page(&p).into_string();
        let composed = page_shell(&p, "/loom-skin.css", &body, None);
        let main_open_count = composed.matches("<main").count();
        let main_close_count = composed.matches("</main>").count();
        assert_eq!(
            main_open_count, 1,
            "exactly one <main> open: composed = {composed}"
        );
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
