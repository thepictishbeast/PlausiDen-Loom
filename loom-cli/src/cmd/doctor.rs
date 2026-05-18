use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorLevel {
    Ok,
    Warn,
    Fail,
}

impl DoctorLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "  ok  ",
            Self::Warn => " warn ",
            Self::Fail => " FAIL ",
        }
    }
}

pub struct DoctorFinding {
    pub level: DoctorLevel,
    pub check: String,
    pub message: String,
    pub remedy: Option<String>,
}

pub fn cmd_doctor(root: &Path) -> Result<()> {
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
                r"Run `loom site init mysite --template basic` to scaffold a site, OR pass the path to your Loom repo / site root.".into()
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

    tracing::info!(
        "loom doctor: {n_ok} ok · {n_warn} warn · {n_fail} fail · root={}",
        root.display()
    );
    tracing::info!(" ");
    for f in &findings {
        tracing::info!("  [{}] {} — {}", f.level.clone().label(), f.check, f.message);
        if let Some(r) = &f.remedy {
            tracing::info!("         → {r}");
        }
    }

    if n_fail > 0 {
        anyhow::bail!("{n_fail} fail finding(s); see above")
    }
    Ok(())
}

fn audit_loom_repo(root: &Path) -> Vec<DoctorFinding> {
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
        "## What this is not",
    ];
    let mut any_missing = false;
    for section in required_sections {
        if !claude.contains(section) {
            out.push(DoctorFinding {
                level: DoctorLevel::Fail,
                check: format!("CLAUDE.md section `{section}`"),
                message: "missing — doctrine has drifted".into(),
                remedy: Some(format!(
                    r"Restore the `{section}` heading. A rename is a doctrine event; coordinate with PlausiDen-AVP-Doctrine."
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
                remedy: Some(r"Either add a `pub use module::Type` for each missing primitive, or update CLAUDE.md to remove the claim.".into()),
            });
        }
    }
    out
}

fn audit_site(root: &Path) -> Vec<DoctorFinding> {
    let mut out = Vec::new();

    // cms/ — exists, has at least one .json, every .json parses.
    let cms_dir = root.join("cms");
    if !cms_dir.is_dir() {
        out.push(DoctorFinding {
            level: DoctorLevel::Warn,
            check: "cms/".into(),
            message: format!("directory missing at {}", cms_dir.display()),
            remedy: Some(r"Run `loom site init <name> --template basic` to scaffold one.".into()),
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
                    r"Edit pages via `loom edit-serve` or copy a sample from `loom site init`."
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
                    remedy: Some(r"Open the file in `loom edit-serve` and the typed editor will refuse invalid edits — or restore from git.".into()),
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
            remedy: Some(r#"Run `loom site init <name>` to scaffold a forge.toml, or write `mode = "poc"` to silence this warning."#.into()),
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
                r"Run `cargo run -p forge-cli` (or `forge build`) and the directory appears.".into(),
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
    let skin_paths = ["static/loom-skin.css", "loom-skin.css"];
    for rel in &skin_paths {
        let p = root.join(rel);
        if let Ok(body) = std::fs::read_to_string(&p) {
            if body.contains(r#"data-theme="dark-amoled""#) {
                out.push(DoctorFinding {
                    level: DoctorLevel::Ok,
                    check: format!("{rel} (dark-amoled tokens)"),
                    message: "dark-amoled token block present — OLED-optimized dark theme will render correctly".into(),
                    remedy: None,
                });
            } else if body.contains(r#"data-theme="dark""#) {
                out.push(DoctorFinding {
                    level: DoctorLevel::Warn,
                    check: format!("{rel} (dark-amoled tokens)"),
                    message: "dark-amoled token block missing — Crawler 24-combo test matrix's ?_theme=dark-amoled axis will render identically to regular dark theme".into(),
                    remedy: Some(r#"Add a `:root[data-theme="dark-amoled"]` block to the skin with bg-canvas at hsl(0 0% 0%) — see PlausiDen-Loom commit history for an example."#.into()),
                });
            }
            break;
        }
    }
    out
}

#[cfg(unix)]
fn check_secret_perms(path: &Path, label: &str, kind: &str) -> Vec<DoctorFinding> {
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
                r"Run `chmod 600 {}` to lock it down.",
                path.display()
            )),
        }]
    }
}

#[cfg(not(unix))]
fn check_secret_perms(_path: &Path, label: &str, _kind: &str) -> Vec<DoctorFinding> {
    vec![DoctorFinding {
        level: DoctorLevel::Warn,
        check: label.into(),
        message: "permission check skipped — Unix only".into(),
        remedy: None,
    }]
}

fn doctor_attest_key_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LOOM_ATTEST_KEY") {
        return Some(PathBuf::from(p));
    }
    dirs_next::config_dir().map(|d| d.join("loom").join("attest-key.b64"))
}
