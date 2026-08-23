//! `loom-tokens` — typed design tokens.
//!
//! Every constant here is a *unit of trust*: a designer or doctrine
//! reviewer signs off on it once, and every component thereafter
//! consumes it instead of inventing its own value. Adding a token is
//! a doctrine change (review required); using a token is free.
//!
//! Tokens are emitted in two shapes:
//!   * As Tailwind class strings (`spacing(4)` → `"4"` → `"px-4"`)
//!     consumed by `loom-components` to build typed components.
//!   * As JSON (`tokens_json()`) so future non-web generators
//!     (GTK, Jetpack Compose, etc.) can consume the same tokens
//!     without re-implementing them.

#![doc(html_no_source)]

pub mod axes;
pub mod color;
pub mod density;
pub mod gradient_pool;
pub mod icons;
pub mod polish;
pub mod radius;
pub mod scale;
pub mod stock_photos;
pub mod style_packs;

pub use density::DensityTier;

/// T69 (cycle 96 iter 13): the canonical Loom skin CSS bytes,
/// bundled at compile time. Forge's render phase writes these
/// bytes to `<static_dir>/loom-skin.css` on every build so the
/// shipped CSS is never stale.
///
/// Cycles 95a-g had to manually `cp` this file every iteration
/// because Forge's render phase wrote `<slug>.html` but not the
/// design-system CSS those HTML files reference. This const +
/// the matching write in `forge-phases::render` closes that
/// loop friction permanently.
pub const SKIN_CSS: &str = include_str!("skin.css");

/// Per-skin expression overlays, appended AFTER `SKIN_CSS`.
///
/// The base skin carries two different kinds of rule mixed together: structure
/// (resets, layout primitives, landmarks, focus handling, motion guards) and
/// expression (type scale, spacing rhythm, component shape, decoration). Only
/// the second kind decides whether two sites look alike — and because there was
/// exactly one skin, every Forge site inherited the same one. Swapping palette
/// variables recoloured that single look; it could never replace it. That is
/// the whole reason sites built on this substrate resemble each other.
///
/// An overlay is a full expression layer: it may restructure, not merely
/// recolour. Appended last at equal-or-greater specificity, it overrides the
/// base's expression while leaving its structure — the part that keeps pages
/// accessible and unbroken — intact. Two tenants on different overlays share
/// DOM and semantics and nothing visual.
///
/// Splitting the 555KB base into structure and expression properly is the
/// eventual job; overlays make distinct skins possible now, without a refactor
/// that would put every existing site at risk in one step.
/// Foreground repair for skins that remove the base's filled backgrounds.
///
/// Composed into each flat skin at COMPILE time, ahead of the skin's own rules,
/// so a new flat skin inherits the repair by construction. It is prepended
/// rather than appended: the shared rules are a floor a skin may override, not
/// a ceiling imposed on it.
///
/// Authoring `editorial` rediscovered the same defect three times — remove a
/// filled band and its near-white "on-fill" text is left on a near-white page,
/// invisible at a measured contrast of 1.0. Sharing the repair is what stops
/// the fourth skin from finding it a fourth time.
const FLAT_FOREGROUNDS: &str = include_str!("skins/_flat-foregrounds.css");

/// Repairs EVERY skin needs: colours the base expresses as a fixed alpha over
/// an unknown background, which land wherever the tenant's palette happens to
/// put them — fine on paper, 3.16 on near-black.
///
/// This was originally folded into `FLAT_FOREGROUNDS` and given only to skins
/// that remove fills. `warm` disproved that partition by keeping its fills,
/// skipping the partial, and inheriting 48 contrast failures in components that
/// have nothing to do with fills.
const PALETTE_SAFETY: &str = include_str!("skins/_palette-safety.css");

const SKIN_EDITORIAL: &str = concat!(
    include_str!("skins/_palette-safety.css"),
    include_str!("skins/_flat-foregrounds.css"),
    include_str!("skins/editorial.css")
);
const SKIN_TECHNICAL: &str = concat!(
    include_str!("skins/_palette-safety.css"),
    include_str!("skins/_flat-foregrounds.css"),
    include_str!("skins/technical.css")
);
const SKIN_CIVIC: &str = concat!(
    include_str!("skins/_palette-safety.css"),
    include_str!("skins/_flat-foregrounds.css"),
    include_str!("skins/civic.css")
);
/// `warm` takes palette safety but NOT the flat repair: it keeps its fills, so
/// the on-fill foregrounds that partial rewrites are still correct here.
const SKIN_WARM: &str = concat!(
    include_str!("skins/_palette-safety.css"),
    include_str!("skins/warm.css")
);

/// Skin names a tenant may select via `forge.toml` `[style] skin = "…"`.
/// `"base"` is the default and adds no overlay.
///
/// Each is a distinct visual language, not a palette: they differ in alignment,
/// density, whether containers exist at all, and type treatment. Two tenants on
/// different skins share DOM and semantics and nothing visual.
///
/// * `editorial` — no containers, type-driven hierarchy, asymmetric, serif.
/// * `technical` — dense, monospaced furniture, shared-border cells, square.
/// * `civic` — centred and symmetric, stacked rules, numbered sections.
/// * `warm` — containers, large radii, soft shadows, generous type.
pub const SKIN_NAMES: &[&str] = &["base", "editorial", "technical", "civic", "warm"];

/// The expression overlay for `name`, or `None` for the default skin.
///
/// Returns `Err(())` for an unknown name rather than silently falling back:
/// quietly shipping the wrong visual identity is worse than failing the build,
/// because nothing downstream would report it.
pub fn skin_overlay(name: &str) -> Result<Option<&'static str>, ()> {
    match name {
        "base" => Ok(None),
        "editorial" => Ok(Some(SKIN_EDITORIAL)),
        "technical" => Ok(Some(SKIN_TECHNICAL)),
        "civic" => Ok(Some(SKIN_CIVIC)),
        "warm" => Ok(Some(SKIN_WARM)),
        _ => Err(()),
    }
}

#[cfg(test)]
mod skin_tests {
    use super::*;

    #[test]
    fn every_advertised_skin_resolves() {
        // A name in SKIN_NAMES that does not resolve would fail the build of any
        // tenant that trusted the list — the list IS the tenant-facing contract.
        for name in SKIN_NAMES {
            let got = skin_overlay(name);
            assert!(got.is_ok(), "advertised skin {name:?} does not resolve");
            if *name != "base" {
                assert!(got.unwrap().is_some(), "{name:?} must carry an overlay");
            }
        }
        assert!(
            skin_overlay("nope").is_err(),
            "unknown names must not fall back"
        );
    }

    #[test]
    fn every_skin_takes_palette_safety() {
        // These repair colours the base expresses as a fixed alpha over an
        // unknown background, so they are wrong for SOME palette no matter what
        // the skin does. Omitting them is what gave `warm` 48 contrast failures
        // in components that have nothing to do with its design.
        for name in SKIN_NAMES.iter().filter(|n| **n != "base") {
            let css = skin_overlay(name).unwrap().unwrap();
            assert!(
                css.starts_with(PALETTE_SAFETY),
                "{name} must take palette safety as its floor"
            );
        }
    }

    #[test]
    fn only_skins_that_remove_fills_take_the_flat_repair() {
        // Applying the flat repair to a skin that KEPT its fills would rewrite
        // foregrounds that are still correct — the mirror image of the bug it
        // exists to prevent. So this asserts both directions.
        for name in ["editorial", "technical", "civic"] {
            let css = skin_overlay(name).unwrap().unwrap();
            assert!(
                css.contains(FLAT_FOREGROUNDS),
                "{name} removes fills but is missing the foreground repair"
            );
            assert!(
                css.contains("loom-steps__num"),
                "{name}: repair looks truncated"
            );
        }
        let warm = skin_overlay("warm").unwrap().unwrap();
        assert!(
            !warm.contains(FLAT_FOREGROUNDS),
            "warm keeps its fills, so the flat repair must not be applied"
        );
    }

    #[test]
    fn skins_never_hardcode_a_colour_outside_the_tenant_palette() {
        // Every colour must resolve through a --loom-color-* token. A literal
        // hex is how the base ended up painting every site with the same blue
        // and violet wash regardless of the palette the tenant chose.
        //
        // `#` alone is not enough to go on: an ID selector like `main#content`
        // contains one. A hex colour is `#` plus exactly 3, 4, 6 or 8 hex
        // digits and nothing word-like after it.
        fn hex_colour_at(bytes: &[u8], hash: usize) -> bool {
            let run = bytes[hash + 1..]
                .iter()
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            let ends_cleanly = bytes
                .get(hash + 1 + run)
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_' && *b != b'-');
            matches!(run, 3 | 4 | 6 | 8) && ends_cleanly
        }
        // Prove the detector still bites. Narrowing it to exclude ID selectors
        // could easily have narrowed it into a check that passes everything,
        // and a green test that cannot fail is worse than no test.
        let fires = |s: &str| {
            let b = s.as_bytes();
            b.iter()
                .enumerate()
                .any(|(i, c)| *c == b'#' && hex_colour_at(b, i))
        };
        assert!(fires("color: #FAF8F4;"), "must catch a 6-digit hex");
        assert!(fires("color: #fff"), "must catch a 3-digit hex");
        assert!(
            fires("border-color: #16130F80;"),
            "must catch an 8-digit hex"
        );
        assert!(
            !fires(":root main#content { counter-reset: x; }"),
            "an ID is not a colour"
        );
        assert!(!fires(".loom-page-footer__col"), "a class is not a colour");

        for name in ["editorial", "technical", "civic", "warm"] {
            let css = skin_overlay(name).unwrap().unwrap();
            for (n, line) in css.lines().enumerate() {
                let code = line.split("/*").next().unwrap_or("").trim();
                if code.starts_with('*') || code.starts_with("//") {
                    continue;
                }
                let bytes = code.as_bytes();
                let hex = bytes
                    .iter()
                    .enumerate()
                    .any(|(i, b)| *b == b'#' && hex_colour_at(bytes, i));
                assert!(
                    !hex && !code.contains("hsl(") && !code.contains("rgb("),
                    "{name}:{} hardcodes a colour: {code}",
                    n + 1
                );
            }
        }
    }
}

pub use color::{Color, ColorRole};
pub use polish::{PolishCategory, PolishSet, PolishToken};
pub use radius::Radius;
pub use scale::{Breakpoint, FontSize, Spacing};

use serde::Serialize;
use std::fmt::Write as _;

/// Top-level export of every token as JSON. Stable wire format.
///
/// Future cross-platform generators (GTK theme, Jetpack Compose Theme
/// builder, etc.) consume this to ensure pixel-identical sizing and
/// color across platforms.
#[derive(Debug, Serialize)]
pub struct AllTokens {
    /// Palette by semantic role (`primary`, `slate-900`, etc.) →
    /// value (CSS color string). Light theme.
    pub colors: Vec<ColorRole>,
    /// Same role list, dark-theme resolutions. Cross-platform
    /// generators that target a dark surface (GTK dark, Material
    /// You dynamic) consume this slice.
    pub colors_dark: Vec<ColorRole>,
    /// Spacing scale steps.
    pub spacing: Vec<Spacing>,
    /// Breakpoints in pixels.
    pub breakpoints: Vec<Breakpoint>,
    /// Font sizes.
    pub font_sizes: Vec<FontSize>,
    /// Border radii.
    pub radii: Vec<Radius>,
}

/// Serialize every token to a JSON string. Used by cross-platform
/// theme generators and by the doctrine doc check that the surface
/// is still in sync.
///
/// # Panics
/// Never panics in practice — the token tree is finite and every
/// type derives `Serialize`. The `expect` exists so a future-broken
/// derive would fail the build, not silently corrupt output.
#[must_use]
pub fn tokens_json() -> String {
    let all = AllTokens {
        colors: ColorRole::all().to_vec(),
        colors_dark: ColorRole::dark_all().to_vec(),
        spacing: Spacing::all().to_vec(),
        breakpoints: Breakpoint::all().to_vec(),
        font_sizes: FontSize::all().to_vec(),
        radii: Radius::all().to_vec(),
    };
    serde_json::to_string_pretty(&all).expect("token tree is finite + serde-clean")
}

/// Emit every token as CSS custom properties.
///
/// Variables land under `:root` and `:root[data-theme="dark"]`.
/// Drop-in stylesheet for any web surface; the lint expects every
/// raw value to be substituted with `var(--loom-color-*)` /
/// `var(--loom-space-*)` from this output.
#[must_use]
pub fn tokens_css() -> String {
    let mut out = String::new();
    out.push_str("/* Generated by loom-tokens — do NOT edit by hand.\n");
    out.push_str(" * Re-emit via `loom css > path/to/loom-tokens.css`.\n */\n\n");
    out.push_str(":root {\n");
    for role in ColorRole::all() {
        let _ = writeln!(out, "  --loom-color-{}: {};", role.role, role.color.css);
    }
    for sp in Spacing::all() {
        let _ = writeln!(out, "  --loom-space-{}: {}rem;", sp.tailwind(), sp.rem());
    }
    for bp in Breakpoint::all() {
        let _ = writeln!(out, "  --loom-break-{}: {}px;", bp.tailwind(), bp.px());
    }
    for fs in FontSize::all() {
        let _ = writeln!(out, "  --loom-font-{}: {};", fs.tailwind(), fs.css_size());
    }
    for r in Radius::all() {
        let _ = writeln!(out, "  --loom-radius-{}: {};", r.tailwind(), r.css_size());
    }
    out.push_str("}\n\n");

    out.push_str(":root[data-theme=\"dark\"] {\n");
    for role in ColorRole::dark_all() {
        let _ = writeln!(out, "  --loom-color-{}: {};", role.role, role.color.css);
    }
    out.push_str("}\n");

    // Substrate utility classes — emitted in the tokens CSS so
    // tenants whose Content-Security-Policy is `style-src 'self'`
    // can pick up Loom shell-padding without inline `style="…"`
    // attributes (which CSP blocks). Mirrors Tailwind class names
    // for readability but lives in loom-tokens output so it ships
    // with every tenant build via `loom css > loom-tokens.css`.
    out.push_str("\n/* Substrate utility classes (CSP-safe alternative to inline padding) */\n");
    out.push_str(".loom-shell {\n");
    out.push_str("  padding-left: 4rem;\n");
    out.push_str("  padding-right: 4rem;\n");
    out.push_str("}\n");
    out.push_str("@media (max-width: 768px) {\n");
    out.push_str("  .loom-shell {\n");
    out.push_str("    padding-left: 1rem;\n");
    out.push_str("    padding-right: 1rem;\n");
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

/// Emit every token as a Rust `pub const` block for egui apps.
///
/// Intended for inclusion in an egui-driven app (Atrium,
/// Sentinel-GUI). Colours are emitted as
/// `egui::Color32::from_rgb(r, g, b)` literals (parsed from the
/// CSS HSL/hex shape at emit time so the egui app doesn't carry a
/// runtime parser); spacing + radius as `f32` rem multiples +
/// `u32` pixel values pre-resolved at the design root font size
/// (16 px).
///
/// Output is drop-in: write to `src/loom_tokens.rs`, add
/// `mod loom_tokens;` to `lib.rs`, then reference
/// `loom_tokens::color::PRIMARY` etc. Requires `egui` to be a
/// dependency of the consuming crate.
#[must_use]
pub fn tokens_egui() -> String {
    let mut out = String::new();
    out.push_str("//! Generated by loom-tokens — do NOT edit by hand.\n");
    out.push_str("//! Re-emit via `loom egui > src/loom_tokens.rs`.\n//!\n");
    out.push_str("//! Cross-platform mirror of the loom-tokens CSS output.\n");
    out.push_str("//! Requires `egui` as a dependency of the consuming crate.\n\n");
    out.push_str("#![allow(dead_code)]\n\n");
    out.push_str("use egui::Color32;\n\n");

    out.push_str("/// Light-theme palette.\n");
    out.push_str("pub mod color {\n");
    out.push_str("    use super::Color32;\n");
    for role in ColorRole::all() {
        emit_color_const(&mut out, role);
    }
    out.push_str("}\n\n");

    out.push_str("/// Dark-theme palette mirror — same role names, dark-resolved values.\n");
    out.push_str("pub mod color_dark {\n");
    out.push_str("    use super::Color32;\n");
    for role in ColorRole::dark_all() {
        emit_color_const(&mut out, role);
    }
    out.push_str("}\n\n");

    out.push_str("/// Spacing scale. `*_REM` for layout that multiplies by base font size,\n");
    out.push_str("/// `*_PX` pre-resolved at the 16 px design root.\n");
    out.push_str("pub mod space {\n");
    for sp in Spacing::all() {
        let (tw, rem, px) = (sp.tailwind(), sp.rem(), sp.px());
        let _ = writeln!(out, "    /// step {tw} — {rem}rem ({px}px @16)");
        let _ = writeln!(
            out,
            "    pub const S{tw}_REM: f32 = {rem}_f32;\n    pub const S{tw}_PX: u32 = {px};"
        );
    }
    out.push_str("}\n\n");

    out.push_str("/// Breakpoints in pixel widths.\n");
    out.push_str("pub mod breakpoint {\n");
    for bp in Breakpoint::all() {
        let upper = bp.tailwind().to_uppercase();
        let _ = writeln!(out, "    pub const {upper}: u32 = {};", bp.px());
    }
    out.push_str("}\n\n");

    out.push_str("/// Border radii. Native px values pre-resolved at the 16 px design root.\n");
    out.push_str("pub mod radius {\n");
    for r in Radius::all() {
        let upper = r.tailwind().to_uppercase();
        let px = match r {
            Radius::None => 0,
            Radius::Sm => 4,  // 0.25rem
            Radius::Md => 8,  // 0.5rem
            Radius::Lg => 12, // 0.75rem
            Radius::Xl => 16, // 1rem
            Radius::Full => 9999,
        };
        let _ = writeln!(
            out,
            "    /// {} ({})\n    pub const {upper}: f32 = {px}_f32;",
            r.css_size(),
            r.tailwind(),
        );
    }
    out.push_str("}\n");
    out
}

fn emit_color_const(out: &mut String, role: &ColorRole) {
    let const_name = role.role.to_uppercase().replace('-', "_");
    let (r, g, b) = role.color.rgb().unwrap_or((255, 0, 255));
    let _ = writeln!(
        out,
        "    /// `{}` — {} — `{}`",
        role.role, role.color.tailwind, role.color.css,
    );
    let _ = writeln!(
        out,
        "    pub const {const_name}: Color32 = Color32::from_rgb({r}, {g}, {b});",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_json_round_trips() {
        let s = tokens_json();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert!(v.get("colors").is_some());
        assert!(v.get("colors_dark").is_some());
        assert!(v.get("spacing").is_some());
        assert!(v.get("breakpoints").is_some());
        assert!(v.get("font_sizes").is_some());
        assert!(v.get("radii").is_some());
    }

    /// Reviewer guard: if the token surface ships fewer than the
    /// minimums, something has been deleted that probably should
    /// not have been. Bumps to these numbers are intentional.
    #[test]
    fn token_surface_is_at_least_minimum() {
        assert!(ColorRole::all().len() >= 8, "palette shrunk");
        assert!(Spacing::all().len() >= 10, "spacing scale shrunk");
        assert!(Breakpoint::all().len() >= 4, "breakpoints shrunk");
        assert!(FontSize::all().len() >= 7, "font scale shrunk");
        assert!(Radius::all().len() >= 4, "radius scale shrunk");
    }

    #[test]
    fn css_emitter_covers_every_role_in_root() {
        let css = tokens_css();
        for role in ColorRole::all() {
            let prop = format!("--loom-color-{}:", role.role);
            assert!(css.contains(&prop), "missing {prop} in :root");
        }
        assert!(css.contains(":root[data-theme=\"dark\"]"));
    }

    #[test]
    fn css_emitter_covers_every_spacing_step() {
        let css = tokens_css();
        for sp in Spacing::all() {
            let prop = format!("--loom-space-{}:", sp.tailwind());
            assert!(css.contains(&prop), "missing {prop}");
        }
    }

    #[test]
    fn egui_emitter_emits_color_const_per_role() {
        let rs = tokens_egui();
        for role in ColorRole::all() {
            let const_name = role.role.to_uppercase().replace('-', "_");
            // Colours are emitted as Color32::from_rgb literals after
            // the parse-at-emit-time refactor.
            assert!(
                rs.contains(&format!(
                    "pub const {const_name}: Color32 = Color32::from_rgb"
                )),
                "missing Color32 const {const_name}",
            );
        }
    }

    #[test]
    fn egui_emitter_compiles_to_valid_rust_lookalike() {
        // Smoke: every line is either a comment, blank, or a known
        // structural token. Detects accidental leakage of strings
        // outside `pub mod` blocks.
        let rs = tokens_egui();
        for line in rs.lines() {
            let s = line.trim_start();
            if s.is_empty() || s.starts_with("//") || s.starts_with("/*") {
                continue;
            }
            let ok = s.starts_with("pub mod")
                || s.starts_with("pub const")
                || s.starts_with("use ")
                || s.starts_with("#![")
                || s == "}"
                || s.starts_with("pub use")
                || s.starts_with("///")
                || s.starts_with("//!");
            assert!(ok, "unexpected line: {line:?}");
        }
    }

    #[test]
    fn egui_emitter_color_consts_are_color32_literals() {
        let rs = tokens_egui();
        // Every role surfaces as a Color32::from_rgb(r, g, b) literal.
        for role in ColorRole::all() {
            let const_name = role.role.to_uppercase().replace('-', "_");
            let needle = format!("pub const {const_name}: Color32 = Color32::from_rgb(");
            assert!(
                rs.contains(&needle),
                "missing Color32 literal for {const_name}",
            );
        }
    }

    #[test]
    fn color_rgb_parses_hex_and_hsl() {
        // White hex.
        let white = Color {
            name: "x",
            tailwind: "x",
            css: "#ffffff",
        };
        assert_eq!(white.rgb(), Some((255, 255, 255)));

        // Pure red HSL.
        let red = Color {
            name: "x",
            tailwind: "x",
            css: "hsl(0 100% 50%)",
        };
        assert_eq!(red.rgb(), Some((255, 0, 0)));

        // Loom primary.
        let primary = Color {
            name: "x",
            tailwind: "x",
            css: "hsl(220 90% 28%)",
        };
        let (r, g, b) = primary.rgb().expect("parses");
        // Expect a deep blue — R+G < B.
        assert!(b > r && b > g, "expected blue dominant: {r},{g},{b}");
    }
}
