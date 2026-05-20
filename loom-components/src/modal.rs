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

/// Corner / chrome shape. `Rounded` is the SaaS-canonical back-compat
/// default; `Square` strips to `rounded-none` for the flat editorial
/// dialog that pairs with `FormStyle::Editorial` + `ButtonShape::Square`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalShape {
    /// `rounded-xl` SaaS dialog. Back-compat default.
    #[default]
    Rounded,
    /// `rounded-none` editorial dialog.
    Square,
}

/// Shadow elevation tier. `Pronounced` keeps the legacy `shadow-2xl`;
/// `Soft` drops to `shadow-lg`; `Flat` removes the shadow entirely —
/// the dialog reads as a typographic panel rather than a hovering
/// card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalElevation {
    /// `shadow-2xl` — the legacy SaaS shape. Back-compat default.
    #[default]
    Pronounced,
    /// `shadow-lg` — softer drop.
    Soft,
    /// No shadow — flat editorial panel.
    Flat,
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
    /// announce `Close <X>` instead of just "Close".
    pub close_label: &'a str,
    /// Corner / chrome shape. Defaults to [`ModalShape::Rounded`].
    pub shape: ModalShape,
    /// Shadow elevation tier. Defaults to [`ModalElevation::Pronounced`].
    pub elevation: ModalElevation,
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
        let shape_class = match self.shape {
            ModalShape::Rounded => "rounded-xl",
            ModalShape::Square => "rounded-none",
        };
        let elevation_class = match self.elevation {
            ModalElevation::Pronounced => "shadow-2xl",
            ModalElevation::Soft => "shadow-lg",
            ModalElevation::Flat => "",
        };
        let shape_attr = match self.shape {
            ModalShape::Rounded => "rounded",
            ModalShape::Square => "square",
        };
        let elevation_attr = match self.elevation {
            ModalElevation::Pronounced => "pronounced",
            ModalElevation::Soft => "soft",
            ModalElevation::Flat => "flat",
        };
        let dialog_class = format!(
            "{shape_class} border border-slate-200 {elevation_class} p-0 bg-white {size_class} w-full backdrop:bg-slate-900/40"
        );
        // Close button radius follows the modal shape — square modal,
        // square close button.
        let close_radius = match self.shape {
            ModalShape::Rounded => "rounded-md",
            ModalShape::Square => "rounded-none",
        };
        let title_id = format!("{}-title", self.id);
        html! {
            dialog
                id=(self.id)
                class=(dialog_class)
                aria-labelledby=(title_id)
                data-loom-modal-shape=(shape_attr)
                data-loom-modal-elevation=(elevation_attr)
            {
                div class="p-6" {
                    div class="flex items-start justify-between mb-4" {
                        h2 id=(title_id) class="font-display text-xl font-bold text-slate-900" {
                            (self.title)
                        }
                        // Close button — uses formmethod=dialog so
                        // browsers natively close the dialog without JS.
                        form method="dialog" {
                            button type="submit" aria-label=(self.close_label) class=(format!("{close_radius} p-1 text-slate-400 hover:text-slate-700 hover:bg-slate-100")) {
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
            shape: ModalShape::default(),
            elevation: ModalElevation::default(),
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

    #[test]
    fn default_shape_is_rounded_with_shadow_2xl() {
        let s = fixture().render().into_string();
        assert!(s.contains("rounded-xl"));
        assert!(s.contains("shadow-2xl"));
        assert!(s.contains(r#"data-loom-modal-shape="rounded""#));
        assert!(s.contains(r#"data-loom-modal-elevation="pronounced""#));
    }

    #[test]
    fn square_shape_strips_rounded_corners() {
        let s = Modal {
            shape: ModalShape::Square,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(s.contains("rounded-none"));
        assert!(!s.contains("rounded-xl"));
        assert!(s.contains(r#"data-loom-modal-shape="square""#));
    }

    #[test]
    fn square_shape_cascades_to_close_button() {
        // Close button radius follows modal shape.
        let s = Modal {
            shape: ModalShape::Square,
            ..fixture()
        }
        .render()
        .into_string();
        // Close-button area shouldn't carry rounded-md either.
        assert!(!s.contains("rounded-md"));
    }

    #[test]
    fn flat_elevation_emits_no_shadow() {
        let s = Modal {
            elevation: ModalElevation::Flat,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(!s.contains("shadow-2xl"));
        assert!(!s.contains("shadow-lg"));
        assert!(s.contains(r#"data-loom-modal-elevation="flat""#));
    }

    #[test]
    fn soft_elevation_uses_shadow_lg() {
        let s = Modal {
            elevation: ModalElevation::Soft,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(s.contains("shadow-lg"));
        assert!(!s.contains("shadow-2xl"));
        assert!(s.contains(r#"data-loom-modal-elevation="soft""#));
    }

    #[test]
    fn editorial_modal_combines_square_and_flat() {
        // The composition: editorial dialog with no rounded corners
        // and no shadow — a flat typographic panel.
        let s = Modal {
            shape: ModalShape::Square,
            elevation: ModalElevation::Flat,
            ..fixture()
        }
        .render()
        .into_string();
        assert!(s.contains("rounded-none"));
        assert!(!s.contains("shadow"));
        assert!(s.contains(r#"data-loom-modal-shape="square""#));
        assert!(s.contains(r#"data-loom-modal-elevation="flat""#));
    }

    #[test]
    fn modal_shape_default_is_rounded() {
        assert!(matches!(ModalShape::default(), ModalShape::Rounded));
    }

    #[test]
    fn modal_elevation_default_is_pronounced() {
        assert!(matches!(
            ModalElevation::default(),
            ModalElevation::Pronounced
        ));
    }
}
