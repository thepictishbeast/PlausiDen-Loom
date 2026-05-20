//! Build script for loom-tokens.
//!
//! Tells cargo to invalidate this crate (and cascade to downstream
//! consumers like loom-cms-render, forge-cli, etc.) whenever
//! `src/skin.css` changes. Without this, the `include_str!("skin.css")`
//! in lib.rs is invisible to cargo's dirty-tracking — edits to skin.css
//! leave the release binary stale until something else in the crate
//! happens to be touched.
//!
//! Discovered 2026-05-20: a logo_cloud CSS rewrite did not propagate to
//! `target/release/forge` until `loom-tokens/src/lib.rs` was manually
//! touched. This file makes the dependency explicit.

fn main() {
    println!("cargo:rerun-if-changed=src/skin.css");
}
