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

pub mod badge;
pub mod button;
pub mod card;
pub mod form;
pub mod hero;
pub mod section;
pub mod typography;

pub use badge::{Badge, BadgeSize, BadgeTone};
pub use button::{Button, ButtonSize, ButtonVariant, Decoration, IconPosition};
pub use card::{Card, CardElevation, CardHover, CardPadding, FeatureCard, LinkCard};
pub use form::{InputType, Select, SelectOption, TextArea, TextInput};
pub use hero::{Hero, HeroBackground};
pub use section::{Section, SectionTheme};
pub use typography::{BodyText, Heading, HeadingLevel, HeadingTone, HeadingVariant, Lede};
