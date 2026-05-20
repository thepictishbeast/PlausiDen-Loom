//! `slots` — typed slot composition for primitives.
//!
//! Closes task #105 (preamble #214). Lets one primitive accept
//! another primitive as a typed child rather than as an opaque
//! `Markup` blob. The substrate can audit / count / introspect the
//! child without parsing rendered HTML.
//!
//! Motivation. `HeroEditorial.decoration: Option<&Markup>` shipped
//! the slot composition pattern in spirit: callers compose a typed
//! primitive (KvPair / PullQuote / CodeShell / Picture) and pass
//! `.render()` into the slot. But the slot type is `Markup` — an
//! opaque rendered-HTML blob. The substrate can't tell from outside
//! the primitive whether the decoration is a KvPair grid or a
//! CodeShell transcript without parsing the HTML back.
//!
//! `EditorialSlot` is the typed alternative. Each variant wraps an
//! editorial-composition primitive that operators commonly drop
//! into a decoration slot:
//!
//! * `EditorialSlot::KvPair(KvPairCard)`
//! * `EditorialSlot::PullQuote(PullQuote)`
//! * `EditorialSlot::CodeShell(CodeShell)`
//! * `EditorialSlot::Picture(Picture)`
//! * `EditorialSlot::Raw(Markup)` — fallback for arbitrary callers
//!
//! The substrate can introspect `EditorialSlot::Raw(_)` count vs.
//! `EditorialSlot::KvPair(_)` count and flag pages where decoration
//! slots have been escape-hatched too often.
//!
//! Render dispatch: `EditorialSlot::render()` returns the same
//! `Markup` the underlying primitive would have produced. Existing
//! callers that consume `Markup` (e.g. `HeroEditorial.decoration`)
//! can wrap an `EditorialSlot` via `slot.render()` and pass through
//! without changing their slot field's type.
//!
//! AVP-2 INVARIANTS
//! ----------------
//! * `unsafe_code = "deny"` (inherited from crate-level lint).
//! * `#[non_exhaustive]` on the enum so adding a variant is non-breaking.
//! * Render is pure; no I/O.

use crate::card::KvPairCard;
use crate::code_shell::CodeShell;
use crate::picture::Picture;
use crate::pull_quote::PullQuote;
use maud::{Markup, PreEscaped};

/// Typed slot for editorial composition. Variants below are the
/// primitives most commonly composed into decoration / aside /
/// inline-mark positions across PlausiDen editorial pages.
///
/// The enum is `#[non_exhaustive]` so new editorial-composition
/// primitives can be added without breaking external callers.
#[non_exhaustive]
pub enum EditorialSlot<'a> {
    /// A `KvPairCard` dense info panel — the substrate's data-
    /// dispatch composition.
    KvPair(KvPairCard<'a>),
    /// A `PullQuote` inline editorial mark — left-border rule, no
    /// decorative quote-mark glyph.
    PullQuote(PullQuote<'a>),
    /// A `CodeShell` terminal transcript — typed-line semantic
    /// shell composition.
    CodeShell(CodeShell<'a>),
    /// A `Picture` responsive image with AVIF + WebP + JPEG
    /// fallback chain.
    Picture(Picture<'a>),
    /// Raw `Markup` escape hatch. Use sparingly — when this variant
    /// appears in a page's slot composition, the substrate's
    /// editorial-uplift audit counts it against the page's "typed
    /// slot ratio."
    Raw(Markup),
}

impl<'a> EditorialSlot<'a> {
    /// Render the slot's underlying primitive to `Markup`.
    ///
    /// Consumers that hold a slot field typed `Option<EditorialSlot>`
    /// can wrap rendering with `.as_ref().map(|s| s.render())` and
    /// drop the result into their HTML composition. Consumers that
    /// hold the legacy `Option<&Markup>` slot field can wrap via
    /// `slot.render()` first.
    #[must_use]
    pub fn render(&self) -> Markup {
        match self {
            Self::KvPair(c) => c.render(),
            Self::PullQuote(q) => q.render(),
            Self::CodeShell(s) => s.render(),
            Self::Picture(p) => p.render(),
            Self::Raw(m) => PreEscaped(m.0.clone()),
        }
    }

    /// Slot kind name — useful for telemetry / audit reporting.
    /// The string is stable; consumers can rely on it for indexing.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::KvPair(_) => "kv-pair",
            Self::PullQuote(_) => "pull-quote",
            Self::CodeShell(_) => "code-shell",
            Self::Picture(_) => "picture",
            Self::Raw(_) => "raw",
        }
    }

    /// `true` iff this slot is the typed-editorial-primitive kind
    /// (not the Raw escape hatch). The substrate's editorial-uplift
    /// audit uses this to compute the "typed slot ratio" per page.
    #[must_use]
    pub const fn is_typed_primitive(&self) -> bool {
        !matches!(self, Self::Raw(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{KvPairDensity, KvPairTone};
    use crate::code_shell::{CodeShellChrome, CodeShellTone};
    use crate::picture::{PictureFit, PictureLoading, PicturePriority};
    use crate::pull_quote::{PullQuoteEmphasis, PullQuoteTone};
    use maud::html;

    fn kv_slot<'a>() -> EditorialSlot<'a> {
        EditorialSlot::KvPair(KvPairCard {
            label: "JURISDICTION",
            value: "Federal+47",
            source: Some("extraterritorial"),
            density: KvPairDensity::Comfortable,
            tone: KvPairTone::Slate,
        })
    }

    fn pq_slot<'a>() -> EditorialSlot<'a> {
        EditorialSlot::PullQuote(PullQuote {
            body: "Substrate carries the page.",
            attribution: Some("paul, 2026-05-20"),
            cite_url: None,
            emphasis: PullQuoteEmphasis::Display,
            tone: PullQuoteTone::Slate,
        })
    }

    fn cs_slot<'a>() -> EditorialSlot<'a> {
        EditorialSlot::CodeShell(CodeShell {
            title: Some("forge build"),
            prompt: None,
            lines: &[],
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Header,
        })
    }

    fn pic_slot<'a>() -> EditorialSlot<'a> {
        EditorialSlot::Picture(Picture {
            src_stem: "hero/dispatch",
            alt: "Editorial dispatch image",
            width: 1280,
            height: 720,
            loading: PictureLoading::Lazy,
            priority: PicturePriority::Auto,
            fit: PictureFit::Cover,
        })
    }

    #[test]
    fn kv_pair_slot_kind_is_kv_pair() {
        assert_eq!(kv_slot().kind(), "kv-pair");
    }

    #[test]
    fn pull_quote_slot_kind_is_pull_quote() {
        assert_eq!(pq_slot().kind(), "pull-quote");
    }

    #[test]
    fn code_shell_slot_kind_is_code_shell() {
        assert_eq!(cs_slot().kind(), "code-shell");
    }

    #[test]
    fn picture_slot_kind_is_picture() {
        assert_eq!(pic_slot().kind(), "picture");
    }

    #[test]
    fn raw_slot_kind_is_raw() {
        let slot = EditorialSlot::Raw(html! { div { "x" } });
        assert_eq!(slot.kind(), "raw");
    }

    #[test]
    fn typed_slots_report_is_typed_primitive_true() {
        assert!(kv_slot().is_typed_primitive());
        assert!(pq_slot().is_typed_primitive());
        assert!(cs_slot().is_typed_primitive());
        assert!(pic_slot().is_typed_primitive());
    }

    #[test]
    fn raw_slot_reports_is_typed_primitive_false() {
        let slot = EditorialSlot::Raw(html! { p { "raw" } });
        assert!(!slot.is_typed_primitive());
    }

    #[test]
    fn kv_pair_slot_renders_underlying_card() {
        let html = kv_slot().render().into_string();
        // KvPairCard emits monospace uppercase label.
        assert!(html.contains(">JURISDICTION<"));
        assert!(html.contains(">Federal+47<"));
        assert!(html.contains("font-mono"));
    }

    #[test]
    fn pull_quote_slot_renders_underlying_blockquote() {
        let html = pq_slot().render().into_string();
        assert!(html.contains("<blockquote"));
        assert!(html.contains(">Substrate carries the page.<"));
        assert!(html.contains(">— paul, 2026-05-20<"));
    }

    #[test]
    fn code_shell_slot_renders_underlying_pre_code() {
        let html = cs_slot().render().into_string();
        assert!(html.contains("<pre"));
        assert!(html.contains("<code"));
        assert!(html.contains(">forge build<"));
    }

    #[test]
    fn picture_slot_renders_underlying_picture_element() {
        let html = pic_slot().render().into_string();
        assert!(html.contains("<picture>"));
        assert!(html.contains(r#"alt="Editorial dispatch image""#));
        assert!(html.contains(r#"width="1280""#));
    }

    #[test]
    fn raw_slot_renders_pre_escaped_markup() {
        let inner = html! { aside { "raw-aside-content" } };
        let slot = EditorialSlot::Raw(inner);
        let html = slot.render().into_string();
        assert!(html.contains("<aside>raw-aside-content</aside>"));
    }

    #[test]
    fn kind_strings_are_stable_kebab_case() {
        // Stable wire / telemetry contract — these strings get
        // indexed by the editorial-uplift audit. Renaming requires
        // a Cat-3 migration.
        let stable_kinds = ["kv-pair", "pull-quote", "code-shell", "picture", "raw"];
        for k in stable_kinds {
            assert!(
                k.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "slot kind not kebab-case: {k}"
            );
        }
    }
}
