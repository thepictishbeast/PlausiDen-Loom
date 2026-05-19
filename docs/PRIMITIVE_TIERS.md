# Loom primitive tiers

**Status:** doctrine. Closes #106. Establishes the tier vocabulary so
new primitive proposals can name which gap they fill, and so caller
sites can reason about composition.

Loom's `CmsSection` is one closed enum with 166+ variants. Without a
tier model the list reads as a flat alphabetical grab-bag and gap
analysis becomes guesswork. The five tiers below partition every
primitive by *what kind of work it does in a page*, not by visual
shape.

## The five tiers

### Tier 1 — Marketing composite

Primitives that orchestrate other content into a single visual unit
optimised for landing pages, product surfaces, conversion paths.

Members: `Hero`, `ImageHero`, `SplitHero`, `FeatureSpotlight`,
`StatBand`, `Steps`, `Pricing`, `Faq`, `Marquee`, `CallToAction`,
`Banner`, `LogoWall`, `LogoCloud`, `Testimonial`, `Comparison`,
`Timeline`, `Roadmap`, `CaseStudy`, `AnnouncementBar`, `PromoStrip`,
`AwardBadges`, `NewsletterSignup`, `ContactStrip`, `PullStat`,
`ProductCard`, `ProductGrid`, `BadgeGrid`, `IconRow`.

Caller responsibility: choose ONE of these per intent. Never stack
two heroes; never stack a `StatBand` AND a `FeatureSpotlight` on the
same fold unless the section has narrative justification.

### Tier 2 — Editorial body

Primitives whose job is long-form readable prose. Newspaper /
literary press / academic publication / deep technical doc shapes.

Members: `Paragraph`, `Heading`, `SubHeading`, `Lede`, `DropCap`,
`PullQuote`, `Epigraph`, `Marginalia`, `Footnote`, `AsideNote`,
`Citation`, `Quote`, `Figure`, `Caption`, `DefList`, `Glossary`,
`MathBlock`, `Diagram`, `Code`, `Article`, `TocBlock`.

Caller responsibility: compose freely. Long-form articles routinely
mix 6-10 of these in a single piece. Density is the goal — substrate
de-consumer-shaping per `feedback_consumer_shaped_substrate`.

Gap-filling rule: when a real-world piece (NYT essay, Pitchfork
review, academic paper, technical RFC) uses a typographic shape Loom
can't express, it's a Tier 2 gap. File on #104.

### Tier 3 — Compositional relationship

Primitives that express SPATIAL relationships between other primitives.
They carry no content themselves.

Members: `Container`, `Stack`, `Cluster`, `Columns`, `GridLayout`,
`Tabs`, `AccordionGroup`, `Reveal`, `Sidebar`.

Caller responsibility: prefer the most specific tier-3 primitive over
inline div-wrapping in Tier 1/2 callers. A 3-column feature grid is
`GridLayout`, not three side-by-side `Cards` inside an `ImageHero`.

### Tier 4 — Decorative

Primitives that exist for visual rhythm — no content, no semantic
relationship, just typographic whitespace and ornament.

Members: `Divider` (styles: Line, Dots, ZigZag, Sparkle), `Spacer`.

Caller responsibility: use sparingly. A page littered with `Spacer`s
is a sign that the design system's vertical-rhythm tokens aren't doing
their job — the right fix is a token, not more spacers.

### Tier 5 — Application surface

Primitives for logged-in / app-context state. Not part of editorial
or marketing flows.

Members: `Form`, `Composer`, `AccountSummary`, `ProfileEdit`,
`LegalDoc`, `SettingsPanel`, `CardFeed`, `Sidebar` (app-context),
`CookieNotice`, `AddToCart`, `PriceTag`, `Group`.

Caller responsibility: these primitives expect the page to be
authenticated or to be hosting an interactive flow. Don't mix into a
marketing landing.

### Tier 6 — Media

Primitives whose primary content is non-text media.

Members: `Picture`, `ImageGrid`, `FigureGroup`, `VideoEmbed`,
`AudioEmbed`, `Slideshow`, `BeforeAfter`, `Lightbox`, `MosaicGrid`.

## Why tiers (not categories)

Tiers compose top-down:

- A Tier 1 (marketing composite) primitive can embed Tier 2/3/6 in
  its slots — `ImageHero.before_headline: Vec<CmsSection>` accepts
  any tier.
- A Tier 3 (compositional) primitive can wrap any Tier 1/2/4/5/6.
- Tier 2 (editorial) primitives embed only Tier 2/3/4/6.
- Tier 4 (decorative) primitives never wrap anything.
- Tier 5 (application) primitives are leaves — they don't compose
  with marketing flows.

This is doctrine, not enforced by the type system. The closed
`CmsSection` enum allows any nesting; the tier model just gives
authors + the AI Critic a vocabulary for "is this composition
sensible."

## Tier-vs-variant: where to file work

- **New primitive in an existing tier** → file on #104 (variant
  explosion).
- **Compositional pattern that doesn't fit any tier** → file as a
  new doctrine task; may require adding a new tier here.
- **Visual ornament in a Tier 1/2/5/6 primitive** → likely a Tier 4
  decorative gap; consider whether a new `DividerStyle` variant or a
  new Tier 4 primitive (e.g. `OrnamentalRule`) fits.

## Forge-side discipline

Forge's CMS authoring flow surfaces the tier when proposing
primitives. A LLM-driven generator that mixes 4 Tier 1 primitives in
one page should be flagged by the LFI Critic (slop pattern: "all
landing-page chrome, no content").

The slop-dictionary entries in
`PlausiDen-LFI/crates/lfi-policy/src/canonical.rs` already
encode some of this — adding a `density-tier-floor` soft rule that
counts Tier 2 occurrences vs Tier 1 occurrences is the natural
next step (file on #109 reference corpus + density tiers).
