# AGENTS.md — PlausiDen-Loom

Orientation for any AI agent (Claude or otherwise) working in this repository. Read **before** writing any code or running any script.

> Per [[tool-starvation-anti-pattern]] doctrine: the failure mode that wastes most time is reaching for generic tools (bash, grep, find, curl, hand-rolled scripts) when a platform tool already exists. Stop and check first.

> Cross-repo orientation: see [PlausiDen-Forge/PLAUSIDEN_ECOSYSTEM.md](../PlausiDen-Forge/PLAUSIDEN_ECOSYSTEM.md) for how Loom relates to Forge / Crawler / Annotator / CMS / Canon / Meta / AVP-Doctrine / LFI / Forge-LFI.

> Tool surface for AI clients: see [PlausiDen-Forge/mcp/manifest.json](../PlausiDen-Forge/mcp/manifest.json) — declares `loom_validate` + `loom_sync` MCP tools.

---

## RULE 0 — The substrate is the only path (LOAD-BEARING)

Loom **is** the substrate's UI layer. Hand-coded CSS / HTML / JS in a Loom-consumer site repo is forbidden. Every site primitive flows through:

1. `loom-cms-render` — typed `CmsSection` variants + render impls.
2. `loom-tokens` — `skin.css` (the canonical CSS bundle) + theme tokens.
3. `loom-components` — composable primitive bundle.
4. `loom-lint` — CSS rules + view-class rules (RawColour = strict; RawSpacing / RawTime = warn).

**Forbidden:** hand-authoring `<style>` blocks, embedding `style="..."` attributes, defining custom CSS outside `loom-tokens/src/skin.css`, adding site-specific primitives. Per `[[substrate-only-path]]` doctrine.

**Canonical defaults (do NOT relitigate):**
- Typed CmsSection variants only; no `extra_classes: String` open fields per rule `prim-006`.
- Closed enums for variants per rule `prim-002`.
- Logical CSS properties (`padding-inline-start`, not `padding-left`) per rule `prim-003`.
- `@container` queries for primitive-internal responsiveness; `@media` only at page-shell level per rule `prim-004`.
- Tokens via `loom-tokens` CSS vars (`var(--loom-color-accent)`); no raw px/hex/rgb per rule `prim-007`.
- Default alignment: start; centered only on explicit `align: "center"` opt-in per rule `prim-009`.
- AMOLED dark theme uses true `#000000` background per memory `[[dark-theme-amoled-true-black]]`.

Genuine emergency? Substrate-bypass workflow is heavyweight + visible. Not a habit.

---

## RULE 1 — Look before you build (tool selection)

Before reaching for bash/grep/find/curl, hand-rolled scripts, or general-purpose libs:

1. **`loom --help`** — see if the subcommand exists.
2. **Scan the "Tool inventory" section below** — every operation has a typed Loom subcommand. Use it.
3. **Check `loom-cms-render/src/lib.rs`** for the `CmsSection` enum — every primitive is a typed variant.
4. **Check `loom-tokens/src/skin.css`** for the canonical CSS conventions before adding new styles.
5. **If none of the above** — propose an extension via capability-request, not by routing around the substrate.

---

## Tool inventory (use these — don't reinvent)

**Top-level CLI (`loom-cli` binary):**

```
loom site init --template <kind>      Scaffold a buildable site (cms/ + forge.toml + backends.toml).
loom edit serve                        Admin CMS editor with cookie auth (per task #141: also surfaces
                                      version-management UI in the future).
loom validate <cms-file>               Type-check cms/*.json against the schema.
loom sync [--regenerate]               Sync Loom-side artifacts into the Forge build. --regenerate
                                      re-emits skin.css from current token-variable definitions.
loom deploy hetzner                    Atomic remote deploy.
loom audit                             Lint + a11y + perf checks across the consumed primitive set.
loom doctor                            Diagnose Loom installation + dependency state.
loom critical-css                      Extract critical CSS for above-the-fold render.
loom journey-from-cms <cms-file>       Generate a Crawler journey from the CMS page composition.
loom hooks install                     Install per-tenant SSH bridge hooks.
```

**Crate roles:**

| Crate | Owns |
|-------|------|
| `loom-cli` | The `loom` binary. argv parsing + subcommand dispatch. |
| `loom-cms-render` | `CmsSection` enum + render impls; the typed UI primitive surface that Forge consumes. |
| `loom-tokens` | `skin.css` (canonical CSS bundle) + theme tokens. `SKIN_CSS` static loaded via `include_str!` per build.rs. |
| `loom-components` | Composable primitive bundle; UI atoms that primitives in `loom-cms-render` use. |
| `loom-lint` | CSS-level + view-class-level lint rules. |
| `loom-bridge` | Schema emission to Forge (`cms-schema.json`). |
| `loom-egui` | egui-based desktop editor (alternative to the web `loom edit serve`). |
| `loom-tui` | TUI editor. |
| `loom-icons` | Vendored SVG icon library; emission point for icon primitives. |
| `loom-assets` | Image bundle + alt-text validation. |

---

## Anti-patterns — do NOT do these

- ❌ Hand-rolling CSS in `static/<site>.css` → all styles flow through `loom-tokens/src/skin.css`. The render phase loads SKIN_CSS via `include_str!` + emits a build artifact.
- ❌ Adding `extra_classes: Option<String>` to a primitive → closed variant enum only (rule `prim-006`).
- ❌ Using `padding-left` / `text-align: left` → logical properties only: `padding-inline-start` / `text-align: start` (rule `prim-003`).
- ❌ Using `@media (min-width: 768px)` in primitive-internal CSS → `@container` query keyed on the primitive's container (rule `prim-004`).
- ❌ Hardcoding `1.5rem` / `#1A6B3C` / `4px` → loom-tokens vars: `var(--loom-space-4)` / `var(--loom-color-accent)` (rule `prim-007`).
- ❌ Naming a primitive after a site (`SkillShotsLeaderboard`) → generalize the shape so multiple sites can use it (rule `prim-012`).
- ❌ Skipping visual regression baselines on a new primitive → required per rule `prim-010`.
- ❌ Editing the rendered `static/loom-skin.css` directly → it's a build artifact regenerated by Forge from `loom-tokens::SKIN_CSS`. Edits go in `loom-tokens/src/skin.css`.

---

## Trait declarations

Per `[[trait-dag]]` (PlausiDen-AVP-Doctrine/TRAIT_DAG.md) + `loom-traits` crate in PlausiDen-Forge: every primitive declares which traits it satisfies. The 8 default-required traits for Loom Visible primitives (rule `prim-001`):

- `mobile-friendly` / `rtl-aware` / `reduced-motion-aware` / `theme-aware`
- `no-site-specific` / `manifested` / `versioned` / `doctrine-cited`

Interactive primitives cascade to add `focusable` / `keyboard-operable` / `screen-reader-accessible`.

Trait declarations land in `trait-manifest.{toml,json}` at the workspace root (task `#168` migrates existing primitives to declare). Forge's `trait_consistency` + `trait_implications` phases (PlausiDen-Forge tasks `#169` + `#171`) audit the manifest.

---

## Doctrine references

- `CLAUDE.md` — the canonical Loom doctrine (alongside this file)
- `PlausiDen-AVP-Doctrine` — AVP-2 protocol + rule database
- `PlausiDen-Forge/AGENTS.md` — the Forge-side companion
- `PlausiDen-Forge/skills/add-loom-primitive/SKILL.md` — the canonical playbook for adding a new primitive

---

## First steps when starting work in this repo

1. **Read `CLAUDE.md`** — the canonical Loom doctrine.
2. **Run `cargo build --workspace`** to confirm green baseline.
3. **Check `loom-cms-render/src/lib.rs` `CmsSection` enum** for existing primitives before adding one.
4. **Skim `loom-tokens/src/skin.css`** for the CSS layer conventions.
5. **State the goal** in one sentence — does it match a new primitive? a variant? a theme? a token? Reach for the typed thing first.

If you are about to invoke bash/grep/find/curl/awk/sed on Loom-managed state, stop and re-read RULE 0.
