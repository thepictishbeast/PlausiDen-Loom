//! Shared recursive directory walker.
//!
//! Extracted from three near-identical `walk_*` helpers (audit:
//! `dry-duplicate-block`) — each filtered the recursive
//! `read_dir` results by extension. The closure-based predicate
//! lets each caller stay expressive at its callsite without
//! duplicating the iteration scaffold.

use std::path::{Path, PathBuf};

/// Recursively walk `dir` and append every file whose extension
/// `accept` returns `true` for into `out`. Directories are
/// recursed unconditionally; files with no extension are skipped.
///
/// `accept` is invoked with the extension as a `&str` exactly
/// as returned by `Path::extension` — callers that want
/// case-insensitive matching normalize themselves.
pub(crate) fn walk_paths_by_ext<F>(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    accept: F,
) -> std::io::Result<()>
where
    F: Fn(&str) -> bool + Copy,
{
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_paths_by_ext(&path, out, accept)?;
            continue;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if accept(ext) {
                out.push(path);
            }
        }
    }
    Ok(())
}
