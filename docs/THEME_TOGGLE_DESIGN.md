# Theme-toggle Design — JS / WASM / CSS-only Decision

**Status:** doctrine. Closes #102. Documents the three modes
Loom ships today + the trade-offs that ruled out a strict CSS-
only `:has()` default.

## The three modes

Loom's `page_shell_themed` emits one of three theme-toggle
shapes depending on environment:

### 1. Default — 30-line inline JS bootstrap

* `THEME_TOGGLE_JS` (constant in `loom-cms-render`) reads
  `localStorage["loom-theme"]`, falls back to the SSR'd
  `<html data-theme="X">` value, falls back to `light`.
* Sets `data-theme` on `<html>`.
* The visible button (`<button class="loom-theme-toggle">`)
  cycles `light → dark → auto` on click and persists to
  localStorage.
* CSP allows the script via its sha256 hash; no `'unsafe-inline'`
  needed.

**Why this is the default**: cross-page theme persistence is
useful UX, the bootstrap is ~30 lines minified, the CSP keeps it
hash-allowed (no `'unsafe-inline'` widening), and the
`<noscript>`-equivalent fallback is built-in — when JS is
disabled, the server-rendered `data-theme` value sticks and
visitors see whichever theme the page declared.

### 2. Strict no-JS — `LOOM_NOSCRIPT_MODE=1` env flag

* The bootstrap script is dropped entirely.
* The visible `<button class="loom-theme-toggle">` markup is
  also dropped (no JS → button does nothing → don't show it).
* CSP becomes `script-src 'none'` — maximally strict.
* The page still respects the SSR'd `data-theme` value +
  `prefers-color-scheme` for `data-theme="auto"`.
* Visitors who want a different theme set their OS preference.
  The OS-driven flip works via the
  `@media (prefers-color-scheme: dark) { :root[data-theme="auto"]
  {...} }` rule in `BASE_THEME_CSS`.

**Why this exists**: LibreJS-compliant visitors, Tor Browser
"Safest" security level, archive.org indexing, screen readers,
hunted-tier (#124) builds. Anyone for whom even hashed inline JS
is a policy violation.

Activated by Forge when `forge.toml [noscript_strict] enabled =
true` per `docs/TOR_I2P_LOKINET_TEMPLATE.md`.

### 3. CSS-only `:has()` pattern — deferred future variant

Pure CSS theme toggle via a hidden radio group:

```html
<input type="radio" name="theme" id="theme-light" checked>
<input type="radio" name="theme" id="theme-dark">
<input type="radio" name="theme" id="theme-auto">
<label for="theme-light">Light</label>
<label for="theme-dark">Dark</label>
<label for="theme-auto">Auto</label>
```

```css
:root:has(#theme-dark:checked) { /* dark tokens */ }
:root:has(#theme-auto:checked) { /* auto tokens */ }
```

**Properties**:
* Works without JS.
* In-page state only — page reload resets to the SSR'd default.
* Multiplies CSS tree size by N themes; `:has()` rules per
  `<html>`-level selector for each of the 14 named themes
  would be substantial.

**Why not the default**:
* Loses cross-page persistence. A user who picks `dark` on
  `/about` then navigates to `/blog` sees `light` again. UX
  worse than the 30-line JS bootstrap.
* Bigger CSS payload. The `BASE_THEME_CSS` block in
  `page_shell_themed` is hash-pinned in CSP; expanding it 14x
  for `:has()` selectors widens the hash window for every page.
* Doesn't compose with the OS-driven `prefers-color-scheme`
  cleanly — the radio's `:checked` state outranks any
  `@media` rule.

**Status**: documented as a future opt-in mode for sites that
explicitly want the in-page-only toggle UX without ANY JS but
ALSO without giving up the toggle entirely (which is what
LOOM_NOSCRIPT_MODE does). Filed as inline future work — no
new task required since the strict no-JS mode already covers
the "absolutely zero script" use case.

## Decision matrix

| Need                                            | Mode                                   |
|-------------------------------------------------|----------------------------------------|
| Cross-page persistence + UX matters             | 1 (default)                            |
| LibreJS / Tor strict / hunted-tier / no JS at all | 2 (LOOM_NOSCRIPT_MODE=1)            |
| In-page toggle, no persistence, no JS           | 3 (deferred — file a task when needed) |
| OS preference only, no manual toggle            | 2 (LOOM_NOSCRIPT_MODE=1) — visitor sets OS theme |

## What ships today

* Mode 1 (default) — `THEME_TOGGLE_JS` + `THEME_TOGGLE_CSS`
  constants in `loom-cms-render`.
* Mode 2 (strict) — env-driven via `LOOM_NOSCRIPT_MODE=1`.
  Loom's `page_shell_themed` drops the script + the button
  markup, tightens CSP.
* Mode 3 (CSS-only) — NOT shipped. Documented as a future
  opt-in.

## Maintenance

If the CSS-only pattern is ever wanted as a third explicit mode,
add a new `LOOM_THEME_TOGGLE_MODE=css-only` env value parsed in
`page_shell_themed` alongside the existing `LOOM_NOSCRIPT_MODE`
check. Emit the radio-group markup + the per-theme
`:has(:checked)` rules. The trade-off (loss of persistence,
bigger CSS) is the caller's to accept.
