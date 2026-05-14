//! report_collector_e2e — integration test for the cycle 63
//! security-report collector. T76 cycle 68 (AVP-2 Tier 5).
//!
//! Spawns `loom edit-serve` against a temporary CMS fixture,
//! POSTs synthetic CSP-violation and Reporting-API report
//! bodies, asserts both return 204, then reads the resulting
//! violations.jsonl and verifies its shape.
//!
//! Stdlib-only (TcpStream + manual HTTP construction). No
//! external HTTP client dep — the test is byte-level
//! verifiable.
//!
//! REGRESSION-GUARD: if cycle 63's collector handler ever
//! changes the on-disk format, this test breaks and forces
//! the operator to confirm intent.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Pick a port unique per test invocation in this process.
/// Tests run in parallel by default; a shared atomic counter
/// hands each test a distinct ephemeral port so two
/// concurrent spawn_server() calls don't collide on the same
/// loom edit-serve binding.
fn pick_port() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    // Base into the high ephemeral range; add per-process
    // PID-jitter so two `cargo test` runs in parallel CI jobs
    // don't collide either. The atomic counter handles
    // intra-process parallelism.
    let pid = std::process::id() as u16;
    let base: u16 = 50000 + (pid % 1000) * 10;
    let offset = NEXT.fetch_add(1, Ordering::Relaxed);
    base.wrapping_add(offset)
}

struct ServerGuard {
    child: Child,
    port: u16,
    fixture: PathBuf,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.fixture);
    }
}

/// Build a temporary fixture (cms/ + static/), invoke
/// `loom edit-serve`, wait for the port to bind, return a
/// guard that kills on drop.
fn spawn_server() -> ServerGuard {
    let port = pick_port();
    // Per-run fixture under /tmp; cleaned up by Drop.
    // REGRESSION-GUARD: `Instant::now().elapsed()` is
    // essentially zero (the Instant was just created), so
    // two concurrent tests under the same PID collided on
    // fixture name. Use the wall-clock nanos AND the port
    // (already unique per test) to guarantee a distinct
    // directory.
    let fixture = std::env::temp_dir().join(format!(
        "loom-collector-e2e-{}-{}-{}",
        std::process::id(),
        port,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    let cms_dir = fixture.join("cms");
    let static_dir = fixture.join("static");
    std::fs::create_dir_all(&cms_dir).expect("create cms dir");
    std::fs::create_dir_all(&static_dir).expect("create static dir");
    // Minimal valid CmsPage so edit-serve has something to list.
    let page_json = r#"{
        "title":"Test","description":"Test fixture","path":"/test.html",
        "sections":[]
    }"#;
    std::fs::write(cms_dir.join("test.json"), page_json)
        .expect("write fixture page");

    // Locate the `loom` binary. Cargo sets `CARGO_BIN_EXE_<name>`
    // for integration tests so we can use the just-built bin.
    let bin = env!("CARGO_BIN_EXE_loom");
    let child = Command::new(bin)
        .arg("edit-serve")
        .arg("--port")
        .arg(port.to_string())
        .current_dir(&fixture)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn loom edit-serve");

    // Wait up to 5 seconds for the port to bind.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return ServerGuard { child, port, fixture };
        }
        sleep(Duration::from_millis(50));
    }
    panic!("loom edit-serve did not bind to port {port} within 5s");
}

/// POST `body` to `path` on the running server with the given
/// content-type. Returns (status_code, response_body).
fn post(port: u16, path: &str, content_type: &str, body: &[u8]) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .expect("connect to edit-serve");
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n",
        len = body.len(),
    );
    stream.write_all(request.as_bytes()).expect("write request line+headers");
    stream.write_all(body).expect("write body");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");
    let text = String::from_utf8_lossy(&response).into_owned();
    // Parse the status line: "HTTP/1.1 NNN ..."
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, text)
}

#[test]
fn collector_accepts_legacy_csp_report_returns_204_and_logs_jsonl() {
    let g = spawn_server();
    let csp_body = br#"{"csp-report":{"document-uri":"https://test.example/","violated-directive":"script-src","blocked-uri":"https://evil.example/x.js"}}"#;
    let (status, _) = post(g.port, "/csp-report", "application/csp-report", csp_body);
    assert_eq!(status, 204, "legacy /csp-report should return 204 No Content");

    // The collector writes to <cms_root>/../reports/violations.jsonl.
    // Our fixture's cms_root is fixture/cms, so reports lives at
    // fixture/reports/violations.jsonl.
    let log = g.fixture.join("reports").join("violations.jsonl");
    // Give the server a moment to flush the write.
    for _ in 0..20 {
        if log.exists() && std::fs::metadata(&log).map(|m| m.len() > 0).unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    let contents = std::fs::read_to_string(&log)
        .expect("violations.jsonl exists after POST");
    let line = contents.lines().last().expect("at least one JSONL line");
    assert!(line.contains("\"endpoint\":\"csp-report\""),
        "JSONL line should record the endpoint: {line}");
    assert!(line.contains("\"content_type\":\"application/csp-report\""),
        "JSONL line should record the content type: {line}");
    assert!(line.contains("violated-directive"),
        "JSONL line should preserve the report body: {line}");
    assert!(line.starts_with("{\"ts\":"),
        "JSONL line should start with the timestamp field: {line}");
}

#[test]
fn collector_accepts_modern_reports_api_returns_204_and_logs_jsonl() {
    let g = spawn_server();
    let reports_body = br#"[{"type":"csp-violation","age":42,"url":"https://test.example/","body":{"effectiveDirective":"script-src"}}]"#;
    let (status, _) = post(g.port, "/reports", "application/reports+json", reports_body);
    assert_eq!(status, 204, "modern /reports should return 204 No Content");

    let log = g.fixture.join("reports").join("violations.jsonl");
    for _ in 0..20 {
        if log.exists() && std::fs::metadata(&log).map(|m| m.len() > 0).unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    let contents = std::fs::read_to_string(&log)
        .expect("violations.jsonl exists after POST");
    let line = contents.lines().last().expect("at least one JSONL line");
    assert!(line.contains("\"endpoint\":\"reports\""),
        "JSONL line should record the endpoint: {line}");
    assert!(line.contains("\"content_type\":\"application/reports+json\""),
        "JSONL line should record the content type: {line}");
    assert!(line.contains("effectiveDirective"),
        "JSONL line should preserve the report body: {line}");
}

#[test]
fn collector_body_cap_silently_truncates_oversized_payloads() {
    // The handler caps the body at 64 KiB. A 100 KiB payload
    // should still return 204 (we don't want to retry-storm
    // browsers), but only the first 64 KiB lands in the log.
    let g = spawn_server();
    let huge = vec![b'a'; 100 * 1024];
    let (status, _) = post(g.port, "/csp-report", "application/csp-report", &huge);
    assert_eq!(status, 204,
        "oversized body should still return 204; never 4xx so browsers don't retry-storm");

    let log = g.fixture.join("reports").join("violations.jsonl");
    for _ in 0..20 {
        if log.exists() && std::fs::metadata(&log).map(|m| m.len() > 0).unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    let contents = std::fs::read_to_string(&log)
        .expect("violations.jsonl exists after POST");
    let line = contents.lines().last().expect("at least one JSONL line");
    // The body field is JSON-escaped (no `\` escape needed for
    // 'a'), so length of "aaa...aaa" should be capped at ~65536.
    // Allow a margin for the surrounding JSON shape.
    assert!(line.len() < 70 * 1024,
        "oversized body should be capped at 64 KiB; got {} bytes", line.len());
    // The first 100 'a's should definitely be in the body field.
    assert!(line.contains("aaaaaaaaaa"),
        "first chunk of oversized body should land in log");
}

#[test]
fn collector_rate_limits_after_100_reports_per_min_same_ip() {
    // T76 cycle 69: an attacker spamming the collector should
    // see every request return 204 (per W3C spec — no retry-
    // storm), but only the first ~100 reports actually land
    // in the JSONL log. The cap is per-IP/minute.
    let g = spawn_server();
    let body = br#"{"csp-report":{"document-uri":"https://test.example/"}}"#;
    // Fire 150 requests in tight succession from the same
    // (loopback) IP. All should return 204.
    let mut sent = 0usize;
    for _ in 0..150 {
        let (status, _) = post(g.port, "/csp-report", "application/csp-report", body);
        assert_eq!(status, 204, "every request must return 204 (spec)");
        sent += 1;
    }
    assert_eq!(sent, 150);

    // Read the JSONL log; expect at most RATE_LIMIT_PER_MIN
    // (100) lines + a small margin for the prior tests'
    // contamination is impossible because each test gets its
    // own fixture directory.
    let log = g.fixture.join("reports").join("violations.jsonl");
    for _ in 0..20 {
        if log.exists() && std::fs::metadata(&log).map(|m| m.len() > 0).unwrap_or(false) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    let contents = std::fs::read_to_string(&log)
        .expect("violations.jsonl exists after burst");
    let lines = contents.lines().count();
    assert!(
        lines <= 100,
        "rate limit caps at 100 reports/min; saw {lines} lines",
    );
    assert!(
        lines >= 90,
        "rate limit should let through ~100 lines; only saw {lines} — limiter too aggressive?",
    );
}

#[test]
fn collector_endpoints_unauthenticated() {
    // The collector MUST be reachable without a session cookie —
    // browsers can't carry session credentials on report POSTs
    // per W3C spec. Confirm both endpoints respond regardless
    // of auth state.
    let g = spawn_server();
    let (s1, _) = post(g.port, "/csp-report", "application/csp-report",
        b"{\"csp-report\":{}}");
    assert_eq!(s1, 204, "/csp-report must be unauthenticated");
    let (s2, _) = post(g.port, "/reports", "application/reports+json", b"[]");
    assert_eq!(s2, 204, "/reports must be unauthenticated");
}
