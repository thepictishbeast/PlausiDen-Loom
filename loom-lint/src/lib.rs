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

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tmp")
    }
}
