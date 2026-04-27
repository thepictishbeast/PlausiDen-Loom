//! `loom-icons` — typed registry of vetted SVG icons.
//!
//! Every icon in this crate is a `&'static Icon` constant. Components
//! (`Button`, `FeatureCard`, contact-row helpers) accept icon references
//! by name; raw inline `SVG` strings in views are replaced by the
//! constant.
//!
//! Icons are derived from Lucide (lucide.dev), MIT-licensed. We keep
//! only the icons actually used in `PlausiDen` UIs — adding a new icon
//! is a one-line const + a doctrine review.
//!
//! SECURITY: SVG bodies are trusted content. The registry never
//! accepts user-provided SVG; only the constants here may flow into
//! components. A reviewer adding an icon must verify the source.

#![doc(html_no_source)]

/// One vetted icon.
///
/// `body` is the inline SVG markup *including* the outer `<svg>`
/// element. `default_class` is the Tailwind class string the icon
/// ships with (size + color). Callers usually use the icon as-is;
/// alternative sizes are exposed via [`Icon::with_class`].
#[derive(Debug, Clone, Copy)]
pub struct Icon {
    /// `snake_case` identifier for audits + tests.
    pub id: &'static str,
    /// `SVG` markup with placeholder `__CLS__` for the class string.
    /// Use [`Icon::render`] / [`Icon::render_with_class`] to materialize.
    pub template: &'static str,
    /// Default Tailwind class string (size + color).
    pub default_class: &'static str,
}

impl Icon {
    /// Render with the default class.
    #[must_use]
    pub fn render(&self) -> String {
        self.template.replace("__CLS__", self.default_class)
    }

    /// Render with an override class string.
    #[must_use]
    pub fn render_with_class(&self, class: &str) -> String {
        self.template.replace("__CLS__", class)
    }
}

/// Helper to construct an icon template. Internal — used only by the
/// `define_icon!` macro below.
#[macro_export]
#[doc(hidden)]
macro_rules! define_icon {
    ($name:ident, $id:literal, $default_class:literal, $body:literal) => {
        #[doc = concat!("Lucide icon: `", $id, "`.")]
        pub const $name: Icon = Icon {
            id: $id,
            template: concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="__CLS__">"#,
                $body,
                r#"</svg>"#
            ),
            default_class: $default_class,
        };
    };
}

// ----- Icon registry --------------------------------------------------
//
// Each icon ships a Lucide path body. The default class is the size +
// color we use on most pages; callers can override via render_with_class.

define_icon!(
    PHONE,
    "phone",
    "w-6 h-6 text-primary",
    r#"<path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"/>"#
);

define_icon!(
    MAIL,
    "mail",
    "w-6 h-6 text-primary",
    r#"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/>"#
);

define_icon!(
    MAP_PIN,
    "map-pin",
    "w-6 h-6 text-primary",
    r#"<path d="M20 10c0 4.993-5.539 10.193-7.399 11.799a1 1 0 0 1-1.202 0C9.539 20.193 4 14.993 4 10a8 8 0 0 1 16 0"/><circle cx="12" cy="10" r="3"/>"#
);

define_icon!(
    SHIELD,
    "shield",
    "w-6 h-6 text-primary",
    r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"#
);

define_icon!(
    LOCK,
    "lock",
    "w-6 h-6 text-primary",
    r#"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>"#
);

define_icon!(
    FILE_TEXT,
    "file-text",
    "w-6 h-6 text-primary",
    r#"<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>"#
);

define_icon!(
    CLIPBOARD_CHECK,
    "clipboard-check",
    "w-6 h-6 text-primary",
    r#"<path d="M9 11l3 3 8-8"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/>"#
);

define_icon!(
    USERS,
    "users",
    "w-6 h-6 text-primary",
    r#"<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>"#
);

define_icon!(
    HEART,
    "heart",
    "w-6 h-6 text-primary",
    r#"<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.29 1.51 4.04 3 5.5l7 7Z"/>"#
);

define_icon!(
    GLOBE,
    "globe",
    "w-6 h-6 text-primary",
    r#"<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>"#
);

define_icon!(
    CHECK,
    "check",
    "w-5 h-5 text-emerald-600 mt-0.5 shrink-0",
    r#"<polyline points="20 6 9 17 4 12"/>"#
);

define_icon!(
    ARROW_RIGHT,
    "arrow-right",
    "w-4 h-4",
    r#"<line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>"#
);

define_icon!(
    MENU,
    "menu",
    "w-6 h-6",
    r#"<line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="18" y2="18"/>"#
);

define_icon!(
    SERVER,
    "server",
    "w-7 h-7 text-primary group-hover:text-white transition-colors",
    r#"<rect width="20" height="8" x="2" y="2" rx="2" ry="2"/><rect width="20" height="8" x="2" y="14" rx="2" ry="2"/><line x1="6" x2="6.01" y1="6" y2="6"/><line x1="6" x2="6.01" y1="18" y2="18"/>"#
);

define_icon!(
    BRAIN_CIRCUIT,
    "brain-circuit",
    "w-7 h-7 text-primary group-hover:text-white transition-colors",
    r#"<path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z"/><path d="M9 13a4.5 4.5 0 0 0 3-4"/><path d="M6.003 5.125A3 3 0 0 0 6.401 6.5"/><path d="M3.477 10.896a4 4 0 0 1 .585-.396"/><path d="M6 18a4 4 0 0 1-1.967-.516"/><path d="M12 13h4"/><path d="M12 18h6a2 2 0 0 1 2 2v1"/><path d="M12 8h8"/><path d="M16 8V5a2 2 0 0 1 2-2"/><circle cx="16" cy="13" r=".5"/><circle cx="18" cy="3" r=".5"/><circle cx="20" cy="21" r=".5"/><circle cx="20" cy="8" r=".5"/>"#
);

define_icon!(
    SETTINGS,
    "settings",
    "w-7 h-7 text-primary group-hover:text-white transition-colors",
    r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#
);

define_icon!(
    CODE,
    "code",
    "w-7 h-7 text-primary group-hover:text-white transition-colors",
    r#"<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>"#
);

define_icon!(
    CPU,
    "cpu",
    "w-7 h-7 text-primary group-hover:text-white transition-colors",
    r#"<rect width="16" height="16" x="4" y="4" rx="2"/><rect width="6" height="6" x="9" y="9" rx="1"/><path d="M15 2v2"/><path d="M15 20v2"/><path d="M2 15h2"/><path d="M2 9h2"/><path d="M20 15h2"/><path d="M20 9h2"/><path d="M9 2v2"/><path d="M9 20v2"/>"#
);

define_icon!(
    TERMINAL,
    "terminal",
    "w-4 h-4",
    r#"<polyline points="4 17 10 11 4 5"/><line x1="12" x2="20" y1="19" y2="19"/>"#
);

define_icon!(
    CIRCLE_CHECK,
    "circle-check",
    "w-6 h-6 text-primary shrink-0",
    r#"<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>"#
);

/// All registered icons. Used by tests and the `loom report` CLI.
#[must_use]
pub const fn all() -> &'static [&'static Icon] {
    &[
        &PHONE,
        &MAIL,
        &MAP_PIN,
        &SHIELD,
        &LOCK,
        &FILE_TEXT,
        &CLIPBOARD_CHECK,
        &USERS,
        &HEART,
        &GLOBE,
        &CHECK,
        &ARROW_RIGHT,
        &MENU,
        &SERVER,
        &BRAIN_CIRCUIT,
        &SETTINGS,
        &CODE,
        &CPU,
        &TERMINAL,
        &CIRCLE_CHECK,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_class() {
        let s = PHONE.render();
        assert!(s.contains(r#"class="w-6 h-6 text-primary""#));
        assert!(s.contains("<svg"));
        assert!(!s.contains("__CLS__"));
    }

    #[test]
    fn render_with_class_overrides() {
        let s = PHONE.render_with_class("w-4 h-4 text-white");
        assert!(s.contains(r#"class="w-4 h-4 text-white""#));
        assert!(!s.contains("text-primary"));
    }

    #[test]
    fn ids_are_unique() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for ico in all() {
            assert!(seen.insert(ico.id), "duplicate id: {}", ico.id);
        }
    }

    #[test]
    fn every_icon_has_svg_body() {
        for ico in all() {
            assert!(ico.template.contains("<svg"), "{} missing svg open", ico.id);
            assert!(
                ico.template.contains("</svg>"),
                "{} missing svg close",
                ico.id
            );
            assert!(
                ico.template.contains("__CLS__"),
                "{} missing class slot",
                ico.id
            );
        }
    }

    #[test]
    fn registry_size_meets_minimum() {
        assert!(all().len() >= 13, "registry shrunk below minimum");
    }
}
