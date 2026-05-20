# Loom Reference Corpus (#109)

**Date:** 2026-05-20
**Closes:** Task #217 (preamble #109) — reference corpus +
density tiers, paired with `loom_tokens::density::DensityTier`.

The reference corpus is the **set of real public websites**
whose composition Loom must be able to reproduce via existing
typed primitives. It serves three purposes:

1. **Density-tier exemplars** — each site gets classified into
   one of `DensityTier::{Sparse, Comfortable, Dense, Extreme}`,
   anchoring the tier vocabulary to real, observable design.
2. **Primitive-coverage target** — every composition shape in
   the corpus must be reachable from `CmsSection` variants
   without hand-rolled HTML/CSS. Missing shape = substrate gap
   = new typed primitive PR.
3. **Pixel-reproduction rotation source** — the rotation
   tracker in `PlausiDen-Forge/docs/PIXEL_REPRODUCTION_ROTATION.md`
   pulls sites from this list when scheduling passes.

---

## Corpus members

| Site | URL | Density tier | Why in corpus |
|---|---|---|---|
| **Stripe** | stripe.com | Comfortable | Premium-SaaS marketing canonical. Hero + 3-col grid + dense kv-pair pricing + monospace code snippets. Reference for "polished + balanced." |
| **Linear** | linear.app | Sparse | Premium type-driven marketing. Gradient text + monospace accents. Reference for "luxe + minimal." |
| **Vercel** | vercel.com | Sparse | Black-on-black aesthetic; type-driven. Reference for "premium dark + monochrome." |
| **GitHub** | github.com | Dense | Logged-in dashboard density. Sidebar + main + activity feed. Reference for "operator tool at marketing-page entry point." |
| **Notion** | notion.so | Comfortable | Marketing site density + warm color palette. Reference for "approachable + structured." |
| **Anthropic** | anthropic.com | Comfortable | Editorial-feeling marketing; heavier prose blocks than typical SaaS. Reference for "research + product mix." |
| **Render** | render.com | Comfortable | Standard SaaS marketing density; useful as a "control" reference against more extreme exemplars. |
| **Fly** | fly.io | Comfortable | Same as Render — control point + slightly more technical voice. |
| **prosperityclub.com** | prosperityclub.com | Comfortable | In-ecosystem. Paul's production target. |
| **plausiden.com** | plausiden.com | Comfortable | In-ecosystem. Forge-static target. |
| **sacred.vote** | sacred.vote / sacredvote.org | Sparse | In-ecosystem (Forge-static approximation only — never touch sacred.vote source per scope constraint). Premium activist branding. |
| **Bloomberg Terminal** | bloomberg.com (representative) | Extreme | Reference exemplar for `Extreme` tier — every-pixel-earns-its-place trader UI. Used as the density-tier upper-bound anchor. |
| **Hacker News** | news.ycombinator.com | Extreme | Reference for `Extreme` tier achieved without graphical chrome — pure text + table density. Anchors "density without ornament." |
| **Datadog** | datadoghq.com | Dense | Dashboard tool density at marketing-page entry point. |

## Tier distribution

| Tier | Members | Count |
|---|---|---|
| Sparse | Linear, Vercel, sacred.vote | 3 |
| Comfortable | Stripe, Notion, Anthropic, Render, Fly, prosperityclub.com, plausiden.com | 7 |
| Dense | GitHub, Datadog | 2 |
| Extreme | Bloomberg, Hacker News | 2 |

The skew toward `Comfortable` is intentional — most production
marketing+editorial sites land there, and the substrate's
default (`DensityTier::default() == Comfortable`) reflects that.
Sparse exemplars exist to validate the substrate can ALSO ship
premium-luxe density; Dense + Extreme exist to ensure
operator-tool density is reachable without bolt-ons.

## What "must reproduce" means

A Loom build qualifies as "covers Reference site X" when:

1. The Forge CMS (`cms/<site>.json`) composes exclusively typed
   `CmsSection` variants — zero hand-authored HTML/CSS/JS.
2. The Forge build output renders at 390/768/1280 viewports.
3. The visual diff against the live site at all three viewports
   is within tolerance:
   * **In-ecosystem sites** (prosperityclub, plausiden,
     sacred.vote-static): ~2% AE per ImageMagick `compare -metric AE
     -fuzz 5%`
   * **Reference sites** (Stripe, Linear, etc.): ~5% AE — accepts
     more variance since exact pixel match against external
     designers' work is neither possible nor desirable (we're
     copying COMPOSITION, not BYTES).
4. The audit phase reports zero strict findings for
   `noscript_strict` + `reader_safety` + `network_target_enforcement`
   + `meta_refresh` + (when configured) `hunted_tier`.

## What's NOT in the corpus

* **Non-public sites** — operator-tool internals + admin portals.
  The corpus exists for substrate-validation against PUBLIC
  composition; private tools are out of scope.
* **Sites built by direct competitors of paul's products** —
  excluded to avoid commercial-IP-adjacent learning signals
  the substrate shouldn't carry.
* **Single-purpose interactive demos** — `webgl.io` /
  `awwwards.com` entries / WebGL showcases. Loom is a substrate
  for marketing+editorial+app-UI sites, not for one-off
  interactive demos. Those don't validate substrate decisions.

## Maintenance cadence

* Per rotation pass (see `PIXEL_REPRODUCTION_ROTATION.md` — task
  #230), the operator re-screenshots the live target. If the
  live target has materially changed (new homepage, new
  composition), update the corpus row to reflect the new shape.
* Sites that go offline / pivot / drop their public presence
  are removed and replaced with a similarly-shaped substitute.
  The corpus should hold at 14-20 members — wide enough to
  cover the tier × audience matrix, narrow enough that the
  rotation completes a full cycle in a reasonable cadence.
* New tier or audience seam discovered? Add a corpus row first,
  THEN propose substrate work to cover the gap. The corpus
  drives the substrate, not the other way around.

## Programmatic access

The `DensityTier` enum in `loom_tokens::density` is the typed
substrate vocabulary. Code that needs to reason about density:

```rust
use loom_tokens::DensityTier;

// Substrate default
let default = DensityTier::default(); // Comfortable

// All tiers (for picker UIs)
for tier in DensityTier::all() {
    println!("{}: {}", tier.slug(), tier.css_class());
}

// Classify by empirical char-per-1000sqpx (from Crawler density audit)
let measured: u32 = 250;
let tier = DensityTier::classify(measured); // Dense

// Stable wire shape
serde_json::to_string(&DensityTier::Sparse).unwrap(); // "\"sparse\""
```

The corpus row's "Density tier" column maps directly to
`DensityTier::{Sparse, Comfortable, Dense, Extreme}`.

## Future work

* **Per-tier audit phase** — a Forge phase that reads a site's
  declared target tier from `forge.toml [composition]
  target_density = "comfortable"` and confirms the rendered
  output's empirical density measurement falls within the
  declared tier's band (per `DensityTier::char_per_1000sqpx`).
* **Per-primitive density mapping** — `KvPairDensity`,
  `FormDensity` etc. should expose a `to_canonical(&self) ->
  DensityTier` method so primitive-level density choices roll
  up to canonical tiers in audit reports.
* **Reference-snapshot capture** — pin the live HTML+CSS+image
  bytes of each corpus member at the moment of a passing pass,
  with explicit copyright + licensing review per snapshot. Lets
  the substrate validate "we still match the snapshot we matched
  last time" without depending on continued availability of the
  live target.
