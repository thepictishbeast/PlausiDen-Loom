//! Shared test helpers for `loom-cli` unit tests.
//!
//! Gated behind `#[cfg(test)]` at the crate-root declaration so the
//! helpers never ship in release binaries. Extracted from 8
//! duplicated `fn unique(label)` callsites flagged by the
//! composition audit as `dry-duplicate-block`.

#![cfg(test)]

use std::path::PathBuf;

/// Build a temp-dir path guaranteed unique across concurrent runs.
///
/// Combines `prefix`, `label`, current process id, and nanos since
/// UNIX epoch so two tests racing on the same prefix never collide.
///
/// Callers that want a file extension chain `with_extension(_)`
/// on the returned path — the basename intentionally has no
/// extension so callers stay in control.
pub(crate) fn unique_tmp(prefix: &str, label: &str) -> PathBuf {
    let pid = std::process::id();
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("{prefix}-{label}-{pid}-{n}"))
}
