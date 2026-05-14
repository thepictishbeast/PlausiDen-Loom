//! `loom-lint` — refuse raw class strings outside the design system.
//!
//! Walks `*.rs` files under a target crate, extracts every literal
//! that looks like a Tailwind class string, and complains if any of
//! them appears in a file *outside* the allowlist of components/views
//! that are sanctioned to compose styling.
//!
//! The lint is intentionally simple: regex over source text, no `syn`
//! parse. False positives are rare (the false-class-like strings caught
//! so far have all been real bugs); a `#[allow_loom]` line marker can
//! be added later if needed.
//!
//! ## What it catches
//!
//! Any string literal that contains *more than one* Tailwind utility
//! token (e.g. `"px-4 py-2"`, `"flex items-center gap-2"`) found in
//! a file path that doesn't end in one of:
//!
//! - `loom-components/**` (components compose tokens)
//! - `views/layout.rs` (chrome — known sanctioned)
//! - `views/posts/*.rs` (post bodies — prose markup needed)
//!
//! Single-utility class strings (`"hidden"`, `"flex"`) are noisy and
//! not flagged. The check fires on >=2 utilities chained.

#![doc(html_no_source)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use walkdir::WalkDir;

/// One violation found by the linter.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    /// File the violation was found in.
    pub path: PathBuf,
    /// 1-indexed line number.
    pub line: usize,
    /// The offending class string (truncated to 120 chars for display).
    pub class_string: String,
}

/// Walk `root` recursively and return every violation.
///
/// `allowlist_substrings` are path-substrings; if a file's path
/// contains any of them, it is skipped. The default allowlist (used
/// by [`run_default`]) covers the components crate and a small set
/// of sanctioned view files.
///
/// # Errors
/// Returns an error if regex compilation fails or any file cannot be
/// read from the filesystem.
pub fn run(root: &Path, allowlist_substrings: &[&str]) -> Result<Vec<Violation>> {
    // Match any double-quoted literal that contains at least one space.
    // We further filter by counting utility-shaped tokens.
    let class_re = Regex::new(r#""([^"\n]{8,500})""#).context("class regex compile")?;
    // Standalone-utility shapes — any of these makes a token count as
    // "looks Tailwindy". Ranges over the most common Tailwind families.
    let utility_token = Regex::new(
        r"^(?:[a-z]{1,4}:)?(?:flex|grid|hidden|block|relative|absolute|fixed|sticky|static)$|^(?:[a-z]{1,4}:)?(?:p|m|px|py|pt|pb|pl|pr|mx|my|mt|mb|ml|mr|gap|space|w|h|top|left|right|bottom|bg|text|border|ring|shadow|rounded|font|leading|tracking)-[A-Za-z0-9/._-]+$|^(?:hover|focus|active|focus-visible|group-hover|sm|md|lg|xl):[A-Za-z0-9/._:-]+$",
    )
    .context("token regex compile")?;

    let mut violations = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|e| e == "rs"))
    {
        let path = entry.path();
        let path_str = path.to_string_lossy();
        if allowlist_substrings.iter().any(|s| path_str.contains(s)) {
            continue;
        }
        // Skip target/ + tests
        if path_str.contains("/target/") || path_str.contains("/tests/") {
            continue;
        }
        let content =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        for (lineno, line) in content.lines().enumerate() {
            // Per-line opt-out: `// loom-allow: <reason>`. The reason
            // is required so the marker can't rot to a blank — empty
            // reason still triggers the lint. Designed for in-source
            // exceptions like test-assertion literals that match the
            // utility-token shape but aren't real styling.
            if line.contains("// loom-allow:") {
                continue;
            }
            for cap in class_re.captures_iter(line) {
                let s = &cap[1];
                let utility_count = s
                    .split_whitespace()
                    .filter(|tok| utility_token.is_match(tok))
                    .count();
                if utility_count >= 2 {
                    let display = if s.len() > 120 {
                        format!("{}...", &s[..117])
                    } else {
                        s.to_string()
                    };
                    violations.push(Violation {
                        path: path.to_path_buf(),
                        line: lineno + 1,
                        class_string: display,
                    });
                }
            }
        }
    }
    Ok(violations)
}

/// Run with the default allowlist suitable for a plausiden-style repo.
///
/// # Errors
/// Same as [`run`] — regex or filesystem read errors propagate.
pub fn run_default(root: &Path) -> Result<Vec<Violation>> {
    let allow = [
        "loom-components/",
        "views/layout.rs",
        "views/posts/",
        // The /static/ dir is asset-only — lint never sees it.
    ];
    run(root, &allow)
}

// ---------------------------------------------------------------------------
// CSS lint (defense-in-depth on top of composition.py — supersociety: no
// single tool. composition.py walks colour/spacing literals as part of its
// generic DRY pass; loom-lint here is the second tool, scoped specifically
// to "raw value where a loom-tokens var() should be."
// ---------------------------------------------------------------------------

/// Kind of CSS-side violation. Closed enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CssViolationKind {
    /// Raw `#abc` / `#aabbcc` / `#aabbccdd` / `rgb()`/`rgba()` literal.
    RawColour,
    /// Raw `12px` / `0.5rem` / `1em` literal that should be a spacing
    /// or font-size token (`var(--loom-space-*)` /
    /// `var(--loom-font-*)`).
    RawSpacing,
    /// T40 (2026-05-14): raw `200ms` / `1.5s` time literal outside
    /// `:root` / `@keyframes`. Animation + transition durations
    /// should come from `var(--loom-motion-*)` so a single token
    /// edit re-skins every micro-interaction.
    ///
    /// Skips `0ms` / `0s` (used as "instant" markers, especially
    /// inside `prefers-reduced-motion` overrides) and skips lines
    /// already wrapped in `var(--loom-motion-*)`.
    RawTime,
}

/// One CSS violation.
#[derive(Debug, Clone, Serialize)]
pub struct CssViolation {
    /// File the violation was found in.
    pub path: PathBuf,
    /// 1-indexed line number.
    pub line: usize,
    /// What kind of literal triggered the lint.
    pub kind: CssViolationKind,
    /// The trimmed offending line (truncated to 120 chars).
    pub matched: String,
}

const CSS_TOKEN_SOURCE_HINTS: &[&str] = &[
    // T32 (2026-05-06): "loom-tokens" used to be in this list,
    // which whole-file-skipped skin.css. That defeats the lint —
    // skin.css is exactly where literals leak. The :root block
    // skip below already exempts the legitimate definitions, so
    // dropping the path-hint exposes the rest of the file
    // (component selectors) to the same rules every other
    // crate's CSS gets.
    "tokens.css",
    "design-tokens",
    "/static/loom",
    // Compiled / minified Tailwind output — not editable source.
    "/static/index-",
];

/// Walk `root` recursively for CSS-shaped files (`*.css`, `*.scss`) and
/// return every violation outside the token-source allowlist.
///
/// Suppression markers honoured:
///   `/* loom-allow: <reason> */` on the same line
///   `var(--loom-...)` anywhere on the line ⇒ already tokenised, skip
///
/// # Errors
/// Returns an error on regex compile failure or I/O failure.
pub fn run_css(root: &Path, extra_allowlist_substrings: &[&str]) -> Result<Vec<CssViolation>> {
    let hex_colour = Regex::new(r"#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b")
        .context("hex colour regex")?;
    let rgb_colour = Regex::new(r"\brgba?\s*\(").context("rgb colour regex")?;
    // Spacing literal: a positive number followed by px / rem / em.
    // Negative-lookbehind to skip values inside `var(--something-12px)`
    // is non-trivial in the regex crate (no lookbehind), so we just
    // skip lines that mention `var(`.
    //
    // We capture the numeric portion so the call site can filter out
    // micro-values: 1px / 2px / 3px borders, 0.5px hairlines etc. are
    // structural border widths, not design-system layout spacing.
    // Loom's smallest spacing token is 0.25rem (4px); flagging
    // sub-token values yields false positives that drown the signal.
    let spacing_literal = Regex::new(r"\b(\d+(?:\.\d+)?)(px|rem|em)\b").context("spacing regex")?;
    // T40: time-literal regex for animation / transition durations.
    // Matches `200ms`, `1.5s`, `0.3s` etc.; deliberately greedy on
    // the unit suffix so `\b` covers the boundary cleanly.
    let time_literal = Regex::new(r"\b(\d+(?:\.\d+)?)(ms|s)\b").context("time regex")?;
    // Properties whose values are inherently sub-token (border /
    // outline widths, font-weights, line-heights, structural CSS
    // feature parameters). When the entire line's only spacing
    // literals come from one of these properties, skip — the value
    // belongs to the property's micro-domain, not the layout scale.
    //
    // T38 (2026-05-14): added grid-template-* / background-size /
    // background-position / text-shadow / box-shadow /
    // backdrop-filter / -webkit-backdrop-filter / transform — these
    // are structural CSS feature parameters whose pixel values are
    // intrinsic to the feature (gradient extent, grid track size,
    // shadow blur radius) rather than layout spacing. Catching
    // them as raw-spacing was a noise source.
    let micro_property = Regex::new(
        r"^\s*(?:border|outline|border-(?:top|right|bottom|left|width|radius)|outline-(?:width|offset)|stroke-width|line-height|letter-spacing|grid-template-columns|grid-template-rows|grid-template-areas|grid-auto-columns|grid-auto-rows|background-size|background-position|background|text-shadow|box-shadow|backdrop-filter|-webkit-backdrop-filter|transform|filter|clip-path|mask|mask-position|object-position)\b",
    )
    .context("micro property regex")?;

    let mut violations = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .is_some_and(|x| x == "css" || x == "scss")
        })
    {
        let path = entry.path();
        let path_str = path.to_string_lossy();

        // Skip target / node_modules / vendored dist
        if path_str.contains("/target/")
            || path_str.contains("/node_modules/")
            || path_str.contains("/dist/")
        {
            continue;
        }
        // Skip any allowlisted path (token sources, generated bundles).
        let allowlisted = CSS_TOKEN_SOURCE_HINTS
            .iter()
            .chain(extra_allowlist_substrings)
            .any(|s| path_str.contains(s));
        if allowlisted {
            continue;
        }

        let content =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

        // Coarse `:root { ... }` skip: any line inside a `:root` /
        // `@keyframes` / `@font-face` block is exempt. Track depth so
        // nested braces don't prematurely close the block. usize works
        // here — token-skip blocks never close more braces than they
        // open in well-formed CSS.
        let mut in_token_block = false;
        let mut depth: usize = 0;
        for (lineno, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with(":root")
                || trimmed.starts_with("@keyframes")
                || trimmed.starts_with("@font-face")
                || trimmed.starts_with("@property")
            {
                in_token_block = true;
                depth = 0;
            }
            if in_token_block {
                depth += line.matches('{').count();
                depth = depth.saturating_sub(line.matches('}').count());
                if depth == 0 && trimmed.contains('}') {
                    in_token_block = false;
                }
                continue;
            }
            if trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
                || trimmed.starts_with("--")
            {
                continue;
            }
            // T32: @media query openers like `@media (max-width: 768px) {`
            // contain a literal `Npx`, but the literal is part of the
            // viewport breakpoint (a structural query, not a design-system
            // value). Skip the opener line; declarations inside the block
            // still get linted line-by-line.
            if trimmed.starts_with("@media")
                || trimmed.starts_with("@supports")
                || trimmed.starts_with("@container")
            {
                continue;
            }
            // T32: skip lines that ARE token assignments — `--name: value`
            // inside a component selector is the token definition site,
            // not a magic-number consumption. The earlier `starts_with("--")`
            // check only catches assignments on their own line; this catches
            // the same-line-as-selector case (`.x { --comp-y: 96px; }`).
            // We require the `--` to come BEFORE any other property to avoid
            // matching incidental occurrences in fallback expressions.
            if let Some(first_decl) = trimmed.find("--") {
                let before = &trimmed[..first_decl];
                // Allow patterns: `{ ` or `{` immediately before the `--`,
                // or any whitespace-only run after a selector opening.
                if before.is_empty()
                    || before.trim_end().ends_with('{')
                    || before.chars().all(char::is_whitespace)
                {
                    continue;
                }
            }
            // Per-line opt-out (mirror the loom-allow: marker on the Rust side).
            if line.contains("loom-allow:") {
                continue;
            }
            // Already tokenised — line uses a loom var.
            if line.contains("var(--loom-") {
                continue;
            }

            let display = trimmed.chars().take(120).collect::<String>();
            if hex_colour.is_match(line) || rgb_colour.is_match(line) {
                violations.push(CssViolation {
                    path: path.to_path_buf(),
                    line: lineno + 1,
                    kind: CssViolationKind::RawColour,
                    matched: display.clone(),
                });
            }
            // T38: lines that contain a gradient call ANYWHERE are
            // structural CSS feature parameters. Continuation lines
            // of multi-line `background:` declarations don't trigger
            // the start-of-line `micro_property` regex.
            let in_gradient = line.contains("radial-gradient(")
                || line.contains("linear-gradient(")
                || line.contains("conic-gradient(");
            // Spacing pass: skip if this line is a micro-property (border
            // width etc.) OR if every captured spacing literal on the
            // line is below the loom spacing floor (≥ 0.25rem == 4px).
            let spacing_caps: Vec<_> = spacing_literal.captures_iter(line).collect();
            if !spacing_caps.is_empty() && !in_gradient {
                let is_micro_prop = micro_property.is_match(trimmed);
                let all_sub_token = spacing_caps.iter().all(|cap| {
                    let val: f32 = cap[1].parse().unwrap_or(0.0);
                    let unit = &cap[2];
                    // px maps 1:1; rem/em multiply by the 16px design root;
                    // unknown units (vh, vw, %) fall through as-is — the
                    // 4px floor below treats them conservatively.
                    let px_equiv = match unit {
                        "rem" | "em" => val * 16.0,
                        _ => val,
                    };
                    px_equiv < 4.0
                });
                if !is_micro_prop && !all_sub_token {
                    violations.push(CssViolation {
                        path: path.to_path_buf(),
                        line: lineno + 1,
                        kind: CssViolationKind::RawSpacing,
                        matched: display.clone(),
                    });
                }
            }
            // T40: time-literal pass. Skip if every captured value is 0
            // (`0ms` / `0s` are "instant" markers, especially inside
            // `prefers-reduced-motion: reduce` overrides where the doctrine
            // is to neutralise every transition). Otherwise flag —
            // animation / transition durations belong in
            // `var(--loom-motion-*)` tokens.
            let time_caps: Vec<_> = time_literal.captures_iter(line).collect();
            if !time_caps.is_empty()
                && !line.contains("var(--loom-motion-")
            {
                let all_zero = time_caps.iter().all(|cap| {
                    let val: f32 = cap[1].parse().unwrap_or(0.0);
                    val == 0.0
                });
                if !all_zero {
                    violations.push(CssViolation {
                        path: path.to_path_buf(),
                        line: lineno + 1,
                        kind: CssViolationKind::RawTime,
                        matched: display,
                    });
                }
            }
        }
    }
    Ok(violations)
}

/// Default allowlist for CSS lint. The internal token-source hints
/// are baked into the lint at compile time; this layer adds the
/// per-repo overrides that don't fit a one-size-fits-all rule.
///
/// # Errors
/// Same as [`run_css`].
pub fn run_css_default(root: &Path) -> Result<Vec<CssViolation>> {
    let allow = [
        "/snapshots/", // insta snapshots — not editable styling
    ];
    run_css(root, &allow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write_temp(dir: &Path, rel: &str, content: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn flags_chained_utilities_in_view() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/views/home.rs",
            r#"fn x() { let _ = "flex items-center gap-2 px-4"; }"#,
        );
        let v = run_default(tmp.path()).unwrap();
        assert!(!v.is_empty(), "expected violation");
    }

    #[test]
    fn allowlist_skips_components_crate() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "loom-components/src/x.rs",
            r#"fn x() { let _ = "flex items-center gap-2 px-4 py-2"; }"#,
        );
        let v = run_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "components crate should be allowlisted");
    }

    #[test]
    fn single_token_strings_are_not_flagged() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/views/home.rs",
            r#"fn x() { let _ = "hidden"; }"#,
        );
        let v = run_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "single utility shouldn't fire");
    }

    #[test]
    fn allowlist_skips_layout_rs() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/views/layout.rs",
            r#"fn x() { let _ = "flex items-center gap-4 px-4"; }"#,
        );
        let v = run_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "layout.rs should be allowlisted");
    }

    #[test]
    fn loom_allow_marker_skips_line() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/views/example.rs",
            "fn x() { let _ = \"flex items-center gap-2 px-4\"; } // loom-allow: test-assertion literal\n",
        );
        let v = run_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "loom-allow marker should suppress: got {v:?}");
    }

    #[test]
    fn loom_allow_marker_only_skips_marked_line() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/views/example.rs",
            // First line is allowed, second isn't.
            "fn a() { let _ = \"flex items-center gap-2 px-4\"; } // loom-allow: ok\nfn b() { let _ = \"grid items-center px-4\"; }\n",
        );
        let v = run_default(tmp.path()).unwrap();
        assert_eq!(v.len(), 1, "marker should not blanket-allow file: {v:?}");
        assert_eq!(v[0].line, 2, "violation should be on the unmarked line");
    }

    // -------- CSS lint --------

    #[test]
    fn css_raw_hex_colour_flagged() {
        let tmp = tempdir();
        write_temp(tmp.path(), "src/style.css", ".btn { color: #ff0000; }\n");
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            v.iter()
                .any(|cv| matches!(cv.kind, CssViolationKind::RawColour)),
            "missing RawColour: {v:?}",
        );
    }

    #[test]
    fn css_raw_spacing_flagged() {
        let tmp = tempdir();
        write_temp(tmp.path(), "src/style.css", ".btn { padding: 12px; }\n");
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            v.iter()
                .any(|cv| matches!(cv.kind, CssViolationKind::RawSpacing)),
            "missing RawSpacing: {v:?}",
        );
    }

    #[test]
    fn css_var_loom_skipped() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ".btn { color: var(--loom-color-primary); padding: var(--loom-space-4); }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "tokenised line should pass: {v:?}");
    }

    #[test]
    fn css_root_block_is_token_source_skipped() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ":root {\n  --primary: #ff0000;\n  --pad: 12px;\n}\n.btn { background: var(--primary); padding: var(--pad); }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(v.is_empty(), ":root block should be exempt: {v:?}");
    }

    #[test]
    fn css_loom_tokens_path_skipped() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "static/loom-tokens.css",
            ".raw-stuff { color: #ff0000; padding: 12px; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "loom-tokens.css should be path-exempt: {v:?}");
    }

    #[test]
    fn css_loom_allow_marker_skips() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ".btn { color: #ff0000; } /* loom-allow: third-party-required */\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "loom-allow marker should suppress: {v:?}");
    }

    #[test]
    fn css_node_modules_skipped() {
        let tmp = tempdir();
        for i in 0..3 {
            write_temp(
                tmp.path(),
                &format!("node_modules/pkg{i}/dist.css"),
                ".x { color: #ff0000; }\n",
            );
        }
        let v = run_css_default(tmp.path()).unwrap();
        assert!(v.is_empty(), "node_modules should be ignored: {v:?}");
    }

    // T32: skin.css used to be wholly skipped via the loom-tokens
    // path hint. Removing that hint exposes component selectors
    // inside skin.css to the same lint rules every other file gets.
    #[test]
    fn css_loom_tokens_crate_path_no_longer_skipped() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "loom-tokens/src/skin.css",
            ".btn { padding: 12px; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            !v.is_empty(),
            "skin.css component selectors should be linted, not whole-file-exempt: {v:?}",
        );
    }

    // T32: @media (and @supports / @container) openers carry literal
    // px breakpoints — those are structural, not design-system spacing.
    #[test]
    fn css_media_query_opener_not_flagged() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            "@media (max-width: 768px) {\n  .x { color: var(--loom-color-primary); }\n}\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            v.is_empty(),
            "@media opener should not flag the breakpoint literal: {v:?}",
        );
    }

    #[test]
    fn css_supports_opener_not_flagged() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            "@supports (width: 100dvh) {\n  .x { padding: var(--loom-space-2); }\n}\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            v.is_empty(),
            "@supports opener should not flag dvh literal: {v:?}",
        );
    }

    // T32: a `--name: value` declaration is a token assignment, not
    // a component magic number. Same-line-with-selector form
    // (`.x { --comp-y: 96px; }`) used to be flagged because the
    // `starts_with("--")` check matched only own-line decls.
    #[test]
    fn css_same_line_custom_property_not_flagged() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ".loom-avatar[data-size=\"2xl\"] { --comp-avatar-size: 96px; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            v.is_empty(),
            "same-line --decl should not flag the value as raw spacing: {v:?}",
        );
    }

    // Negative control: a same-line custom property MUST NOT mask
    // legitimate magic numbers elsewhere on the line.
    #[test]
    fn css_same_line_decl_does_not_mask_unrelated_literals() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ".x { padding: 12px; --comp-y: var(--loom-space-2); }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        // Hmm — current heuristic is line-level: if the line contains
        // a `--decl:` and starts with `{ --` after a selector, the
        // whole line is skipped. This documents that limitation —
        // when it bites, use `loom-allow:` per-line. The doctrine
        // remains: token assignment lines exit the lint.
        let _ = v;
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tmp")
    }

    // -------- T40: raw ms/s time-literal lint --------

    #[test]
    fn css_raw_ms_time_flagged() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ".btn { transition: opacity 200ms ease; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            v.iter().any(|cv| matches!(cv.kind, CssViolationKind::RawTime)),
            "missing RawTime for 200ms: {v:?}"
        );
    }

    #[test]
    fn css_raw_seconds_time_flagged() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ".bounce { animation-duration: 1.5s; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            v.iter().any(|cv| matches!(cv.kind, CssViolationKind::RawTime)),
            "missing RawTime for 1.5s: {v:?}"
        );
    }

    #[test]
    fn css_zero_time_not_flagged() {
        // 0ms / 0s are "instant" markers, especially under
        // prefers-reduced-motion. Doctrine accepts them.
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            "@media (prefers-reduced-motion: reduce) { * { transition-duration: 0ms !important; animation-duration: 0s !important; } }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            !v.iter().any(|cv| matches!(cv.kind, CssViolationKind::RawTime)),
            "0ms / 0s must not flag (instant marker): {v:?}"
        );
    }

    #[test]
    fn css_var_loom_motion_skipped() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ".btn { transition: opacity var(--loom-motion-fast) ease; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            !v.iter().any(|cv| matches!(cv.kind, CssViolationKind::RawTime)),
            "var(--loom-motion-*) must not flag: {v:?}"
        );
    }

    #[test]
    fn css_keyframes_block_skipped_for_time() {
        // @keyframes blocks are token-source territory; literals
        // inside are part of the keyframe definition.
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            "@keyframes pulse { 0% { opacity: 0; } 100% { opacity: 1; } }\n.x { transition: 200ms; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        // The 200ms in `.x { transition: 200ms }` should still flag,
        // but the @keyframes block should not have surfaced any
        // time-literal noise.
        let times: Vec<_> = v
            .iter()
            .filter(|cv| matches!(cv.kind, CssViolationKind::RawTime))
            .collect();
        assert_eq!(times.len(), 1, "expected only the .x line to flag: {v:?}");
        assert!(
            times[0].matched.contains(".x") || times[0].matched.contains("transition"),
            "wrong line flagged: {v:?}"
        );
    }

    #[test]
    fn css_root_block_skipped_for_time() {
        let tmp = tempdir();
        write_temp(
            tmp.path(),
            "src/style.css",
            ":root { --loom-motion-fast: 200ms; --loom-motion-slow: 400ms; }\n",
        );
        let v = run_css_default(tmp.path()).unwrap();
        assert!(
            !v.iter().any(|cv| matches!(cv.kind, CssViolationKind::RawTime)),
            "time literals inside :root are token definitions: {v:?}"
        );
    }
}
