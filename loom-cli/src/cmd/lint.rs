use anyhow::Result;
use std::path::Path;
use tracing::info;

pub fn cmd_lint(root: &Path, json: bool) -> Result<usize> {
    let violations = loom_lint::run_default(root)?;
    let css_violations = loom_lint::run_css_default(root)?;
    let total = violations.len() + css_violations.len();

    if json {
        let payload = serde_json::json!({
            "rust_class_strings": violations,
            "css_raw_values": css_violations,
        });
        info!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into())
        );
        return Ok(total);
    }

    if total == 0 {
        info!("loom lint: clean ({})", root.display());
        return Ok(0);
    }

    if !violations.is_empty() {
        info!(
            "loom lint: {} Rust class-string violation(s) in {}",
            violations.len(),
            root.display()
        );
        for v in &violations {
            info!("  {}:{}", v.path.display(), v.line);
            info!("    \"{}\"", v.class_string);
        }
        info!(" ");
        info!("Each Rust violation = a raw class string in a non-allowlisted file.");
        info!("Move the styling into a typed component in loom-components.");
    }

    if !css_violations.is_empty() {
        info!(" ");
        info!(
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
            info!("  {}:{} [{}]", cv.path.display(), cv.line, kind);
            info!("    {}", cv.matched);
        }
        info!(" ");
        info!("Each CSS violation = a raw colour / spacing literal outside a token-source file.");
        info!("Replace with a `var(--loom-color-*)` / `var(--loom-space-*)` from loom-tokens.css,");
        info!("or add the file to LOOM_CSS_LINT_ALLOWLIST in PlausiDen-Loom doctrine.");
    }

    Ok(total)
}
