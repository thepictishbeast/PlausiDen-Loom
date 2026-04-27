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
        let light: std::collections::HashSet<_> =
            ColorRole::all().iter().map(|r| r.role).collect();
        let dark: std::collections::HashSet<_> =
            ColorRole::dark_all().iter().map(|r| r.role).collect();
        assert_eq!(light, dark, "dark palette must mirror light palette role-for-role");
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
