//! `loom audit-bridge` subcommand — verify the skin.css ships
//! every selector each `CmsSection` variant declares it needs.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3). Body kept byte-for-byte identical to the
//! pre-extraction implementation; visibility on `cmd_audit_bridge`
//! was widened from module-private to `pub` so the entrypoint can
//! call it through the `mod audit_bridge;` boundary.
//!
//! Doctrine note: the variant→selector table is hand-written rather
//! than derived from the bridge code on purpose. This audit catches
//! the case where the BRIDGE evolves but skin.css doesn't (or vice
//! versa). Both sides need a human to update the doctrine table
//! when adding a variant.

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
pub fn cmd_audit_bridge(skin: &std::path::Path) -> Result<u32, std::io::Error> {
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
        crate::test_support::unique_tmp("loom-audit-bridge", label).with_extension("css")
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
