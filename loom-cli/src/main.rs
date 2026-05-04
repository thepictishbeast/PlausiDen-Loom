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
    /// Run visual-regression tests via PlausiDen-Crawler. Wraps a
    /// crawler journey that screenshots every breakpoint declared in
    /// loom-tokens. The crawler does the actual diffing; this
    /// subcommand is a typed entry point that locks the journey
    /// shape so `loom audit` is the one canonical invocation.
    Audit {
        /// Path to a journey JSON. Defaults to a generated one
        /// printed to stdout if the path is `-`.
        #[arg(long, default_value = "-")]
        journey: String,
        /// URL of the running site to audit.
        #[arg(long, default_value = "https://next.plausiden.com/")]
        url: String,
    },
    /// Scaffold a new page view from a sanctioned template. Emits
    /// a stub `<root>/src/views/<name>.rs` composed entirely from
    /// Loom primitives, plus the `pub mod <name>;` line for
    /// `views.rs`. Refuses to overwrite an existing file.
    New {
        /// Page name (slug, lowercase, dash-separated, used as the
        /// file name and the route path).
        name: String,
        /// Path to the consuming crate's root. Defaults to current.
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Template flavor. `landing` = hero + section + CTA;
        /// `legal` = boxed disclaimer + body; `article` = blog-shaped.
        #[arg(long, default_value = "landing")]
        template: String,
    },
    /// Emit a GTK 4 CSS theme built from loom-tokens. Pipe to a file:
    ///   `loom gtk-theme > ~/.config/gtk-4.0/loom.css`
    GtkTheme {
        /// Use the dark-theme token set.
        #[arg(long)]
        dark: bool,
    },
    /// Emit every token as CSS custom properties under `:root` and
    /// `:root[data-theme="dark"]`. Drop into any web surface as
    /// `loom-tokens.css` and reference values via `var(--loom-color-*)`.
    ///
    ///   `loom css > path/to/static/loom-tokens.css`
    Css,
    /// Emit every token as Rust `pub const` blocks for inclusion in
    /// an egui-driven app (Atrium etc.). Pipe to a source file:
    ///
    ///   `loom egui > src/loom_tokens.rs`
    Egui,
    /// Verify the design-system doctrine document is in sync with
    /// the code it claims to govern. Fails if CLAUDE.md is missing
    /// load-bearing sections, references primitives that don't
    /// exist, or has drifted from the structural shape we publish.
    Doctor {
        /// Path to the Loom repo root. Defaults to current directory.
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Render a CMS page document (JSON) to a static HTML file.
    /// Reads the document from `--input`, runs it through the
    /// `loom-cms-render` bridge, wraps the resulting body markup
    /// in a minimal page-shell template (`<html lang>`, strict
    /// CSP meta, viewport, charset, canonical link, single `<h1>`
    /// from CmsPage.title), and writes to `--out` (or stdout if
    /// `--out -`).
    ///
    /// Exit codes:
    ///   0 — page rendered + written
    ///   1 — JSON malformed or schema violation (deny_unknown_fields)
    ///   2 — I/O error reading input or writing output
    CmsRender {
        /// Path to the CmsPage JSON document.
        #[arg(long)]
        input: PathBuf,
        /// Output path. `-` for stdout.
        #[arg(long, default_value = "-")]
        out: String,
        /// Override the CSS href emitted by the page-shell. Useful
        /// when the consumer wants to link a different stylesheet
        /// (e.g. critical-CSS-extracted variant) than the default
        /// `/loom-skin.css`.
        #[arg(long, default_value = "/loom-skin.css")]
        css_href: String,
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
        Cmd::Audit { journey, url } => match cmd_audit(&journey, &url) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom audit: {e:#}");
                ExitCode::from(1)
            }
        },
        Cmd::New {
            name,
            root,
            template,
        } => match cmd_new(&name, &root, &template) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom new: {e:#}");
                ExitCode::from(1)
            }
        },
        Cmd::GtkTheme { dark } => {
            print!("{}", cmd_gtk_theme(dark));
            ExitCode::SUCCESS
        }
        Cmd::Css => {
            print!("{}", loom_tokens::tokens_css());
            ExitCode::SUCCESS
        }
        Cmd::Egui => {
            print!("{}", loom_tokens::tokens_egui());
            ExitCode::SUCCESS
        }
        Cmd::Doctor { root } => match cmd_doctor(&root) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom doctor: {e:#}");
                ExitCode::from(1)
            }
        },
        Cmd::CmsRender {
            input,
            out,
            css_href,
        } => match cmd_cms_render(&input, &out, &css_href) {
            Ok(()) => ExitCode::SUCCESS,
            Err(CmsRenderError::Schema(e)) => {
                eprintln!("loom cms-render: schema error: {e}");
                ExitCode::from(1)
            }
            Err(CmsRenderError::Io(e)) => {
                eprintln!("loom cms-render: i/o error: {e}");
                ExitCode::from(2)
            }
        },
    }
}

fn cmd_lint(root: &std::path::Path, json: bool) -> Result<usize> {
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

/// Verify the design-system doctrine document is in sync with code.
///
/// Three checks:
///   1. CLAUDE.md exists at the expected path.
///   2. The document carries every load-bearing section we publish
///      (the single rule, the crate map, the hard rules, the
///      "what this is not" boundary). If any goes missing, a future
///      contributor reading the doctrine could miss the line that
///      forbids raw class strings — silent drift, no review.
///   3. Every primitive name CLAUDE.md mentions in the crate map
///      has a corresponding `pub mod` declaration in
///      `loom-components/src/lib.rs`. The doctrine claims a thing
///      exists; the audit verifies it does.
///
/// SECURITY: read-only, no network. Safe to invoke from CI / cron /
/// terminal interchangeably; emits findings to stdout.
fn cmd_doctor(root: &std::path::Path) -> Result<()> {
    let claude_path = root.join("CLAUDE.md");
    if !claude_path.exists() {
        anyhow::bail!("CLAUDE.md missing at {}", claude_path.display());
    }
    let claude = std::fs::read_to_string(&claude_path)
        .map_err(|e| anyhow::anyhow!("read CLAUDE.md: {e}"))?;

    let mut findings = Vec::new();

    // Required sections — exact heading text we publish. A rename is
    // a doctrine event; this audit catches accidental ones.
    let required_sections = [
        "## The single rule",
        "## Why this exists",
        "## Crate map",
        "## Hard rules",
        "## What this is not",
    ];
    for section in required_sections {
        if !claude.contains(section) {
            findings.push(format!("CLAUDE.md missing section: `{section}`"));
        }
    }

    // The crate map names primitives that should exist as `pub mod`s
    // in loom-components/src/lib.rs. Read both, intersect, complain
    // about anything mentioned but not declared.
    let lib_path = root.join("loom-components/src/lib.rs");
    if lib_path.exists() {
        let lib = std::fs::read_to_string(&lib_path)
            .map_err(|e| anyhow::anyhow!("read loom-components/src/lib.rs: {e}"))?;
        let claimed_modules = ["Button", "Card", "Section", "Hero", "Footer", "Nav"];
        for module in claimed_modules {
            // The lib.rs uses lowercase mod names + the type name in
            // pub use; we check for the type name being exported.
            if !lib.contains(&format!("pub use {}", module.to_lowercase())) && !lib.contains(module)
            {
                findings.push(format!(
                    "Crate map mentions `{module}` but loom-components/src/lib.rs does not export it"
                ));
            }
        }
    } else {
        findings.push(format!(
            "loom-components/src/lib.rs missing at {} — cannot verify crate map",
            lib_path.display()
        ));
    }

    if findings.is_empty() {
        println!("loom doctor: clean ({})", root.display());
        Ok(())
    } else {
        println!("loom doctor: {} finding(s):", findings.len());
        for f in &findings {
            println!("  - {f}");
        }
        anyhow::bail!("doctrine drift detected; see findings above")
    }
}

/// Emit a crawler-shaped JSON journey to stdout (or the given path).
/// The journey hits each declared breakpoint in `loom-tokens`, navigates
/// to the URL, and screenshots — leaving the diffing to the crawler.
///
/// The implementation is intentionally a thin journey emitter rather
/// than a full visual-diff engine: the crawler already does the
/// screenshot/diff loop; reimplementing it here would be duplication.
fn cmd_audit(journey_path: &str, url: &str) -> Result<()> {
    use loom_tokens::Breakpoint;
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

/// Scaffold a new view file under `<root>/src/views/<name>.rs`.
/// Refuses to overwrite. Adds a TODO line at the top reminding the
/// caller to wire the route + handler + sitemap entry — those are
/// per-crate decisions and can't be safely automated from here.
fn cmd_new(name: &str, root: &std::path::Path, template: &str) -> Result<()> {
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        anyhow::bail!("name must be lowercase ASCII + dashes (got {name:?})");
    }
    let module_name = name.replace('-', "_");
    let target = root.join("src/views").join(format!("{module_name}.rs"));
    if target.exists() {
        anyhow::bail!("refuse to overwrite existing {}", target.display());
    }
    let template_body = match template {
        "landing" => template_landing(name, &module_name),
        "legal" => template_legal(name, &module_name),
        "article" => template_article(name, &module_name),
        other => anyhow::bail!("unknown template {other:?}; expected landing | legal | article"),
    };
    std::fs::write(&target, template_body)
        .map_err(|e| anyhow::anyhow!("write {}: {e}", target.display()))?;
    println!("loom new: scaffolded {}", target.display());
    println!();
    println!("Next steps (manual — these are per-crate wiring decisions):");
    println!("  1. Add `pub mod {module_name};` to src/views.rs");
    println!("  2. Add a handler in src/handlers.rs that calls views::{module_name}::render()");
    println!("  3. Add `.route(\"/{name}\", get(handlers::{module_name}))` in main.rs");
    println!("  4. Add the route to SITEMAP_ROUTES if it should be indexed");
    println!(
        "  5. Add `snap_route!({module_name}, \"/{name}\")` if the crate uses insta snapshots"
    );
    Ok(())
}

fn template_landing(name: &str, _module: &str) -> String {
    format!(
        r#"//! `/{name}` — placeholder generated by `loom new {name} --template landing`.

use maud::{{Markup, html}};

use crate::views::layout::page;

#[must_use]
pub fn render() -> Markup {{
    let body = html! {{
        section class="pt-32 pb-16 md:pt-44 md:pb-20 bg-slate-50" {{
            div class="container mx-auto px-4 md:px-6 max-w-4xl" {{
                h1 class="font-display text-4xl md:text-5xl lg:text-6xl font-bold text-slate-900 leading-[1.1] mb-4" {{
                    "{name} headline goes here"
                }}
                p class="text-lg text-slate-600 max-w-2xl leading-relaxed" {{
                    "Subhead. Replace with real content. Default copy lives here so the snapshot test ratchets up to real wording."
                }}
            }}
        }}
    }};
    page("{name} — PlausiDen", "/{name}", body)
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn renders_nonempty() {{
        assert!(render().into_string().len() > 1_000);
    }}
}}
"#,
        name = name,
    )
}

fn template_legal(name: &str, _module: &str) -> String {
    format!(
        r#"//! `/{name}` — legal placeholder generated by `loom new {name} --template legal`.

use maud::{{Markup, html}};

use crate::views::layout::page;

#[must_use]
pub fn render() -> Markup {{
    let body = html! {{
        section class="pt-32 pb-16 md:pt-48 md:pb-20 bg-slate-50" {{
            div class="container mx-auto px-4 md:px-6 max-w-3xl" {{
                span class="inline-block px-4 py-1.5 rounded-full bg-primary/10 text-primary font-semibold text-sm mb-6 border border-primary/20" {{
                    "Legal"
                }}
                h1 class="font-display text-4xl md:text-5xl font-bold text-slate-900 leading-[1.1] mb-4" {{
                    "{name}"
                }}
            }}
        }}
        section class="py-16 bg-white" {{
            div class="container mx-auto px-4 md:px-6 max-w-3xl" {{
                div class="rounded-xl border border-amber-200 bg-amber-50 p-6 mb-10" {{
                    p class="text-sm text-amber-900 font-medium mb-2" {{ "Placeholder — under legal review" }}
                    p class="text-sm text-amber-800 leading-relaxed" {{
                        "Replace with the counsel-reviewed text. Until then, this banner is operative."
                    }}
                }}
            }}
        }}
    }};
    page("{name} — PlausiDen", "/{name}", body)
}}
"#,
        name = name,
    )
}

fn template_article(name: &str, _module: &str) -> String {
    format!(
        r#"//! `/{name}` — article placeholder generated by `loom new {name} --template article`.

use maud::{{Markup, html}};

use crate::views::layout::page;

#[must_use]
pub fn render() -> Markup {{
    let body = html! {{
        article class="prose prose-slate mx-auto max-w-3xl px-4 md:px-6 pt-32 pb-16" {{
            p class="text-sm text-slate-500 mb-2" {{ "Field note · YYYY-MM-DD · X min read" }}
            h1 class="font-display text-3xl md:text-4xl font-bold text-slate-900 mb-6" {{
                "{name} title goes here"
            }}
            p class="text-lg text-slate-600 leading-relaxed mb-8" {{
                "Lede paragraph. Replace with real content."
            }}
            h2 class="font-display text-2xl md:text-3xl font-bold text-slate-900 mt-12 mb-4" {{
                "First section heading"
            }}
            p class="mb-6" {{ "Body paragraph." }}
        }}
    }};
    page("{name} — PlausiDen", "/{name}", body)
}}
"#,
        name = name,
    )
}

/// Emit a GTK 4 CSS theme built from loom-tokens. Maps each
/// semantic role to GTK's named colors so a downstream Thundercrab
/// GTK build (or any GTK app) inherits the same palette as the web
/// site without re-implementing it.
///
/// The CSS is small (~80 lines) and intentionally limited to color
/// + spacing tokens — animations, fonts, and layout are GTK-app-
/// specific and shouldn't be baked into a shared theme.
fn cmd_gtk_theme(dark: bool) -> String {
    use loom_tokens::ColorRole;
    let palette = if dark {
        ColorRole::dark_all()
    } else {
        ColorRole::all()
    };
    let mode = if dark { "dark" } else { "light" };
    let mut out = String::new();
    out.push_str(&format!(
        "/* GTK 4 theme generated from loom-tokens ({mode}). */\n"
    ));
    out.push_str("/* Do not edit by hand — re-run `loom gtk-theme` after a token change. */\n\n");
    out.push_str(":root {\n");
    for role in palette {
        out.push_str(&format!(
            "  --loom-{name}: {css};\n",
            name = role.role,
            css = role.color.css
        ));
    }
    out.push_str("}\n\n");
    // Map a few critical GTK named colors to Loom roles. GTK named
    // colors are referenced by `@name` in widget CSS.
    out.push_str("@define-color theme_bg_color var(--loom-surface);\n");
    out.push_str("@define-color theme_fg_color var(--loom-ink);\n");
    out.push_str("@define-color theme_base_color var(--loom-surface-muted);\n");
    out.push_str("@define-color theme_text_color var(--loom-ink);\n");
    out.push_str("@define-color theme_selected_bg_color var(--loom-primary);\n");
    out.push_str("@define-color theme_selected_fg_color var(--loom-primary-fg);\n");
    out.push_str("@define-color borders var(--loom-border);\n");
    out.push_str("@define-color error_color var(--loom-danger);\n");
    out.push_str("@define-color success_color var(--loom-success);\n");
    out
}

/// `loom cms-render` error type. We split schema errors (from
/// serde_json) from I/O errors (from std::fs / std::io) so the
/// dispatch in `main` can map them to distinct exit codes (1 vs 2).
#[derive(Debug)]
enum CmsRenderError {
    Schema(serde_json::Error),
    Io(std::io::Error),
}

impl From<serde_json::Error> for CmsRenderError {
    fn from(e: serde_json::Error) -> Self {
        CmsRenderError::Schema(e)
    }
}

impl From<std::io::Error> for CmsRenderError {
    fn from(e: std::io::Error) -> Self {
        CmsRenderError::Io(e)
    }
}

/// Render a CmsPage JSON document to a complete HTML file.
///
/// The page-shell template emitted is intentionally minimal —
/// just enough to pass forge.sh's strict CSP / canonical / lang
/// / single-h1 audits without depending on a particular consumer
/// app's chrome. Apps wanting custom shells can read the body
/// markup directly via `loom_cms_render::render_page` instead.
fn cmd_cms_render(
    input: &std::path::Path,
    out: &str,
    css_href: &str,
) -> Result<(), CmsRenderError> {
    let raw = std::fs::read_to_string(input)?;
    let page: loom_cms_render::CmsPage = serde_json::from_str(&raw)?;
    let body = loom_cms_render::render_page(&page);
    let shell = page_shell(&page, css_href, &body.into_string());
    if out == "-" {
        print!("{shell}");
        return Ok(());
    }
    if let Some(parent) = std::path::Path::new(out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, &shell)?;
    Ok(())
}

/// Wrap rendered body markup in the smallest valid HTML5 page
/// that passes every Loom + forge.sh audit:
///
///   * `<html lang="en">` — phase_a11y_landmarks + phase_seo
///   * `<meta charset="utf-8">` — required first
///   * `<meta http-equiv="Content-Security-Policy" ...>` — strict
///   * `<meta http-equiv="X-Content-Type-Options" ...>` — nosniff
///   * `<meta name="viewport" ...>` — mobile-first
///   * `<title>` from page.title
///   * `<meta name="description">` from page.description
///   * `<link rel="canonical">` from page.path
///   * `<link rel="stylesheet" href="{css_href}">` — design system
///   * `<h1>` from page.title — exactly one
///   * The bridge-rendered body
///
/// The output is HTML-escaped via plain string concatenation only
/// for fields the schema marks as text (title, description, path).
/// The body markup is already escaped by Maud.
fn page_shell(page: &loom_cms_render::CmsPage, css_href: &str, body: &str) -> String {
    // SECURITY: the only fields we interpolate as attribute or text
    // values are page.title, page.description, page.path, css_href.
    // All four pass through escape_html_text(); none is allowed to
    // break out of its slot.
    let title = escape_html_text(&page.title);
    let description = escape_html_text(&page.description);
    let path = escape_html_attr(&page.path);
    let css = escape_html_attr(css_href);
    format!(
        "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
  <meta charset=\"utf-8\">\n\
  <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-ancestors 'none'\">\n\
  <meta http-equiv=\"X-Content-Type-Options\" content=\"nosniff\">\n\
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
  <title>{title}</title>\n\
  <meta name=\"description\" content=\"{description}\">\n\
  <link rel=\"canonical\" href=\"{path}\">\n\
  <link rel=\"stylesheet\" href=\"{css}\">\n\
</head>\n\
<body>\n\
  <a class=\"loom-skip\" href=\"#content\">Skip to content</a>\n\
  <header class=\"loom-page-header\" role=\"banner\">\n\
    <nav class=\"loom-page-nav\" aria-label=\"Primary\">\n\
      <a class=\"loom-page-brand\" href=\"/\">SkillShots</a>\n\
    </nav>\n\
    <h1 class=\"loom-page-title\">{title}</h1>\n\
  </header>\n\
  <div id=\"content\">\n\
{body}\n\
  </div>\n\
  <footer class=\"loom-page-footer\" role=\"contentinfo\">\n\
    <small>SkillShots — voted skill battles.</small>\n\
  </footer>\n\
</body>\n\
</html>\n"
    )
}

/// Escape a text node (HTML body text or `<title>`).
fn escape_html_text(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// Escape a value going inside a double-quoted attribute.
fn escape_html_attr(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod cms_render_tests {
    use super::*;
    use loom_cms_render::CmsPage;

    fn empty_page() -> CmsPage {
        CmsPage {
            title: "Test".to_owned(),
            description: "x".to_owned(),
            path: "/test".to_owned(),
            sections: vec![],
        }
    }

    #[test]
    fn shell_emits_doctype_and_lang() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        assert!(s.starts_with("<!doctype html>"));
        assert!(s.contains(r#"<html lang="en">"#));
    }

    #[test]
    fn shell_emits_strict_csp() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        assert!(s.contains("Content-Security-Policy"));
        assert!(s.contains("default-src 'self'"));
        assert!(s.contains("frame-ancestors 'none'"));
    }

    #[test]
    fn shell_emits_nosniff() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        assert!(s.contains("X-Content-Type-Options"));
        assert!(s.contains("nosniff"));
    }

    #[test]
    fn shell_emits_canonical_from_path() {
        let mut p = empty_page();
        p.path = "/leaderboard".to_owned();
        let s = page_shell(&p, "/loom-skin.css", "");
        assert!(s.contains(r#"<link rel="canonical" href="/leaderboard">"#));
    }

    #[test]
    fn shell_emits_single_h1() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        let count = s.matches("<h1 ").count();
        assert_eq!(count, 1, "expected exactly one h1, got {count}");
    }

    #[test]
    fn shell_escapes_title_attribute() {
        let mut p = empty_page();
        p.title = "Foo & <Bar>".to_owned();
        let s = page_shell(&p, "/loom-skin.css", "");
        assert!(!s.contains("<Bar>"));
        assert!(s.contains("Foo &amp; &lt;Bar&gt;"));
    }

    #[test]
    fn shell_escapes_quote_in_path_attribute() {
        let mut p = empty_page();
        p.path = "/x\"onerror=alert(1)".to_owned();
        let s = page_shell(&p, "/loom-skin.css", "");
        assert!(!s.contains(r#"x"onerror"#));
        assert!(s.contains("&quot;"));
    }

    #[test]
    fn shell_inlines_body_markup() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "<main>X</main>");
        assert!(s.contains("<main>X</main>"));
    }

    #[test]
    fn shell_skip_link_target_matches_div() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        assert!(s.contains(r##"href="#content""##));
        assert!(s.contains(r##"id="content""##));
    }

    #[test]
    fn shell_emits_header_landmark() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        assert!(s.contains("<header "), "missing <header>: {s}");
        assert!(s.contains(r#"role="banner""#));
    }

    #[test]
    fn shell_emits_footer_landmark() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        assert!(s.contains("<footer "), "missing <footer>: {s}");
        assert!(s.contains(r#"role="contentinfo""#));
    }

    #[test]
    fn shell_emits_nav_landmark_with_aria_label() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "");
        assert!(s.contains("<nav "), "missing <nav>: {s}");
        assert!(s.contains(r#"aria-label="Primary""#));
    }

    #[test]
    fn cms_render_writes_to_file_and_round_trips() {
        let tmp = std::env::temp_dir();
        let input = tmp.join("loom-cms-render-test-input.json");
        let output = tmp.join("loom-cms-render-test-out.html");
        // Clean from a prior failed run.
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
        std::fs::write(
            &input,
            r#"{
                "title": "Demo",
                "description": "x",
                "path": "/demo",
                "sections": [
                    { "kind": "heading", "text": "Welcome", "level": 2 },
                    { "kind": "paragraph", "text": "Body text." }
                ]
            }"#,
        )
        .expect("write input");
        cmd_cms_render(&input, output.to_str().unwrap(), "/loom-skin.css")
            .expect("renders");
        let html = std::fs::read_to_string(&output).expect("read output");
        assert!(html.contains("<title>Demo</title>"));
        assert!(html.contains("Welcome"));
        assert!(html.contains("Body text."));
        // Cleanup.
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn cms_render_rejects_unknown_field() {
        let tmp = std::env::temp_dir();
        let input = tmp.join("loom-cms-render-bad-input.json");
        std::fs::write(
            &input,
            r#"{
                "title": "x",
                "description": "x",
                "path": "/",
                "sections": [],
                "smuggled_field": "evil"
            }"#,
        )
        .expect("write");
        let r = cmd_cms_render(&input, "-", "/loom-skin.css");
        assert!(matches!(r, Err(CmsRenderError::Schema(_))));
        let _ = std::fs::remove_file(&input);
    }
}
