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
pub mod code_shell;
pub mod composer;
pub mod footer;
pub mod form;
pub mod hero;
pub mod link;
pub mod modal;
pub mod nav;
pub mod picture;
pub mod pull_quote;
pub mod section;
pub mod toast;
pub mod typography;

pub use badge::{Badge, BadgeShape, BadgeSize, BadgeTone};
pub use button::{
    Button, ButtonShape, ButtonSize, ButtonType, ButtonVariant, Decoration, IconPosition,
};
pub use card::{
    Card, CardElevation, CardHover, CardPadding, CardShape, FeatureCard, FeatureCardStyle,
    KvPairCard, KvPairDensity, KvPairTone, LinkCard,
};
pub use code_shell::{CodeShell, CodeShellChrome, CodeShellLine, CodeShellLineKind, CodeShellTone};
pub use composer::{Composer, ComposerAvatar, ComposerSize, PromptAction, is_safe_url};
pub use footer::{Footer, FooterColumn, FooterItem, FooterLegalLink, FooterStyle};
pub use form::{
    FormDensity, FormStyle, InputType, Select, SelectOption, TextArea, TextInput,
};
pub use hero::{Hero, HeroBackground, HeroEditorial, HeroEditorialBackground};
pub use link::{TextLink, TextLinkSize, TextLinkVariant};
pub use modal::{Modal, ModalElevation, ModalShape, ModalSize};
pub use nav::{Nav, NavCta, NavLink, NavStyle};
pub use picture::{Picture, PictureFit, PictureLoading, PicturePriority};
pub use pull_quote::{PullQuote, PullQuoteEmphasis, PullQuoteTone};
pub use section::{Section, SectionPadding, SectionTheme, SectionWidth};
pub use toast::{Toast, ToastDuration, ToastElevation, ToastShape, ToastTone};
pub use typography::{
    BodyText, Heading, HeadingLevel, HeadingTone, HeadingVariant, HelperSize, HelperText, Lede,
};
