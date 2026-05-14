//! report_review_e2e — integration test for the cycle 88
//! `loom report-review {list|ack|dismiss|status}` subcommand.
//!
//! Spawns the `loom` binary against a temp dir containing a
//! hand-crafted reports/violations.jsonl, exercises each
//! review action, and asserts:
//!   - `list` shows reports with NEW status by default
//!   - `ack <sig>` writes a record to .review-state.jsonl AND
//!     re-list shows ACK for that signature
//!   - `dismiss <sig> --note ""` fails (note required)
//!   - `dismiss <sig> --note "reason"` succeeds
//!   - `status` count totals match
//!   - latest-wins: a second ack overrides a prior dismiss
//!   - unique-prefix sig resolution works
//!   - ambiguous prefix fails cleanly
//!
//! Discipline matches cycle 68's report_collector_e2e:
//! stdlib-only, no spawned server (these are pure CLI runs),
//! deterministic fixture cleanup via Drop.
//!
//! REGRESSION-GUARD: every report-review command must remain
//! purely additive — never mutate violations.jsonl. The
//! `last_modified_violations` check pins this.

use std::path::PathBuf;
use std::process::Command;

struct FixtureGuard {
    dir: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn make_fixture(label: &str) -> FixtureGuard {
    let dir = std::env::temp_dir().join(format!(
        "loom-report-review-e2e-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(dir.join("reports")).expect("mkdir reports");
    // Hand-crafted JSONL with three distinct CSP violations.
    // Each body differs → distinct sha256 → distinct sig.
    let log = "\
{\"ts\":1700000000,\"endpoint\":\"/csp-report\",\"body\":\"{\\\"violated-directive\\\":\\\"script-src\\\",\\\"document-uri\\\":\\\"https://example.com/page-a\\\"}\"}
{\"ts\":1700000060,\"endpoint\":\"/reports\",\"body\":\"{\\\"type\\\":\\\"network-error\\\",\\\"url\\\":\\\"https://example.com/api\\\"}\"}
{\"ts\":1700000120,\"endpoint\":\"/csp-report\",\"body\":\"{\\\"violated-directive\\\":\\\"img-src\\\",\\\"document-uri\\\":\\\"https://example.com/page-b\\\"}\"}
";
    std::fs::write(dir.join("reports").join("violations.jsonl"), log)
        .expect("write violations.jsonl");
    FixtureGuard { dir }
}

fn loom() -> Command {
    Command::new(env!("CARGO_BIN_EXE_loom"))
}

fn run_review(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let reports_dir = dir.join("reports");
    let mut cmd = loom();
    cmd.arg("report-review");
    cmd.args(args);
    cmd.arg("--dir").arg(&reports_dir);
    cmd.current_dir(dir);
    let out = cmd.output().expect("spawn loom report-review");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let code = out.status.code().unwrap_or(-1);
    (code, stdout, stderr)
}

fn first_sig(stdout: &str) -> String {
    // Parse the first 12-char-hex token that appears under the
    // `sig` column. Header row is "sig" (3 chars) so we look
    // for a line whose first whitespace-delimited token is
    // exactly 12 lowercase hex chars.
    for line in stdout.lines() {
        let tok = line.split_whitespace().next().unwrap_or("");
        if tok.len() == 12 && tok.chars().all(|c| c.is_ascii_hexdigit()) {
            return tok.to_owned();
        }
    }
    panic!("no 12-char-hex sig found in stdout:\n{stdout}");
}

#[test]
fn list_shows_three_new_reports_by_default() {
    let g = make_fixture("list_new");
    let (code, stdout, stderr) = run_review(&g.dir, &["list"]);
    assert_eq!(code, 0, "exit=0 expected; stderr={stderr}");
    assert!(stdout.contains("NEW"), "expected NEW status; stdout={stdout}");
    let new_count = stdout.matches("NEW").count();
    // 1 header column "status" doesn't say NEW; 3 data rows do.
    assert!(new_count >= 3, "expected ≥3 NEW rows, got {new_count}; stdout={stdout}");
}

#[test]
fn ack_then_list_shows_ack_status() {
    let g = make_fixture("ack");
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    let sig = first_sig(&list_out);

    let (code, _stdout, stderr) =
        run_review(&g.dir, &["ack", &sig, "--note", "investigated"]);
    assert_eq!(code, 0, "ack exit=0 expected; stderr={stderr}");

    // State log must exist + contain the sig + note.
    let state_path = g.dir.join("reports/.review-state.jsonl");
    let state_bytes = std::fs::read_to_string(&state_path)
        .expect("read state log");
    assert!(state_bytes.contains(&sig), "state log missing sig; got: {state_bytes}");
    assert!(
        state_bytes.contains("\"action\":\"ack\""),
        "state log missing ack action; got: {state_bytes}"
    );
    assert!(
        state_bytes.contains("\"note\":\"investigated\""),
        "state log missing note; got: {state_bytes}"
    );

    // Re-list must show ACK for that sig.
    let (_, list_out2, _) = run_review(&g.dir, &["list"]);
    assert!(
        list_out2.contains("ACK"),
        "re-list missing ACK; stdout={list_out2}"
    );
    // The note must be visible in the list output.
    assert!(
        list_out2.contains("investigated"),
        "note not visible in list; stdout={list_out2}"
    );
}

#[test]
fn dismiss_without_note_fails() {
    let g = make_fixture("dismiss_no_note");
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    let sig = first_sig(&list_out);

    let (code, _stdout, stderr) =
        run_review(&g.dir, &["dismiss", &sig, "--note", ""]);
    assert_ne!(code, 0, "dismiss with empty note must fail; stderr={stderr}");
    assert!(
        stderr.contains("--note is required"),
        "expected note-required error; got: {stderr}"
    );

    // State log must NOT contain a dismiss for this sig.
    let state_path = g.dir.join("reports/.review-state.jsonl");
    if state_path.exists() {
        let state_bytes = std::fs::read_to_string(&state_path).unwrap_or_default();
        assert!(
            !state_bytes.contains("\"action\":\"dismiss\""),
            "rejected dismiss must not write to state log; got: {state_bytes}"
        );
    }
}

#[test]
fn dismiss_with_note_succeeds_and_shows_dismissed() {
    let g = make_fixture("dismiss_ok");
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    let sig = first_sig(&list_out);

    let (code, _stdout, stderr) = run_review(
        &g.dir,
        &["dismiss", &sig, "--note", "extension noise"],
    );
    assert_eq!(code, 0, "dismiss with note exit=0; stderr={stderr}");

    let (_, list_out2, _) = run_review(&g.dir, &["list"]);
    assert!(
        list_out2.contains("DISMISSED"),
        "list missing DISMISSED; stdout={list_out2}"
    );
    assert!(
        list_out2.contains("extension noise"),
        "dismissal note missing; stdout={list_out2}"
    );
}

#[test]
fn status_counts_match_actions() {
    let g = make_fixture("status");
    // Ack 1, dismiss 1, leave 1 NEW.
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    // First 12-char sig in the list.
    let mut sigs: Vec<String> = Vec::new();
    for line in list_out.lines() {
        let tok = line.split_whitespace().next().unwrap_or("");
        if tok.len() == 12 && tok.chars().all(|c| c.is_ascii_hexdigit()) {
            sigs.push(tok.to_owned());
        }
    }
    assert!(sigs.len() >= 3, "expected ≥3 distinct sigs, got {sigs:?}");

    let (code, _, stderr) =
        run_review(&g.dir, &["ack", &sigs[0], "--note", "ok"]);
    assert_eq!(code, 0, "ack failed; stderr={stderr}");
    let (code, _, stderr) =
        run_review(&g.dir, &["dismiss", &sigs[1], "--note", "noise"]);
    assert_eq!(code, 0, "dismiss failed; stderr={stderr}");

    let (code, status_out, stderr) = run_review(&g.dir, &["status"]);
    assert_eq!(code, 0, "status failed; stderr={stderr}");
    assert!(status_out.contains("total distinct reports : 3"), "stdout={status_out}");
    assert!(status_out.contains("NEW (untriaged)        : 1"), "stdout={status_out}");
    assert!(status_out.contains("ACK                    : 1"), "stdout={status_out}");
    assert!(status_out.contains("DISMISSED              : 1"), "stdout={status_out}");
}

#[test]
fn latest_wins_when_signature_retriaged() {
    let g = make_fixture("latest_wins");
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    let sig = first_sig(&list_out);

    // First dismiss, then ack — latest action (ack) should win.
    let (code, _, _) =
        run_review(&g.dir, &["dismiss", &sig, "--note", "initial pass"]);
    assert_eq!(code, 0);
    // 1-second separation to guarantee monotonic ts on the
    // second decision. (The sub-second resolution in
    // review_action_write uses unix-secs.)
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let (code, _, _) =
        run_review(&g.dir, &["ack", &sig, "--note", "reviewed harder"]);
    assert_eq!(code, 0);

    let (_, list_out2, _) = run_review(&g.dir, &["list"]);
    // Latest-wins → must show ACK, not DISMISSED for this sig.
    // Locate the sig line specifically; ignore the rest.
    let sig_line = list_out2
        .lines()
        .find(|l| l.starts_with(&sig))
        .unwrap_or_else(|| panic!("sig line not found in:\n{list_out2}"));
    assert!(
        sig_line.contains("ACK") && !sig_line.contains("DISMISSED"),
        "latest-wins broken — sig line: {sig_line}"
    );
}

#[test]
fn unique_prefix_resolves() {
    let g = make_fixture("prefix_unique");
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    let sig = first_sig(&list_out);
    let prefix: String = sig.chars().take(6).collect();

    let (code, _, stderr) = run_review(
        &g.dir,
        &["ack", &prefix, "--note", "prefix worked"],
    );
    assert_eq!(code, 0, "unique 6-char prefix should resolve; stderr={stderr}");

    let state_bytes =
        std::fs::read_to_string(g.dir.join("reports/.review-state.jsonl"))
            .expect("read state log");
    // The audit log MUST record the full 12-char sig, not the prefix.
    assert!(
        state_bytes.contains(&sig),
        "audit log must store full sig, not prefix; got: {state_bytes}"
    );
}

#[test]
fn rotation_files_aggregate_into_one_view() {
    // Cycle 71 rotates the collector log when it exceeds size
    // bounds, producing `violations-<unix>.<nanos>.jsonl`
    // alongside the active `violations.jsonl`. Cycle 88's
    // `review_collect_entries` accepts both name shapes —
    // this test pins that aggregation so a future rotation-
    // format change doesn't silently drop history from the
    // REVIEW layer.
    //
    // REGRESSION-GUARD (cycle 91): the report-review reader
    // MUST include rotated files when listing/triaging. If
    // review_collect_entries only reads `violations.jsonl`,
    // operators triage today's reports while yesterday's
    // ROTATED violations stay un-triaged forever.

    let g = make_fixture("rotation");
    // Existing fixture already has violations.jsonl with 3
    // entries. Add a SECOND file simulating a rotated log
    // with 2 older entries (different bodies → different sigs).
    let older = "\
{\"ts\":1699900000,\"endpoint\":\"/csp-report\",\"body\":\"{\\\"violated-directive\\\":\\\"style-src\\\",\\\"document-uri\\\":\\\"https://example.com/old-page-1\\\"}\"}
{\"ts\":1699900060,\"endpoint\":\"/csp-report\",\"body\":\"{\\\"violated-directive\\\":\\\"connect-src\\\",\\\"document-uri\\\":\\\"https://example.com/old-page-2\\\"}\"}
";
    std::fs::write(
        g.dir.join("reports").join("violations-1699900000.0.jsonl"),
        older,
    )
    .expect("write rotated fixture");

    let (code, stdout, stderr) =
        run_review(&g.dir, &["status"]);
    assert_eq!(code, 0, "status failed; stderr={stderr}");
    assert!(
        stdout.contains("total distinct reports : 5"),
        "expected aggregate over 2 files = 5 reports; stdout={stdout}"
    );

    // Also: ack a sig from the ROTATED file. Sig resolution
    // must see it. Pull a sig from `list` (which orders newest-
    // first; the rotated entries are older → towards the
    // bottom).
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    let mut sigs: Vec<String> = Vec::new();
    for line in list_out.lines() {
        let tok = line.split_whitespace().next().unwrap_or("");
        if tok.len() == 12 && tok.chars().all(|c| c.is_ascii_hexdigit()) {
            sigs.push(tok.to_owned());
        }
    }
    assert!(
        sigs.len() >= 5,
        "list must show all 5 reports across rotation; got {}",
        sigs.len()
    );
    // The last two sigs are from the rotated file (older ts).
    // Ack the very-oldest one — it would be invisible if we
    // only read violations.jsonl.
    let oldest = sigs.last().unwrap().clone();
    let (code, _, stderr) =
        run_review(&g.dir, &["ack", &oldest, "--note", "old-but-real"]);
    assert_eq!(
        code, 0,
        "ack on rotated-file sig must succeed; stderr={stderr}"
    );
}

#[test]
fn list_purely_additive_does_not_mutate_violations() {
    let g = make_fixture("immutable_log");
    let violations = g.dir.join("reports/violations.jsonl");
    let before = std::fs::metadata(&violations).unwrap().modified().unwrap();
    let before_bytes = std::fs::read_to_string(&violations).unwrap();

    let _ = run_review(&g.dir, &["list"]);
    let (_, list_out, _) = run_review(&g.dir, &["list"]);
    let sig = first_sig(&list_out);
    let _ = run_review(&g.dir, &["ack", &sig, "--note", "x"]);
    let _ = run_review(&g.dir, &["dismiss", &sig, "--note", "y"]);
    let _ = run_review(&g.dir, &["status"]);
    let _ = run_review(&g.dir, &["list"]);

    let after = std::fs::metadata(&violations).unwrap().modified().unwrap();
    let after_bytes = std::fs::read_to_string(&violations).unwrap();
    assert_eq!(before, after, "violations.jsonl mtime changed — review must be read-only");
    assert_eq!(before_bytes, after_bytes, "violations.jsonl content changed");
}
