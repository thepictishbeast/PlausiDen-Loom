# ISO standards adoption — PlausiDen ecosystem

Owner directive 2026-05-13: "you should also adhere to iso
standards." This doc enumerates which ISO/IEC standards every
PlausiDen-* repo defaults to + how / where each is enforced.
Reference for contributors deciding "which spec governs this
input format / output / control / quality attribute."

Where ISO doesn't have a fitting standard, the fallback order is:
**ISO → IETF RFC → W3C → IEEE → vendor**. Document the choice in
the consuming code's doc comment.

---

## Standards in active use

| Standard | What it covers | Where PlausiDen uses it | Enforced by |
|---|---|---|---|
| **ISO 8601** | Date / time string format (`YYYY-MM-DDTHH:MM:SSZ`) | Every committed timestamp, every log line, every audit event, every commit message AVP-PASS-N annotation | Convention; T69 follow-up: `phase_iso_8601` lints any free-text timestamp not in this form |
| **ISO 639-1** | Two-letter language codes (`en`, `de`, `ja`) | Every `<html lang>` attribute Loom emits; CmsPage / nav-link content language tags | Loom `page_shell` hard-codes `lang="en"` today; T36 makes it typed (closed enum of 639-1 codes) |
| **ISO 639-3** | Three-letter language codes when 639-1 doesn't have the language | Reserved for future i18n expansion (Sprint 2 i18n work) | concept |
| **ISO 3166-1 alpha-2** | Two-letter country codes (`US`, `GB`, `JP`) | Reserved for future region selectors / locale-specific content / Salesman client geo metadata | concept |
| **ISO/IEC 25010** | Software quality model — eight characteristics | Every commit message that ships substantive code calls out which quality attribute(s) it advances (functional suitability, performance efficiency, compatibility, usability, reliability, security, maintainability, portability) | Convention enforced in commit message templates |
| **ISO/IEC 40500:2012** | Ratifies WCAG 2.0 AA as an ISO standard | Loom `page_shell` ships dual-theme + skip link + focus-visible + reduced-motion + semantic landmarks BY DEFAULT (T48c v1+v2). Forge `phase_a11y_landmarks` + `phase_contrast` + `phase_dual_theme` enforce. WCAG 2.1 AA is the actual floor; ISO/IEC 40500 is the contract Mom can demand. | Forge phases (strict severity) |
| **ISO/IEC 27001:2022** | Information security management — Annex A controls | The AVP-2 doctrine in `~/.claude/CLAUDE.md` covers most controls. Sprint 2 follow-up: explicit Annex-A-control mapping per AVP-2 pass | Convention; queued for explicit cross-reference |
| **ISO/IEC 27017** | Cloud security controls | Reserved for the multi-tenant Sprint 3 work (T45 + T46) when cloud-deploy targets land | concept |
| **ISO/IEC 27018** | PII protection in public clouds | Same trigger as ISO/IEC 27017. PlausiDen's privacy-positive design (image EXIF strip, zero-cookie public read path, signed audit logs) already exceeds the floor; explicit mapping queued | concept |
| **ISO/IEC 9899** | C99 / C11 / C17 / C23 standard C | N/A for Rust core. Reserved for any FFI binding (none currently in PlausiDen Rust workspaces) | (no consumer) |
| **ISO/IEC 14882** | C++ standard | N/A for Rust core. Same as above | (no consumer) |
| **ISO/IEC 5218** | Sex / gender encoding (0 = not known, 1 = male, 2 = female, 9 = not applicable) | Reserved for future Salesman client demographics OR PlausiDen-Engine synthetic-profile generation. NEVER use arbitrary strings or assumed binary values | concept |
| **ISO/IEC 9075** | SQL standard | Reserved for the multi-tenant Sprint 3 work — PostgreSQL adapter (Sprint 2 if it lands earlier). Use ISO SQL idioms (CTEs, window functions) over vendor-specific extensions when a portable form exists | concept |
| **ISO/IEC 23001-21** (C2PA) | Content authenticity / provenance | Concept territory — every uploaded image could embed a C2PA manifest tied to the editor's hardware key. Captured in vision docs | concept |

## Standards specifically rejected (with rationale)

| Standard | Why rejected |
|---|---|
| **ISO/IEC 29110** | "Software life cycle profiles for very small entities" — ceremonial; AVP-2 doctrine already covers + exceeds for the solo-dev case |
| **ISO 27006** | Audit-body certification — out of scope (we self-audit + publish transparency logs, not 3rd-party certify) |
| **ISO 9001** | Generic quality management system — too generic; ISO/IEC 25010 + AVP-2 are sharper for the software case |

## Enforcement cross-reference

Each standard maps to one or more enforcement points:

| Standard | Enforcement point |
|---|---|
| ISO 8601 | Convention. Future T69b: `phase_iso_8601` lints free-text timestamps |
| ISO 639-1 | Loom `page_shell` hard-codes today; T36 makes typed |
| ISO/IEC 25010 | Convention in commit messages; Oxidizer `check_iso_25010_callout` queued |
| ISO/IEC 40500 (WCAG 2.0 AA) | Forge `phase_contrast` + `phase_dual_theme` + `phase_a11y_landmarks` + `phase_html_semantic` |
| ISO/IEC 27001 Annex A | AVP-2 doctrine cross-reference; Oxidizer `check_avp_27001_mapping` queued |
| ISO/IEC 27017 / 27018 | Multi-tenant Sprint 3 work |
| ISO/IEC 5218 | Future Salesman / Engine schema gates |
| ISO/IEC 9075 | Future PostgreSQL adapter linter |

## Default-behaviour summary

For a contributor or agent operating in any PlausiDen-* repo:

1. **Dates / times:** ISO 8601 (`YYYY-MM-DDTHH:MM:SSZ` UTC).
2. **Languages:** ISO 639-1 two-letter code where available;
   639-3 three-letter only if no 639-1 exists.
3. **Countries:** ISO 3166-1 alpha-2.
4. **Sex / gender encoding (when needed):** ISO/IEC 5218.
5. **Quality attributes called out per commit:** ISO/IEC 25010
   (8 characteristics).
6. **Accessibility floor:** WCAG 2.1 AA = ISO/IEC 40500 + ARIA
   1.2 + Section 508. Forge phases enforce.
7. **Security baseline:** AVP-2 doctrine in
   `~/.claude/CLAUDE.md` ⊃ ISO/IEC 27001 Annex A controls.
8. **SQL when used:** ISO/IEC 9075 idioms over vendor-specific.

## Where this lives in the doctrine hierarchy

This doc is a `~/.claude/CLAUDE.md` cross-reference. The
authoritative source for any individual standard is the standard
itself (purchasable from ISO; many are also mirrored in IEEE /
W3C / IETF in equivalent form). When this doc disagrees with a
standard, the standard wins. When this doc disagrees with the
AVP-2 doctrine, AVP-2 wins for security-relevant cases (AVP-2
is strictly more demanding than the ISO floor).

## Follow-ups queued

- **T69b:** `phase_iso_8601` Forge phase — lint free-text
  timestamps in source / output for ISO 8601 compliance.
- **T69c:** Oxidizer `check_iso_25010_callout` — verify every
  substantive commit message names which 25010 quality attribute
  it advances.
- **T69d:** ISO/IEC 27001 Annex A → AVP-2 pass cross-reference
  table in `PlausiDen-AVP-Doctrine`.
- **T69e:** Per-repo `iso.toml` declaring which standards this
  repo conforms to (so Oxidizer can auto-enforce per repo).
