//! revisions_e2e — integration test for cycle 81's
//! `loom revisions` subcommand.
//!
//! Builds a fixture cms/ with hand-written backups, invokes
//! the CLI, asserts on list / show / diff / restore behaviour.
//! Pure stdlib.

use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "loom-revisions-e2e-{}-{}-{}",
        std::process::id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(p.join("cms")).expect("mkdir fixture");
    p
}

fn run(cms: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let bin = env!("CARGO_BIN_EXE_loom");
    let mut cmd = Command::new(bin);
    cmd.arg("revisions");
    cmd.args(args);
    cmd.arg("--cms").arg(cms);
    let out = cmd.output().expect("spawn loom");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let status = out.status.code().unwrap_or(-1);
    (status, stdout, stderr)
}

fn write(p: &std::path::Path, body: &str) {
    std::fs::write(p, body).expect("write fixture");
}

#[test]
fn revisions_list_orders_newest_first_with_human_timestamps() {
    let f = fixture("list");
    let cms = f.join("cms");
    write(&cms.join("home.json"), r#"{"title":"active","sections":[]}"#);
    // Synthetic backups with three different unix timestamps.
    write(&cms.join("home.bak.1700000000.000000001.json"), r#"{"title":"oldest"}"#);
    write(&cms.join("home.bak.1700000050.000000002.json"), r#"{"title":"middle"}"#);
    write(&cms.join("home.bak.1700000100.000000003.json"), r#"{"title":"newest"}"#);

    let (status, out, _) = run(&cms, &["list", "home"]);
    assert_eq!(status, 0, "list must exit 0:\n{out}");
    // Newest first: line 1 of the table (after header) should
    // be the 1700000100 entry.
    let lines: Vec<&str> = out.lines().collect();
    let first_row = lines.iter().find(|l| l.trim_start().starts_with("1  ")).expect("row 1");
    assert!(first_row.contains("home.bak.1700000100"),
        "row 1 should be newest:\n{out}");
    // Timestamp formatted: 2023-11-14 22:14:00Z is the human
    // form of 1700000100. Confirm format.
    assert!(first_row.contains("2023-11-14"),
        "expected human timestamp for 1700000100:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_show_prints_content_of_indexed_revision() {
    let f = fixture("show");
    let cms = f.join("cms");
    write(&cms.join("home.json"), r#"{"title":"active"}"#);
    write(&cms.join("home.bak.1700000100.0.json"), r#"{"title":"NEWER"}"#);
    write(&cms.join("home.bak.1700000050.0.json"), r#"{"title":"OLDER"}"#);

    // Index 1 = newest.
    let (status, out, _) = run(&cms, &["show", "home", "1"]);
    assert_eq!(status, 0);
    assert!(out.contains("\"title\":\"NEWER\""), "rev 1 should be NEWER:\n{out}");
    // Index 2 = older.
    let (_, out2, _) = run(&cms, &["show", "home", "2"]);
    assert!(out2.contains("\"title\":\"OLDER\""), "rev 2 should be OLDER:\n{out2}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_diff_shows_minus_revision_plus_active() {
    let f = fixture("diff");
    let cms = f.join("cms");
    write(&cms.join("home.json"), "active line A\nshared line\nactive line C\n");
    write(&cms.join("home.bak.1700000100.0.json"),
        "rev line A\nshared line\nrev line C\n");

    let (status, out, _) = run(&cms, &["diff", "home", "1"]);
    assert_eq!(status, 0);
    assert!(out.contains("--- ") && out.contains("(revision 1)"),
        "expected unified-diff header:\n{out}");
    assert!(out.contains("+++ ") && out.contains("(active)"),
        "expected unified-diff +++ line:\n{out}");
    assert!(out.contains("-rev line A"),
        "expected `-rev line A`:\n{out}");
    assert!(out.contains("+active line A"),
        "expected `+active line A`:\n{out}");
    // shared line MUST NOT appear (it's in both).
    assert!(!out.lines().any(|l| l == "-shared line" || l == "+shared line"),
        "shared line shouldn't appear with diff markers:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_diff_json_aware_emits_path_keyed_output() {
    // T76 cycle 87: when both files parse as JSON, the diff
    // walks them structurally and emits one line per
    // field-level difference, keyed by JSON-pointer path.
    let f = fixture("diff-json");
    let cms = f.join("cms");
    write(&cms.join("home.json"), r#"{
  "title": "Active title",
  "sections": [
    {"kind": "paragraph", "text": "B"},
    {"kind": "paragraph", "text": "C"}
  ]
}"#);
    write(&cms.join("home.bak.1700000100.0.json"), r#"{
  "title": "Revision title",
  "sections": [
    {"kind": "paragraph", "text": "A"},
    {"kind": "paragraph", "text": "B"}
  ]
}"#);

    let (status, out, _) = run(&cms, &["diff", "home", "1"]);
    assert_eq!(status, 0);
    // Title diff present with path keys.
    assert!(out.contains("- /title: \"Revision title\""),
        "expected `- /title: \"Revision title\"`:\n{out}");
    assert!(out.contains("+ /title: \"Active title\""),
        "expected `+ /title: \"Active title\"`:\n{out}");
    // Section text changes are path-keyed.
    assert!(out.contains("/sections/0/text"),
        "expected /sections/0/text in output:\n{out}");
    // No "(no semantic differences)" footer when diffs exist.
    assert!(!out.contains("no semantic differences"),
        "should not claim no-diff when diffs exist:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_diff_json_aware_identical_files_emits_no_diff() {
    let f = fixture("diff-identical");
    let cms = f.join("cms");
    let content = r#"{"title":"same","sections":[]}"#;
    write(&cms.join("home.json"), content);
    write(&cms.join("home.bak.1700000100.0.json"), content);

    let (status, out, _) = run(&cms, &["diff", "home", "1"]);
    assert_eq!(status, 0);
    assert!(out.contains("no semantic differences"),
        "identical files should report no semantic differences:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_restore_replaces_active_and_snapshots_prior() {
    let f = fixture("restore");
    let cms = f.join("cms");
    write(&cms.join("home.json"), r#"{"title":"ACTIVE-BEFORE-RESTORE"}"#);
    write(&cms.join("home.bak.1700000100.0.json"), r#"{"title":"RESTORED"}"#);

    // Restore rev 1.
    let (status, out, _) = run(&cms, &["restore", "home", "1"]);
    assert_eq!(status, 0, "restore exit 0:\n{out}");
    assert!(out.contains("restored from revision 1"),
        "expected restore confirmation:\n{out}");

    // Active file now has the RESTORED content.
    let active = std::fs::read_to_string(cms.join("home.json")).expect("read active");
    assert!(active.contains("RESTORED"),
        "active should be RESTORED:\n{active}");

    // A NEW backup exists holding the ACTIVE-BEFORE-RESTORE
    // content (so the restore is itself reversible).
    let entries: Vec<String> = std::fs::read_dir(&cms)
        .expect("readdir")
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    let bak_count = entries.iter()
        .filter(|n| n.starts_with("home.bak.") && n.ends_with(".json"))
        .count();
    assert!(bak_count >= 2, "should have ≥2 backups now: {entries:?}");
    // Find a backup that contains ACTIVE-BEFORE-RESTORE.
    let mut found_active_snapshot = false;
    for name in &entries {
        if name.starts_with("home.bak.") {
            if let Ok(c) = std::fs::read_to_string(cms.join(name)) {
                if c.contains("ACTIVE-BEFORE-RESTORE") {
                    found_active_snapshot = true;
                    break;
                }
            }
        }
    }
    assert!(found_active_snapshot,
        "active content from before restore should be backed up:\n{entries:?}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_pick_out_of_range_errors() {
    let f = fixture("range");
    let cms = f.join("cms");
    write(&cms.join("home.json"), "{}");
    write(&cms.join("home.bak.1700000100.0.json"), "{}");

    let (status, _, stderr) = run(&cms, &["show", "home", "99"]);
    assert_ne!(status, 0, "out-of-range must exit nonzero");
    assert!(stderr.contains("out of range"),
        "expected helpful error:\n{stderr}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_list_all_slugs_aggregates_across_pages() {
    // T76 cycle 84: --all-slugs walks the cms_root for ALL
    // *.bak.<unix>.<nanos>.json files and prints them
    // newest-first regardless of slug.
    let f = fixture("all-slugs");
    let cms = f.join("cms");
    write(&cms.join("home.json"), "{}");
    write(&cms.join("about.json"), "{}");
    // Synthetic backups across two slugs with three different
    // timestamps. Newest-first order must interleave correctly.
    write(&cms.join("home.bak.1700000300.0.json"), "home rev (newest)");
    write(&cms.join("about.bak.1700000200.0.json"), "about rev (middle)");
    write(&cms.join("home.bak.1700000100.0.json"), "home rev (oldest)");

    // Note: --all-slugs ignores the slug arg but clap requires
    // the positional. Pass an empty string by quoting.
    let bin = env!("CARGO_BIN_EXE_loom");
    let out = std::process::Command::new(bin)
        .args(["revisions", "list", "--cms"])
        .arg(&cms)
        .args(["--all-slugs", "--lines", "10"])
        .output()
        .expect("spawn");
    assert!(out.status.success(),
        "all-slugs should exit 0:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // All 3 should appear with their slug column.
    assert!(stdout.contains("home.bak.1700000300"),
        "newest home rev missing:\n{stdout}");
    assert!(stdout.contains("about.bak.1700000200"),
        "about rev missing:\n{stdout}");
    assert!(stdout.contains("home.bak.1700000100"),
        "oldest home rev missing:\n{stdout}");
    // Header includes a `slug` column.
    assert!(stdout.contains("slug"),
        "header should include slug column:\n{stdout}");
    // Newest-first ordering: the line containing 1700000300
    // should appear BEFORE the line containing 1700000200 in
    // the output.
    let idx_300 = stdout.find("1700000300").expect("idx 300");
    let idx_200 = stdout.find("1700000200").expect("idx 200");
    let idx_100 = stdout.find("1700000100").expect("idx 100");
    assert!(idx_300 < idx_200 && idx_200 < idx_100,
        "rows must be newest-first:\n{stdout}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_list_all_slugs_lines_caps_output() {
    let f = fixture("all-slugs-cap");
    let cms = f.join("cms");
    write(&cms.join("home.json"), "{}");
    // Write 8 backups; ask for top 3.
    for i in 0..8 {
        write(
            &cms.join(format!("home.bak.{}.0.json", 1700000000 + i)),
            "x",
        );
    }
    let bin = env!("CARGO_BIN_EXE_loom");
    let out = std::process::Command::new(bin)
        .args(["revisions", "list", "--cms"])
        .arg(&cms)
        .args(["--all-slugs", "--lines", "3"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should contain the 3 newest (1700000005, 1700000006, 1700000007)
    // and a "showing 3 of 8" hint.
    assert!(stdout.contains("1700000007"),
        "newest must appear:\n{stdout}");
    assert!(stdout.contains("showing 3 of 8"),
        "expected `showing N of M` hint:\n{stdout}");
    // Should NOT contain the 5 oldest.
    for old_ts in [1700000000, 1700000001, 1700000002, 1700000003, 1700000004] {
        assert!(!stdout.contains(&format!("{}", old_ts)),
            "old rev {old_ts} should not appear:\n{stdout}");
    }

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn revisions_list_empty_slug_prints_friendly_message() {
    let f = fixture("empty");
    let cms = f.join("cms");
    write(&cms.join("home.json"), "{}");
    // No backups.

    let (status, out, _) = run(&cms, &["list", "home"]);
    assert_eq!(status, 0, "empty list is not an error");
    assert!(out.contains("no backups for 'home'"),
        "expected friendly message:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}
