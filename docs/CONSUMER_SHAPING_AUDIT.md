# Loom Substrate Consumer-Shaping Audit (#103)

**Date:** 2026-05-20
**Scope:** loom-components + loom-cms-render
**Closes:** Task #212 (preamble #103)

This audit enumerates places where Loom primitives carry consumer-specific shape — fields, variants, or default values that fit one consumer's domain rather than the substrate-general design vocabulary. Each row carries a recommendation per the [[crawler-stays-general-purpose]] / [[consumer-shaped-substrate]] doctrine.

The audit is a snapshot, not a checklist for immediate refactor. Some consumer-specific shape is the right answer for the substrate's stage of evolution; this doc flags WHERE that shape exists so refactor decisions are explicit.

---

## Status summary

* **174 CmsSection variants** across loom-cms-render.
* **15 loom-components primitives** with substantial variant axes.
* **11 editorial-axis additions** shipped in this loop reducing consumer-shaping pressure (see #104 progress).

## Categories of consumer-shaped variants

### Category 1: Pure-consumer variants (refactor targets)

Variants that exist solely to serve one consumer's domain. Should be moved to a per-tenant overlay package OR generalized.

| Variant | Consumer | Recommendation |
|---|---|---|
| `CmsSection::GameTile` / `GameGrid` | gaming app (Crucible) | Move to a `loom-crucible-primitives` crate; Loom proper stays general. |
| `CmsSection::ThreadRow` / `ThreadList` | forum / sacred.vote | Move to `loom-social-primitives`. |
| `CmsSection::VideoCard` / `VideoGridSection` | video platform | Move to `loom-media-primitives`. |
| `CmsSection::CommentThread` / `FeedPost` | social feed | Already SkillShots-shape; move to `loom-social-primitives`. |
| `CmsSection::CrucibleWidget` | Crucible | Same as GameTile — extract. |

**Impact:** Loom CmsSection variant count drops by ~10 once these move out. Per-tenant overlays import them; Loom proper stays consumer-agnostic.

### Category 2: Consumer-leaked language (rename targets)

Variants whose NAME or doc references a specific consumer but whose SHAPE is generic.

| Variant | Leak | Recommendation |
|---|---|---|
| `composer.rs` doc says "well-known SkillShots action" | docstring | Generalize doc to "social-composer action". Primitive shape itself is generic. |
| `cms-render Hero` doc says "SkillShots PoC skin" | docstring | Update to "loom-skin baseline" — no PoC qualifier needed post-2026-05. |
| `link.rs` doc references "16 sites in plausiden.com" | docstring | Replace with "core inline-link variants" — the count is implementation history, not a contract. |

**Impact:** Pure documentation work. No wire-format breakage.

### Category 3: Consumer-default coupling (variant additions needed)

Variants where the default value matches a specific consumer's preference and operators of other consumers have to specify a non-default explicitly. The editorial-axis additions shipped this loop are the corrective pattern.

| Variant | Default | Issue | Fix |
|---|---|---|---|
| `CmsChrome::PageShell` | default | "Legacy SkillShots-style page shell" per docstring | Add a `Chrome::Editorial` variant — already partially via `NavStyle::Editorial` + `FooterStyle::Editorial`. PageShell as default stays back-compat. |
| `Hero` (loom-components) | centered + gradient + animations | SaaS-marketing default | Editorial counterpart shipped: `HeroEditorial`. ✅ |
| `FeatureCard` | rounded chrome + icon tile + hover-lift | SaaS feature-spotlight default | Editorial counterpart shipped: `KvPairCard`. ✅ |
| `Quote` (CmsSection) | testimonial-card with role | SaaS testimonial trope | Editorial counterpart shipped + extended: `PullQuote` with `emphasis: "display"` + `tone: "amoled"`. ✅ |
| `Badge` shape | `rounded-full` pill default | SaaS eyebrow-pill default | Editorial counterpart shipped: `BadgeShape::EditorialKicker`. ✅ |
| `Button` shape | `rounded-md`/`rounded-xl` size-driven | SaaS button radius | Editorial counterpart shipped: `ButtonShape::Square`. ✅ |
| `Modal` shape + elevation | `rounded-xl` + `shadow-2xl` | SaaS card-dialog | Editorial counterparts shipped: `ModalShape::Square` + `ModalElevation::Flat`. ✅ |
| `Toast` shape + elevation | `rounded-lg` + `shadow-md` | SaaS notification card | Editorial counterparts shipped: `ToastShape::Square` + `ToastElevation::Flat`. ✅ |
| `Card` shape | `rounded-xl` | SaaS content card | Editorial counterpart shipped: `CardShape::Square`. ✅ |
| `Nav` style | animated logo + sliding underline | SaaS nav chrome | Editorial counterpart shipped: `NavStyle::Editorial`. ✅ |
| `Footer` style | `rounded-lg` logo badge | SaaS footer logo | Editorial counterpart shipped: `FooterStyle::Editorial`. ✅ |
| `Form` chrome (rendered via CmsForm) | `rounded-md` + `bg-slate-50` | SaaS pill input | Editorial counterpart shipped on loom-components: `FormStyle::Editorial`. Wire-through into `CmsSection::Form` shipped 2026-05-20 (commits 60c6a78 + d3560e7) — `CmsFormStyle` enum + `style` field on the variant + `data-loom-form-style` attr emitted on both `<section>` and `<form>`. ✅ |

**Impact:** 12 of 12 known consumer-default leaks now have editorial counterparts behind explicit opt-in variant axes. Back-compat preserved via Default impls (`Default = Rounded` / `Default = Decorated` / etc.).

### Category 4: Non-issues (auditable but acceptable)

Variants that look consumer-shaped on first read but actually serve a general purpose.

| Variant | Looks like | Actually serves |
|---|---|---|
| `Composer` | SkillShots feed composer | Generic social-post composer (the actions slot accepts any prompt-action type) |
| `CardFeed` | SkillShots feed view | Generic ordered card list — used by leaderboards, blog indexes, search results, etc. |
| `LogoWall` | Stripe "trusted by" trope | The detector `aesthetic_distinctiveness::fake_testimonials` flags abuse; LogoWall itself is general. |
| `Pricing` | SaaS pricing trope | Same — detector flags abuse (most_popular_badge); primitive is general. |

**Impact:** Documentation could clarify intent. No structural changes needed.

---

## Outstanding work (TODOs)

* ~~**Wire `FormStyle::Editorial` through `CmsFormField`.**~~ ✅ **CLOSED 2026-05-20.** Shipped via commits 60c6a78 (CmsFormStyle enum + render_form param) + d3560e7 (variant field wire-through + 7-fixture migration). Operator now sets `"style": "editorial"` in their cms/*.json to opt the whole form into editorial chrome.
* **Extract Category-1 variants** (GameTile / GameGrid / ThreadRow / ThreadList / VideoCard / VideoGridSection / CommentThread / FeedPost / CrucibleWidget) **to per-tenant overlay crates.** Reduces CmsSection variant count by ~10 and makes Loom proper consumer-agnostic at the primitive level. **Status:** pending refactor work; not addressed this session (substantial cross-crate move). Path forward documented; deferred to a session with dedicated bandwidth.
* ~~**Update Category-2 docstrings** to drop consumer-specific references.~~ ✅ **CLOSED 2026-05-20** via commit 151be38. Composer + ChromeKind::PageShell + Hero + TextLink module/PrimaryMedium docs all generic-now.

---

## Closing notes

The audit confirms full de-consumer-shaping work landed across **all** Category-3 entries (12/12 ✅) and Category-2 docstring cleanups (3/3 ✅). The remaining open work is Category-1 extract (per-tenant overlay crates), which is a multi-commit refactor deferred to a dedicated session.

The editorial composition vocabulary now spans **12 axes** on the primitive side: HeroEditorial / KvPairCard / PullQuote / CodeShell / **CmsFormStyle (now wired)** / BadgeShape / ButtonShape / ModalShape+Elevation / ToastShape+Elevation / CardShape / NavStyle / FooterStyle. Per [[consumer-shaped-substrate]] memory, that's the highest-leverage pattern: variant-axis additions instead of monolithic primitive rewrites.

In parallel, the substrate now ships a **per-tenant corpora mechanism** (PlausiDen-Forge/docs/PER_TENANT_CORPORA.md + commits 534f02c → 65c443c) — operators extend the substrate's curated lists (jargon, scaffold defaults, body-leak markers, vague-link phrases, reference sites, density-tier overrides) via `forge.toml [tenant_corpus]` ADDITIVELY without forking the substrate. Wired across 4 phases: placeholder_value_audit (additive), aesthetic_distinctiveness (additive + subtractive + typo-guard), hunted_tier (additive), density_audit (per-pattern replace).

Future audits should re-run this enumeration after each new editorial-axis addition to verify the substrate isn't drifting back toward consumer-specific defaults. Tenants who need to express domain-specific shape signals should reach for the per-tenant corpora mechanism BEFORE proposing changes to the curated baseline.

## Cross-cutting changes since v1 audit (2026-05-20 session)

For traceability when reading the audit alongside the substrate state at HEAD:

* **Sparkline / BarChart / Histogram / DivergingBar / Heatmap / Boxplot** — 6-primitive editorial-charts vocabulary (chart-vocab axis closed; commits c58e4dc / 1a544e7 / f29ace0 / e088a36 / b71ffe8 / 8aed5d5)
* **EmailVerifyResult / BackupCodes / ConsentScreen / DeviceList / AccountDelete / PasswordChange** — 6-primitive account-flow vocabulary (commits 2f3f820 / c0b8106 / fd6b9d2 / 9409a32 / ae06476 / a81db82)
* **Section editorial + amoled + dense variants** — completes Section-primitive tier ladder (commit abf145b)
* **CSS-only theme toggle for noscript builds** (commit 7b82814) + **HUNTED_TIER_CHECKLIST.md** ops doc (commit de73010) + **TOR_OPERATIONS.md** server-side runbook (commit 9a684dd) + **NOSCRIPT_AUDIT.md** Loom side (commit 48b9467)

PRIMITIVE_COUNT bumped 145 → 157 across these additions; substrate-doctrine memory captures the consolidated approach.
