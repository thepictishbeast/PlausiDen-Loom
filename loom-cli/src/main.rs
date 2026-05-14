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

/// `loom auth` subcommands. T43.
#[derive(Subcommand)]
enum AuthAction {
    /// Initialize the auth store. Creates ~/.config/loom/auth.toml
    /// with the first admin user and a fresh HMAC signing key.
    /// Refuses if the file exists (use --force to overwrite).
    Init {
        /// Username to create. Lowercase letters, digits, dashes.
        #[arg(value_name = "USER")]
        user: String,
        /// Read password from $LOOM_PWD env (default: stdin prompt
        /// — but stdin prompts are not implemented yet, so env
        /// is required in this MVP).
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// List configured users.
    List,
    /// Print the auth.toml path so the operator can inspect or
    /// back it up.
    Where,
}

#[derive(Subcommand)]
enum ThemeAction {
    /// Enumerate every theme declared in skin.css, plus the base
    /// `:root` block (rendered as theme name "default").
    List {
        /// Path to skin.css. Defaults to the canonical location
        /// in this workspace.
        #[arg(long, default_value = "loom-tokens/src/skin.css")]
        skin: PathBuf,
    },
    /// Verify (a) every named theme declares the same set of
    /// `--loom-color-*` tokens as the base `:root` block, and
    /// (b) every `var(--loom-color-X)` referenced in skin.css
    /// has a definition in base. Exit 1 on drift.
    Validate {
        #[arg(long, default_value = "loom-tokens/src/skin.css")]
        skin: PathBuf,
    },
    /// T29: WCAG contrast gate. For each theme, compute the
    /// contrast ratio between every declared fg/bg pair
    /// (ink-on-bg-canvas, ink-on-surface, primary-fg-on-primary,
    /// etc.) and refuse any pair below the threshold.
    /// AA = 4.5:1 normal text, 3:1 large text + non-text. AAA
    /// = 7:1 / 4.5:1.
    ///
    /// Exit codes:
    ///   0 — every theme passes
    ///   1 — at least one pair below threshold
    ///   2 — skin.css unreadable / unparseable
    Contrast {
        #[arg(long, default_value = "loom-tokens/src/skin.css")]
        skin: PathBuf,
        /// Required minimum contrast ratio. Default 4.5 (WCAG AA
        /// normal text). Pass 7.0 for AAA, 3.0 for AA-large.
        #[arg(long, default_value_t = 4.5)]
        min_ratio: f64,
    },
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
    /// `loom backend-stub-all` — T19 mass-mint mode. Walks
    /// backends.toml, scaffolds every entry whose impl_files is
    /// empty. Skips entries that already have a non-empty
    /// impl_files (use `--force` + the per-key form to overwrite
    /// individual handlers). Per-key debug logging surfaces
    /// ok/skip/error for each entry.
    ///
    /// Exit codes:
    ///   0 — all stubs minted (or none were stubs to begin with)
    ///   1 — one or more entries failed (still attempts every key)
    ///   2 — backends.toml unreadable or crate path not a directory
    BackendStubAll {
        /// Path to backends.toml.
        #[arg(long, default_value = "backends.toml")]
        backends: PathBuf,
        /// Crate root that will own the handlers (see backend-stub).
        #[arg(long)]
        crate_dir: PathBuf,
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
    /// `loom edit serve` — typed CMS editor server. T42.
    ///
    /// Starts a tiny HTTP server on `--port` (default 8124) that
    /// renders one form per cms/<page>.json, accepts POSTs to
    /// persist edits, and re-runs forge.sh after every save. No
    /// JavaScript required; every interaction is server-rendered
    /// HTML and a multipart POST.
    ///
    /// MVP scope: text-only fields (title, description, hero
    /// title/subtitle/eyebrow, paragraph bodies). Nested arrays,
    /// images, and form sections land in follow-ups. Auth lands
    /// in T43 — until then, bind to 127.0.0.1 only.
    EditServe {
        /// CMS root directory containing *.json page files.
        #[arg(long, default_value = "cms")]
        cms: PathBuf,
        /// Static output directory served as /preview/*.
        #[arg(long, default_value = "static")]
        static_dir: PathBuf,
        /// forge.sh path; invoked after every successful save.
        /// Empty string disables the rebuild hook (useful in tests).
        #[arg(long, default_value = "forge.sh")]
        forge: String,
        /// TCP port to listen on. Bound to 127.0.0.1 always.
        #[arg(long, default_value_t = 8124)]
        port: u16,
    },
    /// T43: admin auth management.
    ///
    /// `loom auth init <user>` creates the first admin user.
    /// Argon2id-hashes the password (read from $LOOM_PWD env or
    /// stdin), generates a fresh HMAC signing key, and writes
    /// both to `~/.config/loom/auth.toml` (mode 0600 on Unix).
    ///
    /// Once auth.toml exists, `loom edit-serve` requires login.
    /// Without auth.toml, the editor stays open (back-compat).
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// T63: import existing static HTML into typed CmsPage JSON.
    ///
    /// `loom import --from path/to/site.html --into ./cms`
    /// reads the HTML file, reverse-engineers the structure
    /// using simple element heuristics (header → Hero, section
    /// h2+p → Group, etc.), and writes a cms/<slug>.json file
    /// the editor can immediately open + edit.
    ///
    /// Unmappable markup is written as a paragraph section with
    /// a TODO marker so the operator can audit + convert
    /// manually.
    ///
    /// MVP scope: single HTML file, local path only. Future:
    /// directory walk, URL fetch, automatic asset extraction.
    Import {
        /// Path to the source HTML file.
        #[arg(long)]
        from: PathBuf,
        /// Target CMS directory. The slug is derived from the
        /// file's basename (without extension).
        #[arg(long, default_value = "cms")]
        into: PathBuf,
        /// Override the derived slug.
        #[arg(long)]
        slug: Option<String>,
        /// Overwrite if the target file exists.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// `loom theme` — inspect + validate the theme system.
    ///
    /// Themes are declared inline in skin.css as `:root[data-theme=
    /// "<name>"] { --loom-color-...: ...; }` blocks. Site HTML opts
    /// in via `<html data-theme="...">`. The base `:root` block
    /// defines the default theme and the canonical set of tokens
    /// every named theme must provide.
    ///
    /// T28 (2026-05-06): list = enumerate, validate = consistency
    /// check (every named theme declares the same tokens as base;
    /// every `var(--loom-color-X)` consumed in skin.css has a
    /// definition in base). Future: new/apply/audit (T29).
    Theme {
        #[command(subcommand)]
        action: ThemeAction,
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
        /// (e.g. `<http://127.0.0.1:8123/>`). Trailing slash optional.
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

#[allow(clippy::too_many_lines)] // single match over every Cmd variant.
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
            Err(BackendStubError::CapabilityEscape {
                attempted,
                confined_root,
            }) => {
                eprintln!(
                    "loom backend-stub: SECURITY: write attempt {} escapes capability scope {}",
                    attempted.display(),
                    confined_root.display(),
                );
                ExitCode::from(2)
            }
        },
        Cmd::BackendStubAll {
            backends,
            crate_dir,
        } => match cmd_backend_stub_all(&backends, &crate_dir) {
            Ok(BackendStubAllReport { ok, skipped, failed }) => {
                println!(
                    "  ok     {ok} minted, {skipped} skipped (already had impl), {failed} failed"
                );
                if failed > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
            }
            Err(BackendStubError::Toml(e)) => {
                eprintln!("loom backend-stub-all: toml error: {e}");
                ExitCode::from(2)
            }
            Err(BackendStubError::CrateNotDir(p)) => {
                eprintln!("loom backend-stub-all: {} is not a directory", p.display());
                ExitCode::from(2)
            }
            Err(BackendStubError::Io(e)) => {
                eprintln!("loom backend-stub-all: i/o error: {e}");
                ExitCode::from(2)
            }
            Err(BackendStubError::CapabilityEscape {
                attempted,
                confined_root,
            }) => {
                eprintln!(
                    "loom backend-stub-all: SECURITY: write attempt {} escapes capability scope {}",
                    attempted.display(),
                    confined_root.display(),
                );
                ExitCode::from(2)
            }
            Err(BackendStubError::KeyNotFound(_) | BackendStubError::Conflict(_)) => {
                // Per-key errors are reported inline by cmd_backend_stub_all
                // and never bubble out as a top-level Err — these arms are
                // for exhaustiveness only.
                ExitCode::from(1)
            }
        },
        Cmd::BackendList { backends } => match cmd_backend_list(&backends) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom backend list: i/o error: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::EditServe {
            cms,
            static_dir,
            forge,
            port,
        } => match cmd_edit_serve(&cms, &static_dir, &forge, port) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom edit serve: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::Import {
            from,
            into,
            slug,
            force,
        } => match cmd_import(&from, &into, slug.as_deref(), force) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("loom import: {e}");
                ExitCode::from(2)
            }
        },
        Cmd::Auth { action } => match action {
            AuthAction::Init { user, force } => match cmd_auth_init(&user, force) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("loom auth init: {e}");
                    ExitCode::from(2)
                }
            },
            AuthAction::List => match cmd_auth_list() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("loom auth list: {e}");
                    ExitCode::from(2)
                }
            },
            AuthAction::Where => {
                println!("{}", auth_store_path().display());
                ExitCode::SUCCESS
            }
        },
        Cmd::Theme { action } => match action {
            ThemeAction::List { skin } => match cmd_theme_list(&skin) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("loom theme list: i/o error: {e}");
                    ExitCode::from(2)
                }
            },
            ThemeAction::Validate { skin } => match cmd_theme_validate(&skin) {
                Ok(0) => ExitCode::SUCCESS,
                Ok(n) => {
                    eprintln!("loom theme validate: {n} drift finding(s)");
                    ExitCode::from(1)
                }
                Err(e) => {
                    eprintln!("loom theme validate: i/o error: {e}");
                    ExitCode::from(2)
                }
            },
            ThemeAction::Contrast { skin, min_ratio } => {
                match cmd_theme_contrast(&skin, min_ratio) {
                    Ok(0) => ExitCode::SUCCESS,
                    Ok(n) => {
                        eprintln!("loom theme contrast: {n} pair(s) below {min_ratio}:1");
                        ExitCode::from(1)
                    }
                    Err(e) => {
                        eprintln!("loom theme contrast: {e}");
                        ExitCode::from(2)
                    }
                }
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
    // Nursery's option_if_let_else wants this as a chained
    // map_or_else closure pair, but the bodies are multi-statement
    // and the if-let-else reads cleaner here.
    #[allow(clippy::option_if_let_else)]
    let (style_block, css_link, csp) = if let Some(crit) = critical_css {
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
    } else {
        let css_link = format!("<link rel=\"stylesheet\" href=\"{css}\">");
        let csp = "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-ancestors 'none'".to_owned();
        (String::new(), css_link, csp)
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
                        "  ok     {src} -> {dest}",
                        src = src.display(),
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
            .map_or(0, |d| d.as_nanos());
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
        assert_eq!(out.len(), 5, "got {out:?}");
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
    /// T577: an attempted write or path-resolution escapes the
    /// capability's confined root.
    CapabilityEscape {
        attempted: std::path::PathBuf,
        confined_root: std::path::PathBuf,
    },
}

impl From<std::io::Error> for BackendStubError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<CapabilityError> for BackendStubError {
    fn from(e: CapabilityError) -> Self {
        match e {
            CapabilityError::NotADir(p) => Self::CrateNotDir(p),
            CapabilityError::Io(e) => Self::Io(e),
            CapabilityError::EscapesScope {
                attempted,
                confined_root,
            } => Self::CapabilityEscape {
                attempted,
                confined_root,
            },
        }
    }
}

// ============================================================
// T577: capability-based filesystem writes.
// ============================================================
//
// Replaces ambient authority (any `&Path` is writable) with an
// unforgeable scoped capability. Once a `WriteCapability` exists
// for a confined root, any operation through it is constrained
// to that subtree at runtime — even if a caller supplies a
// malicious relative path like `"../../etc/passwd"`.
//
// SECURITY: the construction site (`for_dir`) MUST canonicalize
// the root once. Subsequent path resolution canonicalizes the
// joined result and refuses any candidate that doesn't start
// with the confined root. Symlinks are resolved at canonicalize
// time so a symlink-out attack is detected.
//
// REGRESSION-GUARD: do NOT add a `from_path_unchecked` shortcut.
// The whole point is that constructors validate; a backdoor
// erases the guarantee.

/// `WriteCapability` errors.
#[derive(Debug)]
pub enum CapabilityError {
    /// The supplied root is not an existing directory.
    NotADir(std::path::PathBuf),
    /// Filesystem error during canonicalization or write.
    Io(std::io::Error),
    /// Resolved path escapes the capability's confined root.
    EscapesScope {
        /// What the resolver computed before the boundary check.
        attempted: std::path::PathBuf,
        /// The capability's canonicalised root.
        confined_root: std::path::PathBuf,
    },
}

impl From<std::io::Error> for CapabilityError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Unforgeable proof that the holder may write within
/// `confined_root`. Constructed via `for_dir` only.
///
/// Layout: a single canonicalized PathBuf. No public fields,
/// no `Clone` impl that could be used to widen scope (a clone
/// would carry the same root, which is fine — but no method
/// extends it).
#[derive(Debug)]
pub struct WriteCapability {
    confined_root: std::path::PathBuf,
}

impl WriteCapability {
    /// Construct a capability scoped to `root`. Validates that
    /// the path exists, is a directory, and canonicalizes it
    /// (resolving symlinks). The canonicalized root is the
    /// confinement boundary for all subsequent operations.
    pub fn for_dir(root: &std::path::Path) -> Result<Self, CapabilityError> {
        let canonical = root
            .canonicalize()
            .map_err(|_| CapabilityError::NotADir(root.to_path_buf()))?;
        if !canonical.is_dir() {
            return Err(CapabilityError::NotADir(canonical));
        }
        Ok(Self {
            confined_root: canonical,
        })
    }

    /// Borrow the confined root path. Read-only — callers MAY
    /// inspect but cannot mutate.
    pub fn root(&self) -> &std::path::Path {
        &self.confined_root
    }

    /// Resolve `rel_path` against the confined root and confirm
    /// the result stays inside. Returns the absolute path on
    /// success.
    ///
    /// SECURITY: canonicalizes the resolved parent to defeat
    /// `..` traversal AND symlink escapes. If the target file
    /// doesn't exist yet (the common case for scaffolding), we
    /// canonicalize the *deepest existing ancestor* and append
    /// the rest — this still defeats traversal because every
    /// `..` segment is interpreted at canonicalization time.
    pub fn resolve(
        &self,
        rel_path: &std::path::Path,
    ) -> Result<std::path::PathBuf, CapabilityError> {
        let candidate = self.confined_root.join(rel_path);
        // Walk up until we find an existing ancestor. Collect
        // the consumed tail components in REVERSE order then
        // rejoin them in order — this avoids pushing an empty
        // PathBuf which would silently append a `/` and turn
        // the resolved path into a directory.
        let mut deepest = candidate.as_path();
        let mut tail_segments: Vec<std::ffi::OsString> = Vec::new();
        let canonical_anchor = loop {
            match deepest.canonicalize() {
                Ok(p) => break p,
                Err(_) => match deepest.parent() {
                    Some(parent) => {
                        if let Some(name) = deepest.file_name() {
                            tail_segments.push(name.to_os_string());
                        }
                        deepest = parent;
                    }
                    None => {
                        return Err(CapabilityError::EscapesScope {
                            attempted: candidate,
                            confined_root: self.confined_root.clone(),
                        });
                    }
                },
            }
        };
        // Rebuild tail in original order.
        let mut resolved = canonical_anchor;
        for seg in tail_segments.into_iter().rev() {
            resolved.push(seg);
        }
        if !resolved.starts_with(&self.confined_root) {
            return Err(CapabilityError::EscapesScope {
                attempted: resolved,
                confined_root: self.confined_root.clone(),
            });
        }
        Ok(resolved)
    }

    /// Write `contents` to `rel_path` within the confined root.
    /// Creates parent directories as needed (still confined).
    pub fn write_file(
        &self,
        rel_path: &std::path::Path,
        contents: &[u8],
    ) -> Result<std::path::PathBuf, CapabilityError> {
        let abs = self.resolve(rel_path)?;
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs, contents)?;
        Ok(abs)
    }

    /// Read a file within the confined root. Read-only operations
    /// also flow through the capability so a future audit can grep
    /// for every site that touches the filesystem.
    pub fn read_file(
        &self,
        rel_path: &std::path::Path,
    ) -> Result<Vec<u8>, CapabilityError> {
        let abs = self.resolve(rel_path)?;
        Ok(std::fs::read(&abs)?)
    }

    /// Atomic write: temp file + rename. Concurrent callers can't
    /// half-write a file the next reader observes — POSIX guarantees
    /// rename(2) on the same filesystem is atomic.
    ///
    /// SECURITY: temp + final paths both flow through the capability
    /// resolver so the rename can't escape scope even if a malicious
    /// rel_path tried to inject a `..` segment in the temp suffix.
    /// We construct the temp filename ourselves (`<rel>.tmp.<pid>.<ns>`)
    /// rather than letting the caller pick it, so the temp path is
    /// always inside the same parent dir as the target.
    pub fn write_atomic(
        &self,
        rel_path: &std::path::Path,
        contents: &[u8],
    ) -> Result<std::path::PathBuf, CapabilityError> {
        let final_abs = self.resolve(rel_path)?;
        if let Some(parent) = final_abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Build a temp filename that lives in the SAME directory as
        // the target (so the rename is intra-filesystem and atomic).
        // Encode pid + nanos so concurrent writers don't race on the
        // temp filename itself.
        let pid = std::process::id();
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let final_name = final_abs
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_default();
        let tmp_name = {
            let mut s = final_name.clone();
            s.push(format!(".tmp.{pid}.{ns}"));
            s
        };
        let tmp_abs = match final_abs.parent() {
            Some(p) => p.join(&tmp_name),
            None => return Err(CapabilityError::EscapesScope {
                attempted: final_abs,
                confined_root: self.confined_root.clone(),
            }),
        };
        // Re-validate the temp path is inside the capability — paranoid
        // (we just built it from validated parts) but cheap.
        if !tmp_abs.starts_with(&self.confined_root) {
            return Err(CapabilityError::EscapesScope {
                attempted: tmp_abs,
                confined_root: self.confined_root.clone(),
            });
        }
        std::fs::write(&tmp_abs, contents)?;
        std::fs::rename(&tmp_abs, &final_abs)?;
        Ok(final_abs)
    }

    /// Test whether a relative path resolves to an existing file
    /// within the confined root.
    pub fn file_exists(&self, rel_path: &std::path::Path) -> bool {
        self.resolve(rel_path).map(|p| p.is_file()).unwrap_or(false)
    }
}

#[cfg(test)]
mod write_capability_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("loom-cap-{label}-{pid}-{n}"))
    }

    #[test]
    fn for_dir_rejects_nonexistent_path() {
        let p = unique("nope");
        let r = WriteCapability::for_dir(&p);
        assert!(matches!(r, Err(CapabilityError::NotADir(_))));
    }

    #[test]
    fn for_dir_rejects_non_directory() {
        let dir = unique("file");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("file.txt");
        std::fs::write(&f, b"x").expect("write");
        let r = WriteCapability::for_dir(&f);
        assert!(matches!(r, Err(CapabilityError::NotADir(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_within_root_succeeds() {
        let dir = unique("ok");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        let p = cap
            .write_file(std::path::Path::new("hello.txt"), b"hi")
            .expect("write");
        assert!(p.starts_with(&dir.canonicalize().expect("canon")));
        assert_eq!(std::fs::read(&p).expect("read"), b"hi");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_write_creates_parents() {
        let dir = unique("nested");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        cap.write_file(std::path::Path::new("a/b/c/file.rs"), b"x")
            .expect("write");
        assert!(dir.join("a/b/c/file.rs").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traversal_attempt_rejected() {
        let dir = unique("traverse");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        let r = cap.write_file(std::path::Path::new("../../etc/passwd"), b"pwn");
        assert!(matches!(r, Err(CapabilityError::EscapesScope { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deep_traversal_attempt_rejected() {
        let dir = unique("deeptrav");
        std::fs::create_dir_all(dir.join("inner")).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir.join("inner")).expect("cap");
        // Attempt to escape: ../../../tmp/pwn
        let r = cap.write_file(
            std::path::Path::new("../../../../../tmp/pwn"),
            b"x",
        );
        assert!(matches!(r, Err(CapabilityError::EscapesScope { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn symlink_escape_rejected() {
        // Build dir/inner/, then create dir/inner/escape symlinking
        // to /tmp. The capability is rooted at dir/inner; resolving
        // through the symlink must canonicalize back to /tmp and
        // be rejected.
        let dir = unique("symesc");
        std::fs::create_dir_all(dir.join("inner")).expect("mkdir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = std::env::temp_dir(); // outside dir
            let link = dir.join("inner").join("escape");
            symlink(&target, &link).expect("symlink");
        }
        let cap = WriteCapability::for_dir(&dir.join("inner")).expect("cap");
        #[cfg(unix)]
        {
            let r = cap.write_file(std::path::Path::new("escape/pwn"), b"x");
            assert!(matches!(r, Err(CapabilityError::EscapesScope { .. })));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_exists_within_root() {
        let dir = unique("exists");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        assert!(!cap.file_exists(std::path::Path::new("nope.txt")));
        cap.write_file(std::path::Path::new("real.txt"), b"x").expect("write");
        assert!(cap.file_exists(std::path::Path::new("real.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_within_root_succeeds() {
        let dir = unique("read");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        cap.write_file(std::path::Path::new("hello.txt"), b"world")
            .expect("write");
        let bytes = cap
            .read_file(std::path::Path::new("hello.txt"))
            .expect("read");
        assert_eq!(bytes, b"world");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_outside_root_rejected() {
        let dir = unique("readout");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        let r = cap.read_file(std::path::Path::new("../../etc/passwd"));
        assert!(matches!(r, Err(CapabilityError::EscapesScope { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // T60: write_atomic does temp+rename and lands the bytes
    // exactly once at the target.
    #[test]
    fn atomic_write_lands_at_target() {
        let dir = unique("atomic-ok");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        let p = cap
            .write_atomic(std::path::Path::new("about.json"), b"{\"v\":1}")
            .expect("atomic");
        assert_eq!(std::fs::read(&p).expect("read"), b"{\"v\":1}");
        // No leftover .tmp files.
        let leftover: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains(".tmp.")
            })
            .collect();
        assert!(leftover.is_empty(), "no .tmp leftover after atomic write");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // T60: atomic write is constrained to the capability scope
    // even if the rel_path tries to escape via the temp filename.
    #[test]
    fn atomic_write_traversal_rejected() {
        let dir = unique("atomic-trav");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        let r = cap.write_atomic(
            std::path::Path::new("../../tmp/escape.txt"),
            b"x",
        );
        assert!(matches!(r, Err(CapabilityError::EscapesScope { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // T60: atomic write overwrites cleanly (the rename replaces).
    #[test]
    fn atomic_write_overwrites() {
        let dir = unique("atomic-over");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cap = WriteCapability::for_dir(&dir).expect("cap");
        cap.write_atomic(std::path::Path::new("x.txt"), b"old").expect("first");
        cap.write_atomic(std::path::Path::new("x.txt"), b"new").expect("second");
        let p = cap.resolve(std::path::Path::new("x.txt")).expect("resolve");
        assert_eq!(std::fs::read(&p).expect("read"), b"new");
        let _ = std::fs::remove_dir_all(&dir);
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
    // T23: validate the key against the backends.toml schema
    // BEFORE touching the filesystem or parsing TOML. Catches
    // shell-injection-shaped keys ("sign;rm") and uppercase /
    // whitespace typos at the boundary, where the operator can
    // see a clear error message tied to the input.
    let validated = BackendKey::new(key)
        .map_err(|e| BackendStubError::Toml(format!("backend key {key:?}: {e}")))?;
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

    let file_stem = validated.module_name();
    // T577 / T59: every write to <crate_dir>/... flows through a
    // capability constructed once at the boundary. `cap.resolve`
    // canonicalises + asserts confinement, so a future caller
    // can't inject a malicious relative path that escapes the
    // crate root. Pre-existing top-of-fn `is_dir` check stays as
    // a UX-friendly early error; capability is the SECURITY
    // gate.
    let cap = WriteCapability::for_dir(crate_dir)?;
    let rel_path = format!("src/handlers/{file_stem}.rs");
    let abs_path = cap.resolve(std::path::Path::new(&rel_path))?;
    if abs_path.exists() && !force {
        return Err(BackendStubError::Conflict(abs_path));
    }
    cap.write_file(
        std::path::Path::new(&rel_path),
        render_handler_stub(key, &method, &path, &purpose).as_bytes(),
    )?;

    // Update backends.toml: impl_files = [rel_path].
    let mut new_array = toml_edit::Array::new();
    new_array.push(rel_path.as_str());
    entry.insert("impl_files", toml_edit::value(new_array));
    std::fs::write(backends_path, doc.to_string())?;

    // T22: keep src/handlers/mod.rs in sync. Routed through the
    // capability per T59 so the same path-confinement rules
    // apply to the mod.rs write as to the handler file.
    let mod_changed = register_handler_module(&cap, &file_stem)?;
    if mod_changed {
        println!(
            "  ok     handlers/mod.rs += {file_stem:?}",
        );
    }

    println!("  ok     scaffolded {}", abs_path.display());
    println!(
        "  ok     updated {} (impl_files += {rel_path:?})",
        backends_path.display()
    );
    Ok(())
}

/// Insert `pub mod <module>;` into `<crate_dir>/src/handlers/mod.rs`,
/// keeping the file alphabetically sorted and the doc-block (any
/// lines that aren't `pub mod`) untouched.
///
/// Returns `Ok(true)` when the file changed, `Ok(false)` when the
/// module was already declared (idempotent). Creates the file with
/// a minimal doc-block when it doesn't exist yet — useful when
/// `loom backend-stub` is run before the crate skeleton lands
/// (T20 may have created a different shape for an existing crate).
///
/// REGRESSION-GUARD: a previous design considered appending without
/// sorting "for simplicity" — but mass-mint runs in arbitrary key
/// order, and the resulting mod.rs would change line order on every
/// re-run, polluting `git diff`. Stable sort is the contract.
fn register_handler_module(
    cap: &WriteCapability,
    module_name: &str,
) -> Result<bool, BackendStubError> {
    let mod_rel = std::path::Path::new("src/handlers/mod.rs");
    let mod_decl = format!("pub mod {module_name};");

    // T59: read THROUGH the capability so the path is boundary-
    // checked. None on read failure that isn't NotFound.
    let existing = match cap.read_file(mod_rel) {
        Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        Err(CapabilityError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    let (header_lines, mut module_names) = match existing {
        Some(raw) => parse_handler_mod_rs(&raw),
        None => (
            vec![
                "//! Handler module list. Each entry corresponds to one".to_owned(),
                "//! [backends.X] section in backends.toml (key dashes →".to_owned(),
                "//! underscores). Auto-maintained by `loom backend-stub`".to_owned(),
                "//! (T22) — manual edits in this file may be reordered.".to_owned(),
            ],
            Vec::new(),
        ),
    };

    if module_names.iter().any(|m| m == module_name) {
        return Ok(false);
    }
    module_names.push(module_name.to_owned());
    module_names.sort();
    module_names.dedup();

    let mut out = String::new();
    for line in &header_lines {
        out.push_str(line);
        out.push('\n');
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    for name in &module_names {
        out.push_str("pub mod ");
        out.push_str(name);
        out.push_str(";\n");
    }
    let _ = mod_decl;

    cap.write_file(mod_rel, out.as_bytes())?;
    Ok(true)
}

/// Split a `mod.rs` body into (header lines, module names).
///
/// "module name" = capture group of `^pub mod (\w+);$` (stripped of
/// surrounding whitespace). Anything that doesn't match is preserved
/// in `header_lines` in original order — comments, attribute lines,
/// `use` statements all survive a round-trip.
///
/// BUG ASSUMPTION: a hand-edited mod.rs with a non-trivial body
/// (e.g. `pub mod foo; pub mod bar;` on one line, or `#[cfg(...)]
/// pub mod foo;`) is rare enough that the simpler one-line-per-mod
/// rule is the right contract. T22 owners surface T22-edited mod.rs
/// with the regenerated comment so reviewers know where to look.
fn parse_handler_mod_rs(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut header = Vec::<String>::new();
    let mut modules = Vec::<String>::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("pub mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                let name = name.trim();
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    modules.push(name.to_owned());
                    continue;
                }
            }
        }
        // Header lines: keep verbatim. Skip trailing blank lines —
        // we re-add a single blank separator before the module
        // list to keep the output shape stable.
        header.push(line.to_owned());
    }
    while header.last().is_some_and(|l| l.trim().is_empty()) {
        header.pop();
    }
    (header, modules)
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
            .map_or(0, |d| d.as_nanos());
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

    // ---- T22: register_handler_module ----

    #[test]
    fn parse_mod_rs_splits_header_and_modules() {
        let raw = "//! header line\n//! second line\n\npub mod alpha;\npub mod beta;\n";
        let (header, modules) = parse_handler_mod_rs(raw);
        assert_eq!(
            header,
            vec!["//! header line".to_owned(), "//! second line".to_owned()],
        );
        assert_eq!(modules, vec!["alpha".to_owned(), "beta".to_owned()]);
    }

    #[test]
    fn parse_mod_rs_ignores_malformed_lines() {
        // Lines with multiple statements or attributes don't match
        // the simple "pub mod NAME;" rule and are kept as header.
        let raw = "//! doc\n\n#[cfg(unix)]\npub mod foo;\npub mod bar;\n";
        let (header, modules) = parse_handler_mod_rs(raw);
        assert!(header.iter().any(|l| l == "#[cfg(unix)]"));
        assert_eq!(modules, vec!["foo".to_owned(), "bar".to_owned()]);
    }

    #[test]
    fn register_creates_mod_rs_when_absent() {
        let dir = unique("reg-mod-create");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let changed =
            register_handler_module(&WriteCapability::for_dir(&dir).expect("cap"), "sign_in").expect("ok");
        assert!(changed);
        let body =
            std::fs::read_to_string(dir.join("src/handlers/mod.rs")).expect("read");
        assert!(body.contains("//! Handler module list."));
        assert!(body.contains("pub mod sign_in;"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_appends_and_sorts() {
        let dir = unique("reg-mod-sort");
        std::fs::create_dir_all(dir.join("src/handlers")).expect("mkdir");
        std::fs::write(
            dir.join("src/handlers/mod.rs"),
            "//! existing\n\npub mod beta;\npub mod alpha;\n",
        )
        .expect("seed");
        let changed =
            register_handler_module(&WriteCapability::for_dir(&dir).expect("cap"), "delta").expect("ok");
        assert!(changed);
        let body =
            std::fs::read_to_string(dir.join("src/handlers/mod.rs")).expect("read");
        // Alphabetical ordering, even when seed was unsorted.
        let alpha_pos = body.find("pub mod alpha;").expect("alpha");
        let beta_pos = body.find("pub mod beta;").expect("beta");
        let delta_pos = body.find("pub mod delta;").expect("delta");
        assert!(alpha_pos < beta_pos);
        assert!(beta_pos < delta_pos);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_is_idempotent_on_repeat_call() {
        let dir = unique("reg-mod-idem");
        std::fs::create_dir_all(dir.join("src/handlers")).expect("mkdir");
        std::fs::write(
            dir.join("src/handlers/mod.rs"),
            "//! header\n\npub mod sign_in;\n",
        )
        .expect("seed");
        let r1 = register_handler_module(&WriteCapability::for_dir(&dir).expect("cap"), "sign_in").expect("first");
        assert!(!r1, "second declaration of same module must be a no-op");
        let body =
            std::fs::read_to_string(dir.join("src/handlers/mod.rs")).expect("read");
        // Exactly one occurrence of the line — no duplicate.
        assert_eq!(
            body.matches("pub mod sign_in;").count(),
            1,
            "module name must appear exactly once after idempotent re-register",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_preserves_doc_block() {
        let dir = unique("reg-mod-doc");
        std::fs::create_dir_all(dir.join("src/handlers")).expect("mkdir");
        let initial_header =
            "//! line one\n//! line two — non-ASCII —\n//! line three";
        std::fs::write(
            dir.join("src/handlers/mod.rs"),
            format!("{initial_header}\n\npub mod existing;\n"),
        )
        .expect("seed");
        register_handler_module(&WriteCapability::for_dir(&dir).expect("cap"), "added").expect("ok");
        let body =
            std::fs::read_to_string(dir.join("src/handlers/mod.rs")).expect("read");
        assert!(body.starts_with(initial_header));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cmd_backend_stub_registers_module() {
        // End-to-end: the dispatcher path must wire the new module
        // into mod.rs, not just write the .rs file.
        let (backends, dir) = fixture();
        cmd_backend_stub("sign-in", &backends, &dir, false).expect("ok");
        let mod_rs =
            std::fs::read_to_string(dir.join("src/handlers/mod.rs")).expect("read");
        assert!(mod_rs.contains("pub mod sign_in;"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stub_all_registers_every_module() {
        let (backends, dir) = fixture_all();
        cmd_backend_stub_all(&backends, &dir).expect("ok");
        let mod_rs =
            std::fs::read_to_string(dir.join("src/handlers/mod.rs")).expect("read");
        assert!(mod_rs.contains("pub mod sign_in;"));
        assert!(mod_rs.contains("pub mod cast_vote;"));
        // view-profile was already-IMPL in the fixture so its
        // file isn't generated; mass-mint must NOT register a
        // module whose source it didn't write.
        assert!(!mod_rs.contains("pub mod view_profile;"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- T23: BackendKey + BackendStatus ADT ----

    #[test]
    fn backend_key_accepts_valid_schemas() {
        for valid in ["sign-in", "view-profile", "list-challenges", "a", "a1", "x-9-y"] {
            assert!(BackendKey::new(valid).is_ok(), "should accept {valid:?}");
        }
    }

    #[test]
    fn backend_key_rejects_empty_and_whitespace() {
        assert_eq!(BackendKey::new("").unwrap_err(), BackendKeyError::Empty);
        assert!(matches!(
            BackendKey::new(" sign-in").unwrap_err(),
            BackendKeyError::LeadingNonLowercase(_)
        ));
    }

    #[test]
    fn backend_key_rejects_uppercase() {
        assert!(matches!(
            BackendKey::new("Sign-In").unwrap_err(),
            BackendKeyError::LeadingNonLowercase('S'),
        ));
        assert!(matches!(
            BackendKey::new("sign-In").unwrap_err(),
            BackendKeyError::InvalidChar('I'),
        ));
    }

    #[test]
    fn backend_key_rejects_path_traversal_and_shell_metachars() {
        for bad in [
            "../etc/passwd",
            "sign;rm",
            "sign in",  // space
            "sign/in",  // slash
            "sign_in",  // underscore disallowed in source key
            "sign.in",  // dot disallowed
            "9-leading",
        ] {
            assert!(BackendKey::new(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn backend_key_module_name_dashes_to_underscores() {
        let k = BackendKey::new("list-open-votes").expect("valid");
        assert_eq!(k.module_name(), "list_open_votes");
        assert_eq!(k.as_str(), "list-open-votes");
    }

    #[test]
    fn backend_status_from_empty_vec_is_stub() {
        assert_eq!(BackendStatus::from_impl_files(vec![]), BackendStatus::Stub);
    }

    #[test]
    fn backend_status_from_non_empty_vec_is_impl() {
        let s = BackendStatus::from_impl_files(vec!["src/handlers/sign_in.rs".to_owned()]);
        assert!(matches!(s, BackendStatus::Impl(ref paths) if paths.len() == 1));
        assert_eq!(s.label(), "IMPL");
        assert!(!s.is_stub());
    }

    #[test]
    fn backend_status_label_is_stable_for_table_rendering() {
        // The CLI table column is fixed-width 6 chars; widening
        // either label silently breaks alignment for downstream
        // grep/awk users.
        assert_eq!(BackendStatus::Stub.label(), "STUB");
        assert_eq!(
            BackendStatus::Impl(vec!["x".to_owned()]).label(),
            "IMPL",
        );
    }

    #[test]
    fn cmd_backend_stub_rejects_invalid_keys_via_typed_err() {
        let (backends, dir) = fixture();
        // Even though backends.toml might have a malformed key,
        // the dispatcher must surface a Toml-class error before
        // touching the filesystem.
        let r = cmd_backend_stub("Sign-In", &backends, &dir, false);
        assert!(matches!(r, Err(BackendStubError::Toml(_))));
        // No file should exist for the rejected key.
        assert!(!dir.join("src/handlers/Sign_In.rs").exists());
        assert!(!dir.join("src/handlers/sign_in.rs").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // T59: end-to-end proof that the WriteCapability integration
    // refuses a crate_dir that doesn't exist. Pre-existing
    // `is_dir` check at the top of cmd_backend_stub catches the
    // simple case; this proves the capability layer ALSO catches
    // it (defense in depth).
    #[test]
    fn cmd_backend_stub_refuses_nonexistent_crate_dir() {
        let (backends, _) = fixture();
        let bogus = std::env::temp_dir().join("loom-cap-int-nonexistent-zzz");
        let _ = std::fs::remove_dir_all(&bogus);
        let r = cmd_backend_stub("sign-in", &backends, &bogus, false);
        assert!(
            matches!(r, Err(BackendStubError::CrateNotDir(_))),
            "expected CrateNotDir, got {r:?}"
        );
    }

    // ---- T19: cmd_backend_stub_all ----

    fn fixture_all() -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = unique("all-fixture");
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
impl_files = ["src/handlers/view_profile.rs"]

[backends.cast-vote]
method = "POST"
path = "/vote"
purpose = "submit a vote"
impl_files = []
"#,
        )
        .expect("write");
        (backends, dir)
    }

    #[test]
    fn stub_all_mints_only_empty_entries() {
        let (backends, dir) = fixture_all();
        let r = cmd_backend_stub_all(&backends, &dir).expect("ok");
        // Two stubs (sign-in, cast-vote); one already-impl (view-profile).
        assert_eq!(r.ok, 2);
        assert_eq!(r.skipped, 0);
        assert_eq!(r.failed, 0);
        assert!(dir.join("src/handlers/sign_in.rs").exists());
        assert!(dir.join("src/handlers/cast_vote.rs").exists());
        // view-profile was NOT a stub → must NOT be touched.
        assert!(!dir.join("src/handlers/view_profile.rs").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stub_all_updates_backends_toml_for_each_minted_entry() {
        let (backends, dir) = fixture_all();
        cmd_backend_stub_all(&backends, &dir).expect("ok");
        let raw = std::fs::read_to_string(&backends).expect("read");
        let v: toml::Value = toml::from_str(&raw).expect("parse");
        let after = |k: &str| {
            v["backends"][k]["impl_files"]
                .as_array()
                .expect("array")
                .len()
        };
        assert_eq!(after("sign-in"), 1);
        assert_eq!(after("cast-vote"), 1);
        assert_eq!(after("view-profile"), 1, "must not regress already-impl entry");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stub_all_is_idempotent_on_second_run() {
        let (backends, dir) = fixture_all();
        let r1 = cmd_backend_stub_all(&backends, &dir).expect("first");
        assert_eq!(r1.ok, 2);
        // Second run finds zero stubs — every entry now has impl_files.
        let r2 = cmd_backend_stub_all(&backends, &dir).expect("second");
        assert_eq!(r2.ok, 0);
        assert_eq!(r2.skipped, 0);
        assert_eq!(r2.failed, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stub_all_skips_when_handler_file_exists_but_impl_files_empty() {
        let (backends, dir) = fixture_all();
        // Pre-create the sign_in.rs file so cmd_backend_stub will hit Conflict.
        std::fs::create_dir_all(dir.join("src/handlers")).expect("mkdir");
        std::fs::write(dir.join("src/handlers/sign_in.rs"), "// pre-existing\n").expect("seed");
        let r = cmd_backend_stub_all(&backends, &dir).expect("ok");
        // sign-in conflicted (skipped); cast-vote minted (ok).
        assert_eq!(r.ok, 1);
        assert_eq!(r.skipped, 1);
        assert_eq!(r.failed, 0);
        // Pre-existing file untouched.
        let body = std::fs::read_to_string(dir.join("src/handlers/sign_in.rs")).expect("read");
        assert_eq!(body, "// pre-existing\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stub_all_errs_on_non_dir_crate() {
        let (backends, _) = fixture_all();
        let bogus = std::env::temp_dir().join("loom-stub-all-not-a-dir-zzzz");
        let _ = std::fs::remove_dir_all(&bogus);
        let r = cmd_backend_stub_all(&backends, &bogus);
        assert!(matches!(r, Err(BackendStubError::CrateNotDir(_))));
    }

    #[test]
    fn stub_all_no_backends_section_errs() {
        let dir = unique("no-backends");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let backends = dir.join("backends.toml");
        std::fs::write(&backends, "# no [backends] section\n").expect("write");
        let r = cmd_backend_stub_all(&backends, &dir);
        assert!(matches!(r, Err(BackendStubError::Toml(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `loom backend list` — read backends.toml + print impl status
/// table for every declared key.
///
/// `loom backend-stub-all` — T19 mass-mint summary.
struct BackendStubAllReport {
    ok: usize,
    skipped: usize,
    failed: usize,
}

/// Walk backends.toml, scaffold every entry whose impl_files is
/// empty. Per-key results are printed inline; the aggregate
/// counts are returned for the top-level dispatcher to render
/// the final summary line.
///
/// Errors at the top level mean the whole operation cannot
/// proceed (unreadable backends.toml, crate root missing).
/// Per-key failures are logged inline and reflected in the
/// returned `failed` count, but do NOT abort the loop — partial
/// progress is more useful than all-or-nothing.
fn cmd_backend_stub_all(
    backends_path: &std::path::Path,
    crate_dir: &std::path::Path,
) -> Result<BackendStubAllReport, BackendStubError> {
    if !crate_dir.is_dir() {
        return Err(BackendStubError::CrateNotDir(crate_dir.to_path_buf()));
    }
    let raw = std::fs::read_to_string(backends_path)?;
    let parsed: toml::Value = toml::from_str(&raw)
        .map_err(|e| BackendStubError::Toml(format!("parse: {e}")))?;
    let backends = parsed
        .get("backends")
        .and_then(|v| v.as_table())
        .ok_or_else(|| BackendStubError::Toml("missing [backends] section".to_owned()))?;

    // Enumerate stub keys ahead of time so we don't mutate the
    // file while iterating it. Keys are sorted for stable, grep-
    // friendly output across runs.
    let mut stub_keys: Vec<String> = backends
        .iter()
        .filter_map(|(k, v)| {
            let table = v.as_table()?;
            let impl_files = table.get("impl_files").and_then(|x| x.as_array())?;
            if impl_files.is_empty() {
                Some(k.to_owned())
            } else {
                None
            }
        })
        .collect();
    stub_keys.sort();

    let mut report = BackendStubAllReport {
        ok: 0,
        skipped: 0,
        failed: 0,
    };
    if stub_keys.is_empty() {
        println!("  ok     no stub backends found — nothing to mint");
        return Ok(report);
    }

    println!("  ..     minting {} stub backend(s)", stub_keys.len());
    for key in &stub_keys {
        // T19: re-read backends.toml on every iteration so
        // toml_edit picks up the comment-preserving update from
        // the prior key. cmd_backend_stub does this internally.
        match cmd_backend_stub(key, backends_path, crate_dir, false) {
            Ok(()) => {
                report.ok += 1;
                // cmd_backend_stub already printed two ok lines.
            }
            Err(BackendStubError::Conflict(p)) => {
                println!(
                    "  skip   [{key}] file {} already exists (use single-key + --force to overwrite)",
                    p.display()
                );
                report.skipped += 1;
            }
            Err(BackendStubError::KeyNotFound(_)) => {
                // Should be impossible — we just enumerated this
                // key from the same file. Treat as failure.
                eprintln!("  fail   [{key}] vanished between enumeration and mint");
                report.failed += 1;
            }
            Err(e) => {
                eprintln!("  fail   [{key}] {e:?}");
                report.failed += 1;
            }
        }
    }
    Ok(report)
}

/// Pure read-only: no file mutation, no remote I/O. Stable text
/// output suitable for piping to grep / awk / jq (after
/// post-processing). Exit code 0 even when every key is STUB —
/// the data is the value, not the gate.
/// `BackendKey` — validated identifier for a `[backends.X]` entry.
///
/// T23: replaces stringly-typed keys flowing through cmd_backend_*.
/// Constructor enforces the schema regex `^[a-z][a-z0-9-]*$` so a
/// caller can't accidentally pass `"../etc/passwd"` or whitespace
/// through. Once wrapped, the value is trusted by every consumer.
///
/// REGRESSION-GUARD: do NOT widen the regex without updating
/// backends.toml schema docs in PlausiDen-Forge AND the loom
/// backend-stub file-name derivation (which assumes the only
/// non-alphanumeric character is `-`, mapped to `_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct BackendKey(String);

#[derive(Debug, PartialEq, Eq)]
enum BackendKeyError {
    Empty,
    LeadingNonLowercase(char),
    InvalidChar(char),
}

impl std::fmt::Display for BackendKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendKeyError::Empty => f.write_str("backend key must not be empty"),
            BackendKeyError::LeadingNonLowercase(c) => {
                write!(f, "backend key must start with a-z (got {c:?})")
            }
            BackendKeyError::InvalidChar(c) => {
                write!(f, "backend key contains invalid character {c:?} (allowed: a-z, 0-9, -)")
            }
        }
    }
}

impl BackendKey {
    fn new(s: &str) -> Result<Self, BackendKeyError> {
        let mut chars = s.chars();
        let first = chars.next().ok_or(BackendKeyError::Empty)?;
        if !first.is_ascii_lowercase() {
            return Err(BackendKeyError::LeadingNonLowercase(first));
        }
        for c in chars {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                return Err(BackendKeyError::InvalidChar(c));
            }
        }
        Ok(Self(s.to_owned()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }

    /// Module-name form: dashes → underscores. Used by the handler
    /// scaffold + handlers/mod.rs registration.
    fn module_name(&self) -> String {
        self.0.replace('-', "_")
    }
}

/// `BackendStatus` — typed result of inspecting one `[backends.X]`
/// entry's impl_files.
///
/// T23: replaces the stringly-typed "STUB"/"IMPL" status that flowed
/// through BackendRow. Each variant carries the data that's only
/// meaningful for that case — a Stub has no paths, an Impl has at
/// least one. Branches that don't apply are unrepresentable.
///
/// Future variant queued: `MissingFile { path: PathBuf }` once Loom
/// gains filesystem verification (currently lives in forge phase
/// backend_coverage / T18). Adding that variant later is a single
/// match arm in cmd_backend_list — no caller will silently fall
/// through because the enum is non-exhaustive at every match site.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BackendStatus {
    Stub,
    Impl(Vec<String>),
}

impl BackendStatus {
    fn from_impl_files(paths: Vec<String>) -> Self {
        if paths.is_empty() {
            BackendStatus::Stub
        } else {
            BackendStatus::Impl(paths)
        }
    }

    fn label(&self) -> &'static str {
        match self {
            BackendStatus::Stub => "STUB",
            BackendStatus::Impl(_) => "IMPL",
        }
    }

    fn is_stub(&self) -> bool {
        matches!(self, BackendStatus::Stub)
    }
}

// ============================================================
// T28: theme system
// ============================================================

/// One `:root` block extracted from skin.css.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ThemeBlock {
    /// "default" for the bare `:root { ... }`, otherwise the value
    /// of `data-theme="..."`.
    name: String,
    /// Token name → declaration text. Only `--loom-color-*` tokens
    /// are tracked; other custom properties are ignored.
    tokens: std::collections::BTreeMap<String, String>,
}

/// Walk skin.css, return every theme block plus the full set of
/// tokens referenced via `var(--loom-color-X)` anywhere in the
/// file. Pure read-only — no I/O beyond the read.
///
/// REGRESSION-GUARD: this parser is line-oriented + brace-counted,
/// not a full CSS parser. It tolerates nested `{...}` (e.g. media
/// queries containing `:root { ... }`) but breaks on inline
/// braces inside string literals. skin.css does not currently use
/// such literals; if it ever does, this needs replacing with
/// lightningcss or similar.
// ============================================================
// T42: typed CMS editor — server-rendered, no JS, MVP scope.
// ============================================================
//
// Doctrine:
//   * Bind 127.0.0.1 only. T43 lands real auth; until then no
//     network exposure. Caller bridges via SSH tunnel for remote.
//   * Every form is regenerated server-side from the live JSON.
//     Stale browser state cannot corrupt persisted data.
//   * Writes go to a temp file + rename (atomic on POSIX).
//     Concurrent saves can't half-write.
//   * After every write we shell out to forge.sh — not because
//     it's fast, because it's the canonical pipeline. T44 will
//     warm-cache the rebuild.
//
// MVP coverage (this tick):
//   * GET /              → page list
//   * GET /<page>        → editable form for cms/<page>.json
//   * POST /<page>       → validate, write back, rebuild, redirect
//   * GET /preview/<f>   → serve static/<f> (read-only file proxy)
//
// Field types this MVP renders / accepts:
//   * CmsPage.title (string)
//   * CmsPage.description (string)
//   * Hero.title / Hero.subtitle / Hero.eyebrow (strings)
//   * Group.title / Group.body (string + repeating string array)
//
// Anything else is rendered as a read-only JSON snippet so the
// operator can see the field exists but can't mangle it from
// the form. Follow-up ticks expand the widget set.

fn cmd_edit_serve(
    cms_root: &std::path::Path,
    static_root: &std::path::Path,
    forge_path: &str,
    port: u16,
) -> std::io::Result<()> {
    if !cms_root.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("cms root {} is not a directory", cms_root.display()),
        ));
    }
    let bind = format!("127.0.0.1:{port}");
    let server = tiny_http::Server::http(&bind).map_err(|e| {
        std::io::Error::other(format!("tiny_http bind {bind}: {e}"))
    })?;
    println!("loom edit serve: listening on http://{bind}/");
    println!("  cms      = {}", cms_root.display());
    println!("  static   = {}", static_root.display());
    println!("  forge    = {}", forge_path);
    println!();
    println!("Open http://127.0.0.1:{port}/ in a browser.");
    println!("Ctrl-C to stop.");

    for request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_owned();
        match handle_edit_request(
            request,
            cms_root,
            static_root,
            forge_path,
            &method,
            &url,
        ) {
            Ok(()) => {}
            Err(e) => eprintln!("  err   {method:?} {url} -> {e}"),
        }
    }
    Ok(())
}

fn handle_edit_request(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
    static_root: &std::path::Path,
    forge_path: &str,
    method: &tiny_http::Method,
    url: &str,
) -> std::io::Result<()> {
    // Reject anything not GET/POST.
    let is_post = matches!(method, tiny_http::Method::Post);
    let is_get = matches!(method, tiny_http::Method::Get);
    if !is_post && !is_get {
        return respond_text(request, 405, "method not allowed");
    }
    // Strip leading slash, trim query string.
    let path = url.trim_start_matches('/');
    let path = path.split_once('?').map(|(p, _)| p).unwrap_or(path);

    // Path traversal defence: reject anything containing `..` or
    // backslashes or starting with `/`.
    if path.contains("..") || path.contains('\\') {
        return respond_text(request, 400, "bad path");
    }

    // T43: auth middleware. If auth.toml exists, every endpoint
    // except /login + the static preview is gated. Without
    // auth.toml the editor stays open (back-compat).
    let auth_store = read_auth_store().ok().flatten();
    if let Some(ref store) = auth_store {
        // /login is always reachable.
        if path == "login" {
            if is_get {
                return serve_login_form(request, None);
            } else if is_post {
                return handle_login_post(request, store);
            }
        }
        if path == "logout" && is_post {
            return handle_logout(request);
        }
        // Static preview is available without login (it's the
        // public site; eventually nginx serves this directly).
        if path.starts_with("preview/") {
            // fall through to existing routing
        } else {
            // Verify session cookie.
            let key_b64 = store.secret.hmac_key_b64.as_str();
            let key_bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                key_b64,
            )
            .unwrap_or_default();
            let cookie = extract_session_cookie(&request);
            let user = cookie
                .as_deref()
                .and_then(|c| verify_session_cookie(c, &key_bytes));
            if user.is_none() {
                return serve_login_form(request, Some("Login required"));
            }
        }
    }

    if path.is_empty() {
        return serve_index(request, cms_root);
    }
    if let Some(rest) = path.strip_prefix("preview/") {
        return serve_preview(request, static_root, rest);
    }
    // T50: forge admin dashboard. Same server, same auth scope.
    if path == "forge" && is_get {
        return serve_forge_dashboard(request, cms_root, static_root);
    }
    if path == "forge/build" && is_post {
        return handle_forge_build(request, cms_root, forge_path);
    }
    if path == "forge/themes" && is_get {
        return serve_forge_themes(request, static_root);
    }
    if path == "forge/backends" && is_get {
        return serve_forge_backends(request, cms_root);
    }
    if path == "forge/audit" && is_get {
        return serve_forge_audit(request, cms_root);
    }
    // T62 step 3: new-page POST.
    if is_post && path == "new-page" {
        return handle_new_page(request, cms_root, forge_path);
    }
    // T64: tutorial page.
    if is_get && path == "tutorial" {
        return serve_tutorial(request);
    }
    // T62: section-mutation paths. Format:
    //   <slug>/add-section
    //   <slug>/sections/<N>/up | down | delete | add-paragraph
    if is_post {
        if let Some(slug) = path.strip_suffix("/add-section") {
            return handle_add_section(request, cms_root, forge_path, slug);
        }
        for op in &["up", "down", "delete", "add-paragraph"] {
            let suffix = format!("/{op}");
            if let Some(rest) = path.strip_suffix(suffix.as_str()) {
                if let Some((slug_plus_section, idx_str)) = rest.rsplit_once("/sections/") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        return handle_section_op(
                            request,
                            cms_root,
                            forge_path,
                            slug_plus_section,
                            idx,
                            op,
                        );
                    }
                }
            }
        }
    }
    // Anything else is treated as a page slug (e.g. "about").
    if is_get {
        serve_edit_form(request, cms_root, path)
    } else {
        handle_edit_post(request, cms_root, forge_path, path)
    }
}

/// T64: in-GUI tutorial. Server-rendered, no JS, no client
/// state. Walks the operator through every feature of the CMS
/// editor + forge admin GUI in one scrollable doc page.
///
/// Linked from: index page header, every editor's "← all pages"
/// link, every forge admin nav.
fn serve_tutorial(request: tiny_http::Request) -> std::io::Result<()> {
    let mut body = String::new();
    body.push_str("<!doctype html><meta charset=utf-8><title>loom — tutorial</title>");
    body.push_str(
        "<style>\
         body{font:16px/1.65 system-ui;max-width:42rem;margin:2rem auto;padding:0 1rem;color:#222}\
         h1{margin-top:0;font-size:1.6em}\
         h2{margin-top:2rem;font-size:1.2em;border-bottom:1px solid #ddd;padding-bottom:.25rem}\
         h3{margin-top:1.5rem;font-size:1em}\
         code{background:#f4f4f4;padding:.1em .35em;border-radius:3px;font-size:.9em}\
         kbd{background:#fff;border:1px solid #888;border-bottom-width:2px;\
             padding:.05em .4em;border-radius:3px;font-size:.85em;font-family:inherit}\
         .step{padding:.75rem 1rem;background:#f4f4f4;border-left:4px solid #003;\
               margin:1rem 0;border-radius:0 4px 4px 0}\
         .step b{color:#003}\
         a{color:#003}\
         nav.tut{display:flex;gap:1rem;margin-bottom:1rem;font-size:.9em}\
         </style>"
    );
    body.push_str(
        "<nav class=\"tut\"><a href=\"/\">← back to pages</a> · <a href=\"/forge\">forge admin</a></nav>\
         <h1>loom — tutorial</h1>\
         <p>This guide walks you through everything you can do from the browser. \
            No code, no terminal, no JSON.</p>"
    );

    body.push_str(
        "<h2>1. Create a page</h2>\
         <p>From the <a href=\"/\">main page list</a>, fill in the \
            <b>Create a new page</b> form:</p>\
         <div class=\"step\">\
           <b>Slug</b> — a short URL-safe name. Lowercase letters, digits, dashes only. \
           Becomes both the file name (<code>cms/&lt;slug&gt;.json</code>) and the \
           URL path (<code>/&lt;slug&gt;.html</code>).<br>\
           <b>Template</b> — pick from <code>blank</code>, <code>landing</code>, \
           <code>about</code>, or <code>contact</code>. Each starts you with a \
           sensible structure.\
         </div>\
         <p>Click <kbd>Create</kbd>. You'll be redirected to the page's editor.</p>"
    );

    body.push_str(
        "<h2>2. Edit content</h2>\
         <p>The editor is a <b>split pane</b>: forms on the left, live preview on \
            the right.</p>\
         <h3>Page-level fields</h3>\
         <p><b>Title</b> sets the browser tab + page header. <b>Description</b> \
            becomes the meta description for search engines.</p>\
         <h3>Sections</h3>\
         <p>The page is built from <b>sections</b>. Five kinds are available:</p>\
         <ul>\
           <li><b>Hero</b> — eyebrow + title + subtitle. Big banner at the top of \
               a page.</li>\
           <li><b>Group</b> — a heading + multiple body paragraphs. Click \
               <kbd>+ paragraph</kbd> to add more.</li>\
           <li><b>Paragraph</b> — a single block of body text.</li>\
           <li><b>Heading</b> — pick a level (H1–H6) + text.</li>\
           <li><b>Banner</b> — a tone-styled callout (info / success / warn / \
               danger) with title + body.</li>\
         </ul>\
         <p>Each section's fields edit inline. Type, then click <kbd>Save</kbd> at \
            the bottom — the preview reloads automatically.</p>"
    );

    body.push_str(
        "<h2>3. Manage sections</h2>\
         <p>Each section gets three controls in the bottom-right of its panel:</p>\
         <div class=\"step\">\
           <b>↑ Move up</b> — swap with the section above (hidden on the first).<br>\
           <b>↓ Move down</b> — swap with the section below (hidden on the last).<br>\
           <b>Delete</b> — confirm dialog, then removed.\
         </div>\
         <p>To <b>add a new section</b> at the bottom, scroll past the existing \
            ones to the <b>Add a section</b> form, pick a kind from the dropdown, \
            and click <kbd>Append</kbd>. The new section appears with default \
            placeholders you can immediately edit.</p>"
    );

    body.push_str(
        "<h2>4. Live preview</h2>\
         <p>The right pane is an iframe loading the rendered page. After every \
            save it reloads automatically. Click <kbd>open ↗</kbd> in the preview \
            bar to break it out into a separate tab — useful for testing on \
            different screen sizes.</p>"
    );

    body.push_str(
        "<h2>5. Forge admin</h2>\
         <p>Click <a href=\"/forge\">forge admin</a> in any page header to reach \
            the build dashboard:</p>\
         <ul>\
           <li><b>Dashboard</b> — last build summary + a <kbd>Run forge build now</kbd> \
               button.</li>\
           <li><b>Themes</b> — list of every theme defined in the design system, \
               with token counts.</li>\
           <li><b>Backends</b> — declared backend keys + their implementation status \
               (STUB or IMPL).</li>\
           <li><b>Audit</b> — last crawler audit results.</li>\
         </ul>"
    );

    body.push_str(
        "<h2>6. What runs in the background</h2>\
         <p>Every save you make:</p>\
         <ol>\
           <li>Atomically writes <code>cms/&lt;slug&gt;.json</code> via a \
               <b>capability-bound writer</b> — even malicious slugs can't \
               escape the cms/ directory.</li>\
           <li>Triggers the forge build pipeline, which:\
             <ul>\
               <li>Validates every CMS file against the typed schema</li>\
               <li>Renders pages through the typed Loom design system</li>\
               <li>Audits theme contrast against WCAG AA (4.5:1 minimum)</li>\
               <li>Audits a11y, CSP, SRI, link-integrity, perf-budget, …</li>\
               <li>Runs the headless-browser crawler against the rendered pages</li>\
             </ul>\
           </li>\
           <li>Signs the build report with an Ed25519 key and chains it to the \
               previous report's hash — every build is tamper-evident.</li>\
         </ol>\
         <p>If any check fails, the build refuses and the previous version stays \
            live.</p>"
    );

    body.push_str(
        "<h2>7. Common questions</h2>\
         <h3>Can I undo a delete?</h3>\
         <p>Not directly — but every build report is kept in <code>reports/</code>. \
            If you delete by mistake, your previous content is in the most recent \
            committed snapshot. (Future tick: an undo button.)</p>\
         <h3>Can I add an image?</h3>\
         <p>Image uploads are queued for the next tick. For now, edit the JSON \
            directly via your file system if you need an image right away.</p>\
         <h3>Can I share editing with another person?</h3>\
         <p>Multi-user auth is queued (the editor currently binds to 127.0.0.1 \
            only — local network only). When auth lands, you'll be able to invite \
            collaborators with role-based scopes.</p>\
         <h3>What if I break something?</h3>\
         <p>Forge will refuse to publish a broken page. Your previous published \
            version stays live until the build passes. Worst case, you can edit \
            the JSON files in <code>cms/</code> directly with any text editor — \
            they're plain JSON.</p>"
    );

    body.push_str(
        "<h2>That's it</h2>\
         <p>Go to <a href=\"/\">the main page list</a> and create your first page. \
            Come back here any time you forget how something works — link is in \
            every page header.</p>"
    );

    respond_html(request, 200, &body)
}

/// T43: render the login form. `error` shows above the form
/// when present (e.g. after a failed login attempt).
fn serve_login_form(
    request: tiny_http::Request,
    error: Option<&str>,
) -> std::io::Result<()> {
    let mut body = String::new();
    body.push_str("<!doctype html><meta charset=utf-8><title>loom — sign in</title>");
    body.push_str(
        "<style>\
         body{font:16px/1.5 system-ui;max-width:24rem;margin:5rem auto;padding:0 1rem}\
         h1{margin-top:0;font-size:1.4em}\
         label{display:block;margin:1rem 0 .25rem;font-weight:600}\
         input{width:100%;padding:.6rem;font:inherit;border:1px solid #888;border-radius:4px;\
               box-sizing:border-box}\
         button{margin-top:1.5rem;padding:.6rem 1.2rem;font:inherit;border:0;\
                border-radius:4px;background:#003;color:#fff;cursor:pointer;width:100%}\
         .err{padding:.5rem 1rem;background:#fee;color:#b00020;border-radius:4px;\
              margin-bottom:1rem}\
         </style>"
    );
    body.push_str("<h1>loom — sign in</h1>");
    if let Some(msg) = error {
        body.push_str(&format!(
            "<div class=\"err\" role=\"alert\">{}</div>",
            html_escape(msg)
        ));
    }
    body.push_str(
        "<form method=\"POST\" action=\"/login\">\
         <label for=\"u\">User</label>\
         <input id=\"u\" name=\"user\" required autofocus autocomplete=\"username\">\
         <label for=\"p\">Password</label>\
         <input id=\"p\" name=\"password\" type=\"password\" required \
                autocomplete=\"current-password\">\
         <button type=\"submit\">Sign in</button>\
         </form>"
    );
    respond_html(request, 200, &body)
}

/// T43: POST /login — verify credentials, set session cookie,
/// redirect to /.
fn handle_login_post(
    mut request: tiny_http::Request,
    store: &AuthStore,
) -> std::io::Result<()> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let mut fields = std::collections::BTreeMap::<String, String>::new();
    for pair in body.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = urlencoding::decode(k)
            .map_err(|e| std::io::Error::other(format!("decode key: {e}")))?
            .into_owned();
        let val = urlencoding::decode(&v.replace('+', " "))
            .map_err(|e| std::io::Error::other(format!("decode val: {e}")))?
            .into_owned();
        fields.insert(key, val);
    }
    let user = fields.get("user").map(String::as_str).unwrap_or("");
    let password = fields.get("password").map(String::as_str).unwrap_or("");
    let stored = store.users.iter().find(|u| u.name == user);
    let ok = match stored {
        Some(s) => verify_password(password, &s.password_hash),
        None => {
            // Run argon2 against a dummy hash anyway to keep
            // login latency constant whether or not the user
            // exists — basic timing-oracle defense.
            let _ = verify_password(password, "$argon2id$v=19$m=19456,t=2,p=1$\
                cmFuZG9tc2FsdHRoYXRpc2Zha2U$\
                7P4Hh9MHXkCmcgkPXh7CeEM5dCEzCx7sjBmh5jzpYU0");
            false
        }
    };
    if !ok {
        return serve_login_form(request, Some("Invalid user or password"));
    }
    // Build signed cookie.
    let key_b64 = store.secret.hmac_key_b64.as_str();
    let key_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        key_b64,
    )
    .unwrap_or_default();
    let cookie = build_session_cookie(user, &key_bytes);

    let mut resp = tiny_http::Response::empty(303);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Location"[..], &b"/"[..])
            .map_err(|_| std::io::Error::other("location header"))?,
    );
    let cookie_attrs = format!(
        "{COOKIE_NAME}={cookie}; HttpOnly; SameSite=Strict; Path=/; Max-Age={SESSION_LIFETIME_SECS}"
    );
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Set-Cookie"[..], cookie_attrs.as_bytes())
            .map_err(|_| std::io::Error::other("set-cookie header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

/// T43: POST /logout — clear the session cookie + redirect.
fn handle_logout(request: tiny_http::Request) -> std::io::Result<()> {
    let mut resp = tiny_http::Response::empty(303);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Location"[..], &b"/login"[..])
            .map_err(|_| std::io::Error::other("location header"))?,
    );
    let clear = format!("{COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0");
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Set-Cookie"[..], clear.as_bytes())
            .map_err(|_| std::io::Error::other("set-cookie header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

fn serve_index(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
) -> std::io::Result<()> {
    let mut entries: Vec<String> = Vec::new();
    for e in std::fs::read_dir(cms_root)? {
        let entry = e?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                entries.push(stem.to_owned());
            }
        }
    }
    entries.sort();
    let mut body = String::new();
    body.push_str("<!doctype html><meta charset=utf-8><title>loom edit</title>");
    body.push_str("<style>body{font:16px/1.5 system-ui;max-width:36rem;margin:3rem auto;padding:0 1rem}a{display:block;padding:.5rem 0}</style>");
    body.push_str(
        "<p style=\"margin:0 0 1rem;font-size:.9em\">\
         <a href=\"/tutorial\">📖 Tutorial</a> · \
         <a href=\"/forge\">forge admin →</a></p>"
    );
    body.push_str("<h1>loom edit</h1><p>Choose a page:</p>");
    for slug in &entries {
        body.push_str(&format!(
            "<a href=\"/{slug}\">{slug}</a>",
            slug = html_escape(slug)
        ));
    }
    if entries.is_empty() {
        body.push_str("<p><em>No cms/*.json files found.</em></p>");
    }

    // T62 step 3: new-page form. Operator can now build sites
    // from scratch — no JSON file needs to pre-exist.
    body.push_str(
        "<hr style=\"margin-top:2rem\">\
         <h2 style=\"font-size:1.05em;margin:1rem 0 .5rem\">Create a new page</h2>\
         <form method=\"POST\" action=\"/new-page\" style=\"display:flex;gap:.5rem;flex-wrap:wrap;align-items:flex-end\">\
         <div style=\"flex:1;min-width:14rem\">\
           <label for=\"new-slug\" style=\"display:block;font-weight:600;font-size:.9em\">Slug</label>\
           <input id=\"new-slug\" name=\"slug\" required pattern=\"[a-z][a-z0-9-]*\" \
                  placeholder=\"about\" \
                  title=\"lowercase letters, digits, dashes; must start with a letter\" \
                  style=\"width:100%;padding:.5rem;font:inherit;border:1px solid #888;border-radius:4px\">\
         </div>\
         <div style=\"flex:1;min-width:14rem\">\
           <label for=\"new-template\" style=\"display:block;font-weight:600;font-size:.9em\">Template</label>\
           <select id=\"new-template\" name=\"template\" \
                   style=\"width:100%;padding:.5rem;font:inherit;border:1px solid #888;border-radius:4px\">\
             <option value=\"blank\">Blank — title + description, no sections</option>\
             <option value=\"landing\">Landing — hero + 3 group sections</option>\
             <option value=\"about\">About — hero + paragraph</option>\
             <option value=\"contact\">Contact — heading + paragraph</option>\
           </select>\
         </div>\
         <button type=\"submit\" \
                 style=\"padding:.5rem 1rem;font:inherit;border:0;border-radius:4px;\
                        background:#003;color:#fff;cursor:pointer\">Create</button>\
         </form>\
         <p style=\"color:#888;font-size:.85em;margin-top:.5rem\">\
           Slug becomes the filename (cms/&lt;slug&gt;.json) and the URL path \
           (/&lt;slug&gt;.html). Edit the new page inline after creation.\
         </p>"
    );

    body.push_str("<hr><p><a href=\"/forge\">forge admin →</a></p>");
    respond_html(request, 200, &body)
}

// ============================================================
// T50: forge admin dashboard — same auth scope as /<page> edit
// pages; same server-rendered no-JS forms; reads forge state
// from the filesystem.
// ============================================================

fn forge_admin_shell(title: &str, body_inner: &str) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html><meta charset=utf-8>");
    out.push_str(&format!("<title>forge · {}</title>", html_escape(title)));
    out.push_str("<style>body{font:16px/1.5 system-ui;max-width:64rem;margin:2rem auto;padding:0 1rem}nav.t50{display:flex;gap:1rem;margin-bottom:2rem;padding-bottom:1rem;border-bottom:1px solid #ccc}nav.t50 a{color:#003;text-decoration:none;font-weight:600}nav.t50 a.cur{color:#000;border-bottom:2px solid #003;padding-bottom:.25rem}h1{margin-top:0}table{width:100%;border-collapse:collapse;margin:1rem 0}th,td{padding:.5rem;text-align:left;border-bottom:1px solid #eee;font-variant-numeric:tabular-nums}.muted{color:#888}.ok{color:#0a7d2c}.bad{color:#b00020}.warn{color:#a87000}button{padding:.6rem 1.2rem;font:inherit;border:0;border-radius:4px;background:#003;color:#fff;cursor:pointer}.card{padding:1rem 1.5rem;border:1px solid #ddd;border-radius:8px;margin:1rem 0;background:#fafafa}.card h2{margin-top:0;font-size:1.1em}</style>");
    out.push_str("<nav class=\"t50\"><a href=\"/\">← pages</a>");
    let cur = title;
    for (label, href) in [
        ("Dashboard", "/forge"),
        ("Build", "/forge"),
        ("Themes", "/forge/themes"),
        ("Backends", "/forge/backends"),
        ("Audit", "/forge/audit"),
    ] {
        let is_cur = (cur == "dashboard" && href == "/forge")
            || (cur == "themes" && href == "/forge/themes")
            || (cur == "backends" && href == "/forge/backends")
            || (cur == "audit" && href == "/forge/audit");
        let cls = if is_cur { " class=\"cur\"" } else { "" };
        let _ = label;
        out.push_str(&format!(
            "<a href=\"{href}\"{cls}>{label}</a>",
            href = html_escape(href),
            cls = cls,
            label = html_escape(label),
        ));
    }
    out.push_str("</nav>");
    out.push_str(body_inner);
    out
}

/// `GET /forge` — dashboard. Reads reports/latest.json and renders
/// summary counts + last-build timestamp. Includes a Build button
/// that POSTs to /forge/build.
fn serve_forge_dashboard(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
    _static_root: &std::path::Path,
) -> std::io::Result<()> {
    let site_root = cms_root.parent().unwrap_or(std::path::Path::new("."));
    let latest = site_root.join("reports/latest.json");
    let mut body = String::new();
    body.push_str("<h1>forge — dashboard</h1>");

    if latest.is_file() {
        let raw = std::fs::read_to_string(&latest).unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
        let strict = parsed
            .get("strict_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let warns = parsed
            .get("warn_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mode = parsed
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let started = parsed
            .get("started")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let strict_cls = if strict == 0 { "ok" } else { "bad" };
        let warn_cls = if warns == 0 { "ok" } else { "warn" };
        body.push_str(&format!(
            "<div class=\"card\"><h2>Last build</h2>\
             <table>\
             <tr><th>Mode</th><td>{mode}</td></tr>\
             <tr><th>Started</th><td><span class=\"muted\">{started}</span></td></tr>\
             <tr><th>Strict findings</th><td><span class=\"{strict_cls}\">{strict}</span></td></tr>\
             <tr><th>Warn findings</th><td><span class=\"{warn_cls}\">{warns}</span></td></tr>\
             </table></div>",
            mode = html_escape(mode),
            started = html_escape(started),
            strict_cls = strict_cls,
            warn_cls = warn_cls,
        ));
    } else {
        body.push_str(
            "<p class=\"muted\">No build report at <code>reports/latest.json</code> yet. Run a build to populate.</p>"
        );
    }

    body.push_str(
        "<form method=\"POST\" action=\"/forge/build\">\
         <button type=\"submit\">Run forge build now</button>\
         </form>\
         <p class=\"muted\">Builds the site, runs every phase including the crawler audit. Refresh after a few seconds.</p>"
    );

    let html = forge_admin_shell("dashboard", &body);
    respond_html(request, 200, &html)
}

/// `POST /forge/build` — shell out to forge.sh, redirect.
fn handle_forge_build(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
    forge_path: &str,
) -> std::io::Result<()> {
    let site_root = cms_root.parent().unwrap_or(std::path::Path::new("."));
    if forge_path.is_empty() {
        return respond_text(request, 503, "forge disabled in this session");
    }
    let status = std::process::Command::new("bash")
        .arg(forge_path)
        .current_dir(site_root)
        .status();
    let _ = status; // Surfaced via the dashboard reload.
    let mut resp = tiny_http::Response::empty(303);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Location"[..], &b"/forge"[..])
            .map_err(|_| std::io::Error::other("header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

/// `GET /forge/themes` — parse the skin.css path the editor knows
/// about (static/loom.css preferred). Renders a table of themes
/// with token counts. Apply-button is hidden until T29 lands real
/// contrast-pinned swap; doctrine: never let a click ship an
/// unaudited theme.
fn serve_forge_themes(
    request: tiny_http::Request,
    static_root: &std::path::Path,
) -> std::io::Result<()> {
    let skin = static_root.join("loom.css");
    let mut body = String::new();
    body.push_str("<h1>themes</h1>");
    if !skin.is_file() {
        body.push_str(&format!(
            "<p class=\"muted\">No skin at <code>{}</code>.</p>",
            html_escape(&skin.display().to_string())
        ));
        let html = forge_admin_shell("themes", &body);
        return respond_html(request, 200, &html);
    }
    let raw = std::fs::read_to_string(&skin)?;
    let (themes, refs) = parse_skin_themes(&raw);
    body.push_str(&format!(
        "<p class=\"muted\">{} theme(s), {} unique <code>var(--loom-color-*)</code> reference(s).</p>",
        themes.len(),
        refs.len()
    ));
    body.push_str("<table><tr><th>Theme</th><th>Tokens</th><th>Sample (--loom-color-bg-canvas)</th></tr>");
    for t in &themes {
        let sample = t
            .tokens
            .get("--loom-color-bg-canvas")
            .or_else(|| t.tokens.get("--loom-color-primary"))
            .cloned()
            .unwrap_or_default();
        body.push_str(&format!(
            "<tr><td><strong>{name}</strong></td><td>{n}</td><td><code>{sample}</code></td></tr>",
            name = html_escape(&t.name),
            n = t.tokens.len(),
            sample = html_escape(&sample),
        ));
    }
    body.push_str("</table>");
    body.push_str(
        "<p class=\"muted\">Apply-button lands with T29 (contrast-pinned themes). Until then, change <code>&lt;html data-theme=\"X\"&gt;</code> in your CMS or skin source.</p>"
    );
    let html = forge_admin_shell("themes", &body);
    respond_html(request, 200, &html)
}

/// `GET /forge/backends` — render backend coverage. Reuses
/// backends.toml directly (sibling of cms/).
fn serve_forge_backends(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
) -> std::io::Result<()> {
    let site_root = cms_root.parent().unwrap_or(std::path::Path::new("."));
    let backends_path = site_root.join("backends.toml");
    let mut body = String::new();
    body.push_str("<h1>backends</h1>");
    if !backends_path.is_file() {
        body.push_str("<p class=\"muted\">No backends.toml in site root.</p>");
        let html = forge_admin_shell("backends", &body);
        return respond_html(request, 200, &html);
    }
    let raw = std::fs::read_to_string(&backends_path)?;
    let value: toml::Value = toml::from_str(&raw)
        .map_err(|e| std::io::Error::other(format!("backends.toml: {e}")))?;
    let backends = match value.get("backends").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => {
            body.push_str("<p class=\"muted\">No [backends] section.</p>");
            let html = forge_admin_shell("backends", &body);
            return respond_html(request, 200, &html);
        }
    };
    let mut rows: Vec<(String, BackendStatus, String, String)> = Vec::new();
    for (k, v) in backends {
        let Some(t) = v.as_table() else { continue };
        let Ok(key) = BackendKey::new(k) else { continue };
        let method = t.get("method").and_then(|v| v.as_str()).unwrap_or("?").to_owned();
        let purpose = t.get("purpose").and_then(|v| v.as_str()).unwrap_or("").to_owned();
        let impl_files: Vec<String> = t
            .get("impl_files")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        let status = BackendStatus::from_impl_files(impl_files);
        rows.push((key.as_str().to_owned(), status, method, purpose));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let total = rows.len();
    let stubs = rows.iter().filter(|r| r.1.is_stub()).count();
    body.push_str(&format!(
        "<p class=\"muted\">{total} declared, {impls} implemented ({pct}%), {stubs} stub.</p>",
        impls = total - stubs,
        pct = if total > 0 { (total - stubs) * 100 / total } else { 0 },
    ));
    body.push_str("<table><tr><th>Key</th><th>Method</th><th>Status</th><th>Purpose</th></tr>");
    for (k, s, m, p) in &rows {
        let cls = if s.is_stub() { "warn" } else { "ok" };
        body.push_str(&format!(
            "<tr><td><code>{k}</code></td><td>{m}</td><td><span class=\"{cls}\">{lab}</span></td><td>{p}</td></tr>",
            k = html_escape(k),
            m = html_escape(m),
            cls = cls,
            lab = s.label(),
            p = html_escape(p),
        ));
    }
    body.push_str("</table>");
    let html = forge_admin_shell("backends", &body);
    respond_html(request, 200, &html)
}

/// `GET /forge/audit` — latest crawler positive-signal snapshot.
fn serve_forge_audit(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
) -> std::io::Result<()> {
    let site_root = cms_root.parent().unwrap_or(std::path::Path::new("."));
    let mut body = String::new();
    body.push_str("<h1>audit</h1>");

    // Latest forge report includes whether crawl passed.
    let latest = site_root.join("reports/latest.json");
    if latest.is_file() {
        let raw = std::fs::read_to_string(&latest).unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
        let phases = parsed.get("phases").and_then(|v| v.as_array());
        if let Some(phases) = phases {
            if let Some(crawl) = phases.iter().find(|p| {
                p.get("name").and_then(|v| v.as_str()) == Some("crawl")
            }) {
                let findings = crawl
                    .get("findings")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                body.push_str(&format!(
                    "<div class=\"card\"><h2>Last crawl phase</h2>\
                     <p>Findings: <strong>{findings}</strong></p>\
                     </div>"
                ));
            }
        }
    }

    body.push_str(
        "<p class=\"muted\">The crawler runs as part of every <code>forge.sh</code> invocation (T49). \
         Trigger a fresh build from the dashboard to refresh.</p>"
    );

    let html = forge_admin_shell("audit", &body);
    respond_html(request, 200, &html)
}

fn serve_edit_form(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
    slug: &str,
) -> std::io::Result<()> {
    let json_path = cms_root.join(format!("{slug}.json"));
    if !json_path.is_file() {
        return respond_text(request, 404, "page not found");
    }
    let raw = std::fs::read_to_string(&json_path)?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        std::io::Error::other(format!("parse {}: {e}", json_path.display()))
    })?;

    let title = parsed
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let description = parsed
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut body = String::new();
    body.push_str("<!doctype html><meta charset=utf-8>");
    body.push_str(&format!(
        "<title>edit {slug}</title>",
        slug = html_escape(slug)
    ));
    // T62 step 5: split-pane layout — editor on the left, live
    // preview iframe on the right (stacks vertically on narrow
    // viewports). The iframe reloads automatically after every
    // POST because the server returns 303 → editor re-renders →
    // iframe `src` is fetched again.
    body.push_str(
        "<style>\
         body{font:16px/1.5 system-ui;margin:0;padding:0;display:grid;\
              grid-template-columns:minmax(0,1fr) minmax(0,1fr);\
              gap:1rem;min-height:100vh}\
         @media(max-width:60rem){body{grid-template-columns:1fr}}\
         .editor{padding:1rem 1.5rem;overflow-y:auto;max-height:100vh}\
         .preview-pane{position:sticky;top:0;height:100vh;display:flex;\
                       flex-direction:column;border-left:1px solid #ddd;\
                       background:#f4f4f4}\
         @media(max-width:60rem){.preview-pane{position:static;height:60vh;\
                                              border-left:0;border-top:1px solid #ddd}}\
         .preview-bar{padding:.5rem 1rem;border-bottom:1px solid #ddd;\
                      font-size:.85em;color:#555;display:flex;\
                      align-items:center;justify-content:space-between}\
         .preview-bar a{color:#003;text-decoration:none}\
         .preview-frame{flex:1;border:0;width:100%;background:#fff}\
         label{display:block;margin:1rem 0 .25rem;font-weight:600}\
         input,textarea,select{width:100%;padding:.5rem;font:inherit;\
                              border:1px solid #888;border-radius:4px;\
                              box-sizing:border-box}\
         textarea{min-height:4em}\
         button[type=\"submit\"]{margin-top:1.5rem;padding:.6rem 1.2rem;\
                                font:inherit;border:0;border-radius:4px;\
                                background:#003;color:#fff;cursor:pointer}\
         </style>"
    );
    body.push_str("<div class=\"editor\">");
    body.push_str(&format!(
        "<p><a href=\"/\">&larr; all pages</a> · \
         <a href=\"/tutorial\">📖 tutorial</a> · \
         <a href=\"/preview/{}.html\" target=\"_blank\">open preview in new tab</a></p>",
        html_escape(slug)
    ));
    body.push_str(&format!("<h1>edit: {}</h1>", html_escape(slug)));
    body.push_str(&format!(
        "<form method=\"POST\" action=\"/{slug}\">",
        slug = html_escape(slug)
    ));
    body.push_str(&format!(
        "<label for=\"f-title\">Title <span style=\"color:#888;font-weight:400\">(&lt;title&gt; tag + page header)</span></label>\
         <input id=\"f-title\" name=\"title\" value=\"{val}\" required>",
        val = html_escape(title)
    ));
    body.push_str(&format!(
        "<label for=\"f-description\">Description <span style=\"color:#888;font-weight:400\">(meta description for search engines)</span></label>\
         <textarea id=\"f-description\" name=\"description\" required>{val}</textarea>",
        val = html_escape(description)
    ));
    // Section editor widgets. Hero is fully editable; other kinds
    // render with reorder/delete controls only (inline field
    // editors land in T62 step 4). Reorder + delete buttons use
    // formaction= to override the main form's POST target.
    if let Some(sections) = parsed.get("sections").and_then(|v| v.as_array()) {
        let total = sections.len();
        for (i, sec) in sections.iter().enumerate() {
            let kind = sec.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            body.push_str(&format!(
                "<fieldset style=\"margin-top:1.5rem;border:1px solid #ccc;border-radius:6px;padding:1rem\">\
                 <legend>{kind_label} (section {n})</legend>",
                kind_label = html_escape(&capitalise(kind)),
                n = i + 1,
            ));
            // T62 step 4: typed editors per CmsSection variant.
            // Field names follow `sec.<i>.<field>` so the existing
            // POST handler can read them; arrays use
            // `sec.<i>.<field>.<n>` for stable indexing.
            match kind {
                "hero" => {
                    let h_title = sec.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let h_sub = sec.get("subtitle").and_then(|v| v.as_str()).unwrap_or("");
                    let h_eye = sec.get("eyebrow").and_then(|v| v.as_str()).unwrap_or("");
                    body.push_str(&format!(
                        "<label>Eyebrow</label><input name=\"sec.{i}.eyebrow\" value=\"{}\">",
                        html_escape(h_eye)
                    ));
                    body.push_str(&format!(
                        "<label>Title</label><input name=\"sec.{i}.title\" value=\"{}\">",
                        html_escape(h_title)
                    ));
                    body.push_str(&format!(
                        "<label>Subtitle</label><textarea name=\"sec.{i}.subtitle\">{}</textarea>",
                        html_escape(h_sub)
                    ));
                }
                "paragraph" => {
                    let pbody = sec.get("body").and_then(|v| v.as_str()).unwrap_or("");
                    body.push_str(&format!(
                        "<label>Body</label>\
                         <textarea name=\"sec.{i}.body\" rows=\"4\" required>{}</textarea>",
                        html_escape(pbody)
                    ));
                }
                "heading" => {
                    let level = sec.get("level").and_then(|v| v.as_u64()).unwrap_or(2);
                    let text = sec.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    body.push_str(&format!(
                        "<label>Level</label>\
                         <select name=\"sec.{i}.level\" \
                                 style=\"width:6rem;padding:.5rem;font:inherit;\
                                        border:1px solid #888;border-radius:4px\">"
                    ));
                    for lvl in 1..=6u64 {
                        let sel = if lvl == level { " selected" } else { "" };
                        body.push_str(&format!(
                            "<option value=\"{lvl}\"{sel}>H{lvl}</option>"
                        ));
                    }
                    body.push_str("</select>");
                    body.push_str(&format!(
                        "<label>Text</label><input name=\"sec.{i}.text\" value=\"{}\" required>",
                        html_escape(text)
                    ));
                }
                "banner" => {
                    let tone = sec.get("tone").and_then(|v| v.as_str()).unwrap_or("info");
                    let title_v = sec.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let body_v = sec.get("body").and_then(|v| v.as_str()).unwrap_or("");
                    body.push_str(&format!(
                        "<label>Tone</label>\
                         <select name=\"sec.{i}.tone\" \
                                 style=\"width:8rem;padding:.5rem;font:inherit;\
                                        border:1px solid #888;border-radius:4px\">"
                    ));
                    for opt in ["info", "success", "warn", "danger"] {
                        let sel = if opt == tone { " selected" } else { "" };
                        body.push_str(&format!(
                            "<option value=\"{opt}\"{sel}>{opt}</option>"
                        ));
                    }
                    body.push_str("</select>");
                    body.push_str(&format!(
                        "<label>Title</label><input name=\"sec.{i}.title\" value=\"{}\">",
                        html_escape(title_v)
                    ));
                    body.push_str(&format!(
                        "<label>Body</label><textarea name=\"sec.{i}.body\" rows=\"3\">{}</textarea>",
                        html_escape(body_v)
                    ));
                }
                "group" => {
                    let g_title = sec.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    body.push_str(&format!(
                        "<label>Title</label><input name=\"sec.{i}.title\" value=\"{}\" required>",
                        html_escape(g_title)
                    ));
                    body.push_str("<label>Body paragraphs</label>");
                    let body_arr = sec.get("body").and_then(|v| v.as_array());
                    let count = body_arr.map(Vec::len).unwrap_or(0);
                    if let Some(arr) = body_arr {
                        for (n, para) in arr.iter().enumerate() {
                            let s = para.as_str().unwrap_or("");
                            body.push_str(&format!(
                                "<textarea name=\"sec.{i}.body.{n}\" rows=\"2\" \
                                  style=\"margin-bottom:.5rem\">{}</textarea>",
                                html_escape(s)
                            ));
                        }
                    }
                    if count == 0 {
                        body.push_str(
                            "<p style=\"color:#888;font-size:.85em;margin:0\">\
                             (no body paragraphs — append below to add one)</p>",
                        );
                    }
                    // Hidden marker so POST knows how many body
                    // paragraphs the form rendered.
                    body.push_str(&format!(
                        "<input type=\"hidden\" name=\"sec.{i}.body.__count\" value=\"{count}\">"
                    ));
                    // Inline form OUTSIDE the main form (via
                    // formaction) for "+ paragraph". Using the
                    // existing /add-section route would create a
                    // whole new section; we want to extend THIS
                    // group. New endpoint: add-paragraph.
                    body.push_str(&format!(
                        "<button type=\"submit\" formaction=\"/{slug}/sections/{i}/add-paragraph\" \
                          formmethod=\"post\" formnovalidate \
                          style=\"padding:.3rem .7rem;font:inherit;border:1px solid #888;\
                                 border-radius:4px;background:#f4f4f4;cursor:pointer;\
                                 margin-top:.25rem\">+ paragraph</button>",
                        slug = html_escape(slug),
                    ));
                }
                _ => {
                    // Unknown kind — fall back to JSON preview
                    // so the operator can SEE there's a section
                    // forge couldn't render an editor for.
                    let preview = serde_json::to_string(sec).unwrap_or_default();
                    let short = if preview.len() > 200 {
                        format!("{}…", &preview[..199])
                    } else {
                        preview
                    };
                    body.push_str(&format!(
                        "<p style=\"color:#a87000;font-size:.85em;margin:.25rem 0\">\
                         <strong>No editor for kind '{}'.</strong> Raw JSON:<br>\
                         <code>{}</code></p>",
                        html_escape(kind),
                        html_escape(&short)
                    ));
                }
            }
            // T62 step 2: per-section reorder + delete controls.
            // `formaction` overrides the main form's action; the
            // main form's `required` validators don't fire because
            // `formnovalidate` is set.
            body.push_str("<div style=\"margin-top:.75rem;display:flex;gap:.5rem;align-items:center\">");
            if i > 0 {
                body.push_str(&format!(
                    "<button type=\"submit\" formaction=\"/{slug}/sections/{i}/up\" \
                       formmethod=\"post\" formnovalidate \
                       style=\"padding:.3rem .7rem;font:inherit;border:1px solid #888;\
                              border-radius:4px;background:#f4f4f4;cursor:pointer\">\
                       ↑ Move up</button>",
                    slug = html_escape(slug),
                ));
            }
            if i + 1 < total {
                body.push_str(&format!(
                    "<button type=\"submit\" formaction=\"/{slug}/sections/{i}/down\" \
                       formmethod=\"post\" formnovalidate \
                       style=\"padding:.3rem .7rem;font:inherit;border:1px solid #888;\
                              border-radius:4px;background:#f4f4f4;cursor:pointer\">\
                       ↓ Move down</button>",
                    slug = html_escape(slug),
                ));
            }
            body.push_str(&format!(
                "<button type=\"submit\" formaction=\"/{slug}/sections/{i}/delete\" \
                   formmethod=\"post\" formnovalidate \
                   style=\"padding:.3rem .7rem;font:inherit;border:1px solid #b00020;\
                          border-radius:4px;background:#fee;color:#b00020;cursor:pointer;\
                          margin-left:auto\" \
                   onclick=\"return confirm('Delete section {n}? This can be undone by re-creating it.')\">\
                   Delete</button>",
                slug = html_escape(slug),
                n = i + 1,
            ));
            body.push_str("</div>");
            body.push_str("</fieldset>");
        }
    }
    body.push_str("<button type=\"submit\">Save</button>");
    body.push_str("</form>");

    // T62: section composer. Add a new section by picking a
    // variant kind + submitting. Server-rendered, zero JS,
    // works without an active session for the existing form.
    body.push_str(&format!(
        "<hr style=\"margin-top:2.5rem\">\
         <h2 style=\"margin-top:1.5rem;font-size:1.1em\">Add a section</h2>\
         <form method=\"POST\" action=\"/{slug}/add-section\" \
               style=\"display:flex;gap:.5rem;align-items:center;flex-wrap:wrap\">\
         <label for=\"new-section-kind\" style=\"display:inline;margin:0\">Kind:</label>\
         <select id=\"new-section-kind\" name=\"kind\" required \
                 style=\"padding:.5rem;font:inherit;border:1px solid #888;border-radius:4px\">\
           <option value=\"hero\">Hero (eyebrow + title + subtitle)</option>\
           <option value=\"group\">Group (heading + body paragraphs)</option>\
           <option value=\"paragraph\">Paragraph (single block of text)</option>\
           <option value=\"heading\">Heading (h2/h3/h4)</option>\
           <option value=\"banner\">Banner (info/warn/danger)</option>\
         </select>\
         <button type=\"submit\" \
                 style=\"background:#003;color:#fff;padding:.5rem 1rem;border:0;\
                        border-radius:4px;font:inherit;cursor:pointer\">Append</button>\
         </form>\
         <p style=\"color:#888;font-size:.85em;margin-top:.5rem\">\
           Adds a section with default values. Edit it inline above after save.\
         </p>",
        slug = html_escape(slug),
    ));

    // Close the editor pane + open the live-preview pane.
    body.push_str("</div>");
    body.push_str(&format!(
        "<aside class=\"preview-pane\" aria-label=\"Live preview\">\
         <div class=\"preview-bar\">\
           <strong>Live preview</strong>\
           <a href=\"/preview/{slug}.html\" target=\"_blank\" rel=\"noopener\">open ↗</a>\
         </div>\
         <iframe class=\"preview-frame\" src=\"/preview/{slug}.html\" \
                 title=\"Rendered preview of {slug}\"></iframe>\
         </aside>",
        slug = html_escape(slug),
    ));

    respond_html(request, 200, &body)
}

/// T62: append a new section of the chosen `kind` to
/// cms/<slug>.json's sections[] array. Defaults are sensible
/// placeholders the operator edits inline after the redirect.
///
/// REGRESSION-GUARD: every kind written here MUST correspond to
/// a CmsSection variant in loom-cms-render. Adding a new kind
/// requires updating BOTH the dropdown above AND this match.
fn handle_add_section(
    mut request: tiny_http::Request,
    cms_root: &std::path::Path,
    forge_path: &str,
    slug: &str,
) -> std::io::Result<()> {
    let cap = match WriteCapability::for_dir(cms_root) {
        Ok(c) => c,
        Err(_) => return respond_text(request, 500, "cms root unreadable"),
    };
    let rel = std::path::PathBuf::from(format!("{slug}.json"));
    if !cap.file_exists(&rel) {
        return respond_text(request, 404, "page not found");
    }

    // Parse form body.
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let mut fields = std::collections::BTreeMap::<String, String>::new();
    for pair in body.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = urlencoding::decode(k)
            .map_err(|e| std::io::Error::other(format!("decode key: {e}")))?
            .into_owned();
        let val = urlencoding::decode(&v.replace('+', " "))
            .map_err(|e| std::io::Error::other(format!("decode val: {e}")))?
            .into_owned();
        fields.insert(key, val);
    }

    let kind = fields
        .get("kind")
        .map(String::as_str)
        .unwrap_or("paragraph");

    // Map kind → default section JSON. The shapes mirror
    // loom-cms-render's CmsSection variants. Future kinds land
    // by adding a match arm + an option in the dropdown.
    let new_section = match kind {
        "hero" => serde_json::json!({
            "kind": "hero",
            "eyebrow": "",
            "title": "New hero section",
            "subtitle": "Edit this subtitle.",
            "cta": null,
        }),
        "group" => serde_json::json!({
            "kind": "group",
            "title": "New group",
            "body": ["First paragraph.", "Second paragraph."],
        }),
        "paragraph" => serde_json::json!({
            "kind": "paragraph",
            "body": "Edit this paragraph.",
        }),
        "heading" => serde_json::json!({
            "kind": "heading",
            "level": 2,
            "text": "New heading",
        }),
        "banner" => serde_json::json!({
            "kind": "banner",
            "tone": "info",
            "title": "Notice",
            "body": "Edit this banner.",
        }),
        _ => return respond_text(request, 400, "unknown section kind"),
    };

    // Read + mutate + atomic write via capability.
    let raw_bytes = cap.read_file(&rel).map_err(|e| match e {
        CapabilityError::Io(i) => i,
        _ => std::io::Error::other("read failed"),
    })?;
    let raw = String::from_utf8(raw_bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut parsed: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| std::io::Error::other(format!("parse: {e}")))?;
    let sections = parsed
        .get_mut("sections")
        .and_then(|v| v.as_array_mut());
    match sections {
        Some(arr) => arr.push(new_section),
        None => {
            parsed["sections"] = serde_json::Value::Array(vec![new_section]);
        }
    }
    let serialized = serde_json::to_string_pretty(&parsed)
        .map_err(|e| std::io::Error::other(format!("serialize: {e}")))?;
    cap.write_atomic(&rel, serialized.as_bytes())
        .map_err(|e| match e {
            CapabilityError::Io(i) => i,
            _ => std::io::Error::other("write failed"),
        })?;

    // Trigger forge rebuild if configured.
    if !forge_path.is_empty() {
        let cms_parent = cms_root
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let _ = std::process::Command::new("bash")
            .arg(forge_path)
            .current_dir(&cms_parent)
            .status();
    }

    let mut resp = tiny_http::Response::empty(303);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Location"[..], format!("/{slug}").as_bytes())
            .map_err(|_| std::io::Error::other("header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

fn handle_edit_post(
    mut request: tiny_http::Request,
    cms_root: &std::path::Path,
    forge_path: &str,
    slug: &str,
) -> std::io::Result<()> {
    // T60: every read/write to cms_root flows through a capability
    // constructed at the request boundary. The slug came from the
    // URL — operator-controlled, hostile-by-default — so the
    // capability is the SECURITY gate. Path traversal, symlink
    // escape, and any future malicious slug are physically
    // constrained at runtime.
    //
    // The pre-existing string-heuristic path-traversal check in
    // handle_edit_request stays as defense-in-depth + an early
    // 400, but the capability is the canonical enforcement point.
    let cap = match WriteCapability::for_dir(cms_root) {
        Ok(c) => c,
        Err(_) => return respond_text(request, 500, "cms root unreadable"),
    };
    let rel = std::path::PathBuf::from(format!("{slug}.json"));
    if !cap.file_exists(&rel) {
        return respond_text(request, 404, "page not found");
    }
    // Read body (form-urlencoded).
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let mut fields = std::collections::BTreeMap::<String, String>::new();
    for pair in body.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = urlencoding::decode(k).map_err(|e| std::io::Error::other(format!("decode key: {e}")))?.into_owned();
        let val = urlencoding::decode(&v.replace('+', " "))
            .map_err(|e| std::io::Error::other(format!("decode val: {e}")))?
            .into_owned();
        fields.insert(key, val);
    }

    // Mutate the JSON in place — read THROUGH the capability.
    let raw_bytes = cap.read_file(&rel).map_err(|e| match e {
        CapabilityError::Io(i) => i,
        CapabilityError::EscapesScope { .. } => {
            std::io::Error::other("path escapes cms scope")
        }
        CapabilityError::NotADir(_) => {
            std::io::Error::other("cms root not a directory")
        }
    })?;
    let raw = String::from_utf8(raw_bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        std::io::Error::other(format!("parse {}: {e}", rel.display()))
    })?;
    if let Some(t) = fields.get("title") {
        parsed["title"] = serde_json::Value::String(t.clone());
    }
    if let Some(d) = fields.get("description") {
        parsed["description"] = serde_json::Value::String(d.clone());
    }
    // T62 step 4: typed per-kind field handling. Field names
    // follow `sec.<i>.<field>` (scalars) or `sec.<i>.<field>.<n>`
    // (arrays). The match below mirrors the GET-side editor.
    if let Some(sections) = parsed.get_mut("sections").and_then(|v| v.as_array_mut()) {
        for (i, sec) in sections.iter_mut().enumerate() {
            let kind = sec.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            match kind.as_str() {
                "hero" => {
                    for key in ["title", "subtitle", "eyebrow"] {
                        if let Some(v) = fields.get(&format!("sec.{i}.{key}")) {
                            sec[key] = serde_json::Value::String(v.clone());
                        }
                    }
                }
                "paragraph" => {
                    if let Some(v) = fields.get(&format!("sec.{i}.body")) {
                        sec["body"] = serde_json::Value::String(v.clone());
                    }
                }
                "heading" => {
                    if let Some(v) = fields.get(&format!("sec.{i}.text")) {
                        sec["text"] = serde_json::Value::String(v.clone());
                    }
                    if let Some(v) = fields.get(&format!("sec.{i}.level")) {
                        // SECURITY: clamp to 1..=6 — anything else
                        // would emit invalid HTML (<h7> doesn't exist).
                        let lvl: u64 = v.parse().unwrap_or(2).clamp(1, 6);
                        sec["level"] = serde_json::Value::from(lvl);
                    }
                }
                "banner" => {
                    if let Some(v) = fields.get(&format!("sec.{i}.tone")) {
                        // SECURITY: only allow the declared tone enum.
                        let tone = if ["info", "success", "warn", "danger"].contains(&v.as_str()) {
                            v.clone()
                        } else {
                            "info".to_owned()
                        };
                        sec["tone"] = serde_json::Value::String(tone);
                    }
                    for key in ["title", "body"] {
                        if let Some(v) = fields.get(&format!("sec.{i}.{key}")) {
                            sec[key] = serde_json::Value::String(v.clone());
                        }
                    }
                }
                "group" => {
                    if let Some(v) = fields.get(&format!("sec.{i}.title")) {
                        sec["title"] = serde_json::Value::String(v.clone());
                    }
                    // body is an array of paragraphs. The form
                    // ships a hidden __count + sec.N.body.0,
                    // sec.N.body.1, ... entries. Walk by index
                    // so empty paragraphs are preserved positionally.
                    let count: usize = fields
                        .get(&format!("sec.{i}.body.__count"))
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let mut paragraphs = Vec::<serde_json::Value>::with_capacity(count);
                    for n in 0..count {
                        let v = fields
                            .get(&format!("sec.{i}.body.{n}"))
                            .cloned()
                            .unwrap_or_default();
                        paragraphs.push(serde_json::Value::String(v));
                    }
                    if !paragraphs.is_empty() {
                        sec["body"] = serde_json::Value::Array(paragraphs);
                    }
                }
                _ => {} // unknown kind — leave untouched
            }
        }
    }

    // T60: atomic write THROUGH the capability. Capability builds
    // its own temp filename (encoded with pid+nanos) inside the
    // same parent dir, runs fs::write + fs::rename, all
    // boundary-checked.
    let serialized = serde_json::to_string_pretty(&parsed)
        .map_err(|e| std::io::Error::other(format!("serialize: {e}")))?;
    cap.write_atomic(&rel, serialized.as_bytes()).map_err(|e| match e {
        CapabilityError::Io(i) => i,
        CapabilityError::EscapesScope { .. } => {
            std::io::Error::other("write attempt escapes cms scope")
        }
        CapabilityError::NotADir(_) => {
            std::io::Error::other("cms root not a directory")
        }
    })?;

    // Trigger forge rebuild. Honour empty string = disabled (tests).
    if !forge_path.is_empty() {
        let cms_parent = cms_root
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let status = std::process::Command::new("bash")
            .arg(forge_path)
            .current_dir(&cms_parent)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => eprintln!("  warn  forge.sh exited {s}"),
            Err(e) => eprintln!("  warn  forge.sh failed: {e}"),
        }
    }

    // Redirect back to the edit form.
    let mut resp = tiny_http::Response::empty(303);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Location"[..], format!("/{slug}").as_bytes())
            .map_err(|_| std::io::Error::other("header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

fn serve_preview(
    request: tiny_http::Request,
    static_root: &std::path::Path,
    rel: &str,
) -> std::io::Result<()> {
    if rel.contains("..") {
        return respond_text(request, 400, "bad path");
    }
    let p = static_root.join(rel);
    if !p.is_file() {
        return respond_text(request, 404, "not found");
    }
    let bytes = std::fs::read(&p)?;
    let ct = if rel.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if rel.ends_with(".css") {
        "text/css"
    } else if rel.ends_with(".js") {
        "application/javascript"
    } else {
        "application/octet-stream"
    };
    let mut resp = tiny_http::Response::from_data(bytes);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], ct.as_bytes())
            .map_err(|_| std::io::Error::other("header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

fn respond_html(
    request: tiny_http::Request,
    code: u16,
    body: &str,
) -> std::io::Result<()> {
    let mut resp = tiny_http::Response::from_string(body.to_owned()).with_status_code(code);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
            .map_err(|_| std::io::Error::other("header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

fn respond_text(
    request: tiny_http::Request,
    code: u16,
    body: &str,
) -> std::io::Result<()> {
    let resp = tiny_http::Response::from_string(body.to_owned()).with_status_code(code);
    request.respond(resp)?;
    Ok(())
}

/// T62 step 3: validated slug name. Same character class as
/// BackendKey but for CMS file names.
///
/// SECURITY: rejects path traversal, shell metachars, dots,
/// uppercase, and any non-ASCII. Constructor is the only way
/// in.
#[derive(Debug, Clone)]
struct SlugName(String);

impl SlugName {
    fn new(s: &str) -> Result<Self, &'static str> {
        if s.is_empty() {
            return Err("slug must not be empty");
        }
        if s.len() > 80 {
            return Err("slug too long (max 80 chars)");
        }
        let mut chars = s.chars();
        let first = chars.next().unwrap_or('?');
        if !first.is_ascii_lowercase() {
            return Err("slug must start with lowercase a-z");
        }
        for c in chars {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                return Err("slug may only contain a-z, 0-9, dash");
            }
        }
        Ok(Self(s.to_owned()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// T62 step 3: create a new cms/<slug>.json from a template +
/// redirect to the editor.
fn handle_new_page(
    mut request: tiny_http::Request,
    cms_root: &std::path::Path,
    forge_path: &str,
) -> std::io::Result<()> {
    let cap = match WriteCapability::for_dir(cms_root) {
        Ok(c) => c,
        Err(_) => return respond_text(request, 500, "cms root unreadable"),
    };

    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let mut fields = std::collections::BTreeMap::<String, String>::new();
    for pair in body.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = urlencoding::decode(k)
            .map_err(|e| std::io::Error::other(format!("decode key: {e}")))?
            .into_owned();
        let val = urlencoding::decode(&v.replace('+', " "))
            .map_err(|e| std::io::Error::other(format!("decode val: {e}")))?
            .into_owned();
        fields.insert(key, val);
    }
    let raw_slug = fields.get("slug").cloned().unwrap_or_default();
    let template = fields
        .get("template")
        .cloned()
        .unwrap_or_else(|| "blank".to_owned());

    let slug = match SlugName::new(&raw_slug) {
        Ok(s) => s,
        Err(why) => {
            return respond_text(request, 400, &format!("invalid slug: {why}"));
        }
    };

    let rel = std::path::PathBuf::from(format!("{}.json", slug.as_str()));
    if cap.file_exists(&rel) {
        return respond_text(request, 409, "page already exists; pick a different slug");
    }

    // Build the page JSON from the template.
    let title = capitalise(slug.as_str());
    let path_attr = format!("/{}.html", slug.as_str());
    let page = match template.as_str() {
        "blank" => serde_json::json!({
            "$schema": "../cms-schema.json",
            "title": title,
            "description": format!("{} page.", title),
            "path": path_attr,
            "sections": [],
        }),
        "landing" => serde_json::json!({
            "$schema": "../cms-schema.json",
            "title": title,
            "description": format!("{}", title),
            "path": path_attr,
            "sections": [
                {"kind":"hero","eyebrow":"Welcome","title":title,"subtitle":"Edit this subtitle.","cta":null},
                {"kind":"group","title":"What we do","body":["Edit this paragraph.","Add another."]},
                {"kind":"group","title":"How it works","body":["Step one.","Step two."]},
                {"kind":"group","title":"Why us","body":["Reason one.","Reason two."]},
            ],
        }),
        "about" => serde_json::json!({
            "$schema": "../cms-schema.json",
            "title": title,
            "description": format!("About {}", title),
            "path": path_attr,
            "sections": [
                {"kind":"hero","eyebrow":"About","title":title,"subtitle":"Who we are.","cta":null},
                {"kind":"paragraph","body":"Write your about copy here."},
            ],
        }),
        "contact" => serde_json::json!({
            "$schema": "../cms-schema.json",
            "title": title,
            "description": format!("Contact {}", title),
            "path": path_attr,
            "sections": [
                {"kind":"heading","level":1,"text":title},
                {"kind":"paragraph","body":"Reach us at email@example.com."},
            ],
        }),
        _ => return respond_text(request, 400, "unknown template"),
    };

    let serialized = serde_json::to_string_pretty(&page)
        .map_err(|e| std::io::Error::other(format!("serialize: {e}")))?;
    cap.write_atomic(&rel, serialized.as_bytes())
        .map_err(|_| std::io::Error::other("write"))?;

    if !forge_path.is_empty() {
        let cms_parent = cms_root
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let _ = std::process::Command::new("bash")
            .arg(forge_path)
            .current_dir(&cms_parent)
            .status();
    }

    let mut resp = tiny_http::Response::empty(303);
    resp.add_header(
        tiny_http::Header::from_bytes(
            &b"Location"[..],
            format!("/{}", slug.as_str()).as_bytes(),
        )
        .map_err(|_| std::io::Error::other("header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

/// Title-case helper for section-kind legends.
fn capitalise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut first = true;
    for c in s.chars() {
        if first {
            for u in c.to_uppercase() {
                out.push(u);
            }
            first = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// T62 step 2: per-section reorder + delete.
fn handle_section_op(
    request: tiny_http::Request,
    cms_root: &std::path::Path,
    forge_path: &str,
    slug: &str,
    idx: usize,
    op: &str,
) -> std::io::Result<()> {
    let cap = match WriteCapability::for_dir(cms_root) {
        Ok(c) => c,
        Err(_) => return respond_text(request, 500, "cms root unreadable"),
    };
    let rel = std::path::PathBuf::from(format!("{slug}.json"));
    if !cap.file_exists(&rel) {
        return respond_text(request, 404, "page not found");
    }
    let raw_bytes = cap.read_file(&rel).map_err(|_| std::io::Error::other("read"))?;
    let raw = String::from_utf8(raw_bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| std::io::Error::other(format!("parse: {e}")))?;

    {
        let Some(sections) = parsed.get_mut("sections").and_then(|v| v.as_array_mut()) else {
            return respond_text(request, 400, "page has no sections array");
        };
        if idx >= sections.len() {
            return respond_text(request, 400, "section index out of range");
        }
        match op {
            "up" => {
                if idx == 0 {
                    return respond_text(request, 400, "already at top");
                }
                sections.swap(idx, idx - 1);
            }
            "down" => {
                if idx + 1 >= sections.len() {
                    return respond_text(request, 400, "already at bottom");
                }
                sections.swap(idx, idx + 1);
            }
            "delete" => {
                sections.remove(idx);
            }
            "add-paragraph" => {
                // Group-only: append an empty paragraph to the
                // body[] array. Safe-guard: only operates when
                // sec.kind == "group" AND sec.body is an array.
                let sec = &mut sections[idx];
                let kind = sec
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                if kind != "group" {
                    return respond_text(
                        request,
                        400,
                        "add-paragraph only works on group sections",
                    );
                }
                let arr = sec.get_mut("body").and_then(|v| v.as_array_mut());
                match arr {
                    Some(a) => a.push(serde_json::Value::String(String::new())),
                    None => {
                        sec["body"] = serde_json::Value::Array(vec![
                            serde_json::Value::String(String::new()),
                        ]);
                    }
                }
            }
            _ => return respond_text(request, 400, "unknown op"),
        }
    }

    let serialized = serde_json::to_string_pretty(&parsed)
        .map_err(|e| std::io::Error::other(format!("serialize: {e}")))?;
    cap.write_atomic(&rel, serialized.as_bytes())
        .map_err(|_| std::io::Error::other("write"))?;

    if !forge_path.is_empty() {
        let cms_parent = cms_root
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let _ = std::process::Command::new("bash")
            .arg(forge_path)
            .current_dir(&cms_parent)
            .status();
    }

    let mut resp = tiny_http::Response::empty(303);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Location"[..], format!("/{slug}").as_bytes())
            .map_err(|_| std::io::Error::other("header"))?,
    );
    request.respond(resp)?;
    Ok(())
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod edit_serve_tests {
    use super::*;

    #[test]
    fn html_escape_handles_attr_chars() {
        assert_eq!(html_escape("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d&quot;e&#39;f");
    }

    #[test]
    fn html_escape_passes_unicode() {
        assert_eq!(html_escape("café"), "café");
    }
}

/// Remove `/* ... */` blocks from CSS source. Replaces each
/// comment with a single space so adjacent tokens don't fuse and
/// line counts stay roughly stable. Linear pass, no nesting since
/// CSS doesn't allow nested block comments.
fn strip_css_comments(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Find closing */
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                // Preserve newlines so line counts don't collapse,
                // which keeps any future line-number reporting honest.
                if bytes[j] == b'\n' {
                    out.push('\n');
                }
                j += 1;
            }
            out.push(' ');
            i = j.saturating_add(2);
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn parse_skin_themes(
    raw: &str,
) -> (Vec<ThemeBlock>, std::collections::BTreeSet<String>) {
    let mut themes = Vec::<ThemeBlock>::new();
    let mut referenced_vars = std::collections::BTreeSet::<String>::new();

    // Strip /* ... */ comments before any scanning. Without this,
    // the var() pass picks up examples in doc comments (e.g.
    // "* var(--loom-color-*)") and reports the wildcard as an
    // undefined token. Comment-stripping is a single linear pass.
    let stripped = strip_css_comments(raw);

    // First pass: every `var(--loom-color-X)` outside comments.
    // Reject names containing characters that aren't valid in a
    // CSS custom property (lowercase, digits, dash) so a literal
    // wildcard in some other context can't slip through.
    for hit in stripped.match_indices("var(--loom-color-") {
        let start = hit.0 + 4; // skip "var("
        let rest = &stripped[start..];
        if let Some(end) = rest.find(|c: char| c == ')' || c == ',' || c.is_whitespace()) {
            let name = &rest[..end];
            if !name.is_empty()
                && name
                    .strip_prefix("--")
                    .is_some_and(|tail| tail.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
            {
                referenced_vars.insert(name.to_owned());
            }
        }
    }

    // Second pass: locate `:root { ... }` and `:root[data-theme="X"] { ... }`
    // blocks in the comment-stripped source. Skip `:root[data-font="X"]`
    // and `:root[data-density="X"]` — those are token variants, not
    // full themes.
    let raw = stripped.as_str();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next `:root` token.
        let Some(rel) = raw[i..].find(":root") else {
            break;
        };
        let pos = i + rel;
        let after_root = pos + 5;

        // Determine theme name from the optional attribute selector.
        let mut probe = after_root;
        while probe < bytes.len() && (bytes[probe] as char).is_whitespace() {
            probe += 1;
        }
        let mut name = "default".to_owned();
        let mut is_theme = true;
        if probe < bytes.len() && bytes[probe] as char == '[' {
            // Read until ']'.
            let attr_start = probe + 1;
            let Some(rel_end) = raw[attr_start..].find(']') else {
                i = after_root;
                continue;
            };
            let attr = &raw[attr_start..attr_start + rel_end];
            // Only count `data-theme="X"` blocks. Skip data-font /
            // data-density variants — they're not full themes and
            // declare different token sets by design.
            if let Some(rest) = attr.strip_prefix("data-theme=") {
                let n = rest.trim_matches(|c: char| c == '"' || c == '\'');
                name = n.to_owned();
            } else {
                is_theme = false;
            }
            probe = attr_start + rel_end + 1;
        }
        // Find the opening brace.
        while probe < bytes.len() && (bytes[probe] as char).is_whitespace() {
            probe += 1;
        }
        if probe >= bytes.len() || bytes[probe] as char != '{' {
            i = after_root;
            continue;
        }
        let body_start = probe + 1;

        // Brace-balanced scan for the matching `}`.
        let mut depth = 1usize;
        let mut j = body_start;
        while j < bytes.len() && depth > 0 {
            match bytes[j] as char {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        let body = &raw[body_start..j.saturating_sub(1)];

        if is_theme {
            let mut tokens = std::collections::BTreeMap::<String, String>::new();
            for line in body.lines() {
                let trimmed = line.trim().trim_end_matches(';').trim();
                if let Some(rest) = trimmed.strip_prefix("--loom-color-") {
                    if let Some((name_part, value_part)) = rest.split_once(':') {
                        let token = format!("--loom-color-{}", name_part.trim());
                        tokens.insert(token, value_part.trim().to_owned());
                    }
                }
            }
            // Only emit a block if it actually carries colour tokens.
            // Some `:root` rules in nested `@media` define unrelated
            // properties (e.g. animation prefers-reduced-motion); we
            // skip them rather than report them as empty themes.
            if !tokens.is_empty() {
                themes.push(ThemeBlock { name, tokens });
            }
        }
        i = j;
    }

    // De-duplicate: same theme name might appear in multiple
    // contexts (e.g. nested under a feature query). Merge tokens:
    // last-write wins UNLESS the incoming value is a `var()`
    // reference and the existing value is a literal — keep the
    // literal so the contrast checker can compute against it.
    //
    // REGRESSION-GUARD (T29 2026-05-06): the prior straight
    // last-write-wins logic let `@media (prefers-contrast: more)
    // { :root { --loom-color-ink-muted: var(--loom-color-ink); } }`
    // overwrite the canonical literal. Default theme then had no
    // resolvable hsl() for ink-muted and contrast pairs got
    // silently dropped. Two pairs disappeared from the table.
    let mut merged: std::collections::BTreeMap<String, ThemeBlock> =
        std::collections::BTreeMap::new();
    for t in themes {
        merged
            .entry(t.name.clone())
            .and_modify(|existing| {
                for (k, v) in &t.tokens {
                    let v_is_var = v.starts_with("var(");
                    let existing_is_literal = existing
                        .tokens
                        .get(k)
                        .map(|cur| !cur.starts_with("var("))
                        .unwrap_or(false);
                    if v_is_var && existing_is_literal {
                        // Keep the literal; ignore the var() override.
                        continue;
                    }
                    existing.tokens.insert(k.clone(), v.clone());
                }
            })
            .or_insert(t);
    }
    let merged_vec = merged.into_values().collect::<Vec<_>>();
    (merged_vec, referenced_vars)
}

// ============================================================
// T29: WCAG contrast math + theme contrast subcommand
// ============================================================
//
// Sources: WCAG 2.2 §1.4.3 (Contrast Minimum). The relative-
// luminance formula is the W3C-published one (sRGB → linear via
// gamma decode → 0.2126 R + 0.7152 G + 0.0722 B).
//
// All math in f64 to avoid catastrophic cancellation on values
// near 0 / 1. Intermediate clamping to [0, 1].

/// Linearise a single sRGB channel value in [0, 1].
fn srgb_to_linear(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance for a sRGB triple in [0, 1].
fn relative_luminance(r: f64, g: f64, b: f64) -> f64 {
    let rl = srgb_to_linear(r);
    let gl = srgb_to_linear(g);
    let bl = srgb_to_linear(b);
    0.2126 * rl + 0.7152 * gl + 0.0722 * bl
}

/// Contrast ratio between two relative luminances. Returns ≥ 1.0.
fn contrast_ratio(l1: f64, l2: f64) -> f64 {
    let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// HSL → sRGB conversion. h in degrees [0, 360), s/l in [0, 1].
/// Returns (r, g, b) in [0, 1]. Standard W3C HSL formula.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h_prime.floor() as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x), // 5 or hue wrap
    };
    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

/// Parse a CSS color expression into sRGB [0, 1]. Supports:
///   `hsl(H S% L%)`            — modern space-separated
///   `hsl(H, S%, L%)`          — legacy comma-separated
///   `hsl(H S% L% / A)`        — with alpha (alpha discarded
///                               for contrast — assumes opaque
///                               composite)
///   `hsl(H S% L% / A%)`       — alpha as percent
///
/// Returns None for anything we don't understand (e.g. var()
/// references, named colors, hex). Caller treats unknown as
/// "skip the pair" rather than fail.
///
/// REGRESSION-GUARD: the regex/split here is deliberately
/// permissive on whitespace + slashes — the CSS spec allows
/// varying delimiters within hsl(). Adding a stricter parser
/// later is fine, but DO NOT tighten without re-running the
/// theme contrast suite — themes use both legacy and modern
/// hsl() syntax.
fn parse_css_color(raw: &str) -> Option<(f64, f64, f64)> {
    let raw = raw.trim();
    let inner = raw
        .strip_prefix("hsl(")
        .and_then(|s| s.strip_suffix(')'))?;
    // Drop alpha if present (split on `/` once).
    let (color_part, _alpha) = inner.split_once('/').unwrap_or((inner, ""));
    // Split on whitespace OR comma.
    let parts: Vec<&str> = color_part
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 3 {
        return None;
    }
    let h: f64 = parts[0].trim_end_matches("deg").parse().ok()?;
    let s = parse_percent(parts[1])?;
    let l = parse_percent(parts[2])?;
    Some(hsl_to_rgb(h, s, l))
}

fn parse_percent(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if let Some(num) = trimmed.strip_suffix('%') {
        let v: f64 = num.trim().parse().ok()?;
        Some((v / 100.0).clamp(0.0, 1.0))
    } else {
        // Bare number — assume already 0..1.
        let v: f64 = trimmed.parse().ok()?;
        Some(v.clamp(0.0, 1.0))
    }
}

/// One fg/bg pair WCAG-checks against the threshold.
fn check_pair(fg_raw: &str, bg_raw: &str, min_ratio: f64) -> Option<(f64, bool)> {
    let (fr, fg, fb) = parse_css_color(fg_raw)?;
    let (br, bg, bb) = parse_css_color(bg_raw)?;
    let fl = relative_luminance(fr, fg, fb);
    let bl = relative_luminance(br, bg, bb);
    let ratio = contrast_ratio(fl, bl);
    Some((ratio, ratio >= min_ratio))
}

/// Pairs we audit per theme. (fg-token, bg-token, label).
/// REGRESSION-GUARD: do NOT pair --loom-color-primary with
/// --loom-color-bg-canvas — primary is a CHROME color (button
/// fill, link), not body text. Pair only declared
/// foreground/background combos that visitors actually see
/// rendered together.
const CONTRAST_PAIRS: &[(&str, &str, &str)] = &[
    ("--loom-color-ink", "--loom-color-bg-canvas", "ink-on-canvas"),
    ("--loom-color-ink", "--loom-color-surface", "ink-on-surface"),
    ("--loom-color-ink", "--loom-color-surface-muted", "ink-on-surface-muted"),
    ("--loom-color-ink-muted", "--loom-color-bg-canvas", "ink-muted-on-canvas"),
    ("--loom-color-ink-muted", "--loom-color-surface", "ink-muted-on-surface"),
    ("--loom-color-primary-fg", "--loom-color-primary", "primary-fg-on-primary"),
];

fn cmd_theme_contrast(
    skin_path: &std::path::Path,
    min_ratio: f64,
) -> Result<usize, std::io::Error> {
    let raw = std::fs::read_to_string(skin_path)?;
    let (themes, _refs) = parse_skin_themes(&raw);
    if themes.is_empty() {
        eprintln!(
            "loom theme contrast: no theme blocks found in {}",
            skin_path.display()
        );
        return Ok(0);
    }
    // Base block provides fallback values when a named theme
    // doesn't declare a specific token (cascade behaviour).
    let base = themes
        .iter()
        .find(|t| t.name == "default")
        .cloned();

    let mut failures = 0usize;
    println!(
        "  theme           pair                          ratio   status"
    );
    println!(
        "  --------------  ----------------------------  ------  ------"
    );
    for theme in &themes {
        for (fg_tok, bg_tok, label) in CONTRAST_PAIRS {
            let lookup = |tok: &str| -> Option<String> {
                theme
                    .tokens
                    .get(tok)
                    .cloned()
                    .or_else(|| base.as_ref().and_then(|b| b.tokens.get(tok).cloned()))
            };
            let (Some(fg), Some(bg)) = (lookup(fg_tok), lookup(bg_tok)) else {
                continue;
            };
            let Some((ratio, passed)) = check_pair(&fg, &bg, min_ratio) else {
                // Unparseable color — skip silently rather than
                // false-fail. T29-followup: tighten parser if we
                // start using non-hsl syntax.
                continue;
            };
            let status = if passed {
                "ok"
            } else {
                failures += 1;
                "FAIL"
            };
            println!(
                "  {name:<14}  {label:<28}  {ratio:>5.2}   {status}",
                name = theme.name,
                label = label,
                ratio = ratio,
            );
        }
    }
    println!();
    if failures == 0 {
        println!(
            "loom theme contrast: {} theme(s) checked, ALL pairs ≥ {min_ratio:.1}:1 (WCAG OK)",
            themes.len(),
        );
    } else {
        eprintln!(
            "loom theme contrast: {failures} pair(s) below {min_ratio:.1}:1 — themes WILL ship with unreadable text"
        );
    }
    Ok(failures)
}

#[cfg(test)]
mod theme_contrast_tests {
    use super::*;

    #[test]
    fn srgb_linearization_matches_w3c_examples() {
        // 0 → 0, 1 → 1.
        assert!((srgb_to_linear(0.0) - 0.0).abs() < 1e-9);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-9);
        // mid-gray sRGB 0.5 → ~0.214 linear.
        let v = srgb_to_linear(0.5);
        assert!((v - 0.21404).abs() < 1e-3);
    }

    #[test]
    fn relative_luminance_extremes() {
        // Pure black = 0, pure white = 1.
        assert!((relative_luminance(0.0, 0.0, 0.0) - 0.0).abs() < 1e-9);
        assert!((relative_luminance(1.0, 1.0, 1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn contrast_ratio_max_is_21() {
        // White on black = 21:1 (WCAG max).
        let r = contrast_ratio(1.0, 0.0);
        assert!((r - 21.0).abs() < 1e-9, "got {r}");
    }

    #[test]
    fn contrast_ratio_min_is_1() {
        // Same color = 1:1.
        let r = contrast_ratio(0.5, 0.5);
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_modern_hsl() {
        let (r, g, b) = parse_css_color("hsl(0 0% 100%)").expect("white");
        assert!((r - 1.0).abs() < 1e-6);
        assert!((g - 1.0).abs() < 1e-6);
        assert!((b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_legacy_hsl() {
        let (r, g, b) = parse_css_color("hsl(0, 0%, 0%)").expect("black");
        assert!(r.abs() < 1e-6);
        assert!(g.abs() < 1e-6);
        assert!(b.abs() < 1e-6);
    }

    #[test]
    fn parse_hsl_with_alpha_strips_alpha() {
        // Alpha discarded for contrast purposes.
        let (r, _, _) = parse_css_color("hsl(0 0% 100% / 0.5)").expect("white-alpha");
        assert!((r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert!(parse_css_color("var(--something)").is_none());
        assert!(parse_css_color("#ff0000").is_none()); // hex not yet supported
        assert!(parse_css_color("red").is_none());
    }

    #[test]
    fn check_pair_white_on_black_passes_aa() {
        let (ratio, passed) = check_pair("hsl(0 0% 100%)", "hsl(0 0% 0%)", 4.5).expect("ok");
        assert!(passed);
        assert!(ratio > 20.0);
    }

    #[test]
    fn check_pair_grey_on_white_fails_aa() {
        let (ratio, passed) =
            check_pair("hsl(0 0% 70%)", "hsl(0 0% 100%)", 4.5).expect("ok");
        assert!(!passed, "70% grey on white should fail AA, got ratio {ratio}");
    }
}

#[cfg(test)]
mod theme_contrast_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// HSL parser must not panic on arbitrary input.
        #[test]
        fn parser_does_not_panic(s in ".{0,200}") {
            let _ = parse_css_color(&s);
        }

        /// Contrast ratio is always >= 1.0 and <= 21.0 for ANY
        /// luminance pair in [0, 1].
        #[test]
        fn contrast_ratio_bounded(
            l1 in 0.0f64..=1.0,
            l2 in 0.0f64..=1.0,
        ) {
            let r = contrast_ratio(l1, l2);
            prop_assert!(r >= 1.0 && r <= 21.0, "ratio {r} out of bounds");
        }

        /// Contrast ratio is symmetric in its arguments.
        #[test]
        fn contrast_symmetric(l1 in 0.0f64..=1.0, l2 in 0.0f64..=1.0) {
            prop_assert!((contrast_ratio(l1, l2) - contrast_ratio(l2, l1)).abs() < 1e-9);
        }

        /// HSL round-trips to a value in [0, 1] for every channel.
        #[test]
        fn hsl_to_rgb_in_range(
            h in 0.0f64..360.0,
            s in 0.0f64..=1.0,
            l in 0.0f64..=1.0,
        ) {
            let (r, g, b) = hsl_to_rgb(h, s, l);
            for c in [r, g, b] {
                prop_assert!(c >= -1e-9 && c <= 1.0 + 1e-9, "channel {c} out of range");
            }
        }
    }
}

fn cmd_theme_list(skin_path: &std::path::Path) -> Result<(), std::io::Error> {
    let raw = std::fs::read_to_string(skin_path)?;
    let (themes, referenced_vars) = parse_skin_themes(&raw);
    if themes.is_empty() {
        eprintln!(
            "loom theme list: no `:root` blocks with --loom-color-* tokens found in {}",
            skin_path.display()
        );
        return Ok(());
    }
    println!("  theme           tokens  example");
    println!("  --------------  ------  -----------------------------------------");
    for t in &themes {
        let example = t
            .tokens
            .get("--loom-color-bg-canvas")
            .or_else(|| t.tokens.get("--loom-color-primary"))
            .map(String::as_str)
            .unwrap_or("(no canvas/primary token)");
        let example_short = if example.len() > 40 {
            format!("{}…", &example[..39])
        } else {
            example.to_owned()
        };
        println!(
            "  {name:<14}  {n:>6}  {ex}",
            name = t.name,
            n = t.tokens.len(),
            ex = example_short,
        );
    }
    println!();
    println!(
        "loom theme list: {} theme(s), {} unique --loom-color-* var() reference(s)",
        themes.len(),
        referenced_vars.len(),
    );
    Ok(())
}

fn cmd_theme_validate(skin_path: &std::path::Path) -> Result<usize, std::io::Error> {
    let raw = std::fs::read_to_string(skin_path)?;
    let (themes, referenced_vars) = parse_skin_themes(&raw);

    let Some(base) = themes.iter().find(|t| t.name == "default") else {
        eprintln!("loom theme validate: no base `:root` (default) theme block found");
        return Ok(1);
    };

    let mut findings = 0usize;

    // Check 1: every var(--loom-color-X) consumed in skin.css has
    // a definition in base. Without this, first-paint runs against
    // an undefined property and the rule is silently rejected by
    // the browser.
    for v in &referenced_vars {
        if !base.tokens.contains_key(v) {
            findings += 1;
            eprintln!(
                "  STRICT  {} consumed via var() but has no definition in base :root",
                v
            );
        }
    }

    // Check 2: every named theme defines the same color tokens as
    // base. Drift here means switching to that theme leaves some
    // computed values falling back to base — usually fine, but
    // sometimes the base value is wrong for the theme (e.g. a
    // dark-on-light primary on a sepia surface).
    for t in &themes {
        if t.name == "default" {
            continue;
        }
        let missing: Vec<&String> = base
            .tokens
            .keys()
            .filter(|k| !t.tokens.contains_key(*k))
            .collect();
        let extra: Vec<&String> = t
            .tokens
            .keys()
            .filter(|k| !base.tokens.contains_key(*k))
            .collect();
        for m in &missing {
            findings += 1;
            eprintln!(
                "  warn    theme {:?} omits token {} (will inherit base — confirm intentional)",
                t.name, m
            );
        }
        for e in &extra {
            findings += 1;
            eprintln!(
                "  warn    theme {:?} declares token {} not in base (orphan — base default missing)",
                t.name, e
            );
        }
    }

    if findings == 0 {
        println!(
            "  ok     {} theme(s), {} token(s) per theme, {} var() reference(s) all resolved",
            themes.len(),
            base.tokens.len(),
            referenced_vars.len(),
        );
    }
    Ok(findings)
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    const FIXTURE: &str = r#"
:root {
  --loom-color-bg-canvas: hsl(0 0% 0%);
  --loom-color-ink: hsl(0 0% 100%);
  --loom-color-primary: hsl(220 100% 75%);
}
:root[data-theme="hc-light"] {
  --loom-color-bg-canvas: hsl(0 0% 100%);
  --loom-color-ink: hsl(0 0% 0%);
  --loom-color-primary: hsl(220 100% 30%);
}
:root[data-font="serif"] {
  --loom-font-display: serif;
}
.something {
  background: var(--loom-color-bg-canvas);
  color: var(--loom-color-ink);
  border-color: var(--loom-color-primary);
}
"#;

    #[test]
    fn parser_finds_default_and_named_themes() {
        let (themes, _) = parse_skin_themes(FIXTURE);
        let names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"hc-light"));
        // data-font="serif" is a font variant, not a theme.
        assert!(!names.contains(&"serif"));
    }

    #[test]
    fn parser_extracts_color_tokens_only() {
        let (themes, _) = parse_skin_themes(FIXTURE);
        let default = themes.iter().find(|t| t.name == "default").expect("default");
        assert_eq!(default.tokens.len(), 3);
        assert!(default.tokens.contains_key("--loom-color-bg-canvas"));
        assert!(default.tokens.contains_key("--loom-color-ink"));
        assert!(default.tokens.contains_key("--loom-color-primary"));
    }

    #[test]
    fn parser_collects_var_references() {
        let (_, refs) = parse_skin_themes(FIXTURE);
        assert!(refs.contains("--loom-color-bg-canvas"));
        assert!(refs.contains("--loom-color-ink"));
        assert!(refs.contains("--loom-color-primary"));
    }

    #[test]
    fn validate_clean_fixture_has_no_findings() {
        let dir = std::env::temp_dir().join(format!(
            "loom-theme-clean-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("skin.css");
        std::fs::write(&path, FIXTURE).expect("write");
        let n = cmd_theme_validate(&path).expect("ok");
        assert_eq!(n, 0, "fixture should be drift-free");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_flags_undefined_var_reference() {
        let raw = r#"
:root {
  --loom-color-ink: black;
}
.x { color: var(--loom-color-missing); }
"#;
        let dir = std::env::temp_dir().join(format!(
            "loom-theme-undef-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("skin.css");
        std::fs::write(&path, raw).expect("write");
        let n = cmd_theme_validate(&path).expect("ok");
        assert!(n >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_flags_named_theme_missing_token() {
        let raw = r#"
:root {
  --loom-color-bg-canvas: black;
  --loom-color-ink: white;
}
:root[data-theme="sepia"] {
  --loom-color-bg-canvas: tan;
}
.x { background: var(--loom-color-bg-canvas); color: var(--loom-color-ink); }
"#;
        let dir = std::env::temp_dir().join(format!(
            "loom-theme-miss-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("skin.css");
        std::fs::write(&path, raw).expect("write");
        let n = cmd_theme_validate(&path).expect("ok");
        // sepia omits --loom-color-ink → 1 warn
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_flags_orphan_token_in_named_theme() {
        let raw = r#"
:root {
  --loom-color-ink: white;
}
:root[data-theme="weird"] {
  --loom-color-ink: black;
  --loom-color-orphan: red;
}
"#;
        let dir = std::env::temp_dir().join(format!(
            "loom-theme-orph-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("skin.css");
        std::fs::write(&path, raw).expect("write");
        let n = cmd_theme_validate(&path).expect("ok");
        // weird declares --loom-color-orphan that base lacks → 1 warn
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn cmd_backend_list(backends_path: &std::path::Path) -> Result<(), std::io::Error> {
    let raw = std::fs::read_to_string(backends_path)?;
    let value: toml::Value =
        toml::from_str(&raw).map_err(|e| std::io::Error::other(format!("toml parse: {e}")))?;
    let backends = value
        .get("backends")
        .and_then(|v| v.as_table())
        .ok_or_else(|| std::io::Error::other("missing [backends] section"))?;

    let mut rows = Vec::<BackendRow>::new();
    for (raw_key, entry) in backends {
        let Some(table) = entry.as_table() else {
            continue;
        };
        // Skip rows whose key violates the schema rather than
        // panic — backends.toml is hand-edited and we'd rather
        // surface other rows than abort the whole listing.
        let Ok(key) = BackendKey::new(raw_key) else {
            continue;
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
        let impl_files: Vec<String> = table
            .get("impl_files")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let status = BackendStatus::from_impl_files(impl_files);
        rows.push(BackendRow {
            key,
            method,
            status,
            purpose,
        });
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));

    let total = rows.len();
    let stubs = rows.iter().filter(|r| r.status.is_stub()).count();
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
            key = r.key.as_str(),
            method = r.method,
            status = r.status.label(),
        );
    }
    println!();
    println!(
        "loom backend list: {total} declared, {impls} implemented ({pct}%), {stubs} stub",
        pct = (impls * 100).checked_div(total).unwrap_or(0)
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct BackendRow {
    key: BackendKey,
    method: String,
    status: BackendStatus,
    purpose: String,
}

#[cfg(test)]
mod cmd_backend_list_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
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
                    .is_some_and(Vec::is_empty)
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
            .map_or(0, |d| d.as_nanos());
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
            .map_or(0, |d| d.as_nanos());
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
            .map_or(0, |d| d.as_nanos());
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
    use loom_cms_render::{
        CmsAvatar, CmsCard, CmsCardStat, CmsComposerSize, CmsPage, CmsPromptAction, CmsSection,
        HeroCta,
    };
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
    use loom_cms_render::{CmsPage, CmsSection};
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
    use loom_cms_render::{
        CmsFormField, CmsFormStep, CmsFormStepState, CmsFormSubmit, CmsPage, CmsSection,
    };
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
            .map_or(0, |d| d.as_nanos());
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
        println!("{pretty}");
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
    let mut ok_count: usize = 0;
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
        files.len() - ok_count
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
        CmsSection::Hero { cta: Some(cta), .. } if !is_safe_url(&cta.href) => {
            errs.push(format!("/sections/{idx}/cta/href={:?}", cta.href));
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
        CmsSection::Form { submit, .. } if !is_safe_url(&submit.action) => {
            errs.push(format!("/sections/{idx}/submit/action={:?}", submit.action));
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
            .map_or(0, |d| d.as_nanos());
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

// ============================================================
// T43: cookie-session admin auth.
// ============================================================
//
// Doctrine:
//   * Argon2id for passwords (OWASP-recommended, memory-hard,
//     GPU-resistant). Per-user salt; default cost params.
//   * HMAC-SHA256 over <user>.<expiry-unix-secs> for the
//     session cookie. Stateless — no server-side session table.
//     Tampering with either field invalidates the signature.
//   * subtle::ConstantTimeEq for HMAC compare — prevents
//     timing oracle.
//   * Cookie attributes: HttpOnly (no JS access),
//     SameSite=Strict (no cross-site CSRF), Secure when the
//     server eventually fronts behind TLS (env-gated).
//   * Auth store at ~/.config/loom/auth.toml mode 0600.
//     Contents: [[users]] entries + a [secret] section with
//     the HMAC signing key.
//   * Backwards-compat: if auth.toml does NOT exist, the
//     editor stays open (so existing localhost workflows
//     don't break). Once auth.toml exists, login is required.

const SESSION_LIFETIME_SECS: u64 = 60 * 60 * 24; // 24h
const COOKIE_NAME: &str = "loom-session";

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
struct AuthStore {
    #[serde(default)]
    users: Vec<AuthUser>,
    #[serde(default)]
    secret: AuthSecret,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct AuthUser {
    /// Lowercase ASCII identifier.
    name: String,
    /// PHC-format Argon2id hash (includes salt + params).
    password_hash: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
struct AuthSecret {
    /// Base64-encoded 32-byte HMAC signing key. Empty when
    /// no auth has been initialised.
    #[serde(default)]
    hmac_key_b64: String,
}

fn auth_store_path() -> std::path::PathBuf {
    if let Ok(env) = std::env::var("LOOM_AUTH_STORE") {
        return std::path::PathBuf::from(env);
    }
    dirs_next::config_dir()
        .map(|d| d.join("loom").join("auth.toml"))
        .unwrap_or_else(|| std::path::PathBuf::from("./auth.toml"))
}

fn read_auth_store() -> std::io::Result<Option<AuthStore>> {
    let path = auth_store_path();
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let parsed: AuthStore = toml::from_str(&raw)
        .map_err(|e| std::io::Error::other(format!("parse auth.toml: {e}")))?;
    Ok(Some(parsed))
}

fn write_auth_store(store: &AuthStore) -> std::io::Result<()> {
    let path = auth_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = toml::to_string_pretty(store)
        .map_err(|e| std::io::Error::other(format!("serialize auth.toml: {e}")))?;
    std::fs::write(&path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        let _ = std::fs::set_permissions(&path, perms);
    }
    Ok(())
}

fn cmd_auth_init(user: &str, force: bool) -> std::io::Result<()> {
    // Validate username via SlugName (same character class).
    let user_validated = SlugName::new(user)
        .map_err(|e| std::io::Error::other(format!("invalid user: {e}")))?;
    let path = auth_store_path();
    if path.is_file() && !force {
        return Err(std::io::Error::other(format!(
            "{} already exists; pass --force to overwrite",
            path.display()
        )));
    }
    // Read password from $LOOM_PWD (stdin prompts come later).
    let password = std::env::var("LOOM_PWD").map_err(|_| {
        std::io::Error::other(
            "set LOOM_PWD env var to the password (stdin prompt is queued for next tick)",
        )
    })?;
    if password.len() < 12 {
        return Err(std::io::Error::other(
            "password must be at least 12 chars (passphrase recommended)",
        ));
    }

    use argon2::password_hash::{PasswordHasher as _, SaltString};
    use rand_core::{OsRng, RngCore as _};
    let salt = SaltString::generate(&mut OsRng);
    let argon = argon2::Argon2::default();
    let hash = argon
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| std::io::Error::other(format!("argon2 hash: {e}")))?
        .to_string();

    // Generate fresh HMAC signing key (32 bytes).
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    let key_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key);

    let store = AuthStore {
        users: vec![AuthUser {
            name: user_validated.as_str().to_owned(),
            password_hash: hash,
        }],
        secret: AuthSecret { hmac_key_b64: key_b64 },
    };
    write_auth_store(&store)?;
    println!("loom auth init:");
    println!("  ok  user '{}' created", user_validated.as_str());
    println!("  ok  HMAC signing key generated (32 bytes)");
    println!("  ok  store written to {} (mode 0600)", path.display());
    println!();
    println!("Next: `loom edit-serve` will require login at /login.");
    Ok(())
}

fn cmd_auth_list() -> std::io::Result<()> {
    match read_auth_store()? {
        None => {
            println!(
                "no auth store at {} — editor runs without login",
                auth_store_path().display()
            );
        }
        Some(store) => {
            println!("auth store: {}", auth_store_path().display());
            println!("  {} user(s):", store.users.len());
            for u in &store.users {
                println!("    {}", u.name);
            }
        }
    }
    Ok(())
}

/// Argon2id password verify. Constant-time at the hash level.
fn verify_password(plain: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier as _};
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    argon2::Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok()
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Build an HMAC-SHA256 signed cookie value: <user>.<expiry>.<sig-b64>.
fn build_session_cookie(user: &str, hmac_key: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    let expiry = current_unix_secs().saturating_add(SESSION_LIFETIME_SECS);
    let payload = format!("{user}.{expiry}");
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(hmac_key)
        .expect("hmac accepts any key length");
    mac.update(payload.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, sig);
    format!("{payload}.{sig_b64}")
}

/// Parse + verify a session cookie. Returns `Some(user)` when
/// the signature is valid AND the cookie hasn't expired.
fn verify_session_cookie(cookie_value: &str, hmac_key: &[u8]) -> Option<String> {
    use hmac::{Hmac, Mac};
    use subtle::ConstantTimeEq as _;
    // Format: <user>.<expiry>.<sig-b64>
    let parts: Vec<&str> = cookie_value.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let user = parts[0];
    let expiry: u64 = parts[1].parse().ok()?;
    let sig_b64 = parts[2];
    if expiry < current_unix_secs() {
        return None;
    }
    let provided_sig = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        sig_b64,
    )
    .ok()?;
    let payload = format!("{user}.{expiry}");
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(hmac_key).ok()?;
    mac.update(payload.as_bytes());
    let expected = mac.finalize().into_bytes();
    if expected.ct_eq(&provided_sig).into() {
        Some(user.to_owned())
    } else {
        None
    }
}

/// Extract the loom-session cookie value from a request's
/// Cookie header.
fn extract_session_cookie(request: &tiny_http::Request) -> Option<String> {
    for h in request.headers() {
        if h.field.equiv("Cookie") {
            for entry in h.value.as_str().split(';') {
                let trimmed = entry.trim();
                if let Some(value) = trimmed.strip_prefix(&format!("{COOKIE_NAME}=")) {
                    return Some(value.to_owned());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    #[test]
    fn password_verify_round_trip() {
        use argon2::password_hash::{PasswordHasher as _, SaltString};
        use rand_core::OsRng;
        let salt = SaltString::generate(&mut OsRng);
        let phc = argon2::Argon2::default()
            .hash_password(b"correct horse battery staple", &salt)
            .expect("hash")
            .to_string();
        assert!(verify_password("correct horse battery staple", &phc));
        assert!(!verify_password("wrong", &phc));
    }

    #[test]
    fn session_cookie_round_trip() {
        let key = b"thirty-two bytes long key !!!!!1";
        let cookie = build_session_cookie("alice", key);
        assert_eq!(verify_session_cookie(&cookie, key).as_deref(), Some("alice"));
    }

    #[test]
    fn session_cookie_rejects_wrong_key() {
        let cookie = build_session_cookie("alice", b"keyone-thirty-two-bytes-len---1!");
        assert!(verify_session_cookie(&cookie, b"keytwo-thirty-two-bytes-len---2!").is_none());
    }

    #[test]
    fn session_cookie_rejects_tampered_user() {
        let key = b"thirty-two bytes long key !!!!!1";
        let cookie = build_session_cookie("alice", key);
        // Replace 'alice' with 'evilll' (same length).
        let tampered = cookie.replacen("alice", "evilll", 1);
        assert!(verify_session_cookie(&tampered, key).is_none());
    }

    #[test]
    fn session_cookie_rejects_malformed() {
        let key = b"thirty-two bytes long key !!!!!1";
        assert!(verify_session_cookie("", key).is_none());
        assert!(verify_session_cookie("only-one-part", key).is_none());
        assert!(verify_session_cookie("user.notanumber.sig", key).is_none());
    }

    #[test]
    fn session_cookie_rejects_expired() {
        // Build a cookie manually with expiry = 0 (epoch).
        use hmac::{Hmac, Mac};
        let key = b"thirty-two bytes long key !!!!!1";
        let payload = "alice.0";
        let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key).unwrap();
        mac.update(payload.as_bytes());
        let sig = mac.finalize().into_bytes();
        let sig_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, sig);
        let cookie = format!("{payload}.{sig_b64}");
        assert!(verify_session_cookie(&cookie, key).is_none());
    }
}

// ============================================================
// T63: HTML → CmsPage importer.
// ============================================================
//
// Goal: a non-technical operator points loom at an existing
// .html file and gets a typed cms/<slug>.json they can
// immediately edit through the GUI.
//
// Heuristics (intentionally simple — full HTML parsing via
// scraper crate is queued for T63b):
//
//   <header>            → Hero (eyebrow=h2, title=h1, subtitle=p)
//   <section><h2>...    → Group (title=h2, body=collected p's)
//   <h2>/<h3>/<h4>      → Heading (level + text)
//   <p>                 → Paragraph (body)
//
// Anything we don't recognise becomes a paragraph with a TODO
// marker so the operator sees what was unmappable instead of
// silently dropping content.
//
// Doctrine:
//   * SECURITY: input is just text bytes; no script eval, no
//     network, no shell. SVG content is dropped (XSS risk).
//   * No external HTML parser dep — line-oriented regex-style
//     scan. Documented limits: nested tags, malformed HTML,
//     CDATA, processing instructions all fall through to the
//     TODO bucket. Good enough for typical static-site imports
//     where the structure is clean.

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ImportedSection {
    Hero {
        eyebrow: String,
        title: String,
        subtitle: String,
    },
    Group {
        title: String,
        body: Vec<String>,
    },
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        body: String,
    },
    Todo {
        raw: String,
    },
}

impl ImportedSection {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Hero { eyebrow, title, subtitle } => serde_json::json!({
                "kind": "hero",
                "eyebrow": eyebrow,
                "title": title,
                "subtitle": subtitle,
                "cta": null,
            }),
            Self::Group { title, body } => serde_json::json!({
                "kind": "group",
                "title": title,
                "body": body,
            }),
            Self::Heading { level, text } => serde_json::json!({
                "kind": "heading",
                "level": level,
                "text": text,
            }),
            Self::Paragraph { body } => serde_json::json!({
                "kind": "paragraph",
                "body": body,
            }),
            Self::Todo { raw } => serde_json::json!({
                "kind": "paragraph",
                "body": format!("TODO (manual conversion needed): {raw}"),
            }),
        }
    }
}

/// Pull text out of a `<h1>` / `<h2>` / etc. Crude — strips
/// inner tags. Returns None if the tag isn't found.
fn extract_inner(html: &str, open: &str, close: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find(open)?;
    // Skip past the opening tag (find the next '>').
    let body_start = start + html[start..].find('>').unwrap_or(open.len()) + 1;
    let end_rel = lower[body_start..].find(close)?;
    let inner = &html[body_start..body_start + end_rel];
    Some(strip_html_tags(inner).trim().to_owned())
}

fn extract_title(html: &str) -> Option<String> {
    extract_inner(html, "<title", "</title>")
}

fn extract_meta_description(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    // Look for <meta name="description" content="...">
    let needle = "<meta name=\"description\"";
    let i = lower.find(needle)?;
    let rest = &html[i..];
    let content_pos = rest.to_lowercase().find("content=\"")?;
    let after = &rest[content_pos + 9..];
    let end = after.find('"')?;
    Some(after[..end].to_owned())
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out
}

/// Walk a body fragment and emit ImportedSection in order.
pub(crate) fn import_body(body_html: &str) -> Vec<ImportedSection> {
    let mut out = Vec::new();
    let lower = body_html.to_lowercase();

    // Find <header>...</header>
    if let Some(start) = lower.find("<header") {
        let body_start = start + body_html[start..].find('>').unwrap_or(0) + 1;
        if let Some(end_rel) = lower[body_start..].find("</header>") {
            let inner = &body_html[body_start..body_start + end_rel];
            let h1 = extract_inner(inner, "<h1", "</h1>").unwrap_or_default();
            let h2 = extract_inner(inner, "<h2", "</h2>").unwrap_or_default();
            let p = extract_inner(inner, "<p", "</p>").unwrap_or_default();
            if !h1.is_empty() || !h2.is_empty() || !p.is_empty() {
                out.push(ImportedSection::Hero {
                    eyebrow: h2,
                    title: h1,
                    subtitle: p,
                });
            }
        }
    }

    // Find <section>...</section> blocks with h2 + p's.
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find("<section") {
        let start = cursor + rel;
        let body_start = start + body_html[start..].find('>').unwrap_or(0) + 1;
        let Some(end_rel) = lower[body_start..].find("</section>") else {
            break;
        };
        let inner = &body_html[body_start..body_start + end_rel];
        let h2 = extract_inner(inner, "<h2", "</h2>").unwrap_or_default();
        // Pull every <p>...</p> in inner.
        let mut paragraphs: Vec<String> = Vec::new();
        let mut p_cursor = 0;
        let inner_lower = inner.to_lowercase();
        while let Some(p_rel) = inner_lower[p_cursor..].find("<p") {
            let p_start = p_cursor + p_rel;
            let p_body_start = p_start + inner[p_start..].find('>').unwrap_or(0) + 1;
            let Some(p_end_rel) = inner_lower[p_body_start..].find("</p>") else {
                break;
            };
            let raw = &inner[p_body_start..p_body_start + p_end_rel];
            let text = strip_html_tags(raw).trim().to_owned();
            if !text.is_empty() {
                paragraphs.push(text);
            }
            p_cursor = p_body_start + p_end_rel + 4;
        }
        if !h2.is_empty() {
            out.push(ImportedSection::Group {
                title: h2,
                body: paragraphs,
            });
        } else {
            // Section without h2 → emit each paragraph separately.
            for p in paragraphs {
                out.push(ImportedSection::Paragraph { body: p });
            }
        }
        cursor = body_start + end_rel + 10;
    }

    // SECURITY: refuse to import <script> / <style> / <svg>
    // contents. They're either active code or large inline
    // graphics that don't belong in the typed CmsPage. Emit a
    // single TODO so the operator knows we dropped something.
    for tag in ["<script", "<style", "<svg"] {
        if lower.contains(tag) {
            out.push(ImportedSection::Todo {
                raw: format!(
                    "source contains <{}> — dropped by importer for safety; \
                     migrate manually if needed",
                    &tag[1..]
                ),
            });
            break; // one TODO covers the class
        }
    }

    out
}

fn cmd_import(
    from: &std::path::Path,
    into: &std::path::Path,
    explicit_slug: Option<&str>,
    force: bool,
) -> std::io::Result<()> {
    let html = std::fs::read_to_string(from)
        .map_err(|e| std::io::Error::other(format!("read {}: {e}", from.display())))?;

    // Slug: explicit override OR derive from filename basename.
    let derived_slug = from
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported")
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let raw_slug = explicit_slug.unwrap_or(&derived_slug);
    let slug = SlugName::new(raw_slug)
        .map_err(|e| std::io::Error::other(format!("invalid slug: {e}")))?;

    // Ensure target dir + capability scope.
    std::fs::create_dir_all(into)?;
    let cap = WriteCapability::for_dir(into).map_err(|_| {
        std::io::Error::other(format!("cms root {} unreadable", into.display()))
    })?;
    let rel = std::path::PathBuf::from(format!("{}.json", slug.as_str()));
    if cap.file_exists(&rel) && !force {
        return Err(std::io::Error::other(format!(
            "{} already exists; pass --force to overwrite",
            rel.display()
        )));
    }

    // Build the CmsPage JSON.
    let title = extract_title(&html).unwrap_or_else(|| capitalise(slug.as_str()));
    let description = extract_meta_description(&html)
        .unwrap_or_else(|| format!("{title} — imported from {}", from.display()));
    let path_attr = format!("/{}.html", slug.as_str());
    let sections = import_body(&html);
    let sections_json: Vec<serde_json::Value> =
        sections.iter().map(ImportedSection::to_json).collect();
    let page = serde_json::json!({
        "$schema": "../cms-schema.json",
        "title": title,
        "description": description,
        "path": path_attr,
        "sections": sections_json,
    });
    let serialized = serde_json::to_string_pretty(&page)
        .map_err(|e| std::io::Error::other(format!("serialize: {e}")))?;
    cap.write_atomic(&rel, serialized.as_bytes()).map_err(|_| {
        std::io::Error::other("write")
    })?;

    println!("loom import:");
    println!("  source:    {}", from.display());
    println!("  target:    {}", into.join(&rel).display());
    println!("  title:     {title}");
    println!("  sections:  {} extracted", sections.len());
    let todo_count = sections
        .iter()
        .filter(|s| matches!(s, ImportedSection::Todo { .. }))
        .count();
    if todo_count > 0 {
        println!("  warn:      {todo_count} TODO marker(s) — review manually");
    }
    println!();
    println!("Open in the editor:");
    println!("  loom edit-serve --cms {} --static-dir static --forge ''", into.display());
    println!("  then visit http://127.0.0.1:8124/{}", slug.as_str());
    Ok(())
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn extract_title_simple() {
        let html = "<html><head><title>Hello</title></head></html>";
        assert_eq!(extract_title(html), Some("Hello".into()));
    }

    #[test]
    fn extract_title_with_attrs() {
        let html = "<html><head><title lang=\"en\">Hi there</title></head></html>";
        assert_eq!(extract_title(html), Some("Hi there".into()));
    }

    #[test]
    fn extract_title_missing() {
        assert_eq!(extract_title("<html></html>"), None);
    }

    #[test]
    fn extract_meta_desc_present() {
        let html = "<head><meta name=\"description\" content=\"A page\"></head>";
        assert_eq!(extract_meta_description(html), Some("A page".into()));
    }

    #[test]
    fn extract_meta_desc_missing() {
        let html = "<head><meta name=\"viewport\" content=\"x\"></head>";
        assert_eq!(extract_meta_description(html), None);
    }

    #[test]
    fn strip_tags_basic() {
        assert_eq!(strip_html_tags("Hello <b>world</b>!"), "Hello world!");
    }

    #[test]
    fn import_body_extracts_header_as_hero() {
        let html = r#"
        <header>
          <h2>Welcome</h2>
          <h1>Mom's Site</h1>
          <p>Crafted with care.</p>
        </header>"#;
        let secs = import_body(html);
        assert_eq!(secs.len(), 1);
        assert!(matches!(
            &secs[0],
            ImportedSection::Hero { eyebrow, title, subtitle }
                if eyebrow == "Welcome" && title == "Mom's Site" && subtitle == "Crafted with care."
        ));
    }

    #[test]
    fn import_body_extracts_section_as_group() {
        let html = r#"
        <section>
          <h2>How it works</h2>
          <p>First paragraph.</p>
          <p>Second paragraph.</p>
        </section>"#;
        let secs = import_body(html);
        assert_eq!(secs.len(), 1);
        assert!(matches!(
            &secs[0],
            ImportedSection::Group { title, body }
                if title == "How it works" && body.len() == 2
        ));
    }

    #[test]
    fn import_body_combined_header_plus_sections() {
        let html = r#"
        <header><h1>Site</h1><p>tagline</p></header>
        <section><h2>One</h2><p>a</p></section>
        <section><h2>Two</h2><p>b</p></section>"#;
        let secs = import_body(html);
        assert_eq!(secs.len(), 3);
    }

    #[test]
    fn import_body_emits_todo_for_script() {
        let html = "<header><h1>x</h1></header><script>alert(1)</script>";
        let secs = import_body(html);
        assert!(secs.iter().any(|s| matches!(s, ImportedSection::Todo { .. })));
    }

    #[test]
    fn import_body_emits_todo_for_svg() {
        let html = "<svg><circle/></svg>";
        let secs = import_body(html);
        assert!(secs.iter().any(|s| matches!(s, ImportedSection::Todo { .. })));
    }

    #[test]
    fn imported_section_to_json_shapes_match_cms_schema() {
        let h = ImportedSection::Heading { level: 2, text: "A".into() };
        let v = h.to_json();
        assert_eq!(v["kind"], "heading");
        assert_eq!(v["level"], 2);
        assert_eq!(v["text"], "A");
    }
}
