//! `loom-variables` — per-tenant variable substitution layer.
//!
//! Tenants ship three sibling files under their repo root:
//!
//! - `variables.json`   — `{{ VAR }}` placeholders → string values
//! - `palette.json`     — `{{ PALETTE.fg }}` placeholders → palette
//!                        entries (nested via `.` access)
//! - `assets-map.json`  — `@asset-slug` references → resolved URL
//!
//! The substrate's CMS authoring layer carries placeholders rather
//! than baking in tenant-specific strings; at render time this
//! crate's [`substitute`] function projects each placeholder
//! through the loaded tables. Missing variables are intentionally
//! preserved verbatim (the placeholder appears in output) so a
//! visual builder can flag unbound references without breaking the
//! page.
//!
//! Per paul 2026-05-21: "you should be using variables and such to
//! replace generic place holders for when you use the content for
//! a specific site... it might pull the variables from a json".

#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Loaded per-tenant substitution tables. Three independent maps;
/// any may be empty for a tenant that doesn't need that layer.
///
/// Construct from JSON via [`serde_json::from_str`] or hand-build
/// in tests with [`TenantVariables::new`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantVariables {
    /// `{{ KEY }}` → value. Keys are matched after trimming
    /// whitespace inside the braces, so `{{ KEY }}` and `{{KEY}}`
    /// resolve identically.
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    /// `{{ PALETTE.fg }}` → palette-entry value. Flat map; nested
    /// access uses `.` in the key.
    #[serde(default)]
    pub palette: BTreeMap<String, String>,
    /// `@asset-slug` → resolved URL. Slugs are matched after
    /// trimming the leading `@`; allowed slug characters are
    /// `[a-zA-Z0-9_-]` so `@hero-bg` and `@hero-bg.` resolve
    /// identically (trailing punctuation is left in place).
    #[serde(default)]
    pub assets: BTreeMap<String, String>,
}

impl TenantVariables {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct from parts. Convenience for tests.
    #[must_use]
    pub fn from_parts(
        variables: BTreeMap<String, String>,
        palette: BTreeMap<String, String>,
        assets: BTreeMap<String, String>,
    ) -> Self {
        Self {
            variables,
            palette,
            assets,
        }
    }

    /// Returns true if every table is empty — no substitution
    /// pass is needed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty() && self.palette.is_empty() && self.assets.is_empty()
    }
}

/// Apply per-tenant substitution to `text` and return a new
/// `String`. The pass is non-allocating in the no-placeholder
/// fast path (returns `text.to_owned()` only after detecting
/// `{{` or `@` in the input).
///
/// ## Placeholder syntax
///
/// - `{{ KEY }}` — looked up in `vars.variables`; a key of
///   `PALETTE.<sub>` falls through to `vars.palette`.
/// - `@asset-slug` — looked up in `vars.assets`. Slug runs from
///   the `@` through the next non-`[a-zA-Z0-9_-]` character.
///
/// ## Missing references
///
/// Missing variables are preserved verbatim so a visual builder
/// (or PR-time linter) can flag unbound references without
/// breaking the rendered page. The audit layer reports an
/// unbound finding; this function never substitutes a placeholder
/// it can't resolve.
#[must_use]
pub fn substitute(text: &str, vars: &TenantVariables) -> String {
    if vars.is_empty() || (!text.contains("{{") && !text.contains('@')) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // {{ KEY }} placeholder
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = text[i + 2..].find("}}") {
                let inner = &text[i + 2..i + 2 + end];
                let key = inner.trim();
                let resolved = if let Some(rest) = key.strip_prefix("PALETTE.") {
                    vars.palette.get(rest)
                } else {
                    vars.variables.get(key)
                };
                if let Some(val) = resolved {
                    out.push_str(val);
                    i += 2 + end + 2;
                    continue;
                }
                // Unresolved — preserve verbatim.
                out.push_str(&text[i..i + 2 + end + 2]);
                i += 2 + end + 2;
                continue;
            }
        }
        // @asset-slug reference
        if bytes[i] == b'@'
            && i + 1 < bytes.len()
            && (bytes[i + 1].is_ascii_alphanumeric() || bytes[i + 1] == b'_' || bytes[i + 1] == b'-')
        {
            let mut j = i + 1;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-')
            {
                j += 1;
            }
            let slug = &text[i + 1..j];
            if let Some(url) = vars.assets.get(slug) {
                out.push_str(url);
                i = j;
                continue;
            }
        }
        // Plain byte — push as-is.
        // SAFETY: `i` is at a UTF-8 boundary because we only
        // advance by full UTF-8 sequences below.
        let ch_end = next_char_boundary(text, i);
        out.push_str(&text[i..ch_end]);
        i = ch_end;
    }
    out
}

/// Find the byte index of the next UTF-8 char boundary after `i`.
/// Caller guarantees `i < text.len()`.
fn next_char_boundary(text: &str, i: usize) -> usize {
    let bytes = text.as_bytes();
    let mut j = i + 1;
    while j < bytes.len() && !text.is_char_boundary(j) {
        j += 1;
    }
    j
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> TenantVariables {
        let mut v = BTreeMap::new();
        v.insert("BRAND_NAME".into(), "Acme".into());
        v.insert("YEAR".into(), "2026".into());
        let mut p = BTreeMap::new();
        p.insert("fg".into(), "#0a0a0a".into());
        p.insert("accent".into(), "#3a7afe".into());
        let mut a = BTreeMap::new();
        a.insert("hero-bg".into(), "/static/hero.webp".into());
        a.insert("logo".into(), "/static/logo.svg".into());
        TenantVariables::from_parts(v, p, a)
    }

    #[test]
    fn substitutes_simple_variable() {
        let out = substitute("Welcome to {{ BRAND_NAME }}", &fixture());
        assert_eq!(out, "Welcome to Acme");
    }

    #[test]
    fn substitutes_without_whitespace() {
        let out = substitute("Year {{YEAR}}.", &fixture());
        assert_eq!(out, "Year 2026.");
    }

    #[test]
    fn substitutes_palette_via_dotted_access() {
        let out = substitute("color: {{ PALETTE.accent }};", &fixture());
        assert_eq!(out, "color: #3a7afe;");
    }

    #[test]
    fn substitutes_asset_slug() {
        let out = substitute("<img src=\"@hero-bg\">", &fixture());
        assert_eq!(out, "<img src=\"/static/hero.webp\">");
    }

    #[test]
    fn asset_slug_stops_at_punctuation() {
        let out = substitute("see @logo.", &fixture());
        assert_eq!(out, "see /static/logo.svg.");
    }

    #[test]
    fn missing_variable_preserved_verbatim() {
        let out = substitute("hi {{ UNBOUND }} there", &fixture());
        assert_eq!(out, "hi {{ UNBOUND }} there");
    }

    #[test]
    fn missing_asset_preserved_verbatim() {
        let out = substitute("see @ghost end", &fixture());
        assert_eq!(out, "see @ghost end");
    }

    #[test]
    fn empty_table_returns_unchanged() {
        let out = substitute("{{ X }} and @y", &TenantVariables::default());
        assert_eq!(out, "{{ X }} and @y");
    }

    #[test]
    fn fast_path_no_braces_no_at() {
        let out = substitute("nothing to do here", &fixture());
        assert_eq!(out, "nothing to do here");
    }

    #[test]
    fn unicode_passthrough() {
        let out = substitute("café — {{ BRAND_NAME }} → ✓", &fixture());
        assert_eq!(out, "café — Acme → ✓");
    }

    #[test]
    fn parses_from_json() {
        let json = r##"{
            "variables": { "FOO": "bar" },
            "palette": { "accent": "#fff" },
            "assets": { "logo": "/l.svg" }
        }"##;
        let v: TenantVariables = serde_json::from_str(json).expect("parses");
        assert_eq!(v.variables.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(v.palette.get("accent").map(String::as_str), Some("#fff"));
        assert_eq!(v.assets.get("logo").map(String::as_str), Some("/l.svg"));
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        // deny_unknown_fields catches typos in tenant config.
        let json = r##"{ "variabls": { "FOO": "bar" } }"##;
        let result: Result<TenantVariables, _> = serde_json::from_str(json);
        assert!(result.is_err(), "typo should be rejected");
    }

    #[test]
    fn empty_braces_are_not_substituted() {
        let out = substitute("a {{ }} b", &fixture());
        // Trim of "" gives empty key, no match — preserved.
        assert_eq!(out, "a {{ }} b");
    }
}
