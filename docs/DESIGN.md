# Loom — design rationale

Loom is a Rust-native CMS + site builder + atomic deploy tool. It's
designed against the threat model in `~/.claude/CLAUDE.md` (AVP-2):
state-actor adversary, full source access, supply chain compromise,
unlimited compute. Everything that follows is a derivation from that
constraint.

## Layered tenets (in priority order)

1. **Type the data, not the path through it.** Field-validity is a
   constructor invariant, not a runtime check at every call site.
   `SlugName::new` rejects path traversal once; downstream code
   assumes the slug is safe.
2. **Capability over permission.** `WriteCapability::for_dir(root)`
   canonicalises a confine-root and refuses any write that escapes
   it. Symlink follow, `../` traversal, and absolute-path injection
   are all defeated by the capability boundary.
3. **Atomic on-disk transitions.** `WriteCapability::write_atomic`
   writes to a sibling temp file and renames into place — POSIX
   guarantees the rename is atomic on the same filesystem. Deploys
   use the same primitive at a higher level (rename of the `current`
   symlink).
4. **Cryptographic provenance.** Every published bundle carries an
   Ed25519 signature over its manifest. The trust anchor is OUT-OF-
   BAND — bundle-local pubkeys are convenience metadata only and
   MUST match the configured anchor (constant-time compare via
   `subtle`).
5. **Constant-time comparisons for anything secret.** Session-cookie
   signatures, attest pubkeys, password verification — all via
   `subtle::ConstantTimeEq`.
6. **Defence in depth.** Every public surface validates inputs at the
   boundary, then again where it matters: SlugName at the dispatcher,
   field-name allow-list at the inline-edit handler, kind→field
   whitelist at the patch site.
7. **No `unwrap`/`expect` in library code** without a `SAFETY:` /
   `// test-only` justification.

## Why these specific choices

### Why a typed CMS (`CmsPage` / `CmsSection`) instead of free-form HTML?

- Round-trip safety: any patch the editor produces goes through
  `CmsPage::deserialize` before atomic write. Invalid patches fail
  closed.
- Auditable surface: every renderable section is enumerated. A new
  variant requires both a renderer arm AND an editor form arm — the
  exhaustiveness check makes drift impossible.
- `deny_unknown_fields` on every variant: schema typos surface as
  parse errors, not silent data loss.
- Beauty by default: the renderer emits semantic HTML
  (`<header>`, `<main>`, `<footer>`, `<aside>` with implicit
  landmarks) regardless of who authored the JSON.

### Why server-rendered forms with zero JavaScript for the editor?

- Works without JS — a browser with scripting disabled still edits
  pages perfectly via form-POST. (The inline-edit overlay is a
  progressive enhancement on top.)
- No `npm install`. No bundle. No runtime dependency on a JS
  ecosystem the user has to audit.
- Tighter CSP — `script-src 'self'` is enough; we don't need
  `'unsafe-inline'` anywhere except the one editor-preview
  iframe (where the inline overlay JS is sha256-pinned).

### Why content-addressed image uploads?

- Same image uploaded twice → same URL → free dedupe.
- The URL is `Cache-Control: immutable`. CDNs cache forever
  without invalidation.
- The hash is computed AFTER metadata strip — two photos that
  differ only in EXIF dedupe correctly.

### Why hand-roll EXIF / PNG metadata strip instead of using `image`?

- `image` pulls a transitive DCT/PNG decoder. Decoding is a hot
  fuzzing surface; we don't need it just to drop a known set of
  segments.
- Our strip is structural — it never re-encodes pixels, so there's
  zero decoder exposure to a malicious image.
- ~200 lines, fully audited, parseable by anyone with a JPEG / PNG
  spec reference. The `image` crate is ~50× larger and pulls
  ~100× more LOC of dependencies.

### Why Ed25519 for deploy signatures (not RSA or ECDSA)?

- Constant-time signing + verification by construction.
- Deterministic — same key + same message = same signature, no
  RNG-side-channel risk.
- Tiny: 64-byte signature, 32-byte pubkey. Fits in any bundle
  manifest with no overhead.
- Same crate (`ed25519-dalek`) PlausiDen-Forge uses for its
  attestation chain. One audited dep, two consumers.

### Why `WriteCapability::write_atomic` instead of a direct `fs::write`?

A direct write is non-atomic — power loss mid-write leaves a
truncated file on disk. The atomic path:

1. Writes the full content to `<dest>.tmp.<pid>`.
2. `fsync`s the temp file.
3. `rename(temp, dest)` — atomic on the same filesystem.

A reader will see either the old content or the new content, never
a partial write.

### Why dual-theme + a11y by default (not opt-in)?

User directive 2026-05-13: "Forge should always make light and dark
themes unless otherwise specified... add accessibility features...
use semantic html and css for beauty."

The page-shell now ALWAYS emits `BASE_THEME_CSS` — light + dark
tokens, focus-visible outlines, skip-link styling, reduced-motion
respect. CSP-pinned via sha256. Cost: ~1 KB per page. Benefit: every
site Loom generates is WCAG 2.1 AA / ISO/IEC 40500 compliant from
the first commit, no integrator wiring required.

## Module map

| Crate | Purpose | Entry point |
|---|---|---|
| `loom-cli` | CLI surface; editor server; deploy; auth; image upload; importer | `loom-cli/src/main.rs` |
| `loom-cms-render` | `CmsPage` + `CmsSection` types + `render_page` / `render_section` | `loom-cms-render/src/lib.rs` |
| `loom-components` | Typed UI primitives (`Button`, `Card`, `Section`, `Hero`) | `loom-components/src/lib.rs` |
| `loom-tokens` | Colour, spacing, breakpoint, font, radius scales | `loom-tokens/src/lib.rs` |
| `loom-icons` | Inline SVG icon set | `loom-icons/src/lib.rs` |
| `loom-lint` | CLI: walks views, refuses raw class strings outside an allowlist | `loom-lint/src/main.rs` |

## Key types

- `SlugName(String)` — validated `[a-z][a-z0-9-]*`, ≤80 chars. The only
  way to introduce a slug into the dispatcher.
- `BackendKey(String)` — same character class, used by Forge's
  phantom-button check.
- `WriteCapability` — confine-root capability for filesystem writes.
  Has `write_file`, `read_file`, `write_atomic`, `resolve`.
- `CapabilityError` — ADT for the failure modes (`Io`,
  `EscapesScope`, `NotADir`).
- `SigStatus` — ADT for signature verification (`Unsigned`,
  `ValidTrusted`, `ValidUntrusted`).

## CSP profile

The page-shell emits these directives by default:

```
default-src 'self';
img-src 'self' data:;
style-src 'self' 'sha256-<base-theme-hash>' [+ 'sha256-<critical-hash>'];
script-src 'self' [+ 'unsafe-hashes' 'sha256-<onload-hash>'];
frame-ancestors 'none'
```

`'unsafe-hashes'` is only added when `critical_css` is present, to
allow the deferred-stylesheet-load `onload=` event handler. Even
then the handler's text is hashed — no arbitrary inline JS.

The editor preview iframe (`/preview-edit/<slug>.html`) uses
`frame-ancestors 'self'` (same-origin only) so the editor can embed
it. Its inline overlay style + script are both sha256-pinned.

## How the inline-edit POST is protected

A `POST /inline-edit` from the iframe shim must:

1. Carry a `X-Loom-Inline-Edit: 1` header. Any cross-origin attempt
   to set this header triggers a CORS preflight, which we never grant.
2. Have a session cookie passing `verify_session_cookie` (Argon2id +
   HMAC-SHA256). The cookie has `SameSite=Strict` so cross-origin
   navigation can't drag it along.
3. Provide a `slug` that passes `SlugName::new`.
4. Provide a numeric `section` index that bounds-checks against the
   page's `sections.len()`.
5. Provide a `field` name passing the per-kind allow-list AND a
   character-class check (lowercase ASCII + underscore, 1..32 chars).
6. Provide a `value` ≤ 8 KiB.

After patching, the new JSON is round-tripped through
`CmsPage::deserialize` BEFORE `write_atomic`. If the patched document
fails to parse, the save aborts with 422 and the file on disk is
unchanged.

## What's deliberately not here yet

- Multi-tenant isolation (T45 — per-tenant SQLite + workspace).
- A sandboxed Claude Code SSH bridge per tenant (T46).
- Compile-time landmark contracts (T36).
- Visual-regression diffing across themes/breakpoints (T33).
- TLA+ spec for the deploy/rollback state machine (T27).

These are queued, not forgotten. See the AVP-2 doctrine for the
ordering rationale: capability-based isolation comes before
sandboxed remote agents because remote agents need an isolation
substrate to be sandboxed INTO.

## Annotation conventions

Every machine-grepable annotation in the source:

- `BUG ASSUMPTION:` — what could go wrong in this block.
- `AVP-PASS-N:` — date + finding from a specific AVP-2 pass.
- `SAFETY:` — proof that an `unsafe` block is sound.
- `SECURITY:` — threat mitigated and how.
- `REGRESSION-GUARD:` — why this fix exists, what broke before.
- `SHIP-DECISION:` — accepted residual risks + signing developer.
- `UX-DEBT:` — manual verification required, risk if skipped.
- `SCHEMA:` — keys here are the source of truth alongside another
  module; both must agree (added during T65 — editor↔renderer
  field-name alignment).

If you find a bug, add the appropriate annotation to your fix and
your future self (or another contributor) can grep for the lineage.
