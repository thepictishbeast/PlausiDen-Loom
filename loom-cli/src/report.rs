//! `loom report` subcommand — drift report counting raw class
//! strings per file with no file allowlist.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3).
//!
//! Unlike `lint` — which enforces a hard pass/fail with a sanctioned
//! set of skip-able paths — `report` shows everything still present
//! across the source tree, including the previously-allowlisted
//! `views/layout.rs` and `views/posts/`. This is the burn-down view
//! for an active migration: which files have the most raw classes,
//! where to focus next.

use std::collections::BTreeMap;

use anyhow::Result;

/// Run the raw-class lint across `root` (only `loom-components/`
/// excluded) and emit a per-file breakdown ranked by violation
/// count.
pub fn cmd_report(root: &std::path::Path, json: bool) -> Result<()> {
    // Only the components crate is sanctioned to compose tokens directly.
    // Everything else (views, even allowlisted ones) counts as drift.
    let allow = ["loom-components/"];
    let violations = loom_lint::run(root, &allow)?;

    let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
    for v in &violations {
        let key = v.path.display().to_string();
        *by_file.entry(key).or_insert(0) += 1;
    }
    let mut ranked: Vec<(String, usize)> = by_file.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    if json {
        let payload = serde_json::json!({
            "root": root.display().to_string(),
            "total_violations": violations.len(),
            "files": ranked
                .iter()
                .map(|(p, n)| serde_json::json!({"path": p, "violations": n}))
                .collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into())
        );
        return Ok(());
    }

    println!("loom report — design-system drift in {}", root.display());
    println!("Total raw-class violations: {}", violations.len());
    println!("(loom-components/ is sanctioned and excluded from the count.)");
    println!();
    if ranked.is_empty() {
        println!("No drift detected — every view file goes through Loom primitives.");
        return Ok(());
    }
    println!("Per-file breakdown (descending):");
    println!();
    println!("  {:<60}  RAW CLASSES", "FILE");
    println!("  {:<60}  {}", "-".repeat(60), "-".repeat(11));
    for (path, count) in &ranked {
        println!("  {path:<60}  {count}");
    }
    println!();
    println!("To resolve: replace the raw class string with a typed");
    println!("primitive from loom-components/. If a primitive does not");
    println!("yet exist, propose one in a separate PR (see CLAUDE.md).");
    Ok(())
}
