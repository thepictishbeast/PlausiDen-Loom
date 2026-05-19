//! `loom lint` subcommand — enforce raw-class-string + raw-CSS-
//! value bans across a Loom-consuming workspace.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3).

use anyhow::Result;

/// Run both the Rust class-string lint and the CSS raw-value lint
/// over `root`. Returns the total violation count. Prints
/// human-readable output (or JSON when `json = true`).
pub fn cmd_lint(root: &std::path::Path, json: bool) -> Result<usize> {
    let violations = loom_lint::run_default(root)?;
    let css_violations = loom_lint::run_css_default(root)?;
    let total = violations.len() + css_violations.len();

    if json {
        // Combined JSON object so consumers can disambiguate.
        let payload = serde_json::json!({
            "rust_class_strings": violations,
            "css_raw_values": css_violations,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into())
        );
        return Ok(total);
    }

    if total == 0 {
        println!("loom lint: clean ({})", root.display());
        return Ok(0);
    }

    if !violations.is_empty() {
        println!(
            "loom lint: {} Rust class-string violation(s) in {}",
            violations.len(),
            root.display()
        );
        for v in &violations {
            println!("  {}:{}", v.path.display(), v.line);
            println!("    \"{}\"", v.class_string);
        }
        println!();
        println!("Each Rust violation = a raw class string in a non-allowlisted file.");
        println!("Move the styling into a typed component in loom-components.");
    }

    if !css_violations.is_empty() {
        println!();
        println!(
            "loom lint: {} CSS raw-value violation(s) in {}",
            css_violations.len(),
            root.display()
        );
        for cv in &css_violations {
            let kind = match cv.kind {
                loom_lint::CssViolationKind::RawColour => "raw-colour",
                loom_lint::CssViolationKind::RawSpacing => "raw-spacing",
                loom_lint::CssViolationKind::RawTime => "raw-time",
            };
            println!("  {}:{} [{}]", cv.path.display(), cv.line, kind);
            println!("    {}", cv.matched);
        }
        println!();
        println!(
            "Each CSS violation = a raw colour / spacing literal outside a token-source file."
        );
        println!(
            "Replace with a `var(--loom-color-*)` / `var(--loom-space-*)` from loom-tokens.css,",
        );
        println!("or extend the token set if no role fits.");
    }

    Ok(total)
}
