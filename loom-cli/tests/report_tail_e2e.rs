//! report_tail_e2e — integration test for the cycle 70
//! `loom report-tail` subcommand.
//!
//! Writes a synthetic violations.jsonl with known timestamps
//! + report types, then invokes the CLI against it, and
//! asserts the rendered output:
//!   - timestamps are formatted YYYY-MM-DD HH:MM:SSZ
//!   - report kinds are correctly classified (csp-violation /
//!     trusted-types / nel from the body type field, or via
//!     the legacy `violated-directive` heuristic)
//!   - --lines truncates correctly
//!   - --kind substring filters
//!
//! Pure stdlib. No external HTTP / TTY emulation.

use std::path::PathBuf;
use std::process::Command;

fn fixture_dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "loom-report-tail-e2e-{}-{}-{}",
        std::process::id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    let reports = p.join("reports");
    std::fs::create_dir_all(&reports).expect("mkdir fixture");
    p
}

fn write_jsonl(path: &std::path::Path, lines: &[&str]) {
    let body = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    std::fs::write(path, body).expect("write fixture");
}

fn run_tail(dir: &std::path::Path, extra: &[&str]) -> (i32, String) {
    let bin = env!("CARGO_BIN_EXE_loom");
    let mut cmd = Command::new(bin);
    cmd.arg("report-tail").arg("--dir").arg(dir);
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd.output().expect("spawn loom report-tail");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let status = out.status.code().unwrap_or(-1);
    (status, stdout)
}

#[test]
fn report_tail_prints_classified_lines_with_human_timestamp() {
    let f = fixture_dir("classify");
    let log = f.join("reports").join("violations.jsonl");
    // 1736380800 = 2025-01-09 00:00:00 UTC (chosen as a
    // round-number anchor; the date math doesn't depend on
    // a specific epoch alignment but a sanity-check helps).
    write_jsonl(
        &log,
        &[
            r#"{"ts":1736380800,"endpoint":"csp-report","content_type":"application/csp-report","body":"{\"csp-report\":{\"violated-directive\":\"script-src\"}}"}"#,
            r#"{"ts":1736380900,"endpoint":"reports","content_type":"application/reports+json","body":"[{\"type\":\"deprecation\",\"body\":{\"id\":\"X\"}}]"}"#,
            r#"{"ts":1736381000,"endpoint":"reports","content_type":"application/reports+json","body":"[{\"type\":\"network-error\",\"body\":{\"phase\":\"connection\"}}]"}"#,
        ],
    );
    let (status, out) = run_tail(&f.join("reports"), &[]);
    assert_eq!(status, 0, "report-tail must exit 0; got {status}\n{out}");

    // Timestamp: 1736380800 = 2025-01-09 00:00:00 UTC. Verify
    // the human format appears in the output.
    assert!(
        out.contains("2025-01-09 00:00:00Z"),
        "expected human timestamp in output:\n{out}"
    );

    // Classification: legacy csp-report → csp-violation,
    // deprecation type → deprecation, network-error → network-error.
    assert!(
        out.contains("csp-violation"),
        "expected csp-violation classification:\n{out}"
    );
    assert!(
        out.contains("deprecation"),
        "expected deprecation classification:\n{out}"
    );
    assert!(
        out.contains("network-error"),
        "expected network-error classification:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn report_tail_lines_flag_truncates_old_entries() {
    let f = fixture_dir("lines");
    let log = f.join("reports").join("violations.jsonl");
    // 10 entries; ask for last 3.
    let lines: Vec<String> = (0..10).map(|i| {
        format!(
            r#"{{"ts":{},"endpoint":"csp-report","content_type":"application/csp-report","body":"line-{i}"}}"#,
            1700000000 + i,
        )
    }).collect();
    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_jsonl(&log, &line_refs);
    let (status, out) = run_tail(&f.join("reports"), &["--lines", "3"]);
    assert_eq!(status, 0);

    // Should contain the LAST 3 (lines 7, 8, 9) but not 0..=6.
    for keep in 7..=9 {
        assert!(
            out.contains(&format!("line-{keep}")),
            "expected last 3 entries (line-{keep}):\n{out}"
        );
    }
    for drop in 0..=6 {
        assert!(
            !out.contains(&format!("line-{drop}\"")),
            "did not expect old entry (line-{drop}):\n{out}"
        );
    }

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn report_tail_kind_filter_drops_non_matching() {
    let f = fixture_dir("kind");
    let log = f.join("reports").join("violations.jsonl");
    write_jsonl(
        &log,
        &[
            r#"{"ts":1700000000,"endpoint":"reports","content_type":"application/reports+json","body":"[{\"type\":\"deprecation\"}]"}"#,
            r#"{"ts":1700000001,"endpoint":"reports","content_type":"application/reports+json","body":"[{\"type\":\"csp-violation\"}]"}"#,
            r#"{"ts":1700000002,"endpoint":"reports","content_type":"application/reports+json","body":"[{\"type\":\"network-error\"}]"}"#,
        ],
    );
    let (status, out) = run_tail(&f.join("reports"), &["--kind", "csp-violation"]);
    assert_eq!(status, 0);
    assert!(
        out.contains("csp-violation"),
        "expected csp-violation line in filtered output:\n{out}"
    );
    assert!(
        !out.contains("deprecation"),
        "did not expect deprecation in filtered output:\n{out}"
    );
    assert!(
        !out.contains("network-error"),
        "did not expect network-error in filtered output:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&f);
}

#[test]
fn report_tail_missing_file_no_follow_emits_friendly_notice() {
    let f = fixture_dir("missing");
    // No violations.jsonl written.
    let (status, out) = run_tail(&f.join("reports"), &[]);
    assert_eq!(status, 0, "missing file should NOT be an error exit");
    assert!(
        out.contains("no reports yet"),
        "expected friendly notice for missing log:\n{out}",
    );

    let _ = std::fs::remove_dir_all(&f);
}
