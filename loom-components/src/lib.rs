//! `loom-components` — typed UI primitives.
//!
//! Every primitive in this crate has the property: **it is impossible
//! to construct it with an arbitrary class string.** Every prop is a
//! constrained enum; every output composes `loom-tokens` values. A
//! caller wanting "just one custom variant" is wrong: extend the
//! design system instead.
//!
//! See [`CLAUDE.md`](../../CLAUDE.md) for the rules.

#![doc(html_no_source)]

pub mod button;
pub mod section;

pub use button::{Button, ButtonSize, ButtonVariant};
pub use section::{Section, SectionTheme};
