//! Typed `Modal` primitive — accessible dialog with overlay.
//!
//! Renders a `<dialog>` element with role="dialog", aria-modal,
//! aria-labelledby pointing at the title element, plus a backdrop
//! overlay. Native `<dialog>` is used instead of an aria-rolled
//! div so browser focus trapping + keyboard ESC close work without
//! JS — matching the doctrine's "compile-time correctness over
//! runtime checks" preference.
//!
//! ## Variants
//!
//! Modal size is a closed enum: [`ModalSize`] with three steps.
//! The dismiss affordance is required: every modal has a close
//! button (the `aria-label` is enforced by [`Modal::close_label`]).
//!
//! ## What this isn't
//!
//! * Not a popover. Popovers anchor to a trigger; modals don't.
//! * Not a sheet. Sheets slide from an edge; modals center.
//! * Not a toast. See [`crate::toast`].

use maud::{Markup, html};

/// Modal size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalSize {
    /// Compact — confirm dialogs, single-line prompts.
    Sm,
    /// Default — most modals.
    Md,
    /// Large — content-heavy (forms, terms acceptance).
    Lg,
}

/// A typed modal dialog.
pub struct Modal<'a> {
    /// Stable element id used by `aria-labelledby` to point at the
    /// title and by JS to open/close. Keep it alphanumeric +
    /// hyphen.
    pub id: &'a str,
    /// Visible title text (rendered as `<h2>`).
    pub title: &'a str,
    /// Body content. Caller composes — typically a paragraph or
    /// a typed form.
    pub body: Markup,
    /// Footer content (typically buttons composed via
    /// [`crate::Button`]). Empty markup if none.
    pub footer: Markup,
    /// Physical size.
    pub size: ModalSize,
    /// Aria label on the dismiss button. Required so screen readers
    /// announce "Close <X>" instead of just "Close".
    pub close_label: &'a str,
}

impl Modal<'_> {
    /// Render the modal. Caller is responsible for adding the
    /// open/close trigger; this primitive emits the dialog markup
    /// only.
    #[must_use]
    pub fn render(&self) -> Markup {
        let size_class = match self.size {
            ModalSize::Sm => "max-w-sm",
            ModalSize::Md => "max-w-lg",
            ModalSize::Lg => "max-w-2xl",
        };
        let title_id = format!("{}-title", self.id);
        html! {
            dialog id=(self.id) class=(format!("rounded-xl border border-slate-200 shadow-2xl p-0 bg-white {size_class} w-full backdrop:bg-slate-900/40")) aria-labelledby=(title_id) {
                div class="p-6" {
                    div class="flex items-start justify-between mb-4" {
                        h2 id=(title_id) class="font-display text-xl font-bold text-slate-900" {
                            (self.title)
                        }
                        // Close button — uses formmethod=dialog so
                        // browsers natively close the dialog without JS.
                        form method="dialog" {
                            button type="submit" aria-label=(self.close_label) class="rounded-md p-1 text-slate-400 hover:text-slate-700 hover:bg-slate-100" {
                                "×"
                            }
                        }
                    }
                    div class="text-slate-600 leading-relaxed" {
                        (self.body)
                    }
                    div class="mt-6 flex items-center justify-end gap-2" {
                        (self.footer)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture<'a>() -> Modal<'a> {
        Modal {
            id: "test-modal",
            title: "Confirm",
            body: html! { p { "Are you sure?" } },
            footer: html! { button { "OK" } },
            size: ModalSize::Md,
            close_label: "Close confirm dialog",
        }
    }

    #[test]
    fn renders_native_dialog_with_aria_label() {
        let s = fixture().render().into_string();
        assert!(s.contains("<dialog"));
        assert!(s.contains(r#"aria-labelledby="test-modal-title""#));
        assert!(s.contains(r#"id="test-modal-title""#));
    }

    #[test]
    fn close_button_uses_dialog_form_method() {
        let s = fixture().render().into_string();
        assert!(s.contains(r#"method="dialog""#));
        assert!(s.contains(r#"aria-label="Close confirm dialog""#));
    }

    #[test]
    fn size_maps_to_max_width_class() {
        let small = Modal {
            size: ModalSize::Sm,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(small.contains("max-w-sm"));
        let large = Modal {
            size: ModalSize::Lg,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(large.contains("max-w-2xl"));
    }
}
