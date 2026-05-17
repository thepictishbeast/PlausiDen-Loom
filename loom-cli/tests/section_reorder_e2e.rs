//! section_reorder_e2e — integration test for the cycle 85
//! `POST /<slug>/sections/reorder` endpoint. T76 cycle 86.
//!
//! Spawns `loom edit-serve` against a fresh CMS fixture with
//! 4 sections in known order, drives reorder POSTs via raw
//! TcpStream, asserts:
//!   - 303 redirect on valid reorder.
//!   - section order in the JSON file matches the requested
//!     splice afterwards.
//!   - 400 on out-of-range indices.
//!   - 400 on missing form fields.
//!   - 303 + no file change on a no-op (from == to).
//!   - cycle 80 backup write fires (a .bak file appears).
//!
//! Stdlib-only HTTP client (TcpStream + manual request line).
//! Same discipline as cycle 68's report_collector_e2e.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn pick_port() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let pid = std::process::id() as u16;
    let base: u16 = 51000 + (pid % 1000) * 10;
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

/// A CmsPage with 4 named sections; each section's text is a
/// distinct letter so post-reorder we can verify the splice
/// landed correctly.
const FIXTURE_JSON: &str = r#"{
    "title": "Reorder Test",
    "description": "Reorder Test fixture",
    "path": "/reorder.html",
    "sections": [
        {"kind":"paragraph","text":"A"},
        {"kind":"paragraph","text":"B"},
        {"kind":"paragraph","text":"C"},
        {"kind":"paragraph","text":"D"}
    ]
}"#;

fn spawn_server() -> ServerGuard {
    let port = pick_port();
    let fixture = std::env::temp_dir().join(format!(
        "loom-section-reorder-e2e-{}-{}-{}",
        std::process::id(),
        port,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(fixture.join("cms")).expect("mkdir cms");
    std::fs::create_dir_all(fixture.join("static")).expect("mkdir static");
    std::fs::write(fixture.join("cms").join("reorder.json"), FIXTURE_JSON)
        .expect("write reorder.json");

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

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return ServerGuard {
                child,
                port,
                fixture,
            };
        }
        sleep(Duration::from_millis(50));
    }
    panic!("loom edit-serve did not bind to port {port} within 5s");
}

fn post(port: u16, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n",
        len = body.len(),
    );
    stream.write_all(request.as_bytes()).expect("write head");
    stream.write_all(body.as_bytes()).expect("write body");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read");
    let text = String::from_utf8_lossy(&response).into_owned();
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, text)
}

/// Return the `text` value of each section in order. Cheap
/// JSON probe — we don't need a full parser here.
///
/// REGRESSION-GUARD: the on-disk format is pretty-printed
/// (cycle T62 uses serde_json::to_string_pretty) — the
/// separator is `": "` with a space. The naive probe
/// `"text":"X"` (no space) misses every match. We accept
/// optional whitespace between `:` and the value's opening
/// quote.
fn read_section_texts(fixture: &std::path::Path) -> Vec<String> {
    let raw =
        std::fs::read_to_string(fixture.join("cms").join("reorder.json")).expect("read fixture");
    let mut out = Vec::new();
    let mut search = raw.as_str();
    let needle = "\"text\"";
    while let Some(idx) = search.find(needle) {
        let after = &search[idx + needle.len()..];
        // Skip optional whitespace + the `:` + optional whitespace.
        let after = after.trim_start();
        let Some(after) = after.strip_prefix(':') else {
            // Not actually a JSON key:value pair; advance.
            search = &search[idx + needle.len()..];
            continue;
        };
        let after = after.trim_start();
        let Some(after) = after.strip_prefix('"') else {
            search = &search[idx + needle.len()..];
            continue;
        };
        // Scan forward to the closing `"`, honouring `\"` escapes.
        let mut end = 0;
        let bytes = after.as_bytes();
        let mut escape = false;
        for (i, &b) in bytes.iter().enumerate() {
            if escape {
                escape = false;
                continue;
            }
            if b == b'\\' {
                escape = true;
                continue;
            }
            if b == b'"' {
                end = i;
                break;
            }
        }
        if end == 0 {
            break;
        }
        out.push(after[..end].to_owned());
        search = &after[end..];
    }
    out
}

fn count_revision_files(fixture: &std::path::Path) -> usize {
    std::fs::read_dir(fixture.join("cms"))
        .expect("readdir cms")
        .flatten()
        .map(|e| e.file_name())
        .filter(|n| {
            n.to_str()
                .map(|s| s.starts_with("reorder.bak.") && s.ends_with(".json"))
                .unwrap_or(false)
        })
        .count()
}

#[test]
fn reorder_moves_section_from_a_to_b_atomically() {
    let g = spawn_server();
    // Pre: [A, B, C, D]
    assert_eq!(read_section_texts(&g.fixture), vec!["A", "B", "C", "D"]);

    // Move section 0 (A) to index 2 → expect [B, C, A, D]
    let (status, _) = post(g.port, "/reorder/sections/reorder", "from=0&to=2");
    assert_eq!(status, 303, "valid reorder must redirect 303");

    sleep(Duration::from_millis(50));
    assert_eq!(
        read_section_texts(&g.fixture),
        vec!["B", "C", "A", "D"],
        "splice landed wrong",
    );
}

#[test]
fn reorder_to_end_works() {
    let g = spawn_server();
    // Move 0 (A) to last (3) → [B, C, D, A]
    let (status, _) = post(g.port, "/reorder/sections/reorder", "from=0&to=3");
    assert_eq!(status, 303);
    sleep(Duration::from_millis(50));
    assert_eq!(read_section_texts(&g.fixture), vec!["B", "C", "D", "A"]);
}

#[test]
fn reorder_to_start_works() {
    let g = spawn_server();
    // Move 3 (D) to index 0 → [D, A, B, C]
    let (status, _) = post(g.port, "/reorder/sections/reorder", "from=3&to=0");
    assert_eq!(status, 303);
    sleep(Duration::from_millis(50));
    assert_eq!(read_section_texts(&g.fixture), vec!["D", "A", "B", "C"]);
}

#[test]
fn reorder_no_op_redirects_without_writing() {
    let g = spawn_server();
    let before_revisions = count_revision_files(&g.fixture);
    let (status, _) = post(g.port, "/reorder/sections/reorder", "from=1&to=1");
    assert_eq!(status, 303);
    sleep(Duration::from_millis(50));
    // No-op MUST NOT trigger a backup write.
    let after_revisions = count_revision_files(&g.fixture);
    assert_eq!(
        before_revisions, after_revisions,
        "no-op should not write a backup; before={before_revisions} after={after_revisions}",
    );
    // Section order unchanged.
    assert_eq!(read_section_texts(&g.fixture), vec!["A", "B", "C", "D"]);
}

#[test]
fn reorder_out_of_range_returns_400() {
    let g = spawn_server();
    let (status, _) = post(g.port, "/reorder/sections/reorder", "from=0&to=99");
    assert_eq!(status, 400, "out-of-range `to` must be 400");
    // Section order untouched.
    assert_eq!(read_section_texts(&g.fixture), vec!["A", "B", "C", "D"]);
}

#[test]
fn reorder_missing_from_returns_400() {
    let g = spawn_server();
    let (status, _) = post(g.port, "/reorder/sections/reorder", "to=1");
    assert_eq!(status, 400, "missing `from` must be 400");
}

#[test]
fn reorder_writes_a_backup_revision() {
    let g = spawn_server();
    let before = count_revision_files(&g.fixture);
    let (status, _) = post(g.port, "/reorder/sections/reorder", "from=0&to=1");
    assert_eq!(status, 303);
    sleep(Duration::from_millis(100));
    let after = count_revision_files(&g.fixture);
    assert_eq!(
        after,
        before + 1,
        "reorder must trigger cycle 80 backup write; before={before} after={after}",
    );
}
