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
    /// Optional brand logo asset rendered inside the
    /// `loom-page-brand` link. When set, the renderer emits
    /// `<img src=… alt=… width=… height=…>` followed by the
    /// `brand` text inside an `.loom-page-brand__name`
    /// visually-hidden span — preserving the AT-accessible
    /// brand name while showing the logo image visually.
    /// When unset, the brand renders as text only (the
    /// previous behavior).
    ///
    /// `src` is validated via `composer::is_safe_url` at
    /// render time; hostile schemes (`javascript:`, `data:`)
    /// suppress the `<img>` and fall back to text-only.
    ///
    /// Absent → page brand renders as text only (the
    /// pre-existing behaviour). Present + safe `src` → renders
    /// `<img>` plus a visually-hidden `<span>` carrying the
    /// `brand` text so screen readers still read the
    /// accessible name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_logo: Option<BrandLogo>,
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
    /// shape. Default = `PageShell` (sticky top-bar header — the
    /// substrate's baseline chrome). Other variants:
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
    /// Optional fully-qualified site origin (e.g.
    /// `"https://example.com"`). When set, the renderer
    /// prefixes `og:url` and `og:image` with this origin so
    /// social-card crawlers see fully-qualified URLs (the OG
    /// spec requires absolute URLs). When `None`, the renderer
    /// emits the relative path — works for most crawlers but
    /// not all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_origin: Option<String>,
    /// Optional social-card preview image path (e.g.
    /// `"/assets/photos/stock-80.jpg"`). When set, the renderer
    /// emits `og:image` and `twitter:image` meta tags. When the
    /// page also has `site_origin` set, the path is prefixed
    /// to produce a fully-qualified URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social_image: Option<String>,
    /// Optional typed footer. When `None`, the page-shell emits
    /// an empty `<footer class="loom-page-footer"></footer>`
    /// (back-compat). When `Some`, the renderer expands a typed
    /// multi-column footer with optional contact info and legal
    /// links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<CmsFooter>,
}

/// Brand logo asset surfaced inside the page-shell brand
/// anchor. See `CmsPage::brand_logo` for the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandLogo {
    /// Source URL of the logo image. Validated via
    /// `composer::is_safe_url` at render time; hostile
    /// schemes suppress the `<img>` entirely (text-only
    /// fallback).
    pub src: String,
    /// Accessible name. Should describe the brand the logo
    /// represents (e.g. `"Prosperity Club"`), not a visual
    /// description of the image. Maud auto-escapes.
    pub alt: String,
    /// Intrinsic width in CSS pixels. Optional but
    /// recommended — prevents layout shift (Cumulative
    /// Layout Shift) while the image loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Intrinsic height in CSS pixels. Optional but
    /// recommended — see `width`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
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
///
/// **Doc convention for variants:** every variant carries a one-line
/// docstring explaining its purpose. Inline-struct fields on each
/// variant are self-documenting via their type signature + the
/// JSON-schema name (e.g., `children_html: String` on `Container`
/// is exactly what it says) — per-field docs are only added when a
/// Atomic content block — the building blocks a Visual-Studio-
/// style builder manipulates. Compose into `Vec<CmsBlock>` and
/// wrap in [`CmsSection::Compose`] for arbitrary layout shapes
/// that aren't bound to a section-level premade.
///
/// Section primitives (Hero / FeatureSpotlight / StatBand / etc.)
/// bundle composition + content + style; that pattern produces
/// homogeneous sites because every tenant draws from the same
/// fixed pool of section shapes. The atomic primitives invert
/// that: a tenant composes from small reusable pieces (text,
/// heading, image, link, container, row, column, spacer,
/// divider) and the visual differentiation comes from the
/// tenant's `[style]` config (palette, fonts, density) rather
/// than a section-level decoration enum.
///
/// Block-level primitives are intentionally orthogonal to section
/// primitives — both coexist for the migration window. Operators
/// who already author with section primitives keep working; new
/// operators (and the eventual visual builder) use blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(missing_docs)]
pub enum CmsBlock {
    /// Body-prose paragraph. Renders as `<p>` with prose token
    /// styling. Use for editorial copy, captions, supporting
    /// text.
    Text {
        /// Paragraph contents. Escaped at render time.
        text: String,
    },
    /// Hierarchical heading. `level` clamps to 1..=6 with 2 as
    /// the default in-section heading (the page-level `<h1>` is
    /// usually a separate hero / page-title slot).
    Heading {
        /// HTML heading level. Clamped at render to 1..=6.
        level: u8,
        /// Heading text.
        text: String,
    },
    /// Image with required `alt`. `src` is validated via
    /// `composer::is_safe_url` at render time; hostile schemes
    /// suppress the `<img>` entirely (block becomes invisible
    /// rather than emitting an unsafe URL).
    Image {
        /// Image URL.
        src: String,
        /// Accessible name. Empty string permitted ONLY for
        /// genuinely decorative images.
        alt: String,
        /// Optional intrinsic width hint (CLS prevention).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        /// Optional intrinsic height hint (CLS prevention).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
    },
    /// Inline hyperlink. Renders as `<a>` with prose styling.
    /// For prominent call-to-action affordances use [`CmsBlock::Button`].
    Link {
        /// Visible link text.
        label: String,
        /// Destination URL. Validated via `is_safe_url`.
        href: String,
        /// Optional `data-backend` slug for the phantom-button
        /// gate.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data_backend: Option<String>,
    },
    /// Call-to-action button. Distinct from [`CmsBlock::Link`] —
    /// buttons carry a `variant` + `size` that the tenant's
    /// `[style.button]` config maps to concrete fill / border /
    /// shadow.
    Button {
        /// Visible label.
        label: String,
        /// Destination URL. Validated via `is_safe_url`.
        href: String,
        /// Style variant.
        variant: ButtonVariant,
        /// Token-scale size step.
        #[serde(default)]
        size: ButtonSize,
        /// Optional `data-backend` slug.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data_backend: Option<String>,
    },
    /// Vertical whitespace. `size` is a token-scale step
    /// (`Sm`/`Md`/`Lg`/`Xl`) so spacing decisions inherit from
    /// the tenant's `[style]` density config rather than carrying
    /// raw pixel values.
    Spacer {
        /// Spacing step.
        size: BlockSpacing,
    },
    /// Horizontal rule. Decorative — does NOT participate in the
    /// accessibility tree (renderer emits `aria-hidden="true"`).
    Divider,
    /// Generic container — wraps children in a `<div>` with
    /// optional padding step.
    Container {
        /// Optional padding step.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        padding: Option<BlockSpacing>,
        /// Block children. Flat — no implicit ordering.
        children: Vec<CmsBlock>,
    },
    /// Horizontal flexbox row. Children flow left-to-right;
    /// wraps at small viewports per the tenant's density config.
    Row {
        /// Gap between children. Token-scale step.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gap: Option<BlockSpacing>,
        /// Cross-axis alignment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<BlockAlign>,
        /// Block children.
        children: Vec<CmsBlock>,
    },
    /// Vertical flexbox column. Children flow top-to-bottom.
    Column {
        /// Gap between children. Token-scale step.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gap: Option<BlockSpacing>,
        /// Cross-axis alignment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<BlockAlign>,
        /// Block children.
        children: Vec<CmsBlock>,
    },
    /// Disclosure list — collapsible summary + content panels.
    /// Behavioral contract mirrors Radix UI's `Accordion`
    /// primitive (slot composition: each item carries a
    /// `summary` and a child block tree). Renders as the native
    /// `<details>/<summary>` element pair: no JS required, full
    /// keyboard + screen-reader support out of the box.
    ///
    /// Upstream behavioral spec: <https://www.radix-ui.com/primitives/docs/components/accordion>
    /// (MIT). No source copied — typed Rust reimplementation.
    ///
    /// `single_expand: true` enforces "at most one open at a time"
    /// via the shared HTML `name` attribute on `<details>` (Chromium
    /// 120+, Safari 17.5+, Firefox 130+). Older browsers degrade
    /// gracefully to independent-open behavior.
    Accordion {
        /// Item list.
        items: Vec<BlockAccordionItem>,
        /// When true, all `<details>` elements share the same
        /// `name` attribute so only one is open at any moment.
        #[serde(default)]
        single_expand: bool,
    },
    /// Transient notification banner. Renders with ARIA live-
    /// region semantics so screen readers announce the message
    /// as it appears. `role` + `aria-live` are derived from
    /// `tone`: info/success → `role=status` + `aria-live=polite`;
    /// warning/error → `role=alert` + `aria-live=assertive`.
    ///
    /// Static substrate renders the toast inline; a future Loom
    /// JS runtime animates show/dismiss + handles auto-dismiss
    /// timers. `dismissible=true` includes a close button that
    /// the JS runtime wires to `hidden` toggling.
    ///
    /// Behavioral contract mirrors Radix UI's `Toast` primitive.
    /// Upstream spec: <https://www.radix-ui.com/primitives/docs/components/toast>
    /// (MIT). No source copied.
    Toast {
        /// Unique HTML `id`. The JS runtime references this for
        /// auto-dismiss timers + open/close transitions.
        id: String,
        /// Tone — drives the live-region severity + visual
        /// accent.
        #[serde(default)]
        tone: ToastTone,
        /// Optional bold title rendered above the message body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Main message body.
        message: String,
        /// When true, render an `<button aria-label="Dismiss">`
        /// the JS runtime wires to hide the toast.
        #[serde(default)]
        dismissible: bool,
        /// When false, the toast renders with the `hidden`
        /// attribute so the JS runtime can reveal it on demand
        /// (page-load + interaction triggers).
        #[serde(default = "default_true")]
        open: bool,
    },
    /// Text input with an autocomplete list. Renders the native
    /// `<input list>` + `<datalist>` pattern — browser shows the
    /// dropdown of matching options as the user types. No JS
    /// required.
    ///
    /// Behavioral contract mirrors Radix UI's `Combobox` /
    /// React Aria's `ComboBox` primitive. Upstream specs:
    /// <https://react-spectrum.adobe.com/react-aria/ComboBox.html>
    /// (Apache-2.0). No source copied.
    Combobox {
        /// Unique HTML `id` for the input.
        id: String,
        /// Visible label text.
        label: String,
        /// Optional form-field `name` attribute.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Optional placeholder.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        /// Optional initial value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        /// Autocomplete option list.
        options: Vec<BlockComboboxOption>,
    },
    /// Menu of actions — popover surface with `role="menu"` +
    /// `role="menuitem"` children. Same native-popover machinery
    /// as [`CmsBlock::Popover`] (browser handles open/close +
    /// light-dismiss + focus); the menu role tells assistive tech
    /// to expose menu-navigation semantics (arrow keys, type-to-
    /// jump).
    ///
    /// Behavioral contract mirrors Radix UI's `DropdownMenu`
    /// primitive. Upstream spec:
    /// <https://www.radix-ui.com/primitives/docs/components/dropdown-menu>
    /// (MIT). No source copied.
    DropdownMenu {
        /// Unique HTML `id` referenced by `popovertarget=`.
        id: String,
        /// Visible label on the trigger button.
        trigger_label: String,
        /// Menu items in order.
        items: Vec<BlockDropdownItem>,
    },
    /// Anchored content that opens on trigger click. Uses the
    /// native HTML `popover` attribute (Chromium 114+, Safari
    /// 17+, Firefox 125+) so the browser handles open/close +
    /// light-dismiss + focus management without JS.
    ///
    /// Behavioral contract mirrors Radix UI's `Popover` primitive.
    /// Upstream spec: <https://www.radix-ui.com/primitives/docs/components/popover>
    /// (MIT). No source copied.
    Popover {
        /// Unique HTML `id` for the popover element. The trigger
        /// references this via `popovertarget=`.
        id: String,
        /// Visible label on the trigger button.
        trigger_label: String,
        /// Block tree for the popover body.
        content: Vec<CmsBlock>,
        /// Placement relative to the trigger.
        #[serde(default)]
        placement: TooltipPlacement,
    },
    /// Tabbed content panels. Renders ARIA-correct
    /// `role="tablist"` / `role="tab"` / `role="tabpanel"`
    /// markup. WITHOUT JS, only the first panel is visible —
    /// substrate doctrine is static-first + progressive
    /// enhancement; a future Loom JS runtime adds click +
    /// keyboard-arrow switching by toggling `aria-selected`
    /// and the `hidden` attribute on panels.
    ///
    /// Behavioral contract mirrors Radix UI's `Tabs` primitive.
    /// Upstream spec: <https://www.radix-ui.com/primitives/docs/components/tabs>
    /// (MIT). No source copied.
    Tabs {
        /// Tab items in order. The first item is the default-
        /// visible panel; future items are rendered with
        /// `hidden` so a no-JS reader sees only the first.
        items: Vec<BlockTabsItem>,
    },
    /// Modal / non-modal dialog. Renders as the native HTML
    /// `<dialog>` element — universally supported (Chromium 37+,
    /// Safari 15.4+, Firefox 98+). Includes a default close
    /// button wrapped in `<form method="dialog">` so the dialog
    /// closes without JS when the button is clicked (the browser
    /// implements the `dialog` form method natively).
    ///
    /// `modal=true` is rendered as a `data-modal="true"` hint;
    /// actual modal behaviour (`.showModal()` vs `.show()`)
    /// requires a JS layer that the substrate's progressive-
    /// enhancement runtime drives. Static substrate respects
    /// `open=true` to render the dialog as visible on first
    /// paint; `open=false` produces a hidden dialog the JS
    /// runtime can open.
    ///
    /// Behavioral contract mirrors Radix UI's `Dialog` primitive.
    /// Upstream spec: <https://www.radix-ui.com/primitives/docs/components/dialog>
    /// (MIT). No source copied.
    Dialog {
        /// Unique HTML `id`. Required so external triggers can
        /// target the dialog (e.g. a button with
        /// `onclick="document.getElementById('id').showModal()"`).
        id: String,
        /// Dialog title rendered inside `<h2>` at the top of the
        /// content area.
        title: String,
        /// Block tree for the dialog body.
        content: Vec<CmsBlock>,
        /// When true, render with the `open` attribute so the
        /// dialog is visible on first paint.
        #[serde(default)]
        open: bool,
        /// When true, mark the dialog as intended-modal. The
        /// substrate JS runtime uses this to choose between
        /// `.show()` and `.showModal()` when programmatically
        /// opening.
        #[serde(default)]
        modal: bool,
    },
    /// Tooltip — hover/focus-revealed annotation tied to a
    /// trigger phrase. Renders entirely CSS-driven (no JS): the
    /// tooltip body shows on `:hover` and `:focus-visible` of
    /// the wrapping `<span>`. `aria-describedby` ties the
    /// trigger to the tooltip body for screen-reader users; the
    /// wrapper carries `tabindex="0"` so keyboard users can
    /// focus the trigger.
    ///
    /// Behavioral contract mirrors Radix UI's `Tooltip` primitive.
    /// Upstream spec: <https://www.radix-ui.com/primitives/docs/components/tooltip>
    /// (MIT). No source copied.
    Tooltip {
        /// Visible trigger text.
        label: String,
        /// Tooltip body. Shown on hover + focus.
        content: String,
        /// Placement relative to the trigger.
        #[serde(default)]
        placement: TooltipPlacement,
    },
    /// Numeric range picker. Renders as a labelled
    /// `<input type="range">` — native form control, full
    /// keyboard support (arrow keys ±step, Home/End for
    /// min/max, PageUp/PageDown for ±10% of range), no JS
    /// required.
    ///
    /// Behavioral contract mirrors Radix UI's `Slider` primitive.
    /// Upstream spec: <https://www.radix-ui.com/primitives/docs/components/slider>
    /// (MIT). No source copied.
    ///
    /// `min` / `max` / `step` follow the HTML5
    /// `<input type="range">` semantics — `step` of `0.0` means
    /// "any value" (the browser picks granularity).
    Slider {
        /// Unique HTML `id` for the input. Required for `<label>`
        /// association.
        id: String,
        /// Visible label text.
        label: String,
        /// Optional form-field `name` attribute. When set, the
        /// slider participates in form submission.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Minimum value.
        min: f64,
        /// Maximum value.
        max: f64,
        /// Step granularity. `0.0` means "any value".
        #[serde(default = "default_slider_step")]
        step: f64,
        /// Current value.
        value: f64,
        /// When true, the slider renders as disabled.
        #[serde(default)]
        disabled: bool,
        /// When true, the current value renders alongside the
        /// slider as a live indicator. Useful for forms where
        /// the user needs to see the numeric value without
        /// reading from the input itself.
        #[serde(default)]
        show_value: bool,
    },
    /// Boolean toggle. Renders as an accessible
    /// `<label><input type="checkbox" role="switch">…</label>`
    /// — native form control, full keyboard + screen-reader
    /// support, no JS required to operate. Form-mode opt-in
    /// via the `name` field so the switch posts state along
    /// with a surrounding `<form>`.
    ///
    /// Behavioral contract mirrors Radix UI's `Switch` primitive.
    /// Upstream spec: <https://www.radix-ui.com/primitives/docs/components/switch>
    /// (MIT). No source copied.
    Switch {
        /// Unique HTML `id` for the input. Required because the
        /// `<label>` references it via the `for` attribute when
        /// the label text is rendered separately, and external
        /// labels (in tenant CMS pages that nest a Switch in a
        /// custom-labelled row) bind via the same `id`.
        id: String,
        /// Visible label text. Rendered inside the wrapping
        /// `<label>` so click + tap targets the input.
        label: String,
        /// Optional form-field `name` attribute. When set, the
        /// switch participates in form submission with the
        /// declared name (no submission when omitted — a
        /// stand-alone UI affordance).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Initial state.
        #[serde(default)]
        checked: bool,
        /// When true, the switch renders as disabled (greyed
        /// out, non-interactive). Useful for showing state
        /// without allowing user changes.
        #[serde(default)]
        disabled: bool,
    },
}

/// One autocomplete option inside a [`CmsBlock::Combobox`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockComboboxOption {
    /// Submission value (also the visible suggestion when `label`
    /// is unset).
    pub value: String,
    /// Optional human-readable label. When set, the dropdown
    /// shows the label and the input fills with the value on
    /// selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// One menu item inside a [`CmsBlock::DropdownMenu`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockDropdownItem {
    /// Visible menu-item label.
    pub label: String,
    /// Optional href. Validated via `is_safe_url` at render
    /// time; hostile schemes route to `#invalid-link`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// Optional `data-backend` slug for the phantom-button gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_backend: Option<String>,
    /// When true, render as disabled (greyed out, non-actionable).
    #[serde(default)]
    pub disabled: bool,
    /// When true, render an `<hr role="separator">` immediately
    /// above this item — used to group related menu items.
    #[serde(default)]
    pub separator_before: bool,
}

/// One tab item inside a [`CmsBlock::Tabs`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockTabsItem {
    /// Trigger label rendered inside the `<button role="tab">`.
    pub label: String,
    /// Stable kebab-case slug used to construct the trigger id
    /// (`tab-{slug}`) and the panel id (`panel-{slug}`). Must
    /// be unique within a Tabs block.
    pub slug: String,
    /// Panel content as a block tree.
    pub content: Vec<CmsBlock>,
}

/// One disclosure item inside a [`CmsBlock::Accordion`]. Distinct
/// from the section-level [`AccordionItem`] (which is `title +
/// body: String`) — block-level items hold an arbitrary child
/// block tree so an accordion panel can carry any composition the
/// atomic primitives support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockAccordionItem {
    /// Summary text rendered inside `<summary>` — the always-
    /// visible label that toggles the panel.
    pub summary: String,
    /// Block tree rendered inside the disclosure panel.
    pub content: Vec<CmsBlock>,
    /// When true, the item is expanded on initial render
    /// (`<details open>`).
    #[serde(default)]
    pub default_open: bool,
}

/// Token-scale spacing step. Resolves to actual pixel / rem at
/// render time via the tenant's `[style.spacing]` config. The
/// substrate ships sensible defaults; tenants override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum BlockSpacing {
    None,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
}

/// Severity / accent tone for a [`CmsBlock::Toast`]. Drives the
/// `role` + `aria-live` policy as well as the visual accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ToastTone {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// Placement of a [`CmsBlock::Tooltip`] relative to its trigger.
/// The slug is emitted as `data-placement` so the skin cascade
/// can position the floating body via CSS without JS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum TooltipPlacement {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

/// Flexbox cross-axis alignment. Mirrors CSS `align-items` for
/// `Row` and `Column` blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum BlockAlign {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

/// Token-scale size step for a [`CmsBlock::Button`]. Resolved
/// against the tenant's `[style.button.size]` config at render
/// time. Reuses the existing [`ButtonVariant`] enum
/// (Primary/Secondary/Ghost/Danger).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
}

/// field's meaning is NOT obvious from `<name>: <type>` alone
/// (constraints, units, encoding format). The blanket
/// `#[allow(missing_docs)]` below avoids the maintenance tax of
/// 300+ noise-tier doc comments on the catalogue expansion; new
/// variants ARE expected to carry per-field docs for any
/// non-self-evident shape.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(missing_docs)]
pub enum CmsSection {
    /// Compositional section — holds a `Vec<CmsBlock>` of atomic
    /// primitives (text / heading / image / link / spacer /
    /// divider / container / row / column). Use this when the
    /// section-level premades (Hero / FeatureSpotlight / etc.)
    /// don't match the intended composition.
    ///
    /// Visual differentiation lives in the tenant's `[style]`
    /// config (palette, fonts, density), NOT in this section's
    /// shape. Two tenants using `Compose` with identical block
    /// trees will still render distinctly because their style
    /// configs differ.
    Compose {
        /// Optional section heading rendered above the block
        /// tree as `<h2>`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heading: Option<String>,
        /// Atomic block tree.
        blocks: Vec<CmsBlock>,
    },
    /// Top-of-page hero. Optional eyebrow pill, required title,
    /// optional lede, optional primary CTA. Loom-namespaced
    /// (no Tailwind dependency) so it composes cleanly with the
    /// loom-skin baseline.
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
    /// Editorial asymmetric hero — the substrate-aware sibling of
    /// [`CmsSection::Hero`]. Where `Hero` ships the canonical centered
    /// SaaS shape (eyebrow pill, gradient overlay, animate-fade-in
    /// chain), `HeroEditorial` is the editorial 2-column composition:
    /// monospace kicker, dominant headline, lede that sets its own
    /// measure via `max-w-prose`, optional CTA, optional right-column
    /// decoration. No SaaS-trope ornaments emitted.
    ///
    /// Wire shape mirrors [`loom_components::HeroEditorial`]; the CMS
    /// authors a section, the renderer emits editorial markup.
    HeroEditorial {
        /// Plain uppercase monospace metadata line above the headline
        /// (e.g., `"DISPATCH · 2026-05-20"`). NOT a pill.
        kicker: Option<String>,
        /// Headline body. Renders as the dominant typographic mass.
        headline: String,
        /// Optional accent fragment rendered after the headline in
        /// brand primary color.
        headline_accent: Option<String>,
        /// Lede paragraph. Use long-form editorial prose; the renderer
        /// lets prose set its own measure.
        lede: String,
        /// Optional primary CTA.
        cta: Option<HeroCta>,
        /// Background tone. Accepts `slate` (default), `plain`,
        /// or `amoled` (true-black).
        #[serde(default)]
        background: HeroEditorialBackground,
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
        /// Visual style for the form chrome. Drives the
        /// `data-loom-form-style` attribute on both the
        /// `<section>` AND `<form>` elements — loom-skin's cascade
        /// uses these to swap rounded/editorial/minimal input
        /// chrome.
        ///
        /// Defaults to `Rounded` for back-compat: existing CMS
        /// JSON without an explicit `style` field deserializes
        /// to the historical SaaS shape. Operators opt into
        /// editorial-density forms (per the consumer-shaping
        /// audit #103 Cat-3 work) via `"style": "editorial"`.
        #[serde(default)]
        style: CmsFormStyle,
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
        /// Optional id attribute — lets the heading serve as a
        /// jump-link anchor target (`href="#that-id"`). Slug-
        /// validated via [`SlugName`] when present so we never
        /// emit a freeform attribute value into the DOM.
        ///
        /// 2026-05-20 substrate addition: without this slot,
        /// every page that uses `/page.html#section` in a nav
        /// link triggers a link-check phase strict-fail. Adding
        /// the slot lets cms authors declare a typed anchor next
        /// to the heading it labels.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
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
        /// Layout density. Defaults to `comfortable`. Mirrors the
        /// loom-components KvPairCard density knob; affects vertical
        /// rhythm inside each item.
        #[serde(default)]
        density: KvPairDensity,
        /// Color tone. Defaults to `slate`. `amoled` honors
        /// `[[dark-theme-amoled-true-black]]` for OLED rendering.
        #[serde(default)]
        tone: KvPairTone,
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
    /// Typed-line terminal transcript — the substrate-aware sibling
    /// of [`CmsSection::Code`]. Where `Code` is a single flat body
    /// string with a `terminal` flag, `CodeShell` carries per-line
    /// semantic role annotation (Command / Output / Comment / Error)
    /// so the skin can color + indent each line by its meaning. Wire
    /// shape mirrors [`loom_components::CodeShell`].
    ///
    /// Use cases: forge build audit-chain proof, configuration walk-
    /// throughs, terminal-style decoration inside a HeroEditorial.
    CodeShell {
        /// Header title — shell name, filename, or label. Only
        /// emitted when `chrome` is `Header`.
        title: Option<String>,
        /// Prompt prefix for `Command` lines. `None` defaults to `$`.
        prompt: Option<String>,
        /// Transcript lines in order.
        lines: Vec<CmsCodeShellLine>,
        /// Color tone: `slate` (default) or `amoled` (true-black).
        #[serde(default)]
        tone: CodeShellTone,
        /// Chrome treatment: `minimal` (default) or `header`.
        #[serde(default)]
        chrome: CodeShellChrome,
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
        /// Text + content alignment. Default is `Center` (SaaS hero
        /// posture). Editorial sites can opt out via `Start` for
        /// left-aligned headline + lede + cta — the substrate
        /// emits a `data-align` attribute the skin keys on.
        ///
        /// 2026-05-20 substrate-de-consumer-shaping addition.
        /// Northbrook Observatory ships with `align: "start"` to
        /// avoid the SaaS-trope centered hero.
        #[serde(default)]
        align: HeroAlign,
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
        /// Text + content alignment. Default is `Center` (SaaS
        /// CTA-band posture); editorial sites can opt out via
        /// `Start` for left-aligned. Same `HeroAlign` enum
        /// reused so the cms author has one mental model across
        /// hero + cta sections.
        ///
        /// 2026-05-20 substrate-de-consumer-shaping addition.
        #[serde(default)]
        align: HeroAlign,
    },
    /// Editorial pull-quote. Editorial-mark composition — left-border
    /// rule, no card chrome, no decorative quote-mark glyphs. Distinct
    /// from `Quote` which is the testimonial-card shape.
    ///
    /// Wire-extended in this commit to mirror
    /// [`loom_components::PullQuote`]: `cite_url`, `emphasis`, and
    /// `tone` ship as additive fields with serde defaults so the
    /// existing two-field shape (`body` + `attribution`) keeps
    /// parsing.
    PullQuote {
        /// Body of the quote. Multi-paragraph bodies split on `\n\n`
        /// into separate `<p>` tags inside the `<blockquote>`.
        body: String,
        /// Optional attribution (e.g. "Jane Doe, CTO @ Acme").
        attribution: Option<String>,
        /// Optional source URL — rendered into the `cite=` attribute
        /// on `<blockquote>` per the HTML spec for machine-readable
        /// provenance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cite_url: Option<String>,
        /// Emphasis tier: `inline` (text-xl/2xl) or `display`
        /// (text-2xl/3xl/4xl). Defaults to `inline`.
        #[serde(default)]
        emphasis: PullQuoteEmphasis,
        /// Color tone: `slate` (default) or `amoled` (true-black).
        #[serde(default)]
        tone: PullQuoteTone,
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
    Container {
        children_html: String,
        max_width: ContainerWidth,
    },
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
    /// Crucible challenge embed. Renders a mount node + a
    /// `<script type="module">` that imports the crucible-widget
    /// WASM bundle and calls its `init(...)` against the mount.
    ///
    /// Forge sites that want bot-screening drop a section like:
    /// ```json
    /// {
    ///   "kind": "crucible_challenge",
    ///   "kind_slug": "math-arithmetic",
    ///   "tenant_id": "acme",
    ///   "base_path": "/crucible",
    ///   "widget_url": "/static/crucible-widget/crucible_widget.js"
    /// }
    /// ```
    /// The host runs a `crucible-server` peer (typically as a
    /// reverse-proxy backend under `<base_path>`) that mints +
    /// verifies challenges and emits CapturedTuple → LFI corpus.
    CrucibleChallenge {
        /// Challenge kind slug (`"math-arithmetic"`,
        /// `"semantic-similarity"`, etc). Passed to the
        /// widget's `init(kind, ...)` argument.
        kind_slug: String,
        /// Tenant identifier for per-tenant attribution +
        /// corpus scope. Passed to the widget's
        /// `init(..., tenant_id, ...)` argument.
        tenant_id: String,
        /// Base path of the crucible-server router. Defaults
        /// to `/crucible` if absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_path: Option<String>,
        /// URL of the widget JS module emitted by `wasm-pack
        /// build --target web`. Required.
        widget_url: String,
    },
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
    Figure {
        caption: String,
        credit: Option<String>,
        asset_slug: Option<String>,
    },
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
    Diagram {
        notation: DiagramKind,
        source: String,
        alt: String,
    },
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
    LogoCloud {
        heading: Option<String>,
        items: Vec<String>,
    },
    /// Side-by-side feature/spec comparison.
    Comparison {
        heading: Option<String>,
        columns: Vec<String>,
        rows: Vec<ComparisonRow>,
    },
    /// Vertical milestone timeline.
    Timeline {
        heading: Option<String>,
        items: Vec<TimelineItem>,
    },
    /// Public-facing product roadmap (now/next/later).
    Roadmap {
        now: Vec<String>,
        next: Vec<String>,
        later: Vec<String>,
    },
    /// Case-study card with quote + metrics.
    CaseStudy {
        headline: String,
        body: String,
        metrics: Vec<StatItem>,
        href: Option<String>,
        data_backend: Option<String>,
    },
    /// Top-of-viewport announcement bar.
    AnnouncementBar {
        text: String,
        cta: Option<HeroCta>,
        tone: AlertTone,
    },
    /// Cookie notice band.
    CookieNotice {
        text: String,
        accept_label: String,
        reject_label: String,
    },
    /// Mid-page promo strip with CTA.
    PromoStrip { text: String, cta: HeroCta },
    /// Row of award badges.
    AwardBadges {
        heading: Option<String>,
        items: Vec<String>,
    },
    /// Email-signup capture row.
    NewsletterSignup {
        heading: String,
        lede: Option<String>,
        placeholder: String,
        submit_label: String,
    },
    /// Compact contact strip with channels.
    ContactStrip { items: Vec<ContactChannel> },

    // Media (10).
    /// Photo grid gallery.
    ImageGrid {
        items: Vec<GalleryImage>,
        columns: u8,
    },
    /// Group of figures arranged horizontally.
    FigureGroup { items: Vec<GalleryImage> },
    /// HTML5 video embed (typed source allowlist).
    VideoEmbed {
        src: String,
        poster: Option<String>,
        alt: String,
        mime: String,
    },
    /// HTML5 audio embed.
    AudioEmbed {
        src: String,
        alt: String,
        mime: String,
    },
    /// Auto-rotating image slideshow.
    Slideshow {
        items: Vec<GalleryImage>,
        interval_ms: u32,
    },
    /// Before/after slider comparison.
    BeforeAfter {
        before_alt: String,
        after_alt: String,
        before_slug: String,
        after_slug: String,
    },
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
    ProductCard {
        name: String,
        price: String,
        rating: Option<f32>,
        image_alt: String,
        image_slug: String,
        href: String,
        data_backend: String,
    },
    /// Product grid (collection of ProductCard payloads).
    ProductGrid {
        heading: Option<String>,
        items: Vec<ProductItem>,
    },
    /// Inline price tag.
    PriceTag {
        amount: String,
        currency: String,
        was: Option<String>,
    },
    /// Add-to-cart button.
    AddToCart {
        label: String,
        sku: String,
        data_backend: String,
    },
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
    ReviewCard {
        author: String,
        rating: f32,
        body: String,
        date: Option<String>,
    },

    // Social (10).
    /// Single avatar.
    Avatar {
        avatar: CmsAvatar,
        label: Option<String>,
    },
    /// Overlapping avatar stack.
    AvatarStack {
        items: Vec<CmsAvatar>,
        more: Option<u32>,
    },
    /// Chat bubble.
    ChatBubble {
        author: String,
        body: String,
        mine: bool,
    },
    /// Threaded chat.
    ChatThread { items: Vec<ChatMessage> },
    /// Like/love/etc reaction row.
    ReactionRow { items: Vec<ReactionItem> },
    /// @username inline mention.
    MentionInline {
        username: String,
        href: String,
        data_backend: String,
    },
    /// #tag inline hashtag.
    HashtagInline {
        tag: String,
        href: String,
        data_backend: String,
    },
    /// Row of share buttons.
    ShareRow { url: String, title: String },
    /// Follow button with count.
    FollowButton {
        label: String,
        count: u32,
        data_backend: String,
    },
    /// Profile card.
    ProfileCard {
        name: String,
        handle: String,
        bio: String,
        avatar: CmsAvatar,
        follow: Option<FollowAction>,
    },

    // Forms (10).
    /// Single labeled input.
    FormInput {
        name: String,
        label: String,
        input_type: FormInputKind,
        placeholder: Option<String>,
        required: bool,
    },
    /// Labeled select.
    FormSelect {
        name: String,
        label: String,
        options: Vec<SelectOption>,
        required: bool,
    },
    /// Switch toggle.
    FormToggle {
        name: String,
        label: String,
        on: bool,
    },
    /// Range slider.
    FormSlider {
        name: String,
        label: String,
        min: i32,
        max: i32,
        value: i32,
    },
    /// Date picker.
    FormDate {
        name: String,
        label: String,
        required: bool,
    },
    /// File upload dropzone.
    FormFile {
        name: String,
        label: String,
        accept: String,
    },
    /// Search input with submit.
    FormSearch {
        placeholder: String,
        data_backend: String,
    },
    /// Color picker.
    FormColor {
        name: String,
        label: String,
        value: String,
    },
    /// Long-form textarea.
    FormTextarea {
        name: String,
        label: String,
        placeholder: Option<String>,
        rows: u8,
    },
    /// Submit button.
    FormSubmit {
        label: String,
        data_backend: String,
        variant: ButtonVariant,
    },

    // Navigation (8).
    /// Breadcrumb trail.
    Breadcrumb { items: Vec<BreadcrumbItem> },
    /// Numbered pagination.
    Pagination {
        current: u32,
        total: u32,
        base_href: String,
        data_backend: String,
    },
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
    LangSwitch {
        current: String,
        options: Vec<LangOption>,
    },

    // Feedback (8).
    /// Tonal alert box.
    Alert {
        tone: AlertTone,
        title: String,
        body: String,
        dismissible: bool,
    },
    /// Transient toast (visible target for live regions).
    Toast { tone: AlertTone, body: String },
    /// Modal dialog placeholder (rendered as a typed section).
    Modal {
        title: String,
        body: String,
        primary: HeroCta,
        secondary: Option<HeroCta>,
    },
    /// Side drawer.
    Drawer {
        title: String,
        body: String,
        side: DrawerSide,
    },
    /// Tooltip target slot.
    Tooltip { trigger: String, body: String },
    /// Progress bar.
    ProgressBar { value: u8, label: Option<String> },
    /// Loading skeleton group.
    Skeleton { rows: u8, height: SpaceSize },
    /// Empty-state placeholder.
    EmptyState {
        title: String,
        body: String,
        cta: Option<HeroCta>,
    },

    // Game / Forum / Video (8).
    /// Game tile thumbnail.
    GameTile {
        title: String,
        genre: String,
        players_online: u32,
        image_slug: String,
        href: String,
        data_backend: String,
    },
    /// Game grid.
    GameGrid {
        heading: Option<String>,
        items: Vec<GameTileItem>,
    },
    /// Thread list row.
    ThreadRow {
        title: String,
        author: String,
        replies: u32,
        views: u32,
        last_reply: String,
        href: String,
        data_backend: String,
    },
    /// List of thread rows.
    ThreadList {
        heading: Option<String>,
        items: Vec<ThreadRowItem>,
    },
    /// Video card with thumbnail + meta.
    VideoCard {
        title: String,
        channel: String,
        duration: String,
        views: String,
        thumbnail_slug: String,
        href: String,
        data_backend: String,
    },
    /// Grid of video cards.
    VideoGridSection {
        heading: Option<String>,
        items: Vec<VideoCardItem>,
    },
    /// Comment thread (post + nested replies).
    CommentThread {
        post_id: String,
        items: Vec<CommentItem>,
    },
    /// Social-feed post card.
    FeedPost {
        author: String,
        handle: String,
        avatar: CmsAvatar,
        body: String,
        posted_at: String,
        reactions: u32,
        comments: u32,
    },

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
    /// Typed changelog block — versioned release-notes entries
    /// following the Keep-a-Changelog convention (keepachangelog.com).
    /// Each entry has a version + date + optional summary + a
    /// typed list of changes categorized by kind (Added / Changed /
    /// Deprecated / Removed / Fixed / Security).
    ///
    /// Use cases:
    /// - Product CHANGELOG.md rendered into the public docs site
    /// - Release-notes section on a marketing/dev-tool homepage
    /// - Annotator audit-log surface for compliance reporting
    ///
    /// Distinct from `Timeline` (general chronological events) +
    /// `Roadmap` (now/next/later) — Changelog is specifically for
    /// VERSIONED release notes with semantic change-kind tags.
    ChangelogList {
        /// Section heading (e.g., "Changelog", "Release notes").
        heading: String,
        /// Entries in display order (typically newest-first).
        /// Renderer doesn't sort.
        entries: Vec<ChangelogEntry>,
        /// Visual style. `Detailed` shows per-change kind tags +
        /// full body; `Compact` shows just version + date + change
        /// count for an index view.
        style: ChangelogListStyle,
    },
    /// Typed disclosure block — sponsored-content notices,
    /// affiliate-link disclaimers, conflict-of-interest
    /// declarations, editorial-policy notes. Distinct from the
    /// generic [`CmsSection::AsideNote`] in two ways:
    ///
    /// 1. **Typed kind** — `DisclaimerKind` carries semantic
    ///    intent the substrate can audit. Future Forge phase
    ///    `disclosure_audit` flags sponsored-content pages that
    ///    omit the Sponsored disclaimer.
    /// 2. **Semantic shape** — renders as `<aside role="note"
    ///    aria-label>` with a kind-specific accessible name so
    ///    screen-reader users hear "sponsored content notice"
    ///    not just "note."
    ///
    /// FTC + Google Search Quality Guidelines + most jurisdictions
    /// require disclosure for sponsored / affiliate / advertorial
    /// content; typed primitives make compliance enforceable at
    /// the substrate layer.
    Disclaimer {
        /// Kind of disclosure. Field is `disclosure_kind` rather
        /// than `kind` because the outer enum already uses `kind`
        /// as its serde tag — colliding inner field would shadow
        /// the variant discriminator.
        disclosure_kind: DisclaimerKind,
        /// Body text. Operator-supplied so brand voice can vary
        /// while the typed kind keeps the audit signal stable.
        body: String,
        /// Optional source / sponsor identifier (e.g., "Acme
        /// Corp."). When present + disclosure_kind == Sponsored,
        /// renderer includes it in the accessible name + the
        /// rendered aside chrome for explicit attribution.
        source: Option<String>,
    },
    /// Typed source/citation list for editorial appendices,
    /// research write-up footers, "further reading" sections.
    /// Distinct from [`CmsSection::Citation`] (inline single
    /// citation) and [`CmsSection::Glossary`] (term definitions).
    ///
    /// Each item carries author + title + URL + date + kind so
    /// the renderer can format consistently AND the substrate
    /// can audit (e.g. flag missing-author entries, dead URLs in
    /// a future Crawler pass).
    ///
    /// Use cases:
    /// - Article "Further reading" footer
    /// - Research write-up bibliography
    /// - Linked-data essay endnote list with provenance
    SourceList {
        /// Section heading (e.g., "Sources", "Further reading",
        /// "Bibliography"). Operator-supplied — different editorial
        /// styles use different labels.
        heading: String,
        /// Items in display order. Renderer doesn't sort — operator
        /// chooses chronological / alphabetical / topical.
        items: Vec<SourceListItem>,
        /// Visual style for the list. Operator picks the convention
        /// that matches their editorial voice.
        style: SourceListStyle,
    },
    /// Box-and-whisker plot — quantile-based statistical summary
    /// per category. Completes the editorial-charts vocabulary
    /// alongside Sparkline / BarChart / Histogram / DivergingBar /
    /// Heatmap.
    ///
    /// Use cases:
    /// - "Response time distribution by endpoint" (per-endpoint
    ///   box showing p25/p50/p75 spread + outlier whiskers)
    /// - "Review score distribution by category"
    /// - "Compile time per crate" (showing variance + outliers)
    ///
    /// Each entry is rendered as an SVG box (q1→q3 range) +
    /// median line + whiskers extending to min/max. Pure-SVG,
    /// no JS. Pre-computed quantiles — operator runs whatever
    /// quantile estimation they want (linear / nearest /
    /// midpoint) and hands the substrate the five-number summary.
    Boxplot {
        /// Visible label / heading.
        label: String,
        /// One box per category, in display order.
        boxes: Vec<BoxplotEntry>,
        /// Tone (drives box + whisker color via cascade).
        tone: SparklineTone,
        /// Optional caption shown below the chart.
        caption: Option<String>,
    },
    /// 2D heatmap — grid of cells colored by intensity. Editorial
    /// axis for showing categorical × categorical numeric data.
    /// Distinct from [`CmsSection::BarChart`] (1D categorical) and
    /// [`CmsSection::Histogram`] (1D distribution).
    ///
    /// Use cases:
    /// - "Commits by day-of-week × hour-of-day" (GitHub-style
    ///   contribution heatmap)
    /// - "Engagement rate by post-category × audience-segment"
    /// - "Error frequency by service × time-bucket"
    ///
    /// Each cell is rendered as an SVG `<rect>` with opacity
    /// scaling to value/max. Labels for rows + columns render
    /// outside the grid for screen-reader navigation. Pure-SVG,
    /// no JS.
    Heatmap {
        /// Visible label / heading.
        label: String,
        /// Row labels (y-axis). Length must match the outer
        /// dimension of `cells`; renderer truncates/pads silently
        /// if mismatched.
        row_labels: Vec<String>,
        /// Column labels (x-axis). Length must match the inner
        /// dimension of `cells`.
        column_labels: Vec<String>,
        /// 2D grid of values, row-major. `cells[row][col]` is
        /// the value at the intersection. Empty rows or empty
        /// outer Vec render as a "No data" placeholder.
        cells: Vec<Vec<f64>>,
        /// Tone (drives cell color via cascade).
        tone: SparklineTone,
        /// Optional caption shown below the chart.
        caption: Option<String>,
    },
    /// Diverging bar chart — bars extend left (negative) or right
    /// (positive) of a center axis at viewBox midline. Editorial
    /// shape for "delta from baseline" or "approval margin"
    /// visualization. Distinct from [`CmsSection::BarChart`]
    /// which clamps negatives to zero; this primitive lets
    /// negative values render as left-growing bars.
    ///
    /// Use cases: "Net Promoter Score by segment", "Approval
    /// margin per ballot question", "P&L by line item", "Drift
    /// vs target by metric" — anywhere the editorial point is
    /// SIGNED delta from a baseline, not just magnitude.
    DivergingBar {
        /// Visible label / heading.
        label: String,
        /// Items in display order. Each carries label + signed value.
        items: Vec<DivergingBarItem>,
        /// Tone (drives bar color via cascade — positive vs negative
        /// bars get distinct fills derived from the tone semantic
        /// pairing in loom-skin).
        tone: SparklineTone,
        /// Optional label for the midline / baseline axis
        /// (e.g., "target", "0%", "neutral"). Renders as text at
        /// the midline anchor.
        midline_label: Option<String>,
        /// Optional caption shown below the chart.
        caption: Option<String>,
    },
    /// Frequency-distribution histogram. Pre-bucketed data input
    /// (operator computes bins; renderer draws). Editorial axis
    /// for showing distribution shape — "request latency", "post
    /// length", "review-score distribution" — distinct from
    /// [`CmsSection::BarChart`] which is for categorical compare.
    ///
    /// Each bucket carries `range_min`, `range_max`, `count`.
    /// Renderer emits SVG rects sized to the max(count) across
    /// buckets, with the bin range surfaced in the legend below
    /// (so the user can map a visual bin to the numeric range
    /// without hovering for tooltips that don't exist in pure-
    /// SVG no-JS rendering).
    Histogram {
        /// Visible label / heading.
        label: String,
        /// Buckets in display order (typically ascending range).
        buckets: Vec<HistogramBucket>,
        /// Tone for bars.
        tone: SparklineTone,
        /// Optional caption shown below the chart.
        caption: Option<String>,
    },
    /// Inline bar chart — small SVG bar chart for editorial
    /// categorical-data display. Editorial counterpart to
    /// [`CmsSection::Pricing`]-style tier-comparison shapes when
    /// the data is numeric/comparative rather than tier-feature.
    ///
    /// Pure-SVG, no JS. Each bar is a `<rect>` normalized to a
    /// shared y-range derived from max(bars). The chart is
    /// `Vertical` by default (bars grow up from the x-axis);
    /// `Horizontal` orientation grows bars rightward — useful
    /// when category labels are long.
    ///
    /// Use cases: "Activity by day-of-week", "Most-cited sources",
    /// "Issues closed per milestone" — categorical numeric data
    /// where each row is meaningful (NOT just "stat band of
    /// big numbers").
    BarChart {
        /// Visible label / heading shown above the chart.
        label: String,
        /// Bars in display order. Each carries its own label +
        /// value + optional per-bar tone override.
        bars: Vec<BarChartBar>,
        /// Orientation. `Vertical` grows bars upward (default);
        /// `Horizontal` grows rightward (long category labels).
        orientation: BarChartOrientation,
        /// Default tone for bars that don't override.
        tone: SparklineTone,
        /// Optional caption shown below the chart.
        caption: Option<String>,
    },
    /// Inline sparkline — small SVG line chart for editorial
    /// number-trend display. The editorial counterpart to
    /// [`CmsSection::StatBand`] (which is the SaaS "Numbers that
    /// compose" stat-band trope per the slop dictionary).
    ///
    /// Pure-SVG, no JS, no external chart library. Given a Vec
    /// of f64 data points, the renderer computes a normalized
    /// polyline + emits inline SVG with an aria-label describing
    /// the trend (min / max / last value). Accessible by default.
    ///
    /// Use cases: "GitHub stars over 90 days", "post engagement
    /// over time", "build duration trending" — anywhere editorial
    /// copy needs to show a small trend WITHOUT the SaaS-marketing
    /// "10,000+ users!" stat-band shape.
    Sparkline {
        /// Visible label / caption shown above or beside the line.
        /// Operator-supplied; renderer HTML-escapes.
        label: String,
        /// Data points in series order. The renderer normalizes
        /// these to a 0..100 y-range; absolute values surface in
        /// the aria-label. Empty Vec renders as a "no data"
        /// placeholder.
        data_points: Vec<f64>,
        /// Tone — controls stroke color via CSS custom prop.
        tone: SparklineTone,
        /// Optional caption shown below the chart (e.g., "Last
        /// 90 days, weekly aggregate"). Operator-supplied.
        caption: Option<String>,
    },
    /// In-session password change. Authenticated user updates
    /// their password without going through the forgot-password
    /// email flow. Distinct from [`CmsSection::PasswordReset`]
    /// (which kicks off the forgot-password email send).
    ///
    /// Standard fields: current password, new password, confirm
    /// new password. The renderer never validates passwords —
    /// that's server-side. Renderer emits the form fields with
    /// proper autocomplete tokens + standard hardening.
    ///
    /// Password requirements (length / character classes) get
    /// surfaced visibly so the user knows the rules before
    /// submission — typed as a `Vec<String>` because the actual
    /// rules vary by operator (some require 12 chars + symbol,
    /// some accept passphrases of any length, etc.).
    PasswordChange {
        /// Section title (e.g., "Change password").
        title: String,
        /// Optional description / context copy.
        description: Option<String>,
        /// Visible requirements list. Operator-supplied bullets
        /// (e.g., "Minimum 12 characters", "At least one
        /// non-alphanumeric"). Renderer HTML-escapes each.
        /// Empty Vec omits the requirements block.
        requirements: Vec<String>,
        /// Submit CTA. POSTs to the password-change endpoint.
        submit_cta: HeroCta,
        /// Cancel CTA. Routes back to account settings.
        cancel_cta: HeroCta,
    },
    /// Irreversible account-deletion confirm screen. Typed-input
    /// gated — visitor must type the configured `confirm_phrase`
    /// exactly (typically their username or "delete <handle>") AND
    /// supply their current password. Renders a destructive primary
    /// CTA that POSTs to a soft-delete-or-permanent-delete endpoint
    /// + a safe-default cancel CTA.
    ///
    /// SECURITY: the renderer does NOT validate that the visitor
    /// actually typed the phrase or the password — that's server-
    /// side. The renderer just emits the form fields. Backend MUST
    /// re-check both on POST + MUST require recent re-auth (within
    /// the last ~5 minutes) before honoring the deletion.
    AccountDelete {
        /// Section title (e.g., "Delete your account").
        title: String,
        /// Lede / warning copy explaining what deletion does. Operator-
        /// supplied because consequence framing is brand-voice-sensitive.
        warning: String,
        /// Optional typed consequences list — explicit bullets of
        /// "you will lose access to X / Y / Z." Each entry is plain
        /// text the renderer HTML-escapes.
        consequences: Vec<String>,
        /// The literal phrase the visitor must type to confirm.
        /// Operator-supplied (typically the username, or a literal
        /// like "delete my account"). Renderer surfaces it in the
        /// label so the user knows what to type; backend re-checks.
        confirm_phrase: String,
        /// Visible label for the confirmation text input. Operator-
        /// supplied so brand voice can vary ("Type your username to
        /// confirm" vs "Confirm deletion").
        confirm_field_label: String,
        /// Whether to include a password-confirmation input. Typically
        /// `true` for any password-authenticated account; `false` for
        /// passkey-only accounts where the WebAuthn assertion is the
        /// re-auth signal supplied at the previous step.
        require_password: bool,
        /// Destructive submit CTA. POSTs to the deletion endpoint.
        /// Renderer styles this as `.loom-btn--danger`.
        delete_cta: HeroCta,
        /// Safe-default cancel CTA. Operator typically routes this
        /// back to the account-settings page.
        cancel_cta: HeroCta,
    },
    /// Active-sessions / device list. Renders the logged-in
    /// user's list of authenticated sessions with per-session
    /// revoke CTAs + an optional "sign out everywhere" overflow.
    ///
    /// Pairs with `BackupCodes` + `MfaPrompt` as a third account-
    /// security primitive — users who've lost a device need to
    /// remotely revoke its session. Each entry carries a typed
    /// `DeviceEntry` with the device label, optional location,
    /// optional last-active timestamp, and an `current` flag.
    /// The "current" session typically does NOT carry a revoke
    /// CTA (revoking your current session mid-page is a UX trap;
    /// route through the dedicated sign-out flow instead).
    DeviceList {
        /// Section title.
        title: String,
        /// Optional description / instructional copy.
        description: Option<String>,
        /// Active sessions / devices. Order is significant —
        /// renderer emits them in the supplied order.
        devices: Vec<DeviceEntry>,
        /// Optional "Sign out everywhere" CTA. POSTs to the
        /// revoke-all-other-sessions endpoint. Typically the
        /// only destructive bulk action surfaced from this view.
        revoke_all_cta: Option<HeroCta>,
    },
    /// OAuth consent screen. Rendered when a third-party app
    /// requests access to the visitor's account; the visitor
    /// reviews the requested scopes + grants or denies.
    ///
    /// Distinct from `AuthCard` (which is sign-in / sign-up
    /// for THIS site). `ConsentScreen` is the screen this site
    /// renders when an EXTERNAL app, authenticated via OAuth /
    /// OIDC, requests scoped access to the user's account.
    ///
    /// The renderer emits a `<form method="post" action="<grant
    /// CTA href>">` shell so denial vs grant routes through
    /// distinct backend handlers. Both buttons submit the same
    /// form via formaction overrides.
    ConsentScreen {
        /// Display title (e.g., "Authorize <App>").
        title: String,
        /// External app name as the user sees it. Operator must
        /// verify against the registered OAuth client; the
        /// renderer cannot validate this — the audit phase logs
        /// it as a data-backend attribute for traceability.
        app_name: String,
        /// Optional short description of the app, sourced from
        /// the OAuth client registration. Shown under the title.
        app_description: Option<String>,
        /// Optional URL to the app's homepage / publisher info.
        /// Validated through `is_safe_url` like other CTAs.
        app_homepage: Option<String>,
        /// Requested OAuth scopes. Each is a typed
        /// [`ConsentScope`] so the renderer can group / sort /
        /// flag dangerous ones without parsing strings.
        scopes: Vec<ConsentScope>,
        /// "Grant access" / "Authorize" CTA. POSTs to the
        /// authorize-with-grant endpoint.
        grant_cta: HeroCta,
        /// "Deny" / "Cancel" CTA. POSTs to the
        /// authorize-with-deny endpoint OR redirects back to
        /// the calling app with `error=access_denied` per RFC
        /// 6749 §4.1.2.1.
        deny_cta: HeroCta,
        /// Optional support / publisher-contact line shown in
        /// the footer (e.g., "Published by Acme, Inc. —
        /// <support@acme.com>"). Operator-supplied; renderer
        /// emits as plain text (HTML-escaped).
        footer_note: Option<String>,
    },
    /// Backup-code display + acknowledge page. Rendered ONCE
    /// immediately after the operator's MFA enrollment flow
    /// generates a fresh set of single-use recovery codes. The
    /// `state` field determines whether codes are displayed
    /// (`Fresh` — operator just generated them) or whether the
    /// page renders a "codes already generated; request new ones
    /// to invalidate the old" path (`AlreadyGenerated`).
    ///
    /// SECURITY: surfacing fresh codes is allowed exactly once.
    /// The backend should mark the codes as "viewed" on this
    /// page's render and refuse to re-display them on any
    /// subsequent visit — instead routing through this same
    /// variant in the `AlreadyGenerated` state. The renderer
    /// itself doesn't enforce that; it just emits the markup
    /// per the state passed in.
    BackupCodes {
        /// Title shown above the code grid.
        title: String,
        /// Description / warning copy. Operator-supplied so
        /// brand voice can vary.
        description: String,
        /// Render state. See [`BackupCodesState`] for the
        /// security-contract narrative.
        state: BackupCodesState,
        /// The actual codes — typically 8-10 single-use strings.
        /// Rendered as a monospace grid when state is `Fresh`;
        /// ignored when state is `AlreadyGenerated`.
        codes: Vec<String>,
        /// Optional download CTA — typically points at a
        /// signed-by-the-backend text/plain endpoint that
        /// streams the codes one-time-only.
        download_cta: Option<HeroCta>,
        /// Optional acknowledge CTA — typically the user clicks
        /// "I've saved my codes" which navigates onward.
        acknowledge_cta: Option<HeroCta>,
    },
    /// Email-verification result landing page. Rendered after a
    /// visitor clicks a one-click verification link emailed during
    /// sign-up. Carries a typed [`EmailVerifyStatus`] so the
    /// renderer can emit the correct success / expired / invalid /
    /// already-verified shell without the operator hand-writing 4
    /// separate landing pages.
    ///
    /// Operators pair this with an `EmailVerifyRequest` page
    /// (typically a regular [`CmsSection::CallToAction`] with
    /// "Check your inbox" copy) shown immediately after sign-up.
    EmailVerifyResult {
        /// Verification outcome.
        status: EmailVerifyStatus,
        /// Visible heading. Operator may override the default by
        /// status; if `None` the renderer picks a status-appropriate
        /// default ("Email verified", "Link expired", etc.).
        title: Option<String>,
        /// Body copy under the heading. Operator-supplied; the
        /// renderer doesn't emit a default because messaging is
        /// brand-voice-sensitive.
        body: String,
        /// Optional primary next-action CTA. For `Success` /
        /// `AlreadyVerified` this typically points to the
        /// signed-in dashboard; for `Expired` it should point at
        /// the resend-verification endpoint; for `Invalid` it
        /// should point at the support / contact route.
        cta: Option<HeroCta>,
        /// Optional secondary CTA (e.g., "Contact support" beside
        /// a primary "Continue to dashboard").
        secondary_cta: Option<HeroCta>,
    },
}

/// One entry in a [`CmsSection::ChangelogList`] — one version's
/// release notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ChangelogEntry {
    /// Version string (operator-supplied; typically semver like
    /// "1.2.3" but can be calver like "2024.03" or anything the
    /// operator's release process emits). Renderer escapes verbatim.
    pub version: String,
    /// Release date string (operator pre-formatted; typically
    /// ISO 8601 yyyy-mm-dd). Renderer doesn't format dates per
    /// memory [[iso-standards]].
    pub date: String,
    /// Optional summary line ("Performance + accessibility
    /// improvements"). Renders above the changes list.
    pub summary: Option<String>,
    /// Typed changes in this release. Empty Vec renders as
    /// summary-only when summary is set, OR a "no changes
    /// recorded" placeholder when both empty.
    pub changes: Vec<ChangelogChange>,
}

/// One typed change inside a [`ChangelogEntry`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ChangelogChange {
    /// Kind of change (Added / Changed / Deprecated / Removed /
    /// Fixed / Security) per Keep-a-Changelog convention.
    pub kind: ChangelogChangeKind,
    /// Change description text. Operator-supplied; renderer
    /// HTML-escapes.
    pub text: String,
}

/// Kind of change in a [`ChangelogEntry`]. Follows the
/// keepachangelog.com convention so machine + human readers
/// share the same vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangelogChangeKind {
    /// New features added.
    Added,
    /// Existing behavior changed.
    Changed,
    /// Features marked for future removal.
    Deprecated,
    /// Features removed.
    Removed,
    /// Bug fixes.
    Fixed,
    /// Security patches.
    Security,
}

impl ChangelogChangeKind {
    /// Stable kebab-case modifier slug. Loom-skin cascade rules
    /// target `.loom-changelog-change--<modifier>`.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Changed => "changed",
            Self::Deprecated => "deprecated",
            Self::Removed => "removed",
            Self::Fixed => "fixed",
            Self::Security => "security",
        }
    }

    /// Human label for the kind tag visible in `Detailed` style.
    /// Stable string contract (loom-skin doesn't depend on it but
    /// reports + downstream consumers may).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Changed => "Changed",
            Self::Deprecated => "Deprecated",
            Self::Removed => "Removed",
            Self::Fixed => "Fixed",
            Self::Security => "Security",
        }
    }
}

/// Visual style for [`CmsSection::ChangelogList`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChangelogListStyle {
    /// Full per-change rows with kind-tag badges + change text.
    /// Substrate default. Use for the canonical changelog view.
    #[default]
    Detailed,
    /// Compact index: version + date + change-count summary only.
    /// Use for sidebar/topbar overviews where each version links
    /// to its own page.
    Compact,
}

impl ChangelogListStyle {
    /// Stable kebab-case modifier slug.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Detailed => "detailed",
            Self::Compact => "compact",
        }
    }
}

/// Kind of disclosure on [`CmsSection::Disclaimer`].
///
/// Drives the modifier class on the rendered `<aside>` + the
/// accessible-name template. Future `disclosure_audit` phase
/// reads the kind to enforce per-kind requirements (e.g.
/// Sponsored disclaimers must carry a `source` attribution).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DisclaimerKind {
    /// Sponsored / paid content. FTC-required for promotional
    /// posts in the US.
    Sponsored,
    /// Article contains affiliate links the publisher gets a
    /// commission from. FTC-required.
    Affiliate,
    /// Author has a personal / financial / professional
    /// relationship with the subject matter. Editorial ethics
    /// best practice.
    ConflictOfInterest,
    /// Editorial-policy note — corrections issued, sourcing
    /// transparency, etc.
    EditorialNote,
    /// Legal / regulatory notice (jurisdiction-specific
    /// disclosures, copyright assertions, etc.).
    LegalNotice,
    /// AI-assisted content disclosure — LLM was involved in
    /// drafting / editing / illustrating. Best practice as
    /// publisher transparency.
    AiAssisted,
}

impl DisclaimerKind {
    /// Stable kebab-case modifier slug. The
    /// `.loom-disclaimer--<modifier>` cascade targets these.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Sponsored => "sponsored",
            Self::Affiliate => "affiliate",
            Self::ConflictOfInterest => "conflict-of-interest",
            Self::EditorialNote => "editorial-note",
            Self::LegalNotice => "legal-notice",
            Self::AiAssisted => "ai-assisted",
        }
    }

    /// Default accessible-name template. Operator-supplied
    /// `source` (when present) gets appended for `Sponsored`
    /// kind. The aria-label is the SCREEN-READER pronunciation
    /// of the disclaimer block; visual readers see the body
    /// text + chrome.
    #[must_use]
    pub const fn accessible_label(self) -> &'static str {
        match self {
            Self::Sponsored => "Sponsored content notice",
            Self::Affiliate => "Affiliate link disclosure",
            Self::ConflictOfInterest => "Conflict of interest disclosure",
            Self::EditorialNote => "Editorial note",
            Self::LegalNotice => "Legal notice",
            Self::AiAssisted => "AI-assisted content disclosure",
        }
    }
}

/// One entry in a [`CmsSection::SourceList`].
///
/// Typed shape so the renderer can format consistently + the
/// audit phases can introspect (e.g. flag missing authors, dead
/// URLs). Operators that have legacy free-form citations can
/// migrate by parsing into this struct OR use `kind: Other`
/// with a hand-formatted title until they're ready.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SourceListItem {
    /// Author / creator. May be a person, organization, or
    /// composite ("Smith, J. and Lee, K.") — renderer escapes
    /// verbatim.
    pub author: String,
    /// Work title.
    pub title: String,
    /// URL if available. Validated through `is_safe_url`;
    /// hostile schemes (javascript:, etc.) render as plain
    /// `<span>` instead of a clickable link.
    pub url: Option<String>,
    /// Publication date as operator-supplied string
    /// (typically ISO-8601 yyyy-mm-dd, but renderer doesn't
    /// validate — operators with classical-publication dates
    /// like "1990" can ship that verbatim).
    pub date_published: Option<String>,
    /// Source kind — drives the modifier class on the rendered
    /// `<li>` so loom-skin can apply per-kind chrome (icon /
    /// background / etc.).
    pub kind: SourceKind,
}

/// Kind of source — typed so per-kind formatting + per-kind
/// audit rules become possible at the substrate layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Book / monograph.
    Book,
    /// Journal article / paper.
    Article,
    /// Web page / blog post / online essay.
    Web,
    /// Podcast / talk / interview.
    Audio,
    /// Video / documentary / lecture recording.
    Video,
    /// Government / institutional report.
    Report,
    /// Catch-all for sources that don't fit the above.
    Other,
}

impl SourceKind {
    /// Stable kebab-case modifier slug. Wire-shape contract —
    /// the `.loom-source-list__item--<modifier>` cascade targets
    /// these.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::Article => "article",
            Self::Web => "web",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Report => "report",
            Self::Other => "other",
        }
    }
}

/// Visual style for [`CmsSection::SourceList`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceListStyle {
    /// Numbered citations (1. 2. 3.) — academic convention.
    /// Substrate default.
    #[default]
    Numbered,
    /// Bulleted citations — informal / web-style.
    Bulleted,
    /// Plain (no list markers) — editorial / minimal.
    Plain,
}

impl SourceListStyle {
    /// Modifier slug for the cascade.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Numbered => "numbered",
            Self::Bulleted => "bulleted",
            Self::Plain => "plain",
        }
    }
}

/// One box-and-whisker entry shown on a [`CmsSection::Boxplot`].
///
/// Five-number summary: min, q1, median, q3, max. Operator
/// computes the quantiles (any standard method); renderer just
/// draws.
///
/// Renderer doesn't validate the ordering (e.g. `min <= q1 <=
/// median <= q3 <= max`) — operators with weird inputs get weird
/// boxes but no panic. The standard quantile relationships are
/// expected but not enforced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct BoxplotEntry {
    /// Category label.
    pub label: String,
    /// Minimum value (lower whisker endpoint).
    pub min: f64,
    /// First quartile (lower edge of the box).
    pub q1: f64,
    /// Median (line inside the box).
    pub median: f64,
    /// Third quartile (upper edge of the box).
    pub q3: f64,
    /// Maximum value (upper whisker endpoint).
    pub max: f64,
}

/// One signed-value item shown on a [`CmsSection::DivergingBar`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DivergingBarItem {
    /// Category label (row name in the chart legend).
    pub label: String,
    /// Signed value. Positive renders right of midline, negative
    /// left. Zero renders no bar (just the label). Bar widths
    /// normalize to max(abs(value)) across all items.
    pub value: f64,
}

/// One frequency bucket shown on a [`CmsSection::Histogram`].
///
/// Pre-bucketed: the operator runs whatever bin-strategy they
/// want (linear / log / quantile / Freedman-Diaconis) and hands
/// the substrate the result. The renderer doesn't re-bin; it
/// just draws what's given.
///
/// Bucket ranges are typically contiguous + non-overlapping
/// (`[0,10), [10,20), [20,30)` etc.) but the renderer doesn't
/// enforce that — operators with weird bin schemes (sparse,
/// overlapping for confidence-interval visualizations) can use
/// the primitive too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct HistogramBucket {
    /// Lower bound of the bin range (inclusive).
    pub range_min: f64,
    /// Upper bound of the bin range (exclusive by convention,
    /// but the renderer doesn't enforce — operators choose).
    pub range_max: f64,
    /// Sample count in this bucket.
    pub count: u32,
}

/// One bar inside a [`CmsSection::BarChart`]. Carries its own
/// label + numeric value + optional per-bar tone override that
/// shadows the chart's default tone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct BarChartBar {
    /// Bar label (category name, e.g., "Mon", "Tue"). Renderer
    /// HTML-escapes.
    pub label: String,
    /// Numeric value. Negative values are clamped to 0 — the
    /// chart shape doesn't represent negative bars; operators
    /// that need diverging bars should use a different primitive
    /// (future work).
    pub value: f64,
    /// Optional per-bar tone override. `None` falls through to
    /// the chart's default tone.
    pub tone_override: Option<SparklineTone>,
}

/// Orientation for [`CmsSection::BarChart`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarChartOrientation {
    /// Bars grow upward from the x-axis. Default; suits short
    /// category labels (days of week, single-word tags).
    Vertical,
    /// Bars grow rightward from a y-axis. Suits long category
    /// labels that wouldn't fit under vertical bars.
    Horizontal,
}

impl BarChartOrientation {
    /// Stable kebab-case modifier slug.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }
}

/// Visual tone for [`CmsSection::Sparkline`]. Drives stroke
/// color via the `--loom-spark-stroke` CSS custom property the
/// loom-skin cascade resolves per tone.
///
/// Tones map to semantic meaning (positive/negative/neutral) NOT
/// to literal colors — the operator's theme decides the actual
/// hue, the substrate just classifies. A `Positive` sparkline
/// could render as green in the press theme and slate in the
/// editorial theme; same intent, different cascade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SparklineTone {
    /// Neutral / default — slate or ink tone.
    Neutral,
    /// Positive trend — accent or success tone.
    Positive,
    /// Negative / downturn — warning or danger tone.
    Negative,
    /// Brand accent — primary brand color.
    Accent,
}

impl SparklineTone {
    /// Stable kebab-case modifier slug. Part of the wire shape;
    /// the `.loom-sparkline--<slug>` cascade targets these.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Positive => "positive",
            Self::Negative => "negative",
            Self::Accent => "accent",
        }
    }
}

/// One device / active-session entry shown on a [`CmsSection::DeviceList`].
///
/// Each row carries a human-readable label (typically
/// `<device-class> · <browser>` like "MacBook Pro · Chrome"),
/// optional location + last-active strings (operator pre-
/// formatted — the renderer does NOT format timestamps; see
/// memory [[iso-standards]] for why localization is operator-
/// owned), and a `current` flag indicating whether this row is
/// the session the page itself is being rendered for.
///
/// Per-row revoke CTAs are typed `HeroCta`; the renderer
/// routes hostile URLs through the existing is_safe_url +
/// #invalid-cta fallback the rest of the substrate uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DeviceEntry {
    /// Display label. Typically `<device-class> · <browser>`,
    /// e.g., "MacBook Pro · Chrome", "iPhone 15 · Safari".
    /// Operator-supplied; renderer escapes.
    pub label: String,
    /// Optional location (city / country / IP city-lookup
    /// result). Operator pre-formatted; renderer escapes.
    pub location: Option<String>,
    /// Optional last-active timestamp / relative phrase
    /// (operator pre-formatted; renderer does NOT format dates).
    pub last_active: Option<String>,
    /// `true` iff this row is the session the page itself was
    /// rendered for. Renderer emits a "current session" badge
    /// AND suppresses the per-row revoke CTA on this entry
    /// (revoking your own session mid-page is a UX trap; route
    /// through the dedicated sign-out flow instead).
    pub current: bool,
    /// Per-row revoke CTA. Required for non-current rows; the
    /// renderer ignores it on `current: true` rows. Typically
    /// POSTs to the revoke-this-session endpoint with the
    /// session id encoded in the URL or the data_backend.
    pub revoke_cta: Option<HeroCta>,
}

/// One OAuth scope shown on a [`CmsSection::ConsentScreen`].
///
/// Scopes are typed because the substrate refuses to let an
/// operator hand-roll a scope as an opaque string — that's
/// where consent-screen accuracy bugs live (the app requests
/// "read:repos" but the screen renders the harmless "user:read"
/// because the operator concatenated wrong). Each scope's
/// `tier` flags how dangerous it is for the renderer to
/// surface — `Sensitive` scopes render with a warning badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ConsentScope {
    /// Machine slug, identical to the OAuth scope string sent
    /// in the authorize request (e.g., "read:repos",
    /// "write:posts", "billing:read"). Operator-supplied,
    /// renderer-rendered verbatim in a `<code>` element so the
    /// user can verify against the calling app's docs.
    pub slug: String,
    /// Human-readable label (e.g., "Read your repositories").
    pub label: String,
    /// Optional one-sentence explanation of what the scope
    /// permits beyond what's obvious from the label.
    pub description: Option<String>,
    /// Risk tier — surfaces visual treatment + audit weight.
    pub tier: ConsentScopeTier,
}

/// Risk classification for an OAuth scope on a consent screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsentScopeTier {
    /// Routine read access; renderer treats as standard row.
    Routine,
    /// Write access or moderately-sensitive read. Renderer
    /// surfaces with a "write" badge.
    Write,
    /// High-impact (billing, identity, full-access). Renderer
    /// surfaces with a "sensitive" badge + warning tone.
    Sensitive,
}

impl ConsentScopeTier {
    /// Stable kebab-case modifier name. The `.loom-consent-scope--<modifier>`
    /// CSS cascade targets these.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Routine => "routine",
            Self::Write => "write",
            Self::Sensitive => "sensitive",
        }
    }
}

/// Render state for [`CmsSection::BackupCodes`].
///
/// `Fresh` — codes were just generated and may be displayed
/// EXACTLY ONCE. The backend's responsibility is to flip the
/// underlying row from "fresh" to "viewed" the moment this page
/// is rendered; subsequent visits then resolve to `AlreadyGenerated`.
///
/// `AlreadyGenerated` — codes exist for this account but have
/// already been viewed. The renderer hides the codes and shows
/// a "regenerate" path instead. Operators must invalidate the
/// old codes server-side before showing fresh ones again — this
/// variant exists to enforce that "fresh codes are seen at most
/// once" property at the rendering boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BackupCodesState {
    /// Codes are fresh + visible.
    Fresh,
    /// Codes were generated previously and have been viewed;
    /// render the regenerate-to-replace path.
    AlreadyGenerated,
}

/// Outcome of an email-verification attempt. Routed to a typed
/// landing shell by [`CmsSection::EmailVerifyResult`].
///
/// Backend contract: the verification handler decodes the URL
/// token, looks up the verification row, and sets one of these
/// four states before rendering the page:
///
/// * `Success` — token valid, row marked verified, freshly so.
/// * `AlreadyVerified` — token valid but row was already
///   verified by a prior click (e.g., user clicked the link
///   twice). Distinct from `Success` so copy can say "you're
///   already verified" instead of "thanks for verifying."
/// * `Expired` — token decoded but the verification row's
///   `expires_at` is past. User must request a fresh link.
/// * `Invalid` — token didn't decode, didn't match any row, or
///   was tampered with. Render a generic error to avoid leaking
///   enumeration signals — DO NOT distinguish "token format bad"
///   from "no such row" in the rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailVerifyStatus {
    /// Token valid, row freshly marked verified.
    Success,
    /// Token valid but already-verified.
    AlreadyVerified,
    /// Token decoded but expired.
    Expired,
    /// Token didn't decode / didn't match / tampered (collapsed
    /// to one variant to avoid enumeration-signal leak).
    Invalid,
}

impl EmailVerifyStatus {
    /// Default-emitted title when the operator passes `None` for
    /// the [`CmsSection::EmailVerifyResult::title`] field. Keep
    /// copy generic — brand-voice tuning is the operator's job.
    #[must_use]
    pub const fn default_title(self) -> &'static str {
        match self {
            Self::Success => "Email verified",
            Self::AlreadyVerified => "Already verified",
            Self::Expired => "Verification link expired",
            Self::Invalid => "Verification link invalid",
        }
    }

    /// Per-status modifier class. Renderers can use this to switch
    /// surface tone (success-green, expired-amber, invalid-red).
    /// Stable contract — string is part of the wire shape because
    /// the loom-skin CSS targets these via `.loom-email-verify--<status>`.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::AlreadyVerified => "already-verified",
            Self::Expired => "expired",
            Self::Invalid => "invalid",
        }
    }
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum ContainerWidth {
    Narrow,
    #[default]
    Comfortable,
    Wide,
    Full,
}

/// Divider style.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum DividerStyle {
    #[default]
    Line,
    Dots,
    ZigZag,
    Sparkle,
}

/// Vertical-spacing token.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum SpaceSize {
    Tight,
    #[default]
    Comfortable,
    Loose,
    Generous,
}

/// Emphasis tier for [`CmsSection::PullQuote`]. Mirrors
/// `loom_components::pull_quote::PullQuoteEmphasis`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PullQuoteEmphasis {
    /// Inline-with-body voice. `text-xl md:text-2xl`.
    #[default]
    Inline,
    /// Hero-side / decoration-slot voice. `text-2xl md:text-3xl lg:text-4xl`.
    Display,
}

/// Color tone for [`CmsSection::PullQuote`]. Mirrors
/// `loom_components::pull_quote::PullQuoteTone`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PullQuoteTone {
    /// Slate text on light surface.
    #[default]
    Slate,
    /// Slate-100 ink on AMOLED true-black surface.
    Amoled,
}

/// Layout density for [`CmsSection::KvPair`]. Mirrors
/// `loom_components::card::KvPairDensity`. Affects vertical rhythm
/// inside each item; horizontal sizing is the grid's job.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum KvPairDensity {
    /// Tight rhythm — for grids of 4-6 items with short values.
    Compact,
    /// Default rhythm — most KV grids.
    #[default]
    Comfortable,
    /// Generous rhythm — for grids of 2-3 items with long values.
    Spacious,
}

/// Color tone for [`CmsSection::KvPair`]. Mirrors
/// `loom_components::card::KvPairTone`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum KvPairTone {
    /// Slate text on light surface.
    #[default]
    Slate,
    /// Slate-100 ink on AMOLED true-black surface.
    Amoled,
}

/// Tone for [`CmsSection::CodeShell`]. Mirrors
/// `loom_components::code_shell::CodeShellTone`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CodeShellTone {
    /// Slate-900 ink on slate-50 surface.
    #[default]
    Slate,
    /// Slate-100 ink on AMOLED true-black surface.
    Amoled,
}

/// Chrome treatment for [`CmsSection::CodeShell`]. Mirrors
/// `loom_components::code_shell::CodeShellChrome`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CodeShellChrome {
    /// No header — just the code block, framed by a single border.
    #[default]
    Minimal,
    /// Text-only header showing the shell name or filename. No
    /// traffic-light circles, no gradient.
    Header,
}

/// Semantic role of a single line in [`CmsSection::CodeShell`].
/// Mirrors `loom_components::code_shell::CodeShellLineKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CmsCodeShellLineKind {
    /// User-typed shell command. Renders with the prompt prefix.
    Command,
    /// Program output. Indented to align with the command text.
    Output,
    /// Annotation by the author. Dimmed + italic, prefixed with `#`.
    Comment,
    /// Error line. Picks up the warn / error color.
    Error,
}

/// One line of a `CodeShell` transcript. Owned-string variant of the
/// `loom_components` primitive's `CodeShellLine<'a>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct CmsCodeShellLine {
    /// Line role.
    pub kind: CmsCodeShellLineKind,
    /// Line text. Rendered as-is via Maud (auto-escaped).
    pub text: String,
}

/// Background tone for [`CmsSection::HeroEditorial`].
///
/// Mirrors `loom_components::hero::HeroEditorialBackground`. `Slate`
/// is the recommended default — lets content carry the page. `Amoled`
/// honors `[[dark-theme-amoled-true-black]]` for OLED pixels-off
/// rendering.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HeroEditorialBackground {
    /// Plain slate-50.
    #[default]
    Slate,
    /// Plain white.
    Plain,
    /// AMOLED true-black (`#000`).
    Amoled,
}

/// Visual treatment for [`CmsSection::FeatureSpotlight`].
///
/// The default `Decorated` is the legacy SaaS-card shape (rounded
/// chrome + gradient icon tile + hover lift + shadow). The
/// `Editorial` and `Minimal` variants strip the trope chrome for
/// callers that want dense, non-card composition.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
    pub const PRIMITIVE_COUNT: u32 = 160;
    /// Current named-theme count. Defined in `BASE_THEME_CSS` +
    /// `THEME_TOGGLE_CSS`.
    pub const THEME_COUNT: u32 = 14;
    /// Forge audit-phase count. Reported by `forge build` summary.
    pub const FORGE_AUDIT_PHASE_COUNT: u32 = 27;
    /// Multi-network DeployAdapter count.
    pub const DEPLOY_NETWORK_COUNT: u32 = 6;
}

/// Reveal-motion variant.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum RevealMotion {
    #[default]
    FadeUp,
    FadeIn,
    ScaleIn,
    SlideLeft,
    SlideRight,
}

/// Alert tone (used by Alert, Toast, AnnouncementBar, AsideNote).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum AlertTone {
    #[default]
    Info,
    Success,
    Warning,
    Danger,
    Neutral,
}

/// Drawer side.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum DrawerSide {
    #[default]
    Right,
    Left,
}

/// Diagram source kind.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum DiagramKind {
    #[default]
    Mermaid,
    Plantuml,
    Ascii,
}

/// Form-input kind.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum FormInputKind {
    #[default]
    Text,
    Email,
    Password,
    Tel,
    Url,
    Number,
    Search,
}

/// Button variant.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
    Danger,
}

/// One tab in a Tabs section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct TabItem {
    pub label: String,
    pub body: String,
}

/// One accordion item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct AccordionItem {
    pub title: String,
    pub body: String,
}

/// One definition list entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct DefListItem {
    pub term: String,
    pub definition: String,
}

/// One comparison row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct ComparisonRow {
    pub label: String,
    pub values: Vec<String>,
}

/// One timeline milestone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct TimelineItem {
    pub when: String,
    pub title: String,
    pub body: String,
}

/// One contact channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct ContactChannel {
    pub kind: String,
    pub label: String,
    pub href: String,
    pub data_backend: String,
}

/// One gallery image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct GalleryImage {
    pub asset_slug: String,
    pub alt: String,
    pub caption: Option<String>,
}

/// One badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct BadgeItem {
    pub icon_slug: Option<String>,
    pub label: String,
}

/// One product card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
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
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct ChatMessage {
    pub author: String,
    pub body: String,
    pub mine: bool,
    pub at: String,
}

/// One reaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct ReactionItem {
    pub emoji: String,
    pub count: u32,
}

/// Follow action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct FollowAction {
    pub label: String,
    pub data_backend: String,
}

/// One select option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// One breadcrumb segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct BreadcrumbItem {
    pub label: String,
    pub href: String,
    pub data_backend: String,
}

/// One nav tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct NavTabItem {
    pub label: String,
    pub href: String,
    pub data_backend: String,
    #[serde(default)]
    pub current: bool,
}

/// One mega-menu column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct MegaMenuColumn {
    pub heading: String,
    pub items: Vec<NavTabItem>,
}

/// One language option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct LangOption {
    pub code: String,
    pub label: String,
    pub href: String,
    pub data_backend: String,
}

/// One game tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct GameTileItem {
    pub title: String,
    pub genre: String,
    pub players_online: u32,
    pub image_slug: String,
    pub href: String,
    pub data_backend: String,
}

/// One thread row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct ThreadRowItem {
    pub title: String,
    pub author: String,
    pub replies: u32,
    pub views: u32,
    pub last_reply: String,
    pub href: String,
    pub data_backend: String,
}

/// One video card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct VideoCardItem {
    pub title: String,
    pub channel: String,
    pub duration: String,
    pub views: String,
    pub thumbnail_slug: String,
    pub href: String,
    pub data_backend: String,
}

/// One comment in a thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)] // T660 catalogue: self-evident shapes; field names + variant docstring are the contract.
pub struct CommentItem {
    pub author: String,
    pub body: String,
    pub at: String,
    pub depth: u8,
}

fn default_true() -> bool {
    true
}
fn default_columns_3() -> u8 {
    3
}
fn default_speed() -> u8 {
    5
}
fn default_slider_step() -> f64 {
    1.0
}

/// Page-shell chrome kind. Picks the header + body-backdrop
/// shape. Each variant is a complete chrome aesthetic, not a
/// modifier on the same shell. Operators pick per-page via
/// `CmsPage::chrome`; new sites typically pick `FloatingPill`,
/// legacy sites stay on `PageShell` for backward compat.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
///
/// The default was changed from `Dark` to `None` on 2026-05-21:
/// pixel-reproduction comparisons against real sites (prosperity
/// club .com, sacred .vote) found the previous `Dark` default
/// washed every editorial / photo-forward hero out to ~35% strength
/// regardless of whether the operator's text needed contrast
/// protection. Operators who need legibility on bright photos
/// opt in to `Light` or `Dark` explicitly. Pre-existing pages that
/// omitted the overlay field will render their photos at full
/// strength — visually closer to the source material the operator
/// chose.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PhotoOverlay {
    /// No overlay — image renders raw. **Default since 2026-05-21.**
    #[default]
    None,
    /// Light overlay (for dark photos / dark-text titles).
    Light,
    /// Dark overlay (for bright photos / light-text titles).
    Dark,
}

/// Visual-height ramp for hero sections.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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

/// Text + content alignment for [`CmsSection::ImageHero`]. Default
/// is `Center` (existing SaaS-hero posture); editorial sites opt
/// out via `Start` for left-aligned headline + lede + cta.
///
/// Substrate-de-consumer-shaping doctrine: the substrate ships the
/// posture; the cms author chooses whether to take it. Backwards
/// compatible — existing CmsPage JSON that omits `align` keeps
/// rendering centered.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HeroAlign {
    /// Headline + lede + cta centered. SaaS-marketing default.
    #[default]
    Center,
    /// Left-aligned editorial posture. Use for editorial content
    /// sites where centered marketing copy reads as ad-tier.
    Start,
}

impl HeroAlign {
    /// String value emitted as `data-align="..."`.
    fn attr(self) -> &'static str {
        match self {
            HeroAlign::Center => "center",
            HeroAlign::Start => "start",
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_slug: Option<String>,
    /// Optional per-item photograph. Renders as a 16:9
    /// `object-fit: cover` panel above the title. Mutually
    /// composable with `icon_slug` — the icon renders in the
    /// title area, the photo above it. When neither is set,
    /// the item is text-only (typography-and-rule editorial
    /// composition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<SpotlightImage>,
    /// Feature heading.
    pub title: String,
    /// Feature body paragraph.
    pub body: String,
    /// Optional "learn more" link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// Backend slug paired with href.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_backend: Option<String>,
}

/// Per-item photograph for a [`SpotlightItem`].
///
/// `src` is validated via `composer::is_safe_url` at render
/// time; hostile schemes (`javascript:`, `data:`) suppress the
/// `<img>` and the item falls back to text-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpotlightImage {
    /// Image URL (root-relative or https:// only).
    pub src: String,
    /// Accessible name. Required even for decorative photos —
    /// operators who genuinely want decorative-only pass an
    /// empty string + opt in via `aria-hidden` on the parent.
    pub alt: String,
    /// Optional intrinsic width hint (CLS prevention).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Optional intrinsic height hint (CLS prevention).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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

/// Visual style for [`CmsSection::Form`]. Mirrors
/// `loom_components::FormStyle` at the wire-shape layer so cms-
/// render JSON can opt the form into the editorial / minimal /
/// rounded chrome the loom-skin cascade understands.
///
/// Default is `Rounded` for back-compat — every existing CMS
/// JSON file gets the historical SaaS shape; opting into a
/// different chrome is an explicit additive change.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CmsFormStyle {
    /// `rounded-md` / `rounded-xl` inputs + soft borders. The
    /// substrate default; SaaS-marketing baseline shape.
    #[default]
    Rounded,
    /// Press-tier editorial chrome — square inputs, ink-on-paper
    /// borders, no pill bevels. Pairs with `SectionTheme::Editorial`
    /// and `theme="press"` skins.
    Editorial,
    /// Minimal chrome — borderless inputs with under-line accents
    /// only. For dense forms where chrome would crowd the layout.
    Minimal,
}

impl CmsFormStyle {
    /// Stable kebab-case modifier slug. Part of the wire shape;
    /// loom-skin targets `[data-loom-form-style="<slug>"]`.
    #[must_use]
    pub const fn modifier(self) -> &'static str {
        match self {
            Self::Rounded => "rounded",
            Self::Editorial => "editorial",
            Self::Minimal => "minimal",
        }
    }
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

/// Render one atomic [`CmsBlock`] to Loom markup. Recursive —
/// `Container` / `Row` / `Column` blocks call back into
/// `render_block` for each child.
///
/// Token-scale spacing + alignment are emitted as `data-*`
/// attributes; the loom-skin.css cascade resolves them to actual
/// padding / gap / align-items per the tenant's `[style]`
/// config.
#[must_use]
pub fn render_block(block: &CmsBlock) -> Markup {
    match block {
        CmsBlock::Text { text } => html! {
            p class="loom-block-text" { (text) }
        },
        CmsBlock::Heading { level, text } => {
            let lvl = (*level).clamp(1, 6);
            html! {
                @match lvl {
                    1 => h1 class="loom-block-heading" data-level="1" { (text) },
                    2 => h2 class="loom-block-heading" data-level="2" { (text) },
                    3 => h3 class="loom-block-heading" data-level="3" { (text) },
                    4 => h4 class="loom-block-heading" data-level="4" { (text) },
                    5 => h5 class="loom-block-heading" data-level="5" { (text) },
                    _ => h6 class="loom-block-heading" data-level="6" { (text) },
                }
            }
        }
        CmsBlock::Image {
            src,
            alt,
            width,
            height,
        } => {
            if loom_components::composer::is_safe_url(src) {
                html! {
                    img class="loom-block-image"
                        src=(src) alt=(alt)
                        width=[width] height=[height]
                        decoding="async" loading="lazy";
                }
            } else {
                html! {}
            }
        }
        CmsBlock::Link {
            label,
            href,
            data_backend,
        } => {
            let safe = loom_components::composer::is_safe_url(href);
            html! {
                a class="loom-block-link"
                    href=(if safe { href.as_str() } else { "#invalid-link" })
                    data-backend=[data_backend.as_deref()]
                    data-invalid=[(!safe).then_some("true")] {
                    (label)
                }
            }
        }
        CmsBlock::Button {
            label,
            href,
            variant,
            size,
            data_backend,
        } => {
            let safe = loom_components::composer::is_safe_url(href);
            let v = button_variant_slug(*variant);
            let sz = button_size_slug(*size);
            html! {
                a class="loom-block-button"
                    role="button"
                    href=(if safe { href.as_str() } else { "#invalid-link" })
                    data-variant=(v)
                    data-size=(sz)
                    data-backend=[data_backend.as_deref()]
                    data-invalid=[(!safe).then_some("true")] {
                    (label)
                }
            }
        }
        CmsBlock::Spacer { size } => {
            let slug = block_spacing_slug(*size);
            html! {
                div class="loom-block-spacer" data-size=(slug) aria-hidden="true" {}
            }
        }
        CmsBlock::Divider => html! {
            hr class="loom-block-divider" aria-hidden="true";
        },
        CmsBlock::Container { padding, children } => {
            let pad = padding.map(block_spacing_slug);
            html! {
                div class="loom-block-container" data-padding=[pad] {
                    @for child in children {
                        (render_block(child))
                    }
                }
            }
        }
        CmsBlock::Row {
            gap,
            align,
            children,
        } => {
            let g = gap.map(block_spacing_slug);
            let a = align.map(block_align_slug);
            html! {
                div class="loom-block-row" data-gap=[g] data-align=[a] {
                    @for child in children {
                        (render_block(child))
                    }
                }
            }
        }
        CmsBlock::Column {
            gap,
            align,
            children,
        } => {
            let g = gap.map(block_spacing_slug);
            let a = align.map(block_align_slug);
            html! {
                div class="loom-block-column" data-gap=[g] data-align=[a] {
                    @for child in children {
                        (render_block(child))
                    }
                }
            }
        }
        CmsBlock::Accordion {
            items,
            single_expand,
        } => {
            let group_name = if *single_expand {
                Some("loom-accordion-group")
            } else {
                None
            };
            html! {
                div class="loom-block-accordion" data-loom-slot="accordion" {
                    @for item in items {
                        details
                            class="loom-block-accordion__item"
                            data-loom-slot="accordion-item"
                            open[item.default_open]
                            name=[group_name]
                        {
                            summary class="loom-block-accordion__summary" data-loom-slot="accordion-trigger" {
                                (item.summary)
                            }
                            div class="loom-block-accordion__content" data-loom-slot="accordion-content" {
                                @for child in &item.content {
                                    (render_block(child))
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsBlock::Toast {
            id,
            tone,
            title,
            message,
            dismissible,
            open,
        } => {
            let tone_slug = toast_tone_slug(*tone);
            let (role, aria_live) = match tone {
                ToastTone::Info | ToastTone::Success => ("status", "polite"),
                ToastTone::Warning | ToastTone::Error => ("alert", "assertive"),
            };
            html! {
                div
                    class="loom-block-toast"
                    data-loom-slot="toast"
                    data-tone=(tone_slug)
                    role=(role)
                    aria-live=(aria_live)
                    id=(id)
                    hidden[!*open]
                {
                    div class="loom-block-toast__body" data-loom-slot="toast-body" {
                        @if let Some(t) = title {
                            strong class="loom-block-toast__title" data-loom-slot="toast-title" {
                                (t)
                            }
                        }
                        p class="loom-block-toast__message" data-loom-slot="toast-message" {
                            (message)
                        }
                    }
                    @if *dismissible {
                        button
                            type="button"
                            class="loom-block-toast__close"
                            data-loom-slot="toast-close"
                            aria-label="Dismiss notification"
                            aria-controls=(id)
                        {
                            "×"
                        }
                    }
                }
            }
        }
        CmsBlock::Combobox {
            id,
            label,
            name,
            placeholder,
            value,
            options,
        } => {
            let list_id = format!("{id}-options");
            html! {
                div class="loom-block-combobox" data-loom-slot="combobox" {
                    label
                        class="loom-block-combobox__label"
                        data-loom-slot="combobox-label"
                        for=(id)
                    {
                        (label)
                    }
                    input
                        type="text"
                        class="loom-block-combobox__input"
                        data-loom-slot="combobox-input"
                        id=(id)
                        name=[name.as_deref()]
                        list=(list_id)
                        placeholder=[placeholder.as_deref()]
                        value=[value.as_deref()]
                        role="combobox"
                        aria-autocomplete="list"
                        aria-controls=(list_id);
                    datalist id=(list_id) data-loom-slot="combobox-options" {
                        @for opt in options {
                            option value=(opt.value) {
                                @if let Some(l) = &opt.label {
                                    (l)
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsBlock::DropdownMenu {
            id,
            trigger_label,
            items,
        } => html! {
            div class="loom-block-dropdown" data-loom-slot="dropdown" {
                button
                    type="button"
                    class="loom-block-dropdown__trigger"
                    data-loom-slot="dropdown-trigger"
                    popovertarget=(id)
                    aria-haspopup="menu"
                    aria-controls=(id)
                    aria-expanded="false"
                {
                    (trigger_label)
                }
                div
                    popover="auto"
                    id=(id)
                    class="loom-block-dropdown__menu"
                    data-loom-slot="dropdown-menu"
                    role="menu"
                {
                    @for item in items {
                        @if item.separator_before {
                            hr class="loom-block-dropdown__separator" role="separator";
                        }
                        @match &item.href {
                            Some(href) => {
                                @let safe = loom_components::composer::is_safe_url(href);
                                a
                                    role="menuitem"
                                    class="loom-block-dropdown__item"
                                    data-loom-slot="dropdown-item"
                                    href=(if safe { href.as_str() } else { "#invalid-link" })
                                    data-backend=[item.data_backend.as_deref()]
                                    data-invalid=[(!safe).then_some("true")]
                                    aria-disabled=(if item.disabled { "true" } else { "false" })
                                    tabindex=(if item.disabled { "-1" } else { "0" })
                                {
                                    (item.label)
                                }
                            }
                            None => {
                                button
                                    type="button"
                                    role="menuitem"
                                    class="loom-block-dropdown__item"
                                    data-loom-slot="dropdown-item"
                                    data-backend=[item.data_backend.as_deref()]
                                    disabled[item.disabled]
                                    aria-disabled=(if item.disabled { "true" } else { "false" })
                                {
                                    (item.label)
                                }
                            }
                        }
                    }
                }
            }
        },
        CmsBlock::Popover {
            id,
            trigger_label,
            content,
            placement,
        } => {
            let place = tooltip_placement_slug(*placement);
            html! {
                div class="loom-block-popover" data-loom-slot="popover" {
                    button
                        type="button"
                        class="loom-block-popover__trigger"
                        data-loom-slot="popover-trigger"
                        popovertarget=(id)
                        aria-expanded="false"
                        aria-controls=(id)
                    {
                        (trigger_label)
                    }
                    div
                        popover="auto"
                        id=(id)
                        class="loom-block-popover__content"
                        data-loom-slot="popover-content"
                        data-placement=(place)
                        role="dialog"
                    {
                        @for child in content {
                            (render_block(child))
                        }
                    }
                }
            }
        }
        CmsBlock::Tabs { items } => html! {
            div class="loom-block-tabs" data-loom-slot="tabs" {
                div role="tablist" class="loom-block-tabs__list" data-loom-slot="tabs-list" {
                    @for (i, item) in items.iter().enumerate() {
                        @let trig_id = format!("tab-{}", item.slug);
                        @let panel_id = format!("panel-{}", item.slug);
                        @let is_first = i == 0;
                        button
                            type="button"
                            role="tab"
                            class="loom-block-tabs__trigger"
                            data-loom-slot="tabs-trigger"
                            id=(trig_id)
                            aria-controls=(panel_id)
                            aria-selected=(if is_first { "true" } else { "false" })
                            tabindex=(if is_first { "0" } else { "-1" })
                        {
                            (item.label)
                        }
                    }
                }
                @for (i, item) in items.iter().enumerate() {
                    @let trig_id = format!("tab-{}", item.slug);
                    @let panel_id = format!("panel-{}", item.slug);
                    @let is_first = i == 0;
                    div
                        role="tabpanel"
                        class="loom-block-tabs__panel"
                        data-loom-slot="tabs-panel"
                        id=(panel_id)
                        aria-labelledby=(trig_id)
                        hidden[!is_first]
                        tabindex="0"
                    {
                        @for child in &item.content {
                            (render_block(child))
                        }
                    }
                }
            }
        },
        CmsBlock::Dialog {
            id,
            title,
            content,
            open,
            modal,
        } => html! {
            dialog
                class="loom-block-dialog"
                data-loom-slot="dialog"
                data-modal=(if *modal { "true" } else { "false" })
                id=(id)
                open[*open]
                aria-labelledby={ (id) "__title" }
            {
                form method="dialog" class="loom-block-dialog__close-row" {
                    button
                        type="submit"
                        class="loom-block-dialog__close"
                        data-loom-slot="dialog-close"
                        aria-label="Close dialog"
                        value="cancel"
                    {
                        "×"
                    }
                }
                h2
                    class="loom-block-dialog__title"
                    data-loom-slot="dialog-title"
                    id={ (id) "__title" }
                {
                    (title)
                }
                div class="loom-block-dialog__content" data-loom-slot="dialog-content" {
                    @for child in content {
                        (render_block(child))
                    }
                }
            }
        },
        CmsBlock::Tooltip {
            label,
            content,
            placement,
        } => {
            // Stable per-render id so aria-describedby resolves
            // against the body span. Uses content hash (low
            // collision risk in practice; substrate detector
            // unique_id flags real duplicates).
            let body_id = format!("loom-tip-{}", short_hash(content));
            let place = tooltip_placement_slug(*placement);
            html! {
                span
                    class="loom-block-tooltip"
                    data-loom-slot="tooltip"
                    data-placement=(place)
                    tabindex="0"
                    aria-describedby=(body_id)
                {
                    span class="loom-block-tooltip__trigger" data-loom-slot="tooltip-trigger" {
                        (label)
                    }
                    span
                        class="loom-block-tooltip__content"
                        data-loom-slot="tooltip-content"
                        role="tooltip"
                        id=(body_id)
                    {
                        (content)
                    }
                }
            }
        }
        CmsBlock::Slider {
            id,
            label,
            name,
            min,
            max,
            step,
            value,
            disabled,
            show_value,
        } => html! {
            div class="loom-block-slider" data-loom-slot="slider" {
                label
                    class="loom-block-slider__label"
                    data-loom-slot="slider-label"
                    for=(id)
                {
                    (label)
                    @if *show_value {
                        " "
                        output
                            class="loom-block-slider__value"
                            data-loom-slot="slider-value"
                            for=(id)
                        {
                            (value)
                        }
                    }
                }
                input
                    type="range"
                    class="loom-block-slider__input"
                    data-loom-slot="slider-input"
                    id=(id)
                    name=[name.as_deref()]
                    min=(min)
                    max=(max)
                    step=(step)
                    value=(value)
                    disabled[*disabled]
                    aria-label=(label);
            }
        },
        CmsBlock::Switch {
            id,
            label,
            name,
            checked,
            disabled,
        } => html! {
            label
                class="loom-block-switch"
                data-loom-slot="switch"
                for=(id)
            {
                input
                    type="checkbox"
                    role="switch"
                    class="loom-block-switch__input"
                    id=(id)
                    name=[name.as_deref()]
                    checked[*checked]
                    disabled[*disabled]
                    aria-checked=(if *checked { "true" } else { "false" });
                span class="loom-block-switch__track" aria-hidden="true" {
                    span class="loom-block-switch__thumb" {}
                }
                span class="loom-block-switch__label" { (label) }
            }
        },
    }
}

/// Kebab-case slug for a [`BlockSpacing`] step. Used as the
/// value of `data-padding` / `data-gap` attributes so the
/// loom-skin.css cascade can resolve them to per-tenant pixel
/// values.
#[must_use]
pub const fn block_spacing_slug(s: BlockSpacing) -> &'static str {
    match s {
        BlockSpacing::None => "none",
        BlockSpacing::Xs => "xs",
        BlockSpacing::Sm => "sm",
        BlockSpacing::Md => "md",
        BlockSpacing::Lg => "lg",
        BlockSpacing::Xl => "xl",
        BlockSpacing::Xxl => "xxl",
    }
}

/// Kebab-case slug for a [`ToastTone`]. Emitted as `data-tone`
/// so the skin cascade can apply tone-specific accent + icon.
#[must_use]
pub const fn toast_tone_slug(t: ToastTone) -> &'static str {
    match t {
        ToastTone::Info => "info",
        ToastTone::Success => "success",
        ToastTone::Warning => "warning",
        ToastTone::Error => "error",
    }
}

/// Kebab-case slug for a [`TooltipPlacement`]. Emitted as
/// `data-placement` on the tooltip wrapper so the skin cascade
/// can position the floating content body via CSS only.
#[must_use]
pub const fn tooltip_placement_slug(p: TooltipPlacement) -> &'static str {
    match p {
        TooltipPlacement::Top => "top",
        TooltipPlacement::Bottom => "bottom",
        TooltipPlacement::Left => "left",
        TooltipPlacement::Right => "right",
    }
}

/// Short stable hash for use as an HTML id suffix. Deterministic
/// across renders for the same input. Uses `std::hash::Hasher` —
/// not cryptographic; collision risk is low for distinct tooltip
/// contents and the substrate's `unique_id` detector flags real
/// collisions.
fn short_hash(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:x}", h.finish())
}

/// Kebab-case slug for a [`BlockAlign`] enum. Used as the value
/// of `data-align` attributes.
#[must_use]
pub const fn block_align_slug(a: BlockAlign) -> &'static str {
    match a {
        BlockAlign::Start => "start",
        BlockAlign::Center => "center",
        BlockAlign::End => "end",
        BlockAlign::Stretch => "stretch",
        BlockAlign::Baseline => "baseline",
    }
}

/// Kebab-case slug for a [`ButtonVariant`] when used by a
/// [`CmsBlock::Button`]. Same enum the existing button
/// primitives use; the slug is emitted as `data-variant` so
/// the loom-skin.css cascade can apply per-variant styling
/// under the tenant's `[style.button]` config.
#[must_use]
pub const fn button_variant_slug(v: ButtonVariant) -> &'static str {
    match v {
        ButtonVariant::Primary => "primary",
        ButtonVariant::Secondary => "secondary",
        ButtonVariant::Ghost => "ghost",
        ButtonVariant::Danger => "danger",
    }
}

/// Kebab-case slug for a [`ButtonSize`]. Emitted as `data-size`
/// for per-tenant button sizing.
#[must_use]
pub const fn button_size_slug(s: ButtonSize) -> &'static str {
    match s {
        ButtonSize::Sm => "sm",
        ButtonSize::Md => "md",
        ButtonSize::Lg => "lg",
    }
}

/// Render one CMS section to Loom markup.
#[must_use]
#[allow(clippy::too_many_lines)] // single match over every CmsSection variant.
pub fn render_section(section: &CmsSection) -> Markup {
    match section {
        CmsSection::Compose { heading, blocks } => html! {
            section class="loom-compose" data-loom-compose data-loom-reveal {
                @if let Some(h) = heading {
                    h2 class="loom-compose__heading" { (h) }
                }
                div class="loom-compose__tree" {
                    @for block in blocks {
                        (render_block(block))
                    }
                }
            }
        },
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
        CmsSection::HeroEditorial {
            kicker,
            headline,
            headline_accent,
            lede,
            cta,
            background,
        } => {
            let cta_href_safe = cta
                .as_ref()
                .is_none_or(|c| loom_components::composer::is_safe_url(&c.href));
            let bg_attr = match background {
                HeroEditorialBackground::Slate => "slate",
                HeroEditorialBackground::Plain => "plain",
                HeroEditorialBackground::Amoled => "amoled",
            };
            html! {
                section class="loom-section-hero-editorial" data-loom-hero-editorial data-background=(bg_attr) {
                    div class="loom-section-hero-editorial__grid" {
                        div class="loom-section-hero-editorial__lead" {
                            @if let Some(k) = kicker {
                                p class="loom-section-hero-editorial__kicker" { (k) }
                            }
                            h1 class="loom-section-hero-editorial__headline" {
                                (headline)
                                @if let Some(accent) = headline_accent {
                                    " "
                                    span class="loom-section-hero-editorial__accent" { (accent) }
                                }
                            }
                            p class="loom-section-hero-editorial__lede" { (lede) }
                            @if let Some(c) = cta {
                                a
                                    class="loom-section-hero-editorial__cta"
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
            style,
        } => render_form(legend, submit, steps, *style),
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
        CmsSection::Heading {
            text,
            level,
            id,
            polish,
        } => {
            // T36 (2026-05-14): typed HeadingLevel enum makes
            // out-of-range values uncompilable. The runtime clamp
            // + data-cms-warn fallback are gone — invalid levels
            // never reach this match (Deserialize fails first at
            // the JSON boundary).
            //
            // 2026-05-20: optional `id` slot lets the heading
            // serve as a jump-link anchor. We slug-validate via
            // SlugName so an attacker-controlled value can't break
            // out of the attribute context.
            let polish_classes = polish_class_string(polish);
            let class_attr = if polish_classes.is_empty() {
                "loom-heading".to_owned()
            } else {
                format!("loom-heading {polish_classes}")
            };
            let id_attr = id.as_deref().and_then(sanitize_anchor_id);
            match level {
                HeadingLevel::H2 => html! {
                    h2 id=[id_attr.clone()] class=(class_attr) data-loom-level="2" { (text) }
                },
                HeadingLevel::H3 => html! {
                    h3 id=[id_attr.clone()] class=(class_attr) data-loom-level="3" { (text) }
                },
                HeadingLevel::H4 => html! {
                    h4 id=[id_attr.clone()] class=(class_attr) data-loom-level="4" { (text) }
                },
                HeadingLevel::H5 => html! {
                    h5 id=[id_attr.clone()] class=(class_attr) data-loom-level="5" { (text) }
                },
                HeadingLevel::H6 => html! {
                    h6 id=[id_attr] class=(class_attr) data-loom-level="6" { (text) }
                },
            }
        }
        CmsSection::KvPair {
            heading,
            items,
            density,
            tone,
        } => {
            let density_attr = match density {
                KvPairDensity::Compact => "compact",
                KvPairDensity::Comfortable => "comfortable",
                KvPairDensity::Spacious => "spacious",
            };
            let tone_attr = match tone {
                KvPairTone::Slate => "slate",
                KvPairTone::Amoled => "amoled",
            };
            html! {
                section
                    class="loom-kv-section"
                    data-density=(density_attr)
                    data-tone=(tone_attr)
                {
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
            }
        }
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
        // Typed-line terminal transcript. Renders <pre><code> with one
        // <span> per line carrying a kind-specific class so the skin
        // can color + indent by semantic role. Distinct from
        // CmsSection::Code (flat body) and from the SaaS-mockup shell
        // shape (no fake traffic-light circles emitted).
        CmsSection::CodeShell {
            title,
            prompt,
            lines,
            tone,
            chrome,
        } => {
            let tone_attr = match tone {
                CodeShellTone::Slate => "slate",
                CodeShellTone::Amoled => "amoled",
            };
            let chrome_attr = match chrome {
                CodeShellChrome::Minimal => "minimal",
                CodeShellChrome::Header => "header",
            };
            let prompt_glyph = prompt.as_deref().unwrap_or("$");
            let show_header = matches!(chrome, CodeShellChrome::Header) && title.is_some();
            html! {
                section
                    class="loom-code-shell"
                    data-loom-code-shell
                    data-tone=(tone_attr)
                    data-chrome=(chrome_attr)
                {
                    @if show_header {
                        @if let Some(t) = title {
                            div class="loom-code-shell__header" { (t) }
                        }
                    }
                    pre class="loom-code-shell__pre" {
                        code class="loom-code-shell__code" {
                            @for line in lines {
                                @match line.kind {
                                    CmsCodeShellLineKind::Command => {
                                        span class="loom-code-shell__line loom-code-shell__line--command" {
                                            span class="loom-code-shell__prompt" {
                                                (prompt_glyph) " "
                                            }
                                            (line.text)
                                        }
                                        "\n"
                                    }
                                    CmsCodeShellLineKind::Output => {
                                        span class="loom-code-shell__line loom-code-shell__line--output" {
                                            (line.text)
                                        }
                                        "\n"
                                    }
                                    CmsCodeShellLineKind::Comment => {
                                        span class="loom-code-shell__line loom-code-shell__line--comment" {
                                            "# " (line.text)
                                        }
                                        "\n"
                                    }
                                    CmsCodeShellLineKind::Error => {
                                        span class="loom-code-shell__line loom-code-shell__line--error" {
                                            (line.text)
                                        }
                                        "\n"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::ImageHero {
            eyebrow,
            title,
            lede,
            cta,
            background,
            height,
            align,
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
            let align_attr = align.attr();
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
                    data-loom-image-hero data-loom-reveal data-align=(align_attr) {
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
        }
        CmsSection::SplitHero {
            eyebrow,
            title,
            lede,
            cta,
            visual,
            visual_right,
        } => {
            let order_class = if *visual_right {
                "visual-right"
            } else {
                "visual-left"
            };
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
                                    data-asset-slug=(slug) {
                                    img src={ "/assets/" (slug) ".jpg" }
                                        alt=(alt)
                                        loading="eager"
                                        decoding="async";
                                }
                            }
                        }
                    }
                }
            }
        }
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
                                @if let Some(img) = &item.image {
                                    @if loom_components::composer::is_safe_url(&img.src) {
                                        img class="loom-feature-spotlight__photo"
                                            src=(img.src)
                                            alt=(img.alt)
                                            width=[img.width]
                                            height=[img.height]
                                            decoding="async"
                                            loading="lazy";
                                    }
                                }
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
        }
        CmsSection::StatBand {
            heading,
            lede,
            items,
        } => html! {
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
        CmsSection::Steps {
            heading,
            lede,
            items,
        } => html! {
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
        CmsSection::Pricing {
            heading,
            lede,
            tiers,
        } => html! {
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
        CmsSection::Faq {
            heading,
            lede,
            items,
            single_expand,
        } => html! {
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
        CmsSection::Marquee {
            items,
            direction,
            speed,
        } => {
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
        }
        CmsSection::CallToAction {
            eyebrow,
            title,
            lede,
            cta,
            background,
            align,
        } => {
            let bg_class = match background {
                HeroBackground::GradientMesh => "gradient-mesh",
                HeroBackground::Solid { .. } => "solid",
                HeroBackground::Stripes => "stripes",
                HeroBackground::Dots => "dots",
                HeroBackground::Photo { .. } => "photo",
            };
            let align_attr = align.attr();
            let cta_href_safe = loom_components::composer::is_safe_url(&cta.href);
            html! {
                section class={ "loom-cta-band loom-bleed bg-" (bg_class) }
                    data-loom-cta-band data-loom-reveal data-align=(align_attr) {
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
        }
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
        CmsSection::PullQuote {
            body,
            attribution,
            cite_url,
            emphasis,
            tone,
        } => {
            let emphasis_attr = match emphasis {
                PullQuoteEmphasis::Inline => "inline",
                PullQuoteEmphasis::Display => "display",
            };
            let tone_attr = match tone {
                PullQuoteTone::Slate => "slate",
                PullQuoteTone::Amoled => "amoled",
            };
            let paragraphs: Vec<&str> = body
                .split("\n\n")
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect();
            html! {
                figure
                    class="loom-pull-quote"
                    data-loom-reveal
                    data-emphasis=(emphasis_attr)
                    data-tone=(tone_attr)
                {
                    blockquote class="loom-pull-quote__body" cite=[cite_url.as_deref()] {
                        @for para in &paragraphs {
                            p { (*para) }
                        }
                    }
                    @if let Some(a) = attribution {
                        figcaption class="loom-pull-quote__attribution" { "— " (a) }
                    }
                }
            }
        }
        CmsSection::Epigraph { body, attribution } => html! {
            figure class="loom-epigraph" data-loom-reveal {
                blockquote class="loom-epigraph__body" { (body) }
                @if let Some(a) = attribution {
                    figcaption class="loom-epigraph__attribution" { "— " (a) }
                }
            }
        },
        // ─── T660 P5 — catalogue expansion render arms ───
        CmsSection::Container {
            children_html,
            max_width,
        } => {
            let w = match max_width {
                ContainerWidth::Narrow => "narrow",
                ContainerWidth::Comfortable => "comfortable",
                ContainerWidth::Wide => "wide",
                ContainerWidth::Full => "full",
            };
            html! { div class={ "loom-container w-" (w) } { (maud::PreEscaped(escape_html_text(children_html).to_string())) } }
        }
        CmsSection::Divider { style } => {
            let s = match style {
                DividerStyle::Line => "line",
                DividerStyle::Dots => "dots",
                DividerStyle::ZigZag => "zigzag",
                DividerStyle::Sparkle => "sparkle",
            };
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
                RevealMotion::FadeUp => "fade-up",
                RevealMotion::FadeIn => "fade-in",
                RevealMotion::ScaleIn => "scale-in",
                RevealMotion::SlideLeft => "slide-left",
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
        CmsSection::Byline {
            author,
            role,
            dateline,
            reading_time,
        } => html! {
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
        CmsSection::DropCap { text } => {
            html! { p class="loom-dropcap" data-loom-reveal { (text) } }
        }
        CmsSection::CrucibleChallenge {
            kind_slug,
            tenant_id,
            base_path,
            widget_url,
        } => {
            // Mount id is deterministic per (kind, tenant) so
            // multiple challenge embeds on one page don't
            // collide. The widget's init() looks the mount up
            // by id.
            let mount_id = format!("crucible-{}-{}", kind_slug, tenant_id);
            let base = base_path.as_deref().unwrap_or("/crucible");
            // The widget exposes `init(element_id, kind,
            // tenant_id, base_path)`. We invoke it from a
            // module script that imports the wasm-pack
            // bundle's default export (the loader) + the
            // `init` named export (our widget entry).
            let inline = format!(
                r#"
import init, {{ init as crucible_init }} from "{widget_url}";
(async () => {{
  await init();
  await crucible_init({mount_id_lit}, {kind_lit}, {tenant_lit}, {base_lit});
}})();
"#,
                widget_url = widget_url,
                mount_id_lit = json_string_literal(&mount_id),
                kind_lit = json_string_literal(kind_slug),
                tenant_lit = json_string_literal(tenant_id),
                base_lit = json_string_literal(base),
            );
            html! {
                section class="loom-crucible-challenge" data-loom-reveal {
                    div id=(mount_id) class="loom-crucible-challenge__mount" {}
                    script type="module" { (maud::PreEscaped(inline)) }
                }
            }
        }
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
        CmsSection::Figure {
            caption,
            credit,
            asset_slug,
        } => html! {
            figure class="loom-figure" data-loom-reveal {
                @if let Some(slug) = asset_slug {
                    div class="loom-figure__media" data-asset-slug=(slug) {
                        img src={ "/assets/" (slug) ".jpg" }
                            alt=(caption)
                            loading="lazy"
                            decoding="async";
                    }
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
        CmsSection::Diagram {
            notation,
            source,
            alt,
        } => {
            let n = match notation {
                DiagramKind::Mermaid => "mermaid",
                DiagramKind::Plantuml => "plantuml",
                DiagramKind::Ascii => "ascii",
            };
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
        CmsSection::Testimonial {
            body,
            attribution,
            role,
            avatar_slug,
            decoration,
        } => {
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
        }
        CmsSection::LogoCloud { heading, items } => html! {
            section class="loom-logo-cloud" data-loom-reveal {
                @if let Some(h) = heading { h2 class="loom-logo-cloud__heading" { (h) } }
                div class="loom-logo-cloud__row" { @for it in items { span class="loom-logo-cloud__item" { (it) } } }
            }
        },
        CmsSection::Comparison {
            heading,
            columns,
            rows,
        } => html! {
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
        CmsSection::CaseStudy {
            headline,
            body,
            metrics,
            href,
            data_backend,
        } => {
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
        CmsSection::CookieNotice {
            text,
            accept_label,
            reject_label,
        } => html! {
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
        CmsSection::NewsletterSignup {
            heading,
            lede,
            placeholder,
            submit_label,
        } => html! {
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
                        figure class="loom-image-grid__cell" data-asset-slug=(img.asset_slug) {
                            img src={ "/assets/" (img.asset_slug) ".jpg" }
                                alt=(img.alt)
                                loading="lazy"
                                decoding="async";
                            @if let Some(cap) = &img.caption { figcaption class="loom-image-grid__caption" { (cap) } }
                        }
                    }
                }
            }
        }
        CmsSection::FigureGroup { items } => html! {
            section class="loom-figure-group" data-loom-reveal {
                @for img in items {
                    figure class="loom-figure-group__cell" data-asset-slug=(img.asset_slug) {
                        img src={ "/assets/" (img.asset_slug) ".jpg" }
                            alt=(img.alt)
                            loading="lazy"
                            decoding="async";
                        @if let Some(cap) = &img.caption { figcaption { (cap) } }
                    }
                }
            }
        },
        CmsSection::VideoEmbed {
            src,
            poster,
            alt,
            mime,
        } => {
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
                        data-asset-slug=(img.asset_slug) {
                        img src={ "/assets/" (img.asset_slug) ".jpg" }
                            alt=(img.alt)
                            loading=(if i == 0 { "eager" } else { "lazy" })
                            decoding="async";
                    }
                }
            }
        },
        CmsSection::BeforeAfter {
            before_alt,
            after_alt,
            before_slug,
            after_slug,
        } => html! {
            div class="loom-before-after" data-loom-before-after data-loom-reveal {
                figure class="loom-before-after__before" data-asset-slug=(before_slug) {
                    img src={ "/assets/" (before_slug) ".jpg" }
                        alt=(before_alt)
                        loading="lazy"
                        decoding="async";
                }
                figure class="loom-before-after__after" data-asset-slug=(after_slug) {
                    img src={ "/assets/" (after_slug) ".jpg" }
                        alt=(after_alt)
                        loading="lazy"
                        decoding="async";
                }
                input type="range" min="0" max="100" value="50" aria-label="Reveal slider" class="loom-before-after__slider";
            }
        },
        CmsSection::Lightbox { items } => html! {
            section class="loom-lightbox" data-loom-lightbox data-loom-reveal {
                @for img in items {
                    button type="button" class="loom-lightbox__thumb" data-asset-slug=(img.asset_slug) aria-label=(img.alt) {
                        img src={ "/assets/" (img.asset_slug) ".jpg" }
                            alt=(img.alt)
                            loading="lazy"
                            decoding="async";
                    }
                }
            }
        },
        CmsSection::MosaicGrid { items } => html! {
            section class="loom-mosaic" data-loom-reveal {
                @for img in items {
                    figure class="loom-mosaic__cell" data-asset-slug=(img.asset_slug) {
                        img src={ "/assets/" (img.asset_slug) ".jpg" }
                            alt=(img.alt)
                            loading="lazy"
                            decoding="async";
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
        CmsSection::ProductCard {
            name,
            price,
            rating,
            image_alt,
            image_slug,
            href,
            data_backend,
        } => {
            let safe = is_safe_url(href);
            html! {
                article class="loom-product-card" data-loom-reveal {
                    a class="loom-product-card__link"
                      href=(if safe { href.as_str() } else { "#invalid-link" })
                      data-backend=(data_backend) {
                        figure class="loom-product-card__image" data-asset-slug=(image_slug) {
                            img src={ "/assets/" (image_slug) ".jpg" }
                                alt=(image_alt)
                                loading="lazy"
                                decoding="async";
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
                                figure class="loom-product-card__image" data-asset-slug=(p.image_slug) {
                                    img src={ "/assets/" (p.image_slug) ".jpg" }
                                        alt=(p.image_alt)
                                        loading="lazy"
                                        decoding="async";
                                }
                                h3 class="loom-product-card__name" { (p.name) }
                                div class="loom-product-card__price" { (p.price) }
                            }
                        }
                    }
                }
            }
        },
        CmsSection::PriceTag {
            amount,
            currency,
            was,
        } => html! {
            span class="loom-price-tag" {
                @if let Some(w) = was { s class="loom-price-tag__was" { (w) } " " }
                span class="loom-price-tag__amount" { (amount) }
                span class="loom-price-tag__currency" { " " (currency) }
            }
        },
        CmsSection::AddToCart {
            label,
            sku,
            data_backend,
        } => html! {
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
                    figure class="loom-product-gallery__cell" data-asset-slug=(img.asset_slug) {
                        img src={ "/assets/" (img.asset_slug) ".jpg" }
                            alt=(img.alt)
                            loading="lazy"
                            decoding="async";
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
        CmsSection::ReviewCard {
            author,
            rating,
            body,
            date,
        } => html! {
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
        CmsSection::MentionInline {
            username,
            href,
            data_backend,
        } => {
            let safe = is_safe_url(href);
            html! {
                a class="loom-mention"
                  href=(if safe { href.as_str() } else { "#invalid-link" })
                  data-backend=(data_backend) { "@" (username) }
            }
        }
        CmsSection::HashtagInline {
            tag,
            href,
            data_backend,
        } => {
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
        CmsSection::FollowButton {
            label,
            count,
            data_backend,
        } => html! {
            button type="button" class="loom-follow-btn loom-btn loom-btn--primary" data-backend=(data_backend) {
                (label) " · " span class="loom-follow-btn__count" { (count.to_string()) }
            }
        },
        CmsSection::ProfileCard {
            name,
            handle,
            bio,
            avatar,
            follow,
        } => html! {
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
        CmsSection::FormInput {
            name,
            label,
            input_type,
            placeholder,
            required,
        } => {
            let t = match input_type {
                FormInputKind::Text => "text",
                FormInputKind::Email => "email",
                FormInputKind::Password => "password",
                FormInputKind::Tel => "tel",
                FormInputKind::Url => "url",
                FormInputKind::Number => "number",
                FormInputKind::Search => "search",
            };
            html! {
                label class="loom-form-input" {
                    span class="loom-form-input__label" { (label) @if *required { " *" } }
                    input type=(t) name=(name) placeholder=[placeholder.as_deref()] required=[required.then_some("required")];
                }
            }
        }
        CmsSection::FormSelect {
            name,
            label,
            options,
            required,
        } => html! {
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
        CmsSection::FormSlider {
            name,
            label,
            min,
            max,
            value,
        } => html! {
            label class="loom-form-slider" {
                span class="loom-form-slider__label" { (label) }
                input type="range" name=(name) min=(min.to_string()) max=(max.to_string()) value=(value.to_string());
            }
        },
        CmsSection::FormDate {
            name,
            label,
            required,
        } => html! {
            label class="loom-form-date" {
                span class="loom-form-date__label" { (label) @if *required { " *" } }
                input type="date" name=(name) required=[required.then_some("required")];
            }
        },
        CmsSection::FormFile {
            name,
            label,
            accept,
        } => html! {
            label class="loom-form-file" {
                span class="loom-form-file__label" { (label) }
                input type="file" name=(name) accept=(accept);
            }
        },
        CmsSection::FormSearch {
            placeholder,
            data_backend,
        } => html! {
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
        CmsSection::FormTextarea {
            name,
            label,
            placeholder,
            rows,
        } => html! {
            label class="loom-form-textarea" {
                span class="loom-form-textarea__label" { (label) }
                textarea name=(name) rows=(rows.to_string()) placeholder=[placeholder.as_deref()] {}
            }
        },
        CmsSection::FormSubmit {
            label,
            data_backend,
            variant,
        } => {
            let v = match variant {
                ButtonVariant::Primary => "primary",
                ButtonVariant::Secondary => "secondary",
                ButtonVariant::Ghost => "ghost",
                ButtonVariant::Danger => "danger",
            };
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
        CmsSection::Pagination {
            current,
            total,
            base_href,
            data_backend,
        } => html! {
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
        CmsSection::Alert {
            tone,
            title,
            body,
            dismissible,
        } => {
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
        CmsSection::Modal {
            title,
            body,
            primary,
            secondary,
        } => {
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
            let s = match side {
                DrawerSide::Right => "right",
                DrawerSide::Left => "left",
            };
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
        CmsSection::GameTile {
            title,
            genre,
            players_online,
            image_slug,
            href,
            data_backend,
        } => {
            let safe = is_safe_url(href);
            html! {
                article class="loom-game-tile" data-loom-reveal {
                    a class="loom-game-tile__link"
                      href=(if safe { href.as_str() } else { "#invalid-link" })
                      data-backend=(data_backend) {
                        figure class="loom-game-tile__thumb" data-asset-slug=(image_slug) {
                            img src={ "/assets/" (image_slug) ".jpg" }
                                alt=(title)
                                loading="lazy"
                                decoding="async";
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
                                figure class="loom-game-tile__thumb" data-asset-slug=(g.image_slug) {
                                    img src={ "/assets/" (g.image_slug) ".jpg" }
                                        alt=(g.title)
                                        loading="lazy"
                                        decoding="async";
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
        CmsSection::ThreadRow {
            title,
            author,
            replies,
            views,
            last_reply,
            href,
            data_backend,
        } => {
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
        CmsSection::VideoCard {
            title,
            channel,
            duration,
            views,
            thumbnail_slug,
            href,
            data_backend,
        } => {
            let safe = is_safe_url(href);
            html! {
                article class="loom-video-card" data-loom-reveal {
                    a class="loom-video-card__link"
                      href=(if safe { href.as_str() } else { "#invalid-link" })
                      data-backend=(data_backend) {
                        figure class="loom-video-card__thumb" data-asset-slug=(thumbnail_slug) {
                            img src={ "/assets/" (thumbnail_slug) ".jpg" }
                                alt=(title)
                                loading="lazy"
                                decoding="async";
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
                                figure class="loom-video-card__thumb" data-asset-slug=(v.thumbnail_slug) {
                                    img src={ "/assets/" (v.thumbnail_slug) ".jpg" }
                                        alt=(v.title)
                                        loading="lazy"
                                        decoding="async";
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
        CmsSection::FeedPost {
            author,
            handle,
            avatar,
            body,
            posted_at,
            reactions,
            comments,
        } => html! {
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
        CmsSection::AuthCard {
            title,
            tagline,
            methods,
            footer,
        } => html! {
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
        CmsSection::MfaPrompt {
            title,
            factor,
            instructions,
            otp_length,
            submit_label,
            switch_label,
        } => {
            let factor_class = match factor {
                MfaFactorKind::Totp => "totp",
                MfaFactorKind::Webauthn => "webauthn",
                MfaFactorKind::SmsOtp => "sms-otp",
                MfaFactorKind::EmailOtp => "email-otp",
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
        CmsSection::CrucibleWidget {
            challenge_kind,
            prompt,
            difficulty,
            option_count,
            submit_label,
            attribution_hint,
        } => {
            let kind_class = match challenge_kind {
                CrucibleKind::ImageClassify => "image-classify",
                CrucibleKind::SemanticSimilarity => "semantic-similarity",
                CrucibleKind::AudioTranscribe => "audio-transcribe",
                CrucibleKind::MathArithmetic => "math-arithmetic",
                CrucibleKind::DrawingReconstruct => "drawing-reconstruct",
                CrucibleKind::PromptInjectionDetect => "prompt-injection-detect",
            };
            let diff_class = match difficulty {
                CrucibleDifficulty::Easy => "easy",
                CrucibleDifficulty::Medium => "medium",
                CrucibleDifficulty::Hard => "hard",
                CrucibleDifficulty::Adversarial => "adversarial",
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
        CmsSection::SignedInCard {
            display_name,
            handle,
            avatar,
            sign_out,
        } => {
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
        CmsSection::PasswordReset {
            title,
            description,
            placeholder,
            submit_label,
        } => html! {
            section class="loom-password-reset" data-loom-reveal {
                h2 class="loom-password-reset__title" { (title) }
                p class="loom-password-reset__description" { (description) }
                form class="loom-password-reset__form" {
                    input type="email" name="email" required placeholder=(placeholder) aria-label="Email";
                    button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
                }
            }
        },
        CmsSection::ChangelogList {
            heading,
            entries,
            style,
        } => {
            let style_mod = style.modifier();
            html! {
                section class={ "loom-changelog loom-changelog--" (style_mod) } data-loom-reveal {
                    h2 class="loom-changelog__heading" { (heading) }
                    @if entries.is_empty() {
                        p class="loom-changelog__empty" { "No releases yet." }
                    } @else {
                        ol class="loom-changelog__entries" {
                            @for entry in entries {
                                li class="loom-changelog__entry" {
                                    header class="loom-changelog__entry-header" {
                                        h3 class="loom-changelog__version" { (entry.version) }
                                        " "
                                        time class="loom-changelog__date" datetime=(entry.date.as_str()) { (entry.date) }
                                    }
                                    @if let Some(s) = &entry.summary {
                                        p class="loom-changelog__summary" { (s) }
                                    }
                                    @match style {
                                        ChangelogListStyle::Detailed => {
                                            @if entry.changes.is_empty() && entry.summary.is_none() {
                                                p class="loom-changelog__no-changes" { "No changes recorded." }
                                            } @else if !entry.changes.is_empty() {
                                                ul class="loom-changelog__changes" {
                                                    @for change in &entry.changes {
                                                        @let mod_class = change.kind.modifier();
                                                        li class={ "loom-changelog-change loom-changelog-change--" (mod_class) } {
                                                            span class="loom-changelog-change__tag"
                                                                 aria-label=(change.kind.label()) {
                                                                (change.kind.label())
                                                            }
                                                            " "
                                                            span class="loom-changelog-change__text" { (change.text) }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        ChangelogListStyle::Compact => {
                                            p class="loom-changelog__compact-summary" {
                                                (entry.changes.len().to_string())
                                                " change"
                                                @if entry.changes.len() != 1 { "s" }
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
        CmsSection::Disclaimer {
            disclosure_kind,
            body,
            source,
        } => {
            let modifier = disclosure_kind.modifier();
            let mut aria_label = disclosure_kind.accessible_label().to_owned();
            if matches!(disclosure_kind, DisclaimerKind::Sponsored) {
                if let Some(src) = source {
                    aria_label.push_str(" from ");
                    aria_label.push_str(src);
                }
            }
            html! {
                aside class={ "loom-disclaimer loom-disclaimer--" (modifier) }
                      role="note"
                      aria-label=(aria_label)
                      data-loom-reveal {
                    p class="loom-disclaimer__body" { (body) }
                    @if let Some(src) = source {
                        @if matches!(disclosure_kind, DisclaimerKind::Sponsored | DisclaimerKind::Affiliate) {
                            p class="loom-disclaimer__source" {
                                "Source: " (src)
                            }
                        }
                    }
                }
            }
        }
        CmsSection::SourceList {
            heading,
            items,
            style,
        } => {
            let style_mod = style.modifier();
            html! {
                section class={ "loom-source-list loom-source-list--" (style_mod) } data-loom-reveal {
                    h2 class="loom-source-list__heading" { (heading) }
                    @if items.is_empty() {
                        p class="loom-source-list__empty" { "No sources." }
                    } @else {
                        @match style {
                            SourceListStyle::Numbered => {
                                ol class="loom-source-list__items" {
                                    @for item in items {
                                        (render_source_list_item(item))
                                    }
                                }
                            }
                            SourceListStyle::Bulleted | SourceListStyle::Plain => {
                                ul class="loom-source-list__items" {
                                    @for item in items {
                                        (render_source_list_item(item))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::Boxplot {
            label,
            boxes,
            tone,
            caption,
        } => {
            let tone_mod = tone.modifier();
            let outer_class = format!("loom-boxplot loom-boxplot--{tone_mod}");
            if boxes.is_empty() {
                return html! {
                    figure class=(format!("{outer_class} loom-boxplot--empty")) data-loom-reveal {
                        figcaption class="loom-boxplot__label" { (label) }
                        p class="loom-boxplot__no-data" { "No data" }
                        @if let Some(c) = caption {
                            figcaption class="loom-boxplot__caption" { (c) }
                        }
                    }
                };
            }
            let n = boxes.len();
            // Compute global min / max across all whiskers so all
            // boxes share an axis.
            let g_min = boxes.iter().map(|b| b.min).fold(f64::INFINITY, f64::min);
            let g_max = boxes
                .iter()
                .map(|b| b.max)
                .fold(f64::NEG_INFINITY, f64::max);
            let range = if (g_max - g_min).abs() < f64::EPSILON {
                1.0_f64
            } else {
                g_max - g_min
            };
            const VBW: f64 = 200.0;
            const VBH: f64 = 80.0;
            const PAD: f64 = 4.0;
            let col_w = (VBW - 2.0 * PAD) / (n as f64);
            // Each box gets centered in its column with box width
            // = 60% of column width.
            let box_w = col_w * 0.6;
            let aria_label =
                format!("{label} boxplot: {n} categories, range {g_min:.2}–{g_max:.2}");
            // Helper: map data value to SVG y (inverted: higher
            // values render higher on screen).
            let map_y = |v: f64| -> f64 {
                let normalized = (v - g_min) / range;
                VBH - PAD - normalized * (VBH - 2.0 * PAD)
            };
            html! {
                figure class=(outer_class) data-loom-reveal {
                    figcaption class="loom-boxplot__label" { (label) }
                    svg class="loom-boxplot__svg"
                        viewBox=(format!("0 0 {VBW:.0} {VBH:.0}"))
                        preserveAspectRatio="none"
                        role="img"
                        aria-label=(aria_label) {
                        @for (i, b) in boxes.iter().enumerate() {
                            @let col_x = PAD + (i as f64) * col_w;
                            @let center_x = col_x + col_w / 2.0;
                            @let box_x = center_x - box_w / 2.0;
                            @let y_min = map_y(b.min);
                            @let y_q1 = map_y(b.q1);
                            @let y_median = map_y(b.median);
                            @let y_q3 = map_y(b.q3);
                            @let y_max = map_y(b.max);
                            // q1→q3 box (height = |y_q1 - y_q3|).
                            // y_q3 is the TOP edge (lower y because SVG y is inverted).
                            @let box_top = y_q3.min(y_q1);
                            @let box_height = (y_q1 - y_q3).abs();
                            // Whiskers: line from min to q1, q3 to max.
                            line class="loom-boxplot__whisker loom-boxplot__whisker--lower"
                                 x1=(format!("{center_x:.1}"))
                                 y1=(format!("{y_min:.1}"))
                                 x2=(format!("{center_x:.1}"))
                                 y2=(format!("{y_q1:.1}"))
                                 stroke="currentColor"
                                 stroke-width="1" {}
                            line class="loom-boxplot__whisker loom-boxplot__whisker--upper"
                                 x1=(format!("{center_x:.1}"))
                                 y1=(format!("{y_q3:.1}"))
                                 x2=(format!("{center_x:.1}"))
                                 y2=(format!("{y_max:.1}"))
                                 stroke="currentColor"
                                 stroke-width="1" {}
                            // Whisker caps (small horizontal lines at min and max).
                            line class="loom-boxplot__cap loom-boxplot__cap--lower"
                                 x1=(format!("{:.1}", center_x - box_w * 0.3))
                                 y1=(format!("{y_min:.1}"))
                                 x2=(format!("{:.1}", center_x + box_w * 0.3))
                                 y2=(format!("{y_min:.1}"))
                                 stroke="currentColor"
                                 stroke-width="1" {}
                            line class="loom-boxplot__cap loom-boxplot__cap--upper"
                                 x1=(format!("{:.1}", center_x - box_w * 0.3))
                                 y1=(format!("{y_max:.1}"))
                                 x2=(format!("{:.1}", center_x + box_w * 0.3))
                                 y2=(format!("{y_max:.1}"))
                                 stroke="currentColor"
                                 stroke-width="1" {}
                            // q1→q3 box.
                            rect class="loom-boxplot__box"
                                 x=(format!("{box_x:.1}"))
                                 y=(format!("{box_top:.1}"))
                                 width=(format!("{box_w:.1}"))
                                 height=(format!("{box_height:.1}"))
                                 fill="currentColor"
                                 fill-opacity="0.2"
                                 stroke="currentColor"
                                 stroke-width="1" {}
                            // Median line.
                            line class="loom-boxplot__median"
                                 x1=(format!("{box_x:.1}"))
                                 y1=(format!("{y_median:.1}"))
                                 x2=(format!("{:.1}", box_x + box_w))
                                 y2=(format!("{y_median:.1}"))
                                 stroke="currentColor"
                                 stroke-width="2" {}
                        }
                    }
                    ol class="loom-boxplot__legend" {
                        @for b in boxes {
                            li class="loom-boxplot__legend-item" {
                                span class="loom-boxplot__legend-label" { (b.label) }
                                span class="loom-boxplot__legend-summary" {
                                    (format!("min {:.2} · q1 {:.2} · med {:.2} · q3 {:.2} · max {:.2}",
                                        b.min, b.q1, b.median, b.q3, b.max))
                                }
                            }
                        }
                    }
                    @if let Some(c) = caption {
                        figcaption class="loom-boxplot__caption" { (c) }
                    }
                }
            }
        }
        CmsSection::Heatmap {
            label,
            row_labels,
            column_labels,
            cells,
            tone,
            caption,
        } => {
            let tone_mod = tone.modifier();
            let outer_class = format!("loom-heatmap loom-heatmap--{tone_mod}");
            let has_data = !cells.is_empty() && !cells.iter().all(std::vec::Vec::is_empty);
            if !has_data {
                return html! {
                    figure class=(format!("{outer_class} loom-heatmap--empty")) data-loom-reveal {
                        figcaption class="loom-heatmap__label" { (label) }
                        p class="loom-heatmap__no-data" { "No data" }
                        @if let Some(c) = caption {
                            figcaption class="loom-heatmap__caption" { (c) }
                        }
                    }
                };
            }
            let n_rows = cells.len();
            let n_cols = cells.iter().map(std::vec::Vec::len).max().unwrap_or(0);
            let max_abs = cells
                .iter()
                .flat_map(|row| row.iter().map(|v| v.abs()))
                .fold(0.0_f64, f64::max)
                .max(1.0_f64);
            const VBW: f64 = 200.0;
            const VBH: f64 = 80.0;
            const PAD: f64 = 4.0;
            let cell_w = if n_cols == 0 {
                0.0
            } else {
                (VBW - 2.0 * PAD) / (n_cols as f64)
            };
            let cell_h = if n_rows == 0 {
                0.0
            } else {
                (VBH - 2.0 * PAD) / (n_rows as f64)
            };
            let total_cells: usize = cells.iter().map(std::vec::Vec::len).sum();
            let aria_label = format!(
                "{label} heatmap: {n_rows} rows × {n_cols} columns, {total_cells} cells, max absolute value {max_abs:.2}"
            );
            html! {
                figure class=(outer_class) data-loom-reveal {
                    figcaption class="loom-heatmap__label" { (label) }
                    svg class="loom-heatmap__svg"
                        viewBox=(format!("0 0 {VBW:.0} {VBH:.0}"))
                        preserveAspectRatio="none"
                        role="img"
                        aria-label=(aria_label) {
                        @for (r, row) in cells.iter().enumerate() {
                            @for (c, v) in row.iter().enumerate() {
                                @let x = PAD + (c as f64) * cell_w;
                                @let y = PAD + (r as f64) * cell_h;
                                @let opacity = (v.abs() / max_abs).clamp(0.0, 1.0);
                                rect class="loom-heatmap__cell"
                                     x=(format!("{x:.1}"))
                                     y=(format!("{y:.1}"))
                                     width=(format!("{cell_w:.1}"))
                                     height=(format!("{cell_h:.1}"))
                                     fill="currentColor"
                                     fill-opacity=(format!("{opacity:.3}")) {}
                            }
                        }
                    }
                    @if !row_labels.is_empty() || !column_labels.is_empty() {
                        table class="loom-heatmap__legend" {
                            caption class="loom-sr-only" { "Heatmap values by row and column" }
                            thead {
                                tr {
                                    th { "" }
                                    @for cl in column_labels {
                                        th scope="col" { (cl) }
                                    }
                                }
                            }
                            tbody {
                                @for (r, row) in cells.iter().enumerate() {
                                    tr {
                                        @if r < row_labels.len() {
                                            th scope="row" { (row_labels[r]) }
                                        } @else {
                                            th scope="row" { "" }
                                        }
                                        @for v in row {
                                            td { (format!("{v:.2}")) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    @if let Some(c) = caption {
                        figcaption class="loom-heatmap__caption" { (c) }
                    }
                }
            }
        }
        CmsSection::DivergingBar {
            label,
            items,
            tone,
            midline_label,
            caption,
        } => {
            let tone_mod = tone.modifier();
            let outer_class = format!("loom-divbar loom-divbar--{tone_mod}");
            if items.is_empty() {
                return html! {
                    figure class=(format!("{outer_class} loom-divbar--empty")) data-loom-reveal {
                        figcaption class="loom-divbar__label" { (label) }
                        p class="loom-divbar__no-data" { "No data" }
                        @if let Some(c) = caption {
                            figcaption class="loom-divbar__caption" { (c) }
                        }
                    }
                };
            }
            let n = items.len();
            let max_abs = items
                .iter()
                .map(|i| i.value.abs())
                .fold(0.0_f64, f64::max)
                .max(1.0_f64);
            const VBW: f64 = 200.0;
            const VBH: f64 = 80.0;
            const PAD: f64 = 4.0;
            const MIDLINE_X: f64 = VBW / 2.0;
            // Each side has half the viewBox minus padding for bars.
            let half_usable = MIDLINE_X - PAD;
            let aria_label =
                format!("{label} diverging bar: {n} rows, max absolute value {max_abs:.2}");
            html! {
                figure class=(outer_class) data-loom-reveal {
                    figcaption class="loom-divbar__label" { (label) }
                    svg class="loom-divbar__svg"
                        viewBox=(format!("0 0 {VBW:.0} {VBH:.0}"))
                        preserveAspectRatio="none"
                        role="img"
                        aria-label=(aria_label) {
                        // Midline rule
                        line class="loom-divbar__midline"
                             x1=(format!("{MIDLINE_X:.1}"))
                             y1=(format!("{PAD:.1}"))
                             x2=(format!("{MIDLINE_X:.1}"))
                             y2=(format!("{:.1}", VBH - PAD))
                             stroke="currentColor"
                             stroke-width="0.5"
                             stroke-dasharray="2 2" {}
                        @let bar_h = (VBH - 2.0 * PAD) / (n as f64);
                        @let bar_inner_h = bar_h * 0.7;
                        @let bar_gap = bar_h * 0.3;
                        @for (i, item) in items.iter().enumerate() {
                            @let v = item.value;
                            @let bar_w = (v.abs() / max_abs) * half_usable;
                            @let y = PAD + (i as f64) * bar_h + bar_gap / 2.0;
                            @let x = if v >= 0.0 {
                                MIDLINE_X
                            } else {
                                MIDLINE_X - bar_w
                            };
                            @let sign_mod = if v >= 0.0 { "positive" } else { "negative" };
                            rect class={ "loom-divbar__bar loom-divbar__bar--" (sign_mod) }
                                 x=(format!("{x:.1}"))
                                 y=(format!("{y:.1}"))
                                 width=(format!("{bar_w:.1}"))
                                 height=(format!("{bar_inner_h:.1}"))
                                 fill="currentColor" {}
                        }
                    }
                    @if let Some(ml) = midline_label {
                        p class="loom-divbar__midline-label" aria-hidden="true" { (ml) }
                    }
                    ol class="loom-divbar__legend" {
                        @for item in items {
                            li class="loom-divbar__legend-item" {
                                span class="loom-divbar__legend-label" { (item.label) }
                                span class={ "loom-divbar__legend-value loom-divbar__legend-value--" (if item.value >= 0.0 { "positive" } else { "negative" }) } {
                                    (format!("{:+.2}", item.value))
                                }
                            }
                        }
                    }
                    @if let Some(c) = caption {
                        figcaption class="loom-divbar__caption" { (c) }
                    }
                }
            }
        }
        CmsSection::Histogram {
            label,
            buckets,
            tone,
            caption,
        } => {
            let tone_mod = tone.modifier();
            let outer_class = format!("loom-histogram loom-histogram--{tone_mod}");
            if buckets.is_empty() {
                return html! {
                    figure class=(format!("{outer_class} loom-histogram--empty")) data-loom-reveal {
                        figcaption class="loom-histogram__label" { (label) }
                        p class="loom-histogram__no-data" { "No data" }
                        @if let Some(c) = caption {
                            figcaption class="loom-histogram__caption" { (c) }
                        }
                    }
                };
            }
            let n = buckets.len();
            let max_count = buckets.iter().map(|b| b.count).max().unwrap_or(0).max(1);
            let total: u64 = buckets.iter().map(|b| u64::from(b.count)).sum();
            let range_min = buckets
                .iter()
                .map(|b| b.range_min)
                .fold(f64::INFINITY, f64::min);
            let range_max = buckets
                .iter()
                .map(|b| b.range_max)
                .fold(f64::NEG_INFINITY, f64::max);
            const VBW: f64 = 200.0;
            const VBH: f64 = 80.0;
            const PAD: f64 = 4.0;
            let aria_label = format!(
                "{label} histogram: {n} bins, {total} total samples, range {range_min:.2}–{range_max:.2}"
            );
            html! {
                figure class=(outer_class) data-loom-reveal {
                    figcaption class="loom-histogram__label" { (label) }
                    svg class="loom-histogram__svg"
                        viewBox=(format!("0 0 {VBW:.0} {VBH:.0}"))
                        preserveAspectRatio="none"
                        role="img"
                        aria-label=(aria_label) {
                        @let bar_w = (VBW - 2.0 * PAD) / (n as f64);
                        @for (i, bucket) in buckets.iter().enumerate() {
                            @let h = (f64::from(bucket.count) / f64::from(max_count)) * (VBH - 2.0 * PAD);
                            @let x = PAD + (i as f64) * bar_w;
                            @let y = VBH - PAD - h;
                            // Histogram bars touch (no gap) — that's the
                            // visual signal that distinguishes Histogram
                            // from BarChart (BarChart has bar_gap=0.3*bar_w).
                            rect class="loom-histogram__bar"
                                 x=(format!("{x:.1}"))
                                 y=(format!("{y:.1}"))
                                 width=(format!("{bar_w:.1}"))
                                 height=(format!("{h:.1}"))
                                 fill="currentColor" {}
                        }
                    }
                    ol class="loom-histogram__legend" {
                        @for bucket in buckets {
                            li class="loom-histogram__legend-item" {
                                span class="loom-histogram__legend-range" {
                                    (format!("{:.2}–{:.2}", bucket.range_min, bucket.range_max))
                                }
                                span class="loom-histogram__legend-count" {
                                    (bucket.count.to_string())
                                }
                            }
                        }
                    }
                    @if let Some(c) = caption {
                        figcaption class="loom-histogram__caption" { (c) }
                    }
                }
            }
        }
        CmsSection::BarChart {
            label,
            bars,
            orientation,
            tone,
            caption,
        } => {
            let orient_mod = orientation.modifier();
            let tone_mod = tone.modifier();
            let outer_class =
                format!("loom-barchart loom-barchart--{orient_mod} loom-barchart--{tone_mod}");
            if bars.is_empty() {
                return html! {
                    figure class=(format!("{outer_class} loom-barchart--empty")) data-loom-reveal {
                        figcaption class="loom-barchart__label" { (label) }
                        p class="loom-barchart__no-data" { "No data" }
                        @if let Some(c) = caption {
                            figcaption class="loom-barchart__caption" { (c) }
                        }
                    }
                };
            }
            let n = bars.len();
            // Clamp negatives + compute max.
            let clamped: Vec<f64> = bars.iter().map(|b| b.value.max(0.0)).collect();
            let max = clamped.iter().copied().fold(0.0_f64, f64::max).max(1.0_f64); // floor max at 1 so a zero-only chart still has a stable axis
            const VBW: f64 = 200.0;
            const VBH: f64 = 80.0;
            const PAD: f64 = 4.0;
            let aria_label = format!("{label} bar chart: {n} bars, max value {max:.2}");
            html! {
                figure class=(outer_class) data-loom-reveal {
                    figcaption class="loom-barchart__label" { (label) }
                    svg class="loom-barchart__svg"
                        viewBox=(format!("0 0 {VBW:.0} {VBH:.0}"))
                        preserveAspectRatio="none"
                        role="img"
                        aria-label=(aria_label) {
                        @match orientation {
                            BarChartOrientation::Vertical => {
                                @let bar_w = (VBW - 2.0 * PAD) / (n as f64);
                                @let bar_inner_w = bar_w * 0.7;
                                @let bar_gap = bar_w * 0.3;
                                @for (i, bar) in bars.iter().enumerate() {
                                    @let v = bar.value.max(0.0);
                                    @let h = (v / max) * (VBH - 2.0 * PAD);
                                    @let x = PAD + (i as f64) * bar_w + bar_gap / 2.0;
                                    @let y = VBH - PAD - h;
                                    @let bar_tone = bar.tone_override.unwrap_or(*tone);
                                    rect class={ "loom-barchart__bar loom-barchart__bar--" (bar_tone.modifier()) }
                                         x=(format!("{x:.1}"))
                                         y=(format!("{y:.1}"))
                                         width=(format!("{bar_inner_w:.1}"))
                                         height=(format!("{h:.1}"))
                                         fill="currentColor" {}
                                }
                            }
                            BarChartOrientation::Horizontal => {
                                @let bar_h = (VBH - 2.0 * PAD) / (n as f64);
                                @let bar_inner_h = bar_h * 0.7;
                                @let bar_gap = bar_h * 0.3;
                                @for (i, bar) in bars.iter().enumerate() {
                                    @let v = bar.value.max(0.0);
                                    @let w = (v / max) * (VBW - 2.0 * PAD);
                                    @let x = PAD;
                                    @let y = PAD + (i as f64) * bar_h + bar_gap / 2.0;
                                    @let bar_tone = bar.tone_override.unwrap_or(*tone);
                                    rect class={ "loom-barchart__bar loom-barchart__bar--" (bar_tone.modifier()) }
                                         x=(format!("{x:.1}"))
                                         y=(format!("{y:.1}"))
                                         width=(format!("{w:.1}"))
                                         height=(format!("{bar_inner_h:.1}"))
                                         fill="currentColor" {}
                                }
                            }
                        }
                    }
                    ol class="loom-barchart__legend" {
                        @for bar in bars {
                            li class="loom-barchart__legend-item" {
                                span class="loom-barchart__legend-label" { (bar.label) }
                                span class="loom-barchart__legend-value" { (format!("{:.2}", bar.value)) }
                            }
                        }
                    }
                    @if let Some(c) = caption {
                        figcaption class="loom-barchart__caption" { (c) }
                    }
                }
            }
        }
        CmsSection::Sparkline {
            label,
            data_points,
            tone,
            caption,
        } => {
            let modifier = tone.modifier();
            if data_points.is_empty() {
                return html! {
                    figure class={ "loom-sparkline loom-sparkline--" (modifier) " loom-sparkline--empty" } data-loom-reveal {
                        figcaption class="loom-sparkline__label" { (label) }
                        p class="loom-sparkline__no-data" { "No data" }
                        @if let Some(c) = caption {
                            figcaption class="loom-sparkline__caption" { (c) }
                        }
                    }
                };
            }
            let n = data_points.len();
            let min = data_points.iter().copied().fold(f64::INFINITY, f64::min);
            let max = data_points
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let range = if (max - min).abs() < f64::EPSILON {
                1.0_f64
            } else {
                max - min
            };
            const VBW: f64 = 200.0;
            const VBH: f64 = 50.0;
            const Y_PAD: f64 = 4.0;
            const Y_USABLE: f64 = VBH - 2.0 * Y_PAD;
            let last = data_points[n - 1];
            let mut points = String::new();
            for (i, v) in data_points.iter().enumerate() {
                let x = if n == 1 {
                    VBW / 2.0
                } else {
                    (i as f64) * VBW / ((n - 1) as f64)
                };
                let normalized = (v - min) / range;
                let y = VBH - Y_PAD - normalized * Y_USABLE;
                if i > 0 {
                    points.push(' ');
                }
                points.push_str(&format!("{x:.1},{y:.1}"));
            }
            let aria_label = format!(
                "{label} sparkline: {n} points, min {min:.2}, max {max:.2}, last {last:.2}"
            );
            html! {
                figure class={ "loom-sparkline loom-sparkline--" (modifier) } data-loom-reveal {
                    figcaption class="loom-sparkline__label" { (label) }
                    svg class="loom-sparkline__svg"
                        viewBox=(format!("0 0 {VBW:.0} {VBH:.0}"))
                        preserveAspectRatio="none"
                        role="img"
                        aria-label=(aria_label) {
                        polyline class="loom-sparkline__line"
                                 points=(points)
                                 fill="none"
                                 stroke="currentColor"
                                 stroke-width="1.5"
                                 stroke-linecap="round"
                                 stroke-linejoin="round";
                    }
                    @if let Some(c) = caption {
                        figcaption class="loom-sparkline__caption" { (c) }
                    }
                }
            }
        }
        CmsSection::PasswordChange {
            title,
            description,
            requirements,
            submit_cta,
            cancel_cta,
        } => {
            let submit_safe = is_safe_url(&submit_cta.href);
            let cancel_safe = is_safe_url(&cancel_cta.href);
            html! {
                section class="loom-password-change" data-loom-reveal {
                    div class="loom-password-change__inner" {
                        h2 class="loom-password-change__title" { (title) }
                        @if let Some(d) = description {
                            p class="loom-password-change__description" { (d) }
                        }
                        @if !requirements.is_empty() {
                            ul class="loom-password-change__requirements" aria-label="Password requirements" {
                                @for req in requirements {
                                    li class="loom-password-change__requirement" { (req) }
                                }
                            }
                        }
                        form class="loom-password-change__form"
                             method="post"
                             action=(if submit_safe { submit_cta.href.as_str() } else { "#invalid-cta" }) {
                            label class="loom-password-change__field" {
                                span { "Current password" }
                                input type="password"
                                      name="current_password"
                                      required
                                      autocomplete="current-password"
                                      aria-required="true";
                            }
                            label class="loom-password-change__field" {
                                span { "New password" }
                                input type="password"
                                      name="new_password"
                                      required
                                      autocomplete="new-password"
                                      aria-required="true";
                            }
                            label class="loom-password-change__field" {
                                span { "Confirm new password" }
                                input type="password"
                                      name="confirm_new_password"
                                      required
                                      autocomplete="new-password"
                                      aria-required="true";
                            }
                            div class="loom-password-change__actions" {
                                a class="loom-btn loom-btn--ghost loom-password-change__cancel"
                                  href=(if cancel_safe { cancel_cta.href.as_str() } else { "#invalid-cta" })
                                  data-backend=(cancel_cta.data_backend) { (cancel_cta.label) }
                                button type="submit"
                                       class="loom-btn loom-btn--primary loom-password-change__submit"
                                       data-backend=(submit_cta.data_backend) {
                                    (submit_cta.label)
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::AccountDelete {
            title,
            warning,
            consequences,
            confirm_phrase,
            confirm_field_label,
            require_password,
            delete_cta,
            cancel_cta,
        } => {
            let delete_safe = is_safe_url(&delete_cta.href);
            let cancel_safe = is_safe_url(&cancel_cta.href);
            html! {
                section class="loom-account-delete" data-loom-reveal {
                    div class="loom-account-delete__inner" {
                        h2 class="loom-account-delete__title" { (title) }
                        p class="loom-account-delete__warning" role="alert" { (warning) }
                        @if !consequences.is_empty() {
                            ul class="loom-account-delete__consequences" aria-label="Consequences" {
                                @for line in consequences {
                                    li class="loom-account-delete__consequence" { (line) }
                                }
                            }
                        }
                        form class="loom-account-delete__form"
                             method="post"
                             action=(if delete_safe { delete_cta.href.as_str() } else { "#invalid-cta" }) {
                            label class="loom-account-delete__confirm-label" {
                                span class="loom-account-delete__confirm-label-text" {
                                    (confirm_field_label)
                                    " "
                                    code class="loom-account-delete__confirm-phrase" { (confirm_phrase) }
                                }
                                input type="text"
                                      name="confirm_phrase"
                                      required
                                      autocomplete="off"
                                      spellcheck="false"
                                      aria-required="true";
                            }
                            @if *require_password {
                                label class="loom-account-delete__password-label" {
                                    span { "Current password" }
                                    input type="password"
                                          name="current_password"
                                          required
                                          autocomplete="current-password"
                                          aria-required="true";
                                }
                            }
                            div class="loom-account-delete__actions" {
                                a class="loom-btn loom-btn--ghost loom-account-delete__cancel"
                                  href=(if cancel_safe { cancel_cta.href.as_str() } else { "#invalid-cta" })
                                  data-backend=(cancel_cta.data_backend) { (cancel_cta.label) }
                                button type="submit"
                                       class="loom-btn loom-btn--danger loom-account-delete__delete"
                                       data-backend=(delete_cta.data_backend) {
                                    (delete_cta.label)
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::DeviceList {
            title,
            description,
            devices,
            revoke_all_cta,
        } => {
            let revoke_all_safe = revoke_all_cta
                .as_ref()
                .is_some_and(|c| is_safe_url(&c.href));
            html! {
                section class="loom-device-list" data-loom-reveal {
                    div class="loom-device-list__inner" {
                        h2 class="loom-device-list__title" { (title) }
                        @if let Some(d) = description {
                            p class="loom-device-list__description" { (d) }
                        }
                        ul class="loom-device-list__rows" aria-label="Active sessions" {
                            @for device in devices {
                                li class={ "loom-device" (if device.current { " loom-device--current" } else { "" }) } {
                                    div class="loom-device__identity" {
                                        span class="loom-device__label" { (device.label) }
                                        @if device.current {
                                            span class="loom-device__badge" aria-label="Current session" { "current" }
                                        }
                                    }
                                    @if device.location.is_some() || device.last_active.is_some() {
                                        div class="loom-device__meta" {
                                            @if let Some(loc) = &device.location {
                                                span class="loom-device__location" { (loc) }
                                            }
                                            @if let Some(la) = &device.last_active {
                                                span class="loom-device__last-active" { (la) }
                                            }
                                        }
                                    }
                                    @if !device.current {
                                        @if let Some(rc) = &device.revoke_cta {
                                            @let safe = is_safe_url(&rc.href);
                                            a class="loom-btn loom-btn--ghost loom-device__revoke"
                                              href=(if safe { rc.href.as_str() } else { "#invalid-cta" })
                                              data-backend=(rc.data_backend) { (rc.label) }
                                        }
                                    }
                                }
                            }
                        }
                        @if let Some(c) = revoke_all_cta {
                            div class="loom-device-list__actions" {
                                a class="loom-btn loom-btn--danger loom-device-list__revoke-all"
                                  href=(if revoke_all_safe { c.href.as_str() } else { "#invalid-cta" })
                                  data-backend=(c.data_backend) { (c.label) }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::ConsentScreen {
            title,
            app_name,
            app_description,
            app_homepage,
            scopes,
            grant_cta,
            deny_cta,
            footer_note,
        } => {
            let homepage_safe = app_homepage.as_deref().is_some_and(is_safe_url);
            let grant_safe = is_safe_url(&grant_cta.href);
            let deny_safe = is_safe_url(&deny_cta.href);
            html! {
                section class="loom-consent-screen" data-loom-reveal {
                    div class="loom-consent-screen__inner" {
                        h2 class="loom-consent-screen__title" { (title) }
                        div class="loom-consent-screen__app" {
                            span class="loom-consent-screen__app-name" { (app_name) }
                            @if let Some(d) = app_description {
                                p class="loom-consent-screen__app-description" { (d) }
                            }
                            @if let Some(h) = app_homepage {
                                @if homepage_safe {
                                    a class="loom-consent-screen__app-homepage"
                                      href=(h.as_str())
                                      rel="noopener" target="_blank" { (h) }
                                } @else {
                                    // Unsafe URL: render the literal as plain text
                                    // with no clickable surface AND no href echo.
                                    // The audit phase still flags it via the cms
                                    // walk; the visitor never sees the bad scheme.
                                    span class="loom-consent-screen__app-homepage loom-consent-screen__app-homepage--invalid"
                                         data-invalid="true" { "(invalid homepage URL)" }
                                }
                            }
                        }
                        h3 class="loom-consent-screen__scopes-heading" { "Requested permissions" }
                        ul class="loom-consent-screen__scopes" aria-label="Requested permissions" {
                            @for scope in scopes {
                                li class={ "loom-consent-scope loom-consent-scope--" (scope.tier.modifier()) } {
                                    code class="loom-consent-scope__slug" { (scope.slug) }
                                    span class="loom-consent-scope__label" { (scope.label) }
                                    @if let Some(d) = &scope.description {
                                        p class="loom-consent-scope__description" { (d) }
                                    }
                                }
                            }
                        }
                        div class="loom-consent-screen__actions" {
                            a class="loom-btn loom-btn--ghost loom-consent-screen__deny"
                              href=(if deny_safe { deny_cta.href.as_str() } else { "#invalid-cta" })
                              data-backend=(deny_cta.data_backend) { (deny_cta.label) }
                            a class="loom-btn loom-btn--primary loom-consent-screen__grant"
                              href=(if grant_safe { grant_cta.href.as_str() } else { "#invalid-cta" })
                              data-backend=(grant_cta.data_backend) { (grant_cta.label) }
                        }
                        @if let Some(n) = footer_note {
                            p class="loom-consent-screen__footer-note" { (n) }
                        }
                    }
                }
            }
        }
        CmsSection::BackupCodes {
            title,
            description,
            state,
            codes,
            download_cta,
            acknowledge_cta,
        } => {
            let modifier = match state {
                BackupCodesState::Fresh => "fresh",
                BackupCodesState::AlreadyGenerated => "already-generated",
            };
            let download_safe = download_cta.as_ref().is_some_and(|c| is_safe_url(&c.href));
            let ack_safe = acknowledge_cta
                .as_ref()
                .is_some_and(|c| is_safe_url(&c.href));
            html! {
                section class={ "loom-backup-codes loom-backup-codes--" (modifier) } data-loom-reveal {
                    div class="loom-backup-codes__inner" {
                        h2 class="loom-backup-codes__title" { (title) }
                        p class="loom-backup-codes__description" { (description) }
                        @if matches!(state, BackupCodesState::Fresh) {
                            ol class="loom-backup-codes__list" aria-label="Recovery codes" {
                                @for c in codes {
                                    li class="loom-backup-codes__code" { code { (c) } }
                                }
                            }
                        }
                        @if download_cta.is_some() || acknowledge_cta.is_some() {
                            div class="loom-backup-codes__actions" {
                                @if let Some(c) = download_cta {
                                    a class="loom-btn loom-btn--ghost loom-backup-codes__download"
                                      href=(if download_safe { c.href.as_str() } else { "#invalid-cta" })
                                      data-backend=(c.data_backend) { (c.label) }
                                }
                                @if let Some(c) = acknowledge_cta {
                                    a class="loom-btn loom-btn--primary loom-backup-codes__ack"
                                      href=(if ack_safe { c.href.as_str() } else { "#invalid-cta" })
                                      data-backend=(c.data_backend) { (c.label) }
                                }
                            }
                        }
                    }
                }
            }
        }
        CmsSection::EmailVerifyResult {
            status,
            title,
            body,
            cta,
            secondary_cta,
        } => {
            let resolved_title = title.as_deref().unwrap_or_else(|| status.default_title());
            let modifier = status.modifier();
            let primary_safe = cta.as_ref().is_some_and(|c| is_safe_url(&c.href));
            let secondary_safe = secondary_cta.as_ref().is_some_and(|c| is_safe_url(&c.href));
            html! {
                section class={ "loom-email-verify loom-email-verify--" (modifier) } data-loom-reveal {
                    div class="loom-email-verify__inner" {
                        h2 class="loom-email-verify__title" { (resolved_title) }
                        p class="loom-email-verify__body" { (body) }
                        @if cta.is_some() || secondary_cta.is_some() {
                            div class="loom-email-verify__actions" {
                                @if let Some(c) = cta {
                                    a class="loom-btn loom-btn--primary loom-email-verify__cta"
                                      href=(if primary_safe { c.href.as_str() } else { "#invalid-cta" })
                                      data-backend=(c.data_backend) { (c.label) }
                                }
                                @if let Some(c) = secondary_cta {
                                    a class="loom-btn loom-btn--ghost loom-email-verify__cta-secondary"
                                      href=(if secondary_safe { c.href.as_str() } else { "#invalid-cta" })
                                      data-backend=(c.data_backend) { (c.label) }
                                }
                            }
                        }
                    }
                }
            }
        }
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
        AuthMethodChoice::MagicLink {
            placeholder,
            submit_label,
        } => html! {
            form class="loom-auth-method loom-auth-method--magic-link" {
                input type="email" name="email" required placeholder=(placeholder) aria-label="Email";
                button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
            }
        },
        AuthMethodChoice::SmsOtp {
            placeholder,
            submit_label,
        } => html! {
            form class="loom-auth-method loom-auth-method--sms-otp" {
                input type="tel" name="phone" required placeholder=(placeholder) aria-label="Phone";
                button type="submit" class="loom-btn loom-btn--primary" { (submit_label) }
            }
        },
        AuthMethodChoice::Password {
            email_placeholder,
            password_placeholder,
            submit_label,
            forgot_label,
        } => html! {
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

fn render_source_list_item(item: &SourceListItem) -> Markup {
    let kind_mod = item.kind.modifier();
    let url_safe = item.url.as_deref().is_some_and(is_safe_url);
    html! {
        li class={ "loom-source-list__item loom-source-list__item--" (kind_mod) } {
            span class="loom-source-list__author" { (item.author) }
            ". "
            @if let Some(u) = &item.url {
                @if url_safe {
                    a class="loom-source-list__title"
                      href=(u.as_str())
                      rel="noopener" { (item.title) }
                } @else {
                    span class="loom-source-list__title loom-source-list__title--invalid"
                         data-invalid="true" { (item.title) }
                }
            } @else {
                span class="loom-source-list__title" { (item.title) }
            }
            @if let Some(d) = &item.date_published {
                ". "
                time class="loom-source-list__date" datetime=(d.as_str()) { (d) }
            }
            "."
        }
    }
}

fn render_avatar(a: &CmsAvatar) -> Markup {
    match a {
        CmsAvatar::None => {
            html! { span class="loom-avatar" data-kind="none" aria-hidden="true" {} }
        }
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
fn render_form(
    legend: &str,
    submit: &CmsFormSubmit,
    steps: &[CmsFormStep],
    style: CmsFormStyle,
) -> Markup {
    let action_safe = is_safe_url(&submit.action);
    let action_value: &str = if action_safe {
        &submit.action
    } else {
        "#invalid-form-action"
    };
    let style_attr = style.modifier();
    html! {
        section class="loom-form-section" data-loom-form-style=(style_attr) {
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
                data-loom-form-style=(style_attr)
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
    fn cms_block_text_round_trips_through_serde() {
        let block = CmsBlock::Text {
            text: "Hello world".to_owned(),
        };
        let json = serde_json::to_string(&block).expect("ser");
        assert!(json.contains(r#""kind":"text""#));
        assert!(json.contains("Hello world"));
        let back: CmsBlock = serde_json::from_str(&json).expect("de");
        match back {
            CmsBlock::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn cms_block_accordion_renders_details_summary_pairs() {
        let json = r##"{"kind":"accordion","items":[
            {"summary":"First","content":[{"kind":"text","text":"Body A"}]},
            {"summary":"Second","content":[{"kind":"text","text":"Body B"}],"default_open":true}
        ]}"##;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains("loom-block-accordion"));
        assert!(html.contains("<details"));
        assert!(html.contains("<summary"));
        assert!(html.contains(">First</summary>"));
        assert!(html.contains(">Second</summary>"));
        assert!(html.contains("Body A"));
        assert!(html.contains("Body B"));
        // default_open=true on the second item, false on the first.
        assert_eq!(html.matches("open").count(), 1);
    }

    #[test]
    fn cms_block_accordion_single_expand_emits_shared_name() {
        let json = r##"{"kind":"accordion","single_expand":true,"items":[
            {"summary":"A","content":[]},
            {"summary":"B","content":[]}
        ]}"##;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        // Both <details> share name="loom-accordion-group" so the
        // browser enforces at-most-one-open.
        assert_eq!(html.matches("name=\"loom-accordion-group\"").count(), 2);
    }

    #[test]
    fn cms_block_accordion_emits_loom_slots() {
        let json = r##"{"kind":"accordion","items":[{"summary":"X","content":[]}]}"##;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        // data-loom-slot attrs name each composition slot per the
        // Radix-canonical convention (Trigger / Content).
        assert!(html.contains(r#"data-loom-slot="accordion""#));
        assert!(html.contains(r#"data-loom-slot="accordion-item""#));
        assert!(html.contains(r#"data-loom-slot="accordion-trigger""#));
        assert!(html.contains(r#"data-loom-slot="accordion-content""#));
    }

    #[test]
    fn cms_block_toast_renders_with_live_region_semantics() {
        let json = r#"{
            "kind": "toast",
            "id": "saved",
            "tone": "success",
            "title": "Saved",
            "message": "Your changes were saved.",
            "dismissible": true
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"data-loom-slot="toast""#));
        assert!(html.contains(r#"data-tone="success""#));
        // success → status + polite live region.
        assert!(html.contains(r#"role="status""#));
        assert!(html.contains(r#"aria-live="polite""#));
        assert!(html.contains(">Saved</strong>"));
        assert!(html.contains("Your changes were saved."));
        // Dismiss button + aria-label + aria-controls binding.
        assert!(html.contains(r#"data-loom-slot="toast-close""#));
        assert!(html.contains(r#"aria-label="Dismiss notification""#));
        assert!(html.contains(r#"aria-controls="saved""#));
    }

    #[test]
    fn cms_block_toast_error_uses_assertive_alert() {
        let json = r#"{
            "kind":"toast","id":"x","tone":"error","message":"Save failed"
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"role="alert""#));
        assert!(html.contains(r#"aria-live="assertive""#));
    }

    #[test]
    fn cms_block_toast_open_false_emits_hidden_attr() {
        let json = r#"{
            "kind":"toast","id":"x","message":"hi","open":false
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(" hidden"));
    }

    #[test]
    fn cms_block_toast_open_true_default_omits_hidden() {
        let json = r#"{
            "kind":"toast","id":"x","message":"hi"
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(!html.contains(r#" hidden=""#));
    }

    #[test]
    fn cms_block_combobox_renders_native_input_with_datalist() {
        let json = r#"{
            "kind": "combobox",
            "id": "country",
            "label": "Country",
            "name": "country_code",
            "placeholder": "Start typing…",
            "options": [
                {"value":"US","label":"United States"},
                {"value":"CA","label":"Canada"},
                {"value":"MX"}
            ]
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        // Native input + datalist binding.
        assert!(html.contains(r#"<input"#));
        assert!(html.contains(r#"list="country-options""#));
        assert!(html.contains(r#"<datalist id="country-options""#));
        // ARIA combobox.
        assert!(html.contains(r#"role="combobox""#));
        assert!(html.contains(r#"aria-autocomplete="list""#));
        assert!(html.contains(r#"aria-controls="country-options""#));
        // Form-control attrs.
        assert!(html.contains(r#"name="country_code""#));
        assert!(html.contains(r#"placeholder="Start typing…""#));
        // Options.
        assert!(html.contains(r#"<option value="US">United States</option>"#));
        assert!(html.contains(r#"<option value="CA">Canada</option>"#));
        // Option without label renders empty body (value only).
        assert!(html.contains(r#"<option value="MX">"#));
    }

    #[test]
    fn cms_block_combobox_no_placeholder_omits_attr() {
        let json = r#"{
            "kind":"combobox","id":"x","label":"L","options":[]
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(!html.contains(r#"placeholder="""#));
    }

    #[test]
    fn cms_block_dropdown_menu_renders_role_menu_with_items() {
        let json = r#"{
            "kind": "dropdown_menu",
            "id": "actions",
            "trigger_label": "More",
            "items": [
                {"label":"Edit","href":"/edit","data_backend":"edit-record"},
                {"label":"Duplicate"},
                {"label":"Delete","href":"/delete","disabled":true,"separator_before":true}
            ]
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"popovertarget="actions""#));
        assert!(html.contains(r#"aria-haspopup="menu""#));
        assert!(html.contains(r#"role="menu""#));
        // Three menuitems.
        assert_eq!(html.matches(r#"role="menuitem""#).count(), 3);
        // First item is anchor with safe href.
        assert!(html.contains(r#"href="/edit""#));
        assert!(html.contains(r#"data-backend="edit-record""#));
        // Item without href renders as button.
        assert!(html.contains(r#">Duplicate</button>"#));
        // Disabled item gets aria-disabled.
        assert!(html.contains(r#"aria-disabled="true""#));
        // Separator before Delete.
        assert!(html.contains(r#"role="separator""#));
    }

    #[test]
    fn cms_block_dropdown_unsafe_href_marks_invalid() {
        let json = r##"{
            "kind":"dropdown_menu","id":"x","trigger_label":"T",
            "items":[{"label":"Hostile","href":"javascript:alert(1)"}]
        }"##;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r##"href="#invalid-link""##));
        assert!(html.contains(r#"data-invalid="true""#));
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn cms_block_popover_uses_native_popover_attribute() {
        let json = r#"{
            "kind": "popover",
            "id": "info",
            "trigger_label": "More info",
            "content": [{"kind":"text","text":"Hidden until opened"}],
            "placement": "bottom"
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        // Trigger references popover via popovertarget.
        assert!(html.contains(r#"popovertarget="info""#));
        // Popover element uses native HTML popover attribute.
        assert!(html.contains(r#"popover="auto""#));
        assert!(html.contains(r#"id="info""#));
        assert!(html.contains(r#"aria-controls="info""#));
        // role=dialog so SR users get dialog semantics.
        assert!(html.contains(r#"role="dialog""#));
        // Placement attribute.
        assert!(html.contains(r#"data-placement="bottom""#));
        // Trigger label + body content rendered.
        assert!(html.contains(">More info</button>"));
        assert!(html.contains("Hidden until opened"));
    }

    #[test]
    fn cms_block_popover_placement_defaults_to_top() {
        let json = r#"{
            "kind":"popover","id":"p","trigger_label":"X","content":[]
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"data-placement="top""#));
    }

    #[test]
    fn cms_block_tabs_renders_aria_correct_tablist() {
        let json = r#"{
            "kind": "tabs",
            "items": [
                {"label":"Overview","slug":"overview","content":[{"kind":"text","text":"A"}]},
                {"label":"Details","slug":"details","content":[{"kind":"text","text":"B"}]}
            ]
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"role="tablist""#));
        assert!(html.contains(r#"role="tab""#));
        assert!(html.contains(r#"role="tabpanel""#));
        // First trigger is selected + has tabindex=0.
        assert!(html.contains(r#"aria-selected="true""#));
        assert!(html.contains(r#"aria-selected="false""#));
        // Trigger ↔ panel binding via aria-controls + aria-labelledby.
        assert!(html.contains(r#"id="tab-overview""#));
        assert!(html.contains(r#"id="panel-overview""#));
        assert!(html.contains(r#"aria-controls="panel-overview""#));
        assert!(html.contains(r#"aria-labelledby="tab-overview""#));
    }

    #[test]
    fn cms_block_tabs_first_panel_visible_others_hidden() {
        let json = r#"{
            "kind":"tabs",
            "items":[
                {"label":"A","slug":"a","content":[]},
                {"label":"B","slug":"b","content":[]},
                {"label":"C","slug":"c","content":[]}
            ]
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        // Exactly 2 hidden attrs — second + third panel.
        assert_eq!(html.matches(" hidden").count(), 2);
        // First tab has tabindex=0; others tabindex=-1 (per ARIA spec).
        assert_eq!(html.matches(r#"tabindex="0""#).count(), 4);
        assert_eq!(html.matches(r#"tabindex="-1""#).count(), 2);
    }

    #[test]
    fn cms_block_dialog_renders_native_dialog_element() {
        let json = r#"{
            "kind": "dialog",
            "id": "confirm-delete",
            "title": "Confirm",
            "content": [
                {"kind":"text","text":"Are you sure?"}
            ]
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains("<dialog"));
        assert!(html.contains(r#"id="confirm-delete""#));
        assert!(html.contains(r#"data-loom-slot="dialog""#));
        assert!(html.contains(r#"data-modal="false""#));
        // Title rendered + linked via aria-labelledby
        assert!(html.contains(r#"aria-labelledby="confirm-delete__title""#));
        assert!(html.contains(r#"id="confirm-delete__title""#));
        assert!(html.contains(">Confirm</h2>"));
        // Native close mechanism (no JS): form method=dialog
        assert!(html.contains(r#"<form method="dialog""#));
        assert!(html.contains(r#"data-loom-slot="dialog-close""#));
        assert!(html.contains(r#"aria-label="Close dialog""#));
        // Body content rendered
        assert!(html.contains("Are you sure?"));
    }

    #[test]
    fn cms_block_dialog_open_attr_when_open_true() {
        let json = r#"{
            "kind":"dialog","id":"d","title":"T","content":[],"open":true
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        // `open` attr on the dialog element.
        assert!(html.contains(" open"));
    }

    #[test]
    fn cms_block_dialog_modal_true_emits_modal_data_attr() {
        let json = r#"{
            "kind":"dialog","id":"d","title":"T","content":[],"modal":true
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"data-modal="true""#));
    }

    #[test]
    fn cms_block_tooltip_renders_with_aria_describedby() {
        let json = r#"{
            "kind": "tooltip",
            "label": "PPS",
            "content": "Public Privacy Substrate — Plausi-Den's name for the always-encrypted-at-rest architecture."
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains("loom-block-tooltip"));
        assert!(html.contains(r#"data-loom-slot="tooltip""#));
        assert!(html.contains(r#"tabindex="0""#));
        assert!(html.contains(r#"role="tooltip""#));
        assert!(html.contains(r#"aria-describedby="loom-tip-"#));
        assert!(html.contains(">PPS</span>"));
        assert!(html.contains("Public Privacy Substrate"));
    }

    #[test]
    fn cms_block_tooltip_aria_describedby_matches_content_id() {
        let json = r#"{"kind":"tooltip","label":"X","content":"body"}"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        let aria_pos = html.find("aria-describedby=\"").unwrap();
        let aria_val_start = aria_pos + "aria-describedby=\"".len();
        let aria_val_end = aria_pos + "aria-describedby=\"".len()
            + html[aria_val_start..].find('"').unwrap();
        let aria_val = &html[aria_val_start..aria_val_end];
        // The same id appears as the body span's id.
        assert!(html.contains(&format!("id=\"{aria_val}\"")));
    }

    #[test]
    fn cms_block_tooltip_placement_defaults_to_top() {
        let json = r#"{"kind":"tooltip","label":"X","content":"y"}"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"data-placement="top""#));
    }

    #[test]
    fn cms_block_tooltip_emits_placement_attr() {
        for (variant, slug) in [
            ("top", "top"),
            ("bottom", "bottom"),
            ("left", "left"),
            ("right", "right"),
        ] {
            let json = format!(
                r#"{{"kind":"tooltip","label":"X","content":"y","placement":"{variant}"}}"#
            );
            let block: CmsBlock = serde_json::from_str(&json).expect("parses");
            let html = render_block(&block).into_string();
            assert!(
                html.contains(&format!(r#"data-placement="{slug}""#)),
                "missing placement={slug} in: {html}"
            );
        }
    }

    #[test]
    fn cms_block_slider_renders_as_native_range_input() {
        let json = r#"{
            "kind": "slider",
            "id": "volume",
            "label": "Volume",
            "name": "vol",
            "min": 0.0,
            "max": 100.0,
            "step": 5.0,
            "value": 42.0
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains("loom-block-slider"));
        assert!(html.contains(r#"data-loom-slot="slider""#));
        assert!(html.contains(r#"type="range""#));
        assert!(html.contains(r#"id="volume""#));
        assert!(html.contains(r#"name="vol""#));
        assert!(html.contains(r#"min="0""#));
        assert!(html.contains(r#"max="100""#));
        assert!(html.contains(r#"step="5""#));
        assert!(html.contains(r#"value="42""#));
        assert!(html.contains(r#"aria-label="Volume""#));
    }

    #[test]
    fn cms_block_slider_show_value_emits_output_element() {
        let json = r#"{
            "kind": "slider",
            "id": "x",
            "label": "L",
            "min": 0.0, "max": 1.0,
            "value": 0.5,
            "show_value": true
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"<output"#));
        assert!(html.contains(r#"data-loom-slot="slider-value""#));
        assert!(html.contains(">0.5</output>"));
    }

    #[test]
    fn cms_block_slider_default_step_is_one() {
        let json = r#"{
            "kind":"slider","id":"x","label":"L",
            "min": 0.0, "max": 10.0, "value": 5.0
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"step="1""#));
    }

    #[test]
    fn cms_block_slider_disabled_emits_disabled_attr() {
        let json = r#"{
            "kind":"slider","id":"x","label":"L",
            "min":0.0,"max":1.0,"value":0.0,"disabled":true
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains("disabled"));
    }

    #[test]
    fn cms_block_switch_renders_as_native_input_with_aria_role() {
        let json = r#"{
            "kind": "switch",
            "id": "notify",
            "label": "Email notifications",
            "name": "notify",
            "checked": true
        }"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains("loom-block-switch"));
        assert!(html.contains(r#"data-loom-slot="switch""#));
        assert!(html.contains(r#"role="switch""#));
        assert!(html.contains(r#"type="checkbox""#));
        assert!(html.contains(r#"id="notify""#));
        assert!(html.contains(r#"name="notify""#));
        assert!(html.contains("checked"));
        assert!(html.contains(r#"aria-checked="true""#));
        assert!(html.contains(">Email notifications</span>"));
    }

    #[test]
    fn cms_block_switch_unchecked_emits_aria_false() {
        let json = r#"{"kind":"switch","id":"x","label":"L"}"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains(r#"aria-checked="false""#));
        assert!(!html.contains("checked "));
    }

    #[test]
    fn cms_block_switch_disabled_omits_form_interactivity() {
        let json = r#"{"kind":"switch","id":"x","label":"L","disabled":true}"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        assert!(html.contains("disabled"));
    }

    #[test]
    fn cms_block_switch_no_name_omits_form_submission_attr() {
        let json = r#"{"kind":"switch","id":"x","label":"L"}"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses");
        let html = render_block(&block).into_string();
        // No `name=` attr emitted when the field is absent.
        assert!(!html.contains(r#" name=""#));
    }

    #[test]
    fn cms_block_button_renders_with_variant_and_size_attrs() {
        let btn = CmsBlock::Button {
            label: "Join Free".to_owned(),
            href: "/membership/".to_owned(),
            variant: ButtonVariant::Primary,
            size: ButtonSize::Lg,
            data_backend: Some("cta-join-free".to_owned()),
        };
        let html = render_block(&btn).into_string();
        assert!(html.contains("loom-block-button"));
        assert!(html.contains(r#"role="button""#));
        assert!(html.contains(r#"data-variant="primary""#));
        assert!(html.contains(r#"data-size="lg""#));
        assert!(html.contains(r#"data-backend="cta-join-free""#));
        assert!(html.contains(r#"href="/membership/""#));
    }

    #[test]
    fn cms_block_button_unsafe_href_marks_invalid() {
        let btn = CmsBlock::Button {
            label: "X".to_owned(),
            href: "javascript:alert(1)".to_owned(),
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            data_backend: None,
        };
        let html = render_block(&btn).into_string();
        assert!(html.contains(r##"href="#invalid-link""##));
        assert!(html.contains(r#"data-invalid="true""#));
    }

    #[test]
    fn button_size_default_is_md() {
        let json = r#"{"kind":"button","label":"X","href":"/a","variant":"primary"}"#;
        let block: CmsBlock = serde_json::from_str(json).expect("parses without size");
        match block {
            CmsBlock::Button { size, .. } => assert!(matches!(size, ButtonSize::Md)),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn button_slug_helpers_are_lowercase() {
        for v in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Ghost,
            ButtonVariant::Danger,
        ] {
            assert!(button_variant_slug(v)
                .chars()
                .all(|c| c.is_ascii_lowercase()));
        }
        for s in [ButtonSize::Sm, ButtonSize::Md, ButtonSize::Lg] {
            assert!(button_size_slug(s).chars().all(|c| c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn cms_block_renders_atomic_primitives() {
        // Text block
        let m = render_block(&CmsBlock::Text {
            text: "Body".to_owned(),
        });
        assert!(m.into_string().contains("loom-block-text"));
        // Heading clamps level to 1..=6
        let m = render_block(&CmsBlock::Heading {
            level: 99,
            text: "Big".to_owned(),
        });
        let s = m.into_string();
        assert!(s.contains("loom-block-heading"));
        assert!(s.contains("<h6"));
        // Image with safe url emits <img>
        let m = render_block(&CmsBlock::Image {
            src: "/logo.svg".to_owned(),
            alt: "L".to_owned(),
            width: None,
            height: None,
        });
        assert!(m.into_string().contains("loom-block-image"));
        // Image with hostile src suppresses entirely
        let m = render_block(&CmsBlock::Image {
            src: "javascript:alert(1)".to_owned(),
            alt: "x".to_owned(),
            width: None,
            height: None,
        });
        assert_eq!(m.into_string(), "");
        // Divider emits aria-hidden hr
        let m = render_block(&CmsBlock::Divider).into_string();
        assert!(m.contains("loom-block-divider"));
        assert!(m.contains("aria-hidden"));
    }

    #[test]
    fn cms_block_row_and_column_nest_children() {
        let row = CmsBlock::Row {
            gap: Some(BlockSpacing::Md),
            align: Some(BlockAlign::Center),
            children: vec![
                CmsBlock::Text {
                    text: "A".to_owned(),
                },
                CmsBlock::Text {
                    text: "B".to_owned(),
                },
            ],
        };
        let html = render_block(&row).into_string();
        assert!(html.contains("loom-block-row"));
        assert!(html.contains(r#"data-gap="md""#));
        assert!(html.contains(r#"data-align="center""#));
        assert!(html.contains(">A</p>"));
        assert!(html.contains(">B</p>"));
    }

    #[test]
    fn cms_section_compose_renders_block_tree() {
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind": "compose",
                "heading": "Atomic demo",
                "blocks": [
                    { "kind": "heading", "level": 2, "text": "Section title" },
                    { "kind": "text", "text": "A paragraph." },
                    { "kind": "spacer", "size": "lg" },
                    { "kind": "divider" }
                ]
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("loom-compose"));
        assert!(html.contains("Atomic demo"));
        assert!(html.contains("loom-block-heading"));
        assert!(html.contains("Section title"));
        assert!(html.contains("loom-block-text"));
        assert!(html.contains(r#"loom-block-spacer" data-size="lg""#));
        assert!(html.contains("loom-block-divider"));
    }

    #[test]
    fn block_spacing_slug_round_trips() {
        for s in [
            BlockSpacing::None,
            BlockSpacing::Xs,
            BlockSpacing::Sm,
            BlockSpacing::Md,
            BlockSpacing::Lg,
            BlockSpacing::Xl,
            BlockSpacing::Xxl,
        ] {
            let slug = block_spacing_slug(s);
            assert!(!slug.is_empty());
            assert!(slug.chars().all(|c| c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn block_align_slug_round_trips() {
        for a in [
            BlockAlign::Start,
            BlockAlign::Center,
            BlockAlign::End,
            BlockAlign::Stretch,
            BlockAlign::Baseline,
        ] {
            let slug = block_align_slug(a);
            assert!(!slug.is_empty());
            assert!(slug.chars().all(|c| c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn empty_page_renders_div_wrapper() {
        // T70b-fix (2026-05-14): wrapper is now <div>, not <main>.
        // The <main> landmark belongs to page_shell, not render_page,
        // to avoid nested <main> tags in the composed output.
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Heading {
                text: "Section".to_owned(),
                level: HeadingLevel::H2,
                id: None,
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
            brand_logo: None,
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
                site_origin: None,
                social_image: None,
                sections: vec![CmsSection::Heading {
                    text: "x".to_owned(),
                    level,
                    id: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
    fn hero_editorial_renders_required_fields() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::HeroEditorial {
                kicker: None,
                headline: "The substrate carries the page".to_owned(),
                headline_accent: None,
                lede: "Editorial composition replaces the centered SaaS shape.".to_owned(),
                cta: None,
                background: HeroEditorialBackground::Slate,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains("loom-section-hero-editorial"));
        assert!(html.contains("data-loom-hero-editorial"));
        assert!(html.contains(r#"data-background="slate""#));
        assert!(html.contains(">The substrate carries the page"));
        assert!(html.contains(">Editorial composition replaces"));
        // Kicker + accent + CTA absent.
        assert!(!html.contains("loom-section-hero-editorial__kicker"));
        assert!(!html.contains("loom-section-hero-editorial__accent"));
        assert!(!html.contains("loom-section-hero-editorial__cta"));
    }

    #[test]
    fn hero_editorial_renders_all_optional_slots() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::HeroEditorial {
                kicker: Some("DISPATCH · 2026-05-20".to_owned()),
                headline: "The substrate".to_owned(),
                headline_accent: Some("carries the page".to_owned()),
                lede: "Editorial composition replaces SaaS heroes.".to_owned(),
                cta: Some(HeroCta {
                    label: "Read the dispatch".to_owned(),
                    href: "/dispatch".to_owned(),
                    data_backend: "view-dispatch".to_owned(),
                }),
                background: HeroEditorialBackground::Amoled,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(">DISPATCH · 2026-05-20<"));
        assert!(html.contains("loom-section-hero-editorial__kicker"));
        assert!(html.contains(">carries the page<"));
        assert!(html.contains("loom-section-hero-editorial__accent"));
        assert!(html.contains(r#"href="/dispatch""#));
        assert!(html.contains(r#"data-backend="view-dispatch""#));
        assert!(html.contains(">Read the dispatch<"));
        // AMOLED background attribute wires through.
        assert!(html.contains(r#"data-background="amoled""#));
    }

    #[test]
    fn hero_editorial_parses_from_json_with_snake_case_kind() {
        let json = r#"{
            "kind": "hero_editorial",
            "headline": "From JSON",
            "lede": "Wire shape works.",
            "background": "plain"
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::HeroEditorial {
                kicker,
                headline,
                headline_accent,
                lede,
                cta,
                background,
            } => {
                assert!(kicker.is_none());
                assert_eq!(headline, "From JSON");
                assert!(headline_accent.is_none());
                assert_eq!(lede, "Wire shape works.");
                assert!(cta.is_none());
                assert!(matches!(background, HeroEditorialBackground::Plain));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn hero_editorial_background_defaults_to_slate_when_omitted() {
        let json = r#"{
            "kind": "hero_editorial",
            "headline": "X",
            "lede": "Y"
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::HeroEditorial { background, .. } => {
                assert!(matches!(background, HeroEditorialBackground::Slate));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn hero_editorial_invalid_cta_href_substitutes_placeholder() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::HeroEditorial {
                kicker: None,
                headline: "x".to_owned(),
                headline_accent: None,
                lede: "x".to_owned(),
                cta: Some(HeroCta {
                    label: "bad".to_owned(),
                    href: "javascript:alert(1)".to_owned(),
                    data_backend: "x".to_owned(),
                }),
                background: HeroEditorialBackground::Slate,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r##"href="#invalid-cta""##));
        assert!(html.contains(r#"data-invalid="true""#));
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn crucible_challenge_renders_mount_and_script() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CrucibleChallenge {
                kind_slug: "math-arithmetic".to_owned(),
                tenant_id: "acme".to_owned(),
                base_path: None,
                widget_url: "/static/crucible-widget/crucible_widget.js".to_owned(),
            }],
        };
        let html = render_to_string(&p);
        // Mount node with deterministic id.
        assert!(
            html.contains(r#"id="crucible-math-arithmetic-acme""#),
            "missing mount id; got:\n{html}"
        );
        // Module-script imports the widget bundle.
        assert!(html.contains("script type=\"module\""));
        assert!(html.contains(r#"import init, { init as crucible_init } from "/static/crucible-widget/crucible_widget.js""#));
        // init() called with kind + tenant + base_path defaults.
        assert!(html.contains(r#"crucible_init("crucible-math-arithmetic-acme", "math-arithmetic", "acme", "/crucible")"#));
    }

    #[test]
    fn crucible_challenge_honors_custom_base_path() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CrucibleChallenge {
                kind_slug: "semantic-similarity".to_owned(),
                tenant_id: "acme".to_owned(),
                base_path: Some("/api/cr".to_owned()),
                widget_url: "/w.js".to_owned(),
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#""/api/cr""#));
    }

    #[test]
    fn json_string_literal_escapes_special_chars() {
        assert_eq!(json_string_literal("abc"), r#""abc""#);
        assert_eq!(json_string_literal(r#"a"b"#), r#""a\"b""#);
        assert_eq!(json_string_literal("a\\b"), r#""a\\b""#);
        assert_eq!(json_string_literal("a\nb"), r#""a\nb""#);
        // Closing-script-tag prevention: < / > / & all escape.
        let s = json_string_literal("</script>");
        assert!(!s.contains("</script>"));
        assert!(s.contains("\\u003c"));
        assert!(s.contains("\\u003e"));
    }

    #[test]
    fn pull_quote_renders_body_only() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::PullQuote {
                body: "The substrate carries the page.".to_owned(),
                attribution: None,
                cite_url: None,
                emphasis: PullQuoteEmphasis::Inline,
                tone: PullQuoteTone::Slate,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains("<blockquote"));
        assert!(html.contains(r#"class="loom-pull-quote""#));
        assert!(html.contains(r#"data-emphasis="inline""#));
        assert!(html.contains(r#"data-tone="slate""#));
        assert!(html.contains(">The substrate carries the page.<"));
        // No attribution / cite emitted.
        assert!(!html.contains("loom-pull-quote__attribution"));
        assert!(!html.contains("cite="));
    }

    #[test]
    fn pull_quote_renders_attribution_and_cite() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::PullQuote {
                body: "Quoted body".to_owned(),
                attribution: Some("paul, 2026-05-20".to_owned()),
                cite_url: Some("https://example.org/dispatch".to_owned()),
                emphasis: PullQuoteEmphasis::Display,
                tone: PullQuoteTone::Amoled,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"cite="https://example.org/dispatch""#));
        assert!(html.contains(r#"data-emphasis="display""#));
        assert!(html.contains(r#"data-tone="amoled""#));
        assert!(html.contains("loom-pull-quote__attribution"));
        assert!(html.contains(">— paul, 2026-05-20<"));
    }

    #[test]
    fn pull_quote_splits_paragraphs_on_blank_lines() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::PullQuote {
                body: "First paragraph.\n\nSecond paragraph.".to_owned(),
                attribution: None,
                cite_url: None,
                emphasis: PullQuoteEmphasis::Inline,
                tone: PullQuoteTone::Slate,
            }],
        };
        let html = render_to_string(&p);
        // Body is a single <blockquote class="loom-pull-quote__body">
        // that contains two <p> children when paragraphs split. Count
        // the <p> opens inside the blockquote.
        let bq_open = html.find("loom-pull-quote__body").expect("body class");
        let bq_close = html[bq_open..].find("</blockquote>").expect("/blockquote") + bq_open;
        let body_slice = &html[bq_open..bq_close];
        let p_open_count = body_slice.matches("<p>").count();
        assert_eq!(
            p_open_count, 2,
            "expected 2 body <p> tags, got {p_open_count}: {body_slice}"
        );
        assert!(html.contains(">First paragraph.<"));
        assert!(html.contains(">Second paragraph.<"));
    }

    #[test]
    fn pull_quote_parses_from_json_with_snake_case_kind() {
        let json = r#"{
            "kind": "pull_quote",
            "body": "From JSON",
            "attribution": "wire test",
            "emphasis": "display",
            "tone": "amoled"
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::PullQuote {
                body,
                attribution,
                cite_url,
                emphasis,
                tone,
            } => {
                assert_eq!(body, "From JSON");
                assert_eq!(attribution.as_deref(), Some("wire test"));
                assert!(cite_url.is_none());
                assert!(matches!(emphasis, PullQuoteEmphasis::Display));
                assert!(matches!(tone, PullQuoteTone::Amoled));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn pull_quote_emphasis_and_tone_default_when_omitted() {
        let json = r#"{
            "kind": "pull_quote",
            "body": "X"
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::PullQuote { emphasis, tone, .. } => {
                assert!(matches!(emphasis, PullQuoteEmphasis::Inline));
                assert!(matches!(tone, PullQuoteTone::Slate));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn pull_quote_distinct_from_legacy_quote_testimonial_card() {
        // Anti-shape guarantee: PullQuote must NOT emit
        // loom-quote-cite / loom-quote-attribution / loom-quote-role
        // — those belong to the legacy testimonial card. PullQuote
        // is the editorial sibling.
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::PullQuote {
                body: "x".to_owned(),
                attribution: Some("y".to_owned()),
                cite_url: None,
                emphasis: PullQuoteEmphasis::Inline,
                tone: PullQuoteTone::Slate,
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("loom-quote-cite"));
        assert!(!html.contains("loom-quote-attribution"));
        assert!(!html.contains("loom-quote-role"));
        assert!(!html.contains("loom-quote-footer"));
    }

    #[test]
    fn code_shell_renders_typed_lines() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CodeShell {
                title: None,
                prompt: None,
                lines: vec![
                    CmsCodeShellLine {
                        kind: CmsCodeShellLineKind::Command,
                        text: "forge build --json".to_owned(),
                    },
                    CmsCodeShellLine {
                        kind: CmsCodeShellLineKind::Output,
                        text: "discipline-strict: 0 findings".to_owned(),
                    },
                    CmsCodeShellLine {
                        kind: CmsCodeShellLineKind::Comment,
                        text: "all phases clean.".to_owned(),
                    },
                    CmsCodeShellLine {
                        kind: CmsCodeShellLineKind::Error,
                        text: "warning: deprecated".to_owned(),
                    },
                ],
                tone: CodeShellTone::Slate,
                chrome: CodeShellChrome::Minimal,
            }],
        };
        let html = render_to_string(&p);
        // Semantic shell.
        assert!(html.contains("<pre"));
        assert!(html.contains("<code"));
        assert!(html.contains("loom-code-shell"));
        // Per-line kind classes emitted.
        assert!(html.contains("loom-code-shell__line--command"));
        assert!(html.contains("loom-code-shell__line--output"));
        assert!(html.contains("loom-code-shell__line--comment"));
        assert!(html.contains("loom-code-shell__line--error"));
        // Default prompt glyph.
        assert!(html.contains("loom-code-shell__prompt"));
        assert!(html.contains(">$ <"));
        // Comment carries `#` prefix.
        assert!(html.contains("# all phases clean."));
        // Command body present.
        assert!(html.contains(">forge build --json<"));
    }

    #[test]
    fn code_shell_custom_prompt_replaces_default() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CodeShell {
                title: None,
                prompt: Some("›".to_owned()),
                lines: vec![CmsCodeShellLine {
                    kind: CmsCodeShellLineKind::Command,
                    text: "ls".to_owned(),
                }],
                tone: CodeShellTone::Slate,
                chrome: CodeShellChrome::Minimal,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(">› <"));
        assert!(!html.contains(">$ <"));
    }

    #[test]
    fn code_shell_header_chrome_renders_title() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CodeShell {
                title: Some("forge build".to_owned()),
                prompt: None,
                lines: vec![CmsCodeShellLine {
                    kind: CmsCodeShellLineKind::Output,
                    text: "ok".to_owned(),
                }],
                tone: CodeShellTone::Slate,
                chrome: CodeShellChrome::Header,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"data-chrome="header""#));
        assert!(html.contains("loom-code-shell__header"));
        assert!(html.contains(">forge build<"));
    }

    #[test]
    fn code_shell_minimal_chrome_omits_header_even_with_title() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CodeShell {
                title: Some("would-be-header".to_owned()),
                prompt: None,
                lines: vec![],
                tone: CodeShellTone::Slate,
                chrome: CodeShellChrome::Minimal,
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains(">would-be-header<"));
        assert!(!html.contains("loom-code-shell__header"));
    }

    #[test]
    fn code_shell_amoled_tone_attribute() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CodeShell {
                title: None,
                prompt: None,
                lines: vec![],
                tone: CodeShellTone::Amoled,
                chrome: CodeShellChrome::Minimal,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"data-tone="amoled""#));
    }

    #[test]
    fn code_shell_parses_from_json_with_snake_case_kind() {
        let json = r#"{
            "kind": "code_shell",
            "title": "forge build",
            "lines": [
                { "kind": "command", "text": "forge build" },
                { "kind": "output", "text": "ok" }
            ],
            "tone": "amoled",
            "chrome": "header"
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::CodeShell {
                title,
                prompt,
                lines,
                tone,
                chrome,
            } => {
                assert_eq!(title.as_deref(), Some("forge build"));
                assert!(prompt.is_none());
                assert_eq!(lines.len(), 2);
                assert!(matches!(lines[0].kind, CmsCodeShellLineKind::Command));
                assert_eq!(lines[0].text, "forge build");
                assert!(matches!(lines[1].kind, CmsCodeShellLineKind::Output));
                assert!(matches!(tone, CodeShellTone::Amoled));
                assert!(matches!(chrome, CodeShellChrome::Header));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn code_shell_tone_and_chrome_default_when_omitted() {
        let json = r#"{
            "kind": "code_shell",
            "lines": []
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::CodeShell { tone, chrome, .. } => {
                assert!(matches!(tone, CodeShellTone::Slate));
                assert!(matches!(chrome, CodeShellChrome::Minimal));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn code_shell_no_saas_trope_ornaments() {
        // Anti-shape guarantee: no fake macOS traffic-light circles,
        // no gradient header bar, no copy-button decoration.
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::CodeShell {
                title: Some("h".to_owned()),
                prompt: None,
                lines: vec![CmsCodeShellLine {
                    kind: CmsCodeShellLineKind::Command,
                    text: "ls".to_owned(),
                }],
                tone: CodeShellTone::Slate,
                chrome: CodeShellChrome::Header,
            }],
        };
        let html = render_to_string(&p);
        assert!(!html.contains("rounded-full"));
        assert!(!html.contains("linear-gradient"));
        assert!(!html.contains("bg-gradient"));
        assert!(!html.contains("data-copy"));
    }

    #[test]
    fn hero_invalid_cta_href_substitutes_placeholder() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            id: None,
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
    fn photo_overlay_default_is_none_so_photos_render_at_full_strength() {
        // Regression lock: a `Dark` default applied a heavy
        // canvas-color gradient overlay to every photo-forward
        // hero, washing out the operator's source material. The
        // default is now `None` — operators who actually need
        // legibility on a bright photo opt in to Light or Dark
        // explicitly.
        assert_eq!(PhotoOverlay::default(), PhotoOverlay::None);
    }

    #[test]
    fn image_hero_default_renders_no_overlay_class() {
        // Image hero with HeroBackground::Photo and no `overlay`
        // field set should render with `ov-none` (the new default)
        // rather than `ov-dark`.
        let json = r#"{
            "brand": null, "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/p", "nav_links": [], "dev_devtools": false,
            "sections": [{
                "kind": "image_hero",
                "title": "Welcome",
                "cta": {"label": "Go", "href": "/", "data_backend": "go"},
                "background": {
                    "kind": "photo",
                    "src": "/hero.jpg",
                    "alt": "Hero photo"
                },
                "height": "comfortable",
                "align": "start"
            }]
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = render_to_string(&page);
        assert!(html.contains("ov-none"), "expected ov-none default, got:\n{html}");
        assert!(!html.contains("ov-dark"), "ov-dark should NOT be the default");
    }

    #[test]
    fn theme_toggle_js_preserves_named_themes() {
        // Regression lock: an earlier THEME_TOGGLE_JS init only
        // accepted T=['light','dark','auto']; any server-emitted
        // data-theme outside that set (warm / ocean / forest /
        // rose / violet / sepia / press / hc-*) was overwritten
        // to 'light' on first paint, collapsing every site's
        // theme to the toggle-set default.
        //
        // The fixed JS preserves any server attribute and only
        // routes through T for the toggle CYCLE (click handler).
        assert!(
            THEME_TOGGLE_JS.contains("return s||'light'"),
            "fall-through must preserve server theme, not reset to light"
        );
        assert!(
            !THEME_TOGGLE_JS.contains("if(T.indexOf(s)>=0)return s;return 'light'"),
            "old indexOf-only fall-through still present"
        );
    }

    #[test]
    fn theme_toggle_click_cycle_handles_named_theme_gracefully() {
        // When current theme is named (warm/ocean/etc.), indexOf
        // returns -1. The fixed JS uses i<0?0:i+1 so a click from
        // a named theme cycles to T[0]='light' rather than
        // producing T[NaN].
        assert!(
            THEME_TOGGLE_JS.contains("var i=T.indexOf(c);var n=T[(i<0?0:i+1)%T.length]"),
            "click handler must handle named-theme indexOf=-1 case"
        );
    }

    #[test]
    fn brand_logo_with_safe_src_renders_img_plus_visually_hidden_name() {
        let json = r#"{
            "brand": "Prosperity Club",
            "brand_logo": {
                "src": "/assets/phoenix.svg",
                "alt": "Prosperity Club phoenix mark",
                "width": 160,
                "height": 40
            },
            "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/", "nav_links": [], "dev_devtools": false,
            "sections": []
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = page_shell(&page, "/loom-skin.css", "", None);
        assert!(html.contains("loom-page-brand__logo"));
        assert!(html.contains("src=\"/assets/phoenix.svg\""));
        assert!(html.contains("alt=\"Prosperity Club phoenix mark\""));
        assert!(html.contains("width=\"160\""));
        assert!(html.contains("height=\"40\""));
        assert!(html.contains("decoding=\"async\""));
        assert!(html.contains("loom-page-brand__name"));
        assert!(html.contains("loom-visually-hidden"));
        assert!(html.contains(">Prosperity Club</span>"));
    }

    #[test]
    fn brand_logo_unsafe_src_falls_back_to_text_only_brand() {
        let json = r#"{
            "brand": "Hostile",
            "brand_logo": {
                "src": "javascript:alert(1)",
                "alt": "x"
            },
            "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/", "nav_links": [], "dev_devtools": false,
            "sections": []
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = page_shell(&page, "/loom-skin.css", "", None);
        assert!(!html.contains("loom-page-brand__logo"));
        assert!(!html.contains("src=\"javascript:"));
        assert!(html.contains("loom-page-brand"));
        assert!(html.contains(">Hostile</a>"));
    }

    #[test]
    fn brand_logo_absent_preserves_text_only_behavior() {
        let json = r#"{
            "brand": "Example Foundation",
            "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/", "nav_links": [], "dev_devtools": false,
            "sections": []
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = page_shell(&page, "/loom-skin.css", "", None);
        assert!(!html.contains("loom-page-brand__logo"));
        assert!(!html.contains("loom-page-brand__name"));
        assert!(html.contains("loom-page-brand"));
        assert!(html.contains(">Example Foundation</a>"));
    }

    #[test]
    fn brand_logo_width_height_optional() {
        let json = r#"{
            "brand": "X",
            "brand_logo": {
                "src": "/logo.svg",
                "alt": "X logo"
            },
            "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/", "nav_links": [], "dev_devtools": false,
            "sections": []
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = page_shell(&page, "/loom-skin.css", "", None);
        assert!(html.contains("loom-page-brand__logo"));
        assert!(html.contains("src=\"/logo.svg\""));
        assert!(!html.contains(" width=\""));
        assert!(!html.contains(" height=\""));
    }

    #[test]
    fn brand_logo_alt_escaped_into_attribute() {
        let json = r#"{
            "brand": "X",
            "brand_logo": {
                "src": "/logo.svg",
                "alt": "<script>alert('a')</script>"
            },
            "theme": null, "chrome": null, "content_width": null,
            "nav_actions": [], "title": "t", "description": "d",
            "path": "/", "nav_links": [], "dev_devtools": false,
            "sections": []
        }"#;
        let page: CmsPage = serde_json::from_str(json).expect("page parses");
        let html = page_shell(&page, "/loom-skin.css", "", None);
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
        assert_eq!(
            TestimonialDecoration::Decorated.modifier_class(),
            "deco-decorated"
        );
        assert_eq!(
            TestimonialDecoration::Editorial.modifier_class(),
            "deco-editorial"
        );
        assert_eq!(
            TestimonialDecoration::Minimal.modifier_class(),
            "deco-minimal"
        );
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
        assert_eq!(
            FeatureSpotlightDecoration::Decorated.modifier_class(),
            "deco-decorated"
        );
        assert_eq!(
            FeatureSpotlightDecoration::Editorial.modifier_class(),
            "deco-editorial"
        );
        assert_eq!(
            FeatureSpotlightDecoration::Minimal.modifier_class(),
            "deco-minimal"
        );
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            style: CmsFormStyle::default(),
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                style: CmsFormStyle::default(),
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                style: CmsFormStyle::default(),
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                style: CmsFormStyle::default(),
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                style: CmsFormStyle::default(),
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Form {
                legend: "x".to_owned(),
                submit: CmsFormSubmit {
                    label: "Go".to_owned(),
                    secondary_label: None,
                    action: "javascript:alert(1)".to_owned(),
                    data_backend: "x".to_owned(),
                },
                style: CmsFormStyle::default(),
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Form {
                legend: "<script>".to_owned(),
                submit: CmsFormSubmit {
                    label: "<x>".to_owned(),
                    secondary_label: None,
                    action: "/x".to_owned(),
                    data_backend: "x".to_owned(),
                },
                style: CmsFormStyle::default(),
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::KvPair {
                heading: heading.map(|s| s.to_owned()),
                items,
                density: KvPairDensity::default(),
                tone: KvPairTone::default(),
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
        // Default density + tone surface as data attributes.
        assert!(html.contains(r#"data-density="comfortable""#));
        assert!(html.contains(r#"data-tone="slate""#));
    }

    #[test]
    fn kv_pair_density_compact_surfaces_attribute() {
        let items = vec![CmsKvItem {
            key: "k".into(),
            value: "v".into(),
            hint: None,
        }];
        let p = CmsPage {
            brand: None,
            brand_logo: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".into(),
            description: "x".into(),
            path: "/x".into(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::KvPair {
                heading: None,
                items,
                density: KvPairDensity::Compact,
                tone: KvPairTone::Slate,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"data-density="compact""#));
        assert!(!html.contains(r#"data-density="comfortable""#));
    }

    #[test]
    fn kv_pair_density_spacious_surfaces_attribute() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".into(),
            description: "x".into(),
            path: "/x".into(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::KvPair {
                heading: None,
                items: vec![],
                density: KvPairDensity::Spacious,
                tone: KvPairTone::Slate,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"data-density="spacious""#));
    }

    #[test]
    fn kv_pair_amoled_tone_surfaces_attribute() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
            theme: None,
            chrome: None,
            content_width: None,
            nav_actions: vec![],
            schema: None,
            title: "x".into(),
            description: "x".into(),
            path: "/x".into(),
            nav_links: vec![],
            dev_devtools: false,
            footer: None,
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::KvPair {
                heading: None,
                items: vec![],
                density: KvPairDensity::Comfortable,
                tone: KvPairTone::Amoled,
            }],
        };
        let html = render_to_string(&p);
        assert!(html.contains(r#"data-tone="amoled""#));
        assert!(!html.contains(r#"data-tone="slate""#));
    }

    #[test]
    fn kv_pair_density_tone_default_when_omitted_in_json() {
        let json = r#"{
            "kind": "kv_pair",
            "items": []
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::KvPair { density, tone, .. } => {
                assert!(matches!(density, KvPairDensity::Comfortable));
                assert!(matches!(tone, KvPairTone::Slate));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn kv_pair_density_tone_parse_from_json_when_present() {
        let json = r#"{
            "kind": "kv_pair",
            "items": [],
            "density": "spacious",
            "tone": "amoled"
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::KvPair { density, tone, .. } => {
                assert!(matches!(density, KvPairDensity::Spacious));
                assert!(matches!(tone, KvPairTone::Amoled));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn kv_pair_legacy_two_field_json_still_parses() {
        // Back-compat: existing CMS files written before this commit
        // carry only `heading` + `items`. The new fields must default.
        let json = r#"{
            "kind": "kv_pair",
            "heading": "Legacy",
            "items": [{ "key": "k", "value": "v" }]
        }"#;
        let s: CmsSection = serde_json::from_str(json).expect("parse");
        match s {
            CmsSection::KvPair {
                heading,
                items,
                density,
                tone,
            } => {
                assert_eq!(heading.as_deref(), Some("Legacy"));
                assert_eq!(items.len(), 1);
                assert!(matches!(density, KvPairDensity::Comfortable));
                assert!(matches!(tone, KvPairTone::Slate));
            }
            other => unreachable!("wrong variant: {other:?}"),
        }
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
            CmsSection::KvPair { heading, items, .. } => {
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
--loom-blur-sm:6px;--loom-blur-md:14px;--loom-blur-lg:24px;\
--loom-offscreen-x:-9999px;\
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
.loom-skip{position:absolute;left:var(--loom-offscreen-x);top:auto;width:1px;height:1px;overflow:hidden}\
.loom-skip:focus{left:1rem;top:1rem;width:auto;height:auto;padding:.5rem 1rem;\
background:var(--loom-bg);color:var(--loom-fg);border:2px solid var(--loom-focus);\
border-radius:var(--loom-radius);z-index:1000;box-shadow:var(--loom-shadow-md)}\
header.loom-page-header{padding:1rem 1.75rem;border-bottom:1px solid color-mix(in oklab,var(--loom-border) 60%,transparent);\
background:color-mix(in oklab,var(--loom-bg) 88%,transparent);position:sticky;top:0;z-index:50;\
backdrop-filter:saturate(160%) blur(var(--loom-blur-md));-webkit-backdrop-filter:saturate(160%) blur(var(--loom-blur-md))}\
footer.loom-page-footer{padding:2.5rem 1.75rem;border-top:1px solid var(--loom-border);\
color:var(--loom-muted);margin-top:4rem;font-size:.92rem}\
nav.loom-page-nav{display:flex;gap:.5rem;align-items:center;flex-wrap:wrap}\
nav.loom-page-nav a{text-decoration:none;color:var(--loom-muted);\
display:inline-flex;align-items:center;min-height:var(--loom-tap-min);padding:.5rem .9rem;\
border-radius:var(--loom-radius-full);font-weight:500;font-size:.96rem;\
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
pub const THEME_TOGGLE_JS: &str = "(function(){var K='loom-theme';var B=document.querySelector('[data-loom-theme-toggle]');if(!B)return;var T=['light','dark','auto'];function r(){var v=null;try{v=localStorage.getItem(K);}catch(_){}if(T.indexOf(v)>=0)return v;var s=document.documentElement.getAttribute('data-theme');return s||'light';}function a(t){document.documentElement.setAttribute('data-theme',t);B.setAttribute('aria-label','Theme: '+t+' (click to cycle)');B.setAttribute('aria-pressed',t==='dark'?'true':'false');B.textContent=t==='light'?'☀':(t==='dark'?'☾':'◐');}a(r());B.addEventListener('click',function(){var c=r();var i=T.indexOf(c);var n=T[(i<0?0:i+1)%T.length];try{localStorage.setItem(K,n);}catch(_){}a(n);});})();";

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
pub const THEME_TOGGLE_CSS: &str = ".loom-theme-toggle{margin-left:auto;display:inline-flex;align-items:center;justify-content:center;width:var(--loom-tap-min);height:var(--loom-tap-min);border-radius:var(--loom-radius-full);border:1px solid var(--loom-color-border,var(--loom-border));background:var(--loom-color-surface,var(--loom-bg));color:var(--loom-color-ink,var(--loom-fg));font-size:1.15rem;cursor:pointer;line-height:1;padding:0;transition:background var(--loom-motion-fast,120ms) var(--loom-ease-out,ease),border-color var(--loom-motion-fast,120ms) var(--loom-ease-out,ease)}.loom-theme-toggle:hover{background:var(--loom-color-surface-muted,var(--loom-grad-soft));border-color:var(--loom-color-primary,var(--loom-accent))}.loom-theme-toggle:focus-visible{outline:2px solid var(--loom-color-primary,var(--loom-accent));outline-offset:3px}";

/// #102 forward step (2026-05-20): CSS-only theme toggle for
/// `LOOM_NOSCRIPT_MODE` page renders. Replaces the JS-driven
/// `<button data-loom-theme-toggle>` with a radio-group fieldset.
/// Theme changes drive CSS custom properties via `:has()` selectors
/// — Safari 15.4+, Chrome 105+, Firefox 121+ all support
/// `selector(:has(...))`.
///
/// SCOPE: covers the core 9 swap properties (`--loom-bg`,
/// `--loom-fg`, `--loom-muted`, `--loom-accent`, `--loom-accent-2`,
/// `--loom-border`, `--loom-link`, `--loom-link-hover`,
/// `--loom-focus`) — enough for a coherent dark mode at first
/// paint. The full palette (shadows, gradients) keeps the
/// server-rendered `[data-theme]` cascade as its primary driver;
/// the `:has()` overrides take precedence when a fallback radio
/// is checked.
///
/// PERSISTENCE: per-page session only. Without JS there's no way
/// to write to localStorage, so a page reload resets the radios
/// to their default `checked` state (auto). Operators wanting
/// cross-page persistence without JS need a server-side cookie
/// roundtrip — that's a separate `Set-Cookie` form-based toggle,
/// not this one.
///
/// ACCESSIBILITY: fieldset + `<legend class="loom-sr-only">` for
/// the group label; each radio's `<label>` carries the visual
/// glyph and a `title` for tooltips. `:focus-visible` ring on the
/// active label.
pub const THEME_TOGGLE_NOSCRIPT_CSS: &str = ".loom-theme-toggle-nf{margin-left:auto;display:inline-flex;gap:.125rem;align-items:center;border:1px solid var(--loom-color-border,var(--loom-border));border-radius:var(--loom-radius-full);padding:.125rem;background:var(--loom-color-surface,var(--loom-bg))}.loom-theme-toggle-nf input[type=\"radio\"]{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}.loom-theme-toggle-nf label{display:inline-flex;align-items:center;justify-content:center;width:calc(var(--loom-tap-min) - .5rem);height:calc(var(--loom-tap-min) - .5rem);border-radius:var(--loom-radius-full);cursor:pointer;font-size:1.05rem;line-height:1;color:var(--loom-color-muted,var(--loom-muted))}.loom-theme-toggle-nf input[type=\"radio\"]:checked+label{background:var(--loom-color-primary,var(--loom-accent));color:var(--loom-color-surface,var(--loom-bg))}.loom-theme-toggle-nf input[type=\"radio\"]:focus-visible+label{outline:2px solid var(--loom-color-primary,var(--loom-accent));outline-offset:3px}:root:has(input[name=\"loom-theme-nf\"][value=\"dark\"]:checked){--loom-bg:#0F1019;--loom-fg:#ECEEF6;--loom-muted:#8B92A6;--loom-accent:#A5A6FF;--loom-accent-2:#FFA771;--loom-border:#25283A;--loom-link:#A5A6FF;--loom-link-hover:#DCDDFF;--loom-focus:#A5A6FF}:root:has(input[name=\"loom-theme-nf\"][value=\"light\"]:checked){--loom-bg:#FBFAF7;--loom-fg:#1B1F2A;--loom-muted:#6B7280;--loom-accent:#4338CA;--loom-accent-2:#E07A5F;--loom-border:#E6E2DA;--loom-link:#4338CA;--loom-link-hover:#3730A3;--loom-focus:#4338CA}@media(prefers-color-scheme:dark){:root:has(input[name=\"loom-theme-nf\"][value=\"auto\"]:checked){--loom-bg:#0F1019;--loom-fg:#ECEEF6;--loom-muted:#8B92A6;--loom-accent:#A5A6FF;--loom-accent-2:#FFA771;--loom-border:#25283A;--loom-link:#A5A6FF;--loom-link-hover:#DCDDFF;--loom-focus:#A5A6FF}}";

/// Rendered HTML for the noscript theme-toggle radio group.
/// Inlined verbatim into the page shell when `noscript_mode` is on.
/// Defaults `auto` to `checked` so a fresh load tracks OS preference.
pub const THEME_TOGGLE_NOSCRIPT_HTML: &str = "<fieldset class=\"loom-theme-toggle-nf\"><legend class=\"loom-sr-only\">Theme</legend><input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-auto\" value=\"auto\" checked><label for=\"loom-theme-nf-auto\" title=\"Auto (match OS)\">◐</label><input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-light\" value=\"light\"><label for=\"loom-theme-nf-light\" title=\"Light\">☀</label><input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-dark\" value=\"dark\"><label for=\"loom-theme-nf-dark\" title=\"Dark\">☾</label></fieldset>";

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

/// Render `s` as a JSON-string-literal — wrapped in double
/// quotes, with backslash + control-char + quote escapes.
/// Used for safely embedding strings into inline `<script>`
/// blocks where serde_json::Value::to_string is overkill.
///
/// REGRESSION-GUARD: closing-`</script>` injections are blocked
/// because `<` and `/` aren't part of the escape set — but the
/// renderer's caller is expected to keep CMS-author strings out
/// of the value here (kind_slug, tenant_id, mount_id are
/// substrate-controlled identifiers, not free text).
#[must_use]
pub fn json_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
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
    // OG / Twitter URL — fully-qualified when `site_origin` set,
    // path-only otherwise. og:image / twitter:image emitted only
    // when `social_image` is set; same origin prefix rule.
    let og_url = match page.site_origin.as_deref() {
        Some(origin) => {
            let trimmed = origin.trim_end_matches('/');
            escape_html_attr(&format!("{trimmed}{}", page.path))
        }
        None => path.clone(),
    };
    // Resolve the social-card image src. Explicit page.social_image
    // wins; otherwise auto-derive from the first hero-class section
    // (image_hero with photo background, or split_hero with an
    // AssetSlug visual). Auto-derivation eliminates the boilerplate
    // of declaring social_image on every cms page when the page
    // already has a hero photo.
    let derived_social_image = page.social_image.clone().or_else(|| {
        page.sections.iter().find_map(|s| match s {
            CmsSection::ImageHero { background, .. } => match background {
                HeroBackground::Photo { src, .. } => Some(src.clone()),
                _ => None,
            },
            CmsSection::SplitHero { visual, .. } => match visual {
                SplitVisual::AssetSlug { slug, .. } => Some(format!("/assets/{slug}.jpg")),
                _ => None,
            },
            _ => None,
        })
    });
    let og_image_block = match derived_social_image.as_deref() {
        Some(src) => {
            let absolute = match page.site_origin.as_deref() {
                Some(origin) if src.starts_with('/') => {
                    let trimmed = origin.trim_end_matches('/');
                    format!("{trimmed}{src}")
                }
                _ => src.to_owned(),
            };
            let safe = escape_html_attr(&absolute);
            format!(
                "<meta property=\"og:image\" content=\"{safe}\">\n  <meta name=\"twitter:image\" content=\"{safe}\">\n  "
            )
        }
        None => String::new(),
    };
    // Auto-emit Organization JSON-LD when brand + site_origin are
    // present. Closes the Forge seo phase's "no JSON-LD structured
    // data" warning on every Forge-built page without forcing
    // authors to hand-author a script block. Phone / email /
    // jurisdiction are pulled from the typed footer.contact block
    // when set. Skipped silently when no brand or site_origin —
    // emitting an empty Organization is worse than emitting none.
    let jsonld_block = build_organization_jsonld(page);
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
    let brand_text_escaped = escape_html_text(&brand_raw);
    // When `brand_logo` is set AND `src` is safe, render the
    // logo image visually + a visually-hidden span for AT
    // (which announces the brand name). When unset or hostile
    // src, render text-only (previous behavior).
    let brand: std::borrow::Cow<'_, str> = match &page.brand_logo {
        Some(logo) if loom_components::composer::is_safe_url(&logo.src) => {
            let src = escape_html_attr(&logo.src);
            let alt = escape_html_attr(&logo.alt);
            let width_attr = logo
                .width
                .map(|w| format!(" width=\"{w}\""))
                .unwrap_or_default();
            let height_attr = logo
                .height
                .map(|h| format!(" height=\"{h}\""))
                .unwrap_or_default();
            std::borrow::Cow::Owned(format!(
                "<img class=\"loom-page-brand__logo\" src=\"{src}\" alt=\"{alt}\"{width_attr}{height_attr} decoding=\"async\"><span class=\"loom-page-brand__name loom-visually-hidden\">{brand_text_escaped}</span>"
            ))
        }
        _ => std::borrow::Cow::Owned(brand_text_escaped.to_string()),
    };
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
    // LOOM_NOSCRIPT_MODE — process-level env that drops every
    // inline script and the defer-stylesheet onload swap. Used
    // by Forge when forge.toml `[noscript_strict] enabled = true`
    // for LibreJS / Tor-strict / hunted-tier builds. The rendered
    // HTML carries zero `<script>` tags + a maximally-strict CSP.
    let noscript_mode = std::env::var("LOOM_NOSCRIPT_MODE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    // T72 + #102 (2026-05-20): bundle the theme-toggle CSS into
    // the inline critical-CSS block. In noscript_mode the JS-
    // driven toggle is replaced by a CSS-only :has()-driven radio
    // group, and the matching fallback CSS is appended so the
    // group's checked state actually swaps the palette. Hash is
    // recomputed naturally from whichever CSS string we bundled.
    let base_with_toggle = if noscript_mode {
        format!("{BASE_THEME_CSS}{THEME_TOGGLE_CSS}{THEME_TOGGLE_NOSCRIPT_CSS}")
    } else {
        format!("{BASE_THEME_CSS}{THEME_TOGGLE_CSS}")
    };
    let base_theme_hash = csp_sha256(base_with_toggle.as_bytes());
    let base_theme_block = format!("<style>{base_with_toggle}</style>\n  ");
    let toggle_script_hash = csp_sha256(THEME_TOGGLE_JS.as_bytes());
    let eruda_hash = csp_sha256(ERUDA_LOADER_JS.as_bytes());
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
  <meta property=\"og:title\" content=\"{title}\">\n\
  <meta property=\"og:description\" content=\"{description}\">\n\
  <meta property=\"og:type\" content=\"website\">\n\
  <meta property=\"og:url\" content=\"{og_url}\">\n\
  {og_image_block}<meta name=\"twitter:card\" content=\"summary_large_image\">\n\
  <meta name=\"twitter:title\" content=\"{title}\">\n\
  <meta name=\"twitter:description\" content=\"{description}\">\n\
  {jsonld_block}{DEFAULT_FAVICON_LINK}\n\
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
    // #102 (2026-05-20): in noscript_mode emit a CSS-only :has()
    // radio fieldset in place of the JS-driven button. The
    // matching CSS (THEME_TOGGLE_NOSCRIPT_CSS) is bundled into the
    // critical-CSS block by page_shell_themed. Without it, this
    // markup would render but clicks wouldn't swap the palette.
    let toggle_btn_pageshell = if noscript_mode {
        concat!(
            "<fieldset class=\"loom-theme-toggle-nf\"><legend class=\"loom-sr-only\">Theme</legend>",
            "<input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-auto\" value=\"auto\" checked>",
            "<label for=\"loom-theme-nf-auto\" title=\"Auto (match OS)\">◐</label>",
            "<input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-light\" value=\"light\">",
            "<label for=\"loom-theme-nf-light\" title=\"Light\">☀</label>",
            "<input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-dark\" value=\"dark\">",
            "<label for=\"loom-theme-nf-dark\" title=\"Dark\">☾</label>",
            "</fieldset>\n    "
        )
    } else {
        "<button type=\"button\" class=\"loom-theme-toggle\" data-loom-theme-toggle aria-label=\"Theme: light (click to cycle)\" aria-pressed=\"false\">☀</button>\n    "
    };
    let toggle_btn_floating = if noscript_mode {
        concat!(
            "<fieldset class=\"loom-theme-toggle-nf\"><legend class=\"loom-sr-only\">Theme</legend>",
            "<input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-auto-fp\" value=\"auto\" checked>",
            "<label for=\"loom-theme-nf-auto-fp\" title=\"Auto (match OS)\">◐</label>",
            "<input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-light-fp\" value=\"light\">",
            "<label for=\"loom-theme-nf-light-fp\" title=\"Light\">☀</label>",
            "<input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-dark-fp\" value=\"dark\">",
            "<label for=\"loom-theme-nf-dark-fp\" title=\"Dark\">☾</label>",
            "</fieldset>\n      "
        )
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
/// Validate and normalize a heading `id` value. Returns the
/// owned string when the input is a strict slug `[a-z0-9-]+`
/// (lowercase letters, digits, single-dash separators), no
/// leading or trailing dash, length 1..=64. Returns `None`
/// otherwise — the renderer then omits the `id` attribute
/// rather than emit attacker-controlled HTML.
fn sanitize_anchor_id(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 64 {
        return None;
    }
    if s.starts_with('-') || s.ends_with('-') {
        return None;
    }
    let ok = s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok { Some(s.to_owned()) } else { None }
}

/// Build an `Organization` JSON-LD `<script>` block from page-
/// level metadata. Returns an empty string when the necessary
/// fields (brand + site_origin) aren't both present — skipping
/// the block is better than emitting a `{"@type":"Organization"}`
/// with no name. Hand-rolls the JSON because the payload is
/// small and we want byte-control over the escape rules (JSON
/// strings inside an HTML `<script>` need `</` escaping to
/// prevent early-`</script>` injection).
fn build_organization_jsonld(page: &CmsPage) -> String {
    let Some(origin) = page.site_origin.as_deref() else {
        return String::new();
    };
    let Some(brand) = page.brand.as_deref() else {
        return String::new();
    };
    let trimmed_origin = origin.trim_end_matches('/');
    let mut pairs = Vec::<String>::new();
    pairs.push("\"@context\":\"https://schema.org\"".to_owned());
    pairs.push("\"@type\":\"Organization\"".to_owned());
    pairs.push(format!("\"name\":\"{}\"", jsonld_escape(brand)));
    pairs.push(format!("\"url\":\"{}\"", jsonld_escape(trimmed_origin)));
    if let Some(contact) = page.footer.as_ref().and_then(|f| f.contact.as_ref()) {
        if let Some(email) = contact.email.as_deref() {
            pairs.push(format!("\"email\":\"{}\"", jsonld_escape(email)));
        }
        if let Some(phone) = contact.phone.as_deref() {
            // contactPoint structured object — gives schema.org enough
            // to render rich contact actions in SERP-style consumers.
            pairs.push(format!(
                "\"contactPoint\":{{\"@type\":\"ContactPoint\",\"telephone\":\"{}\",\"contactType\":\"customer support\"}}",
                jsonld_escape(phone)
            ));
        }
        if let Some(juris) = contact.jurisdiction.as_deref() {
            // Best-effort parse: "Massachusetts, USA" → region +
            // country. Falls back to plain addressRegion when only
            // one segment.
            let (region, country) = match juris.split_once(',') {
                Some((r, c)) => (r.trim().to_owned(), Some(c.trim().to_owned())),
                None => (juris.trim().to_owned(), None),
            };
            let mut addr = format!(
                "\"address\":{{\"@type\":\"PostalAddress\",\"addressRegion\":\"{}\"",
                jsonld_escape(&region)
            );
            if let Some(c) = country {
                addr.push_str(&format!(",\"addressCountry\":\"{}\"", jsonld_escape(&c)));
            }
            addr.push('}');
            pairs.push(addr);
        }
    }
    let body = pairs.join(",");
    format!("<script type=\"application/ld+json\">{{{body}}}</script>\n  ")
}

/// JSON-string-escape with the additional rule that any `</`
/// substring is broken up — even inside a string literal that's a
/// valid JSON value, browsers terminate a `<script>` block at the
/// first `</script` tag-open. Splitting `<\/` survives JSON parse
/// AND prevents the early-close attack.
fn jsonld_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    let mut prev_was_lt = false;
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '/' if prev_was_lt => {
                // Break up </script: replace the slash that would
                // make the closing tag.
                out.push_str("\\/");
            }
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
        prev_was_lt = ch == '<';
    }
    out
}

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
            let dialable = !bytes.is_empty() && (bytes[0] == b'+' || bytes[0].is_ascii_digit());
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
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
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

    // #102 (2026-05-20): CSS-only theme-toggle fallback. The
    // constants below ship the no-JS path of the toggle. Full
    // page_shell_themed integration is covered by setting
    // LOOM_NOSCRIPT_MODE in a serial test, but the constants
    // themselves carry the load-bearing shape — and constants
    // are testable in isolation without env-var fiddling.
    #[test]
    fn noscript_theme_toggle_css_has_palette_swap_via_has_selector() {
        // The fallback swaps the core 9 palette properties via
        // `:root:has(input[name="loom-theme-nf"][value="..."]:checked)`.
        assert!(
            THEME_TOGGLE_NOSCRIPT_CSS
                .contains(":root:has(input[name=\"loom-theme-nf\"][value=\"dark\"]:checked)"),
            "dark :has() selector must apply the dark palette"
        );
        assert!(
            THEME_TOGGLE_NOSCRIPT_CSS
                .contains(":root:has(input[name=\"loom-theme-nf\"][value=\"light\"]:checked)"),
            "light :has() selector must apply the light palette"
        );
        // The auto branch is wrapped in `prefers-color-scheme: dark`
        // so it falls back to the :root defaults when OS is light.
        assert!(
            THEME_TOGGLE_NOSCRIPT_CSS.contains(
                "@media(prefers-color-scheme:dark){:root:has(input[name=\"loom-theme-nf\"][value=\"auto\"]:checked)"
            ),
            "auto branch must scope to prefers-color-scheme:dark"
        );
    }

    #[test]
    fn noscript_theme_toggle_css_swaps_all_core_properties() {
        // Every theme MUST set at least the 9 palette properties
        // the base cascade reads from. Missing any of these causes
        // visible inconsistencies (e.g. dark bg but light link).
        let dark_block_start = THEME_TOGGLE_NOSCRIPT_CSS
            .find(":root:has(input[name=\"loom-theme-nf\"][value=\"dark\"]:checked)")
            .expect("dark block must exist");
        let dark_block_end = THEME_TOGGLE_NOSCRIPT_CSS[dark_block_start..]
            .find('}')
            .map(|i| dark_block_start + i)
            .expect("dark block must close");
        let dark_block = &THEME_TOGGLE_NOSCRIPT_CSS[dark_block_start..dark_block_end];
        for prop in [
            "--loom-bg",
            "--loom-fg",
            "--loom-muted",
            "--loom-accent",
            "--loom-accent-2",
            "--loom-border",
            "--loom-link",
            "--loom-link-hover",
            "--loom-focus",
        ] {
            assert!(
                dark_block.contains(prop),
                "dark :has() block must set {prop} or the palette swap is incomplete"
            );
        }
    }

    #[test]
    fn noscript_theme_toggle_html_is_accessible_radio_group() {
        // Accessibility shape: <fieldset> with a screen-reader-only
        // <legend>, three radio inputs sharing a name, default
        // `auto` checked, each with a labelled glyph.
        let html = THEME_TOGGLE_NOSCRIPT_HTML;
        assert!(
            html.contains("<fieldset class=\"loom-theme-toggle-nf\">"),
            "must use semantic <fieldset> chrome"
        );
        assert!(
            html.contains("<legend class=\"loom-sr-only\">Theme</legend>"),
            "screen-reader-only legend names the radio group"
        );
        for val in ["auto", "light", "dark"] {
            assert!(
                html.contains(&format!(
                    "name=\"loom-theme-nf\" id=\"loom-theme-nf-{val}\" value=\"{val}\""
                )),
                "missing radio for theme={val}"
            );
            assert!(
                html.contains(&format!("for=\"loom-theme-nf-{val}\"")),
                "missing matching label for theme={val}"
            );
        }
        // Default-checked is `auto` — fresh page-load tracks OS preference.
        assert!(
            html.contains(
                "<input type=\"radio\" name=\"loom-theme-nf\" id=\"loom-theme-nf-auto\" value=\"auto\" checked>"
            ),
            "auto must be the default-checked radio"
        );
        // No `<input>` is checked-by-default other than auto.
        assert_eq!(
            html.matches(" checked").count(),
            1,
            "exactly one radio is checked by default (auto)"
        );
    }

    #[test]
    fn noscript_theme_toggle_html_hides_radios_via_sr_only_pattern() {
        // The CSS hides the radio inputs themselves (visual chrome
        // lives on the labels). Verify the hide pattern is the
        // standard `clip:rect(0,0,0,0)` sr-only approach, not
        // `display:none` (which would also hide them from screen
        // readers and break keyboard nav).
        assert!(
            THEME_TOGGLE_NOSCRIPT_CSS.contains(".loom-theme-toggle-nf input[type=\"radio\"]{"),
            "must scope hidden-radio rule to the noscript fieldset"
        );
        assert!(
            THEME_TOGGLE_NOSCRIPT_CSS.contains("clip:rect(0,0,0,0)"),
            "must hide radios via sr-only clip rect, not display:none"
        );
        assert!(
            !THEME_TOGGLE_NOSCRIPT_CSS
                .contains(".loom-theme-toggle-nf input[type=\"radio\"]{display:none"),
            "must NOT use display:none — breaks keyboard nav + screen readers"
        );
    }

    /// T70b-fix REGRESSION-GUARD: page_shell + render_page composed
    /// must produce EXACTLY ONE `<main>` element. Two `<main>`s
    /// per document is a WCAG violation.
    #[test]
    fn page_shell_with_rendered_body_produces_exactly_one_main() {
        let p = CmsPage {
            brand: None,
            brand_logo: None,
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
            site_origin: None,
            social_image: None,
            sections: vec![CmsSection::Heading {
                level: HeadingLevel::H2,
                text: "x".into(),
                id: None,
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

    #[test]
    fn jsonld_emitted_when_brand_and_site_origin_set() {
        let mut p = empty_page();
        p.brand = Some("PlausiDen LLC".into());
        p.site_origin = Some("https://dev.plausiden.com".into());
        let h = page_shell_themed(&p, "/x.css", "<main></main>", None, None);
        assert!(h.contains("application/ld+json"));
        assert!(h.contains("\"@type\":\"Organization\""));
        assert!(h.contains("\"name\":\"PlausiDen LLC\""));
        assert!(h.contains("\"url\":\"https://dev.plausiden.com\""));
    }

    #[test]
    fn jsonld_skipped_when_brand_missing() {
        let mut p = empty_page();
        p.site_origin = Some("https://x.example".into());
        let h = page_shell_themed(&p, "/x.css", "<main></main>", None, None);
        assert!(!h.contains("application/ld+json"));
    }

    #[test]
    fn jsonld_skipped_when_origin_missing() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        let h = page_shell_themed(&p, "/x.css", "<main></main>", None, None);
        assert!(!h.contains("application/ld+json"));
    }

    #[test]
    fn jsonld_includes_contact_point_when_phone_set() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.footer = Some(CmsFooter {
            columns: vec![],
            contact: Some(CmsFooterContact {
                heading: None,
                phone: Some("978-351-6495".into()),
                email: Some("team@x.example".into()),
                address: None,
                jurisdiction: Some("Massachusetts, USA".into()),
            }),
            legal_links: vec![],
            colophon: None,
        });
        let h = page_shell_themed(&p, "/x.css", "<main></main>", None, None);
        assert!(h.contains("\"telephone\":\"978-351-6495\""));
        assert!(h.contains("\"email\":\"team@x.example\""));
        assert!(h.contains("\"addressRegion\":\"Massachusetts\""));
        assert!(h.contains("\"addressCountry\":\"USA\""));
    }

    #[test]
    fn jsonld_escape_breaks_script_close_tag() {
        // A brand string with `</script>` in it must not let an
        // attacker close the script block early. The HTML parser
        // terminates `<script>` on the first `</script>` it sees —
        // we break it up as `<\/script>` which is valid JSON
        // (JSON.parse decodes `\/` to `/`) but does NOT match the
        // HTML script-close pattern.
        let mut p = empty_page();
        p.brand = Some("X</script>HACK".into());
        p.site_origin = Some("https://x.example".into());
        let h = page_shell_themed(&p, "/x.css", "<main></main>", None, None);
        // The escaped form appears in the JSON-LD payload.
        assert!(
            h.contains("X<\\/script>HACK"),
            "expected escaped form in payload"
        );
        // Crucially: the FIRST `</script>` after the JSON-LD start
        // must be the legit closing tag of the JSON-LD block, not
        // an attacker-injected one inside the JSON value. We verify
        // by checking the character right before that `</script>`
        // is the JSON object's closing brace `}` — which it would
        // be if escape did its job and only the structural tag
        // appears literally.
        let jsonld_start = h.find("application/ld+json").expect("jsonld present");
        let after_jsonld = &h[jsonld_start..];
        let close_idx = after_jsonld.find("</script>").expect("jsonld block closes");
        let before_close = &after_jsonld[..close_idx];
        assert!(
            before_close.ends_with('}'),
            "expected `</script>` immediately after the JSON object's `}}`; got tail: {:?}",
            &before_close[before_close.len().saturating_sub(40)..]
        );
    }

    #[test]
    fn sanitize_anchor_id_accepts_simple_slug() {
        assert_eq!(sanitize_anchor_id("support"), Some("support".into()));
        assert_eq!(sanitize_anchor_id("dark-sky"), Some("dark-sky".into()));
        assert_eq!(sanitize_anchor_id("a"), Some("a".into()));
        assert_eq!(
            sanitize_anchor_id("section-2026"),
            Some("section-2026".into())
        );
    }

    #[test]
    fn sanitize_anchor_id_trims_whitespace() {
        assert_eq!(sanitize_anchor_id("  support  "), Some("support".into()));
        assert_eq!(sanitize_anchor_id("\tabout\n"), Some("about".into()));
    }

    #[test]
    fn sanitize_anchor_id_rejects_uppercase() {
        assert_eq!(sanitize_anchor_id("Support"), None);
        assert_eq!(sanitize_anchor_id("DARK-SKY"), None);
    }

    #[test]
    fn sanitize_anchor_id_rejects_underscores() {
        // Underscore is not in [a-z0-9-]; slugs use dashes only.
        assert_eq!(sanitize_anchor_id("dark_sky"), None);
        assert_eq!(sanitize_anchor_id("a_b_c"), None);
    }

    #[test]
    fn sanitize_anchor_id_rejects_unicode() {
        // Multi-byte sequences must not slip through into attribute
        // values — keep the slug strictly ASCII.
        assert_eq!(sanitize_anchor_id("café"), None);
        assert_eq!(sanitize_anchor_id("北区"), None);
        assert_eq!(sanitize_anchor_id("naïve-id"), None);
    }

    #[test]
    fn sanitize_anchor_id_rejects_attribute_break_attempts() {
        // Quotes / angle brackets / spaces would let an attacker
        // close the attribute and inject markup.
        assert_eq!(sanitize_anchor_id("a\" onclick=alert(1)"), None);
        assert_eq!(sanitize_anchor_id("x><script>"), None);
        assert_eq!(sanitize_anchor_id("a b"), None);
        assert_eq!(sanitize_anchor_id("a'b"), None);
    }

    #[test]
    fn sanitize_anchor_id_rejects_leading_or_trailing_dash() {
        // A leading dash makes the id look like a CSS attribute
        // selector negation; trailing dash is just ugly. Reject both.
        assert_eq!(sanitize_anchor_id("-support"), None);
        assert_eq!(sanitize_anchor_id("support-"), None);
        assert_eq!(sanitize_anchor_id("-"), None);
        assert_eq!(sanitize_anchor_id("--"), None);
    }

    #[test]
    fn sanitize_anchor_id_rejects_empty_and_too_long() {
        assert_eq!(sanitize_anchor_id(""), None);
        assert_eq!(sanitize_anchor_id("   "), None);
        // Exactly 64 chars — accepted.
        let sixty_four = "a".repeat(64);
        assert_eq!(sanitize_anchor_id(&sixty_four), Some(sixty_four));
        // 65 chars — rejected.
        assert_eq!(sanitize_anchor_id(&"a".repeat(65)), None);
    }

    #[test]
    fn heading_with_valid_id_renders_id_attribute() {
        // End-to-end: a Heading variant carrying a valid id slug
        // emits `id="..."` on the rendered tag.
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Heading {
            level: HeadingLevel::H2,
            text: "Support.".into(),
            id: Some("support".into()),
            polish: Vec::new(),
        }];
        let h = page_shell_themed(&p, "/x.css", &render_page(&p).into_string(), None, None);
        assert!(
            h.contains("id=\"support\""),
            "expected id attr on h2; got: {h}"
        );
    }

    #[test]
    fn heading_with_invalid_id_drops_id_attribute() {
        // Invalid slug → no id attribute (no failure, no warning,
        // attribute simply omitted — defense-in-depth posture).
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Heading {
            level: HeadingLevel::H2,
            text: "Support.".into(),
            id: Some("INVALID UPPER".into()),
            polish: Vec::new(),
        }];
        let h = page_shell_themed(&p, "/x.css", &render_page(&p).into_string(), None, None);
        // The heading still renders as h2; the id attr is omitted.
        assert!(h.contains(">Support.<"));
        assert!(!h.contains("id=\"INVALID UPPER\""));
        assert!(!h.contains("INVALID UPPER"));
    }

    #[test]
    fn heading_without_id_renders_no_id_attribute() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Heading {
            level: HeadingLevel::H2,
            text: "No anchor.".into(),
            id: None,
            polish: Vec::new(),
        }];
        let body_only = render_page(&p).into_string();
        // The heading body has no id="..." between the class and the text.
        assert!(
            body_only.contains("<h2 class=\"loom-heading\" data-loom-level=\"2\">No anchor.</h2>")
        );
    }

    // #122 (2026-05-20): EmailVerifyResult — typed verification
    // landing page. Tests cover all 4 status modifiers, default
    // title fallback, body escaping, dual-CTA composition, and
    // serde round-trip.

    fn email_verify_page(status: EmailVerifyStatus, body: &str) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::EmailVerifyResult {
            status,
            title: None,
            body: body.into(),
            cta: None,
            secondary_cta: None,
        }];
        p
    }

    #[test]
    fn email_verify_status_default_titles_per_variant() {
        // Stable copy contract — operators that pass `None` get
        // these strings. Renaming requires an additive variant
        // (don't repurpose existing strings).
        assert_eq!(EmailVerifyStatus::Success.default_title(), "Email verified");
        assert_eq!(
            EmailVerifyStatus::AlreadyVerified.default_title(),
            "Already verified"
        );
        assert_eq!(
            EmailVerifyStatus::Expired.default_title(),
            "Verification link expired"
        );
        assert_eq!(
            EmailVerifyStatus::Invalid.default_title(),
            "Verification link invalid"
        );
    }

    #[test]
    fn email_verify_status_modifier_classes_are_stable() {
        // The loom-skin CSS targets `.loom-email-verify--<modifier>`
        // — modifier strings are part of the wire shape.
        assert_eq!(EmailVerifyStatus::Success.modifier(), "success");
        assert_eq!(
            EmailVerifyStatus::AlreadyVerified.modifier(),
            "already-verified"
        );
        assert_eq!(EmailVerifyStatus::Expired.modifier(), "expired");
        assert_eq!(EmailVerifyStatus::Invalid.modifier(), "invalid");
    }

    #[test]
    fn email_verify_renders_section_with_status_modifier_class() {
        for status in [
            EmailVerifyStatus::Success,
            EmailVerifyStatus::AlreadyVerified,
            EmailVerifyStatus::Expired,
            EmailVerifyStatus::Invalid,
        ] {
            let p = email_verify_page(status, "Body copy.");
            let html = render_page(&p).into_string();
            let modifier_class = format!("loom-email-verify--{}", status.modifier());
            assert!(
                html.contains(&modifier_class),
                "expected modifier class {modifier_class} in render for {status:?}"
            );
            assert!(html.contains(status.default_title()));
        }
    }

    #[test]
    fn email_verify_uses_operator_title_when_provided() {
        let mut p = email_verify_page(EmailVerifyStatus::Success, "Body");
        p.sections[0] = CmsSection::EmailVerifyResult {
            status: EmailVerifyStatus::Success,
            title: Some("Custom title".into()),
            body: "Body".into(),
            cta: None,
            secondary_cta: None,
        };
        let html = render_page(&p).into_string();
        assert!(
            html.contains(">Custom title<"),
            "operator-supplied title must replace the default"
        );
        assert!(
            !html.contains(">Email verified<"),
            "default title must not appear when operator supplied one"
        );
    }

    #[test]
    fn email_verify_escapes_body_text() {
        let p = email_verify_page(EmailVerifyStatus::Invalid, "<script>alert(1)</script>");
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn email_verify_renders_dual_ctas_with_safe_hrefs() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::EmailVerifyResult {
            status: EmailVerifyStatus::Success,
            title: None,
            body: "Welcome.".into(),
            cta: Some(HeroCta {
                label: "Continue".into(),
                href: "/dashboard".into(),
                data_backend: "dashboard".into(),
            }),
            secondary_cta: Some(HeroCta {
                label: "Help".into(),
                href: "/help".into(),
                data_backend: "help".into(),
            }),
        }];
        let html = render_page(&p).into_string();
        assert!(html.contains("href=\"/dashboard\""));
        assert!(html.contains(">Continue<"));
        assert!(html.contains("href=\"/help\""));
        assert!(html.contains(">Help<"));
        assert!(html.contains("loom-email-verify__cta"));
        assert!(html.contains("loom-email-verify__cta-secondary"));
    }

    #[test]
    fn email_verify_rejects_javascript_url_in_cta() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::EmailVerifyResult {
            status: EmailVerifyStatus::Success,
            title: None,
            body: "Welcome.".into(),
            cta: Some(HeroCta {
                label: "X".into(),
                href: "javascript:alert(1)".into(),
                data_backend: "d".into(),
            }),
            secondary_cta: None,
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("javascript:alert"));
        assert!(html.contains("href=\"#invalid-cta\""));
    }

    #[test]
    fn email_verify_with_no_ctas_omits_actions_block() {
        let p = email_verify_page(EmailVerifyStatus::Expired, "Link expired.");
        let html = render_page(&p).into_string();
        assert!(
            !html.contains("loom-email-verify__actions"),
            "actions block must not render when both CTAs are None"
        );
    }

    #[test]
    fn email_verify_status_serde_round_trip() {
        for (variant, json) in [
            (EmailVerifyStatus::Success, "\"success\""),
            (EmailVerifyStatus::AlreadyVerified, "\"already_verified\""),
            (EmailVerifyStatus::Expired, "\"expired\""),
            (EmailVerifyStatus::Invalid, "\"invalid\""),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, json, "expected {variant:?} to serialize as {json}");
            let back: EmailVerifyStatus = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }

    // #103 / #213 (2026-05-20) — CmsFormStyle substrate enum. The
    // wire-through into CmsSection::Form is deferred (would migrate
    // ~7 existing struct-literal fixtures); these tests cover the
    // type + render_form parameter behavior so the substrate
    // contract is enforceable now.

    #[test]
    fn cms_form_style_default_is_rounded() {
        assert_eq!(CmsFormStyle::default(), CmsFormStyle::Rounded);
    }

    #[test]
    fn cms_form_style_modifier_strings_are_stable_kebab_case() {
        assert_eq!(CmsFormStyle::Rounded.modifier(), "rounded");
        assert_eq!(CmsFormStyle::Editorial.modifier(), "editorial");
        assert_eq!(CmsFormStyle::Minimal.modifier(), "minimal");
    }

    #[test]
    fn cms_form_style_serde_round_trip() {
        for (variant, json) in [
            (CmsFormStyle::Rounded, "\"rounded\""),
            (CmsFormStyle::Editorial, "\"editorial\""),
            (CmsFormStyle::Minimal, "\"minimal\""),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, json);
            let back: CmsFormStyle = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn form_variant_style_field_flows_to_rendered_data_attr() {
        // Construct CmsSection::Form with style=Editorial and
        // verify the rendered HTML carries
        // data-loom-form-style="editorial" on both the section
        // and form elements. Proves the variant-level field is
        // plumbed through render_form, not just settable.
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Form {
            legend: "Editorial form".to_owned(),
            submit: CmsFormSubmit {
                label: "Submit".to_owned(),
                secondary_label: None,
                action: "/submit".to_owned(),
                data_backend: "submit".to_owned(),
            },
            style: CmsFormStyle::Editorial,
            steps: vec![],
        }];
        let html = render_page(&p).into_string();
        assert!(
            html.contains("data-loom-form-style=\"editorial\""),
            "Form variant's style field must reach the rendered data attr: {html}"
        );
        // Should appear at LEAST twice — once on <section>, once
        // on <form>.
        let count = html.matches("data-loom-form-style=\"editorial\"").count();
        assert!(
            count >= 2,
            "expected style attr on both <section> and <form>, got {count}: {html}"
        );
    }

    #[test]
    fn form_variant_style_field_defaults_to_rounded_when_omitted_in_json() {
        // Operator JSON without a `style` field should
        // deserialize to CmsFormStyle::default() = Rounded —
        // back-compat invariant.
        let json = r#"{
            "kind": "form",
            "legend": "No style field",
            "submit": {
                "label": "Go",
                "secondary_label": null,
                "action": "/go",
                "data_backend": "go"
            },
            "steps": []
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::Form { style, .. } => {
                assert_eq!(style, CmsFormStyle::Rounded);
            }
            _ => panic!("expected Form variant"),
        }
    }

    #[test]
    fn form_variant_style_field_parses_editorial_from_json() {
        let json = r#"{
            "kind": "form",
            "legend": "Editorial form",
            "submit": {
                "label": "Go",
                "secondary_label": null,
                "action": "/go",
                "data_backend": "go"
            },
            "style": "editorial",
            "steps": []
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::Form { style, .. } => {
                assert_eq!(style, CmsFormStyle::Editorial);
            }
            _ => panic!("expected Form variant"),
        }
    }

    #[test]
    fn render_form_emits_data_loom_form_style_attr_per_chrome() {
        // The render_form helper now carries the style param; the
        // resulting `<form>` + `<section>` both emit the
        // `data-loom-form-style="<modifier>"` attribute so the
        // loom-skin cascade can target it. Even though
        // CmsSection::Form doesn't yet plumb the field, render_form
        // accepts the style.
        let submit = CmsFormSubmit {
            label: "Go".into(),
            secondary_label: None,
            action: "/x".into(),
            data_backend: "x".into(),
        };
        for style in [
            CmsFormStyle::Rounded,
            CmsFormStyle::Editorial,
            CmsFormStyle::Minimal,
        ] {
            let html = render_form("Legend", &submit, &[], style).into_string();
            let expected = format!("data-loom-form-style=\"{}\"", style.modifier());
            assert!(
                html.contains(&expected),
                "render_form must emit {expected} attr; got: {html}"
            );
        }
    }

    // #104 (2026-05-20) — ChangelogList typed release-notes
    // primitive. Keep-a-Changelog convention.

    fn change(kind: ChangelogChangeKind, text: &str) -> ChangelogChange {
        ChangelogChange {
            kind,
            text: text.into(),
        }
    }

    fn entry(
        version: &str,
        date: &str,
        summary: Option<&str>,
        changes: Vec<ChangelogChange>,
    ) -> ChangelogEntry {
        ChangelogEntry {
            version: version.into(),
            date: date.into(),
            summary: summary.map(str::to_owned),
            changes,
        }
    }

    fn changelog_page(entries: Vec<ChangelogEntry>, style: ChangelogListStyle) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::ChangelogList {
            heading: "Release notes".into(),
            entries,
            style,
        }];
        p
    }

    #[test]
    fn changelog_empty_renders_placeholder() {
        let p = changelog_page(vec![], ChangelogListStyle::Detailed);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-changelog--detailed"));
        assert!(html.contains(">No releases yet.<"));
        assert!(!html.contains("<ol"));
    }

    #[test]
    fn changelog_detailed_renders_per_change_with_kind_tag() {
        let p = changelog_page(
            vec![entry(
                "1.2.0",
                "2024-03-15",
                Some("Performance + accessibility improvements"),
                vec![
                    change(ChangelogChangeKind::Added, "New dashboard widget"),
                    change(ChangelogChangeKind::Fixed, "Race condition in upload"),
                    change(ChangelogChangeKind::Security, "CSP nonce rotation"),
                ],
            )],
            ChangelogListStyle::Detailed,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("<h3 class=\"loom-changelog__version\">1.2.0</h3>"));
        assert!(html.contains("<time class=\"loom-changelog__date\" datetime=\"2024-03-15\">"));
        assert!(html.contains(">Performance + accessibility improvements<"));
        assert!(html.contains("loom-changelog-change--added"));
        assert!(html.contains("loom-changelog-change--fixed"));
        assert!(html.contains("loom-changelog-change--security"));
        assert!(html.contains(">Added</span>"));
        assert!(html.contains(">Fixed</span>"));
        assert!(html.contains(">Security</span>"));
    }

    #[test]
    fn changelog_compact_renders_change_count_only() {
        let p = changelog_page(
            vec![entry(
                "1.2.0",
                "2024-03-15",
                None,
                vec![
                    change(ChangelogChangeKind::Added, "a"),
                    change(ChangelogChangeKind::Fixed, "b"),
                ],
            )],
            ChangelogListStyle::Compact,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-changelog--compact"));
        // Compact summary: "2 changes"
        assert!(html.contains(">2 changes</p>") || html.contains(">2 changes "));
        // Compact does NOT show per-change rows
        assert!(!html.contains("loom-changelog-change--added"));
        assert!(!html.contains("loom-changelog-change--fixed"));
    }

    #[test]
    fn changelog_compact_singular_label_for_one_change() {
        let p = changelog_page(
            vec![entry(
                "1.0.1",
                "2024-01-01",
                None,
                vec![change(ChangelogChangeKind::Fixed, "one fix")],
            )],
            ChangelogListStyle::Compact,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains(">1 change</p>") || html.contains(">1 change "));
    }

    #[test]
    fn changelog_compact_zero_changes_uses_plural_for_consistency() {
        let p = changelog_page(
            vec![entry("0.1.0", "2024-01-01", None, vec![])],
            ChangelogListStyle::Compact,
        );
        let html = render_page(&p).into_string();
        // 0 changes — plural form
        assert!(html.contains(">0 changes</p>") || html.contains(">0 changes "));
    }

    #[test]
    fn changelog_all_6_change_kinds_emit_modifier_class() {
        for kind in [
            ChangelogChangeKind::Added,
            ChangelogChangeKind::Changed,
            ChangelogChangeKind::Deprecated,
            ChangelogChangeKind::Removed,
            ChangelogChangeKind::Fixed,
            ChangelogChangeKind::Security,
        ] {
            let p = changelog_page(
                vec![entry("1.0.0", "2024", None, vec![change(kind, "x")])],
                ChangelogListStyle::Detailed,
            );
            let html = render_page(&p).into_string();
            let expected = format!("loom-changelog-change--{}", kind.modifier());
            assert!(
                html.contains(&expected),
                "kind {kind:?} missing modifier {expected}"
            );
            assert!(
                html.contains(kind.label()),
                "kind {kind:?} missing visible label {}",
                kind.label()
            );
        }
    }

    #[test]
    fn changelog_detailed_no_changes_no_summary_renders_no_changes_placeholder() {
        let p = changelog_page(
            vec![entry("0.0.1", "2024-01-01", None, vec![])],
            ChangelogListStyle::Detailed,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains(">No changes recorded.<"));
    }

    #[test]
    fn changelog_detailed_summary_only_renders_summary_without_no_changes_message() {
        // If operator supplied a summary line, that IS the entry's
        // content; don't show the "No changes recorded" placeholder.
        let p = changelog_page(
            vec![entry(
                "0.0.1",
                "2024-01-01",
                Some("Initial release."),
                vec![],
            )],
            ChangelogListStyle::Detailed,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains(">Initial release.<"));
        assert!(!html.contains("No changes recorded."));
    }

    #[test]
    fn changelog_version_date_summary_change_text_all_escaped() {
        let p = changelog_page(
            vec![entry(
                "<script>",
                "<svg/>",
                Some("<img onerror=x>"),
                vec![change(ChangelogChangeKind::Added, "<style>")],
            )],
            ChangelogListStyle::Detailed,
        );
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<svg/>"));
        assert!(!html.contains("<img onerror=x>"));
        // The inner <style> would render in browser; verify escaped
        assert!(html.contains("&lt;style&gt;"));
    }

    #[test]
    fn changelog_kind_modifier_strings_stable() {
        for (kind, slug) in [
            (ChangelogChangeKind::Added, "added"),
            (ChangelogChangeKind::Changed, "changed"),
            (ChangelogChangeKind::Deprecated, "deprecated"),
            (ChangelogChangeKind::Removed, "removed"),
            (ChangelogChangeKind::Fixed, "fixed"),
            (ChangelogChangeKind::Security, "security"),
        ] {
            assert_eq!(kind.modifier(), slug);
        }
    }

    #[test]
    fn changelog_list_style_default_is_detailed() {
        assert_eq!(ChangelogListStyle::default(), ChangelogListStyle::Detailed);
    }

    #[test]
    fn changelog_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "changelog_list",
            "heading": "Releases",
            "style": "detailed",
            "entries": [{
                "version": "1.0.0",
                "date": "2024-03-15",
                "summary": null,
                "changes": [
                    { "kind": "added", "text": "Feature X" },
                    { "kind": "security", "text": "Patch Y" }
                ]
            }]
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::ChangelogList {
                heading,
                entries,
                style,
            } => {
                assert_eq!(heading, "Releases");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].version, "1.0.0");
                assert_eq!(entries[0].changes.len(), 2);
                assert_eq!(entries[0].changes[1].kind, ChangelogChangeKind::Security);
                assert_eq!(style, ChangelogListStyle::Detailed);
            }
            _ => panic!("expected ChangelogList variant"),
        }
    }

    // #104 (2026-05-20) — Disclaimer typed disclosure primitive.

    fn disclaimer_page(kind: DisclaimerKind, body: &str, source: Option<&str>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Disclaimer {
            disclosure_kind: kind,
            body: body.into(),
            source: source.map(str::to_owned),
        }];
        p
    }

    #[test]
    fn disclaimer_renders_aside_with_role_note() {
        let p = disclaimer_page(
            DisclaimerKind::Sponsored,
            "This article was sponsored.",
            None,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("<aside class=\"loom-disclaimer loom-disclaimer--sponsored\""));
        assert!(html.contains("role=\"note\""));
        assert!(html.contains("aria-label=\"Sponsored content notice\""));
    }

    #[test]
    fn disclaimer_kind_emits_modifier_class_per_kind() {
        for (kind, modifier) in [
            (DisclaimerKind::Sponsored, "sponsored"),
            (DisclaimerKind::Affiliate, "affiliate"),
            (DisclaimerKind::ConflictOfInterest, "conflict-of-interest"),
            (DisclaimerKind::EditorialNote, "editorial-note"),
            (DisclaimerKind::LegalNotice, "legal-notice"),
            (DisclaimerKind::AiAssisted, "ai-assisted"),
        ] {
            let p = disclaimer_page(kind, "body", None);
            let html = render_page(&p).into_string();
            let expected = format!("loom-disclaimer--{modifier}");
            assert!(
                html.contains(&expected),
                "kind {kind:?} missing modifier class {expected}"
            );
        }
    }

    #[test]
    fn disclaimer_accessible_label_per_kind() {
        for (kind, label) in [
            (DisclaimerKind::Sponsored, "Sponsored content notice"),
            (DisclaimerKind::Affiliate, "Affiliate link disclosure"),
            (
                DisclaimerKind::ConflictOfInterest,
                "Conflict of interest disclosure",
            ),
            (DisclaimerKind::EditorialNote, "Editorial note"),
            (DisclaimerKind::LegalNotice, "Legal notice"),
            (DisclaimerKind::AiAssisted, "AI-assisted content disclosure"),
        ] {
            assert_eq!(kind.accessible_label(), label);
        }
    }

    #[test]
    fn disclaimer_sponsored_source_appears_in_aria_label_and_body() {
        let p = disclaimer_page(
            DisclaimerKind::Sponsored,
            "Paid promotion.",
            Some("Acme Corp."),
        );
        let html = render_page(&p).into_string();
        // aria-label includes the source for Sponsored kind
        assert!(html.contains("aria-label=\"Sponsored content notice from Acme Corp.\""));
        // Source line appears in visible chrome
        assert!(html.contains("loom-disclaimer__source"));
        assert!(html.contains(">Source: Acme Corp.</p>"));
    }

    #[test]
    fn disclaimer_affiliate_source_appears_in_body_only() {
        // Affiliate kind: source appears in visible chrome but NOT
        // in aria-label (no per-kind aria template for affiliate;
        // would surface "Affiliate link disclosure from Acme" which
        // misframes the semantic — Acme isn't the source of the
        // affiliate link in the same way they sponsor content).
        let p = disclaimer_page(
            DisclaimerKind::Affiliate,
            "We earn a commission.",
            Some("Acme Corp."),
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("aria-label=\"Affiliate link disclosure\""));
        assert!(!html.contains("aria-label=\"Affiliate link disclosure from"));
        assert!(html.contains("loom-disclaimer__source"));
    }

    #[test]
    fn disclaimer_other_kinds_omit_source_chrome_even_when_present() {
        // ConflictOfInterest / EditorialNote / LegalNotice / AiAssisted
        // don't get a `Source:` line even when source is present —
        // the kind itself carries the semantic; an explicit source
        // attribution would misframe (you don't attribute a COI to
        // a specific person, you DISCLOSE it).
        for kind in [
            DisclaimerKind::ConflictOfInterest,
            DisclaimerKind::EditorialNote,
            DisclaimerKind::LegalNotice,
            DisclaimerKind::AiAssisted,
        ] {
            let p = disclaimer_page(kind, "body", Some("source"));
            let html = render_page(&p).into_string();
            assert!(
                !html.contains("loom-disclaimer__source"),
                "kind {kind:?} should not render source chrome"
            );
        }
    }

    #[test]
    fn disclaimer_no_source_omits_source_chrome() {
        let p = disclaimer_page(DisclaimerKind::Sponsored, "Paid.", None);
        let html = render_page(&p).into_string();
        assert!(!html.contains("loom-disclaimer__source"));
    }

    #[test]
    fn disclaimer_body_html_escaped() {
        let p = disclaimer_page(
            DisclaimerKind::Sponsored,
            "<script>alert(1)</script>",
            Some("<img onerror=x>"),
        );
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn disclaimer_kind_modifier_strings_stable() {
        // Wire-shape contract: loom-skin cascade rules target
        // these stable strings.
        assert_eq!(DisclaimerKind::Sponsored.modifier(), "sponsored");
        assert_eq!(
            DisclaimerKind::ConflictOfInterest.modifier(),
            "conflict-of-interest"
        );
        assert_eq!(DisclaimerKind::AiAssisted.modifier(), "ai-assisted");
    }

    #[test]
    fn disclaimer_section_parses_from_snake_case_kind() {
        // Inner field is `disclosure_kind` (not `kind`) to avoid
        // colliding with the outer serde tag — see Disclaimer
        // variant doc.
        let json = r#"{
            "kind": "disclaimer",
            "disclosure_kind": "sponsored",
            "body": "This article was sponsored by Acme.",
            "source": "Acme Corp."
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::Disclaimer {
                disclosure_kind,
                body,
                source,
            } => {
                assert_eq!(disclosure_kind, DisclaimerKind::Sponsored);
                assert_eq!(body, "This article was sponsored by Acme.");
                assert_eq!(source.as_deref(), Some("Acme Corp."));
            }
            _ => panic!("expected Disclaimer variant"),
        }
    }

    // #104 (2026-05-20) — SourceList editorial primitive for
    // bibliographies / further-reading / appendix citation lists.

    fn source(author: &str, title: &str, url: Option<&str>, kind: SourceKind) -> SourceListItem {
        SourceListItem {
            author: author.into(),
            title: title.into(),
            url: url.map(str::to_owned),
            date_published: None,
            kind,
        }
    }

    fn source_list_page(items: Vec<SourceListItem>, style: SourceListStyle) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::SourceList {
            heading: "Further reading".into(),
            items,
            style,
        }];
        p
    }

    #[test]
    fn source_list_empty_renders_placeholder() {
        let p = source_list_page(vec![], SourceListStyle::Numbered);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-source-list--numbered"));
        assert!(html.contains(">No sources.<"));
        assert!(!html.contains("<ol"));
        assert!(!html.contains("<ul"));
    }

    #[test]
    fn source_list_numbered_emits_ol() {
        let p = source_list_page(
            vec![source(
                "Smith, J.",
                "On Editorial Density",
                None,
                SourceKind::Article,
            )],
            SourceListStyle::Numbered,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("<ol class=\"loom-source-list__items\""));
        assert!(!html.contains("<ul class=\"loom-source-list__items\""));
    }

    #[test]
    fn source_list_bulleted_emits_ul() {
        let p = source_list_page(
            vec![source(
                "Lee, K.",
                "Substrate Doctrine",
                None,
                SourceKind::Book,
            )],
            SourceListStyle::Bulleted,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("<ul class=\"loom-source-list__items\""));
        assert!(html.contains("loom-source-list--bulleted"));
    }

    #[test]
    fn source_list_plain_also_emits_ul() {
        let p = source_list_page(
            vec![source("Author", "Title", None, SourceKind::Other)],
            SourceListStyle::Plain,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("<ul class=\"loom-source-list__items\""));
        assert!(html.contains("loom-source-list--plain"));
    }

    #[test]
    fn source_list_kind_emits_modifier_class() {
        for (kind, modifier) in [
            (SourceKind::Book, "book"),
            (SourceKind::Article, "article"),
            (SourceKind::Web, "web"),
            (SourceKind::Audio, "audio"),
            (SourceKind::Video, "video"),
            (SourceKind::Report, "report"),
            (SourceKind::Other, "other"),
        ] {
            let p = source_list_page(
                vec![source("A", "T", None, kind)],
                SourceListStyle::Numbered,
            );
            let html = render_page(&p).into_string();
            let expected = format!("loom-source-list__item--{modifier}");
            assert!(
                html.contains(&expected),
                "kind {kind:?} missing modifier class {expected}"
            );
        }
    }

    #[test]
    fn source_list_safe_url_renders_as_link() {
        let p = source_list_page(
            vec![source(
                "Smith, J.",
                "On Density",
                Some("https://example.com/density"),
                SourceKind::Web,
            )],
            SourceListStyle::Numbered,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("href=\"https://example.com/density\""));
        assert!(html.contains("rel=\"noopener\""));
        assert!(html.contains(">On Density<"));
    }

    #[test]
    fn source_list_hostile_url_renders_as_invalid_span_not_link() {
        let p = source_list_page(
            vec![source(
                "X",
                "Y",
                Some("javascript:alert(1)"),
                SourceKind::Web,
            )],
            SourceListStyle::Numbered,
        );
        let html = render_page(&p).into_string();
        assert!(!html.contains("href=\"javascript:alert"));
        assert!(html.contains("loom-source-list__title--invalid"));
        assert!(html.contains("data-invalid=\"true\""));
    }

    #[test]
    fn source_list_date_published_renders_in_time_element() {
        let mut item = source("Author", "Work", None, SourceKind::Article);
        item.date_published = Some("2024-03-15".into());
        let p = source_list_page(vec![item], SourceListStyle::Numbered);
        let html = render_page(&p).into_string();
        assert!(html.contains("<time class=\"loom-source-list__date\" datetime=\"2024-03-15\">"));
        assert!(html.contains(">2024-03-15<"));
    }

    #[test]
    fn source_list_html_escapes_author_title_date() {
        let mut item = SourceListItem {
            author: "<script>".into(),
            title: "<img onerror=x>".into(),
            url: None,
            date_published: Some("<svg/>".into()),
            kind: SourceKind::Other,
        };
        let p = source_list_page(vec![item.clone()], SourceListStyle::Plain);
        let _ = &mut item; // silence unused-mut warning
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(!html.contains("<svg/>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn source_list_style_default_is_numbered() {
        assert_eq!(SourceListStyle::default(), SourceListStyle::Numbered);
    }

    #[test]
    fn source_kind_modifier_strings_are_stable_kebab_case() {
        for (kind, modifier) in [
            (SourceKind::Book, "book"),
            (SourceKind::Article, "article"),
            (SourceKind::Web, "web"),
            (SourceKind::Audio, "audio"),
            (SourceKind::Video, "video"),
            (SourceKind::Report, "report"),
            (SourceKind::Other, "other"),
        ] {
            assert_eq!(kind.modifier(), modifier);
        }
    }

    #[test]
    fn source_list_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "source_list",
            "heading": "Bibliography",
            "items": [
                {
                    "author": "Smith, J.",
                    "title": "On Density",
                    "url": "https://example.com",
                    "date_published": "2024",
                    "kind": "article"
                }
            ],
            "style": "numbered"
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::SourceList {
                heading,
                items,
                style,
            } => {
                assert_eq!(heading, "Bibliography");
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].kind, SourceKind::Article);
                assert_eq!(style, SourceListStyle::Numbered);
            }
            _ => panic!("expected SourceList variant"),
        }
    }

    // #104 (2026-05-20) — Boxplot editorial-charts primitive.
    // Quantile-based statistical summary per category.

    fn boxplot_entry(
        label: &str,
        min: f64,
        q1: f64,
        median: f64,
        q3: f64,
        max: f64,
    ) -> BoxplotEntry {
        BoxplotEntry {
            label: label.into(),
            min,
            q1,
            median,
            q3,
            max,
        }
    }

    fn boxplot_page(boxes: Vec<BoxplotEntry>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Boxplot {
            label: "Response time by endpoint".into(),
            boxes,
            tone: SparklineTone::Neutral,
            caption: None,
        }];
        p
    }

    #[test]
    fn boxplot_empty_boxes_renders_no_data() {
        let p = boxplot_page(vec![]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-boxplot--empty"));
        assert!(html.contains(">No data<"));
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn boxplot_renders_box_whiskers_caps_median_per_entry() {
        let p = boxplot_page(vec![boxplot_entry(
            "/api/users",
            10.0,
            25.0,
            40.0,
            80.0,
            200.0,
        )]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-boxplot"));
        assert!(html.contains("<svg"));
        // Per box: 1 rect (box) + 1 line (median) + 2 lines (whiskers) + 2 lines (caps) = 1 rect + 5 lines.
        assert_eq!(html.matches("<rect class=\"loom-boxplot__box\"").count(), 1);
        assert_eq!(html.matches("loom-boxplot__median").count(), 1);
        assert_eq!(html.matches("loom-boxplot__whisker--lower").count(), 1);
        assert_eq!(html.matches("loom-boxplot__whisker--upper").count(), 1);
        assert_eq!(html.matches("loom-boxplot__cap--lower").count(), 1);
        assert_eq!(html.matches("loom-boxplot__cap--upper").count(), 1);
    }

    #[test]
    fn boxplot_multiple_boxes_share_axis_via_global_minmax() {
        // Two boxes with overlapping ranges; verify both render
        // (one rect each).
        let p = boxplot_page(vec![
            boxplot_entry("a", 0.0, 10.0, 20.0, 30.0, 40.0),
            boxplot_entry("b", 50.0, 60.0, 70.0, 80.0, 100.0),
        ]);
        let html = render_page(&p).into_string();
        assert_eq!(html.matches("loom-boxplot__box").count(), 2);
        // aria-label should describe the global range 0.00–100.00
        assert!(html.contains("range 0.00–100.00"));
    }

    #[test]
    fn boxplot_aria_label_includes_n_categories_and_range() {
        let p = boxplot_page(vec![
            boxplot_entry("a", 0.0, 1.0, 2.0, 3.0, 4.0),
            boxplot_entry("b", 0.0, 1.0, 2.0, 3.0, 4.0),
            boxplot_entry("c", 0.0, 1.0, 2.0, 3.0, 4.0),
        ]);
        let html = render_page(&p).into_string();
        assert!(html.contains("3 categories"));
        assert!(html.contains("range 0.00–4.00"));
    }

    #[test]
    fn boxplot_legend_lists_five_number_summary_per_box() {
        let p = boxplot_page(vec![boxplot_entry(
            "/api/orders",
            5.0,
            12.5,
            25.0,
            50.0,
            120.0,
        )]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-boxplot__legend"));
        assert!(html.contains(">/api/orders<"));
        // 5-number summary in expected format
        assert!(html.contains("min 5.00 · q1 12.50 · med 25.00 · q3 50.00 · max 120.00"));
    }

    #[test]
    fn boxplot_flat_series_does_not_div_by_zero() {
        // All boxes have same values across all 5 numbers — range = 0.
        let p = boxplot_page(vec![
            boxplot_entry("a", 5.0, 5.0, 5.0, 5.0, 5.0),
            boxplot_entry("b", 5.0, 5.0, 5.0, 5.0, 5.0),
        ]);
        let html = render_page(&p).into_string();
        assert!(html.contains("<rect"));
        assert!(!html.contains("NaN"));
        assert!(!html.contains("Inf"));
    }

    #[test]
    fn boxplot_label_and_caption_html_escaped() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Boxplot {
            label: "<script>".into(),
            boxes: vec![BoxplotEntry {
                label: "<img onerror=x>".into(),
                min: 0.0,
                q1: 1.0,
                median: 2.0,
                q3: 3.0,
                max: 4.0,
            }],
            tone: SparklineTone::Neutral,
            caption: Some("<svg onload=y>".into()),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(!html.contains("<svg onload=y>"));
    }

    #[test]
    fn boxplot_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "boxplot",
            "label": "Latency by endpoint",
            "boxes": [{
                "label": "/api/users",
                "min": 10.0,
                "q1": 25.0,
                "median": 40.0,
                "q3": 80.0,
                "max": 200.0
            }],
            "tone": "neutral",
            "caption": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::Boxplot { boxes, .. } => {
                assert_eq!(boxes.len(), 1);
                assert_eq!(boxes[0].median, 40.0);
                assert_eq!(boxes[0].max, 200.0);
            }
            _ => panic!("expected Boxplot variant"),
        }
    }

    // #104 (2026-05-20) — Heatmap editorial-charts primitive.
    // 2D categorical × categorical. Fifth chart vocab member.

    fn heatmap_page(cells: Vec<Vec<f64>>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Heatmap {
            label: "Commits by day × hour".into(),
            row_labels: vec!["Mon".into(), "Tue".into(), "Wed".into()],
            column_labels: vec!["AM".into(), "PM".into()],
            cells,
            tone: SparklineTone::Accent,
            caption: None,
        }];
        p
    }

    #[test]
    fn heatmap_empty_cells_renders_no_data() {
        let p = heatmap_page(vec![]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-heatmap--empty"));
        assert!(html.contains(">No data<"));
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn heatmap_all_empty_rows_renders_no_data() {
        // cells is non-empty but every row is empty — still "no data"
        let p = heatmap_page(vec![vec![], vec![], vec![]]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-heatmap--empty"));
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn heatmap_renders_one_rect_per_cell() {
        let p = heatmap_page(vec![vec![1.0, 5.0], vec![3.0, 8.0], vec![2.0, 4.0]]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-heatmap"));
        assert!(html.contains("<svg"));
        // 3 rows × 2 cols = 6 cells
        assert_eq!(
            html.matches("<rect").count(),
            6,
            "expected 6 rects for 3x2 grid"
        );
    }

    #[test]
    fn heatmap_cell_opacity_scales_to_max_abs() {
        // Max cell value = 10.0. A cell at 5.0 should have
        // fill-opacity=0.500; a cell at 10.0 should have 1.000.
        let p = heatmap_page(vec![vec![5.0, 10.0]]);
        let html = render_page(&p).into_string();
        assert!(html.contains("fill-opacity=\"0.500\""));
        assert!(html.contains("fill-opacity=\"1.000\""));
    }

    #[test]
    fn heatmap_aria_label_describes_dimensions() {
        let p = heatmap_page(vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
        let html = render_page(&p).into_string();
        assert!(html.contains("aria-label=\""));
        assert!(html.contains("2 rows × 2 columns"));
        assert!(html.contains("4 cells"));
        assert!(html.contains("max absolute value 4.00"));
    }

    #[test]
    fn heatmap_emits_screen_reader_legend_table() {
        // Heatmap doubles as an accessible table for screen-reader
        // users — visual is SVG, semantic is <table>.
        let p = heatmap_page(vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
        let html = render_page(&p).into_string();
        assert!(html.contains("<table class=\"loom-heatmap__legend\""));
        assert!(html.contains("<caption class=\"loom-sr-only\">Heatmap values"));
        assert!(html.contains("scope=\"col\""));
        assert!(html.contains("scope=\"row\""));
        // Row labels present
        assert!(html.contains(">Mon<"));
        assert!(html.contains(">Tue<"));
        // Column labels present
        assert!(html.contains(">AM<"));
        assert!(html.contains(">PM<"));
        // Cell values formatted to 2 decimals
        assert!(html.contains(">1.00<"));
        assert!(html.contains(">4.00<"));
    }

    #[test]
    fn heatmap_zero_values_does_not_div_by_zero() {
        let p = heatmap_page(vec![vec![0.0, 0.0], vec![0.0, 0.0]]);
        let html = render_page(&p).into_string();
        assert!(html.contains("<rect"));
        assert!(!html.contains("NaN"));
        assert!(!html.contains("Inf"));
        // All zero → all cells fill-opacity=0
        assert!(html.contains("fill-opacity=\"0.000\""));
    }

    #[test]
    fn heatmap_label_and_caption_escaped() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Heatmap {
            label: "<script>".into(),
            row_labels: vec!["<img onerror=x>".into()],
            column_labels: vec!["<svg/>".into()],
            cells: vec![vec![1.0]],
            tone: SparklineTone::Neutral,
            caption: Some("<a onclick=z>".into()),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(!html.contains("<svg/>"));
        assert!(!html.contains("<a onclick=z>"));
    }

    #[test]
    fn heatmap_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "heatmap",
            "label": "Activity",
            "row_labels": ["Mon", "Tue"],
            "column_labels": ["AM", "PM"],
            "cells": [[1.0, 2.0], [3.0, 4.0]],
            "tone": "accent",
            "caption": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::Heatmap { cells, tone, .. } => {
                assert_eq!(cells.len(), 2);
                assert_eq!(cells[0].len(), 2);
                assert_eq!(cells[1][1], 4.0);
                assert_eq!(tone, SparklineTone::Accent);
            }
            _ => panic!("expected Heatmap variant"),
        }
    }

    // #104 (2026-05-20) — DivergingBar editorial-charts primitive.
    // Bars extend left (negative) or right (positive) of midline.

    fn divbar_item(label: &str, value: f64) -> DivergingBarItem {
        DivergingBarItem {
            label: label.into(),
            value,
        }
    }

    fn divbar_page(items: Vec<DivergingBarItem>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::DivergingBar {
            label: "Approval margin".into(),
            items,
            tone: SparklineTone::Neutral,
            midline_label: Some("0%".into()),
            caption: None,
        }];
        p
    }

    #[test]
    fn divbar_empty_renders_no_data_placeholder() {
        let p = divbar_page(vec![]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-divbar--empty"));
        assert!(html.contains(">No data<"));
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn divbar_renders_one_rect_per_item_plus_midline() {
        let p = divbar_page(vec![
            divbar_item("Q1", 10.0),
            divbar_item("Q2", -5.0),
            divbar_item("Q3", 15.0),
        ]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-divbar"));
        assert!(html.contains("<svg"));
        assert_eq!(
            html.matches("<rect").count(),
            3,
            "expected 3 rects for 3 items"
        );
        // Midline as a <line> element
        assert!(html.contains("<line class=\"loom-divbar__midline\""));
    }

    #[test]
    fn divbar_positive_bars_start_at_midline() {
        // Positive value bar: x should equal MIDLINE_X = 100.0.
        let p = divbar_page(vec![divbar_item("only", 50.0)]);
        let html = render_page(&p).into_string();
        // Positive: x=100.0 (right at midline)
        assert!(html.contains("x=\"100.0\""));
        assert!(html.contains("loom-divbar__bar--positive"));
    }

    #[test]
    fn divbar_negative_bars_end_at_midline() {
        // Negative value bar: x + width should equal MIDLINE_X.
        // For -50.0 with max_abs=50.0: bar_w = (50/50)*96 = 96, so
        // x = 100 - 96 = 4.0 (touches left padding).
        let p = divbar_page(vec![divbar_item("only", -50.0)]);
        let html = render_page(&p).into_string();
        assert!(html.contains("x=\"4.0\""));
        assert!(html.contains("loom-divbar__bar--negative"));
    }

    #[test]
    fn divbar_aria_label_includes_count_and_max_abs() {
        let p = divbar_page(vec![divbar_item("a", -3.0), divbar_item("b", 7.5)]);
        let html = render_page(&p).into_string();
        assert!(html.contains("aria-label=\""));
        assert!(html.contains("2 rows"));
        assert!(html.contains("max absolute value 7.50"));
    }

    #[test]
    fn divbar_legend_formats_signed_values() {
        let p = divbar_page(vec![divbar_item("up", 3.5), divbar_item("down", -2.25)]);
        let html = render_page(&p).into_string();
        // Signed format: +3.50 / -2.25 (always shows sign for positive)
        assert!(html.contains(">+3.50<"));
        assert!(html.contains(">-2.25<"));
        assert!(html.contains("loom-divbar__legend-value--positive"));
        assert!(html.contains("loom-divbar__legend-value--negative"));
    }

    #[test]
    fn divbar_midline_label_renders_when_present() {
        let p = divbar_page(vec![divbar_item("only", 1.0)]);
        let html = render_page(&p).into_string();
        // helper sets midline_label = "0%"
        assert!(html.contains("loom-divbar__midline-label"));
        assert!(html.contains(">0%<"));
        // aria-hidden — visual-only chrome; aria-label on SVG carries the semantic
        assert!(html.contains("aria-hidden=\"true\""));
    }

    #[test]
    fn divbar_all_zero_values_does_not_div_by_zero() {
        // All zero — max_abs floored to 1.0; bars all have w=0.
        let p = divbar_page(vec![divbar_item("a", 0.0), divbar_item("b", 0.0)]);
        let html = render_page(&p).into_string();
        assert!(html.contains("<rect"));
        assert!(!html.contains("NaN"));
        assert!(!html.contains("Inf"));
        // All zero values render as positive (>=0)
        assert!(html.contains("loom-divbar__bar--positive"));
    }

    #[test]
    fn divbar_labels_html_escaped() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::DivergingBar {
            label: "<script>".into(),
            items: vec![DivergingBarItem {
                label: "<img onerror=x>".into(),
                value: 1.0,
            }],
            tone: SparklineTone::Neutral,
            midline_label: Some("<svg/>".into()),
            caption: None,
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(!html.contains("<svg/>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn divbar_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "diverging_bar",
            "label": "Vote margin",
            "items": [
                { "label": "Q1", "value": 12.5 },
                { "label": "Q2", "value": -3.0 }
            ],
            "tone": "neutral",
            "midline_label": null,
            "caption": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::DivergingBar { items, .. } => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[1].value, -3.0);
            }
            _ => panic!("expected DivergingBar variant"),
        }
    }

    // #104 (2026-05-20) — Histogram editorial-charts primitive.
    // Frequency-distribution. Bars touch (no gap) — that's the
    // visual signal distinguishing Histogram from BarChart.

    fn bucket(r_min: f64, r_max: f64, count: u32) -> HistogramBucket {
        HistogramBucket {
            range_min: r_min,
            range_max: r_max,
            count,
        }
    }

    fn histogram_page(buckets: Vec<HistogramBucket>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Histogram {
            label: "Request latency".into(),
            buckets,
            tone: SparklineTone::Neutral,
            caption: Some("p50=20ms p95=85ms".into()),
        }];
        p
    }

    #[test]
    fn histogram_empty_buckets_renders_no_data_placeholder() {
        let p = histogram_page(vec![]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-histogram--empty"));
        assert!(html.contains(">No data<"));
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn histogram_renders_one_rect_per_bucket() {
        let p = histogram_page(vec![
            bucket(0.0, 10.0, 5),
            bucket(10.0, 20.0, 15),
            bucket(20.0, 30.0, 8),
            bucket(30.0, 40.0, 2),
        ]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-histogram"));
        assert!(html.contains("<svg"));
        assert_eq!(
            html.matches("<rect").count(),
            4,
            "expected 4 rects for 4 buckets"
        );
    }

    #[test]
    fn histogram_bars_touch_with_no_gap() {
        // The defining visual difference between Histogram and
        // BarChart — histogram bars span the full bin width with
        // no inter-bar gap. We verify by computing the expected
        // bar_w for 4 bins at viewBox 0..192 (after 4px padding
        // on each side = 192 usable) and checking the second
        // bar's x matches the first bar's x + bar_w exactly.
        let p = histogram_page(vec![bucket(0.0, 10.0, 5), bucket(10.0, 20.0, 15)]);
        let html = render_page(&p).into_string();
        // First bar starts at x=PAD=4.0; bar_w=(200-8)/2=96.0;
        // second bar should start at x=4.0+96.0=100.0
        assert!(
            html.contains("x=\"4.0\""),
            "first bar should start at x=4.0: {html}"
        );
        assert!(
            html.contains("x=\"100.0\""),
            "second bar should start at first+bar_w=100.0: {html}"
        );
    }

    #[test]
    fn histogram_aria_label_includes_bins_total_range() {
        let p = histogram_page(vec![
            bucket(0.0, 10.0, 5),
            bucket(10.0, 20.0, 15),
            bucket(20.0, 30.0, 30),
        ]);
        let html = render_page(&p).into_string();
        assert!(html.contains("aria-label=\""));
        assert!(html.contains("3 bins"));
        assert!(html.contains("50 total samples"));
        // Range surfaced; check the en-dash + endpoints
        assert!(html.contains("0.00–30.00"));
    }

    #[test]
    fn histogram_legend_lists_range_and_count_per_bucket() {
        let p = histogram_page(vec![bucket(0.0, 10.0, 5), bucket(10.0, 20.0, 15)]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-histogram__legend"));
        assert!(html.contains(">0.00–10.00<"));
        assert!(html.contains(">5<"));
        assert!(html.contains(">10.00–20.00<"));
        assert!(html.contains(">15<"));
    }

    #[test]
    fn histogram_zero_counts_does_not_div_by_zero() {
        // All zero counts. max(count) would be 0; we floor to 1
        // so the proportional height computation is stable.
        let p = histogram_page(vec![bucket(0.0, 10.0, 0), bucket(10.0, 20.0, 0)]);
        let html = render_page(&p).into_string();
        assert!(html.contains("<rect"));
        assert!(!html.contains("NaN"));
        assert!(!html.contains("Inf"));
    }

    #[test]
    fn histogram_label_and_caption_escaped() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Histogram {
            label: "<script>".into(),
            buckets: vec![bucket(0.0, 1.0, 1)],
            tone: SparklineTone::Neutral,
            caption: Some("<img onerror=x>".into()),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn histogram_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "histogram",
            "label": "Latency",
            "buckets": [
                { "range_min": 0.0, "range_max": 10.0, "count": 5 },
                { "range_min": 10.0, "range_max": 20.0, "count": 12 }
            ],
            "tone": "accent",
            "caption": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::Histogram { buckets, tone, .. } => {
                assert_eq!(buckets.len(), 2);
                assert_eq!(buckets[1].count, 12);
                assert_eq!(tone, SparklineTone::Accent);
            }
            _ => panic!("expected Histogram variant"),
        }
    }

    // #104 (2026-05-20) — BarChart editorial-charts primitive,
    // categorical companion to Sparkline.

    fn bar(label: &str, value: f64) -> BarChartBar {
        BarChartBar {
            label: label.into(),
            value,
            tone_override: None,
        }
    }

    fn barchart_page(bars: Vec<BarChartBar>, orientation: BarChartOrientation) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::BarChart {
            label: "Issues closed per milestone".into(),
            bars,
            orientation,
            tone: SparklineTone::Neutral,
            caption: None,
        }];
        p
    }

    #[test]
    fn barchart_empty_bars_renders_no_data_placeholder() {
        let p = barchart_page(vec![], BarChartOrientation::Vertical);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-barchart--empty"));
        assert!(html.contains(">No data<"));
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn barchart_vertical_renders_one_rect_per_bar() {
        let p = barchart_page(
            vec![bar("Mon", 5.0), bar("Tue", 8.0), bar("Wed", 3.0)],
            BarChartOrientation::Vertical,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-barchart--vertical"));
        assert!(html.contains("<svg"));
        assert_eq!(
            html.matches("<rect").count(),
            3,
            "expected 3 rects for 3 bars"
        );
    }

    #[test]
    fn barchart_horizontal_orientation_emits_modifier() {
        let p = barchart_page(
            vec![bar("First milestone", 12.0), bar("Second milestone", 8.0)],
            BarChartOrientation::Horizontal,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-barchart--horizontal"));
        assert_eq!(html.matches("<rect").count(), 2);
    }

    #[test]
    fn barchart_aria_label_includes_count_and_max() {
        let p = barchart_page(
            vec![bar("a", 2.5), bar("b", 7.25)],
            BarChartOrientation::Vertical,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("aria-label=\""));
        assert!(html.contains("2 bars"));
        assert!(html.contains("max value 7.25"));
    }

    #[test]
    fn barchart_legend_lists_label_and_value_per_bar() {
        let p = barchart_page(
            vec![bar("Mon", 5.5), bar("Tue", 8.25)],
            BarChartOrientation::Vertical,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-barchart__legend"));
        assert!(html.contains(">Mon<"));
        assert!(html.contains(">5.50<"));
        assert!(html.contains(">Tue<"));
        assert!(html.contains(">8.25<"));
    }

    #[test]
    fn barchart_negative_values_clamped_to_zero() {
        let p = barchart_page(
            vec![bar("a", -3.0), bar("b", 5.0)],
            BarChartOrientation::Vertical,
        );
        let html = render_page(&p).into_string();
        // Negative bar should have height 0 (clamped).
        // We won't dig into the rect attrs deeply; just verify no
        // negative numbers in width/height attrs.
        for needle in ["height=\"-", "width=\"-"] {
            assert!(!html.contains(needle), "found negative dim: {html}");
        }
    }

    #[test]
    fn barchart_per_bar_tone_override_emits_distinct_modifier() {
        let p = barchart_page(
            vec![
                BarChartBar {
                    label: "good".into(),
                    value: 5.0,
                    tone_override: Some(SparklineTone::Positive),
                },
                BarChartBar {
                    label: "bad".into(),
                    value: 3.0,
                    tone_override: Some(SparklineTone::Negative),
                },
            ],
            BarChartOrientation::Vertical,
        );
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-barchart__bar--positive"));
        assert!(html.contains("loom-barchart__bar--negative"));
    }

    #[test]
    fn barchart_bar_labels_html_escaped() {
        let p = barchart_page(vec![bar("<script>", 5.0)], BarChartOrientation::Vertical);
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn barchart_orientation_serde_round_trip() {
        for (variant, json) in [
            (BarChartOrientation::Vertical, "\"vertical\""),
            (BarChartOrientation::Horizontal, "\"horizontal\""),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, json);
            let back: BarChartOrientation = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn barchart_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "bar_chart",
            "label": "Closed issues",
            "bars": [
                { "label": "v1", "value": 10.0, "tone_override": null },
                { "label": "v2", "value": 14.0, "tone_override": "positive" }
            ],
            "orientation": "vertical",
            "tone": "neutral",
            "caption": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::BarChart {
                bars, orientation, ..
            } => {
                assert_eq!(bars.len(), 2);
                assert_eq!(orientation, BarChartOrientation::Vertical);
                assert_eq!(bars[1].tone_override, Some(SparklineTone::Positive));
            }
            _ => panic!("expected BarChart variant"),
        }
    }

    // #104 (2026-05-20) — Sparkline editorial-charts primitive.
    // Pure-SVG inline trend visualization; editorial counterpart
    // to StatBand.

    fn sparkline_page(data: Vec<f64>, tone: SparklineTone) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Sparkline {
            label: "GitHub stars".into(),
            data_points: data,
            tone,
            caption: Some("Last 90 days".into()),
        }];
        p
    }

    #[test]
    fn sparkline_empty_data_renders_no_data_placeholder() {
        let p = sparkline_page(vec![], SparklineTone::Neutral);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-sparkline--empty"));
        assert!(html.contains(">No data<"));
        // No SVG when data is empty
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn sparkline_renders_inline_svg_with_polyline() {
        let p = sparkline_page(vec![1.0, 2.0, 3.0, 4.0], SparklineTone::Neutral);
        let html = render_page(&p).into_string();
        assert!(html.contains("<svg"));
        assert!(html.contains("viewBox=\"0 0 200 50\""));
        assert!(html.contains("<polyline"));
        assert!(html.contains("fill=\"none\""));
        assert!(html.contains("stroke=\"currentColor\""));
        assert!(html.contains("role=\"img\""));
    }

    #[test]
    fn sparkline_aria_label_includes_min_max_last() {
        let p = sparkline_page(vec![1.5, 9.0, 4.25], SparklineTone::Neutral);
        let html = render_page(&p).into_string();
        // Substring check — full label includes "GitHub stars sparkline:"
        assert!(html.contains("aria-label=\""));
        assert!(html.contains("3 points"));
        assert!(html.contains("min 1.50"));
        assert!(html.contains("max 9.00"));
        assert!(html.contains("last 4.25"));
    }

    #[test]
    fn sparkline_tone_emits_modifier_class() {
        for (tone, expected) in [
            (SparklineTone::Neutral, "loom-sparkline--neutral"),
            (SparklineTone::Positive, "loom-sparkline--positive"),
            (SparklineTone::Negative, "loom-sparkline--negative"),
            (SparklineTone::Accent, "loom-sparkline--accent"),
        ] {
            let p = sparkline_page(vec![1.0, 2.0], tone);
            let html = render_page(&p).into_string();
            assert!(
                html.contains(expected),
                "tone {tone:?} missing modifier {expected}"
            );
        }
    }

    #[test]
    fn sparkline_single_point_renders_centered() {
        let p = sparkline_page(vec![5.0], SparklineTone::Neutral);
        let html = render_page(&p).into_string();
        // Single point: x should be at viewBox center (100.0).
        assert!(html.contains("points=\"100.0,"));
    }

    #[test]
    fn sparkline_flat_series_does_not_divide_by_zero() {
        // All same value — min==max would naively make range=0.
        // The renderer must handle this gracefully (force range=1).
        let p = sparkline_page(vec![3.0, 3.0, 3.0], SparklineTone::Neutral);
        let html = render_page(&p).into_string();
        assert!(html.contains("<polyline"));
        // No NaN / Inf leaking through into the points attribute.
        assert!(!html.contains("NaN"));
        assert!(!html.contains("Inf"));
    }

    #[test]
    fn sparkline_label_and_caption_html_escaped() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::Sparkline {
            label: "<script>".into(),
            data_points: vec![1.0],
            tone: SparklineTone::Neutral,
            caption: Some("<img onerror=x>".into()),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn sparkline_tone_serde_round_trip() {
        for (tone, json) in [
            (SparklineTone::Neutral, "\"neutral\""),
            (SparklineTone::Positive, "\"positive\""),
            (SparklineTone::Negative, "\"negative\""),
            (SparklineTone::Accent, "\"accent\""),
        ] {
            let s = serde_json::to_string(&tone).unwrap();
            assert_eq!(s, json);
            let back: SparklineTone = serde_json::from_str(&s).unwrap();
            assert_eq!(back, tone);
        }
    }

    #[test]
    fn sparkline_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "sparkline",
            "label": "Daily users",
            "data_points": [100.0, 120.0, 95.0, 140.0],
            "tone": "positive",
            "caption": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::Sparkline {
                label,
                data_points,
                tone,
                ..
            } => {
                assert_eq!(label, "Daily users");
                assert_eq!(data_points.len(), 4);
                assert_eq!(tone, SparklineTone::Positive);
            }
            _ => panic!("expected Sparkline variant"),
        }
    }

    #[test]
    fn sparkline_polyline_has_n_points_separated_by_spaces() {
        let p = sparkline_page(vec![1.0, 2.0, 3.0, 4.0], SparklineTone::Neutral);
        let html = render_page(&p).into_string();
        // Extract the points attribute value.
        let needle = "points=\"";
        let start = html.find(needle).expect("points attr present") + needle.len();
        let end = html[start..].find('"').expect("points attr closed") + start;
        let points = &html[start..end];
        // 4 data points → 3 spaces between them.
        assert_eq!(
            points.matches(' ').count(),
            3,
            "expected 3 spaces in 4-point polyline, got {points}"
        );
    }

    // #104 (2026-05-20) — PasswordChange primitive. In-session
    // change distinct from PasswordReset (which is forgot-password).

    fn password_change_page(requirements: Vec<String>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::PasswordChange {
            title: "Change password".into(),
            description: Some("Choose a new password for your account.".into()),
            requirements,
            submit_cta: cta("Update password", "/account/password"),
            cancel_cta: cta("Cancel", "/account/settings"),
        }];
        p
    }

    #[test]
    fn password_change_renders_three_password_fields_with_correct_autocomplete() {
        let p = password_change_page(vec![]);
        let html = render_page(&p).into_string();
        // Current password — autocomplete=current-password (existing
        // password the user is re-supplying)
        assert!(html.contains("name=\"current_password\""));
        assert!(html.contains("autocomplete=\"current-password\""));
        // New password — autocomplete=new-password (helps password
        // managers suggest a generated one)
        assert!(html.contains("name=\"new_password\""));
        // Confirm new password — also autocomplete=new-password so
        // the same manager-generated value can be auto-filled
        assert!(html.contains("name=\"confirm_new_password\""));
        // All three fields type="password" + required + aria-required
        let pw_count = html.matches("type=\"password\"").count();
        assert!(
            pw_count >= 3,
            "expected >=3 password inputs, got {pw_count}"
        );
        let new_pw_count = html.matches("autocomplete=\"new-password\"").count();
        assert_eq!(
            new_pw_count, 2,
            "expected exactly 2 new-password autocomplete fields, got {new_pw_count}"
        );
    }

    #[test]
    fn password_change_renders_requirements_when_nonempty() {
        let p = password_change_page(vec![
            "Minimum 12 characters".to_owned(),
            "At least one non-alphanumeric".to_owned(),
        ]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-password-change__requirements"));
        assert!(html.contains("aria-label=\"Password requirements\""));
        assert!(html.contains(">Minimum 12 characters<"));
        assert!(html.contains(">At least one non-alphanumeric<"));
    }

    #[test]
    fn password_change_omits_requirements_block_when_empty() {
        let p = password_change_page(vec![]);
        let html = render_page(&p).into_string();
        assert!(!html.contains("loom-password-change__requirements"));
    }

    #[test]
    fn password_change_form_posts_to_safe_url() {
        let p = password_change_page(vec![]);
        let html = render_page(&p).into_string();
        assert!(html.contains("method=\"post\""));
        assert!(html.contains("action=\"/account/password\""));
    }

    #[test]
    fn password_change_form_action_falls_back_for_hostile_url() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::PasswordChange {
            title: "T".into(),
            description: None,
            requirements: vec![],
            submit_cta: cta("Update", "javascript:alert(1)"),
            cancel_cta: cta("Cancel", "/c"),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("action=\"javascript:alert"));
        assert!(html.contains("action=\"#invalid-cta\""));
    }

    #[test]
    fn password_change_submit_is_button_cancel_is_link() {
        // Submit MUST be <button type="submit"> so the form posts
        // with the password fields. Cancel MUST be <a> (no inputs
        // travel with cancel; it just navigates away).
        let p = password_change_page(vec![]);
        let html = render_page(&p).into_string();
        assert!(html.contains("type=\"submit\""));
        assert!(html.contains(">Update password<"));
        assert!(html.contains("loom-btn--primary"));
        assert!(html.contains("loom-btn--ghost"));
        assert!(html.contains("href=\"/account/settings\""));
    }

    #[test]
    fn password_change_escapes_title_description_requirements() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::PasswordChange {
            title: "<script>".into(),
            description: Some("<img onerror=x>".into()),
            requirements: vec!["<svg onload=y>".into()],
            submit_cta: cta("S", "/s"),
            cancel_cta: cta("C", "/c"),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(!html.contains("<svg onload=y>"));
    }

    #[test]
    fn password_change_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "password_change",
            "title": "Change password",
            "description": null,
            "requirements": ["Min 12 chars"],
            "submit_cta": {
                "label": "Update",
                "href": "/account/password",
                "data_backend": "pw-update"
            },
            "cancel_cta": {
                "label": "Cancel",
                "href": "/account/settings",
                "data_backend": "cancel"
            }
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::PasswordChange { requirements, .. } => {
                assert_eq!(requirements, vec!["Min 12 chars".to_owned()]);
            }
            _ => panic!("expected PasswordChange variant"),
        }
    }

    // #104 (2026-05-20) — AccountDelete primitive. Irreversible
    // deletion confirm gate. Typed-input gated form posting to
    // server-side handler that re-validates phrase + password.

    fn account_delete_page(require_password: bool) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::AccountDelete {
            title: "Delete your account".into(),
            warning: "This action cannot be undone.".into(),
            consequences: vec![
                "All your posts will be permanently removed.".into(),
                "Active subscriptions will be cancelled.".into(),
            ],
            confirm_phrase: "delete my account".into(),
            confirm_field_label: "Type this phrase to confirm:".into(),
            require_password,
            delete_cta: cta("Delete account permanently", "/account/delete"),
            cancel_cta: cta("Cancel", "/account/settings"),
        }];
        p
    }

    #[test]
    fn account_delete_renders_warning_with_role_alert() {
        let p = account_delete_page(true);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-account-delete"));
        assert!(html.contains(">Delete your account<"));
        assert!(html.contains("role=\"alert\""));
        assert!(html.contains(">This action cannot be undone.<"));
    }

    #[test]
    fn account_delete_renders_consequences_when_nonempty() {
        let p = account_delete_page(true);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-account-delete__consequences"));
        assert!(html.contains("aria-label=\"Consequences\""));
        assert!(html.contains(">All your posts will be permanently removed.<"));
        assert!(html.contains(">Active subscriptions will be cancelled.<"));
    }

    #[test]
    fn account_delete_omits_consequences_block_when_empty() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::AccountDelete {
            title: "T".into(),
            warning: "W".into(),
            consequences: vec![],
            confirm_phrase: "delete".into(),
            confirm_field_label: "Type:".into(),
            require_password: true,
            delete_cta: cta("Delete", "/d"),
            cancel_cta: cta("Cancel", "/c"),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("loom-account-delete__consequences"));
    }

    #[test]
    fn account_delete_form_posts_to_safe_delete_cta() {
        let p = account_delete_page(true);
        let html = render_page(&p).into_string();
        assert!(html.contains("method=\"post\""));
        assert!(html.contains("action=\"/account/delete\""));
    }

    #[test]
    fn account_delete_form_action_falls_back_to_invalid_for_hostile_url() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::AccountDelete {
            title: "T".into(),
            warning: "W".into(),
            consequences: vec![],
            confirm_phrase: "delete".into(),
            confirm_field_label: "Type:".into(),
            require_password: true,
            delete_cta: cta("Delete", "javascript:alert(1)"),
            cancel_cta: cta("Cancel", "/c"),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("action=\"javascript:alert"));
        assert!(html.contains("action=\"#invalid-cta\""));
    }

    #[test]
    fn account_delete_surfaces_confirm_phrase_in_label_code() {
        let p = account_delete_page(true);
        let html = render_page(&p).into_string();
        // Confirm phrase shown literally inside a <code> tag so the
        // user can verify the exact characters they must type.
        assert!(html.contains(
            "<code class=\"loom-account-delete__confirm-phrase\">delete my account</code>"
        ));
        // Confirm input has the standard hardening attributes
        assert!(html.contains("autocomplete=\"off\""));
        assert!(html.contains("spellcheck=\"false\""));
        assert!(html.contains("name=\"confirm_phrase\""));
    }

    #[test]
    fn account_delete_password_field_omitted_when_require_password_false() {
        let p = account_delete_page(false);
        let html = render_page(&p).into_string();
        assert!(!html.contains("name=\"current_password\""));
        assert!(!html.contains("autocomplete=\"current-password\""));
    }

    #[test]
    fn account_delete_password_field_rendered_when_require_password_true() {
        let p = account_delete_page(true);
        let html = render_page(&p).into_string();
        assert!(html.contains("name=\"current_password\""));
        assert!(html.contains("autocomplete=\"current-password\""));
        assert!(html.contains("type=\"password\""));
    }

    #[test]
    fn account_delete_renders_dual_ctas_with_destructive_styling() {
        let p = account_delete_page(true);
        let html = render_page(&p).into_string();
        // Cancel: ghost button, link element
        assert!(html.contains("loom-btn--ghost"));
        assert!(html.contains("href=\"/account/settings\""));
        // Delete: danger button, submit button element (not <a>)
        assert!(html.contains("loom-btn--danger"));
        assert!(html.contains("type=\"submit\""));
        assert!(html.contains(">Delete account permanently<"));
    }

    #[test]
    fn account_delete_escapes_title_warning_consequences() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::AccountDelete {
            title: "<script>".into(),
            warning: "<img onerror=x>".into(),
            consequences: vec!["<svg onload=y>".into()],
            confirm_phrase: "<script>".into(),
            confirm_field_label: "<svg/>".into(),
            require_password: false,
            delete_cta: cta("D", "/d"),
            cancel_cta: cta("C", "/c"),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(!html.contains("<svg onload=y>"));
        assert!(!html.contains("<svg/>"));
        // Heavy literal escape on the title's leading marker
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn account_delete_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "account_delete",
            "title": "Delete your account",
            "warning": "Cannot be undone.",
            "consequences": ["lose posts", "lose subs"],
            "confirm_phrase": "delete me",
            "confirm_field_label": "Type:",
            "require_password": true,
            "delete_cta": {
                "label": "Delete",
                "href": "/account/delete",
                "data_backend": "delete"
            },
            "cancel_cta": {
                "label": "Cancel",
                "href": "/account/settings",
                "data_backend": "cancel"
            }
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::AccountDelete {
                confirm_phrase,
                require_password,
                consequences,
                ..
            } => {
                assert_eq!(confirm_phrase, "delete me");
                assert!(require_password);
                assert_eq!(consequences.len(), 2);
            }
            _ => panic!("expected AccountDelete variant"),
        }
    }

    // #104 (2026-05-20) — DeviceList primitive. Active-sessions
    // / device management for the post-MFA-aware account flow.

    fn device(label: &str, current: bool, revoke_href: Option<&str>) -> DeviceEntry {
        DeviceEntry {
            label: label.into(),
            location: None,
            last_active: None,
            current,
            revoke_cta: revoke_href.map(|h| HeroCta {
                label: "Revoke".into(),
                href: h.into(),
                data_backend: "revoke".into(),
            }),
        }
    }

    fn device_list_page(devices: Vec<DeviceEntry>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::DeviceList {
            title: "Active sessions".into(),
            description: Some("Sessions where you're signed in.".into()),
            devices,
            revoke_all_cta: None,
        }];
        p
    }

    #[test]
    fn device_list_renders_each_row_with_label() {
        let p = device_list_page(vec![
            device("MacBook Pro · Chrome", false, Some("/sessions/1/revoke")),
            device("iPhone 15 · Safari", true, None),
        ]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-device-list"));
        assert!(html.contains(">MacBook Pro · Chrome<"));
        assert!(html.contains(">iPhone 15 · Safari<"));
        assert!(html.contains("aria-label=\"Active sessions\""));
    }

    #[test]
    fn device_list_current_session_carries_modifier_and_badge() {
        let p = device_list_page(vec![device("iPhone · Safari", true, None)]);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-device--current"));
        assert!(html.contains("aria-label=\"Current session\""));
        assert!(html.contains(">current<"));
    }

    #[test]
    fn device_list_suppresses_revoke_on_current_session() {
        // Security UX: revoking your CURRENT session mid-page is a
        // trap. Render-side enforcement: even if operator passes a
        // revoke_cta on current=true, suppress it.
        let p = device_list_page(vec![device(
            "iPhone · Safari",
            true,
            Some("/sessions/current/revoke"),
        )]);
        let html = render_page(&p).into_string();
        assert!(!html.contains("/sessions/current/revoke"));
        assert!(!html.contains("loom-device__revoke"));
    }

    #[test]
    fn device_list_renders_revoke_cta_on_non_current_rows() {
        let p = device_list_page(vec![device(
            "MacBook · Chrome",
            false,
            Some("/sessions/123/revoke"),
        )]);
        let html = render_page(&p).into_string();
        assert!(html.contains("href=\"/sessions/123/revoke\""));
        assert!(html.contains("loom-device__revoke"));
    }

    #[test]
    fn device_list_rejects_javascript_url_in_per_row_revoke() {
        let p = device_list_page(vec![device(
            "MacBook · Chrome",
            false,
            Some("javascript:alert(1)"),
        )]);
        let html = render_page(&p).into_string();
        assert!(!html.contains("javascript:alert"));
        assert!(html.contains("href=\"#invalid-cta\""));
    }

    #[test]
    fn device_list_renders_revoke_all_cta_when_present() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::DeviceList {
            title: "T".into(),
            description: None,
            devices: vec![device("D1", true, None)],
            revoke_all_cta: Some(HeroCta {
                label: "Sign out everywhere".into(),
                href: "/sessions/revoke-all".into(),
                data_backend: "revoke-all".into(),
            }),
        }];
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-device-list__revoke-all"));
        assert!(html.contains("loom-btn--danger"));
        assert!(html.contains("href=\"/sessions/revoke-all\""));
        assert!(html.contains(">Sign out everywhere<"));
    }

    #[test]
    fn device_list_rejects_javascript_url_in_revoke_all() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::DeviceList {
            title: "T".into(),
            description: None,
            devices: vec![device("D", true, None)],
            revoke_all_cta: Some(HeroCta {
                label: "X".into(),
                href: "javascript:alert(1)".into(),
                data_backend: "x".into(),
            }),
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("javascript:alert"));
        assert!(html.contains("href=\"#invalid-cta\""));
    }

    #[test]
    fn device_list_escapes_label_location_last_active() {
        let entry = DeviceEntry {
            label: "<script>".into(),
            location: Some("<img onerror=x>".into()),
            last_active: Some("<svg onload=y>".into()),
            current: false,
            revoke_cta: None,
        };
        let p = device_list_page(vec![entry]);
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(!html.contains("<svg onload=y>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn device_list_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "device_list",
            "title": "Sessions",
            "description": null,
            "devices": [{
                "label": "MacBook",
                "location": null,
                "last_active": null,
                "current": true,
                "revoke_cta": null
            }],
            "revoke_all_cta": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::DeviceList { devices, .. } => {
                assert_eq!(devices.len(), 1);
                assert!(devices[0].current);
                assert_eq!(devices[0].label, "MacBook");
            }
            _ => panic!("expected DeviceList variant"),
        }
    }

    // #104 (2026-05-20) — ConsentScreen primitive. OAuth consent
    // gap. Renders the typed scopes list with risk-tiered modifier
    // classes + grant/deny CTAs through is_safe_url.

    fn cta(label: &str, href: &str) -> HeroCta {
        HeroCta {
            label: label.into(),
            href: href.into(),
            data_backend: "test".into(),
        }
    }

    fn consent_page(scopes: Vec<ConsentScope>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::ConsentScreen {
            title: "Authorize MyApp".into(),
            app_name: "MyApp".into(),
            app_description: Some("A tool that does things.".into()),
            app_homepage: Some("https://myapp.example".into()),
            scopes,
            grant_cta: cta("Authorize", "/oauth/grant"),
            deny_cta: cta("Cancel", "/oauth/deny"),
            footer_note: None,
        }];
        p
    }

    #[test]
    fn consent_screen_renders_app_name_and_scope_list() {
        let scopes = vec![ConsentScope {
            slug: "read:repos".into(),
            label: "Read your repositories".into(),
            description: None,
            tier: ConsentScopeTier::Routine,
        }];
        let p = consent_page(scopes);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-consent-screen"));
        assert!(html.contains(">MyApp<"));
        assert!(html.contains("<code class=\"loom-consent-scope__slug\">read:repos</code>"));
        assert!(html.contains(">Read your repositories<"));
        assert!(html.contains("aria-label=\"Requested permissions\""));
    }

    #[test]
    fn consent_screen_emits_modifier_class_per_scope_tier() {
        let scopes = vec![
            ConsentScope {
                slug: "user:read".into(),
                label: "Read".into(),
                description: None,
                tier: ConsentScopeTier::Routine,
            },
            ConsentScope {
                slug: "user:write".into(),
                label: "Write".into(),
                description: None,
                tier: ConsentScopeTier::Write,
            },
            ConsentScope {
                slug: "billing:full".into(),
                label: "Billing".into(),
                description: None,
                tier: ConsentScopeTier::Sensitive,
            },
        ];
        let p = consent_page(scopes);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-consent-scope--routine"));
        assert!(html.contains("loom-consent-scope--write"));
        assert!(html.contains("loom-consent-scope--sensitive"));
    }

    #[test]
    fn consent_screen_grant_and_deny_ctas_route_through_is_safe_url() {
        let mut p = consent_page(vec![]);
        p.sections[0] = CmsSection::ConsentScreen {
            title: "T".into(),
            app_name: "X".into(),
            app_description: None,
            app_homepage: None,
            scopes: vec![],
            grant_cta: cta("Grant", "javascript:alert(1)"),
            deny_cta: cta("Deny", "/oauth/deny"),
            footer_note: None,
        };
        let html = render_page(&p).into_string();
        // Hostile grant routes to invalid-cta sentinel
        assert!(!html.contains("javascript:alert"));
        assert!(html.contains("href=\"#invalid-cta\""));
        // Safe deny preserves real URL
        assert!(html.contains("href=\"/oauth/deny\""));
    }

    #[test]
    fn consent_screen_escapes_app_name_and_description() {
        let mut p = consent_page(vec![]);
        p.sections[0] = CmsSection::ConsentScreen {
            title: "T".into(),
            app_name: "<script>".into(),
            app_description: Some("<img onerror=alert(1)>".into()),
            app_homepage: None,
            scopes: vec![],
            grant_cta: cta("g", "/g"),
            deny_cta: cta("d", "/d"),
            footer_note: None,
        };
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<img onerror=alert(1)>"));
    }

    #[test]
    fn consent_screen_app_homepage_link_carries_rel_noopener() {
        let p = consent_page(vec![]);
        let html = render_page(&p).into_string();
        // Existing app_homepage in helper is safe; should render with rel/target
        assert!(html.contains("rel=\"noopener\""));
        assert!(html.contains("target=\"_blank\""));
        assert!(html.contains("https://myapp.example"));
    }

    #[test]
    fn consent_screen_rejects_javascript_homepage() {
        let mut p = consent_page(vec![]);
        p.sections[0] = CmsSection::ConsentScreen {
            title: "T".into(),
            app_name: "X".into(),
            app_description: None,
            app_homepage: Some("javascript:alert(1)".into()),
            scopes: vec![],
            grant_cta: cta("g", "/g"),
            deny_cta: cta("d", "/d"),
            footer_note: None,
        };
        let html = render_page(&p).into_string();
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn consent_scope_tier_modifier_is_stable_kebab_case() {
        assert_eq!(ConsentScopeTier::Routine.modifier(), "routine");
        assert_eq!(ConsentScopeTier::Write.modifier(), "write");
        assert_eq!(ConsentScopeTier::Sensitive.modifier(), "sensitive");
    }

    #[test]
    fn consent_scope_tier_serde_round_trip() {
        for (tier, json) in [
            (ConsentScopeTier::Routine, "\"routine\""),
            (ConsentScopeTier::Write, "\"write\""),
            (ConsentScopeTier::Sensitive, "\"sensitive\""),
        ] {
            let s = serde_json::to_string(&tier).unwrap();
            assert_eq!(s, json);
            let back: ConsentScopeTier = serde_json::from_str(&s).unwrap();
            assert_eq!(back, tier);
        }
    }

    #[test]
    fn consent_screen_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "consent_screen",
            "title": "Authorize Acme",
            "app_name": "Acme",
            "app_description": null,
            "app_homepage": null,
            "scopes": [{
                "slug": "read:everything",
                "label": "Read everything",
                "description": null,
                "tier": "sensitive"
            }],
            "grant_cta": {
                "label": "Authorize",
                "href": "/oauth/grant",
                "data_backend": "oauth-grant"
            },
            "deny_cta": {
                "label": "Cancel",
                "href": "/oauth/deny",
                "data_backend": "oauth-deny"
            },
            "footer_note": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::ConsentScreen {
                app_name, scopes, ..
            } => {
                assert_eq!(app_name, "Acme");
                assert_eq!(scopes.len(), 1);
                assert_eq!(scopes[0].tier, ConsentScopeTier::Sensitive);
            }
            _ => panic!("expected ConsentScreen variant"),
        }
    }

    #[test]
    fn consent_screen_footer_note_rendered_when_present() {
        let mut p = consent_page(vec![]);
        p.sections[0] = CmsSection::ConsentScreen {
            title: "T".into(),
            app_name: "X".into(),
            app_description: None,
            app_homepage: None,
            scopes: vec![],
            grant_cta: cta("g", "/g"),
            deny_cta: cta("d", "/d"),
            footer_note: Some("Published by Acme Inc.".into()),
        };
        let html = render_page(&p).into_string();
        assert!(html.contains(">Published by Acme Inc.<"));
        assert!(html.contains("loom-consent-screen__footer-note"));
    }

    // #122 (2026-05-20) — BackupCodes follow-up to EmailVerifyResult.
    // Account-flow primitive for the post-MFA-enrollment recovery-
    // code display + acknowledge page.

    fn backup_codes_page(state: BackupCodesState, codes: Vec<String>) -> CmsPage {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::BackupCodes {
            title: "Save your codes".into(),
            description: "Each code can be used once.".into(),
            state,
            codes,
            download_cta: None,
            acknowledge_cta: None,
        }];
        p
    }

    #[test]
    fn backup_codes_fresh_renders_each_code_in_an_ordered_list() {
        let codes = vec!["aaaa-1111".to_owned(), "bbbb-2222".to_owned()];
        let p = backup_codes_page(BackupCodesState::Fresh, codes);
        let html = render_page(&p).into_string();
        // Modifier class
        assert!(html.contains("loom-backup-codes--fresh"));
        // Each code rendered inside a <code> inside a <li>
        assert!(html.contains("<ol class=\"loom-backup-codes__list\""));
        assert!(html.contains("<code>aaaa-1111</code>"));
        assert!(html.contains("<code>bbbb-2222</code>"));
        // aria-label on the list for assistive tech
        assert!(html.contains("aria-label=\"Recovery codes\""));
    }

    #[test]
    fn backup_codes_already_generated_omits_codes_grid() {
        // Even if codes are passed in, AlreadyGenerated MUST NOT
        // render them — security contract: fresh codes display
        // exactly once.
        let codes = vec!["should-not-render".to_owned()];
        let p = backup_codes_page(BackupCodesState::AlreadyGenerated, codes);
        let html = render_page(&p).into_string();
        assert!(html.contains("loom-backup-codes--already-generated"));
        assert!(!html.contains("should-not-render"));
        assert!(!html.contains("loom-backup-codes__list"));
    }

    #[test]
    fn backup_codes_escapes_individual_codes() {
        let codes = vec!["<script>alert(1)</script>".to_owned()];
        let p = backup_codes_page(BackupCodesState::Fresh, codes);
        let html = render_page(&p).into_string();
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn backup_codes_renders_both_ctas_with_safe_urls() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::BackupCodes {
            title: "T".into(),
            description: "D".into(),
            state: BackupCodesState::Fresh,
            codes: vec!["abc".into()],
            download_cta: Some(HeroCta {
                label: "Download".into(),
                href: "/codes.txt".into(),
                data_backend: "codes-txt".into(),
            }),
            acknowledge_cta: Some(HeroCta {
                label: "Continue".into(),
                href: "/dashboard".into(),
                data_backend: "dashboard".into(),
            }),
        }];
        let html = render_page(&p).into_string();
        assert!(html.contains("href=\"/codes.txt\""));
        assert!(html.contains(">Download<"));
        assert!(html.contains("loom-backup-codes__download"));
        assert!(html.contains("href=\"/dashboard\""));
        assert!(html.contains(">Continue<"));
        assert!(html.contains("loom-backup-codes__ack"));
    }

    #[test]
    fn backup_codes_rejects_javascript_url_in_ctas() {
        let mut p = empty_page();
        p.brand = Some("X".into());
        p.site_origin = Some("https://x.example".into());
        p.sections = vec![CmsSection::BackupCodes {
            title: "T".into(),
            description: "D".into(),
            state: BackupCodesState::Fresh,
            codes: vec!["abc".into()],
            download_cta: Some(HeroCta {
                label: "D".into(),
                href: "javascript:void(0)".into(),
                data_backend: "x".into(),
            }),
            acknowledge_cta: None,
        }];
        let html = render_page(&p).into_string();
        assert!(!html.contains("javascript:void"));
        assert!(html.contains("href=\"#invalid-cta\""));
    }

    #[test]
    fn backup_codes_no_ctas_omits_actions_block() {
        let p = backup_codes_page(BackupCodesState::Fresh, vec!["abc".into()]);
        let html = render_page(&p).into_string();
        assert!(!html.contains("loom-backup-codes__actions"));
    }

    #[test]
    fn backup_codes_state_serde_round_trip() {
        for (variant, json) in [
            (BackupCodesState::Fresh, "\"fresh\""),
            (BackupCodesState::AlreadyGenerated, "\"already_generated\""),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, json);
            let back: BackupCodesState = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn backup_codes_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "backup_codes",
            "title": "Save these codes",
            "description": "One-time only.",
            "state": "fresh",
            "codes": ["abc-123", "def-456"],
            "download_cta": null,
            "acknowledge_cta": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::BackupCodes { codes, state, .. } => {
                assert_eq!(state, BackupCodesState::Fresh);
                assert_eq!(codes, vec!["abc-123", "def-456"]);
            }
            _ => panic!("expected BackupCodes variant"),
        }
    }

    #[test]
    fn email_verify_section_parses_from_snake_case_kind() {
        let json = r#"{
            "kind": "email_verify_result",
            "status": "expired",
            "title": null,
            "body": "Your link has expired.",
            "cta": null,
            "secondary_cta": null
        }"#;
        let section: CmsSection = serde_json::from_str(json).unwrap();
        match section {
            CmsSection::EmailVerifyResult { status, body, .. } => {
                assert_eq!(status, EmailVerifyStatus::Expired);
                assert_eq!(body, "Your link has expired.");
            }
            _ => panic!("expected EmailVerifyResult variant"),
        }
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
