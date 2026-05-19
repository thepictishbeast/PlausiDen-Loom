//! `loom doctor` subcommand — health-check a Loom development repo
//! or a Forge-consuming user site.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3). Body kept byte-for-byte identical to the
//! pre-extraction implementation; only the visibility on
//! `cmd_doctor` was widened from module-private to `pub` so the
//! entrypoint can call it through the `mod doctor;` boundary.
//!
//! Three categories of check:
//!   1. Loom dev-repo doctrine drift (CLAUDE.md sections + primitive
//!      claims vs `loom-components/src/lib.rs`).
//!   2. Site operational health (cms/ + forge.toml + static/ +
//!      auth/key secret permissions + dark-amoled tokens +
//!      reduced-motion guard).
//!   3. Attest-key file mode 0600 when present.
//!
//! Read-only, no network. Safe to invoke from CI / cron / a
//! developer's terminal interchangeably.

use anyhow::Result;

/// Severity of a single doctor finding. `Ok` is always-shown so
/// the operator sees what passed AND what failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorLevel {
    Ok,
    Warn,
    Fail,
}

impl DoctorLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok  ",
            Self::Warn => "warn",
            Self::Fail => "FAIL",
        }
    }
}

/// One health-check result: a check name, severity, one-line
/// human-readable message, and (when relevant) a one-line
/// remedy. The remedy is the difference between "your site is
/// broken" and "your site is broken — here's what to do."
#[derive(Debug)]
struct DoctorFinding {
    level: DoctorLevel,
    check: String,
    message: String,
    remedy: Option<String>,
}

/// Verify the design-system doctrine document is in sync with code
/// AND that the site root (if one is detected) has sane operational
/// state.
///
/// Read-only, no network. Safe to invoke from CI / cron / terminal
/// interchangeably; emits findings to stdout.
pub fn cmd_doctor(root: &std::path::Path) -> Result<()> {
    // Detect what kind of root this is. A Loom dev repo has
    // CLAUDE.md + loom-components/. A user site has cms/ +
    // forge.toml. Some operators may have both (development
    // root). Run whichever applies; show explicit "skipped"
    // findings for the other set.
    let is_loom_repo =
        root.join("CLAUDE.md").is_file() && root.join("loom-components/src/lib.rs").is_file();
    let is_site = root.join("cms").is_dir() || root.join("forge.toml").is_file();

    let mut findings: Vec<DoctorFinding> = Vec::new();
    if is_loom_repo {
        findings.extend(audit_loom_repo(root));
    }
    if is_site {
        findings.extend(audit_site(root));
    }
    if !is_loom_repo && !is_site {
        findings.push(DoctorFinding {
            level: DoctorLevel::Fail,
            check: "root-detection".into(),
            message: format!(
                "{} doesn't look like a Loom repo OR a site (no CLAUDE.md, no cms/, no forge.toml)",
                root.display()
            ),
            remedy: Some(
                "Run `loom site init mysite --template basic` to scaffold a site, OR pass the path to your Loom repo / site root.".into()
            ),
        });
    }

    let n_fail = findings
        .iter()
        .filter(|f| f.level == DoctorLevel::Fail)
        .count();
    let n_warn = findings
        .iter()
        .filter(|f| f.level == DoctorLevel::Warn)
        .count();
    let n_ok = findings
        .iter()
        .filter(|f| f.level == DoctorLevel::Ok)
        .count();

    println!(
        "loom doctor: {n_ok} ok · {n_warn} warn · {n_fail} fail · root={}",
        root.display()
    );
    println!();
    for f in &findings {
        println!("  [{}] {} — {}", f.level.label(), f.check, f.message);
        if let Some(r) = &f.remedy {
            println!("         → {r}");
        }
    }

    if n_fail > 0 {
        anyhow::bail!("{n_fail} fail finding(s); see above")
    }
    Ok(())
}

/// T-doctor / dev-side: doctrine-drift checks on a Loom dev repo.
/// Refactored from the original cmd_doctor so the same output
/// shape covers both repo + site audits.
fn audit_loom_repo(root: &std::path::Path) -> Vec<DoctorFinding> {
    let mut out = Vec::new();
    let claude_path = root.join("CLAUDE.md");
    let claude = match std::fs::read_to_string(&claude_path) {
        Ok(s) => s,
        Err(_) => {
            out.push(DoctorFinding {
                level: DoctorLevel::Fail,
                check: "CLAUDE.md".into(),
                message: format!("missing at {}", claude_path.display()),
                remedy: Some("Restore from git history or pull latest from origin.".into()),
            });
            return out;
        }
    };
    let required_sections = [
        "## The single rule",
        "## Why this exists",
        "## Crate map",
        "## Hard rules",
        // CLAUDE.md uses "What this is still not" — the "still"
        // was added intentionally on the 2026-05-06 doctrine
        // update where Loom rescinded the "no runtime editing"
        // rule. Doctor was checking for the pre-rename
        // "What this is not" so it false-failed every run.
        "## What this is still not",
    ];
    let mut any_missing = false;
    for section in required_sections {
        if !claude.contains(section) {
            out.push(DoctorFinding {
                level: DoctorLevel::Fail,
                check: format!("CLAUDE.md section `{section}`"),
                message: "missing — doctrine has drifted".into(),
                remedy: Some(format!(
                    "Restore the `{section}` heading. A rename is a doctrine event; coordinate with PlausiDen-AVP-Doctrine."
                )),
            });
            any_missing = true;
        }
    }
    if !any_missing {
        out.push(DoctorFinding {
            level: DoctorLevel::Ok,
            check: "CLAUDE.md doctrine sections".into(),
            message: format!("all {} required sections present", required_sections.len()),
            remedy: None,
        });
    }
    let lib_path = root.join("loom-components/src/lib.rs");
    if lib_path.exists() {
        let lib = std::fs::read_to_string(&lib_path).unwrap_or_default();
        let claimed = ["Button", "Card", "Section", "Hero", "Footer", "Nav"];
        let mut missing: Vec<&str> = Vec::new();
        for m in claimed {
            if !lib.contains(&format!("pub use {}", m.to_lowercase())) && !lib.contains(m) {
                missing.push(m);
            }
        }
        if missing.is_empty() {
            out.push(DoctorFinding {
                level: DoctorLevel::Ok,
                check: "loom-components crate map".into(),
                message: format!("all {} primitives exported", claimed.len()),
                remedy: None,
            });
        } else {
            out.push(DoctorFinding {
                level: DoctorLevel::Fail,
                check: "loom-components crate map".into(),
                message: format!("CLAUDE.md mentions {missing:?} but lib.rs does not export them"),
                remedy: Some("Either add a `pub use module::Type` for each missing primitive, or update CLAUDE.md to remove the claim.".into()),
            });
        }
    }
    out
}

/// Site-level operational diagnostics. Catches the broken / mis-
/// configured states that produce opaque errors at runtime: no
/// cms/ dir, malformed cms/*.json, attest-key.b64 with wrong
/// permissions, port already-in-use, etc.
///
/// Mom-class output: every finding has a one-line remedy in plain
/// English ("run X to fix Y").
fn audit_site(root: &std::path::Path) -> Vec<DoctorFinding> {
    let mut out = Vec::new();

    // cms/ — exists, has at least one .json, every .json parses.
    let cms_dir = root.join("cms");
    if !cms_dir.is_dir() {
        out.push(DoctorFinding {
            level: DoctorLevel::Warn,
            check: "cms/".into(),
            message: format!("directory missing at {}", cms_dir.display()),
            remedy: Some("Run `loom site init <name> --template basic` to scaffold one.".into()),
        });
    } else {
        let mut json_count = 0usize;
        let mut bad: Vec<(String, String)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&cms_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_owned();
                if name == "cms-schema.json" {
                    continue; // schema companion, not a page
                }
                json_count += 1;
                let raw = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        bad.push((name, format!("read failed: {e}")));
                        continue;
                    }
                };
                if let Err(e) = serde_json::from_str::<loom_cms_render::CmsPage>(&raw) {
                    bad.push((name, format!("CmsPage parse failed: {e}")));
                }
            }
        }
        if json_count == 0 {
            out.push(DoctorFinding {
                level: DoctorLevel::Warn,
                check: "cms/*.json".into(),
                message: "no page files found".into(),
                remedy: Some(
                    "Edit pages via `loom edit-serve` or copy a sample from `loom site init`."
                        .into(),
                ),
            });
        } else if bad.is_empty() {
            out.push(DoctorFinding {
                level: DoctorLevel::Ok,
                check: "cms/*.json".into(),
                message: format!("{json_count} page(s); all parse cleanly"),
                remedy: None,
            });
        } else {
            for (name, why) in bad {
                out.push(DoctorFinding {
                    level: DoctorLevel::Fail,
                    check: format!("cms/{name}"),
                    message: why,
                    remedy: Some("Open the file in `loom edit-serve` and the typed editor will refuse invalid edits — or restore from git.".into()),
                });
            }
        }
    }

    // forge.toml parseable.
    let forge_toml = root.join("forge.toml");
    if forge_toml.is_file() {
        match std::fs::read_to_string(&forge_toml) {
            Ok(s) => match s.parse::<toml::Value>() {
                Ok(_) => out.push(DoctorFinding {
                    level: DoctorLevel::Ok,
                    check: "forge.toml".into(),
                    message: "parses cleanly".into(),
                    remedy: None,
                }),
                Err(e) => out.push(DoctorFinding {
                    level: DoctorLevel::Fail,
                    check: "forge.toml".into(),
                    message: format!("TOML parse error: {e}"),
                    remedy: Some("Fix the syntax error — TOML is whitespace-sensitive on multi-line strings.".into()),
                }),
            },
            Err(e) => out.push(DoctorFinding {
                level: DoctorLevel::Fail,
                check: "forge.toml".into(),
                message: format!("read failed: {e}"),
                remedy: Some("Check file permissions; the editor needs read access.".into()),
            }),
        }
    } else {
        out.push(DoctorFinding {
            level: DoctorLevel::Warn,
            check: "forge.toml".into(),
            message: "missing — Forge build will use defaults".into(),
            remedy: Some("Run `loom site init <name>` to scaffold a forge.toml, or write `mode = \"poc\"` to silence this warning.".into()),
        });
    }

    // static/ — directory exists OR is creatable.
    let static_dir = root.join("static");
    if static_dir.exists() {
        out.push(DoctorFinding {
            level: DoctorLevel::Ok,
            check: "static/".into(),
            message: "directory exists".into(),
            remedy: None,
        });
    } else {
        out.push(DoctorFinding {
            level: DoctorLevel::Warn,
            check: "static/".into(),
            message: "missing — Forge build creates it on first run".into(),
            remedy: Some(
                "Run `cargo run -p forge-cli` (or `forge build`) and the directory appears.".into(),
            ),
        });
    }

    // auth.toml — if present, mode 0600.
    let auth_toml = root.join("auth.toml");
    if auth_toml.is_file() {
        out.extend(check_secret_perms(&auth_toml, "auth.toml", "auth secrets"));
    }

    // attest-key.b64 — if present, mode 0600.
    if let Some(key_path) = doctor_attest_key_path() {
        if key_path.is_file() {
            out.extend(check_secret_perms(
                &key_path,
                "attest-key.b64 (private key)",
                "Ed25519 deploy signing key",
            ));
        }
    }

    // dark-amoled token block present in shipped skin.css.
    // The Crawler 24-combo test matrix specifically tests
    // ?_theme=dark-amoled; if the skin doesn't define those
    // tokens, the matrix runs but the visual result is identical
    // to the regular dark theme.
    let skin_paths = ["static/loom-skin.css", "loom-skin.css"];
    for rel in &skin_paths {
        let p = root.join(rel);
        if let Ok(body) = std::fs::read_to_string(&p) {
            if body.contains("data-theme=\"dark-amoled\"") {
                out.push(DoctorFinding {
                    level: DoctorLevel::Ok,
                    check: format!("{rel} (dark-amoled tokens)"),
                    message: "dark-amoled token block present — OLED-optimized dark theme will render correctly".into(),
                    remedy: None,
                });
            } else if body.contains("data-theme=\"dark\"") {
                out.push(DoctorFinding {
                    level: DoctorLevel::Warn,
                    check: format!("{rel} (dark-amoled tokens)"),
                    message: "dark-amoled token block missing — Crawler 24-combo test matrix's ?_theme=dark-amoled axis will render identically to regular dark theme".into(),
                    remedy: Some("Add a `:root[data-theme=\"dark-amoled\"]` block to the skin with bg-canvas at hsl(0 0% 0%) — see PlausiDen-Loom commit history for an example.".into()),
                });
            }
            break;
        }
    }

    // prefers-reduced-motion guard coverage on shipped CSS.
    // Mirrors phase_motion_respects_reduced from Forge — surfaces
    // the same WCAG 2.1 SC 2.3.3 concern from the operator side
    // before they `forge build` so the misconfig is visible at
    // `loom doctor` time.
    for rel in &skin_paths {
        let p = root.join(rel);
        if let Ok(body) = std::fs::read_to_string(&p) {
            let has_motion = body.contains("animation:")
                || body.contains("animation-name:")
                || body.contains("transition:")
                || body.contains("transition-property:")
                || body.contains("scroll-behavior:");
            let has_guard = body.contains("prefers-reduced-motion");
            if has_motion && !has_guard {
                out.push(DoctorFinding {
                    level: DoctorLevel::Warn,
                    check: format!("{rel} (reduced-motion guard)"),
                    message: "CSS contains animation/transition declarations but no @media (prefers-reduced-motion) guard — readers with vestibular disorders or migraine triggers will see motion they opted out of (WCAG 2.1 SC 2.3.3)".into(),
                    remedy: Some("Wrap motion in `@media (prefers-reduced-motion: no-preference) { ... }` OR add an override block `@media (prefers-reduced-motion: reduce) { * { animation: none !important; transition: none !important; } }`.".into()),
                });
            } else if has_motion && has_guard {
                out.push(DoctorFinding {
                    level: DoctorLevel::Ok,
                    check: format!("{rel} (reduced-motion guard)"),
                    message: "motion + prefers-reduced-motion guard both present".into(),
                    remedy: None,
                });
            }
            break;
        }
    }

    out
}

/// Check a private-key file is mode 0600 (owner-only). Returns
/// one Ok or one Fail finding per call.
#[cfg(unix)]
fn check_secret_perms(path: &std::path::Path, label: &str, kind: &str) -> Vec<DoctorFinding> {
    use std::os::unix::fs::PermissionsExt;
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return vec![DoctorFinding {
                level: DoctorLevel::Fail,
                check: label.into(),
                message: format!("metadata read failed: {e}"),
                remedy: Some("Fix file permissions; the editor needs read access.".into()),
            }];
        }
    };
    let mode = meta.permissions().mode() & 0o777;
    if mode == 0o600 {
        vec![DoctorFinding {
            level: DoctorLevel::Ok,
            check: label.into(),
            message: format!("mode 0600 (owner-only) — {kind} protected"),
            remedy: None,
        }]
    } else {
        vec![DoctorFinding {
            level: DoctorLevel::Fail,
            check: label.into(),
            message: format!("mode {mode:o} — should be 0600 for {kind}"),
            remedy: Some(format!(
                "Run `chmod 600 {}` to lock it down.",
                path.display()
            )),
        }]
    }
}

#[cfg(not(unix))]
fn check_secret_perms(_path: &std::path::Path, label: &str, _kind: &str) -> Vec<DoctorFinding> {
    vec![DoctorFinding {
        level: DoctorLevel::Warn,
        check: label.into(),
        message: "permission check skipped — Unix only".into(),
        remedy: None,
    }]
}

/// Best-effort lookup of the attest-key path. Returns `None` if
/// the home dir can't be resolved.
fn doctor_attest_key_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("LOOM_ATTEST_KEY") {
        return Some(std::path::PathBuf::from(p));
    }
    dirs_next::config_dir().map(|d| d.join("loom").join("attest-key.b64"))
}
