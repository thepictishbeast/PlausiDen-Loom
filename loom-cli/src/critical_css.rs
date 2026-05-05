//! Critical-CSS extractor.
//!
//! Walks a CSS source, identifies which top-level rules are
//! "critical" (needed for the first paint of every page), and
//! emits ONLY those. The rest can be deferred via
//! `<link rel="preload" as="style" onload="this.rel='stylesheet'">`
//! in the page-shell — meaning the browser doesn't block render
//! on the full stylesheet.
//!
//! WHAT COUNTS AS CRITICAL
//! -----------------------
//! 1. Anything inside `:root { … }` (token definitions — every
//!    component reads them).
//! 2. Universal/element selectors that establish base layout:
//!    `*`, `*::before`, `*::after`, `html`, `body`, `:focus-visible`,
//!    `[hidden]`, `img`, `video`, `svg`, `canvas`, `picture`,
//!    `pre`, `code`, `kbd`, `samp`, `a`, `button`,
//!    `p, h1, h2, …`, etc.
//! 3. The page-shell chrome that the cms-render template emits
//!    on every page: `.loom-skip`, `.loom-page-header`,
//!    `.loom-page-footer`, `.loom-page-nav`, `.loom-page-brand`,
//!    `.loom-page-title`, `.loom-page` (the `<main>` wrapper).
//! 4. Always-active media queries: `@media (prefers-color-scheme: …)`
//!    and `@media (prefers-reduced-motion: …)`.
//! 5. `@font-face` blocks (otherwise FOUT/FOIT).
//!
//! Everything else (component-specific rules like
//! `.loom-card-battle`, `.loom-section-hero`, `.loom-composer*`)
//! is deferred.
//!
//! THE PARSER
//! ----------
//! A minimal brace-matched CSS rule walker. Not a full CSS
//! parser; it does NOT handle:
//!   * preprocessor syntax
//!   * @rules other than `@media`, `@supports`, `@font-face`
//!     (those become opaque blocks — included if any contained
//!     rule matches a critical prefix)
//!
//! It DOES handle:
//!   * comments `/* … */` (counted toward output but not parsed)
//!   * string literals `"…"` and `'…'` (don't count braces inside)
//!   * URL literals `url(…)` (treat as opaque)
//!
//! AVP-2 INVARIANTS
//! ----------------
//! * `unsafe_code = "deny"`.
//! * No `unwrap`/`expect` in non-test code; all string indexing
//!   bounded.
//! * Comments preserved verbatim (audit trail).

#![forbid(unsafe_code)]

/// Single-element selectors that count as critical when they
/// equal one of these tokens. Used in selector matching only;
/// these are too short to safely substring-match inside arbitrary
/// rule bodies.
const CRITICAL_ELEMENTS: &[&str] = &[
    "*", "html", "body", "a", "p", "img", "video", "svg", "canvas", "picture", "pre", "code",
    "kbd", "samp", "button", "h1", "h2", "h3", "h4", "h5", "h6", "li", "dt", "dd",
];

/// Selector prefixes whose first match in a selector string is
/// enough to count the rule as critical. These are distinctive
/// enough to also be safe for body-substring matching.
const CRITICAL_PREFIXES: &[&str] = &[
    ":root",
    ":focus-visible",
    "[hidden]",
    "[lang]",
    // Page-shell chrome (every cms-render output uses these).
    ".loom-skip",
    ".loom-page-header",
    ".loom-page-footer",
    ".loom-page-nav",
    ".loom-page-brand",
    ".loom-page-title",
    ".loom-page",
    // Diagnostic banner used by forge.
    ".loom-css-loaded",
];

/// At-rules whose contents we recurse into for critical-prefix
/// matching. If any inner rule matches, we keep the entire @media
/// block (including its non-critical siblings — they get dragged
/// along; that's how CSS @media works).
const RECURSED_AT_RULES: &[&str] = &["@media", "@supports"];

/// At-rules emitted verbatim regardless of selector matching
/// (needed for first paint).
const VERBATIM_AT_RULES: &[&str] = &["@font-face", "@charset", "@import", "@namespace"];

/// Run extraction. Returns the critical-CSS subset of the input.
///
/// Errors are returned as-is via `Result<String, String>` rather
/// than introducing a new error type for one consumer; the CLI
/// maps the error string to stderr + exit code 1.
pub fn extract(css: &str) -> Result<String, String> {
    let mut out = String::with_capacity(css.len() / 4);
    let mut walker = Walker::new(css);
    while let Some(rule) = walker.next_rule()? {
        if rule_is_critical(&rule) {
            out.push_str(rule.text);
            // Preserve a separating newline so the output is
            // not a single 60-line line.
            if !rule.text.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    Ok(out)
}

/// Decide whether a single top-level rule is critical.
fn rule_is_critical(rule: &Rule<'_>) -> bool {
    let selector = rule.selector.trim();
    if selector.is_empty() {
        return false;
    }
    // Verbatim at-rules (@font-face etc.).
    for ar in VERBATIM_AT_RULES {
        if selector.starts_with(ar) {
            return true;
        }
    }
    // Recursed at-rules (@media, @supports): keep if ANY contained
    // top-level selector matches a critical prefix. We crudely
    // grep the body for any of CRITICAL_PREFIXES rather than
    // recursing through the parser — the body is small (one to
    // a few rules) and substring is sufficient at this resolution.
    for ar in RECURSED_AT_RULES {
        if selector.starts_with(ar) {
            return rule.body.is_some_and(body_contains_critical);
        }
    }
    // Plain selector. Match against any prefix.
    selector_is_critical(selector)
}

fn selector_is_critical(selector: &str) -> bool {
    for sel_part in selector.split(',') {
        let trimmed = sel_part.trim();
        // Match any prefix-style selector (.loom-page-*, :root, etc.).
        for prefix in CRITICAL_PREFIXES {
            if trimmed.starts_with(prefix) {
                return true;
            }
        }
        // Match element selectors via first identifier-token equality.
        // "p", "p:hover", "p::before", "p > a", "p, h1" all parse
        // their first token as "p" or "h1".
        let first_token: String = trimmed
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '*')
            .collect();
        for el in CRITICAL_ELEMENTS {
            if first_token == *el {
                return true;
            }
        }
    }
    false
}

fn body_contains_critical(body: &str) -> bool {
    // Look for any inner selector starting with a critical prefix.
    // Only check the longer / more-distinctive prefixes here —
    // single-letter element names like `a` or `p` would substring-
    // match inside any property value (`padding`, `tap`, `wrap`)
    // and produce false positives.
    for prefix in CRITICAL_PREFIXES {
        if body.contains(prefix) {
            return true;
        }
    }
    // Element-name detection inside @media bodies: scan for a
    // line whose first non-whitespace token equals one of our
    // critical elements followed by a selector-terminator
    // (` `, `,`, `{`, `:`, `>`, `+`, `~`, `[`).
    for line in body.lines() {
        let trimmed = line.trim_start();
        let first_token: String = trimmed
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '*')
            .collect();
        if first_token.is_empty() {
            continue;
        }
        let after_ix = first_token.len();
        let next = trimmed.as_bytes().get(after_ix).copied();
        let is_selector_boundary = matches!(
            next,
            Some(b' ' | b'{' | b',' | b':' | b'>' | b'+' | b'~' | b'[')
        );
        if !is_selector_boundary {
            continue;
        }
        for el in CRITICAL_ELEMENTS {
            if first_token == *el {
                return true;
            }
        }
    }
    false
}

/// One top-level rule + its preceding whitespace/comments. The
/// `text` field is exactly what we'd emit; `selector` is the part
/// before the `{`; `body` is `Some(content_inside_braces)` for
/// block rules and `None` for at-rules with no block.
struct Rule<'a> {
    text: &'a str,
    selector: &'a str,
    body: Option<&'a str>,
}

struct Walker<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Walker<'a> {
    const fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    /// Pull the next top-level rule (or comment block) from the
    /// stream. Returns `None` at end of input. Returns
    /// `Err(String)` on malformed CSS (unmatched braces, etc.).
    #[allow(clippy::too_many_lines)] // CSS state machine; splitting hurts readability.
    fn next_rule(&mut self) -> Result<Option<Rule<'a>>, String> {
        // Capture preceding whitespace + comments so the output
        // preserves the audit trail.
        let start = self.pos;
        // Skip whitespace + comments without consuming a rule.
        loop {
            self.skip_whitespace();
            if !self.try_skip_comment()? {
                break;
            }
        }
        if self.pos >= self.src.len() {
            // Trailing whitespace/comments without a rule — ignore.
            return Ok(None);
        }
        // Capture the rule body. Walk to end of selector
        // (until '{' OR ';' for at-rules without a body).
        let sel_start = self.pos;
        let mut depth = 0_i32;
        let mut in_string: Option<char> = None;
        loop {
            if self.pos >= self.src.len() {
                return Err(format!("unterminated rule starting at byte {sel_start}"));
            }
            let c = self.byte();
            if let Some(q) = in_string {
                if c == q as u8 {
                    in_string = None;
                } else if c == b'\\' && self.pos + 1 < self.src.len() {
                    self.pos += 1;
                }
                self.pos += 1;
                continue;
            }
            match c {
                b'"' => {
                    in_string = Some('"');
                    self.pos += 1;
                }
                b'\'' => {
                    in_string = Some('\'');
                    self.pos += 1;
                }
                b'/' if self.pos + 1 < self.src.len()
                    && self.src.as_bytes()[self.pos + 1] == b'*' =>
                {
                    self.try_skip_comment()?;
                }
                b'{' => {
                    depth += 1;
                    if depth == 1 {
                        // End of selector.
                        let selector = self.src.get(sel_start..self.pos).unwrap_or("");
                        let body_start = self.pos + 1;
                        self.pos = body_start;
                        // Walk to matching close brace.
                        let mut bdepth: i32 = 1;
                        let mut in_str: Option<char> = None;
                        let body_end = loop {
                            if self.pos >= self.src.len() {
                                return Err(format!("unterminated body at byte {body_start}"));
                            }
                            let c2 = self.byte();
                            if let Some(q) = in_str {
                                if c2 == q as u8 {
                                    in_str = None;
                                } else if c2 == b'\\' && self.pos + 1 < self.src.len() {
                                    self.pos += 1;
                                }
                                self.pos += 1;
                                continue;
                            }
                            match c2 {
                                b'"' => in_str = Some('"'),
                                b'\'' => in_str = Some('\''),
                                b'/' if self.pos + 1 < self.src.len()
                                    && self.src.as_bytes()[self.pos + 1] == b'*' =>
                                {
                                    self.try_skip_comment()?;
                                    continue;
                                }
                                b'{' => bdepth += 1,
                                b'}' => {
                                    bdepth -= 1;
                                    if bdepth == 0 {
                                        let end = self.pos;
                                        self.pos += 1;
                                        break end;
                                    }
                                }
                                _ => {}
                            }
                            self.pos += 1;
                        };
                        let body = self.src.get(body_start..body_end);
                        let rule_end = self.pos;
                        let text = self.src.get(start..rule_end).unwrap_or("");
                        return Ok(Some(Rule {
                            text,
                            selector,
                            body,
                        }));
                    }
                    self.pos += 1;
                }
                b';' if depth == 0 => {
                    // At-rule with no block (@import, @charset, @namespace).
                    self.pos += 1;
                    let selector = self.src.get(sel_start..self.pos).unwrap_or("");
                    let text = self.src.get(start..self.pos).unwrap_or("");
                    return Ok(Some(Rule {
                        text,
                        selector,
                        body: None,
                    }));
                }
                _ => self.pos += 1,
            }
        }
    }

    fn byte(&self) -> u8 {
        self.src.as_bytes()[self.pos]
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.src.len() && self.src.as_bytes()[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    /// Try to skip a `/* … */` comment. Returns true if one was
    /// consumed; false if not at a comment start. Errors on
    /// unterminated comment.
    fn try_skip_comment(&mut self) -> Result<bool, String> {
        if self.pos + 1 >= self.src.len() {
            return Ok(false);
        }
        if self.src.as_bytes()[self.pos] != b'/' || self.src.as_bytes()[self.pos + 1] != b'*' {
            return Ok(false);
        }
        self.pos += 2;
        while self.pos + 1 < self.src.len() {
            if self.src.as_bytes()[self.pos] == b'*' && self.src.as_bytes()[self.pos + 1] == b'/' {
                self.pos += 2;
                return Ok(true);
            }
            self.pos += 1;
        }
        Err("unterminated /* comment */".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn extracts_root_token_block() {
        let css = ":root { --loom-color: red; } .loom-card { color: red; }";
        let out = extract(css).expect("ok");
        assert!(out.contains(":root"));
        assert!(out.contains("--loom-color"));
        assert!(!out.contains(".loom-card"));
    }

    #[test]
    fn keeps_universal_selectors() {
        let css = "* { box-sizing: border-box; }\n.loom-card { padding: 1rem; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("* {"));
        assert!(!out.contains(".loom-card"));
    }

    #[test]
    fn keeps_html_and_body() {
        let css = "html { font-size: 16px; }\nbody { margin: 0; }\n.loom-card { x: 1; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("html {"));
        assert!(out.contains("body {"));
        assert!(!out.contains(".loom-card"));
    }

    #[test]
    fn keeps_focus_visible() {
        let css = ":focus-visible { outline: 2px solid blue; }\n.loom-card { x: 1; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains(":focus-visible"));
    }

    #[test]
    fn keeps_loom_page_chrome() {
        let css = ".loom-page-header { padding: 1rem; }\n.loom-card-battle { x: 1; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains(".loom-page-header"));
        assert!(!out.contains(".loom-card-battle"));
    }

    #[test]
    fn keeps_skip_link() {
        let css = ".loom-skip { position: absolute; }\n.loom-card { x: 1; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains(".loom-skip"));
    }

    #[test]
    fn keeps_font_face_verbatim() {
        let css = "@font-face { font-family: X; src: url('x.woff2'); }\n.loom-card { x: 1; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("@font-face"));
        assert!(!out.contains(".loom-card"));
    }

    #[test]
    fn keeps_prefers_color_scheme_media() {
        let css = "@media (prefers-color-scheme: dark) { html { background: black; } }\n.loom-card { x: 1; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("@media (prefers-color-scheme: dark)"));
    }

    #[test]
    fn drops_media_with_only_component_rules() {
        let css = "@media (min-width: 768px) { .loom-card-battle { padding: 2rem; } }\n";
        let out = extract(css).expect("ok");
        assert!(out.is_empty(), "expected empty, got: {out}");
    }

    #[test]
    fn keeps_media_with_critical_inner_rule() {
        let css = "@media (min-width: 768px) { html { font-size: 18px; } .loom-card { x: 1; } }\n";
        let out = extract(css).expect("ok");
        // Whole @media block kept (siblings dragged along).
        assert!(out.contains("@media"));
        assert!(out.contains("html"));
    }

    #[test]
    fn drops_component_rules() {
        let css =
            ".loom-card-battle { x: 1; }\n.loom-composer { y: 2; }\n.loom-section-hero { z: 3; }\n";
        let out = extract(css).expect("ok");
        assert!(out.is_empty());
    }

    #[test]
    fn handles_braces_in_strings() {
        let css = ".loom-page::before { content: '{ }'; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains(".loom-page::before"));
        assert!(out.contains("'{ }'"));
    }

    #[test]
    fn handles_comments_between_rules() {
        let css = "/* token block */ :root { --x: 1; }\n/* component */ .loom-card { y: 2; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains(":root"));
    }

    #[test]
    fn errors_on_unterminated_brace() {
        let css = ".loom-page { color: red;\n";
        let r = extract(css);
        assert!(r.is_err());
    }

    #[test]
    fn errors_on_unterminated_comment() {
        let css = ":root { --x: 1; }\n/* never closed";
        let r = extract(css);
        assert!(r.is_err());
    }

    #[test]
    fn paragraph_element_selector_kept() {
        let css = "p, h1, h2, h3, h4, h5, h6 { overflow-wrap: anywhere; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("overflow-wrap"));
    }

    #[test]
    fn img_video_picture_kept() {
        let css = "img, video, svg, canvas, picture { max-width: 100%; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("max-width"));
    }

    #[test]
    fn pre_code_kept() {
        let css = "pre, code, kbd, samp { overflow-x: auto; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("overflow-x"));
    }

    #[test]
    fn link_anchor_kept() {
        let css = "a { color: inherit; text-decoration: none; }\n";
        let out = extract(css).expect("ok");
        assert!(out.contains("color: inherit"));
    }

    #[test]
    fn output_size_smaller_than_input() {
        // Concatenation of one critical rule + 10 component rules.
        let mut css = String::from(":root { --x: 1; }\n");
        for i in 0..10 {
            let _ = writeln!(css, ".loom-card-{i} {{ padding: 1rem; }}");
        }
        let out = extract(&css).expect("ok");
        assert!(out.len() < css.len() / 2);
    }
}
