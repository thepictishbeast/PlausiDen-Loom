# TOOLS.md — PlausiDen-Loom

Canonical command index for the `loom` CLI + the loom-* crate surface. Grepable single source of truth. PRs that add/remove/change a subcommand update this file in the same commit (per AVP-Doctrine rule `docs-007`).

> Cross-repo TOOLS reference: see [../PlausiDen-Forge/TOOLS.md](../PlausiDen-Forge/TOOLS.md) for the Forge-side command index (the primary consumer of Loom).

Run `loom --help` for the latest live surface.

---

## Site lifecycle

```
loom site init --template <kind>      Scaffold a buildable site (cms/ + forge.toml + backends.toml).
                                      Kinds: business / personal_blog / ecommerce / saas / nonprofit
                                            / government / education / anonymous_publishing.
```

---

## CMS authoring + editing

```
loom edit serve                       Admin CMS editor with cookie auth.
                                      Per #141: also surfaces version-management UI.

loom validate <cms-file>              Type-check cms/*.json against the canonical
                                      CmsPage schema (mirrored from loom-cms-render).

loom critical-css                     Extract critical CSS for above-the-fold render.

loom journey-from-cms <cms-file>      Generate a Crawler journey from the CMS page
                                      composition (verify rendering matches intent).
```

---

## Build + sync

```
loom sync                             Sync Loom-side artifacts into the consuming Forge build.
loom sync --regenerate                Re-emit skin.css from current token-variable definitions.
```

---

## Deploy

```
loom deploy hetzner                   Atomic remote deploy to the Hetzner target.
                                      (Other providers land via separate sub-commands.)
```

---

## Quality + diagnostics

```
loom audit                            Lint + a11y + perf across the consumed primitive set.
loom doctor                           Diagnose Loom installation + dependency state.
loom hooks install                    Install per-tenant SSH bridge hooks (admin portal).
```

---

## Crate-level direct invocation

For tasks that don't have a `loom` subcommand wrapper, call the crate directly via `cargo`:

```
cargo run -p loom-cli                 # Direct CLI invocation
cargo run -p loom-bridge -- emit-schema > cms-schema.json   # Regenerate the CmsSection schema
cargo test --workspace                # Loom-wide test suite
cargo build --release -p loom-cli     # Build the loom binary
```

---

## Anti-patterns — DO NOT do these

- ❌ `loom site init` then hand-editing scaffolded files → use `loom edit serve` for content; CmsSection variants for shape.
- ❌ `cp ../skin.css static/` → use `loom sync --regenerate`; the canonical skin.css is `include_str!`'d into the build.
- ❌ Direct chromiumoxide invocation → use `crawler --journey ...` from PlausiDen-Crawler for browser automation.
- ❌ `python3 -m http.server` to test static output → use `loom edit serve` (or run Forge build + Caddy).

---

## See also

- `AGENTS.md` — orientation including Rule 0 + Rule 1.
- `CLAUDE.md` — the canonical Loom doctrine.
- `../PlausiDen-Forge/TOOLS.md` — Forge-side TOOLS index.
- `../PlausiDen-Forge/mcp/manifest.json` — `loom_validate` + `loom_sync` MCP tools.
- `loom --help` — live CLI surface (most-current authority).
