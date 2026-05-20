//! `loom audit` subcommand — emit a visual-regression journey file
//! that the PlausiDen-Crawler can execute.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3).

use anyhow::Result;

use loom_tokens::Breakpoint;

/// Emit a Crawler journey JSON that visits `url` at every Loom
/// breakpoint and screenshots each. Writes to `journey_path` (or
/// stdout when `journey_path == "-"`).
///
/// Implementation is intentionally a thin journey emitter rather
/// than a full visual-diff engine: PlausiDen-Crawler already does
/// the screenshot/diff loop.
pub fn cmd_audit(journey_path: &str, url: &str) -> Result<()> {
    let breakpoints = Breakpoint::all();
    let mut steps: Vec<serde_json::Value> = Vec::with_capacity(breakpoints.len() * 3);
    for bp in breakpoints {
        let bp_name = bp.tailwind();
        let bp_px = bp.px();
        // The crawler journey runner currently expects per-step
        // viewport via the journey's top-level `viewport` field
        // OR a CLI override; per-step viewport switching is
        // tracked as a crawler enhancement. For now emit one
        // goto+screenshot per breakpoint and leave viewport
        // switching to the crawler --viewport flag invocation.
        steps.push(serde_json::json!({
            "kind": "goto",
            "url": url,
            "timeout": 15000,
            "label": format!("goto-{bp_name}-{bp_px}px"),
        }));
        steps.push(serde_json::json!({ "kind": "wait", "ms": 600 }));
        steps.push(serde_json::json!({
            "kind": "screenshot",
            "label": format!("loom-audit-{bp_name}"),
        }));
    }
    let journey = serde_json::json!({
        "name": "loom-audit",
        "description": "Visual-regression journey — screenshot every Loom breakpoint. Run via `node --loader ts-node/esm src/main.ts --journey <path>` in PlausiDen-Crawler.",
        "baseUrl": url,
        "viewport": { "w": 1440, "h": 900 },
        "steps": steps,
    });
    let pretty =
        serde_json::to_string_pretty(&journey).expect("token tree is finite + serde-clean");
    if journey_path == "-" {
        println!("{pretty}");
    } else {
        std::fs::write(journey_path, pretty)
            .map_err(|e| anyhow::anyhow!("write {journey_path}: {e}"))?;
        println!("loom audit: journey written to {journey_path}");
        println!("Run with:");
        println!("  cd /path/to/PlausiDen-Crawler");
        println!("  node --loader ts-node/esm src/main.ts --journey {journey_path}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_stdout_path_emits_journey_to_stdout() {
        // `journey_path == "-"` writes to stdout instead of disk.
        // We can't easily capture stdout in a unit test; we just
        // confirm the function returns Ok and doesn't try to
        // write to "-" as a file path.
        assert!(cmd_audit("-", "https://example.com").is_ok());
    }

    #[test]
    fn audit_to_file_writes_valid_json() {
        let tmp = std::env::temp_dir().join("loom-audit-test.json");
        let path_str = tmp.to_string_lossy().into_owned();
        cmd_audit(&path_str, "https://example.com").expect("ok");
        let body = std::fs::read_to_string(&tmp).expect("readable");
        let v: serde_json::Value = serde_json::from_str(&body).expect("emitted JSON parses");
        assert_eq!(v["name"], "loom-audit");
        assert_eq!(v["baseUrl"], "https://example.com");
        assert!(v["steps"].as_array().unwrap().len() >= 3);
        let _ = std::fs::remove_file(&tmp);
    }
}
