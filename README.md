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
> + components + lint work; `loom audit` (visual regression) and
> `loom new` (page scaffolder) are stubs.

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
| `loom-cli` | Top-level `loom <subcommand>` binary — `lint`, `tokens`, `theme`, `cms`, `site`, `deploy`, `edit serve`, `bridge` (queued: `audit`, `new`). |

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
6. **CLI** (`loom`) wires it all together. Subcommands today:
   `loom lint`, `loom tokens`, `loom theme contrast`, `loom cms render`,
   `loom site init`, `loom edit serve`, `loom deploy hetzner`.
   Queued: `loom audit` (Playwright visual diff), `loom new page`
   (template scaffolder), `loom bridge serve` (T46.next).

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
./target/release/loom edit serve --site my-site/ --bind 127.0.0.1:8080
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
- ✅ **CLI** — `lint`, `tokens`, `theme contrast`, `cms render`,
  `site init`, `edit serve`, `deploy hetzner`.
- 🚧 **icons** — Lucide-derived UI glyphs shipped; brand-logo set
  queued as separate `loom-brand-icons` crate.
- ⏳ **`loom audit`** — Playwright visual regression at every
  breakpoint, screenshot diff vs. baseline. Stub today.
- ⏳ **`loom new page`** — template scaffolder. Stub today.
- 📋 **Token export** — GTK / Adwaita CSS / Jetpack Compose theme
  generators. Tokens are language-neutral JSON; future generators
  consume them.
- 📋 **`loom-brand-icons`** — vetted brand SVG sources only; never
  user-uploaded markup.

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
