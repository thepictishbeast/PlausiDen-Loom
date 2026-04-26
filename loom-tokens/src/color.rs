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
}
