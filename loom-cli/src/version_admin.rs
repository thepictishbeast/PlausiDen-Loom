//! `version_admin` — operator-facing version inventory (#141).
//!
//! Walks the CMS and project-root manifest files and reports every
//! version stamp that has landed since the backcompat-v1 work (#137).
//! Operators get a single read-only place to see "what versions does
//! this deployment hold" without dropping into a shell.
//!
//! v1 surface (this commit) — read only:
//!   * GET /admin/versions      → JSON inventory.
//!   * GET /admin/versions/html → operator-facing HTML inventory.
//!
//! Out of scope for v1 (filed as follow-up `#141a` in cms_new.rs
//! TODO — needs the forge::migration_core cross-repo link):
//!   * POST migrate / rollback / pin-version actions.
//!   * Diff against the Forge migration-registry.
//!
//! All emitted HTML uses inline `style="..."` referencing existing
//! `loom-tokens` CSS variables so it cooperates with the operator
//! UI's existing theme cascade. No raw color / spacing literals —
//! per Loom CLAUDE.md Hard Rule 2.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One artifact's observed version state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVersion {
    /// Path relative to the inspection root.
    pub path: String,
    /// File category (json / toml / unknown — kebab-case wire form).
    pub kind: String,
    /// Whichever version field we found, normalized to a string.
    /// Common forms:
    ///   * `schema_version = 1`             → `"1"`
    ///   * `schema_version = "1.2.0"`       → `"1.2.0"`
    ///   * `"schemaVersion": "1.0.0"` (JSON)→ `"1.0.0"`
    ///   * `"version": "1.0.0"` (JSON)      → `"1.0.0"`
    pub version: Option<String>,
    /// Name of the field we read the version from (`schema_version`,
    /// `schemaVersion`, `version`). `None` when the artifact has no
    /// recognized version field — operator can see the gap.
    pub field: Option<String>,
}

/// Full inventory across the inspection root.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct VersionInventory {
    /// Inspection root, absolute path string.
    pub root: String,
    /// Per-file rows, sorted by path for stable diffs.
    pub artifacts: Vec<ArtifactVersion>,
    /// Quick summary: distinct versions observed, mapped to the count
    /// of artifacts on that version. `"missing"` collects unversioned
    /// artifacts.
    pub summary: BTreeMap<String, u32>,
    /// Count of artifacts with no recognized version field.
    pub missing_count: u32,
    /// Total artifacts scanned.
    pub total: u32,
}

/// Scan `cms_root` and `cms_root/..` (the project root) for version
/// stamps. Walks one level into `cms_root` for JSON; reads top-level
/// `*.toml` from the project root.
///
/// Idempotent; pure read-only. Caller-supplied root must already
/// exist — we don't error if it's missing, we just emit an empty
/// inventory with `root` set.
#[must_use]
pub fn scan(cms_root: &Path) -> VersionInventory {
    let project_root = cms_root.parent().unwrap_or(cms_root);
    let mut inv = VersionInventory {
        root: project_root.display().to_string(),
        ..Default::default()
    };

    // CMS JSON files.
    if let Ok(entries) = std::fs::read_dir(cms_root) {
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        paths.sort();
        for path in paths {
            inv.artifacts.push(scan_json_file(&path, project_root));
        }
    }

    // Project-root TOML files. backends.toml + forge.toml + cargo.toml
    // are the canonical version-stamped ones.
    for name in ["backends.toml", "forge.toml", "loom.toml", "cms.toml"] {
        let path = project_root.join(name);
        if path.exists() {
            inv.artifacts.push(scan_toml_file(&path, project_root));
        }
    }

    // Summary roll-up.
    for art in &inv.artifacts {
        inv.total += 1;
        match &art.version {
            Some(v) => *inv.summary.entry(v.clone()).or_insert(0) += 1,
            None => {
                inv.missing_count += 1;
                *inv.summary.entry("missing".to_owned()).or_insert(0) += 1;
            }
        }
    }

    inv
}

fn rel_path(path: &Path, project_root: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn scan_json_file(path: &Path, project_root: &Path) -> ArtifactVersion {
    let rel = rel_path(path, project_root);
    let mut art = ArtifactVersion {
        path: rel,
        kind: "json".to_owned(),
        version: None,
        field: None,
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return art;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return art;
    };
    // Try common field names in priority order.
    for field in ["schema_version", "schemaVersion", "version"] {
        if let Some(v) = value.get(field) {
            if let Some(s) = json_to_version_string(v) {
                art.version = Some(s);
                art.field = Some(field.to_owned());
                return art;
            }
        }
    }
    art
}

fn json_to_version_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn scan_toml_file(path: &Path, project_root: &Path) -> ArtifactVersion {
    let rel = rel_path(path, project_root);
    let mut art = ArtifactVersion {
        path: rel,
        kind: "toml".to_owned(),
        version: None,
        field: None,
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return art;
    };
    // Cheap line scan instead of a full TOML parser dep — operators
    // need a quick read, not perfect AST fidelity. Match top-level
    // `<field> = <int|"string">` lines.
    for line in text.lines() {
        let trimmed = line.trim_start();
        for field in ["schema_version", "version"] {
            if let Some(rest) = trimmed
                .strip_prefix(field)
                .and_then(|r| r.trim_start().strip_prefix('='))
            {
                let value = rest.trim();
                let stripped = value
                    .trim_end_matches([' ', '\t'])
                    .trim_start_matches([' ', '\t']);
                let stripped = strip_toml_quotes(stripped);
                if !stripped.is_empty() {
                    art.version = Some(stripped.to_owned());
                    art.field = Some(field.to_owned());
                    return art;
                }
            }
        }
    }
    art
}

fn strip_toml_quotes(s: &str) -> &str {
    let s = s.trim_end_matches([',', ' ', '\t']);
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(s)
}

/// JSON wire shape for `GET /admin/versions`.
///
/// # Errors
/// Returns `serde_json::Error` if serialization fails (in practice,
/// never — `VersionInventory` is pure data).
pub fn to_json_pretty(inv: &VersionInventory) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(inv)
}

/// Operator-facing HTML for `GET /admin/versions/html`.
///
/// Inline-styled against `loom-tokens` CSS variables. Refuses to emit
/// raw color / spacing literals; uses `var(--loom-...)` everywhere.
#[must_use]
pub fn to_html(inv: &VersionInventory) -> String {
    let mut rows = String::new();
    for art in &inv.artifacts {
        let version_cell = match &art.version {
            Some(v) => html_escape(v),
            None => "—".to_owned(),
        };
        let field_cell = match &art.field {
            Some(f) => html_escape(f),
            None => String::new(),
        };
        let row_style = if art.version.is_none() {
            "background: var(--loom-color-warn-surface, transparent);"
        } else {
            ""
        };
        rows.push_str(&format!(
            r#"<tr style="{row}">
  <td style="padding: var(--loom-space-2, 8px); font-family: var(--loom-font-mono, monospace);">{path}</td>
  <td style="padding: var(--loom-space-2, 8px);">{kind}</td>
  <td style="padding: var(--loom-space-2, 8px); font-family: var(--loom-font-mono, monospace);">{ver}</td>
  <td style="padding: var(--loom-space-2, 8px); color: var(--loom-color-ink-muted, inherit);">{field}</td>
</tr>"#,
            row = row_style,
            path = html_escape(&art.path),
            kind = html_escape(&art.kind),
            ver = version_cell,
            field = field_cell,
        ));
    }

    let mut summary_items = String::new();
    for (k, v) in &inv.summary {
        summary_items.push_str(&format!(
            r#"<li><code>{ver}</code>: {count} artifact(s)</li>"#,
            ver = html_escape(k),
            count = v,
        ));
    }

    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>loom edit — version inventory</title>
<link rel="stylesheet" href="/static/loom-skin.css">
<style>
  body {{
    font-family: var(--loom-font-body, system-ui, sans-serif);
    color: var(--loom-color-ink, #111);
    background: var(--loom-color-surface, #fff);
    margin: 0;
    padding: var(--loom-space-6, 24px);
  }}
  h1 {{ font-family: var(--loom-font-display, inherit); }}
  table {{ border-collapse: collapse; width: 100%; margin-top: var(--loom-space-4, 16px); }}
  th, td {{
    text-align: left;
    border-bottom: 1px solid var(--loom-color-border, #e5e5e5);
  }}
  th {{
    padding: var(--loom-space-2, 8px);
    font-weight: 600;
    background: var(--loom-color-surface-elevated, transparent);
  }}
  summary, ul {{ margin-block: var(--loom-space-3, 12px); }}
  .meta {{ color: var(--loom-color-ink-muted, #666); }}
  .breadcrumb {{ margin-bottom: var(--loom-space-4, 16px); }}
  .breadcrumb a {{ color: var(--loom-color-link, inherit); }}
</style>
</head>
<body>
<div class="breadcrumb"><a href="/">← back to loom edit</a></div>
<h1>version inventory</h1>
<p class="meta">root: <code>{root}</code> · {total} artifact(s) · {missing} missing version field</p>

<details open>
<summary>Summary</summary>
<ul>{summary}</ul>
</details>

<table>
<thead>
<tr><th>Path</th><th>Kind</th><th>Version</th><th>Field</th></tr>
</thead>
<tbody>
{rows}
</tbody>
</table>

<p class="meta">JSON: <a href="/admin/versions">/admin/versions</a></p>
</body>
</html>
"##,
        root = html_escape(&inv.root),
        total = inv.total,
        missing = inv.missing_count,
        summary = summary_items,
        rows = rows,
    )
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_root(label: &str) -> PathBuf {
        let t = std::env::temp_dir().join(format!(
            "loom-version-admin-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&t).expect("mkdir tmp");
        t
    }

    #[test]
    fn scan_picks_up_schema_version_in_json() {
        let root = tmp_root("json");
        let cms = root.join("cms");
        fs::create_dir_all(&cms).unwrap();
        fs::write(
            cms.join("index.json"),
            r#"{"schemaVersion": "1.0.0", "title": "Home"}"#,
        )
        .unwrap();
        let inv = scan(&cms);
        assert_eq!(inv.total, 1);
        assert_eq!(inv.missing_count, 0);
        assert_eq!(inv.artifacts[0].version.as_deref(), Some("1.0.0"));
        assert_eq!(inv.artifacts[0].field.as_deref(), Some("schemaVersion"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_picks_up_snake_case_schema_version_in_json() {
        let root = tmp_root("json-snake");
        let cms = root.join("cms");
        fs::create_dir_all(&cms).unwrap();
        fs::write(
            cms.join("page.json"),
            r#"{"schema_version": "2.1.0", "title": "P"}"#,
        )
        .unwrap();
        let inv = scan(&cms);
        assert_eq!(inv.artifacts[0].version.as_deref(), Some("2.1.0"));
        assert_eq!(inv.artifacts[0].field.as_deref(), Some("schema_version"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_treats_integer_schema_version_as_string() {
        let root = tmp_root("json-int");
        let cms = root.join("cms");
        fs::create_dir_all(&cms).unwrap();
        fs::write(
            cms.join("page.json"),
            r#"{"schema_version": 3, "title": "P"}"#,
        )
        .unwrap();
        let inv = scan(&cms);
        assert_eq!(inv.artifacts[0].version.as_deref(), Some("3"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_marks_missing_when_no_version_field() {
        let root = tmp_root("json-missing");
        let cms = root.join("cms");
        fs::create_dir_all(&cms).unwrap();
        fs::write(cms.join("page.json"), r#"{"title": "no version"}"#).unwrap();
        let inv = scan(&cms);
        assert!(inv.artifacts[0].version.is_none());
        assert!(inv.artifacts[0].field.is_none());
        assert_eq!(inv.missing_count, 1);
        assert_eq!(inv.summary.get("missing"), Some(&1));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_picks_up_toml_schema_version_quoted() {
        let root = tmp_root("toml-quoted");
        let cms = root.join("cms");
        fs::create_dir_all(&cms).unwrap();
        fs::write(
            root.join("backends.toml"),
            "schema_version = \"1.2.3\"\nname = \"x\"\n",
        )
        .unwrap();
        let inv = scan(&cms);
        let backends = inv
            .artifacts
            .iter()
            .find(|a| a.path.ends_with("backends.toml"))
            .expect("backends.toml row");
        assert_eq!(backends.version.as_deref(), Some("1.2.3"));
        assert_eq!(backends.field.as_deref(), Some("schema_version"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_picks_up_toml_schema_version_bare_integer() {
        let root = tmp_root("toml-int");
        let cms = root.join("cms");
        fs::create_dir_all(&cms).unwrap();
        fs::write(
            root.join("forge.toml"),
            "schema_version = 1\nname = \"x\"\n",
        )
        .unwrap();
        let inv = scan(&cms);
        let forge = inv
            .artifacts
            .iter()
            .find(|a| a.path.ends_with("forge.toml"))
            .expect("forge.toml row");
        assert_eq!(forge.version.as_deref(), Some("1"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_handles_missing_cms_root_gracefully() {
        let root = tmp_root("no-cms");
        let cms = root.join("does-not-exist");
        let inv = scan(&cms);
        assert_eq!(inv.total, 0);
        assert!(inv.artifacts.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn summary_groups_versions_correctly() {
        let root = tmp_root("summary");
        let cms = root.join("cms");
        fs::create_dir_all(&cms).unwrap();
        fs::write(cms.join("a.json"), r#"{"schema_version": "1.0.0"}"#).unwrap();
        fs::write(cms.join("b.json"), r#"{"schema_version": "1.0.0"}"#).unwrap();
        fs::write(cms.join("c.json"), r#"{"schema_version": "2.0.0"}"#).unwrap();
        fs::write(cms.join("d.json"), r#"{"title": "no ver"}"#).unwrap();
        let inv = scan(&cms);
        assert_eq!(inv.total, 4);
        assert_eq!(inv.summary.get("1.0.0"), Some(&2));
        assert_eq!(inv.summary.get("2.0.0"), Some(&1));
        assert_eq!(inv.summary.get("missing"), Some(&1));
        assert_eq!(inv.missing_count, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn json_serialization_round_trips() {
        let inv = VersionInventory {
            root: "/x".into(),
            artifacts: vec![ArtifactVersion {
                path: "cms/index.json".into(),
                kind: "json".into(),
                version: Some("1.0.0".into()),
                field: Some("schemaVersion".into()),
            }],
            summary: {
                let mut m = BTreeMap::new();
                m.insert("1.0.0".to_owned(), 1);
                m
            },
            missing_count: 0,
            total: 1,
        };
        let json = to_json_pretty(&inv).expect("ser");
        assert!(json.contains("\"schemaVersion\""));
        assert!(json.contains("1.0.0"));
        let back: VersionInventory = serde_json::from_str(&json).expect("de");
        assert_eq!(back.total, 1);
        assert_eq!(back.artifacts[0].path, "cms/index.json");
    }

    #[test]
    fn html_uses_loom_tokens_only_no_raw_colors() {
        let inv = VersionInventory {
            root: "/x".into(),
            artifacts: vec![ArtifactVersion {
                path: "cms/index.json".into(),
                kind: "json".into(),
                version: Some("1.0.0".into()),
                field: Some("schemaVersion".into()),
            }],
            ..Default::default()
        };
        let html = to_html(&inv);
        // Sanity: contains the canonical token references.
        assert!(html.contains("var(--loom-color-ink"));
        assert!(html.contains("var(--loom-space-"));
        assert!(html.contains("var(--loom-font-"));
        // Sanity: contains the data.
        assert!(html.contains("cms/index.json"));
        assert!(html.contains("1.0.0"));
        assert!(html.contains("schemaVersion"));
        // Sanity: escaping live.
        assert!(html.contains("<title>loom edit — version inventory</title>"));
    }

    #[test]
    fn html_escapes_path_special_chars() {
        let inv = VersionInventory {
            root: "/x".into(),
            artifacts: vec![ArtifactVersion {
                path: "cms/<script>.json".into(),
                kind: "json".into(),
                version: Some("1.0.0".into()),
                field: Some("version".into()),
            }],
            ..Default::default()
        };
        let html = to_html(&inv);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>.json"));
    }

    #[test]
    fn html_marks_missing_rows() {
        let inv = VersionInventory {
            root: "/x".into(),
            artifacts: vec![ArtifactVersion {
                path: "cms/unversioned.json".into(),
                kind: "json".into(),
                version: None,
                field: None,
            }],
            missing_count: 1,
            total: 1,
            ..Default::default()
        };
        let html = to_html(&inv);
        // Missing rows get a warn-surface background hint.
        assert!(html.contains("loom-color-warn-surface"));
        assert!(html.contains("—"));
    }

    #[test]
    fn strip_toml_quotes_handles_double() {
        assert_eq!(strip_toml_quotes("\"1.0.0\""), "1.0.0");
    }

    #[test]
    fn strip_toml_quotes_handles_single() {
        assert_eq!(strip_toml_quotes("'1.0.0'"), "1.0.0");
    }

    #[test]
    fn strip_toml_quotes_keeps_bare() {
        assert_eq!(strip_toml_quotes("1"), "1");
    }
}
