# PlausiDen-Loom — UI doctrine for AI agents

If you are an AI agent (Claude or otherwise) about to touch UI code in
any PlausiDen project, **read this file before generating markup**.
Every rule here exists because skipping it has produced bad output in
the past.

## The single rule

> **Never write a raw class string, raw CSS, or raw inline style. Use
> a typed `loom-components` primitive, or extend the design system.**

If your first instinct is to write `class="bg-blue-500 px-4 py-2 ..."`,
stop. Either:

1. **A typed component already exists** — call it.
2. **A token already exists** — use it through the typed component API.
3. **Neither exists** — propose an extension to the design system as a
   *separate* PR before writing UI that uses it.

The third option is rare and deliberate. If you find yourself doing it
twice in one session, the design system has a real gap; surface it
explicitly and stop.

## Why this exists

Past agents (including Claude in earlier sessions) burned hours of
token cost rebuilding the same UI inconsistently across pages, then
needed human review to reconcile. Every "small visual tweak" fragmented
the system. This crate is the single source of truth so:

- A new page is composed from primitives in 30 lines, not 300.
- A design tweak edits one token; every page rebuilds correctly.
- Visual regression caught at lint-time, not by a human reviewer
  spotting a 4px margin difference three weeks later.

## Crate map

| Crate | What's in it | When you touch it |
|-------|--------------|-------------------|
| `loom-tokens` | Palette, spacing scale, breakpoints, font scale, radius scale. **Read-only constants.** | Almost never — adding a token is a doctrine change. |
| `loom-components` | Typed `Button`, `Card`, `Section`, `Hero`, etc. Every prop is a constrained enum. | When proposing new variants; usually no. |
| `loom-lint` | CLI: `loom-lint <crate>`. Walks `*.rs` view files. Refuses raw class strings outside an allowlist. | Adding a check; never to disable one. |
| `loom-audit` | (Coming) CLI: visual-regression at every breakpoint. | Not yet. |
| `loom-cli` | Top-level entrypoint: `loom <subcommand>`. | Adding new subcommands. |

## Hard rules

1. **No raw `class=` strings outside `loom-components`.** Every class
   string lives behind a typed prop. Lint will fail your PR.
2. **No magic numbers in spacing, color, font-size, or radius.** Use
   the token enums.
3. **Every component variant is enumerated.** No "open" props that
   accept arbitrary strings. If a caller wants something not in the
   enum, the caller is wrong (or the enum is missing a real variant).
4. **Mobile + desktop always tested.** Components render at every
   declared breakpoint; if a variant breaks at one size it breaks at all.
5. **Accessible by default.** Every interactive element has a typed
   accessible-name slot. Forgetting it is a lint error, not a runtime
   bug.

## How to add a new page

1. `loom new page <name> --template <hero|legal|article|blog-post>` —
   scaffolds a valid skeleton.
2. Compose primitives from `loom-components`. Don't write Maud or
   raw HTML directly.
3. `cargo check` — must pass.
4. `loom lint` — must pass.
5. `loom audit` (when it lands) — must pass.

## How to extend the design system

1. Open a separate proposal PR.
2. Add the new token / variant / component primitive.
3. Add a test that pins its rendered output (snapshot or assertion).
4. Update this file's "what exists" section.
5. Only then start using it in pages.

## What this is not

- **Not a CMS.** No runtime editing. Content changes are PRs.
- **Not WordPress.** No themes, plugins, or admin panel.
- **Not a Tailwind alternative.** The compiled output still uses
  Tailwind classes; the doctrine is on which classes appear and
  how they're composed.
- **Not a framework lock-in.** Tokens are language-neutral JSON. A
  future GTK / Jetpack Compose generator can consume them too.

## Status: v0

This is a fresh repo. The pieces here are the floor, not the ceiling:

- `loom-tokens`: typed palette, spacing, breakpoints, font, radius.
- `loom-components`: `Button` (3 variants × 3 sizes), more landing in
  follow-up.
- `loom-lint`: walks views, flags raw classes outside an allowlist.
- `loom-cli`: `loom lint` works; `loom new`, `loom audit` are stubs.
