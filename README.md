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
constrained components, lint + audit CLIs. Built so AI agents (Claude,
others) can ship UI without human babysitting and without fragmenting
the visual system.

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

## How it works

1. **Tokens** (`loom-tokens`) are the read-only single source of truth:
   palette, spacing scale, breakpoints, font scale, radii. Adding a
   token is a doctrine review.
2. **Components** (`loom-components`) compose tokens into typed
   primitives — `Button`, `Section`. Every prop is a constrained
   enum. There is no `extra_classes` escape hatch.
3. **Lint** (`loom-lint`) walks `*.rs` views and refuses raw
   Tailwind class strings outside an allowlist (components crate, a
   few sanctioned chrome files).
4. **CLI** (`loom`) wires it together: `loom lint`, `loom tokens`,
   eventually `loom audit` and `loom new`.

## Try it

```sh
# Build
cargo build --release -p loom-cli

# See the design tokens
./target/release/loom tokens

# Lint a site
./target/release/loom lint /path/to/your/site
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

## Roadmap

- v0 (this commit): tokens, components (Button + Section), lint, CLI.
- v0.1: more components (Card, Hero, Form primitives, Nav).
- v0.2: `loom new page <name> --template <T>` scaffolder.
- v0.3: `loom audit` — Playwright at every breakpoint, screenshot diff
  vs. baseline.
- v0.4: cross-platform token export (GTK / Adwaita CSS, Jetpack
  Compose theme).
- v1: `plausiden-site` fully migrated to `loom-components`; lint runs
  as a CI gate; visual regression catches every drift.

## License

AGPL-3.0-or-later.
