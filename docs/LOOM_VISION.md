# Loom — vision document

> "If Loom was already built and did everything we wanted, what
> would this doc say?"

This is that doc. Sections marked **[shipped]** work today on
`main`. **[in-flight]** is mid-build. **[queued]** has a task ID.
**[concept]** has been requested or implied by an owner directive
and a developer should design it.

Where this doc and the code disagree, the code wins this week and
the doc wins next week.

---

## 1. What Loom IS

Loom is the **typed primitive + CMS + site-builder + deploy tool**
that lets a non-technical owner build, edit, theme, audit, and
publish a real accessible website without writing any code.

Operationally, Loom is seven Rust crates:

| Crate | Role |
|---|---|
| `loom-tokens`      | Typed palette, spacing, breakpoints, font, radius scales — read-only constants |
| `loom-icons`       | Inline SVG icon set |
| `loom-components`  | Typed `Button`, `Card`, `Section`, `Hero`, `Banner`, `Composer`… every prop is a constrained enum |
| `loom-cms-render`  | The render layer — `CmsPage` / `CmsSection` types + `render_page` + `render_section` + `page_shell` (T70b moved this in from loom-cli) |
| `loom-lint`        | Walks `*.rs` view files; refuses raw class strings outside an allowlist |
| `loom-audit`       | (Coming) Visual-regression at every breakpoint |
| `loom-cli`         | Top-level CLI: `loom site`, `loom edit-serve`, `loom deploy`, `loom attest`, `loom import`, `loom cms-render`, `loom lint`, `loom audit`, `loom new` |

Loom is **not**:

- A static-site generator (Forge is that — Loom provides the
  rendering primitives Forge consumes via `loom-cms-render`)
- A bundler / framework / npm-driven runtime
- WordPress (themes are JSON tokens, not plugins; admin is a
  Loom subcommand, not a marketplace)
- A Tailwind alternative (Loom emits classes through typed
  components; the doctrine is on which classes appear and how)

Loom's contract: feed it a `cms/<slug>.json` file and it returns
HTML that ships WCAG 2.1 AA / ISO/IEC 40500-compliant in light AND
dark mode, with semantic landmarks, focus-visible outlines, skip
link, prefers-reduced-motion honour, and CSP-pinned inline styles
(never `unsafe-inline`).

## The meta-mission: making AI-built UI reliable

Every PlausiDen tool — Loom, CMS, Forge, Crawler, Annotator —
exists for one common reason: **AI agents building GUI / frontend /
UX work need a reliability substrate that humans don't.** A human
dev opens DevTools, eyeballs the layout, fixes the colour. An AI
agent doesn't open DevTools — so without typed primitives,
schema-validated content, mathematical contrast verification, and
runtime audit, regressions ship silently every iteration.

Loom is the substrate that makes AI-driven UI work reliable BY
CONSTRUCTION:

- **Typed components** mean an agent can't pass a bad string into
  a Button variant — the compile error catches it.
- **Typed `CmsPage` + `deny_unknown_fields`** mean an agent can't
  silently corrupt a page by typo'ing a field name — serde fails
  closed.
- **The page-shell ALWAYS emits dual theme + a11y defaults** mean
  an agent can't accidentally ship a light-only or
  no-skip-link site.
- **The `loom edit-serve` inline editor** lets an agent
  programmatically POST to `/inline-edit` with the same CSRF
  + per-kind whitelist defences a human gets.
- **`loom deploy verify`** gives an agent an oracle: "did my
  signed bundle land cleanly?" — branch on `SigStatus`.

Sibling tools close the loop: Forge audits the build, Crawler
verifies the runtime, Annotator captures human review for
agent-replay.

## 2. The supersociety stack Loom uses

- **Memory-safe core** — Rust everywhere, `#![forbid(unsafe_code)]`
  in every crate, no `unwrap`/`expect` in lib code (lint enforced)
  [shipped].
- **Type-safe rendering** — `CmsPage` + `CmsSection` enum with
  `deny_unknown_fields`. A schema-drift typo in `cms/<slug>.json`
  surfaces as a `serde_json` parse error at the boundary, not a
  silent miss-render [shipped].
- **Capability-based filesystem writes** — `WriteCapability::for_dir`
  canonicalises a confine-root and refuses any write that escapes.
  Atomic temp+rename for every output [shipped].
- **Cryptographic provenance** — `loom deploy publish` carries an
  Ed25519-signed manifest [shipped T47c]; the trust anchor is OUT-OF-
  BAND — bundle-local pubkeys are convenience only and MUST match
  the configured anchor (constant-time compare via `subtle`) [shipped].
- **Argon2id auth + HMAC-SHA256 cookies** — `SameSite=Strict`,
  `HttpOnly`, `Secure`. Constant-time secret comparison [shipped T43].
- **Strict CSP** — every page emits `style-src 'self' 'sha256-…'`
  (the base-theme block is hash-pinned), never `unsafe-inline`
  [shipped T48c v2].
- **WCAG 2.1 AA / ISO/IEC 40500 by default** — every page ships
  dual theme + skip link + focus-visible + reduced-motion + semantic
  landmarks via `loom_cms_render::page_shell`, no integrator wiring
  [shipped T48c v1+v2 + T70b].
- **Privacy-preserving uploads** — JPEG / PNG metadata strip
  before content-addressed storage [shipped T62 step 7]; GIF/WebP
  pending [queued T62 step 7b].
- **Property-based + mutation testing** — `proptest` on every
  parser; `cargo mutants` survival rate target < 5%.
- **Reproducible builds** — same inputs → bit-identical bundle
  (content-addressed `publish-<sha>/` dirs prove this) [shipped T47].

## 3. Personas

### 3.1 Mom — non-technical client (the gold standard)

What Mom does today:

1. `loom site init mybakery --template basic` — gets a complete
   site (schema-validated against the renderer) [shipped].
2. `loom edit-serve` opens a browser editor with **two panes**:
   typed forms on the left, live preview on the right.
3. **She clicks any text in the live preview and types over it.**
   Hit Enter to save. [shipped — T62 step 10]
4. She uploads a photo from her iPhone. **GPS / EXIF / timestamp
   stripped automatically** before storage. Her home address never
   leaks [shipped T62 step 7].
5. **An interactive in-browser tour** walks her through the
   editor on first visit [in-flight — current is static
   `/tutorial`; T64b adds query-string-driven highlighting].
6. She clicks "Publish". `loom deploy publish` ships an Ed25519-
   signed bundle [shipped T47, T47c].
7. She broke something? `loom deploy rollback` flips back in one
   command [shipped].

What Mom never has to think about:

- Path traversal, CSRF, XSS, mixed-content warnings, CSP, cookies.
- Typing JSON, editing CSS, picking colours that pass contrast.
- Whether her site works in dark mode — it just does.
- Whether her site is accessible — it just is.

What Mom can ALSO do (once queued capabilities ship):

- **WebAuthn passkey login** [queued T43d] — no password, just
  her phone or YubiKey.
- **Multiple sites under one account** [queued T45] — bakery,
  knitting club, family newsletter, all isolated.
- **Bring her old website to Loom** [shipped T63 + queued T63b
  for richer extraction] — `loom import --input old-site.html`
  emits Loom CmsPage JSON.
- **Pick from more bundled templates** [queued T48b — portfolio,
  blog, restaurant, photography, freelancer].
- **Embed contact forms / mailing-list signup / social links**
  via typed Composer / Banner sections [partially shipped, more
  variants queued].

### 3.2 The technical client — wants control

A small-business owner who CAN write Markdown but not Rust.

What they get today:

- **Loom design tokens are JSON** — `loom-tokens` exposes palette,
  spacing, breakpoints, font, radius. Edit the JSON and the whole
  theme re-skins.
- **Every typed component variant is enumerated** — no string
  blindness; the compiler exhaustiveness-checks every renderer.
- **`backends.toml` declares their custom endpoints** — Forge
  cross-checks every UI `data-backend="X"` ref has a declaration.
- **The same click-to-edit live preview Mom uses** — but with
  fine-grained access to every typed field via the form pane.
- **Per-section schema is surfaced via `loom cms-schema`** —
  emits a JSON Schema for IDE autocomplete + validation.
- **Atomic, rollback-able deploys** with cryptographic provenance.

What they get next:

- **Zero-JS theme/density/font switcher** (form-POST cookie) —
  user picks a preference, server emits `data-theme`, the choice
  sticks across navigation [queued T37].
- **WebAuthn auth** [queued T43d].
- **SSH/rsync deploy transport** for non-cloud targets
  [queued T47b].
- **Custom component primitives** via a future `loom-components`
  extension protocol [concept].
- **`loom doctor`** — health-check command that surfaces any
  misconfiguration in plain English [concept].

### 3.3 The developer — contributor or forker

What they get:

- **Crate boundary discipline** — `loom-tokens` is constants only,
  `loom-components` is typed primitives, `loom-cms-render` is the
  pure render layer (with `page_shell` since T70b), `loom-cli` is
  the binary surface.
- **`loom lint`** refuses raw class strings outside an allowlist —
  forces every visual rule through the typed component layer.
- **Inline annotation grammar** — `BUG ASSUMPTION:`, `AVP-PASS-N:`,
  `SECURITY:`, `REGRESSION-GUARD:`, `SHIP-DECISION:`, `SCHEMA:`,
  `UX-DEBT:` — all machine-grepable [shipped doctrine].
- **AVP-2 audit chain** built into commit messages, with a Merkle-
  chained build report from Forge.
- **Property-based tests** alongside fixtures on every parser
  (the contrast math, the image strippers, the inline-edit
  whitelist).

What developers want next:

- **`loom-audit`** — the visual-regression crate sketched but not
  implemented. Pixel-hash + 4-theme × 3-viewport snapshot grid
  [queued T33 in Forge complements].
- **Type-state Section heading + landmark contracts** — make
  level + landmark choices compile-time invariants. Trying to
  emit two `<h1>`s in one CmsPage is a compile error
  [queued T36].
- **`loom-lint` extension** for raw `ms`/`s` outside `:root`
  (any literal duration outside the token layer is drift)
  [queued T40].
- **Compose new bundled templates** via a `loom site init`
  contributor protocol [partially shipped via TEMPLATE_BASIC;
  template authoring guide queued].

### 3.4 Claude Code (and other autonomous agents)

What an agent gets today:

- **Stable JSON contract** — `cms/<slug>.json` is the addressable
  surface. Read, mutate, write — the typed schema makes drift
  impossible.
- **`loom site init`, `loom deploy publish/verify/rollback`,
  `loom attest init/pubkey`, `loom auth init`, `loom import`** —
  every command idempotent + deterministic.
- **`loom edit-serve --port N`** — multiple isolated editor
  instances in parallel.
- **Inline-edit POST is shaped for programmatic use** —
  `application/x-www-form-urlencoded` body, JSON-friendly
  response, `X-Loom-Inline-Edit: 1` CSRF marker for cross-origin
  defence [shipped T62 step 10].
- **Deploy verify returns SigStatus enum** — agents can branch
  on `ValidTrusted` / `ValidUntrusted` / `Unsigned` / error.

What agents want next:

- **API-key auth** alongside cookie auth [concept].
- **Multi-tenant per-tenant workspace** so an orchestrator can
  spawn one Claude per tenant [queued T45].
- **Sandboxed Claude Code SSH bridge** per tenant — outbound only
  through approved channels, can't see other tenants
  [queued T46 — biggest novel piece].
- **Cross-repo CROSSFIX protocol** — when an agent in Loom spots
  a fix that applies to Forge, it follows the AVP-2 cross-repo
  contribution flow.

## 4. Capability map

### 4.1 Content authoring

| Capability | Status |
|---|---|
| Typed CMS (`CmsPage` + 8 `CmsSection` variants: hero, group, paragraph, heading, banner, card-feed, sidebar, form, composer) | shipped |
| Typed editor forms per kind (`loom edit-serve`) | shipped |
| Click-to-edit inline editing in live preview | shipped (T62 step 10) |
| Click-to-jump-to-form overlay (lite version) | shipped (T62 step 9) |
| Section reorder / delete / add via form-POST | shipped |
| Bundled site template (`basic`) | shipped |
| Bundled portfolio + blog templates | queued (T48b) |
| Compound-field inline editing (group.body[N], cards) | queued (T62 step 10b) |
| HTML import → CmsPage | shipped (T63) |
| Markdown / WordPress / Notion import | concept (T63b extension) |
| Interactive in-browser tutorial | shipped static (T64); query-string tour queued (T64b) |

### 4.2 Image handling

| Capability | Status |
|---|---|
| Multipart upload, magic-byte sniff (JPEG/PNG/GIF/WebP) | shipped |
| Content-addressed storage with `Cache-Control: immutable` | shipped |
| EXIF / GPS / metadata strip on JPEG + PNG | shipped (T62 step 7) |
| EXIF strip on GIF + WebP | queued (T62 step 7b) |
| In-editor image picker | queued (T62 step 8) |
| Responsive `<picture>` with WebP/AVIF fallback | concept |
| Auto-resize at deploy time | concept |
| SVG (rejected — XSS/XXE risk; no current use case requiring it) | by design |

### 4.3 Theming + accessibility

| Capability | Status |
|---|---|
| Light + dark themes shipped on every page by default | shipped (T48c v2) |
| `<meta name="color-scheme" content="light dark">` always emitted | shipped |
| `prefers-color-scheme: dark` `@media` block always emitted | shipped |
| `prefers-reduced-motion: reduce` honoured globally | shipped |
| WCAG 2.1 AA contrast verified at compile + runtime | shipped (T29, T29b in Forge) |
| Skip-to-content link (visible on focus) | shipped |
| `:focus-visible` outline on every interactive | shipped |
| Semantic landmarks (`<header>`, `<main>`, `<footer>`, `<nav>`) | shipped (T48c v1) |
| Zero-JS theme/density/font switcher | queued (T37) |
| Type-state landmark contracts (compile-time guarantee) | queued (T36) |
| Keyboard-only navigation audit | concept |

### 4.4 Deploy

| Capability | Status |
|---|---|
| Local atomic deploy (symlink swap) | shipped (T47) |
| Content-addressed bundle dirs (`publish-<sha>`) | shipped |
| One-command rollback | shipped |
| Ed25519-signed manifests + bundle pubkey deposit | shipped (T47c) |
| Trust-anchor-required signature verification (no key substitution) | shipped (T47c v2) |
| `loom attest init/pubkey` — manage the signing keypair | shipped |
| `loom attest export` (QR + fingerprint sharing) | queued (T47e) |
| SSH/rsync transport for remote deploys | queued (T47b) |
| Hetzner / cloud-storage transport plugins | concept |
| Multi-region propagation | concept |
| Sigstore-style transparency log | concept |

### 4.5 Auth

| Capability | Status |
|---|---|
| Argon2id passwords + HMAC-SHA256 cookies | shipped (T43) |
| `SameSite=Strict` + `HttpOnly` + `Secure` cookies | shipped |
| Constant-time secret comparison via `subtle` | shipped |
| `loom auth init` — bootstrap auth.toml | shipped |
| WebAuthn / passkey login | queued (T43d) |
| API-key auth for agent integrations | concept |
| Multi-tenant per-tenant workspace + SQLite | queued (T45) |
| Sandboxed per-tenant Claude SSH bridge | queued (T46) |

### 4.6 Privacy + opsec

| Capability | Status |
|---|---|
| Image metadata strip (JPEG/PNG) | shipped |
| Image metadata strip (GIF/WebP) | queued (T62 step 7b) |
| Error scrubbing (no PII / paths leaked to client) | shipped per-site |
| `SHIP-DECISION` annotations for every accepted residual risk | shipped (doctrine) |
| TLS 1.3 only for outbound | doctrine |
| Tor / I2P / onion-service deploy target | concept |
| At-rest encryption for editor secrets | partial (cookie key on disk) |

### 4.7 Developer ergonomics

| Capability | Status |
|---|---|
| `loom new`, `loom site init`, `loom edit-serve` | shipped |
| `loom deploy {publish,verify,rollback}` | shipped |
| `loom attest {init,pubkey}` | shipped |
| `loom import`, `loom cms-render`, `loom auth init` | shipped |
| `loom lint`, `loom audit` | partial (lint shipped; audit stub) |
| `loom doctor` — diagnose misconfiguration | concept |
| `loom site init` with custom-template authoring guide | concept |

### 4.8 Documentation

| Capability | Status |
|---|---|
| `docs/USAGE.md` (Mom-friendly walkthrough) | shipped |
| `docs/DESIGN.md` (design rationale) | shipped |
| `docs/LOOM_VISION.md` (this doc) | shipped (T72) |
| Per-command `--help` with full doctrine | partial |
| In-GUI tutorial | shipped static (T64) |
| Interactive query-string tour mode | queued (T64b) |
| Architecture decision records (ADRs) | concept |

## 5. Architecture (when fully built)

```
┌──────────────────────────────────────────────────────────────┐
│                    Loom (PlausiDen-Loom)                      │
│                                                               │
│  ┌────────────┐   ┌────────────┐   ┌────────────────────┐   │
│  │ loom-tokens│──▶│ loom-      │──▶│ loom-cms-render    │   │
│  │ palette,   │   │ components │   │ - CmsPage / Section│   │
│  │ spacing,   │   │ Button,    │   │ - render_page      │   │
│  │ breakpoint │   │ Hero,      │   │ - render_section   │   │
│  │ scales     │   │ Banner,    │   │ - page_shell (T70b)│   │
│  └────────────┘   │ CardFeed…  │   │ - WCAG-AA + dual   │   │
│                   └────────────┘   │   theme + a11y     │   │
│                                    └────────────────────┘   │
│                                            │                  │
│                                            ▼                  │
│                                    ┌────────────────────┐    │
│                                    │ loom-cli           │    │
│                                    │ - site init        │    │
│                                    │ - edit-serve       │    │
│                                    │ - deploy           │    │
│                                    │ - attest           │    │
│                                    │ - import           │    │
│                                    │ - cms-render       │    │
│                                    │ - lint / audit     │    │
│                                    └────────────────────┘    │
│                                                               │
│  ┌─────────────────────────────┐   ┌──────────────────┐     │
│  │ loom-lint                   │   │ loom-audit       │     │
│  │ refuses raw class strings   │   │ visual diff (TBD) │     │
│  └─────────────────────────────┘   └──────────────────┘     │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼ public crate API
                    ┌──────────────────┐
                    │ PlausiDen-Forge  │
                    │ phase_render →   │
                    │ render_page +    │
                    │ page_shell       │
                    │ (in-process)     │
                    └──────────────────┘
                              │
                              ▼
                    ┌──────────────────┐
                    │  Ed25519-signed  │
                    │  bundle          │
                    │  publish-<sha>/  │
                    └──────────────────┘
```

Per-tenant view (T45 + T46 future):

```
┌────────── tenant A ──────────┐  ┌────────── tenant B ──────────┐
│  cms-A/                       │  │  cms-B/                       │
│  static-A/                    │  │  static-B/                    │
│  auth-A/                      │  │  auth-B/                      │
│  sandbox-A/  (claude ssh)     │  │  sandbox-B/  (claude ssh)     │
└──────────────────────────────┘  └──────────────────────────────┘
            │                                  │
            └──────────────┬───────────────────┘
                           ▼
                ┌──────────────────────┐
                │   Loom binary        │
                │   (shared, isolated  │
                │    per-tenant state) │
                └──────────────────────┘
```

## 6. Roadmap from now to "done"

### Sprint 1 — closing the directives that arrived this week

- [shipped] T48c v1+v2 — dual-theme + a11y baseline
- [shipped] T70b — page_shell into loom-cms-render so Forge inherits
- [shipped] T72 — this doc
- [queued] T68 — extend phase_theme_contrast in Forge to dual-theme
- [queued] T48b — portfolio + blog bundled templates
- [queued] T64b — interactive query-string tour mode
- [queued] T62 step 7b — GIF/WebP metadata strip
- [queued] T62 step 8 — image picker UI
- [queued] T62 step 10b — compound-field inline edit

### Sprint 2 — close the supersociety stack

- T36 — type-state Section heading + landmark contracts
- T37 — zero-JS theme/density/font switcher (form-POST)
- T38 — tokenize the 33 spacing literals T32 surfaced
- T40 — extend `loom-lint` to flag raw `ms`/`s` outside `:root`
- T34 — component state-matrix fixtures + crawler coverage
- T47b — SSH/rsync transport for `loom deploy`
- T47e — `loom attest export` (QR + fingerprint)
- T43d — WebAuthn passkey auth

### Sprint 3 — multi-tenant + agent farm

- T45 — multi-tenant per-tenant SQLite + workspace
- T46 — Claude Code SSH bridge (sandboxed per-tenant agent)
- API-key auth for agent integrations
- `loom doctor` — diagnose-and-suggest tooling
- Annotator integration — replay flagged sessions inside the editor

### Sprint 4 — capabilities not yet ticketed

- Markdown / WordPress / Notion → CmsSection importers (T63 extensions)
- Tor onion-service deploy transport
- Cloud-storage / Hetzner / R2 deploy transports
- Sigstore-style transparency log of every signed bundle
- Visual-regression crate (`loom-audit`) reaching parity with the
  Forge `phase_visual_diff` (T33)
- Component primitive expansion: more typed CmsSection variants
  (gallery, testimonial, faq, pricing-table, contact-form)
- Custom-template authoring protocol + contributor docs
- Responsive `<picture>` rendering with WebP/AVIF fallback
- Auto-image-resize at deploy time
- TLS-only outbound everywhere

### Sprint 5+ — the supersociety horizon (futures the world hasn't asked for yet)

These are deliberately ambitious. Each is here because thinking
hard about Mom, the technical client, the developer, and the
agent surfaced an unmet need.

**For Mom (non-technical client):**

- **Voice-to-CMS dictation.** Mom dictates a paragraph; on-device
  Whisper-class model transcribes; auto-creates a CmsSection.
- **Local AI-assisted content suggestions.** When Mom opens a hero
  with empty title, a local LLM proposes 3 candidates from a few
  bullet points she wrote — never sending her text to a cloud.
- **Local image generation.** Local diffusion model produces stock
  art for a section that needs a placeholder, never leaves her box.
- **Self-healing layout.** When content overflows on mobile, the
  editor offers "fit to mobile" with a one-click size adjustment.
- **One-button "make it match my brand."** Pick a brand colour;
  Loom auto-derives a WCAG-AAA-clean palette around it.
- **GDPR / CCPA / age-gating compliance UI** that BY DEFAULT sets
  zero cookies because Loom doesn't track — the banner is a
  declaration, not a popup.
- **Built-in newsletter signup** with double opt-in, encrypted
  subscriber list at rest, no SaaS dependency.
- **Form-builder with server-stub generator.** Drag-and-drop form
  fields → typed CmsSection → Forge generates the matching
  `backends.toml` entry → server-stub crate emits a real handler
  with Argon2id rate-limiting + spam defence.
- **PDF export of any page** — share-friendly, print-friendly, no
  external service.
- **Time-locked publish.** "Publish on Tuesday at 9am" — signed in
  advance with a future-dated key envelope, executed by a tiny
  systemd timer.
- **Localization with branch-per-locale + machine-translation
  diff** for review before publish.
- **Per-page A/B test** with statistical-significance reporting,
  zero-third-party (the variant choice lives in a deterministic
  hash of the request, no cookies).

**For the technical client:**

- **CRDT-backed multi-author editing** (like Notion / Figma) for
  real-time collaboration without a central server lock.
- **Loom-as-a-PWA** that works offline and syncs back later.
- **Custom typed CmsSection variants** declared in TOML/Rust, no
  fork required.
- **Webhook outbound on publish** for downstream automation.
- **Component state-matrix renderer** — every variant of every
  primitive rendered into an inspection grid for visual review.
- **Contract test against axe / Lighthouse** as a `loom audit`
  subcommand that fails if WCAG / perf budgets break.
- **End-to-end encryption** for editor-to-editor comments inside
  the admin portal (per-editor key, no central decryption ability).

**For the developer:**

- **Type-state phase pipeline shared with Forge** (T24) — the
  build order is a compile-time graph; trying to attest before
  signing is a compile error.
- **TLA+ specification** of the CmsPage edit + deploy state
  machine, refined to the Rust code (T27).
- **Mutation-testing CI gate** — `cargo mutants` survival rate <
  5% per crate, enforced.
- **Property-based fuzz on every parser** with corpus-driven
  regression replay.
- **Differential renderer** — render the same CmsPage with two
  different rendering backends (Maud-emit vs raw-format) and
  diff; mismatches are bugs.
- **Reproducible-build attestation** stored in a transparency log,
  consumable from the developer's own CI.

**For Claude Code (and other autonomous agents):**

- **Per-tenant Claude SSH bridge** (T46) — sandboxed via
  capability tokens, can't see other tenants, network-egress
  filtered to allow-list.
- **Annotator integration** — replay an annotated browser session
  through the editor so the agent can see what a human reviewer
  flagged.
- **Stable JSON-RPC API** for external orchestrators that don't
  speak Cargo.
- **MCP server** exposing Loom's capabilities (read CMS, write
  CMS, deploy, rollback, audit) as discoverable tools.
- **`loom record-session` + `loom replay-session`** — capture an
  agent's full edit trail for later audit + branch.
- **Cost / token / time budgets** per agent session, surfaced as
  a `loom budget` command.

**Cross-cutting supersociety capabilities:**

- **Hardware-attested deploys** (TPM-backed signing) so the deploy
  signature includes the host's measurement chain.
- **Post-quantum signature variant** — ML-DSA alongside Ed25519 in
  the attest manifest, dual-signed for forward-secrecy under
  algorithm-break scenarios.
- **Tor onion-service publish target** — a fully anonymous
  PlausiDen site at a `.onion` address.
- **IPFS / Hypercore decentralized publish target** —
  censorship-resistant mirror of any Loom site.
- **ActivityPub / Fediverse cross-publish** — every published
  page also lands as a federated post.
- **WebMention / IndieWeb support** — link backs from anyone
  citing the site, surfaced in the editor.
- **C2PA content provenance** — every image carries a signed
  Content Authenticity manifest tying it to the editor's key.
- **Memory-safe deserialization throughout** — no opaque parsers
  in the trust path; every parser is fuzz-targeted.
- **Compile-time CSP derivation** — the type system tracks what
  inline-style/script hashes a page emits, and the CSP is derived
  from the type, not the runtime output.

## 8. Future shape — three years out

Loom's job stays narrow: typed primitives + render layer + editor
+ deploy. What changes is how invisibly the supersociety stack
sits underneath.

A Mom-class client opens her browser, says "make me a bakery
website," and a local agent (running on her own machine, not in
the cloud) walks the Loom site-init protocol, asks four
clarifying questions, and ships a real signed accessible site to
her own onion-service endpoint in five minutes. Every primitive
she gets is the same primitive a developer would get. Every
guarantee she gets — WCAG, CSP, signed deploys, EXIF strip — is
the same guarantee an enterprise tenant gets. The supersociety
stack is not a Pro tier; it's the default and only tier.

Loom's interface to the rest of the PlausiDen ecosystem narrows:
PlausiDen-CMS handles multi-tenant storage + audit + admin
portal; PlausiDen-Forge handles build orchestration; PlausiDen-
Crawler handles runtime verification. Loom owns just the typed
render layer + the local editor + the local deploy primitive.
Every other concern lives in a sibling that depends on Loom but
that Loom does NOT depend on — same shape as today, just sharper.

A developer can fork any one of those four repos and replace it
without breaking the others, because every interface is a typed
schema (CmsPage / Phase / Finding / Journey) committed across all
four. The PlausiDen ecosystem is a federation of sharp tools, not
a monolith.

## 7. Acceptance criteria for "done"

Loom is **done** when:

1. Mom can build, edit, theme, audit, and publish a complete
   accessible site WITHOUT EVER seeing a stack trace.
2. A developer can fork the repo, add a new typed CmsSection
   variant in <100 lines (one renderer arm, one editor form arm,
   one form-POST handler arm, one regression test), and have it
   pass `loom lint` + `loom audit` immediately.
3. A Claude Code agent can spawn a fresh tenant, populate it with
   pages from a specification, run the build, fix every
   fixable finding, and deliver a signed bundle, in <5 minutes.
4. Every file carries an `AVP-PASS-N` annotation somewhere in
   its blame history.
5. `cargo mutants` survival rate is < 5% across the workspace.
6. Every public function has a property-based test with ≥10k cases
   where the input space is unbounded.
7. The page-shell + base-theme hold against axe-core + Lighthouse +
   manual screen-reader audit in BOTH light and dark mode.
8. Build outputs are bit-identical across machines (reproducible).
9. The threat model from `~/.claude/CLAUDE.md` (state-actor
   adversary, full breach, unlimited time) holds against the
   deployed system.

The verdict is always **STILL BROKEN** — shipping is risk
acceptance, not a declaration of correctness. The loop resumes
on the next commit.
