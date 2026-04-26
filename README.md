# PlausiDen-Loom

A design-system-as-code for the PlausiDen ecosystem. Typed tokens,
constrained components, lint + audit CLIs. Built so AI agents (Claude,
others) can ship UI without human babysitting and without fragmenting
the visual system.

> Status: v0 scaffold. Tokens + components + lint work. Audit (visual
> regression) and `new` (page scaffolder) are stubs.

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
