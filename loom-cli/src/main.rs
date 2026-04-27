//! `loom` — top-level CLI for the PlausiDen-Loom design system.
//!
//! Today: `loom lint`, `loom tokens`. The `audit` and `new` subcommands
//! are stubs that print what they will do and exit non-zero, so a CI
//! invocation that gets ahead of the implementation fails loudly rather
//! than silently no-op'ing.

#![doc(html_no_source)]

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "loom", version, about = "PlausiDen-Loom design-system CLI")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Walk a crate's `src/` and refuse raw class strings outside the
    /// design system.
    Lint {
        /// Path to the crate root (containing Cargo.toml). Defaults to
        /// the current directory.
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Emit machine-readable JSON instead of human output.
        #[arg(long)]
        json: bool,
    },
    /// Print the design tokens as JSON. For cross-platform consumers.
    Tokens,
    /// Drift report: raw class strings still present in a target crate,
    /// grouped by file. Unlike `lint`, the report includes
    /// previously-allowlisted files (`views/layout.rs`, `views/posts/`)
    /// so the migration backlog is visible. Suitable for monthly status
    /// (the design-system team's burn-down dashboard).
    Report {
        /// Path to the crate root. Defaults to the current directory.
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Emit machine-readable JSON instead of human output.
        #[arg(long)]
        json: bool,
    },
    /// (Not yet implemented.) Run visual-regression tests at every
    /// declared breakpoint.
    Audit,
    /// (Not yet implemented.) Scaffold a new page from a sanctioned
    /// template.
    New {
        /// Page name (slug, lowercase, dash-separated).
        name: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Lint { root, json } => match cmd_lint(&root, json) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(1),
            Err(e) => {
                eprintln!("loom lint: {e:#}");
                ExitCode::from(2)
            }
        },
        Cmd::Tokens => {
            println!("{}", loom_tokens::tokens_json());
            ExitCode::SUCCESS
        }
        Cmd::Report { root, json } => match cmd_report(&root, json) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom report: {e:#}");
                ExitCode::from(2)
            }
        },
        Cmd::Audit => {
            eprintln!("loom audit: not yet implemented (Playwright integration in follow-up).");
            ExitCode::from(1)
        }
        Cmd::New { name } => {
            eprintln!("loom new {name}: not yet implemented (template scaffold in follow-up).");
            ExitCode::from(1)
        }
    }
}

fn cmd_lint(root: &std::path::Path, json: bool) -> Result<usize> {
    let violations = loom_lint::run_default(root)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&violations).unwrap_or_else(|_| "[]".into()));
    } else if violations.is_empty() {
        println!("loom lint: clean ({})", root.display());
    } else {
        println!("loom lint: {} violation(s) in {}", violations.len(), root.display());
        for v in &violations {
            println!("  {}:{}", v.path.display(), v.line);
            println!("    \"{}\"", v.class_string);
        }
        println!();
        println!("Each violation = a raw class string in a non-allowlisted file.");
        println!("Move the styling into a typed component in loom-components.");
    }
    Ok(violations.len())
}

/// Drift report: count raw class strings per file, no file allowlist.
///
/// Unlike `lint` — which enforces a hard pass/fail with a sanctioned
/// set of skip-able paths — `report` shows everything still present
/// across the source tree, including the previously-allowlisted
/// `views/layout.rs` and `views/posts/`. This is the burn-down view
/// for an active migration: which files have the most raw classes,
/// where to focus next.
///
/// BUG ASSUMPTION: `loom-components/` (the design-system crate
/// itself) IS still skipped — those classes are sanctioned by
/// definition. A report that listed them would mistake the floor for
/// the ceiling.
///
/// SECURITY: Read-only; no side effects beyond stdout. Safe to invoke
/// from CI / cron / a developer's terminal interchangeably.
fn cmd_report(root: &std::path::Path, json: bool) -> Result<()> {
    use std::collections::BTreeMap;

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
    println!("  {:<60}  {}", "FILE", "RAW CLASSES");
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
