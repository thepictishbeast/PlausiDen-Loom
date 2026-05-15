//! Vendored Lucide icon set (MIT-licensed). T67 cycle 96 iter 12.
//!
//! Lucide is a fork of Feather Icons maintained by an open-source
//! community since 2020. ~1400 icons total; this module vendors a
//! curated **starter subset** (~30 commonly used icons) covering
//! navigation, actions, status, social, and theme controls — the
//! categories that show up on essentially every website.
//!
//! ATTRIBUTION
//! ----------
//! Icons reproduced verbatim from <https://lucide.dev> under the
//! ISC license (Lucide is dual-licensed ISC/MIT historically;
//! current main repo uses ISC. See LUCIDE-LICENSE.txt at the
//! workspace root once vendored). Source paths are 24×24 viewBox,
//! stroke-based, currentColor — drop-in for any color theme.
//!
//! API
//! ---
//! - [`lucide_icon_inner(name)`] — returns the inner SVG content
//!   (paths, circles, etc.) for a known icon name. None for unknown.
//!   Useful when composing custom `<svg>` wrappers.
//! - [`lucide_icon_svg(name, size, aria_label)`] — returns a
//!   complete `<svg>` ready to embed. Sets aria-label (or
//!   aria-hidden when None), width/height = size px, viewBox 24×24,
//!   stroke=currentColor, stroke-width=2.
//! - [`LUCIDE_ICONS`] — slice of every (name, inner_svg) pair so
//!   callers can iterate / build pickers / catalog pages.
//!
//! WHY VENDOR (not depend on a crate)
//! -----------------------------------
//! Per AVP-2 doctrine, every dependency is attack surface. Lucide
//! ships as ~70KB of SVG strings that never change shape over the
//! life of an icon. Vendoring 30 of them as `&'static str` adds
//! zero compile time, zero binary bloat (stripped at link), and
//! removes any update-treadmill risk. New icons added by extending
//! this file (one entry per icon).
//!
//! NEXT STEPS (future cycles)
//! ---------------------------
//! - Wire into `loom-cms-render` as a `CmsSection` variant for icon
//!   buttons + a CSS class for inline icons in text.
//! - Forge build phase that subset-strips the binary based on which
//!   icon names actually appear in cms/*.json (so a site that uses
//!   3 icons doesn't ship the entire 1400-icon catalog).
//! - Expand the curated subset as real sites surface gaps.

#![allow(missing_docs)] // every const is self-documenting via name

/// Each entry: (lucide-name, inner-svg-content). Inner content
/// goes inside a 24×24 viewBox with stroke=currentColor stroke-width=2.
pub const LUCIDE_ICONS: &[(&str, &str)] = &[
    // --- arrows + chevrons ---
    ("arrow-left", r#"<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>"#),
    ("arrow-right", r#"<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>"#),
    ("arrow-up", r#"<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>"#),
    ("arrow-down", r#"<path d="M12 5v14"/><path d="m19 12-7 7-7-7"/>"#),
    ("chevron-left", r#"<path d="m15 18-6-6 6-6"/>"#),
    ("chevron-right", r#"<path d="m9 18 6-6-6-6"/>"#),
    ("chevron-up", r#"<path d="m18 15-6-6-6 6"/>"#),
    ("chevron-down", r#"<path d="m6 9 6 6 6-6"/>"#),

    // --- actions ---
    ("plus", r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#),
    ("minus", r#"<path d="M5 12h14"/>"#),
    ("x", r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#),
    ("check", r#"<path d="M20 6 9 17l-5-5"/>"#),
    ("trash", r#"<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"#),
    ("edit", r#"<path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z"/>"#),
    ("download", r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/>"#),
    ("upload", r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" x2="12" y1="3" y2="15"/>"#),
    ("share", r#"<path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/><polyline points="16 6 12 2 8 6"/><line x1="12" x2="12" y1="2" y2="15"/>"#),
    ("copy", r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#),

    // --- nav + UI ---
    ("menu", r#"<line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="18" y2="18"/>"#),
    ("search", r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>"#),
    ("settings", r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#),
    ("home", r#"<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/>"#),
    ("user", r#"<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>"#),
    ("bell", r#"<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>"#),

    // --- status + content ---
    ("info", r#"<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="16" y2="12"/><line x1="12" x2="12.01" y1="8" y2="8"/>"#),
    ("alert-triangle", r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#),
    ("alert-circle", r#"<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>"#),
    ("check-circle", r#"<path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>"#),
    ("heart", r#"<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.29 1.51 4.04 3 5.5l7 7Z"/>"#),
    ("star", r#"<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>"#),
    ("eye", r#"<path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z"/><circle cx="12" cy="12" r="3"/>"#),

    // --- theme + media ---
    ("sun", r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>"#),
    ("moon", r#"<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>"#),
    ("image", r#"<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>"#),
    ("play", r#"<polygon points="6 3 20 12 6 21 6 3"/>"#),
    ("pause", r#"<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>"#),

    // --- comms + links ---
    ("mail", r#"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/>"#),
    ("link", r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#),
    ("external-link", r#"<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" x2="21" y1="14" y2="3"/>"#),
];

/// Look up an icon's inner SVG content by Lucide name. None for unknown.
#[must_use]
pub fn lucide_icon_inner(name: &str) -> Option<&'static str> {
    LUCIDE_ICONS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, svg)| *svg)
}

/// Render a Lucide icon as a complete `<svg>` element, ready to
/// inline into HTML. `size` is in pixels. `aria_label`:
/// - `Some(text)` → `aria-label="…"` + `role="img"` (icon
///   carries semantic meaning; screen readers announce the label).
/// - `None` → `aria-hidden="true"` (icon is decorative; screen
///   readers skip it. Caller is responsible for ensuring the
///   surrounding text already conveys the meaning).
///
/// Returns `None` if `name` isn't in `LUCIDE_ICONS`.
#[must_use]
pub fn lucide_icon_svg(name: &str, size: u32, aria_label: Option<&str>) -> Option<String> {
    let inner = lucide_icon_inner(name)?;
    let aria = match aria_label {
        Some(text) => format!(r#"role="img" aria-label="{}""#, escape_attr(text)),
        None => r#"aria-hidden="true""#.to_owned(),
    };
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" {aria} class="lucide lucide-{name}">{inner}</svg>"#
    ))
}

/// Minimal HTML-attr escape for the aria-label text.
fn escape_attr(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_set_is_non_empty() {
        assert!(LUCIDE_ICONS.len() >= 30, "starter set should have ≥30 icons");
    }

    #[test]
    fn vendor_set_has_no_duplicate_names() {
        let mut names: Vec<&str> = LUCIDE_ICONS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let dedup_len = {
            let mut v = names.clone();
            v.dedup();
            v.len()
        };
        assert_eq!(names.len(), dedup_len, "duplicate names in LUCIDE_ICONS");
    }

    #[test]
    fn vendor_set_names_are_kebab_case_lowercase_alphanumeric() {
        for (name, _) in LUCIDE_ICONS {
            for c in name.chars() {
                assert!(
                    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-',
                    "icon {name:?} has invalid char {c:?}"
                );
            }
            assert!(!name.is_empty(), "empty icon name");
            assert!(!name.starts_with('-'), "icon {name:?} starts with -");
            assert!(!name.ends_with('-'), "icon {name:?} ends with -");
        }
    }

    #[test]
    fn lucide_icon_inner_known() {
        let svg = lucide_icon_inner("arrow-right").expect("arrow-right");
        assert!(svg.contains(r#"<path"#));
    }

    #[test]
    fn lucide_icon_inner_unknown_returns_none() {
        assert!(lucide_icon_inner("not-a-real-icon").is_none());
    }

    #[test]
    fn lucide_icon_svg_with_label_has_role_img() {
        let s = lucide_icon_svg("search", 24, Some("Search the site")).unwrap();
        assert!(s.contains(r#"role="img""#));
        assert!(s.contains(r#"aria-label="Search the site""#));
        assert!(!s.contains(r#"aria-hidden="true""#));
        assert!(s.contains("lucide-search"));
    }

    #[test]
    fn lucide_icon_svg_without_label_is_decorative_aria_hidden() {
        let s = lucide_icon_svg("heart", 16, None).unwrap();
        assert!(s.contains(r#"aria-hidden="true""#));
        assert!(!s.contains(r#"role="img""#));
        assert!(!s.contains(r#"aria-label="#));
    }

    #[test]
    fn lucide_icon_svg_size_emitted_as_width_height() {
        let s = lucide_icon_svg("plus", 32, None).unwrap();
        assert!(s.contains(r#"width="32""#));
        assert!(s.contains(r#"height="32""#));
        assert!(s.contains(r#"viewBox="0 0 24 24""#));
    }

    #[test]
    fn lucide_icon_svg_unknown_returns_none() {
        assert!(lucide_icon_svg("bogus", 16, None).is_none());
    }

    #[test]
    fn lucide_icon_svg_aria_label_xss_safe() {
        // Attacker-controlled label gets escaped before emission.
        let s = lucide_icon_svg(
            "info",
            16,
            Some(r#"</svg><script>alert(1)</script>"#),
        )
        .unwrap();
        assert!(!s.contains("<script>"));
        assert!(s.contains("&lt;script&gt;"));
    }

    #[test]
    fn lucide_icon_svg_uses_currentcolor_for_themability() {
        // Critical: stroke must be currentColor so icons inherit
        // the surrounding text color. Otherwise theme switches
        // (T72) wouldn't recolor icons.
        let s = lucide_icon_svg("star", 24, None).unwrap();
        assert!(s.contains(r#"stroke="currentColor""#));
        assert!(s.contains(r#"fill="none""#));
    }

    #[test]
    fn every_vendored_icon_renders_a_complete_svg() {
        // Smoke-test: every icon in the vendored set produces
        // valid-looking SVG output via the public API.
        for (name, _) in LUCIDE_ICONS {
            let s = lucide_icon_svg(name, 24, None).expect(name);
            assert!(s.starts_with("<svg "), "{name}: bad svg start");
            assert!(s.ends_with("</svg>"), "{name}: bad svg end");
            assert!(s.contains(r#"viewBox="0 0 24 24""#), "{name}: missing viewBox");
        }
    }
}
