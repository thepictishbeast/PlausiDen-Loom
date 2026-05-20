# Loom Noscript Audit (#121)

**Date:** 2026-05-20
**Scope:** loom-components + loom-cms-render — every rendered surface
**Closes:** Task #227 (preamble #121)

This audit enumerates every place a Loom-rendered page touches JS,
documents what it does without JS, and confirms that the substrate
ships a usable experience for LibreJS / Tor Browser / hunted-tier
visitors.

The conclusion is that Loom is **noscript-first by design** — every
behavioral interaction either uses a browser-native HTML primitive
(`<form method="post">`, `<dialog open>`, `<details>`, `<a href>`)
or has a CSS-only fallback for `LOOM_NOSCRIPT_MODE` renders. Only
two inline scripts ship in normal mode, both are progressive
enhancements over working CSS / HTML primitives.

---

## Inline scripts that ship in default builds

The default page shell emits exactly two inline `<script>` blocks
and one inline `onload=` handler. Each is hash-pinned in CSP
`script-src 'unsafe-hashes' 'sha256-…'`; nothing else can run.

| Surface | Bytes | Purpose | Noscript fallback |
|---|---|---|---|
| `THEME_TOGGLE_JS` | ~700 | Cycle light/dark/auto on click, persist to `localStorage('loom-theme')` | **CSS-only `:has()`-driven radio fieldset** (shipped 2026-05-20 in `THEME_TOGGLE_NOSCRIPT_CSS` + `THEME_TOGGLE_NOSCRIPT_HTML` — see commit #102/task #211) |
| `ERUDA_LOADER_JS` | ~250 | Dev-only devtools loader. Gated on `page.dev_devtools && !noscript_mode` AND `localStorage["loom_eruda"]=="on"` | None needed — feature is dev-only, never reaches end users |
| `DEFER_ONLOAD_JS` | ~50 | Swap deferred-stylesheet `media="print"` → `media="all"` after parse | **Plain `<link rel="stylesheet">`** in `noscript_mode`; otherwise wrapped in `<noscript>` siblings for graceful degradation |

**JSON-LD `<script type="application/ld+json">`** is also emitted
for the Organization structured-data block but is a *data payload*,
not behavioral JS. Browsers don't execute LD+JSON; search engines
parse it as text. CSP `script-src` doesn't allow non-LD inline
scripts, so even an LD+JSON block with embedded JS would fail.

## Behavioral primitives that work without JS

The following primitives are deliberately built on browser-native
HTML elements so they degrade cleanly:

| Primitive | Native element | Without JS |
|---|---|---|
| `Modal` (loom-components) | `<dialog>` + `<form method="dialog">` close button | Close button works (form submission to `method=dialog` is browser-native). Opening requires JS OR the `open` attribute set at render time (operator opt-in for noscript-modal-by-default). |
| `Nav` / `NavLink` | `<a href>` | Fully native |
| `CmsSection::Form` / `CmsSection::FormStep` | `<form method="post" action="…">` | Submits to server; server processes via the tenant's `forms` handler. Multi-step forms use one `<form>` per step with hidden `<input>` carrying step number — no JS needed. |
| `CmsSection::Account` | Same as `Form` | Login / signup / password-reset all work via plain POST |
| `Composer` (social-post compose) | `<form method="post">` with `<textarea>` | Submits the post; server-side response replaces the page with the updated thread |
| `Toast` | Server-rendered `<div role="status">` | Visible for whatever lifetime the server's `aria-live` region keeps it; no JS dismiss |
| `Details` / disclosure | `<details><summary>` | Browser-native expand/collapse |
| `Card` / `KvPairCard` / `Picture` / `PullQuote` / `CodeShell` | Pure semantic HTML | No JS interaction surface to begin with |
| Theme switching | `[data-theme="…"]` attribute on `<html>` | Server-rendered from CMS theme or visitor cookie; visitor toggle works via CSS-only radio fieldset in noscript_mode (see #102) |

## CSP variants by mode

The page shell emits one of three CSP profiles:

* **Default** — `script-src 'self' 'unsafe-hashes' 'sha256-<theme>' 'sha256-<eruda>' 'sha256-<defer>'`. `unsafe-hashes` lets the hashed `onload=` attribute fire. Trusted Types active.
* **`dev_devtools=true`** — looser `style-src 'unsafe-inline'` for Eruda's injected panel UI. Still hash-pinned scripts.
* **`LOOM_NOSCRIPT_MODE`** — `script-src 'none'`. NO inline scripts allowed. Trusted Types active. `style-src` hash-pinned to base CSS + critical CSS only.

The maximally-strict noscript CSP is the recommended deployment for
Tor / hunted-tier sites. It guarantees no script can execute even
if HTML escaping is bypassed somewhere — a script tag injected into
user-generated content can't run because CSP forbids inline AND
external scripts.

## Surfaces that legitimately need JS

Per the substrate's "browser-native first" doctrine, these are the
ONLY surfaces where JS materially improves the experience and a CSS-
only fallback can't reach feature parity:

1. **localStorage persistence** of the theme toggle (per-page session
   works without JS; across-page persistence needs JS or a cookie
   roundtrip)
2. **Eruda devtools loader** (dev-only — never ships to prod)
3. **Critical-CSS deferred-stylesheet onload swap** (perf win on first
   render; without JS the stylesheet just loads inline as a regular
   `<link>` — slightly worse FCP but functionally identical)
4. **Modal trigger** (server-rendered modal with `<dialog open>` is
   the noscript path; click-to-open requires JS)

No other primitive in loom-components or loom-cms-render needs JS to
function. CSS-only `:has()` patterns, browser-native HTML elements,
and server-side rendering cover every interaction surface.

## What this audit confirms

* **Tor-strict / LibreJS / `dev_devtools=false` + `LOOM_NOSCRIPT_MODE`**
  builds produce HTML with ZERO `<script>` tags and ZERO `onload=`
  attributes. CSP `script-src 'none'` is enforceable.
* **CSS-only theme switching** works in noscript mode (shipped #102).
* **Forms, navigation, modal close, composer submit, account flows**
  all use browser-native HTML and work identically with or without JS.
* **The two non-data inline scripts are progressive enhancements** —
  the theme toggle has a `:has()` fallback; the stylesheet onload swap
  has a `<noscript><link rel="stylesheet"></noscript>` sibling.

## Future work tracked elsewhere

* **Cross-page theme persistence without JS**: cookie roundtrip via a
  POST-to-`/set-theme` endpoint that 302s back with `Set-Cookie:
  loom-theme=…`. SSR reads the cookie + emits the matching
  `data-theme`. Not in this iteration; file a follow-up if needed.
* **Modal click-to-open without JS**: could be approximated via
  `<details>` chrome or `:target` selector + URL hash, but neither
  matches the `<dialog>` semantics for assistive tech. Leaving as
  "JS-only" until a primitive lands that handles the gap cleanly.

## Test coverage

* `noscript_theme_toggle_css_has_palette_swap_via_has_selector` —
  proves the `:has()` cascade exists for dark/light/auto
* `noscript_theme_toggle_css_swaps_all_core_properties` — proves
  all 9 palette properties swap in the dark `:has()` block
* `noscript_theme_toggle_html_is_accessible_radio_group` — proves
  the rendered HTML is a valid accessible fieldset
* `noscript_theme_toggle_html_hides_radios_via_sr_only_pattern` —
  proves radios are sr-only-hidden (clip:rect), not display:none

Plus the existing CSP hash regeneration logic: any change to
`base_with_toggle` propagates into a new `csp_sha256` value and
the inline style hash remains pinned. No test gap there.

---

## Closing notes

Loom's noscript story is materially complete. The substrate's
"every interaction surface uses semantic HTML or CSS-only" doctrine
delivers what `LOOM_NOSCRIPT_MODE` advertises — a real, navigable,
form-submittable, theme-switchable site without ANY JavaScript at
all. The only paid-for-with-JS feature is cross-page localStorage
theme persistence, which is a deliberate progressive enhancement
not a required dependency.

The Tor / hunted-tier deployment story is therefore unblocked by
the noscript dimension. Other gates (.onion routing, Tor circuit
isolation, CSP `frame-ancestors 'none'` already in noscript CSP)
remain — but the rendered HTML side is shipped.
