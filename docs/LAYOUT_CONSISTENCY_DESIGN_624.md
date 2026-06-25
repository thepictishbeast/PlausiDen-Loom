# Layout Consistency by Construction (Forge/Loom) — Task #624

**One content-width source of truth; header, every body section, and the footer derive from it.**

Status: design / actionable. Scope: `loom-tokens/src/skin.css`, `loom-cms-render/src/lib.rs`, `loom-lint`.
Doctrine alignment: typed tokens, N-orientation manifest, consistency-by-construction, byte-identical/SRI posture, backward-compat version discipline.

---

## 1. The cross-system pattern

Four reference systems (WordPress block themes, Tailwind/Bootstrap, Wix/Squarespace, modern CSS-token methodology) converge on the **same two-part mechanism**. None of them makes width "automatic" — consistency is the product of two ingredients applied together:

1. **A single content-width source of truth, declared once, read everywhere by indirection.**
   - WordPress: `theme.json` `settings.layout.contentSize`/`wideSize` → emitted once as `--wp--style--global--content-size`/`--wide-size` on `body`; layout-support rules reference them only via `var()`, never literals.
   - Squarespace / Wix Studio: `--sqs-site-max-width` + `--sqs-site-gutter` (or one Site-Styles max-width) declared on a high ancestor and propagated by inheritance; full-bleed is a *local override of the same two tokens*, not a competing literal.
   - Tailwind: `--breakpoint-*` doubles as the container-max ramp (container-max == breakpoint at each step — the SSOT elegance); Bootstrap derives container edge-padding **and** grid gutters from one `--bs-gutter-x`.
   - The win is **indirection**: semantic selector → `var()` → one declaration. Change the one value and every region reflows at once.

2. **One shared centering primitive that every region runs identically.**
   - This is the part frameworks *don't* enforce — Tailwind makes you hand-repeat `mx-auto px-*` on header/main/footer; Bootstrap bakes centering into `.container` but freezes the ladder at build time. Drift happens wherever a region uses a different wrapper or a literal.
   - The token alone is **not** sufficient: a region that hardcodes `max-width: 80rem` ignores the token and diverges silently. Consistency = *one token* **plus** *one centering formula applied by every chrome region*.

Two orthogonal layers, kept separate by every mature system:

| Layer | Owns | Scope | Loom name |
|---|---|---|---|
| **Width envelope** | content max-width + gutter + centering | site-wide, inherited (SSOT) | `--loom-page-content-max` |
| **Interior grid / measure** | columns, rows, gap, reading measure (ch) | per-section, local | `SectionGrid`, prose `65–70ch` |

Measure (reading line-length, in `ch`) is a *different axis* from layout width (in `rem`) and must never be folded into the envelope.

---

## 2. The Forge/Loom design

### 2.1 The single token (already exists, already correct)

`loom-tokens/src/skin.css` already declares the SSOT on `<body>`, dispatched by a data-attribute (the canonical knob — keep it):

```css
body                                   { --loom-page-content-max: 64rem; }   /* L13374 */
body[data-content-width="narrow"]      { --loom-page-content-max: 42rem; }
body[data-content-width="comfortable"] { --loom-page-content-max: 64rem; }
body[data-content-width="roomy"]       { --loom-page-content-max: 70rem; }
body[data-content-width="wide"]        { --loom-page-content-max: 90rem; }
body[data-content-width="full"]        { --loom-page-content-max: 100%; }
```

The typed driver already exists in `loom-cms-render/src/lib.rs`:

- `enum ContentWidth { Narrow, #[default] Comfortable, Roomy, Wide, Full }` (L6902) → `attr_value()` slug → `data-content-width` on `<body>`. **`Comfortable` is the byte-identical default and emits the baseline value.**
- Page- and tenant-level `content_width: Option<ContentWidth>` fields merged via `fill_opt!(content_width)` (L477), resolved at L22325 with `unwrap_or_default()` → `Comfortable`.

This is the WordPress `contentSize`/`wideSize` analog, typed: an off-scale width *cannot be expressed* — there is no `Other(String)` variant, so an invalid width is a compile error, not a silently-ignored class. **No new token, no new enum is required for the core fix.** #624 is about making the rest of the CSS *honor* the token that already exists.

### 2.2 The shared centering primitive (already exists for 3 of 4 regions)

Three chrome regions already run the identical envelope formula:

```css
padding-inline: max(<gutter>, calc((100% - var(--loom-page-content-max, 64rem)) / 2));
```

- `.loom-page` (body band) — skin.css L885
- `header.loom-page-header` — L13381
- `.loom-utility-strip` — L13417

Backgrounds/borders stay full-bleed; only the inner content column is inset by symmetric inline padding, so all three line up on the same left/right edge and track the one token together. The L874–885 comment records *why*: an earlier `max-width + margin:auto` body wrapper double-counted the gutter against a `padding-inline` header and left the body narrower — fixed by making both use the identical math. **This is the centering primitive. The remaining work is to route the regions that don't use it through it.**

### 2.3 Per-tenant + per-page configurability (preserve)

- **Per-page**: `CmsPage.content_width: Option<ContentWidth>`.
- **Per-tenant / site default**: same field at the site-config layer, merged by `fill_opt!`. Page overrides tenant; tenant overrides the `Comfortable` baseline.
- **Default**: unset → `Comfortable` → 64rem, byte-identical to current output.
- Tenant overrides re-point the token via the data-attribute (or a scoped `:root` in the external tenant stylesheet) — **never** by editing primitive token names/formulas (matches per-tenant-corpora additive doctrine + tenant-style-must-be-external CSP rule).

### 2.4 Full-bleed as a typed seam, not ad-hoc CSS (forward-looking, optional for #624)

Model the Squarespace/Wix override pattern as an enum rather than scattered `!important`:

```rust
enum SectionWidth {
    Contained,                       // inherits the envelope (default)
    FullBleed,                       // scoped override of the SAME token: --loom-page-content-max: 100%
    BleedBackgroundContainedContent, // 100vw band, inner wrapper reads the envelope (Wix strip / SQSP section)
}
```

Overrides are still expressed *through the single source* (re-pointing the one token), never as a competing literal — exactly Squarespace's `#sectionID .fluid-engine { --sqs-site-max-width: 100vw }` trick. This supersedes the `.loom-page-header[data-nav-bg-role] { padding: 0 !important }` escape hatch (L13390), which currently enforces the invariant by convention and can be `!important`-ed around.

---

## 3. Migration (the #624 change set)

The hardcoded widths are **not one uniform replacement** — bucket them by intent. Misclassifying a sub-column as the page envelope would widen it and break both design and byte-identical output.

### Bucket A — page-column literals → route to the token (byte-identical at default)

These re-pin the page column to a literal instead of reading the SSOT. A page set to `Wide` (90rem) currently gets a 90rem body but these elements stay 64rem.

| File:line | Current | Change to |
|---|---|---|
| `skin.css:7061` `.loom-image-hero__inner` | `max-width: 64rem;` | `max-width: var(--loom-page-content-max, 64rem);` |
| `skin.css:8705` `.loom-container.w-comfortable` | `max-width: 64rem;` | `max-width: var(--loom-page-content-max, 64rem);` |

Byte-identical for the default `Comfortable` (token already resolves to 64rem); correct for non-default widths. The `.loom-container.w-narrow`/`.w-wide`/`.w-full` variants stay explicit — they are an *intentional per-section override* of the page width (the `ContainerWidth` enum at L5590, render arm L11192), analogous to WordPress `.alignwide`/`.alignfull`. Only `w-comfortable` (= "inherit the page default") should track the token.

### Bucket B — the footer leak → route to the token (VISIBLE change, classify)

The rich chrome footer does **not** use the envelope formula; it hardcodes its own column:

| File:line | Current |
|---|---|
| `skin.css:6479` `.loom-page-footer__columns` | `max-width: 80rem; margin: 0 auto;` |
| `skin.css:6637 / 6661 / 6680` (sibling footer rows) | `max-width: 80rem;` |

A page set to `narrow` (42rem) or `roomy` (70rem) gets an 80rem footer band — visibly wider than the header/body above it. This is **the leak the cross-system pattern is meant to close.** Replace each with the shared envelope inset:

```css
padding-inline: max(1.25rem, calc((100% - var(--loom-page-content-max, 64rem)) / 2));
margin-inline: 0;   /* drop the max-width + margin:auto; the formula centers */
```

(Also `.loom-footer` at L2951, the simple footer, has no content-max participation — give it the same inset.)

**Version discipline (BACKWARD_COMPAT 4-category):** this is **Category 3 — auto-migration / visible change**, NOT invisible. Default-width pages change footer column 80rem → 64rem. Call it out in the changelog and the migration registry; it is the deliberate price of header/body/footer alignment. Re-verify ProsperityClub + next.plausiden.com footers after the change.

### Bucket C — intentional sub-widths and measures → LEAVE (do NOT route through the envelope)

These are deliberately narrower than the page column or are reading-measures (`ch`), not layout widths. Folding them into `--loom-page-content-max` would widen them and break design + byte-identical:

| File:line | Value | Why it stays |
|---|---|---|
| `skin.css:8289` `.loom-cta-band__inner` | `56rem` | intentional sub-column, narrower than the page |
| `skin.css:7214` hero lede | `60ch` | reading measure (ch), not width |
| `skin.css:576 / 1442` prose | `70ch` | reading measure |
| `skin.css:4185` `.loom-center[data-max="prose"]` | `65ch` | reading measure |

If a future need arises, promote the reading measure to a *separate* named token (`--loom-measure`, ~66ch) with its own typed primitive — kept distinct from `--loom-page-content-max` so width and measure can never be conflated. Not required for #624.

### Fix C2 — align the envelope `var()` fallbacks (the real "None→wider" bug)

The task brief's "None→Wide override" does **not** match the Rust resolve: `page.content_width.unwrap_or_default()` (L22325) yields `Comfortable`, not `Wide`, and both `ContentWidth` and `ContainerWidth` default to `Comfortable`. There is **no active None→Wide path**, and a step that forced `None → Wide` would break the Comfortable=64rem byte-identical default — do not do that.

The actual latent divergence is an inconsistent `var()` fallback across the three envelope sites:

| Site | Fallback when `--loom-page-content-max` is unset |
|---|---|
| `.loom-page` body (L885) | `var(--loom-break-xl)` = **80rem** |
| header (L13381) | `64rem` |
| utility-strip (L13417) | `64rem` |

If the token ever fails to set (no `data-content-width`, or a future code path that omits the body declaration), the **body resolves to an 80rem column while the chrome resolves to 64rem** — a 16rem "None→wider" divergence between body and header. It is latent today (the body declaration at L13374 always sets the var), but it is the most likely referent for the brief's wording and it is a genuine consistency hole. **Fix:** make all three fallbacks identical to the Comfortable baseline:

```css
/* L885 */ padding-inline: max(1.25rem, calc((100% - var(--loom-page-content-max, 64rem)) / 2));
```

### Audit gate — make the invariant mechanical (`loom-lint`)

Add a `theme_consistency` rule to `loom-lint/src/lib.rs`: **fail the build** if any chrome/section selector (`.loom-page*`, `*__inner`, `.loom-*-footer*`, `.loom-container.w-comfortable`) sets a `max-width: <N>rem` page-column literal that is **not** `var(--loom-page-content-max…)`. Allowlist the intentional sub-widths (Bucket C) and the explicit `ContainerWidth` override classes by name. This converts the invariant from convention to construction — the same role WordPress's `:where()`-keyed layout support plays.

---

## 4. How this delivers "works first time / consistency by construction"

- **One declaration, one formula, every region.** After Buckets A + B, header, every body section, the utility strip, and the footer all resolve `var(--loom-page-content-max)` through the identical `padding-inline: max(gutter, calc((100% − content-max)/2))` inset. Setting one page's `content_width: roomy` reflows the *whole* chrome to 70rem in one place — the author cannot produce a divergent-width footer, because no code path emits one.
- **Invalid widths can't compile.** `ContentWidth` / `ContainerWidth` are closed enums with no free-string/px escape. An author picks a typed step or full-bleed seam; an off-scale width is a `rustc` error, not a silently-ignored class (the concrete win over theme.json strings, Sass maps, and `mx-auto px-*` convention).
- **Defaults are safe and silent.** Unset → `Comfortable` → 64rem, byte-identical to today; non-default widths are *correct* rather than partially applied. The `loom-lint` gate then guarantees no future hand-authored section can reintroduce a literal — drift is caught at build, not in a browser with zero console signal.
- **CSP / SRI posture intact.** The token lives in the external tokens layer (`:root`/`body` in `loom-critical.css`), tenant overrides go to the external tenant stylesheet, and `:where()` zero-specificity keeps single-source theming overridable without `!important` — no inline-style trap.

**Definition of done for #624:**
1. Bucket A migrated (byte-identical at default; verified by snapshot of a Comfortable page).
2. Bucket B migrated (footer tracks the token; classified Category 3; ProsperityClub + next footers re-verified at narrow/roomy/wide).
3. Fix C2: three envelope `var()` fallbacks unified to `64rem`.
4. `loom-lint` `theme_consistency` rule added + green across all skins, Bucket C allowlisted.
5. Snapshot/audit: one page at each `ContentWidth` shows header, body, **and** footer sharing one column edge.
