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
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to_byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
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
                    tailwind: "slate-50",
                    css: "hsl(210 40% 98%)",
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
                    tailwind: "slate-600",
                    css: "hsl(215 16% 47%)",
                },
            },
            Self {
                role: "border",
                color: Color {
                    name: "border",
                    tailwind: "slate-200",
                    css: "hsl(214 32% 91%)",
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
                    tailwind: "slate-400",
                    css: "hsl(215 20% 65%)",
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
