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
        html! {
            div role=(role) aria-live=(live) data-toast-duration=(duration) class=(format!("rounded-lg border shadow-md px-4 py-3 max-w-sm {tone_classes}")) {
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

    #[test]
    fn info_uses_polite_status_role() {
        let s = Toast {
            title: "Saved",
            body: None,
            tone: ToastTone::Info,
            duration: ToastDuration::Default,
        }
        .render()
        .into_string();
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
        }
        .render()
        .into_string();
        assert!(warn.contains("amber"));
        let danger = Toast {
            title: "Failure",
            body: None,
            tone: ToastTone::Danger,
            duration: ToastDuration::Sticky,
        }
        .render()
        .into_string();
        assert!(danger.contains("red"));
    }
}
