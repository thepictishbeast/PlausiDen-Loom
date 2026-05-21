//! Palette tokens — semantic roles + a stable HSL value per role.
//!
//! Roles are *meaning-named* (`primary`, `surface`, `muted-text`),
//! never appearance-named (`blue-500`). When the palette is retuned,
//! every component using a role updates correctly without code change.

use serde::{Deserialize, Serialize};

/// A single named color value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    /// Display name, e.g. `"primary"` or `"slate-900"`.
    pub name: &'static str,
    /// Tailwind utility prefix (no color-step suffix), e.g. `"primary"`,
    /// `"slate"`. Components compose with `bg-{prefix}` etc.
    pub tailwind: &'static str,
    /// CSS color string in HSL or hex.
    pub css: &'static str,
}

impl Color {
    /// Parse the CSS string into 8-bit RGB. Supports `#rgb`, `#rrggbb`,
    /// and `hsl(h s% l%)` shapes — the only colour shapes loom-tokens
    /// emits today. Returns `None` for any other shape so a future
    /// unknown form fails the call site visibly rather than silently
    /// producing the wrong colour.
    #[must_use]
    pub fn rgb(&self) -> Option<(u8, u8, u8)> {
        parse_css_color(self.css)
    }
}

fn parse_css_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(inner) = s.strip_prefix("hsl(").and_then(|x| x.strip_suffix(')')) {
        return parse_hsl(inner);
    }
    None
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some((r, g, b))
        }
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn parse_hsl(inner: &str) -> Option<(u8, u8, u8)> {
    // Accepts both comma- and space-separated forms, with `%` on s/l.
    let parts: Vec<&str> = inner
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 3 {
        return None;
    }
    let h: f32 = parts[0].parse().ok()?;
    let s_pct: f32 = parts[1].trim_end_matches('%').parse().ok()?;
    let l_pct: f32 = parts[2].trim_end_matches('%').parse().ok()?;
    Some(hsl_to_rgb(h, s_pct / 100.0, l_pct / 100.0))
}

#[allow(clippy::many_single_char_names)]
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    // Standard CSS HSL → sRGB conversion. h in degrees, s/l in 0..=1.
    let c = (1.0 - 2.0f32.mul_add(l, -1.0).abs()) * s;
    let h_prime = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
    // `rem_euclid(360)/60` keeps h_prime in [0, 6); the `as u32` is
    // a sextant-floor lookup, never NaN, never negative, never > 5.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let sextant = h_prime as u32;
    let (r1, g1, b1) = match sextant {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    // .clamp(0.0, 255.0) keeps v in [0, 255] before the u8 cast,
    // so truncation / sign-loss can't fire — but clippy still flags
    // the float→int cast as suspicious. Document the invariant.
    let to_byte = |v: f32| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        byte
    };
    (to_byte(r1), to_byte(g1), to_byte(b1))
}

/// One semantic role + the [`Color`] it resolves to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRole {
    /// Role identifier — never appearance-named.
    pub role: &'static str,
    /// Resolved color.
    pub color: Color,
}

impl ColorRole {
    /// Every defined role. Order is stable; new roles append at the end.
    #[must_use]
    #[allow(clippy::too_many_lines)] // it's a flat data table.
    pub const fn all() -> &'static [Self] {
        &[
            Self {
                role: "primary",
                color: Color {
                    name: "primary",
                    tailwind: "primary",
                    css: "hsl(220 90% 28%)",
                },
            },
            Self {
                role: "primary-fg",
                color: Color {
                    name: "primary-fg",
                    tailwind: "primary-foreground",
                    css: "#ffffff",
                },
            },
            Self {
                role: "surface",
                color: Color {
                    name: "surface",
                    tailwind: "white",
                    css: "#ffffff",
                },
            },
            Self {
                role: "surface-muted",
                color: Color {
                    name: "surface-muted",
                    // Was 98% L (basically white). Bumped to 95% L
                    // so cards visibly stand out from canvas at
                    // a glance.
                    tailwind: "slate-100",
                    css: "hsl(214 32% 95%)",
                },
            },
            Self {
                role: "ink",
                color: Color {
                    name: "ink",
                    tailwind: "slate-900",
                    css: "hsl(222 47% 11%)",
                },
            },
            Self {
                role: "ink-muted",
                color: Color {
                    name: "ink-muted",
                    // Was 47% L → 4.0:1 on the new 95% L surface
                    // (fails AA). 38% L gives 6.4:1 — AA pass + AAA
                    // for 18pt+.
                    tailwind: "slate-700",
                    css: "hsl(215 25% 38%)",
                },
            },
            Self {
                role: "border",
                color: Color {
                    name: "border",
                    // Was 91% L (barely visible against white). 80%
                    // L is a clearly-readable hairline.
                    tailwind: "slate-300",
                    css: "hsl(214 32% 80%)",
                },
            },
            Self {
                role: "danger",
                color: Color {
                    name: "danger",
                    tailwind: "red-600",
                    css: "hsl(0 72% 51%)",
                },
            },
            Self {
                role: "success",
                color: Color {
                    name: "success",
                    tailwind: "emerald-600",
                    css: "hsl(160 84% 30%)",
                },
            },
            // Warn channel — amber. Used for "degraded but
            // functional" state (egui status bars, banner alerts).
            // BUG ASSUMPTION: warn is a state colour, not a tier
            // colour. The tier_* roles below pin importance
            // semantics to the same colour value but a future
            // re-tune of state vs tier could split them.
            Self {
                role: "warn",
                color: Color {
                    name: "warn",
                    tailwind: "amber-600",
                    css: "hsl(28 80% 50%)",
                },
            },
            // Pale-amber surface used as the background for warn
            // banners. Foreground text on this background should
            // be `ink` for max contrast.
            Self {
                role: "warn-bg",
                color: Color {
                    name: "warn-bg",
                    tailwind: "amber-50",
                    css: "hsl(40 90% 93%)",
                },
            },
            // Mid-importance tier (between high and low). Distinct
            // from danger/warn/primary/success so the importance
            // scale doesn't reuse a state colour for a non-state
            // signal.
            Self {
                role: "tier-medium",
                color: Color {
                    name: "tier-medium",
                    tailwind: "yellow-500",
                    css: "hsl(50 80% 56%)",
                },
            },
            // Stronger border for elevated panels / focused
            // sections. Pairs with `border` (the regular hairline).
            Self {
                role: "border-strong",
                color: Color {
                    name: "border-strong",
                    tailwind: "slate-300",
                    css: "hsl(212 16% 82%)",
                },
            },
            // Soft accent for focused glow + selected-row tint.
            // ~80% lightness of primary.
            Self {
                role: "accent-soft",
                color: Color {
                    name: "accent-soft",
                    tailwind: "blue-200",
                    css: "hsl(220 75% 80%)",
                },
            },
            // Even softer accent — outer halo around focused
            // controls / shadow-glow on hero CTAs.
            Self {
                role: "accent-glow",
                color: Color {
                    name: "accent-glow",
                    tailwind: "blue-100",
                    css: "hsl(220 75% 90%)",
                },
            },
            // Brand gradient endpoints (blue → purple). Use the
            // pair as a `linear-gradient(...)` from `gradient-a`
            // to `gradient-b` for premium hero CTAs.
            Self {
                role: "gradient-a",
                color: Color {
                    name: "gradient-a",
                    tailwind: "blue-500",
                    css: "hsl(218 78% 56%)",
                },
            },
            Self {
                role: "gradient-b",
                color: Color {
                    name: "gradient-b",
                    tailwind: "purple-500",
                    css: "hsl(269 65% 57%)",
                },
            },
            // Canvas — sits *behind* `surface`; what shows when a
            // panel doesn't fill the whole viewport. On web this is
            // typically the `<body>` background.
            Self {
                role: "bg-canvas",
                color: Color {
                    name: "bg-canvas",
                    tailwind: "slate-100",
                    css: "hsl(225 33% 99%)",
                },
            },
            // Modal / dropdown scrim. Distinct from any surface
            // because it sits *over* content rather than under it.
            Self {
                role: "bg-overlay",
                color: Color {
                    name: "bg-overlay",
                    tailwind: "slate-200",
                    css: "hsl(220 28% 95%)",
                },
            },
            // ── Cascade aliases (#336) ──
            // The atomic-primitive cascade in `skin.css` consumes
            // these via `var(--loom-color-<name>, fallback)`. The
            // values below back the fallback path so substrate
            // tenants without a custom palette get coherent
            // defaults. Tenant `[style.palette]` config overrides
            // any of these.
            //
            // Semantic aliases for existing slots:
            Self {
                role: "bg",
                color: Color { name: "bg", tailwind: "white", css: "#ffffff" },
            },
            Self {
                role: "text",
                color: Color { name: "text", tailwind: "slate-900", css: "hsl(222 47% 11%)" },
            },
            Self {
                role: "muted",
                color: Color { name: "muted", tailwind: "slate-500", css: "hsl(215 16% 47%)" },
            },
            Self {
                role: "accent-2",
                color: Color { name: "accent-2", tailwind: "indigo-500", css: "hsl(239 84% 67%)" },
            },
            Self {
                role: "focus",
                color: Color { name: "focus", tailwind: "blue-500", css: "hsl(217 91% 60%)" },
            },
            Self {
                role: "on-primary",
                color: Color { name: "on-primary", tailwind: "white", css: "#ffffff" },
            },
            Self {
                role: "on-dark",
                color: Color { name: "on-dark", tailwind: "white", css: "#ffffff" },
            },
            Self {
                role: "primary-hover",
                color: Color { name: "primary-hover", tailwind: "primary-700", css: "hsl(220 90% 22%)" },
            },
            // Hero / Quote / Code / Table per-primitive surfaces:
            Self {
                role: "hero-bg",
                color: Color { name: "hero-bg", tailwind: "transparent", css: "transparent" },
            },
            Self {
                role: "quote-bg",
                color: Color { name: "quote-bg", tailwind: "transparent", css: "transparent" },
            },
            Self {
                role: "quote-text",
                color: Color { name: "quote-text", tailwind: "slate-700", css: "hsl(215 25% 27%)" },
            },
            Self {
                role: "code-bg",
                color: Color { name: "code-bg", tailwind: "slate-100", css: "hsl(214 32% 95%)" },
            },
            Self {
                role: "code-text",
                color: Color { name: "code-text", tailwind: "slate-900", css: "hsl(222 47% 11%)" },
            },
            Self {
                role: "table-stripe",
                color: Color { name: "table-stripe", tailwind: "slate-50", css: "hsl(210 40% 98%)" },
            },
            Self {
                role: "progress-track",
                color: Color { name: "progress-track", tailwind: "slate-200", css: "hsl(214 32% 91%)" },
            },
            // Avatar:
            Self {
                role: "avatar-bg",
                color: Color { name: "avatar-bg", tailwind: "slate-200", css: "hsl(214 32% 91%)" },
            },
            Self {
                role: "avatar-text",
                color: Color { name: "avatar-text", tailwind: "slate-700", css: "hsl(215 25% 27%)" },
            },
            // Kbd:
            Self {
                role: "kbd-bg",
                color: Color { name: "kbd-bg", tailwind: "slate-100", css: "hsl(214 32% 95%)" },
            },
            Self {
                role: "kbd-text",
                color: Color { name: "kbd-text", tailwind: "slate-700", css: "hsl(215 25% 27%)" },
            },
            Self {
                role: "kbd-border",
                color: Color { name: "kbd-border", tailwind: "slate-300", css: "hsl(213 27% 84%)" },
            },
            // Trend (Stat block):
            Self {
                role: "trend-up",
                color: Color { name: "trend-up", tailwind: "emerald-600", css: "hsl(158 64% 39%)" },
            },
            Self {
                role: "trend-down",
                color: Color { name: "trend-down", tailwind: "rose-600", css: "hsl(347 77% 49%)" },
            },
            // Stepper:
            Self {
                role: "stepper-done",
                color: Color { name: "stepper-done", tailwind: "emerald-600", css: "hsl(158 64% 39%)" },
            },
            Self {
                role: "stepper-done-bg",
                color: Color { name: "stepper-done-bg", tailwind: "emerald-50", css: "hsl(152 81% 96%)" },
            },
            Self {
                role: "stepper-current",
                color: Color { name: "stepper-current", tailwind: "blue-600", css: "hsl(221 83% 53%)" },
            },
            Self {
                role: "stepper-current-bg",
                color: Color { name: "stepper-current-bg", tailwind: "blue-50", css: "hsl(214 100% 97%)" },
            },
            // Badge tones (neutral / info / success / warning / danger / accent)
            // — each carries a {bg, text, border} triple. Tenants
            // override per tone or per surface as needed.
            Self {
                role: "badge-neutral-bg",
                color: Color { name: "badge-neutral-bg", tailwind: "slate-100", css: "hsl(214 32% 95%)" },
            },
            Self {
                role: "badge-neutral-text",
                color: Color { name: "badge-neutral-text", tailwind: "slate-700", css: "hsl(215 25% 27%)" },
            },
            Self {
                role: "badge-neutral-border",
                color: Color { name: "badge-neutral-border", tailwind: "slate-300", css: "hsl(213 27% 84%)" },
            },
            Self {
                role: "badge-info-bg",
                color: Color { name: "badge-info-bg", tailwind: "sky-100", css: "hsl(204 94% 94%)" },
            },
            Self {
                role: "badge-info-text",
                color: Color { name: "badge-info-text", tailwind: "sky-700", css: "hsl(201 90% 35%)" },
            },
            Self {
                role: "badge-info-border",
                color: Color { name: "badge-info-border", tailwind: "sky-300", css: "hsl(199 95% 74%)" },
            },
            Self {
                role: "badge-success-bg",
                color: Color { name: "badge-success-bg", tailwind: "emerald-100", css: "hsl(149 80% 90%)" },
            },
            Self {
                role: "badge-success-text",
                color: Color { name: "badge-success-text", tailwind: "emerald-700", css: "hsl(158 64% 30%)" },
            },
            Self {
                role: "badge-success-border",
                color: Color { name: "badge-success-border", tailwind: "emerald-300", css: "hsl(156 72% 67%)" },
            },
            Self {
                role: "badge-warning-bg",
                color: Color { name: "badge-warning-bg", tailwind: "amber-100", css: "hsl(48 96% 89%)" },
            },
            Self {
                role: "badge-warning-text",
                color: Color { name: "badge-warning-text", tailwind: "amber-800", css: "hsl(23 83% 31%)" },
            },
            Self {
                role: "badge-warning-border",
                color: Color { name: "badge-warning-border", tailwind: "amber-300", css: "hsl(46 97% 65%)" },
            },
            Self {
                role: "badge-danger-bg",
                color: Color { name: "badge-danger-bg", tailwind: "rose-100", css: "hsl(356 100% 94%)" },
            },
            Self {
                role: "badge-danger-text",
                color: Color { name: "badge-danger-text", tailwind: "rose-700", css: "hsl(345 83% 41%)" },
            },
            Self {
                role: "badge-danger-border",
                color: Color { name: "badge-danger-border", tailwind: "rose-300", css: "hsl(352 96% 79%)" },
            },
            Self {
                role: "badge-accent-bg",
                color: Color { name: "badge-accent-bg", tailwind: "indigo-100", css: "hsl(226 100% 94%)" },
            },
            Self {
                role: "badge-accent-text",
                color: Color { name: "badge-accent-text", tailwind: "indigo-700", css: "hsl(229 76% 40%)" },
            },
            Self {
                role: "badge-accent-border",
                color: Color { name: "badge-accent-border", tailwind: "indigo-300", css: "hsl(230 94% 77%)" },
            },
        ]
    }

    /// Look up by role name. Returns `None` for unknown roles — caller
    /// (typically `loom-lint`) should fail loudly if an unknown role
    /// shows up in source.
    #[must_use]
    pub fn by_name(role: &str) -> Option<&'static Self> {
        Self::all().iter().find(|r| r.role == role)
    }

    /// Dark-theme parallel of [`Self::all`]. Same role names, same
    /// order; the `color` field carries the dark resolution.
    ///
    /// A component composes both layers like:
    /// `bg-{light_tailwind} dark:bg-{dark_tailwind}`. The Tailwind
    /// `dark:` variant kicks in based on the `<html>` data-attribute
    /// or `prefers-color-scheme` media query — Tailwind resolves
    /// which strategy at compile time.
    ///
    /// BUG ASSUMPTION: every role here corresponds 1:1 to a role in
    /// [`Self::all`]. The `dark_palette_parity` test enforces this.
    #[must_use]
    #[allow(clippy::too_many_lines)] // it's a flat data table.
    pub const fn dark_all() -> &'static [Self] {
        &[
            Self {
                role: "primary",
                color: Color {
                    // Brighten primary in dark mode so it stands out
                    // against a slate-950 surface; use the 400 step
                    // for pop without sacrificing contrast.
                    name: "primary",
                    tailwind: "primary",
                    css: "hsl(220 90% 65%)",
                },
            },
            Self {
                role: "primary-fg",
                color: Color {
                    name: "primary-fg",
                    tailwind: "slate-950",
                    css: "hsl(222 47% 6%)",
                },
            },
            Self {
                role: "surface",
                color: Color {
                    name: "surface",
                    tailwind: "slate-950",
                    css: "hsl(222 47% 6%)",
                },
            },
            Self {
                role: "surface-muted",
                color: Color {
                    name: "surface-muted",
                    tailwind: "slate-900",
                    css: "hsl(222 47% 11%)",
                },
            },
            Self {
                role: "ink",
                color: Color {
                    name: "ink",
                    tailwind: "slate-50",
                    css: "hsl(210 40% 98%)",
                },
            },
            Self {
                role: "ink-muted",
                color: Color {
                    name: "ink-muted",
                    tailwind: "slate-300",
                    css: "hsl(215 20% 78%)",
                },
            },
            Self {
                role: "border",
                color: Color {
                    name: "border",
                    tailwind: "slate-800",
                    css: "hsl(217 33% 18%)",
                },
            },
            Self {
                role: "danger",
                color: Color {
                    name: "danger",
                    tailwind: "red-400",
                    css: "hsl(0 72% 65%)",
                },
            },
            Self {
                role: "success",
                color: Color {
                    name: "success",
                    tailwind: "emerald-400",
                    css: "hsl(160 84% 55%)",
                },
            },
            // -- dark-theme parallels of the new roles --
            Self {
                role: "warn",
                color: Color {
                    name: "warn",
                    tailwind: "amber-400",
                    css: "hsl(33 100% 66%)",
                },
            },
            Self {
                role: "warn-bg",
                color: Color {
                    name: "warn-bg",
                    tailwind: "amber-950",
                    css: "hsl(33 50% 16%)",
                },
            },
            Self {
                role: "tier-medium",
                color: Color {
                    name: "tier-medium",
                    tailwind: "yellow-400",
                    css: "hsl(50 75% 65%)",
                },
            },
            Self {
                role: "border-strong",
                color: Color {
                    name: "border-strong",
                    tailwind: "slate-700",
                    css: "hsl(218 22% 30%)",
                },
            },
            Self {
                role: "accent-soft",
                color: Color {
                    name: "accent-soft",
                    tailwind: "blue-700",
                    css: "hsl(222 41% 47%)",
                },
            },
            Self {
                role: "accent-glow",
                color: Color {
                    name: "accent-glow",
                    tailwind: "blue-200",
                    css: "hsl(220 100% 85%)",
                },
            },
            Self {
                role: "gradient-a",
                color: Color {
                    name: "gradient-a",
                    tailwind: "blue-400",
                    css: "hsl(218 83% 65%)",
                },
            },
            Self {
                role: "gradient-b",
                color: Color {
                    name: "gradient-b",
                    tailwind: "purple-400",
                    css: "hsl(272 84% 65%)",
                },
            },
            Self {
                role: "bg-canvas",
                color: Color {
                    name: "bg-canvas",
                    tailwind: "slate-950",
                    css: "hsl(220 33% 6%)",
                },
            },
            Self {
                role: "bg-overlay",
                color: Color {
                    name: "bg-overlay",
                    tailwind: "slate-800",
                    css: "hsl(220 24% 20%)",
                },
            },
            // ── Cascade aliases (#336) — dark theme ──
            Self {
                role: "bg",
                color: Color { name: "bg", tailwind: "slate-950", css: "hsl(220 33% 6%)" },
            },
            Self {
                role: "text",
                color: Color { name: "text", tailwind: "slate-100", css: "hsl(210 40% 96%)" },
            },
            Self {
                role: "muted",
                color: Color { name: "muted", tailwind: "slate-400", css: "hsl(213 27% 64%)" },
            },
            Self {
                role: "accent-2",
                color: Color { name: "accent-2", tailwind: "indigo-400", css: "hsl(234 89% 74%)" },
            },
            Self {
                role: "focus",
                color: Color { name: "focus", tailwind: "blue-400", css: "hsl(213 94% 68%)" },
            },
            Self {
                role: "on-primary",
                color: Color { name: "on-primary", tailwind: "slate-950", css: "hsl(220 33% 6%)" },
            },
            Self {
                role: "on-dark",
                color: Color { name: "on-dark", tailwind: "slate-100", css: "hsl(210 40% 96%)" },
            },
            Self {
                role: "primary-hover",
                color: Color { name: "primary-hover", tailwind: "primary-300", css: "hsl(220 90% 70%)" },
            },
            Self {
                role: "hero-bg",
                color: Color { name: "hero-bg", tailwind: "transparent", css: "transparent" },
            },
            Self {
                role: "quote-bg",
                color: Color { name: "quote-bg", tailwind: "transparent", css: "transparent" },
            },
            Self {
                role: "quote-text",
                color: Color { name: "quote-text", tailwind: "slate-300", css: "hsl(213 27% 84%)" },
            },
            Self {
                role: "code-bg",
                color: Color { name: "code-bg", tailwind: "slate-800", css: "hsl(220 24% 20%)" },
            },
            Self {
                role: "code-text",
                color: Color { name: "code-text", tailwind: "slate-100", css: "hsl(210 40% 96%)" },
            },
            Self {
                role: "table-stripe",
                color: Color { name: "table-stripe", tailwind: "slate-900", css: "hsl(222 47% 11%)" },
            },
            Self {
                role: "progress-track",
                color: Color { name: "progress-track", tailwind: "slate-800", css: "hsl(220 24% 20%)" },
            },
            Self {
                role: "avatar-bg",
                color: Color { name: "avatar-bg", tailwind: "slate-700", css: "hsl(215 25% 27%)" },
            },
            Self {
                role: "avatar-text",
                color: Color { name: "avatar-text", tailwind: "slate-200", css: "hsl(214 32% 91%)" },
            },
            Self {
                role: "kbd-bg",
                color: Color { name: "kbd-bg", tailwind: "slate-800", css: "hsl(220 24% 20%)" },
            },
            Self {
                role: "kbd-text",
                color: Color { name: "kbd-text", tailwind: "slate-200", css: "hsl(214 32% 91%)" },
            },
            Self {
                role: "kbd-border",
                color: Color { name: "kbd-border", tailwind: "slate-700", css: "hsl(215 25% 27%)" },
            },
            Self {
                role: "trend-up",
                color: Color { name: "trend-up", tailwind: "emerald-400", css: "hsl(158 64% 52%)" },
            },
            Self {
                role: "trend-down",
                color: Color { name: "trend-down", tailwind: "rose-400", css: "hsl(351 95% 71%)" },
            },
            Self {
                role: "stepper-done",
                color: Color { name: "stepper-done", tailwind: "emerald-400", css: "hsl(158 64% 52%)" },
            },
            Self {
                role: "stepper-done-bg",
                color: Color { name: "stepper-done-bg", tailwind: "emerald-950", css: "hsl(166 88% 8%)" },
            },
            Self {
                role: "stepper-current",
                color: Color { name: "stepper-current", tailwind: "blue-400", css: "hsl(213 94% 68%)" },
            },
            Self {
                role: "stepper-current-bg",
                color: Color { name: "stepper-current-bg", tailwind: "blue-950", css: "hsl(229 84% 13%)" },
            },
            Self {
                role: "badge-neutral-bg",
                color: Color { name: "badge-neutral-bg", tailwind: "slate-800", css: "hsl(220 24% 20%)" },
            },
            Self {
                role: "badge-neutral-text",
                color: Color { name: "badge-neutral-text", tailwind: "slate-200", css: "hsl(214 32% 91%)" },
            },
            Self {
                role: "badge-neutral-border",
                color: Color { name: "badge-neutral-border", tailwind: "slate-700", css: "hsl(215 25% 27%)" },
            },
            Self {
                role: "badge-info-bg",
                color: Color { name: "badge-info-bg", tailwind: "sky-950", css: "hsl(204 80% 16%)" },
            },
            Self {
                role: "badge-info-text",
                color: Color { name: "badge-info-text", tailwind: "sky-300", css: "hsl(199 95% 74%)" },
            },
            Self {
                role: "badge-info-border",
                color: Color { name: "badge-info-border", tailwind: "sky-800", css: "hsl(201 90% 27%)" },
            },
            Self {
                role: "badge-success-bg",
                color: Color { name: "badge-success-bg", tailwind: "emerald-950", css: "hsl(166 88% 8%)" },
            },
            Self {
                role: "badge-success-text",
                color: Color { name: "badge-success-text", tailwind: "emerald-300", css: "hsl(156 72% 67%)" },
            },
            Self {
                role: "badge-success-border",
                color: Color { name: "badge-success-border", tailwind: "emerald-800", css: "hsl(163 88% 20%)" },
            },
            Self {
                role: "badge-warning-bg",
                color: Color { name: "badge-warning-bg", tailwind: "amber-950", css: "hsl(20 91% 14%)" },
            },
            Self {
                role: "badge-warning-text",
                color: Color { name: "badge-warning-text", tailwind: "amber-300", css: "hsl(46 97% 65%)" },
            },
            Self {
                role: "badge-warning-border",
                color: Color { name: "badge-warning-border", tailwind: "amber-800", css: "hsl(23 83% 31%)" },
            },
            Self {
                role: "badge-danger-bg",
                color: Color { name: "badge-danger-bg", tailwind: "rose-950", css: "hsl(343 88% 12%)" },
            },
            Self {
                role: "badge-danger-text",
                color: Color { name: "badge-danger-text", tailwind: "rose-300", css: "hsl(352 96% 79%)" },
            },
            Self {
                role: "badge-danger-border",
                color: Color { name: "badge-danger-border", tailwind: "rose-800", css: "hsl(343 80% 27%)" },
            },
            Self {
                role: "badge-accent-bg",
                color: Color { name: "badge-accent-bg", tailwind: "indigo-950", css: "hsl(232 62% 16%)" },
            },
            Self {
                role: "badge-accent-text",
                color: Color { name: "badge-accent-text", tailwind: "indigo-300", css: "hsl(230 94% 77%)" },
            },
            Self {
                role: "badge-accent-border",
                color: Color { name: "badge-accent-border", tailwind: "indigo-800", css: "hsl(232 70% 30%)" },
            },
        ]
    }

    /// Look up the dark-theme mapping for a role.
    #[must_use]
    pub fn dark_by_name(role: &str) -> Option<&'static Self> {
        Self::dark_all().iter().find(|r| r.role == role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_role_present() {
        let r = ColorRole::by_name("primary").expect("primary role exists");
        assert_eq!(r.color.tailwind, "primary");
    }

    #[test]
    fn unknown_role_returns_none() {
        assert!(ColorRole::by_name("vermillion-electric").is_none());
    }

    #[test]
    fn role_names_are_unique() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for r in ColorRole::all() {
            assert!(seen.insert(r.role), "duplicate role: {}", r.role);
        }
    }

    #[test]
    fn role_names_are_meaning_not_appearance() {
        // Reviewer guard: the role list should never include
        // appearance-named entries like "blue" or "red".
        for r in ColorRole::all() {
            for forbidden in &["blue", "red", "green", "yellow", "purple"] {
                assert!(
                    !r.role.contains(forbidden),
                    "role {} appearance-named, use a meaning name",
                    r.role
                );
            }
        }
    }

    #[test]
    fn dark_palette_parity() {
        // Every light role must have a dark-mode counterpart with the
        // same role name. A missing counterpart leaves a hole in the
        // dark theme that components would discover at runtime; we
        // catch it at test time instead.
        let light: std::collections::HashSet<_> = ColorRole::all().iter().map(|r| r.role).collect();
        let dark: std::collections::HashSet<_> =
            ColorRole::dark_all().iter().map(|r| r.role).collect();
        assert_eq!(
            light, dark,
            "dark palette must mirror light palette role-for-role"
        );
    }

    #[test]
    fn dark_lookup_returns_dark_color() {
        // Sanity: the dark resolution of `surface` should be a dark
        // color (slate-950), distinct from the light resolution.
        let light = ColorRole::by_name("surface").expect("surface light");
        let dark = ColorRole::dark_by_name("surface").expect("surface dark");
        assert_ne!(light.color.tailwind, dark.color.tailwind);
        assert_eq!(dark.color.tailwind, "slate-950");
    }
}
