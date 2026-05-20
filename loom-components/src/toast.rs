//! Typed `Toast` primitive — transient notification.
//!
//! Renders a single notification card with a closed-enum [`ToastTone`]
//! (info / success / warning / danger) and a closed-enum
//! [`ToastDuration`]. The container element is `role="status"` for
//! polite tones (info / success) and `role="alert"` for assertive
//! tones (warning / danger), so screen readers behave correctly
//! without runtime JS configuration.
//!
//! ## What this isn't
//!
//! * Not a queue. The caller decides which toasts to render and
//!   when. The render path is pure markup; lifecycle (auto-dismiss
//!   timer, animation) is the caller's responsibility.
//! * Not a modal. See [`crate::modal`].

use maud::{Markup, html};

/// Tone of a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastTone {
    /// Neutral information.
    Info,
    /// Operation succeeded.
    Success,
    /// Soft warning (non-blocking).
    Warning,
    /// Error / failure (assertive).
    Danger,
}

/// Approximate auto-dismiss timing. The actual auto-dismiss is the
/// caller's job; this enum just communicates intent so a JS layer
/// can read the data attribute and start a timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastDuration {
    /// ~3s — confirmations.
    Short,
    /// ~6s — info, warnings.
    Default,
    /// Sticky — caller must dismiss explicitly.
    Sticky,
}

/// Corner / chrome shape. `Rounded` is the SaaS-canonical back-compat
/// default; `Square` strips to `rounded-none` for the flat editorial
/// composition (pairs with `ButtonShape::Square` + `ModalShape::Square`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastShape {
    /// `rounded-lg` toast card. Back-compat default.
    #[default]
    Rounded,
    /// `rounded-none` flat editorial notification strip.
    Square,
}

/// Shadow elevation. `Soft` keeps the legacy `shadow-md`; `Flat`
/// strips the shadow for an editorial inline notification look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastElevation {
    /// `shadow-md` — the legacy SaaS shape. Back-compat default.
    #[default]
    Soft,
    /// No shadow — flat editorial notification.
    Flat,
}

/// A typed toast notification.
pub struct Toast<'a> {
    /// Visible title (one short line).
    pub title: &'a str,
    /// Optional body text below the title.
    pub body: Option<&'a str>,
    /// Tone — drives color + the aria role.
    pub tone: ToastTone,
    /// Approximate auto-dismiss duration.
    pub duration: ToastDuration,
    /// Corner / chrome shape. Defaults to [`ToastShape::Rounded`].
    pub shape: ToastShape,
    /// Shadow elevation tier. Defaults to [`ToastElevation::Soft`].
    pub elevation: ToastElevation,
}

impl Toast<'_> {
    /// Render as a single notification card.
    #[must_use]
    pub fn render(&self) -> Markup {
        let tone_classes = match self.tone {
            ToastTone::Info => "border-slate-200 bg-white text-slate-900",
            ToastTone::Success => "border-emerald-200 bg-emerald-50 text-emerald-900",
            ToastTone::Warning => "border-amber-200 bg-amber-50 text-amber-900",
            ToastTone::Danger => "border-red-200 bg-red-50 text-red-900",
        };
        let role = match self.tone {
            ToastTone::Info | ToastTone::Success => "status",
            ToastTone::Warning | ToastTone::Danger => "alert",
        };
        let live = match self.tone {
            ToastTone::Info | ToastTone::Success => "polite",
            ToastTone::Warning | ToastTone::Danger => "assertive",
        };
        let duration = match self.duration {
            ToastDuration::Short => "short",
            ToastDuration::Default => "default",
            ToastDuration::Sticky => "sticky",
        };
        let shape_class = match self.shape {
            ToastShape::Rounded => "rounded-lg",
            ToastShape::Square => "rounded-none",
        };
        let elevation_class = match self.elevation {
            ToastElevation::Soft => "shadow-md",
            ToastElevation::Flat => "",
        };
        let shape_attr = match self.shape {
            ToastShape::Rounded => "rounded",
            ToastShape::Square => "square",
        };
        let elevation_attr = match self.elevation {
            ToastElevation::Soft => "soft",
            ToastElevation::Flat => "flat",
        };
        let wrapper_class =
            format!("{shape_class} border {elevation_class} px-4 py-3 max-w-sm {tone_classes}");
        html! {
            div
                role=(role)
                aria-live=(live)
                data-toast-duration=(duration)
                data-loom-toast-shape=(shape_attr)
                data-loom-toast-elevation=(elevation_attr)
                class=(wrapper_class)
            {
                p class="font-semibold text-sm" { (self.title) }
                @if let Some(body) = self.body {
                    p class="text-sm mt-1 opacity-80" { (body) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture<'a>() -> Toast<'a> {
        Toast {
            title: "Saved",
            body: None,
            tone: ToastTone::Info,
            duration: ToastDuration::Default,
            shape: ToastShape::default(),
            elevation: ToastElevation::default(),
        }
    }

    #[test]
    fn info_uses_polite_status_role() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"role="status""#));
        assert!(s.contains(r#"aria-live="polite""#));
    }

    #[test]
    fn danger_uses_assertive_alert_role() {
        let s = Toast {
            title: "Failed",
            body: Some("Something broke"),
            tone: ToastTone::Danger,
            duration: ToastDuration::Sticky,
            shape: ToastShape::default(),
            elevation: ToastElevation::default(),
        }
        .render()
        .into_string();
        assert!(s.contains(r#"role="alert""#));
        assert!(s.contains(r#"aria-live="assertive""#));
        assert!(s.contains("Something broke"));
        assert!(s.contains(r#"data-toast-duration="sticky""#));
    }

    #[test]
    fn body_is_optional() {
        let s = Toast {
            title: "Saved",
            body: None,
            tone: ToastTone::Success,
            duration: ToastDuration::Short,
            shape: ToastShape::default(),
            elevation: ToastElevation::default(),
        }
        .render()
        .into_string();
        assert!(s.contains("Saved"));
        // Without body, the second <p> should not appear.
        assert_eq!(s.matches("<p ").count(), 1);
    }

    #[test]
    fn tone_drives_color_classes() {
        let warn = Toast {
            title: "Heads up",
            body: None,
            tone: ToastTone::Warning,
            duration: ToastDuration::Default,
            shape: ToastShape::default(),
            elevation: ToastElevation::default(),
        }
        .render()
        .into_string();
        assert!(warn.contains("amber"));
        let danger = Toast {
            title: "Failure",
            body: None,
            tone: ToastTone::Danger,
            duration: ToastDuration::Sticky,
            shape: ToastShape::default(),
            elevation: ToastElevation::default(),
        }
        .render()
        .into_string();
        assert!(danger.contains("red"));
    }

    #[test]
    fn default_shape_rounded_with_shadow_md() {
        let s = fixture().render().into_string();
        assert!(s.contains("rounded-lg"));
        assert!(s.contains("shadow-md"));
        assert!(s.contains(r#"data-loom-toast-shape="rounded""#));
        assert!(s.contains(r#"data-loom-toast-elevation="soft""#));
    }

    #[test]
    fn square_shape_strips_radius() {
        let s = Toast {
            shape: ToastShape::Square,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(s.contains("rounded-none"));
        assert!(!s.contains("rounded-lg"));
        assert!(s.contains(r#"data-loom-toast-shape="square""#));
    }

    #[test]
    fn flat_elevation_strips_shadow() {
        let s = Toast {
            elevation: ToastElevation::Flat,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(!s.contains("shadow-md"));
        assert!(s.contains(r#"data-loom-toast-elevation="flat""#));
    }

    #[test]
    fn editorial_combo_square_and_flat() {
        let s = Toast {
            shape: ToastShape::Square,
            elevation: ToastElevation::Flat,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(s.contains("rounded-none"));
        assert!(!s.contains("shadow"));
    }

    #[test]
    fn shape_and_elevation_defaults_back_compat() {
        assert!(matches!(ToastShape::default(), ToastShape::Rounded));
        assert!(matches!(ToastElevation::default(), ToastElevation::Soft));
    }
}
