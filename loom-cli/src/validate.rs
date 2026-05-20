//! `loom validate` subcommand — schema + URL safety validation for
//! `CmsPage` JSON files (single file or whole directory tree).
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3). Body kept byte-for-byte identical to the
//! pre-extraction implementation; only the visibility on
//! `cmd_validate` was widened from module-private to `pub` so the
//! entrypoint can call it through the `mod validate;` boundary.
//!
//! Two passes per file:
//!   1. serde deserialize against `CmsPage` — catches missing
//!      fields, typos (`deny_unknown_fields`), wrong types, wrong
//!      enum tags.
//!   2. URL-validity walk — every href / cta.href / avatar.src /
//!      submit_endpoint / nav-link href / form action / panel
//!      list-item href / card href passes through
//!      `loom_cms_render::is_safe_url`.

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
pub fn cmd_validate(input: &std::path::Path) -> Result<bool, std::io::Error> {
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
