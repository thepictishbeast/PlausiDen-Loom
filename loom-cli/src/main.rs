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
