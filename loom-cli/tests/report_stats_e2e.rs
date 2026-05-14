//! report_stats_e2e — integration test for the cycle 72
//! `loom report-stats` subcommand.
//!
//! Mirrors the cycle 70 report-tail test fixture pattern:
//! writes synthetic violations.jsonl + rotated siblings,
//! invokes the CLI, asserts on the aggregated output.

use std::path::PathBuf;
use std::process::Command;

fn fixture_dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "loom-report-stats-e2e-{}-{}-{}",
        std::process::id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(p.join("reports")).expect("mkdir fixture");
    p
}

fn write_lines(path: &std::path::Path, lines: &[&str]) {
    let body = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    std::fs::write(path, body).expect("write fixture");
}

fn run_stats(dir: &std::path::Path, extra: &[&str]) -> (i32, String) {
    let bin = env!("CARGO_BIN_EXE_loom");
    let mut cmd = Command::new(bin);
    cmd.arg("report-stats").arg("--dir").arg(dir);
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd.output().expect("spawn loom report-stats");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let status = out.status.code().unwrap_or(-1);
    (status, stdout)
}

#[test]
fn report_stats_aggregates_kinds_with_counts() {
    let f = fixture_dir("aggregate");
    let log = f.join("reports").join("violations.jsonl");
    write_lines(&log, &[
        // 3 csp-violation, 2 deprecation, 1 network-error.
        r#"{"ts":1700000001,"endpoint":"csp-report","body":"{\"csp-report\":{\"violated-directive\":\"script-src\",\"document-uri\":\"https://a.example/\"}}"}"#,
        r#"{"ts":1700000002,"endpoint":"csp-report","body":"{\"csp-report\":{\"violated-directive\":\"script-src\",\"document-uri\":\"https://a.example/\"}}"}"#,
        r#"{"ts":1700000003,"endpoint":"csp-report","body":"{\"csp-report\":{\"violated-directive\":\"img-src\",\"document-uri\":\"https://b.example/\"}}"}"#,
        r#"{"ts":1700000004,"endpoint":"reports","body":"[{\"type\":\"deprecation\",\"url\":\"https://c.example/\"}]"}"#,
        r#"{"ts":1700000005,"endpoint":"reports","body":"[{\"type\":\"deprecation\",\"url\":\"https://c.example/\"}]"}"#,
        r#"{"ts":1700000006,"endpoint":"reports","body":"[{\"type\":\"network-error\",\"url\":\"https://d.example/\"}]"}"#,
    ]);
    let (status, out) = run_stats(&f.join("reports"), &[]);
    assert_eq!(status, 0, "report-stats must exit 0; got {status}\n{out}");

    // Header present.
    assert!(out.contains("kind") && out.contains("count"),
        "header missing:\n{out}");
    // 3 csp-violation lines aggregated into one row.
    assert!(out.contains("csp-violation"),
        "expected csp-violation row:\n{out}");
    assert!(out.contains("    3"),
        "expected csp-violation count = 3:\n{out}");
    // 2 deprecation.
    assert!(out.contains("deprecation"),
        "expected deprecation row:\n{out}");
    // 1 network-error.
    assert!(out.contains("network-error"),
        "expected network-error row:\n{out}");
    // Top URL for csp-violation = https://a.example/ (2 hits)
    assert!(out.contains("https://a.example/"),
        "expected top URL https://a.example/ for csp-violation:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn report_stats_json_emits_one_object() {
    let f = fixture_dir("json");
    let log = f.join("reports").join("violations.jsonl");
    write_lines(&log, &[
        r#"{"ts":1700000001,"endpoint":"reports","body":"[{\"type\":\"deprecation\",\"url\":\"https://x.example/\"}]"}"#,
    ]);
    let (status, out) = run_stats(&f.join("reports"), &["--json"]);
    assert_eq!(status, 0);
    // Single-line JSON document.
    let trimmed = out.trim();
    assert!(trimmed.starts_with('{') && trimmed.ends_with('}'),
        "expected single JSON object:\n{out}");
    assert!(trimmed.contains("\"kinds\":["),
        "expected kinds array:\n{out}");
    assert!(trimmed.contains("\"window\":{"),
        "expected window object:\n{out}");
    assert!(trimmed.contains("\"kind\":\"deprecation\""),
        "expected kind=deprecation:\n{out}");
    assert!(trimmed.contains("\"top_url\":\"https://x.example/\""),
        "expected top_url:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn report_stats_reads_across_rotation_boundaries() {
    let f = fixture_dir("rotation");
    let reports = f.join("reports");
    // Rotated file: older entries.
    let rotated = reports.join("violations-1700000000.123456789.jsonl");
    write_lines(&rotated, &[
        r#"{"ts":1700000010,"endpoint":"reports","body":"[{\"type\":\"deprecation\",\"url\":\"https://old.example/\"}]"}"#,
        r#"{"ts":1700000020,"endpoint":"reports","body":"[{\"type\":\"deprecation\",\"url\":\"https://old.example/\"}]"}"#,
    ]);
    // Active file: newer entries.
    let active = reports.join("violations.jsonl");
    write_lines(&active, &[
        r#"{"ts":1700000100,"endpoint":"reports","body":"[{\"type\":\"deprecation\",\"url\":\"https://new.example/\"}]"}"#,
    ]);
    let (status, out) = run_stats(&reports, &[]);
    assert_eq!(status, 0);
    // 2 from rotated + 1 from active = 3 deprecation.
    assert!(out.contains("    3"),
        "expected aggregated count of 3 across rotation:\n{out}");
    // Files read should be 2.
    assert!(out.contains("read 2 file(s)"),
        "expected files_read=2 across rotation:\n{out}");
    // Top URL is whichever has more hits → old.example (2 vs 1).
    assert!(out.contains("https://old.example/"),
        "expected old.example as top URL:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn report_stats_since_filters_old_entries() {
    let f = fixture_dir("since");
    let log = f.join("reports").join("violations.jsonl");
    write_lines(&log, &[
        r#"{"ts":1700000000,"endpoint":"reports","body":"[{\"type\":\"deprecation\"}]"}"#,
        r#"{"ts":1700000050,"endpoint":"reports","body":"[{\"type\":\"deprecation\"}]"}"#,
        r#"{"ts":1700000100,"endpoint":"reports","body":"[{\"type\":\"deprecation\"}]"}"#,
    ]);
    // --since 1700000050 should keep entries 2 and 3 (ts >= 50).
    let (status, out) = run_stats(&f.join("reports"), &["--since", "1700000050"]);
    assert_eq!(status, 0);
    // Count should be 2, not 3.
    assert!(out.contains("    2"),
        "expected count=2 with --since filter:\n{out}");
    // And the since metadata in the footer.
    assert!(out.contains("since=1700000050"),
        "expected since footer:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn report_stats_empty_directory_prints_friendly_message() {
    let f = fixture_dir("empty");
    let (status, out) = run_stats(&f.join("reports"), &[]);
    assert_eq!(status, 0, "empty dir is not an error exit");
    assert!(out.contains("no entries match"),
        "expected friendly empty message:\n{out}");

    let _ = std::fs::remove_dir_all(&f);
}
