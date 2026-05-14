# Loom — usage guide

This is the Mom-friendly walkthrough. If you can copy a command into a
terminal you can build a real, accessible, secure website with Loom.

## What you get out of the box

Every site Loom generates ships with:

- **Light + dark mode** — automatic via `prefers-color-scheme`. No flag,
  no setting; it just works.
- **WCAG 2.1 AA contrast** in both palettes (verified at compile time).
- **Semantic HTML** — `<header>`, `<main>`, `<footer>`, `<nav>` —
  not stacks of `<div>`. Screen readers announce your page correctly.
- **Skip-to-content link** that becomes visible on focus.
- **`prefers-reduced-motion` respected** globally.
- **Strict CSP** — no `unsafe-inline`. Every inline style/script is
  pinned by sha256.
- **Ed25519-signed deploys** with a trust-anchor model that defends
  against key substitution.

You don't have to know what any of that means. You get it for free.

## The three commands you need

### 1. Make a site

```bash
loom site init mybakery --template basic
cd mybakery
```

That created `cms/index.json`, `cms/about.json`, `forge.toml`,
`backends.toml`, `README.md`, and a `.gitignore`. Two pages, ready to edit.

### 2. Edit it in your browser

```bash
loom edit-serve --cms cms --static-dir static --forge ''
```

Open `http://127.0.0.1:8124/` and you'll see your pages listed.
Click one, and the editor opens with **two panes**:

- **Left:** typed forms for every section on the page.
- **Right:** a live preview of the page.

**Click any text in the preview to edit it directly** — type, hit
Enter to save, or Escape to undo. This is the inline editor (T62 step 10).
The form on the left also scrolls to whichever section you click.

You can also:
- Add new sections via the dropdown at the bottom of the form pane.
- Reorder, delete, and add paragraphs to a Group section.
- Upload images. They're automatically stripped of GPS / EXIF /
  timestamps before storage. Mom's home address never leaks.
- Convert an existing HTML file into Loom JSON via `loom import`.

### 3. Publish it

```bash
loom attest init      # one-time: generate your signing keypair
loom deploy publish --from static --to /var/www/mybakery --name mybakery
```

The deploy is atomic — a single `current` symlink swap. The bundle is
content-addressed (`publish-<sha256>/`). The manifest carries an
Ed25519 signature derived from the keypair you just generated. To
verify a published bundle:

```bash
loom deploy verify --at /var/www/mybakery/current
```

Roll back if needed:

```bash
loom deploy rollback --at /var/www/mybakery
```

## Editor authentication

When `auth.toml` exists in your editor's working directory, every
endpoint except `/login` (and the public `/preview/*` and `/uploads/*`
paths) requires a session cookie. Set up auth with:

```bash
loom auth init       # writes auth.toml + asks for a password
```

The password is hashed with **Argon2id** (memory-hard, OWASP-recommended).
Sessions are HMAC-SHA256-signed cookies with `HttpOnly`, `Secure`,
`SameSite=Strict`. Constant-time comparison via `subtle`.

If `auth.toml` doesn't exist, the editor binds to `127.0.0.1` only and
runs unauthenticated (back-compat for solo developers).

## Common tasks

| I want to… | Run |
|---|---|
| add a new page | the editor's "new page" form, or hand-write `cms/<slug>.json` |
| change a hero title | click on it in the live preview |
| switch a banner from "info" to "warn" | edit the form's `Tone` dropdown |
| upload a photo | the editor's "Upload an image" link (in the gallery) |
| import a real HTML file | `loom import --input old-site.html --out cms/imported.json` |
| build the published HTML | `loom cms-render --input cms/index.json --out static/index.html --css-href /loom-skin.css` |
| lint the design | `loom lint <crate>` |
| audit themes for parity | (queued: forge `phase_dual_theme` T66) |

## What you don't have to think about

- **Path traversal.** Slug names are constrained to `[a-z][a-z0-9-]*`
  via the `SlugName` value type. `slug=../etc/passwd` is rejected at
  the constructor.
- **CSRF on inline edits.** The `X-Loom-Inline-Edit: 1` header
  triggers a CORS preflight that we never grant. Cross-origin POSTs
  fail before they reach the handler.
- **Image XSS.** Magic-byte sniffing rejects SVG. Only JPEG / PNG /
  GIF / WebP are accepted.
- **Schema drift.** Every section the editor writes is round-tripped
  through `CmsPage::deserialize` before save — if the patch produces
  an invalid document, the save fails closed.

## When something doesn't work

Loom errors are written to stderr with enough context to diagnose
without reproducing. They're scrubbed of paths and PII before reaching
the user. If you hit an unexpected error, the relevant module's
`AVP-PASS-N` annotation in the source (look for the error message
text) explains why the check exists and what the fix is.

## Getting fancier

Each command has its own `--help`:

```bash
loom --help
loom site --help
loom edit-serve --help
loom deploy --help
loom attest --help
loom import --help
```

For the design rationale — why the page-shell looks the way it does,
why the editor uses zero JS for forms, why the deploy model is what it
is — see `docs/DESIGN.md`.
