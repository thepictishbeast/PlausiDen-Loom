//! `loom gtk-theme` subcommand — emit a GTK 4 stylesheet derived
//! from the loom-tokens palette.
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3). Each subcommand should live in its own module
//! so the entrypoint stays small and the command surface is
//! greppable.

use std::fmt::Write as _;

use loom_tokens::ColorRole;

/// Generate the GTK 4 stylesheet CSS. `dark = true` emits the
/// dark palette; `false` emits light.
pub fn cmd_gtk_theme(dark: bool) -> String {
    let palette = if dark {
        ColorRole::dark_all()
    } else {
        ColorRole::all()
    };
    let mode = if dark { "dark" } else { "light" };
    let mut out = String::new();
    let _ = writeln!(
        out,
        "/* GTK 4 theme generated from loom-tokens ({mode}). */"
    );
    out.push_str("/* Do not edit by hand — re-run `loom gtk-theme` after a token change. */\n\n");
    out.push_str(":root {\n");
    for role in palette {
        let _ = writeln!(out, "  --loom-{}: {};", role.role, role.color.css);
    }
    out.push_str("}\n\n");
    // Map a few critical GTK named colors to Loom roles. GTK named
    // colors are referenced by `@name` in widget CSS.
    out.push_str("@define-color theme_bg_color var(--loom-surface);\n");
    out.push_str("@define-color theme_fg_color var(--loom-ink);\n");
    out.push_str("@define-color theme_base_color var(--loom-surface-muted);\n");
    out.push_str("@define-color theme_text_color var(--loom-ink);\n");
    out.push_str("@define-color theme_selected_bg_color var(--loom-primary);\n");
    out.push_str("@define-color theme_selected_fg_color var(--loom-primary-fg);\n");
    out.push_str("@define-color borders var(--loom-border);\n");
    out.push_str("@define-color error_color var(--loom-danger);\n");
    out.push_str("@define-color success_color var(--loom-success);\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_theme_emits_root_block() {
        let css = cmd_gtk_theme(false);
        assert!(css.contains("(light)"));
        assert!(css.contains(":root {"));
        assert!(css.contains("--loom-"));
        assert!(css.contains("@define-color theme_bg_color"));
    }

    #[test]
    fn dark_theme_emits_root_block() {
        let css = cmd_gtk_theme(true);
        assert!(css.contains("(dark)"));
        assert!(css.contains(":root {"));
    }
}
