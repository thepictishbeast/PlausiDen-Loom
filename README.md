> # ⚠️ DO NOT USE — UNVERIFIED — UNSAFE ⚠️
>
> This software is **unverified and unsafe for any production use**.
> It is published publicly only for transparency, third-party audit,
> and reproducibility. Treat every commit as guilty until proven
> innocent.
>
> By using this code you accept:
> - **No warranty** of any kind, express or implied.
> - **No fitness** for any particular purpose.
> - **No guarantee** of correctness, safety, or freedom from defects.
> - **Zero liability** on the maintainer for any damages — data loss,
>   security compromise, financial loss, or any consequential damages.
>
> The code is under active engineering development per the
> [Adversarial Validation Protocol v2](https://github.com/thepictishbeast/PlausiDen-AVP-Doctrine/blob/main/AVP2_PROTOCOL.md).
> Every commit's default verdict is **STILL BROKEN**. AVP-2 requires
> a minimum of 36 verification passes before a `SHIP-DECISION:`
> annotation may be considered. **No commit in this repository has
> reached `SHIP-DECISION:` status.**

# PlausiDen-Loom

A design-system-as-code for the PlausiDen ecosystem. Typed tokens,
constrained components, a typed CMS-rendering bridge, lint + audit
CLIs, and a sandboxed per-tenant SSH bridge that routes admin-portal
chat sessions to jailed Claude Code invocations. Built so AI agents
(Claude, others) can ship UI without human babysitting and without
fragmenting the visual system.

> ## ⚠ Status: pre-1.0, AVP-2 in flight — NOT production-ready
>
> This codebase is published publicly for transparency, third-party
> audit, and reproducibility — **not** as a shipped product. Per the
> [Adversarial Validation Protocol v2](https://github.com/thepictishbeast/PlausiDen-AVP-Doctrine/blob/main/AVP2_PROTOCOL.md),
> every commit is treated as guilty until proven innocent via a
> minimum of 36 verification passes. The current verdict is **STILL
> BROKEN** — that's the protocol's default and changes only with an
> explicit `SHIP-DECISION:` annotation listing accepted residual risk.
>
> APIs, file layout, CLI flags, and on-disk formats can and will
> change between commits. Tests pass locally; CI may or may not be
> green at any given moment (see Actions tab). Treat this as a
> live engineering tree, not a release.
>
> Licensed under [FSL-1.1-MIT](./LICENSE) — source-available with
> a 2-year competitor-restriction window, after which it converts
> automatically to MIT.
>
> Loom is `v0` per the doctrine in [CLAUDE.md](./CLAUDE.md): tokens
> + components + lint work; `loom audit` (visual regression via
> Crawler) and `loom new` (page scaffolder) are shipping —
> previously marked as stubs in this status note.

## What this replaces

- **WordPress / WYSIWYG CMSes** — runtime editors that produce
  inconsistent output.
- **Ad-hoc Tailwind classes scattered across views** — every page
  reinvents spacing, colors, sizes.
- **"AI babysitter" reviews** — a human re-reading every generated
  page to catch the same five visual mistakes.

It's not a runtime CMS. It's a *compile-time doctrine* that any agent
or developer points at, with CLI checks that fail closed.

## Crate map

| Crate | Role |
|-------|------|
| `loom-tokens` | Palette, spacing scale, breakpoints, font scale, radius scale. Read-only constants. Adding a token is a doctrine change. |
| `loom-components` | Typed `Button`, `Section`, `Card`, etc. Every prop is a constrained enum. No `extra_classes` escape hatch. |
| `loom-cms-render` | **Typed CMS rendering bridge.** Translates `CmsPage` / `CmsSection` JSON (schema lives in PlausiDen-CMS) into `maud::Markup` via Loom primitives. The single render path for CMS-driven sites; Forge invokes it as its `phase_render`. |
| `loom-bridge` | **Sandboxed per-tenant Claude Code SSH bridge (T46).** Routes admin-portal chat sessions to a jailed `claude --resume` under a per-tenant unix user with cgroup CPU+memory ceilings + an Anthropic+GitHub-only egress allowlist. ed25519 auth, russh transport, bwrap sandboxing. |
| `loom-icons` | Curated Lucide-derived SVG icon set, served as typed Rust enums (no `<svg>` string drops). Brand logos live in a separate `loom-brand-icons` crate (queued). |
| `loom-lint` | CLI: walks `*.rs` views and refuses raw class strings outside an allowlist (components crate + a few sanctioned chrome files). |
| `loom-cli` | Top-level `loom <subcommand>` binary — see the full subcommand list below (30+ subcommands shipping). |

## How it works

1. **Tokens** are the read-only single source of truth (palette,
   spacing, breakpoints, fonts, radii). Adding a token is a doctrine
   review.
2. **Components** compose tokens into typed primitives. Every prop
   is a constrained enum. There is no `extra_classes` escape hatch.
3. **CMS rendering** is a one-way bridge: typed `CmsPage` JSON →
   `maud::Markup` → HTML. `loom-cms-render` owns this transform; no
   other code emits Loom-styled markup from CMS data.
4. **Lint** walks `*.rs` views and refuses raw Tailwind class
   strings outside the allowlist.
5. **SSH bridge** (T46): the admin portal serves a "chat with Claude
   Code" panel per tenant. The bridge:
   - Resolves the incoming SSH key against the cookie session.
   - Spawns `claude --resume` in a per-tenant unix user.
   - Wraps the spawn in `bwrap` + cgroup v2 CPU+memory limits.
   - Restricts egress to Anthropic API + GitHub.
   - Streams bidirectional stdio over the russh transport.
6. **CLI** (`loom`) wires it all together. **30+ subcommands
   shipping today** across design-system / CMS / build /
   operations / governance surfaces — see "CLI subcommands"
   below for the full list.

## CLI subcommands

`loom --help` is the canonical source. Subcommand families
currently shipping:

**Design system core**

- `loom lint` — refuses raw class strings outside the allowlist
- `loom tokens` — print design tokens as JSON
- `loom report` — drift report: raw class strings grouped by
  file, includes previously-allowlisted files (migration burn-
  down dashboard)
- `loom audit` — visual-regression via PlausiDen-Crawler;
  screenshots every breakpoint declared in `loom-tokens`
- `loom state-matrix` — T34: emit a self-contained HTML page
  rendering every `CmsSection` variant + named state into a
  single grid (one output per theme)
- `loom new` — scaffold a new page view from a sanctioned
  template (stub composed entirely from Loom primitives)
- `loom doctor` — verify the design-system doctrine document is
  in sync with the code it claims to govern
- `loom theme` — inspect + validate the theme system
- `loom critical-css` — extract the critical-CSS subset for
  first paint (drops component-specific rules into the deferred
  sheet)

**Token exporters (cross-platform)**

- `loom gtk-theme` — emit GTK 4 CSS theme from tokens
- `loom css` — emit every token as CSS custom properties under
  `:root` and `:root[data-theme="dark"]`
- `loom egui` — emit every token as Rust `pub const` blocks for
  inclusion in an egui-driven app (Atrium etc.)

**Backend manifest (capability-manifest pattern)**

- `loom backend-stub` — scaffold a Rust handler stub for one
  `backends.toml` key (typed Request/Response + axum-style
  handler signature + test stub)
- `loom backend-stub-all` — T19 mass-mint mode: walk
  `backends.toml`, scaffold every entry whose `impl_files` is
  empty
- `loom backend-list` — list every backend with its impl status
  (STUB vs IMPL)

**CMS / editor / sites**

- `loom site` — T41 + T48: scaffold a new site from a bundled
  template
- `loom edit-serve` — T42: typed CMS editor server
- `loom import` — T63: import existing static HTML into typed
  `CmsPage` JSON
- `loom revisions` — T76: inspect + restore CMS revision
  backups from the auto-save snapshot pipeline
- `loom auth` — T43: admin auth management

**Operational reporting (T76)**

- `loom report-stats` — aggregate cycle-63 `violations.jsonl`
  into per-kind summary stats
- `loom report-tail` — tail the cycle-63 `violations.jsonl` log
- `loom report-review` — operator triage:
  `list | ack | dismiss | status`

**Deploy + attestation**

- `loom deploy` — T47: atomic deploy with signed manifest +
  rollback
- `loom attest` — T47c: Ed25519 attestation key management for
  deploy manifests. **Same shape as Forge's `forge attest`**.

## Try it

```sh
# Build
cargo build --release -p loom-cli

# See the design tokens
./target/release/loom tokens

# Lint a site
./target/release/loom lint /path/to/your/site

# Render a CMS page
./target/release/loom cms render --in cms/index.json --out dist/

# Theme contrast check (WCAG AA per theme)
./target/release/loom theme contrast --skin static/skin.css --min-ratio 4.5

# Site scaffolder (T41)
./target/release/loom site init --template hero my-site/

# Run admin editor (T42 — cookie-session auth, T43)
./target/release/loom edit-serve --site my-site/ --bind 127.0.0.1:8080

# Visual regression via Crawler (was queued in earlier README; shipping)
./target/release/loom audit --site my-site/

# Scaffold a new page view (was queued in earlier README; shipping)
./target/release/loom new --name about --template legal

# Emit token-derived stylesheets for downstream consumers
./target/release/loom css > my-site/static/loom-tokens.css
./target/release/loom gtk-theme > ~/.config/gtk-4.0/loom.css

# Initialize the deploy attestation chain
./target/release/loom attest init
```

When `loom lint` reports violations, the fix is always one of:

- Use a typed component from `loom-components`.
- Move the styling into a new typed component (and add it to the
  components crate).
- Add the file path to the lint allowlist (rare, requires doctrine
  review).

Never disable the lint.

## Read this before generating UI

[`CLAUDE.md`](./CLAUDE.md) is the doctrine. Every AI agent (and every
human contributor) reads it before touching UI code in any
PlausiDen-* repo.

## Status (current)

- ✅ **tokens** — palette + spacing + breakpoints + font + radius scales
  shipped. AMOLED-aware dark theme (true `#000000` background) is the
  default `tokens-dark.json`.
- ✅ **components** — `Button`, `Section`, `Card`, `Hero`, `KvPair`,
  `Banner`, `Group`, `Heading`, `Letter`, `Quote`, `CardFeed`. More
  variants queued per the [Forge dedup table](https://github.com/thepictishbeast/PlausiDen-Forge/blob/main/docs/DEDUP_TABLE.md).
- ✅ **lint** — walks views, flags raw classes outside allowlist.
- ✅ **cms-render** — typed `CmsPage` → Loom markup bridge.
- ✅ **bridge** — T46 SSH bridge end-to-end (200 tests with
  `russh-transport` feature; auth → resolve → prepare → spawn →
  bidirectional stdio). Pending: real-bwrap host smoke test,
  `loom bridge serve` CLI subcommand, ops runbook, admin-portal
  consumer integration, Merkle audit log.
- ✅ **CLI** — 30+ subcommands across design-system / CMS / build
  / operations / governance surfaces. See "CLI subcommands"
  above. Highlights: `lint`, `tokens`, `theme`, `cms render`,
  `site init`, `edit-serve`, `deploy`, `attest`, `audit`, `new`,
  `state-matrix`, `doctor`, `critical-css`, `gtk-theme`, `css`,
  `egui`, `backend-stub`/`-all`/`-list`, `import`, `revisions`,
  `auth`, `report-stats`/`-tail`/`-review`.
- ✅ **icons** — Lucide-derived UI glyphs shipped; brand-logo set
  queued as separate `loom-brand-icons` crate.
- ✅ **`loom audit`** — visual regression via PlausiDen-Crawler.
  Screenshots every breakpoint declared in `loom-tokens`. The
  Crawler does the actual diffing; this subcommand is a typed
  entry point locking the journey shape so `loom audit` is the
  one canonical invocation.
- ✅ **`loom new`** — page-view scaffolder from sanctioned
  templates. Emits a stub composed entirely from Loom primitives,
  plus the `pub mod <name>;` line. Refuses to overwrite existing.
- ✅ **Token exporters** — `loom css` (web), `loom gtk-theme`
  (GTK 4), `loom egui` (Rust constants for egui apps). Adwaita
  + Jetpack Compose exporters queued.
- ✅ **`loom backend-stub` / `-all` / `-list`** — capability-
  manifest pattern integration. `backends.toml` entries become
  scaffolded Rust handler stubs with typed Request/Response +
  axum-style signatures + test stubs. `loom backend-stub-all`
  walks the whole file. **This is the manifest-projection
  pattern in shipping form** — see
  [PlausiDen-Forge/docs/PLATFORM_ROADMAP.md §3](https://github.com/thepictishbeast/PlausiDen-Forge/blob/main/docs/PLATFORM_ROADMAP.md#3-the-manifest-layer--the-architectural-keystone).
- ✅ **`loom attest`** — T47c: Ed25519 attestation key
  management for deploy manifests. Same shape as Forge's
  `forge attest`; shared substrate for cryptographic build
  + deploy attestation.
- ✅ **`loom doctor`** — verifies the design-system doctrine
  document is in sync with the code it claims to govern. Fails
  if `CLAUDE.md` is missing load-bearing sections, references
  primitives that don't exist, or has drifted from the
  structural shape we publish.
- 🚧 **bridge** — T46 SSH bridge end-to-end (200 tests with
  `russh-transport` feature). Pending: real-bwrap host smoke
  test, `loom bridge serve` CLI subcommand, ops runbook,
  admin-portal consumer integration, Merkle audit log.
- 📋 **`loom-brand-icons`** — vetted brand SVG sources only;
  never user-uploaded markup.
- 📋 **Jetpack Compose / Adwaita token exporters** — queued
  on top of the existing `css` / `gtk-theme` / `egui` exporter
  pattern.

## Ecosystem integration

Loom is one of six PlausiDen tools. See
[`PlausiDen-Forge/README.md`](https://github.com/thepictishbeast/PlausiDen-Forge/blob/main/README.md#ecosystem-integration)
for the full pipe diagram. Loom's role:

- **CMS schema → Loom render** via `loom-cms-render::render_page`.
- **Loom tokens → Forge phases** (`theme_consistency`, `theme_contrast`,
  `dual_theme`) enforce tokens at build time.
- **Loom design doctrine → all PlausiDen frontends** consume the
  same `loom-tokens` JSON; no fork.
- **Admin portal → loom-bridge** routes Claude Code chat to
  jailed per-tenant invocations.

## Test matrix

Per the [Forge 24-combo test matrix](https://github.com/thepictishbeast/PlausiDen-Forge/blob/main/docs/TESTING.md):
every Loom-rendered output is tested across 3 themes (light, dark,
dark-amoled) × 2 viewports × 2 modes (static/dynamic) × 2 debug
modes = 24 runs per page. AMOLED dark is the default for OLED
battery savings; sites can opt to a muted-dark variant via
`?_theme=dark` URL override.

## ISO/IEC standards

- **ISO 8601** — date/time formatting
- **ISO 639-1** — `<html lang="en">`
- **ISO/IEC 25010** — software quality attributes noted in commits
- **ISO/IEC 40500:2012 / WCAG 2.1 AA** — accessibility floor
  enforced by Loom tokens + components by default

## License

[FSL-1.1-MIT](./LICENSE) — Functional Source License v1.1 with
MIT future license. Source-available with a 2-year competitor-
restriction window, after which it converts automatically to MIT.
