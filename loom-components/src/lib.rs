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
pub mod footer;
pub mod form;
pub mod hero;
pub mod modal;
pub mod nav;
pub mod section;
pub mod toast;
pub mod typography;

pub use badge::{Badge, BadgeSize, BadgeTone};
pub use button::{Button, ButtonSize, ButtonVariant, Decoration, IconPosition};
pub use card::{
    Card, CardElevation, CardHover, CardPadding, FeatureCard, FeatureCardStyle, LinkCard,
};
pub use footer::{Footer, FooterColumn, FooterItem, FooterLegalLink};
pub use form::{InputType, Select, SelectOption, TextArea, TextInput};
pub use hero::{Hero, HeroBackground};
pub use modal::{Modal, ModalSize};
pub use nav::{Nav, NavCta, NavLink};
pub use section::{Section, SectionTheme};
pub use toast::{Toast, ToastDuration, ToastTone};
pub use typography::{BodyText, Heading, HeadingLevel, HeadingTone, HeadingVariant, Lede};
