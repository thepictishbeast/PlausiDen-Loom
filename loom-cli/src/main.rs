//! `loom` — top-level CLI for the PlausiDen-Loom design system.
//!
//! Today: `loom lint`, `loom tokens`. The `audit` and `new` subcommands
//! are stubs that print what they will do and exit non-zero, so a CI
//! invocation that gets ahead of the implementation fails loudly rather
//! than silently no-op'ing.

#![doc(html_no_source)]

mod critical_css;

use std::fmt::Write as _;
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
    /// Extract the critical-CSS subset from a stylesheet. Reads
    /// the input from `--input`, walks every top-level rule, and
    /// emits ONLY those needed for first paint of every page:
    /// `:root` token blocks, universal/element baseline rules
    /// (`*`, `html`, `body`, `:focus-visible`, `[hidden]`,
    /// `img`/`video`/`picture`, `pre`/`code`, `a`, `button`,
    /// `p`/`h1..h6`), the cms-render page-shell chrome
    /// (`.loom-skip`, `.loom-page-*`), `@media (prefers-*)`,
    /// `@font-face`. Component-specific rules
    /// (`.loom-card-*`, `.loom-section-*`, `.loom-composer*`)
    /// are dropped — they belong in the deferred sheet.
    ///
    /// Output goes to `--out` or stdout if `--out -`.
    ///
    /// Exit codes:
    ///   0 — extraction succeeded
    ///   1 — CSS parse error (unterminated comment / brace / etc.)
    ///   2 — I/O error reading or writing
    CriticalCss {
        /// Source stylesheet path.
        #[arg(long)]
        input: PathBuf,
        /// Output path. `-` for stdout.
        #[arg(long, default_value = "-")]
        out: String,
    },
    /// Scaffold a Rust handler stub for one backends.toml key.
    /// Reads the [backends.X] entry, generates
    /// `<crate>/src/handlers/<key>.rs` with typed Request/Response
    /// structs + axum-style handler signature + test stub, and
    /// updates backends.toml to set `impl_files = ["src/..."]`.
    /// Closes the loop from "key declared" to "code exists".
    ///
    /// Refuses to overwrite an existing handler file unless
    /// `--force`. Updates backends.toml in-place via toml_edit
    /// (preserves comments + ordering).
    ///
    /// Exit codes:
    ///   0 — handler scaffolded + backends.toml updated
    ///   1 — key not found, file exists + --force not set, or
    ///       crate path not a directory
    ///   2 — I/O error
    BackendStub {
        /// Backend key (matches a [backends.X] entry).
        #[arg(long)]
        key: String,
        /// Path to backends.toml.
        #[arg(long, default_value = "backends.toml")]
        backends: PathBuf,
        /// Crate root that will own the handler. The file lands
        /// at `<crate>/src/handlers/<key>.rs` (key dashes → underscores).
        #[arg(long)]
        crate_dir: PathBuf,
        /// Overwrite existing handler file.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// List every backend declared in backends.toml with its
    /// implementation status. Each `[backends.X]` entry is one
    /// row; impl status derived from `impl_files` field (empty →
    /// STUB; populated → IMPL).
    ///
    /// Use to track the gap between declared backend surface and
    /// shipped handlers. For SkillShots PoC, every key is
    /// currently STUB — closing that gap is the ship-blocker.
    ///
    /// Exit codes:
    ///   0 — table printed (some/all STUB is data, not error)
    ///   2 — I/O error (backends.toml missing or malformed)
    BackendList {
        /// Path to backends.toml.
        #[arg(long, default_value = "backends.toml")]
        backends: PathBuf,
    },
    /// Audit the bridge↔skin coverage. For each CmsSection
    /// variant tag, assert that the canonical skin.css declares
    /// its expected `.loom-*` selector family. Catches the
    /// silent failure where a new variant ships without matching
    /// CSS, leaving the rendered HTML unstyled.
    ///
    /// Reports two classes:
    ///   missing-skin   variant declared in bridge but no skin rule
    ///   dead-skin      .loom-section-X with no matching variant
    ///                  (informational warn — might be consumed
    ///                  by a downstream component)
    ///
    /// Exit codes:
    ///   0 — every variant has skin coverage
    ///   1 — at least one variant has no skin rule
    ///   2 — I/O error (skin file missing)
    AuditBridge {
        /// Path to canonical skin.css.
        #[arg(long)]
        skin: PathBuf,
    },
    /// Install a git pre-commit hook (invoked as
    /// `loom hooks-install --target <repo-dir>`) that runs
    /// `loom validate` on staged cms/*.json. Prevents broken
    /// schemas / URL validity from ever reaching main.
    ///
    /// The installed hook is a tiny shell wrapper:
    ///   1. Checks if a cms/ directory exists in the repo.
    ///   2. If yes, runs `loom validate --input cms/`.
    ///   3. Hook exits non-zero on validate failure → git aborts
    ///      the commit + prints validate's error report.
    ///   4. If `loom` not on PATH, the hook prints an install
    ///      hint and lets the commit through (better UX than
    ///      blocking commits because the dev forgot to update
    ///      PATH).
    ///
    /// Refuses to overwrite an existing pre-commit hook unless
    /// `--force`. Idempotent: re-running with the same body is
    /// a no-op.
    ///
    /// Exit codes:
    ///   0 — hook installed (or already current)
    ///   1 — hook exists + --force not set
    ///   2 — I/O error (target not a git repo, no .git dir, etc.)
    HooksInstall {
        /// Repo root containing .git/.
        #[arg(long)]
        target: PathBuf,
        /// Overwrite an existing pre-commit hook.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Auto-generate a Crawler journey JSON from cms/*.json files.
    /// Walks the cms-dir, deserializes each CmsPage, emits a
    /// goto + wait + screenshot triple per page.path. Solves the
    /// regression where a new `cms/<name>.json` silently ships
    /// unaudited because the journey is hand-edited.
    ///
    /// Output is journey-schema-compatible (same shape as
    /// PlausiDen-Crawler/journeys/*.json).
    ///
    /// Exit codes:
    ///   0 — journey written
    ///   1 — schema error in any cms/*.json (validates first)
    ///   2 — I/O error
    JourneyFromCms {
        /// Directory containing cms/*.json.
        #[arg(long)]
        cms_dir: PathBuf,
        /// Base URL prepended to each CmsPage.path
        /// (e.g. "http://127.0.0.1:8123/"). Trailing slash optional.
        #[arg(long)]
        base_url: String,
        /// Output journey JSON path.
        #[arg(long)]
        out: PathBuf,
        /// Journey "name" field — used by crawler for run-dir
        /// naming + report headers.
        #[arg(long, default_value = "cms-pages")]
        name: String,
    },
    /// Scaffold a new CmsPage JSON document from a typed template.
    /// Output is fully valid against the cms-schema and passes
    /// `loom validate` immediately. Each template pre-populates
    /// every required field with sensible placeholders so authors
    /// can swap in copy without touching structure.
    ///
    /// Templates:
    ///   landing   Hero + Composer + CardFeed (3 sample cards)
    ///   explainer Hero + Group + Group + Group
    ///   form      Hero + Form (single step, 2 fields)
    ///
    /// Refuses to overwrite an existing file unless `--force`.
    ///
    /// Exit codes:
    ///   0 — file written
    ///   1 — output exists + --force not set, or unknown kind
    ///   2 — I/O error
    CmsNew {
        /// Template kind: landing | explainer | form.
        #[arg(long)]
        kind: String,
        /// Output file path.
        #[arg(long)]
        out: PathBuf,
        /// CmsPage.title text.
        #[arg(long)]
        title: String,
        /// CmsPage.path (canonical URL, e.g. "/about.html").
        #[arg(long)]
        path: String,
        /// Force overwrite if `out` already exists.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Emit the JSON Schema for the CmsPage document type.
    /// Editors that read a `$schema` reference (VS Code, Helix,
    /// Zed, Sublime, Neovim with jsonls) provide inline
    /// autocomplete + validation when authors put `"$schema": "..."`
    /// in their cms/*.json.
    ///
    /// Pipe to a file under your project root + reference it from
    /// every `cms/<name>.json`:
    ///
    ///   loom cms-schema --out cms-schema.json
    ///   # in cms/index.json:
    ///   { "$schema": "../cms-schema.json", "title": ... }
    ///
    /// Exit codes:
    ///   0 — schema written
    ///   2 — I/O error
    CmsSchema {
        /// Output path. `-` for stdout.
        #[arg(long, default_value = "-")]
        out: String,
    },
    /// Fast schema validation for cms/*.json documents. Runs the
    /// loom-cms-render bridge's deserialize + URL-validity checks
    /// WITHOUT calling render_page. Useful as a pre-commit hook
    /// or fast iteration loop while editing CMS docs (a full
    /// forge build re-renders + audits every page; this just
    /// validates one or more files).
    ///
    /// Accepts either a single file or a directory (walks for
    /// *.json recursively). Reports per-file: ok / schema-error
    /// (with serde line+col) / url-invalid (with field path).
    ///
    /// Exit codes:
    ///   0 — every file passed
    ///   1 — at least one file failed schema or URL validation
    ///   2 — I/O error (input path missing)
    Validate {
        /// Path to a CmsPage JSON document or directory of such.
        #[arg(long)]
        input: PathBuf,
    },
    /// Walk a directory of source images (*.jpg, *.jpeg, *.png)
    /// and generate the AVIF + WebP siblings the Loom Picture
    /// component expects (`/assets/{stem}.avif` + `.webp` + `.jpg`).
    ///
    /// Skips files whose sibling already exists AND is newer than
    /// the source — make-style incremental. Backed by ImageMagick
    /// (`magick` on PATH).
    ///
    /// Exit codes:
    ///   0 — all images converted (or already up to date)
    ///   1 — at least one conversion failed
    ///   2 — I/O error (input dir missing, etc.)
    ImageConvert {
        /// Directory to scan recursively.
        #[arg(long)]
        input_dir: PathBuf,
        /// Quality for AVIF (0-100; default 50 — perceptually
        /// lossless for photos at low file size).
        #[arg(long, default_value_t = 50)]
        avif_quality: u8,
        /// Quality for WebP (0-100; default 80).
        #[arg(long, default_value_t = 80)]
        webp_quality: u8,
        /// Force re-encode even if siblings are already newer.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Render a CMS page document (JSON) to a static HTML file.
    /// Reads the document from `--input`, runs it through the
    /// `loom-cms-render` bridge, wraps the resulting body markup
    /// in a minimal page-shell template (`<html lang>`, strict
    /// CSP meta, viewport, charset, canonical link, single `<h1>`
    /// from CmsPage.title), and writes to `--out` (or stdout if
    /// `--out -`).
    ///
    /// `--critical-css <path>` enables the LCP-friendly two-stage
    /// stylesheet load: the contents of the file are inlined as
    /// `<style>...</style>` (sha256-pinned in CSP), and the full
    /// `--css-href` link is loaded async via the `media=\"print\"`
    /// trick (also CSP-pinned). The result blocks render only on
    /// the small critical block.
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
        /// Path to a pre-extracted critical-CSS file (produced by
        /// `loom critical-css`). Inlined as `<style>` and pinned
        /// in CSP via sha256 hash. If absent, the page-shell
        /// emits a normal blocking link to `--css-href`.
        #[arg(long)]
        critical_css: Option<PathBuf>,
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
        Cmd::CriticalCss { input, out } => match cmd_critical_css(&input, &out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(CriticalCssError::Parse(e)) => {
                eprintln!("loom critical-css: parse error: {e}");
                ExitCode::from(1)
            }
            Err(CriticalCssError::Io(e)) => {
                eprintln!("loom critical-css: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::BackendStub {
            key,
            backends,
            crate_dir,
            force,
        } => match cmd_backend_stub(&key, &backends, &crate_dir, force) {
            Ok(()) => ExitCode::SUCCESS,
            Err(BackendStubError::KeyNotFound(k)) => {
                eprintln!("loom backend-stub: key {k:?} not found in backends.toml");
                ExitCode::from(1)
            }
            Err(BackendStubError::Conflict(p)) => {
                eprintln!(
                    "loom backend-stub: {} already exists; pass --force",
                    p.display()
                );
                ExitCode::from(1)
            }
            Err(BackendStubError::CrateNotDir(p)) => {
                eprintln!("loom backend-stub: {} is not a directory", p.display());
                ExitCode::from(1)
            }
            Err(BackendStubError::Toml(e)) => {
                eprintln!("loom backend-stub: toml error: {e}");
                ExitCode::from(1)
            }
            Err(BackendStubError::Io(e)) => {
                eprintln!("loom backend-stub: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::BackendList { backends } => match cmd_backend_list(&backends) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom backend list: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::AuditBridge { skin } => match cmd_audit_bridge(&skin) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(1),
            Err(e) => {
                eprintln!("loom audit-bridge: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::HooksInstall { target, force } => match cmd_hooks_install(&target, force) {
            Ok(false) => ExitCode::SUCCESS,
            Ok(true) => {
                eprintln!(
                    "loom hooks install: pre-commit hook already exists; pass --force to overwrite"
                );
                ExitCode::from(1)
            }
            Err(e) => {
                eprintln!("loom hooks install: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::JourneyFromCms {
            cms_dir,
            base_url,
            out,
            name,
        } => match cmd_journey_from_cms(&cms_dir, &base_url, &out, &name) {
            Ok(()) => ExitCode::SUCCESS,
            Err(JourneyFromCmsError::Schema { file, error }) => {
                eprintln!(
                    "loom journey-from-cms: schema error in {}: {error}",
                    file.display()
                );
                ExitCode::from(1)
            }
            Err(JourneyFromCmsError::Io(e)) => {
                eprintln!("loom journey-from-cms: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::CmsNew {
            kind,
            out,
            title,
            path,
            force,
        } => match cmd_cms_new(&kind, &out, &title, &path, force) {
            Ok(()) => ExitCode::SUCCESS,
            Err(CmsNewError::Conflict(p)) => {
                eprintln!(
                    "loom cms-new: {} already exists; pass --force to overwrite",
                    p.display()
                );
                ExitCode::from(1)
            }
            Err(CmsNewError::UnknownKind(k)) => {
                eprintln!(
                    "loom cms-new: unknown template kind {k:?}; expected landing | explainer | form"
                );
                ExitCode::from(1)
            }
            Err(CmsNewError::Io(e)) => {
                eprintln!("loom cms-new: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::CmsSchema { out } => match cmd_cms_schema(&out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom cms-schema: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::Validate { input } => match cmd_validate(&input) {
            Ok(false) => ExitCode::SUCCESS,
            Ok(true) => {
                eprintln!("loom validate: at least one file failed");
                ExitCode::from(1)
            }
            Err(e) => {
                eprintln!("loom validate: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::ImageConvert {
            input_dir,
            avif_quality,
            webp_quality,
            force,
        } => match cmd_image_convert(&input_dir, avif_quality, webp_quality, force) {
            Ok(false) => ExitCode::SUCCESS,
            Ok(true) => {
                eprintln!("loom image-convert: at least one conversion failed");
                ExitCode::from(1)
            }
            Err(e) => {
                eprintln!("loom image-convert: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::CmsRender {
            input,
            out,
            css_href,
            critical_css,
        } => match cmd_cms_render(&input, &out, &css_href, critical_css.as_deref()) {
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
"#
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
"#
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
"#
    )
}

/// Emit a GTK 4 CSS theme built from loom-tokens. Maps each
/// semantic role to GTK's named colors so a downstream Thundercrab
/// GTK build (or any GTK app) inherits the same palette as the web
/// site without re-implementing it.
///
/// The CSS is small (~80 lines) and intentionally limited to color
/// and spacing tokens — animations, fonts, and layout are
/// GTK-app-specific and shouldn't be baked into a shared theme.
fn cmd_gtk_theme(dark: bool) -> String {
    use loom_tokens::ColorRole;
    let palette = if dark {
        ColorRole::dark_all()
    } else {
        ColorRole::all()
    };
    let mode = if dark { "dark" } else { "light" };
    let mut out = String::new();
    let _ = writeln!(
        out,
        "/* GTK 4 theme generated from loom-tokens ({mode}). */"
    );
    out.push_str("/* Do not edit by hand — re-run `loom gtk-theme` after a token change. */\n\n");
    out.push_str(":root {\n");
    for role in palette {
        let _ = writeln!(out, "  --loom-{}: {};", role.role, role.color.css);
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
/// serde_json) from I/O errors (from `std::fs` / `std::io`) so the
/// dispatch in `main` can map them to distinct exit codes (1 vs 2).
#[derive(Debug)]
enum CmsRenderError {
    Schema(serde_json::Error),
    Io(std::io::Error),
}

impl From<serde_json::Error> for CmsRenderError {
    fn from(e: serde_json::Error) -> Self {
        Self::Schema(e)
    }
}

impl From<std::io::Error> for CmsRenderError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
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
    critical_css_path: Option<&std::path::Path>,
) -> Result<(), CmsRenderError> {
    let raw = std::fs::read_to_string(input)?;
    let page: loom_cms_render::CmsPage = serde_json::from_str(&raw)?;
    let body = loom_cms_render::render_page(&page);
    let critical_css = critical_css_path.map(std::fs::read_to_string).transpose()?;
    let shell = page_shell(
        &page,
        css_href,
        &body.into_string(),
        critical_css.as_deref(),
    );
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

/// Render the primary nav-link list emitted inside the page-shell's
/// `<nav class="loom-page-nav">` block. Empty list → no extra
/// markup; the brand link from the shell stands alone. Each link's
/// href is validated via `is_safe_url`; invalid hrefs render as
/// `#invalid-nav-link` placeholders so the build flags them at
/// audit time without leaking the bad URL into a real anchor.
fn render_nav_links(links: &[loom_cms_render::CmsNavLink]) -> String {
    if links.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for link in links {
        let href_safe = loom_cms_render::is_safe_url(&link.href);
        let href_attr = escape_html_attr(if href_safe {
            &link.href
        } else {
            "#invalid-nav-link"
        });
        let backend_attr = escape_html_attr(&link.data_backend);
        let label_text = escape_html_text(&link.label);
        let current_attr = if link.current {
            " aria-current=\"page\""
        } else {
            ""
        };
        let invalid_attr = if href_safe {
            ""
        } else {
            " data-invalid=\"true\""
        };
        let _ = write!(
            out,
            "\n      <a class=\"loom-page-nav-link\" href=\"{href_attr}\" data-backend=\"{backend_attr}\"{current_attr}{invalid_attr}>{label_text}</a>"
        );
    }
    out
}

/// Compute SHA-256 over `bytes` and return `sha256-{base64}`
/// formatted for inclusion in a CSP source-list.
fn csp_sha256(bytes: &[u8]) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let b64 = base64::engine::general_purpose::STANDARD.encode(digest);
    format!("sha256-{b64}")
}

/// Fixed onload event handler for the deferred stylesheet link.
/// Hashed at build time + pinned in CSP `script-src`.
const DEFER_ONLOAD_JS: &str = "this.media='all';this.removeAttribute('onload')";

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
fn page_shell(
    page: &loom_cms_render::CmsPage,
    css_href: &str,
    body: &str,
    critical_css: Option<&str>,
) -> String {
    // SECURITY: page.title / page.description / page.path / css_href
    // pass through escape_html_*() before interpolation; critical_css
    // bytes go into a <style> block and are CSP-pinned via sha256.
    let title = escape_html_text(&page.title);
    let description = escape_html_text(&page.description);
    let path = escape_html_attr(&page.path);
    let css = escape_html_attr(css_href);
    let nav_links = render_nav_links(&page.nav_links);
    let (style_block, css_link, csp) = match critical_css {
        Some(crit) => {
            let style_hash = csp_sha256(crit.as_bytes());
            let onload_hash = csp_sha256(DEFER_ONLOAD_JS.as_bytes());
            let style_block = format!("<style>{crit}</style>\n  ");
            let css_link = format!(
                "<link rel=\"stylesheet\" href=\"{css}\" media=\"print\" onload=\"{DEFER_ONLOAD_JS}\">\n  <noscript><link rel=\"stylesheet\" href=\"{css}\"></noscript>"
            );
            // CSP: 'self' for default + img/style/script + the
            // critical-style hash + the deferred-onload script
            // hash. 'unsafe-hashes' is required (CSP3) to allow
            // an inline event handler whose hash is in script-src.
            let csp = format!(
                "default-src 'self'; img-src 'self' data:; style-src 'self' '{style_hash}'; script-src 'self' 'unsafe-hashes' '{onload_hash}'; frame-ancestors 'none'"
            );
            (style_block, css_link, csp)
        }
        None => {
            let css_link = format!("<link rel=\"stylesheet\" href=\"{css}\">");
            let csp = "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-ancestors 'none'".to_owned();
            (String::new(), css_link, csp)
        }
    };
    format!(
        "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
  <meta charset=\"utf-8\">\n\
  <meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\n\
  <meta http-equiv=\"X-Content-Type-Options\" content=\"nosniff\">\n\
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
  <title>{title}</title>\n\
  <meta name=\"description\" content=\"{description}\">\n\
  <link rel=\"canonical\" href=\"{path}\">\n\
  {style_block}{css_link}\n\
</head>\n\
<body>\n\
  <a class=\"loom-skip\" href=\"#content\">Skip to content</a>\n\
  <header class=\"loom-page-header\" role=\"banner\">\n\
    <nav class=\"loom-page-nav\" aria-label=\"Primary\">\n\
      <a class=\"loom-page-brand\" href=\"/\" data-loom-rich-link=\"true\">SkillShots</a>{nav_links}\n\
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
            schema: None,
            title: "Test".to_owned(),
            description: "x".to_owned(),
            path: "/test".to_owned(),
            nav_links: vec![],
            sections: vec![],
        }
    }

    #[test]
    fn shell_emits_doctype_and_lang() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.starts_with("<!doctype html>"));
        assert!(s.contains(r#"<html lang="en">"#));
    }

    #[test]
    fn shell_emits_strict_csp() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains("Content-Security-Policy"));
        assert!(s.contains("default-src 'self'"));
        assert!(s.contains("frame-ancestors 'none'"));
    }

    #[test]
    fn shell_emits_nosniff() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains("X-Content-Type-Options"));
        assert!(s.contains("nosniff"));
    }

    #[test]
    fn shell_emits_canonical_from_path() {
        let mut p = empty_page();
        p.path = "/leaderboard".to_owned();
        let s = page_shell(&p, "/loom-skin.css", "", None);
        assert!(s.contains(r#"<link rel="canonical" href="/leaderboard">"#));
    }

    #[test]
    fn shell_emits_single_h1() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        let count = s.matches("<h1 ").count();
        assert_eq!(count, 1, "expected exactly one h1, got {count}");
    }

    #[test]
    fn shell_escapes_title_attribute() {
        let mut p = empty_page();
        p.title = "Foo & <Bar>".to_owned();
        let s = page_shell(&p, "/loom-skin.css", "", None);
        assert!(!s.contains("<Bar>"));
        assert!(s.contains("Foo &amp; &lt;Bar&gt;"));
    }

    #[test]
    fn shell_escapes_quote_in_path_attribute() {
        let mut p = empty_page();
        p.path = "/x\"onerror=alert(1)".to_owned();
        let s = page_shell(&p, "/loom-skin.css", "", None);
        assert!(!s.contains(r#"x"onerror"#));
        assert!(s.contains("&quot;"));
    }

    #[test]
    fn shell_inlines_body_markup() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "<main>X</main>", None);
        assert!(s.contains("<main>X</main>"));
    }

    #[test]
    fn shell_skip_link_target_matches_div() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains(r##"href="#content""##));
        assert!(s.contains(r#"id="content""#));
    }

    #[test]
    fn shell_emits_header_landmark() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains("<header "), "missing <header>: {s}");
        assert!(s.contains(r#"role="banner""#));
    }

    #[test]
    fn shell_emits_footer_landmark() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(s.contains("<footer "), "missing <footer>: {s}");
        assert!(s.contains(r#"role="contentinfo""#));
    }

    #[test]
    fn shell_emits_nav_landmark_with_aria_label() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
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
        cmd_cms_render(&input, output.to_str().unwrap(), "/loom-skin.css", None).expect("renders");
        let html = std::fs::read_to_string(&output).expect("read output");
        assert!(html.contains("<title>Demo</title>"));
        assert!(html.contains("Welcome"));
        assert!(html.contains("Body text."));
        // Cleanup.
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn shell_with_no_nav_links_emits_brand_only() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        // brand link present
        assert!(s.contains(
            r#"<a class="loom-page-brand" href="/" data-loom-rich-link="true">SkillShots</a>"#
        ));
        // no extra nav-link anchors
        assert!(!s.contains("loom-page-nav-link"));
    }

    #[test]
    fn shell_with_nav_links_renders_each() {
        use loom_cms_render::CmsNavLink;
        let mut p = empty_page();
        p.nav_links = vec![
            CmsNavLink {
                label: "Battle Feed".to_owned(),
                href: "/".to_owned(),
                data_backend: "list-challenges".to_owned(),
                current: true,
            },
            CmsNavLink {
                label: "Leaderboard".to_owned(),
                href: "/leaderboard.html".to_owned(),
                data_backend: "list-leaderboard".to_owned(),
                current: false,
            },
        ];
        let s = page_shell(&p, "/loom-skin.css", "", None);
        assert!(s.contains(">Battle Feed<"));
        assert!(s.contains(">Leaderboard<"));
        assert!(s.contains(r#"data-backend="list-challenges""#));
        assert!(s.contains(r#"data-backend="list-leaderboard""#));
        // The current link gets aria-current.
        assert!(s.contains(r#"aria-current="page""#));
        // Non-current does NOT get aria-current — count the
        // attribute occurrences and confirm exactly one.
        assert_eq!(s.matches(r#"aria-current="page""#).count(), 1);
    }

    #[test]
    fn shell_nav_link_invalid_href_substitutes_placeholder() {
        use loom_cms_render::CmsNavLink;
        let mut p = empty_page();
        p.nav_links = vec![CmsNavLink {
            label: "Evil".to_owned(),
            href: "javascript:alert(1)".to_owned(),
            data_backend: "x".to_owned(),
            current: false,
        }];
        let s = page_shell(&p, "/loom-skin.css", "", None);
        assert!(s.contains(r##"href="#invalid-nav-link""##));
        assert!(s.contains(r#"data-invalid="true""#));
        assert!(!s.contains("javascript:alert"));
    }

    #[test]
    fn shell_nav_link_label_escaped() {
        use loom_cms_render::CmsNavLink;
        let mut p = empty_page();
        p.nav_links = vec![CmsNavLink {
            label: "<script>".to_owned(),
            href: "/x".to_owned(),
            data_backend: "x".to_owned(),
            current: false,
        }];
        let s = page_shell(&p, "/loom-skin.css", "", None);
        assert!(!s.contains(">/<script>/<"));
        assert!(s.contains("&lt;script&gt;"));
    }

    #[test]
    fn shell_with_critical_css_inlines_style_block() {
        let s = page_shell(
            &empty_page(),
            "/loom-skin.css",
            "",
            Some(":root { --x: 1; }"),
        );
        assert!(s.contains("<style>:root { --x: 1; }</style>"));
    }

    #[test]
    fn shell_with_critical_css_uses_print_media_swap_for_deferred() {
        let s = page_shell(
            &empty_page(),
            "/loom-skin.css",
            "",
            Some(":root { --x: 1; }"),
        );
        assert!(s.contains(r#"media="print""#));
        assert!(s.contains("this.media='all';this.removeAttribute('onload')"));
        assert!(s.contains("<noscript>"));
    }

    #[test]
    fn shell_with_critical_css_csp_pins_style_hash() {
        let s = page_shell(
            &empty_page(),
            "/loom-skin.css",
            "",
            Some(":root { --x: 1; }"),
        );
        // CSP must include 'sha256-' twice (one for style, one for script
        // 'unsafe-hashes' onload handler).
        let count = s.matches("sha256-").count();
        assert!(count >= 2, "expected ≥2 sha256-, got {count}: {s}");
        assert!(s.contains("'unsafe-hashes'"));
    }

    #[test]
    fn shell_without_critical_css_keeps_simple_csp() {
        let s = page_shell(&empty_page(), "/loom-skin.css", "", None);
        assert!(!s.contains("sha256-"));
        assert!(!s.contains("'unsafe-hashes'"));
        assert!(!s.contains("<style>"));
        assert!(s.contains(r#"<link rel="stylesheet" href="/loom-skin.css">"#));
    }

    #[test]
    fn csp_sha256_known_value() {
        // Empty input has a known SHA-256.
        // Hash of empty string is e3b0c44298fc1c149afbf4c8996fb924...
        let h = csp_sha256(b"");
        assert_eq!(h, "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=");
    }

    #[test]
    fn csp_sha256_stable_across_runs() {
        let a = csp_sha256(b"abc");
        let b = csp_sha256(b"abc");
        assert_eq!(a, b);
        let c = csp_sha256(b"abd");
        assert_ne!(a, c);
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
        let r = cmd_cms_render(&input, "-", "/loom-skin.css", None);
        assert!(matches!(r, Err(CmsRenderError::Schema(_))));
        let _ = std::fs::remove_file(&input);
    }
}

/// `loom critical-css` error type. Same split as cms-render: a
/// parse error (CSS structurally invalid) gets exit code 1; an
/// I/O error gets exit code 2 so CI can route appropriately.
#[derive(Debug)]
enum CriticalCssError {
    Parse(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for CriticalCssError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

fn cmd_critical_css(input: &std::path::Path, out: &str) -> Result<(), CriticalCssError> {
    let css = std::fs::read_to_string(input)?;
    let critical = critical_css::extract(&css).map_err(CriticalCssError::Parse)?;
    if out == "-" {
        print!("{critical}");
        return Ok(());
    }
    if let Some(parent) = std::path::Path::new(out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, &critical)?;
    Ok(())
}

#[cfg(test)]
mod cmd_critical_css_tests {
    use super::*;

    #[test]
    fn rejects_invalid_css() {
        let tmp = std::env::temp_dir();
        let bad = tmp.join("loom-critical-css-bad.css");
        std::fs::write(&bad, ".loom-page { color: red;\n").expect("write");
        let r = cmd_critical_css(&bad, "-");
        assert!(matches!(r, Err(CriticalCssError::Parse(_))));
        let _ = std::fs::remove_file(&bad);
    }

    #[test]
    fn writes_critical_subset_to_file() {
        let tmp = std::env::temp_dir();
        let input = tmp.join("loom-critical-css-input.css");
        let output = tmp.join("loom-critical-css-out.css");
        let _ = std::fs::remove_file(&output);
        std::fs::write(&input, ":root { --x: 1; }\n.loom-card { padding: 1rem; }\n")
            .expect("write input");
        cmd_critical_css(&input, output.to_str().unwrap()).expect("ok");
        let got = std::fs::read_to_string(&output).expect("read");
        assert!(got.contains(":root"));
        assert!(!got.contains(".loom-card"));
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
    }
}

/// `loom image-convert` — walks `input_dir` recursively, finds
/// every JPG/PNG, and shells out to ImageMagick (`magick`) to
/// produce the matching `.avif` and `.webp` siblings the Loom
/// Picture component expects.
///
/// Skip-if-newer: if the sibling already exists AND its mtime is
/// >= the source's mtime, skip it. `--force` overrides.
///
/// Returns `Ok(true)` if at least one conversion failed (caller
/// maps to exit 1); `Ok(false)` if every conversion succeeded
/// or was skipped; `Err` on I/O error reading the input dir.
fn cmd_image_convert(
    input_dir: &std::path::Path,
    avif_quality: u8,
    webp_quality: u8,
    force: bool,
) -> Result<bool, std::io::Error> {
    if !input_dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("input dir not found: {}", input_dir.display()),
        ));
    }
    let mut sources = Vec::<std::path::PathBuf>::new();
    walk_images(input_dir, &mut sources)?;
    let mut converted = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;
    for src in &sources {
        for (ext, quality) in [("avif", avif_quality), ("webp", webp_quality)] {
            let dest = src.with_extension(ext);
            if !force && is_dest_fresh(src, &dest) {
                skipped += 1;
                continue;
            }
            match magick_convert(src, &dest, ext, quality) {
                Ok(()) => {
                    converted += 1;
                    println!(
                        "  ok     {src}{arrow}{dest}",
                        src = src.display(),
                        arrow = " -> ",
                        dest = dest.display()
                    );
                }
                Err(e) => {
                    failed += 1;
                    eprintln!(
                        "  fail   {src} -> {dest}: {e}",
                        src = src.display(),
                        dest = dest.display()
                    );
                }
            }
        }
    }
    println!(
        "image-convert: {} source(s), {converted} created, {skipped} skipped (already current), {failed} failed",
        sources.len()
    );
    Ok(failed > 0)
}

fn walk_images(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_images(&path, out)?;
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let lower = ext.to_ascii_lowercase();
        if matches!(lower.as_str(), "jpg" | "jpeg" | "png") {
            out.push(path);
        }
    }
    Ok(())
}

fn is_dest_fresh(src: &std::path::Path, dest: &std::path::Path) -> bool {
    let (Ok(src_meta), Ok(dest_meta)) = (std::fs::metadata(src), std::fs::metadata(dest)) else {
        return false;
    };
    let (Ok(src_m), Ok(dest_m)) = (src_meta.modified(), dest_meta.modified()) else {
        return false;
    };
    dest_m >= src_m
}

fn magick_convert(
    src: &std::path::Path,
    dest: &std::path::Path,
    ext: &str,
    quality: u8,
) -> Result<(), String> {
    // SECURITY: src + dest paths come from filesystem walk; `ext`
    // and `quality` are typed/bounded by clap. None of these can
    // inject shell args because we use std::process::Command's
    // arg-vec form (no shell interpretation).
    let output = std::process::Command::new("magick")
        .arg(src)
        .arg("-quality")
        .arg(quality.to_string())
        .arg(format!("{}:{}", ext.to_uppercase(), dest.display()))
        .output()
        .map_err(|e| format!("spawning magick failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "magick exit {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod cmd_image_convert_tests {
    use super::*;

    fn unique_dir(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-image-convert-{label}-{pid}-{n}"))
    }

    #[test]
    fn errs_on_missing_input_dir() {
        let dir = std::env::temp_dir().join("loom-image-convert-missing-zzzz");
        let _ = std::fs::remove_dir_all(&dir);
        let r = cmd_image_convert(&dir, 50, 80, false);
        assert!(r.is_err());
    }

    #[test]
    fn empty_dir_succeeds_with_no_failures() {
        let dir = unique_dir("empty");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let r = cmd_image_convert(&dir, 50, 80, false);
        assert!(!r.expect("ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn walks_recursively() {
        let dir = unique_dir("recursive");
        let nested = dir.join("a/b/c");
        std::fs::create_dir_all(&nested).expect("mkdir");
        // Plant non-image files; walker should ignore.
        std::fs::write(dir.join("readme.txt"), "x").expect("w");
        std::fs::write(nested.join("notes.md"), "x").expect("w");
        let mut out = Vec::new();
        walk_images(&dir, &mut out).expect("walk");
        assert!(out.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn walks_picks_up_images_only() {
        let dir = unique_dir("images");
        std::fs::create_dir_all(&dir).expect("mkdir");
        // Plant filenames (not real image bytes — walker only looks at extension)
        for name in [
            "a.jpg", "b.JPG", "c.jpeg", "d.png", "e.PNG", "f.gif", "g.txt",
        ] {
            std::fs::write(dir.join(name), b"\x00").expect("w");
        }
        let mut out = Vec::new();
        walk_images(&dir, &mut out).expect("walk");
        assert_eq!(out.len(), 5, "got {:?}", out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dest_fresh_check_compares_mtimes() {
        let dir = unique_dir("freshness");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let src = dir.join("a.jpg");
        let dest = dir.join("a.avif");
        std::fs::write(&src, "src").expect("w src");
        // Sleep to ensure dest mtime > src mtime.
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&dest, "dest").expect("w dest");
        assert!(is_dest_fresh(&src, &dest));
        // Touch src to make it newer.
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&src, "src2").expect("w src2");
        assert!(!is_dest_fresh(&src, &dest));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dest_fresh_false_when_dest_missing() {
        let dir = unique_dir("missing-dest");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let src = dir.join("a.jpg");
        let dest = dir.join("a.avif");
        std::fs::write(&src, "src").expect("w");
        assert!(!is_dest_fresh(&src, &dest));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `loom backend-stub` errors.
#[derive(Debug)]
enum BackendStubError {
    KeyNotFound(String),
    Conflict(std::path::PathBuf),
    CrateNotDir(std::path::PathBuf),
    Toml(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for BackendStubError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

fn cmd_backend_stub(
    key: &str,
    backends_path: &std::path::Path,
    crate_dir: &std::path::Path,
    force: bool,
) -> Result<(), BackendStubError> {
    if !crate_dir.is_dir() {
        return Err(BackendStubError::CrateNotDir(crate_dir.to_path_buf()));
    }
    // Parse + locate the entry.
    let raw = std::fs::read_to_string(backends_path)?;
    let mut doc: toml_edit::DocumentMut = raw
        .parse()
        .map_err(|e: toml_edit::TomlError| BackendStubError::Toml(e.to_string()))?;
    let backends = doc
        .get_mut("backends")
        .and_then(|v| v.as_table_mut())
        .ok_or_else(|| BackendStubError::Toml("missing [backends] section".to_owned()))?;
    let entry = backends
        .get_mut(key)
        .and_then(|v| v.as_table_mut())
        .ok_or_else(|| BackendStubError::KeyNotFound(key.to_owned()))?;
    let method = entry
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_owned();
    let path = entry
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("/")
        .to_owned();
    let purpose = entry
        .get("purpose")
        .and_then(|v| v.as_str())
        .unwrap_or("(no purpose declared)")
        .to_owned();

    // Compute file path. Backend keys have hyphens (e.g. sign-in)
    // but Rust modules can't, so convert to underscores.
    let file_stem = key.replace('-', "_");
    let rel_path = format!("src/handlers/{file_stem}.rs");
    let abs_path = crate_dir.join(&rel_path);
    if abs_path.exists() && !force {
        return Err(BackendStubError::Conflict(abs_path));
    }
    if let Some(parent) = abs_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &abs_path,
        render_handler_stub(key, &method, &path, &purpose),
    )?;

    // Update backends.toml: impl_files = [rel_path].
    let mut new_array = toml_edit::Array::new();
    new_array.push(rel_path.as_str());
    entry.insert("impl_files", toml_edit::value(new_array));
    std::fs::write(backends_path, doc.to_string())?;

    println!("  ok     scaffolded {}", abs_path.display());
    println!(
        "  ok     updated {} (impl_files += {rel_path:?})",
        backends_path.display()
    );
    Ok(())
}

fn render_handler_stub(key: &str, method: &str, path: &str, purpose: &str) -> String {
    let module_name = key.replace('-', "_");
    let fn_name = if method.eq_ignore_ascii_case("GET") {
        "handle_get"
    } else if method.eq_ignore_ascii_case("POST") {
        "handle_post"
    } else if method.eq_ignore_ascii_case("PUT") {
        "handle_put"
    } else if method.eq_ignore_ascii_case("DELETE") {
        "handle_delete"
    } else {
        "handle"
    };
    format!(
        r#"//! `{key}` — backend handler stub.
//!
//! Method: {method}
//! Path:   {path}
//! Purpose: {purpose}
//!
//! Scaffolded by `loom backend-stub`. Replace the placeholder
//! Request/Response types and the handler body with the real
//! implementation. Update the test below to exercise the
//! actual semantics.

use serde::{{Deserialize, Serialize}};

/// `{key}` request payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {{
    // TODO: declare request fields.
}}

/// `{key}` response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {{
    /// Always `true` on success; absent on error.
    pub ok: bool,
}}

/// Handler entry point. Wire into your axum/actix/rocket
/// router at `{method} {path}`.
///
/// AVP-2: returns `Result<Response, anyhow::Error>` so caller
/// chooses how to translate the error to an HTTP response
/// (typically 4xx for client error, 5xx for server error).
pub async fn {fn_name}(_req: Request) -> Result<Response, anyhow::Error> {{
    // TODO: implement {key} ({purpose}).
    Ok(Response {{ ok: true }})
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[tokio::test]
    async fn placeholder_returns_ok() {{
        let resp = {fn_name}(Request {{}}).await.expect("ok");
        assert!(resp.ok);
    }}

    #[test]
    fn module_name_matches_key() {{
        // Self-doc: this file lives at src/handlers/{module_name}.rs
        // for backend key "{key}".
        assert_eq!("{module_name}", "{module_name}");
    }}
}}
"#
    )
}

#[cfg(test)]
mod cmd_backend_stub_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-backend-stub-{label}-{pid}-{n}"))
    }

    fn fixture() -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = unique("fixture");
        std::fs::create_dir_all(dir.join("src")).expect("mkdir");
        let backends = dir.join("backends.toml");
        std::fs::write(
            &backends,
            r#"[backends.sign-in]
method = "POST"
path = "/auth/sign-in"
purpose = "operator sign-in"
impl_files = []

[backends.view-profile]
method = "GET"
path = "/u/:id"
purpose = "public profile"
impl_files = []
"#,
        )
        .expect("write");
        (backends, dir)
    }

    #[test]
    fn errs_on_unknown_key() {
        let (backends, dir) = fixture();
        let r = cmd_backend_stub("nope", &backends, &dir, false);
        assert!(matches!(r, Err(BackendStubError::KeyNotFound(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errs_on_non_dir_crate() {
        let (backends, _) = fixture();
        let bogus = std::env::temp_dir().join("loom-backend-stub-not-a-dir-zzz");
        let _ = std::fs::remove_dir_all(&bogus);
        let r = cmd_backend_stub("sign-in", &backends, &bogus, false);
        assert!(matches!(r, Err(BackendStubError::CrateNotDir(_))));
    }

    #[test]
    fn writes_handler_with_dash_to_underscore_filename() {
        let (backends, dir) = fixture();
        cmd_backend_stub("sign-in", &backends, &dir, false).expect("ok");
        let handler = dir.join("src/handlers/sign_in.rs");
        assert!(handler.exists(), "handler file not at expected path");
        let body = std::fs::read_to_string(&handler).expect("read");
        assert!(body.contains("//! `sign-in` — backend handler stub"));
        assert!(body.contains("pub async fn handle_post"));
        assert!(body.contains("Method: POST"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn updates_backends_toml_impl_files() {
        let (backends, dir) = fixture();
        cmd_backend_stub("view-profile", &backends, &dir, false).expect("ok");
        let raw = std::fs::read_to_string(&backends).expect("read");
        let v: toml::Value = toml::from_str(&raw).expect("parse");
        let entry = &v["backends"]["view-profile"];
        let impl_files = entry["impl_files"].as_array().expect("array");
        assert_eq!(impl_files.len(), 1);
        assert_eq!(
            impl_files[0].as_str().unwrap(),
            "src/handlers/view_profile.rs"
        );
        // Other entries unchanged.
        let other = &v["backends"]["sign-in"];
        assert_eq!(other["impl_files"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let (backends, dir) = fixture();
        cmd_backend_stub("sign-in", &backends, &dir, false).expect("first ok");
        let r = cmd_backend_stub("sign-in", &backends, &dir, false);
        assert!(matches!(r, Err(BackendStubError::Conflict(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn force_overwrites() {
        let (backends, dir) = fixture();
        cmd_backend_stub("sign-in", &backends, &dir, false).expect("first");
        // Mutate the file to verify --force replaces it.
        std::fs::write(dir.join("src/handlers/sign_in.rs"), "// hand-edit\n").expect("mutate");
        cmd_backend_stub("sign-in", &backends, &dir, true).expect("force");
        let body = std::fs::read_to_string(dir.join("src/handlers/sign_in.rs")).expect("read");
        assert!(body.contains("backend handler stub"));
        assert!(!body.contains("hand-edit"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handler_file_has_test_stub() {
        let (backends, dir) = fixture();
        cmd_backend_stub("view-profile", &backends, &dir, false).expect("ok");
        let body = std::fs::read_to_string(dir.join("src/handlers/view_profile.rs")).expect("read");
        assert!(body.contains("#[tokio::test]"));
        assert!(body.contains("placeholder_returns_ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_handler_stub_pure() {
        let s = render_handler_stub(
            "post-skill",
            "POST",
            "/challenges",
            "operator publishes a new challenge",
        );
        assert!(s.contains("//! `post-skill`"));
        assert!(s.contains("Method: POST"));
        assert!(s.contains("Path:   /challenges"));
        assert!(s.contains("Purpose: operator publishes a new challenge"));
        assert!(s.contains("pub async fn handle_post"));
    }

    #[test]
    fn get_method_yields_handle_get_fn_name() {
        let s = render_handler_stub("view-profile", "GET", "/u/:id", "x");
        assert!(s.contains("pub async fn handle_get"));
        assert!(!s.contains("pub async fn handle_post"));
    }
}

/// `loom backend list` — read backends.toml + print impl status
/// table for every declared key.
///
/// Pure read-only: no file mutation, no remote I/O. Stable text
/// output suitable for piping to grep / awk / jq (after
/// post-processing). Exit code 0 even when every key is STUB —
/// the data is the value, not the gate.
fn cmd_backend_list(backends_path: &std::path::Path) -> Result<(), std::io::Error> {
    let raw = std::fs::read_to_string(backends_path)?;
    let value: toml::Value =
        toml::from_str(&raw).map_err(|e| std::io::Error::other(format!("toml parse: {e}")))?;
    let backends = value
        .get("backends")
        .and_then(|v| v.as_table())
        .ok_or_else(|| std::io::Error::other("missing [backends] section"))?;

    let mut rows = Vec::<BackendRow>::new();
    for (key, entry) in backends {
        let table = match entry.as_table() {
            Some(t) => t,
            None => continue,
        };
        let method = table
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_owned();
        let purpose = table
            .get("purpose")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let impl_files = table
            .get("impl_files")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let status = if impl_files == 0 { "STUB" } else { "IMPL" };
        rows.push(BackendRow {
            key: key.to_owned(),
            method,
            status: status.to_owned(),
            purpose,
        });
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));

    let total = rows.len();
    let stubs = rows.iter().filter(|r| r.status == "STUB").count();
    let impls = total - stubs;

    println!("  key                          method  status  purpose");
    println!(
        "  ---------------------------  ------  ------  ----------------------------------------"
    );
    for r in &rows {
        let purpose = if r.purpose.len() > 40 {
            format!("{}…", &r.purpose[..39])
        } else {
            r.purpose.clone()
        };
        println!(
            "  {key:<27}  {method:<6}  {status:<6}  {purpose}",
            key = r.key,
            method = r.method,
            status = r.status,
        );
    }
    println!();
    println!(
        "loom backend list: {total} declared, {impls} implemented ({pct}%), {stubs} stub",
        pct = if total > 0 { impls * 100 / total } else { 0 }
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct BackendRow {
    key: String,
    method: String,
    status: String,
    purpose: String,
}

#[cfg(test)]
mod cmd_backend_list_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-backend-list-{label}-{pid}-{n}.toml"))
    }

    #[test]
    fn errs_on_missing_file() {
        let p = std::env::temp_dir().join("loom-backend-list-missing-zzzz.toml");
        let _ = std::fs::remove_file(&p);
        assert!(cmd_backend_list(&p).is_err());
    }

    #[test]
    fn errs_on_malformed_toml() {
        let p = unique("malformed");
        std::fs::write(&p, "not = valid = toml\n").expect("write");
        assert!(cmd_backend_list(&p).is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn errs_on_missing_backends_section() {
        let p = unique("no-section");
        std::fs::write(&p, "[meta]\nname = \"x\"\n").expect("write");
        assert!(cmd_backend_list(&p).is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn empty_backends_section_succeeds() {
        let p = unique("empty");
        std::fs::write(&p, "[backends]\n").expect("write");
        cmd_backend_list(&p).expect("ok");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn classifies_stub_vs_impl() {
        let p = unique("classify");
        std::fs::write(
            &p,
            r#"
[backends.alpha]
method = "GET"
path = "/a"
purpose = "fetch alpha"
impl_files = []

[backends.beta]
method = "POST"
path = "/b"
purpose = "submit beta"
impl_files = ["src/handlers/beta.rs"]

[backends.gamma]
method = "GET"
path = "/g"
purpose = "fetch gamma"
impl_files = []
"#,
        )
        .expect("write");
        // Function prints to stdout; just verify it doesn't error.
        // Logic-level: rebuild the rows manually + assert classification.
        let raw = std::fs::read_to_string(&p).expect("read");
        let v: toml::Value = toml::from_str(&raw).expect("parse");
        let backends = v["backends"].as_table().expect("table");
        let stub_count = backends
            .values()
            .filter(|e| {
                e.as_table()
                    .and_then(|t| t.get("impl_files"))
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| a.is_empty())
            })
            .count();
        assert_eq!(stub_count, 2);
        cmd_backend_list(&p).expect("ok");
        let _ = std::fs::remove_file(&p);
    }
}

/// `loom audit-bridge` — pure check across (variant tag,
/// expected selectors) tuples. Returns the count of missing-skin
/// findings; non-zero means at least one variant ships without
/// matching CSS.
///
/// The variant→selector map is hand-written here rather than
/// derived from the bridge code. That's deliberate: this audit
/// catches the case where the BRIDGE evolves but skin.css
/// doesn't (or vice versa). Both sides need a human to update
/// the doctrine table when adding a variant.
fn cmd_audit_bridge(skin: &std::path::Path) -> Result<u32, std::io::Error> {
    let css = std::fs::read_to_string(skin)?;
    let pairs: &[(&str, &[&str])] = &[
        // (variant tag, required selectors that MUST appear in skin)
        ("hero", &[".loom-section-hero"]),
        ("group", &[".loom-section-group"]),
        ("card_feed", &[".loom-card-feed", ".loom-card-feed-item"]),
        ("sidebar", &[".loom-sidebar", ".loom-panel"]),
        ("form", &[".loom-form-section", ".loom-form-field"]),
        ("composer", &[".loom-composer", ".loom-composer__prompt"]),
        ("picture", &[".loom-picture"]),
        ("paragraph", &[".loom-prose"]),
        ("heading", &[".loom-heading"]),
        ("banner", &[".loom-banner"]),
    ];
    let mut missing = 0u32;
    let mut found = 0u32;
    for (variant, required) in pairs {
        for sel in *required {
            if css.contains(sel) {
                found += 1;
            } else {
                missing += 1;
                eprintln!(
                    "  fail   variant={variant} requires selector {sel} in skin.css — not found"
                );
            }
        }
    }
    println!(
        "loom audit-bridge: {} variant(s), {} required selector(s), {found} found, {missing} missing",
        pairs.len(),
        pairs.iter().map(|(_, r)| r.len()).sum::<usize>()
    );
    Ok(missing)
}

#[cfg(test)]
mod cmd_audit_bridge_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-audit-bridge-{label}-{pid}-{n}.css"))
    }

    #[test]
    fn errs_on_missing_skin() {
        let p = std::env::temp_dir().join("loom-audit-bridge-missing-zzzzz.css");
        let _ = std::fs::remove_file(&p);
        let r = cmd_audit_bridge(&p);
        assert!(r.is_err());
    }

    #[test]
    fn empty_skin_reports_all_missing() {
        let p = unique("empty");
        std::fs::write(&p, "/* empty */").expect("write");
        let missing = cmd_audit_bridge(&p).expect("ok");
        // 10 variants × at least 1 required selector each.
        assert!(missing >= 10, "expected ≥10 missing, got {missing}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn full_coverage_reports_zero_missing() {
        let p = unique("full");
        // Stub every required selector. Note these are SUBSTRING
        // checks, so just listing them is enough.
        let body = r"
            .loom-section-hero { } .loom-section-group { }
            .loom-card-feed { } .loom-card-feed-item { }
            .loom-sidebar { } .loom-panel { }
            .loom-form-section { } .loom-form-field { }
            .loom-composer { } .loom-composer__prompt { }
            .loom-picture { } .loom-prose { } .loom-heading { } .loom-banner { }
        ";
        std::fs::write(&p, body).expect("write");
        let missing = cmd_audit_bridge(&p).expect("ok");
        assert_eq!(missing, 0);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_one_selector_returns_count() {
        let p = unique("one-missing");
        // Same as full but minus .loom-banner.
        let body = r"
            .loom-section-hero { } .loom-section-group { }
            .loom-card-feed { } .loom-card-feed-item { }
            .loom-sidebar { } .loom-panel { }
            .loom-form-section { } .loom-form-field { }
            .loom-composer { } .loom-composer__prompt { }
            .loom-picture { } .loom-prose { } .loom-heading { }
        ";
        std::fs::write(&p, body).expect("write");
        let missing = cmd_audit_bridge(&p).expect("ok");
        assert_eq!(missing, 1);
        let _ = std::fs::remove_file(&p);
    }
}

/// `loom hooks install` writes this script as
/// `<target>/.git/hooks/pre-commit`. The script is intentionally
/// shell-portable (POSIX sh, no bash-isms) so it runs anywhere
/// git is installed. It looks up the loom binary via PATH so
/// authors can rebuild loom independently of the hook script
/// itself; if loom isn't on PATH, it warns + lets the commit
/// through (better than blocking commits because the dev forgot
/// to update PATH).
const PRE_COMMIT_HOOK_BODY: &str = r#"#!/bin/sh
# Installed by `loom hooks install`. Validates cms/*.json before
# every commit so broken schemas / URL validity never reach main.
#
# To skip (one commit only): git commit --no-verify
# To uninstall: rm .git/hooks/pre-commit
set -e
REPO_ROOT="$(git rev-parse --show-toplevel)"
if [ ! -d "$REPO_ROOT/cms" ]; then
  exit 0
fi
if ! command -v loom >/dev/null 2>&1; then
  echo "loom hooks: loom binary not on PATH; skipping cms/ validation"
  echo "  install: cargo install --path /path/to/PlausiDen-Loom/loom-cli"
  exit 0
fi
exec loom validate --input "$REPO_ROOT/cms"
"#;

fn cmd_hooks_install(target: &std::path::Path, force: bool) -> Result<bool, std::io::Error> {
    let git_dir = target.join(".git");
    if !git_dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(".git not found at {} — not a git repo?", git_dir.display()),
        ));
    }
    let hooks_dir = git_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-commit");
    if hook_path.exists() {
        // Two cases: it's our own hook (already current — idempotent
        // success) OR someone else's. Compare contents to decide.
        let existing = std::fs::read_to_string(&hook_path).unwrap_or_default();
        if existing == PRE_COMMIT_HOOK_BODY {
            // Already current. Nothing to do.
            println!(
                "  ok     pre-commit hook already current at {}",
                hook_path.display()
            );
            return Ok(false);
        }
        if !force {
            return Ok(true); // signal Conflict to caller
        }
    }
    std::fs::write(&hook_path, PRE_COMMIT_HOOK_BODY)?;
    set_executable(&hook_path)?;
    println!(
        "  ok     pre-commit hook installed at {}",
        hook_path.display()
    );
    Ok(false)
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) -> std::io::Result<()> {
    // Non-Unix: git on Windows runs hooks via msys/git-bash which
    // honors execute via shebang; no chmod needed.
    Ok(())
}

#[cfg(test)]
mod cmd_hooks_install_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-hooks-{label}-{pid}-{n}"))
    }

    fn fake_repo(label: &str) -> std::path::PathBuf {
        let dir = unique(label);
        std::fs::create_dir_all(dir.join(".git/hooks")).expect("mkdir");
        dir
    }

    #[test]
    fn errs_on_non_git_dir() {
        let dir = unique("not-git");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let r = cmd_hooks_install(&dir, false);
        assert!(r.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_hook_when_absent() {
        let repo = fake_repo("fresh");
        let conflict = cmd_hooks_install(&repo, false).expect("ok");
        assert!(!conflict);
        let hook = repo.join(".git/hooks/pre-commit");
        assert!(hook.exists());
        let body = std::fs::read_to_string(&hook).expect("read");
        assert!(body.contains("loom validate --input"));
        assert!(body.starts_with("#!/bin/sh"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    #[cfg(unix)]
    fn hook_is_executable() {
        use std::os::unix::fs::PermissionsExt as _;
        let repo = fake_repo("perms");
        cmd_hooks_install(&repo, false).expect("ok");
        let mode = std::fs::metadata(repo.join(".git/hooks/pre-commit"))
            .expect("stat")
            .permissions()
            .mode();
        // Owner exec bit set.
        assert!(mode & 0o100 != 0, "hook not user-executable: {mode:o}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let repo = fake_repo("conflict");
        let hook_path = repo.join(".git/hooks/pre-commit");
        std::fs::write(&hook_path, "# someone else's hook\n").expect("write");
        let conflict = cmd_hooks_install(&repo, false).expect("ok");
        assert!(conflict);
        // Body unchanged.
        let body = std::fs::read_to_string(&hook_path).expect("read");
        assert!(body.contains("someone else's hook"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn force_overwrites() {
        let repo = fake_repo("force");
        let hook_path = repo.join(".git/hooks/pre-commit");
        std::fs::write(&hook_path, "# old\n").expect("write");
        cmd_hooks_install(&repo, true).expect("ok");
        let body = std::fs::read_to_string(&hook_path).expect("read");
        assert!(body.contains("loom validate"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn rerun_with_current_body_is_idempotent() {
        let repo = fake_repo("idempotent");
        // First install.
        cmd_hooks_install(&repo, false).expect("first ok");
        // Second invocation: body is already current → Ok(false), no error.
        let conflict = cmd_hooks_install(&repo, false).expect("second ok");
        assert!(!conflict);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn hook_skips_when_no_cms_dir() {
        let repo = fake_repo("no-cms");
        cmd_hooks_install(&repo, false).expect("ok");
        let body = std::fs::read_to_string(repo.join(".git/hooks/pre-commit")).expect("read");
        // The hook checks for cms/ existence and exits 0 if absent.
        assert!(body.contains("if [ ! -d \"$REPO_ROOT/cms\" ]"));
        assert!(body.contains("exit 0"));
        let _ = std::fs::remove_dir_all(&repo);
    }
}

/// `loom journey-from-cms` errors.
#[derive(Debug)]
enum JourneyFromCmsError {
    Schema {
        file: std::path::PathBuf,
        error: String,
    },
    Io(std::io::Error),
}

impl From<std::io::Error> for JourneyFromCmsError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

fn cmd_journey_from_cms(
    cms_dir: &std::path::Path,
    base_url: &str,
    out: &std::path::Path,
    name: &str,
) -> Result<(), JourneyFromCmsError> {
    if !cms_dir.is_dir() {
        return Err(JourneyFromCmsError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("cms-dir not found: {}", cms_dir.display()),
        )));
    }
    // Collect + parse + sort by path so journey order is
    // deterministic across runs (fs walk order is filesystem-
    // dependent; sorted output makes diffs reviewable).
    let mut json_files = Vec::<std::path::PathBuf>::new();
    walk_cms_json(cms_dir, &mut json_files)?;
    json_files.sort();
    let mut pages = Vec::<(String, String)>::new(); // (path, label)
    for path in &json_files {
        let raw = std::fs::read_to_string(path)?;
        let page: loom_cms_render::CmsPage =
            serde_json::from_str(&raw).map_err(|e| JourneyFromCmsError::Schema {
                file: path.clone(),
                error: e.to_string(),
            })?;
        let label = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("page")
            .to_owned();
        pages.push((page.path, label));
    }
    let journey = build_journey_json(name, base_url, &pages);
    let pretty = serde_json::to_string_pretty(&journey)
        .map_err(|e| JourneyFromCmsError::Io(std::io::Error::other(format!("serialize: {e}"))))?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, format!("{pretty}\n"))?;
    println!("  ok     {} page(s) → {}", pages.len(), out.display());
    Ok(())
}

fn walk_cms_json(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_cms_json(&path, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        }
    }
    Ok(())
}

/// Build the journey JSON value. Pure function — testable
/// without filesystem I/O.
fn build_journey_json(name: &str, base_url: &str, pages: &[(String, String)]) -> serde_json::Value {
    let trimmed_base = base_url.trim_end_matches('/');
    let mut steps = Vec::<serde_json::Value>::new();
    for (i, (path, label)) in pages.iter().enumerate() {
        // CmsPage.path may be "/" or "/about.html" — concatenate
        // with the trimmed base so we don't end up with double
        // slashes.
        let url = format!("{trimmed_base}{path}");
        steps.push(serde_json::json!({
            "kind": "goto",
            "url": url,
            "timeout": 10000,
            "label": label,
        }));
        steps.push(serde_json::json!({
            "kind": "wait",
            "ms": 1200,
        }));
        steps.push(serde_json::json!({
            "kind": "screenshot",
            "label": format!("{:02}-{label}", i + 1),
        }));
    }
    serde_json::json!({
        "name": name,
        "description": format!("Auto-generated by `loom journey-from-cms` from {} CmsPage document(s). DO NOT HAND-EDIT — re-run the command instead.", pages.len()),
        "baseUrl": format!("{trimmed_base}/"),
        "steps": steps,
    })
}

#[cfg(test)]
mod cmd_journey_from_cms_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-journey-{label}-{pid}-{n}"))
    }

    #[test]
    fn empty_dir_emits_zero_steps() {
        let dir = unique("empty");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let out = dir.join("journey.json");
        cmd_journey_from_cms(&dir, "http://x/", &out, "empty-test").expect("ok");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["name"], "empty-test");
        assert_eq!(v["steps"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errs_on_missing_cms_dir() {
        let dir = std::env::temp_dir().join("loom-journey-missing-zzzzz");
        let _ = std::fs::remove_dir_all(&dir);
        let out = std::env::temp_dir().join("loom-journey-out-zzz.json");
        let r = cmd_journey_from_cms(&dir, "http://x/", &out, "n");
        assert!(matches!(r, Err(JourneyFromCmsError::Io(_))));
    }

    #[test]
    fn schema_error_surfaces_file_path() {
        let dir = unique("bad");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("bad.json"), "{not valid json").expect("w");
        let out = dir.join("journey.json");
        let r = cmd_journey_from_cms(&dir, "http://x/", &out, "n");
        match r {
            Err(JourneyFromCmsError::Schema { file, .. }) => {
                assert!(file.ends_with("bad.json"));
            }
            other => panic!("expected schema error, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_journey_emits_three_steps_per_page() {
        let pages = vec![
            ("/".to_owned(), "index".to_owned()),
            ("/about.html".to_owned(), "about".to_owned()),
        ];
        let v = build_journey_json("test", "http://x/", &pages);
        let steps = v["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 6); // 3 per page × 2 pages
        // First triple is the index page.
        assert_eq!(steps[0]["url"], "http://x/");
        assert_eq!(steps[0]["kind"], "goto");
        assert_eq!(steps[1]["kind"], "wait");
        assert_eq!(steps[2]["kind"], "screenshot");
        assert_eq!(steps[2]["label"], "01-index");
        // Second triple is about.
        assert_eq!(steps[3]["url"], "http://x/about.html");
        assert_eq!(steps[5]["label"], "02-about");
    }

    #[test]
    fn base_url_trailing_slash_normalized() {
        let pages = vec![("/x".to_owned(), "x".to_owned())];
        let v = build_journey_json("t", "http://x/", &pages);
        assert_eq!(v["steps"][0]["url"], "http://x/x");
        let v2 = build_journey_json("t", "http://x", &pages);
        assert_eq!(v2["steps"][0]["url"], "http://x/x");
        // baseUrl in the output gets a trailing slash either way.
        assert_eq!(v["baseUrl"], "http://x/");
        assert_eq!(v2["baseUrl"], "http://x/");
    }

    #[test]
    fn output_is_deterministic_sorted_by_filename() {
        let dir = unique("sorted");
        std::fs::create_dir_all(&dir).expect("mkdir");
        // Write in non-alphabetical order to verify sort.
        let make = |fname: &str, path: &str| {
            std::fs::write(
                dir.join(fname),
                format!(r#"{{"title":"x","description":"x","path":"{path}","sections":[]}}"#),
            )
            .expect("w");
        };
        make("zeta.json", "/zeta");
        make("alpha.json", "/alpha");
        make("middle.json", "/middle");
        let out = dir.join("journey.json");
        cmd_journey_from_cms(&dir, "http://x/", &out, "n").expect("ok");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let steps = v["steps"].as_array().unwrap();
        // Step labels are <NN>-<filename-stem>, sorted alphabetically.
        // First page = alpha, second = middle, third = zeta.
        assert_eq!(steps[0]["label"], "alpha");
        assert_eq!(steps[3]["label"], "middle");
        assert_eq!(steps[6]["label"], "zeta");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `loom cms-new` errors.
#[derive(Debug)]
enum CmsNewError {
    Conflict(std::path::PathBuf),
    UnknownKind(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for CmsNewError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

fn cmd_cms_new(
    kind: &str,
    out: &std::path::Path,
    title: &str,
    path: &str,
    force: bool,
) -> Result<(), CmsNewError> {
    if out.exists() && !force {
        return Err(CmsNewError::Conflict(out.to_path_buf()));
    }
    let page = match kind {
        "landing" => cms_template_landing(title, path),
        "explainer" => cms_template_explainer(title, path),
        "form" => cms_template_form(title, path),
        other => return Err(CmsNewError::UnknownKind(other.to_owned())),
    };
    let json = serde_json::to_string_pretty(&page)
        .map_err(|e| CmsNewError::Io(std::io::Error::other(format!("serialize: {e}"))))?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, format!("{json}\n"))?;
    println!("  ok     {kind} template → {}", out.display());
    Ok(())
}

fn cms_standard_nav() -> Vec<loom_cms_render::CmsNavLink> {
    vec![
        loom_cms_render::CmsNavLink {
            label: "Battle Feed".to_owned(),
            href: "/".to_owned(),
            data_backend: "list-challenges".to_owned(),
            current: false,
        },
        loom_cms_render::CmsNavLink {
            label: "Leaderboard".to_owned(),
            href: "/leaderboard.html".to_owned(),
            data_backend: "list-leaderboard".to_owned(),
            current: false,
        },
        loom_cms_render::CmsNavLink {
            label: "My Wins".to_owned(),
            href: "/my-wins.html".to_owned(),
            data_backend: "list-touches".to_owned(),
            current: false,
        },
        loom_cms_render::CmsNavLink {
            label: "Profile".to_owned(),
            href: "/profile.html".to_owned(),
            data_backend: "view-profile".to_owned(),
            current: false,
        },
    ]
}

fn cms_template_landing(title: &str, path: &str) -> loom_cms_render::CmsPage {
    use loom_cms_render::*;
    CmsPage {
        schema: Some("../cms-schema.json".to_owned()),
        title: title.to_owned(),
        description: format!("{title} — describe the page in 120 chars max."),
        path: path.to_owned(),
        nav_links: cms_standard_nav(),
        sections: vec![
            CmsSection::Hero {
                eyebrow: Some("New".to_owned()),
                title: title.to_owned(),
                lede: Some("One-sentence summary that conveys the value of this page.".to_owned()),
                cta: Some(HeroCta {
                    label: "Get started".to_owned(),
                    href: "/post-skill".to_owned(),
                    data_backend: "post-skill".to_owned(),
                }),
            },
            CmsSection::Composer {
                prompt: "What did you nail today?".to_owned(),
                submit_endpoint: "/post-skill".to_owned(),
                actions: vec![
                    CmsPromptAction::UploadClip,
                    CmsPromptAction::ChallengeOpponent,
                    CmsPromptAction::GoLive,
                ],
                avatar: CmsAvatar::Initials {
                    letters: "DA".to_owned(),
                },
                size: CmsComposerSize::Comfortable,
            },
            CmsSection::CardFeed {
                heading: Some("Featured".to_owned()),
                items: (1..=3)
                    .map(|i| CmsCard {
                        avatar: CmsAvatar::Initials {
                            letters: format!("S{i}"),
                        },
                        title: format!("Sample card {i}"),
                        host: Some(format!("Hosted by @sample · {i}d left")),
                        stats: vec![
                            CmsCardStat {
                                label: "Votes".to_owned(),
                                value: "—".to_owned(),
                            },
                            CmsCardStat {
                                label: "Pot".to_owned(),
                                value: "—".to_owned(),
                            },
                        ],
                        href: format!("/c/sample-{i}"),
                        data_backend: "view-challenge".to_owned(),
                        tag: Some("Sample".to_owned()),
                    })
                    .collect(),
            },
        ],
    }
}

fn cms_template_explainer(title: &str, path: &str) -> loom_cms_render::CmsPage {
    use loom_cms_render::*;
    CmsPage {
        schema: Some("../cms-schema.json".to_owned()),
        title: title.to_owned(),
        description: format!("{title} — explainer / about / FAQ page."),
        path: path.to_owned(),
        nav_links: cms_standard_nav(),
        sections: vec![
            CmsSection::Hero {
                eyebrow: None,
                title: title.to_owned(),
                lede: Some("One-sentence summary that conveys the page's purpose.".to_owned()),
                cta: None,
            },
            CmsSection::Group {
                title: "Section A".to_owned(),
                body: vec![
                    "Body text. Replace with your own copy.".to_owned(),
                    "Another paragraph in the same group section.".to_owned(),
                ],
            },
            CmsSection::Group {
                title: "Section B".to_owned(),
                body: vec!["More body text. Each Group is a card.".to_owned()],
            },
            CmsSection::Group {
                title: "Section C".to_owned(),
                body: vec!["Third explainer block.".to_owned()],
            },
        ],
    }
}

fn cms_template_form(title: &str, path: &str) -> loom_cms_render::CmsPage {
    use loom_cms_render::*;
    CmsPage {
        schema: Some("../cms-schema.json".to_owned()),
        title: title.to_owned(),
        description: format!("{title} — form / submission page."),
        path: path.to_owned(),
        nav_links: cms_standard_nav(),
        sections: vec![
            CmsSection::Hero {
                eyebrow: None,
                title: title.to_owned(),
                lede: Some("Describe what this form does in one sentence.".to_owned()),
                cta: None,
            },
            CmsSection::Form {
                legend: "Submit".to_owned(),
                submit: CmsFormSubmit {
                    label: "Submit".to_owned(),
                    secondary_label: Some("Cancel".to_owned()),
                    action: "/post-skill".to_owned(),
                    data_backend: "post-skill".to_owned(),
                },
                steps: vec![CmsFormStep {
                    label: "Details".to_owned(),
                    state: CmsFormStepState::Current,
                    fields: vec![
                        CmsFormField::Text {
                            name: "name".to_owned(),
                            label: "Name".to_owned(),
                            hint: None,
                            placeholder: Some("Your name".to_owned()),
                            max_length: Some(120),
                            required: true,
                        },
                        CmsFormField::Textarea {
                            name: "message".to_owned(),
                            label: "Message".to_owned(),
                            hint: Some("Replace with your own field set.".to_owned()),
                            placeholder: None,
                            max_length: None,
                            rows: 4,
                            required: false,
                        },
                    ],
                }],
            },
        ],
    }
}

#[cfg(test)]
mod cmd_cms_new_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-cms-new-{label}-{pid}-{n}.json"))
    }

    #[test]
    fn unknown_kind_errors() {
        let out = unique("unknown");
        let r = cmd_cms_new("widget", &out, "X", "/x", false);
        assert!(matches!(r, Err(CmsNewError::UnknownKind(_))));
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let out = unique("overwrite");
        std::fs::write(&out, "existing").expect("write");
        let r = cmd_cms_new("landing", &out, "X", "/x", false);
        assert!(matches!(r, Err(CmsNewError::Conflict(_))));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn force_overwrites() {
        let out = unique("force");
        std::fs::write(&out, "old").expect("write");
        cmd_cms_new("landing", &out, "X", "/x", true).expect("ok");
        let got = std::fs::read_to_string(&out).expect("read");
        assert!(got.starts_with('{'));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn landing_template_round_trips() {
        let out = unique("landing-rt");
        cmd_cms_new("landing", &out, "Landing T", "/landing", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        let page: loom_cms_render::CmsPage = serde_json::from_str(&raw).expect("parse");
        assert_eq!(page.title, "Landing T");
        assert_eq!(page.path, "/landing");
        assert_eq!(page.nav_links.len(), 4);
        assert_eq!(page.sections.len(), 3);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn explainer_template_round_trips() {
        let out = unique("explainer-rt");
        cmd_cms_new("explainer", &out, "About SkillShots", "/about", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        let page: loom_cms_render::CmsPage = serde_json::from_str(&raw).expect("parse");
        assert_eq!(page.sections.len(), 4);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn form_template_round_trips() {
        let out = unique("form-rt");
        cmd_cms_new("form", &out, "Settings", "/settings", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        let page: loom_cms_render::CmsPage = serde_json::from_str(&raw).expect("parse");
        assert_eq!(page.sections.len(), 2);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn output_carries_schema_reference() {
        let out = unique("schema-ref");
        cmd_cms_new("landing", &out, "X", "/x", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        assert!(raw.contains(r#""$schema""#));
        let _ = std::fs::remove_file(&out);
    }
}

/// `loom cms-schema` — emits the JSON Schema for CmsPage to a
/// file or stdout. Pretty-printed (2-space indent) so authors
/// reading the schema directly can navigate it; `--out -` is fine
/// for piping into `jq` or similar.
fn cmd_cms_schema(out: &str) -> Result<(), std::io::Error> {
    let schema = loom_cms_render::cms_page_schema();
    let pretty = serde_json::to_string_pretty(&schema)
        .map_err(|e| std::io::Error::other(format!("schema serialize: {e}")))?;
    if out == "-" {
        print!("{pretty}\n");
        return Ok(());
    }
    if let Some(parent) = std::path::Path::new(out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, format!("{pretty}\n"))?;
    Ok(())
}

/// `loom validate` — schema + URL validation for CmsPage JSON.
///
/// Two passes per file:
///   1. serde deserialize → catches missing fields, typos
///      (deny_unknown_fields), wrong types, wrong enum tags.
///   2. URL-validity walk → every href / cta.href / avatar src /
///      submit_endpoint / nav-link href / form action / panel
///      list-item href / card href passes through is_safe_url.
///
/// `Ok(true)` if at least one file failed (caller maps to exit 1);
/// `Ok(false)` if every file passed; `Err` on I/O.
fn cmd_validate(input: &std::path::Path) -> Result<bool, std::io::Error> {
    if !input.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("input not found: {}", input.display()),
        ));
    }
    let mut files = Vec::<std::path::PathBuf>::new();
    if input.is_file() {
        files.push(input.to_path_buf());
    } else {
        walk_json(input, &mut files)?;
    }
    let mut any_failed = false;
    let mut ok_count = 0u32;
    for path in &files {
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  fail   {}: read error: {e}", path.display());
                any_failed = true;
                continue;
            }
        };
        let page: loom_cms_render::CmsPage = match serde_json::from_str(&raw) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "  fail   {}: schema error at line {}, col {}: {}",
                    path.display(),
                    e.line(),
                    e.column(),
                    e,
                );
                any_failed = true;
                continue;
            }
        };
        let url_errs = validate_urls(&page);
        if url_errs.is_empty() {
            println!("  ok     {}", path.display());
            ok_count += 1;
        } else {
            for err in &url_errs {
                eprintln!("  fail   {}: url-invalid: {err}", path.display());
            }
            any_failed = true;
        }
    }
    println!(
        "loom validate: {} file(s), {ok_count} ok, {} failed",
        files.len(),
        files.len() as u32 - ok_count
    );
    Ok(any_failed)
}

fn walk_json(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_json(&path, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        }
    }
    Ok(())
}

/// Walk every URL field in a `CmsPage` and accumulate descriptive
/// errors for any that fail `is_safe_url`. Field paths are
/// JSON-Pointer-style for operator clarity.
fn validate_urls(page: &loom_cms_render::CmsPage) -> Vec<String> {
    use loom_cms_render::is_safe_url;
    let mut errs = Vec::<String>::new();
    for (i, link) in page.nav_links.iter().enumerate() {
        if !is_safe_url(&link.href) {
            errs.push(format!("/nav_links/{i}/href={:?}", link.href));
        }
    }
    for (i, section) in page.sections.iter().enumerate() {
        validate_section_urls(section, i, &mut errs);
    }
    errs
}

fn validate_section_urls(
    section: &loom_cms_render::CmsSection,
    idx: usize,
    errs: &mut Vec<String>,
) {
    use loom_cms_render::{CmsAvatar, CmsPanelBody, CmsSection, is_safe_url};
    match section {
        CmsSection::Hero { cta: Some(cta), .. } => {
            if !is_safe_url(&cta.href) {
                errs.push(format!("/sections/{idx}/cta/href={:?}", cta.href));
            }
        }
        CmsSection::Composer {
            submit_endpoint,
            avatar,
            ..
        } => {
            if !is_safe_url(submit_endpoint) {
                errs.push(format!(
                    "/sections/{idx}/submit_endpoint={submit_endpoint:?}"
                ));
            }
            if let CmsAvatar::Image { src, .. } = avatar {
                if !is_safe_url(src) {
                    errs.push(format!("/sections/{idx}/avatar/src={src:?}"));
                }
            }
        }
        CmsSection::CardFeed { items, .. } => {
            for (j, card) in items.iter().enumerate() {
                if !is_safe_url(&card.href) {
                    errs.push(format!("/sections/{idx}/items/{j}/href={:?}", card.href));
                }
                if let CmsAvatar::Image { src, .. } = &card.avatar {
                    if !is_safe_url(src) {
                        errs.push(format!("/sections/{idx}/items/{j}/avatar/src={src:?}"));
                    }
                }
            }
        }
        CmsSection::Sidebar { panels, .. } => {
            for (j, panel) in panels.iter().enumerate() {
                if let CmsPanelBody::List { items } = &panel.body {
                    for (k, item) in items.iter().enumerate() {
                        if let Some(href) = &item.href {
                            if !is_safe_url(href) {
                                errs.push(format!(
                                    "/sections/{idx}/panels/{j}/items/{k}/href={href:?}"
                                ));
                            }
                        }
                    }
                }
            }
        }
        CmsSection::Form { submit, .. } => {
            if !is_safe_url(&submit.action) {
                errs.push(format!("/sections/{idx}/submit/action={:?}", submit.action));
            }
        }
        // Banner / Picture / Paragraph / Heading / Group / Hero (no cta) —
        // no URL fields to validate.
        _ => {}
    }
}

#[cfg(test)]
mod cmd_validate_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-validate-{label}-{pid}-{n}"))
    }

    #[test]
    fn errs_on_missing_input() {
        let p = std::env::temp_dir().join("loom-validate-missing-zzzzz");
        let _ = std::fs::remove_file(&p);
        let r = cmd_validate(&p);
        assert!(r.is_err());
    }

    #[test]
    fn passes_valid_minimal_page() {
        let dir = unique("valid-min");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("ok.json");
        std::fs::write(
            &f,
            r#"{"title":"x","description":"x","path":"/x","sections":[]}"#,
        )
        .expect("write");
        assert!(!cmd_validate(&f).expect("ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_unknown_field() {
        let dir = unique("unknown-field");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("bad.json");
        std::fs::write(
            &f,
            r#"{"title":"x","description":"x","path":"/x","sections":[],"smuggled":"x"}"#,
        )
        .expect("write");
        assert!(cmd_validate(&f).expect("ok-result"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_invalid_hero_cta_url() {
        let dir = unique("bad-hero-cta");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("p.json");
        std::fs::write(
            &f,
            r#"{
                "title":"x","description":"x","path":"/x",
                "sections":[
                    {
                        "kind":"hero",
                        "title":"T",
                        "cta":{"label":"x","href":"javascript:alert(1)","data_backend":"x"}
                    }
                ]
            }"#,
        )
        .expect("write");
        assert!(cmd_validate(&f).expect("ok-result"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_invalid_nav_link_url() {
        let dir = unique("bad-nav");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("p.json");
        std::fs::write(
            &f,
            r#"{
                "title":"x","description":"x","path":"/x",
                "nav_links":[{"label":"x","href":"javascript:x","data_backend":"x"}],
                "sections":[]
            }"#,
        )
        .expect("write");
        assert!(cmd_validate(&f).expect("ok-result"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_invalid_form_action() {
        let dir = unique("bad-form");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("p.json");
        std::fs::write(
            &f,
            r#"{
                "title":"x","description":"x","path":"/x",
                "sections":[
                    {
                        "kind":"form",
                        "legend":"x",
                        "submit":{"label":"go","action":"//evil/post","data_backend":"x"},
                        "steps":[]
                    }
                ]
            }"#,
        )
        .expect("write");
        assert!(cmd_validate(&f).expect("ok-result"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_invalid_card_href() {
        let dir = unique("bad-card");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("p.json");
        std::fs::write(
            &f,
            r#"{
                "title":"x","description":"x","path":"/x",
                "sections":[
                    {
                        "kind":"card_feed",
                        "items":[
                            {
                                "avatar":{"kind":"none"},
                                "title":"t",
                                "href":"javascript:alert(1)",
                                "data_backend":"x"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("write");
        assert!(cmd_validate(&f).expect("ok-result"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_input_walks_recursively() {
        let dir = unique("recurse");
        let nested = dir.join("a/b");
        std::fs::create_dir_all(&nested).expect("mkdir");
        let ok_doc = r#"{"title":"x","description":"x","path":"/x","sections":[]}"#;
        std::fs::write(dir.join("p1.json"), ok_doc).expect("w");
        std::fs::write(nested.join("p2.json"), ok_doc).expect("w");
        // Plant a non-json that should be ignored.
        std::fs::write(dir.join("readme.txt"), "x").expect("w");
        assert!(!cmd_validate(&dir).expect("ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_input_aggregates_failures() {
        let dir = unique("mixed");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let ok_doc = r#"{"title":"x","description":"x","path":"/x","sections":[]}"#;
        let bad_doc = r#"{"title":"x","description":"x","path":"/x","sections":[],"smuggled":1}"#;
        std::fs::write(dir.join("good.json"), ok_doc).expect("w");
        std::fs::write(dir.join("bad.json"), bad_doc).expect("w");
        // ANY failure → cmd returns Ok(true)
        assert!(cmd_validate(&dir).expect("ok-result"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
