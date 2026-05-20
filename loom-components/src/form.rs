//! Typed form primitives — `TextInput`, `TextArea`, `Select`, `Field`.
//!
//! Every primitive in this module enforces accessible-name binding by
//! construction: the `id` field is non-optional, the `label` field is
//! non-optional, and rendering wires the matching `for=` attribute on
//! the visible `<label>`. axe-core sees the binding; assistive tech
//! announces the field correctly. Skipping the label is a compile error.
//!
//! SECURITY: Length caps (`max_length`) on text inputs are advisory at
//! the client (HTML5 `maxlength`); the server-side handler MUST enforce
//! its own bounds. The handler-bound `name` field is the wire-format
//! contract — renaming a `name` here without updating the handler
//! struct silently drops the field on submit.

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

const LABEL_CLASSES: &str = "text-sm font-medium leading-none";

/// Visual chrome for form controls.
///
/// `Rounded` is the SaaS-shape default (rounded-md, slate-50 background).
/// `Editorial` strips the rounded corners and background — just a 1px
/// bottom border on the input, transparent surface, no pill chrome.
/// `Minimal` is editorial minus the visible border — underline on focus
/// only, designed for in-prose editorial forms (newsletter signups
/// embedded in body text, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormStyle {
    /// SaaS-friendly rounded pill input. Back-compat default.
    #[default]
    Rounded,
    /// Editorial flat input: no rounded corners, bottom-border only,
    /// transparent background. Pairs with `HeroEditorial` + `PullQuote`
    /// editorial compositions.
    Editorial,
    /// Stripped-to-the-bone editorial: no visible border in resting
    /// state, underline on focus. For in-prose form embeds.
    Minimal,
}

/// Vertical density for form controls. `Compact` collapses height
/// from h-12 to h-9; `Comfortable` keeps the spacious default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormDensity {
    /// Tight rhythm — h-9, smaller padding. For dense form grids.
    Compact,
    /// Default rhythm — h-12.
    #[default]
    Comfortable,
}

fn input_classes(style: FormStyle, density: FormDensity, multiline: bool) -> String {
    let mut out = String::with_capacity(200);
    out.push_str("flex w-full border ");
    out.push_str(
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 ring-offset-background text-base md:text-sm",
    );
    // Style-dependent classes.
    match style {
        FormStyle::Rounded => {
            out.push_str(" rounded-md border-input bg-slate-50 px-3 py-2");
        }
        FormStyle::Editorial => {
            out.push_str(" rounded-none border-0 border-b border-slate-300 bg-transparent px-1 py-2");
        }
        FormStyle::Minimal => {
            out.push_str(" rounded-none border-0 border-b border-transparent bg-transparent px-1 py-2 focus-visible:border-slate-400");
        }
    }
    // Density-dependent height.
    if multiline {
        out.push_str(" min-h-[150px] resize-none");
    } else {
        match density {
            FormDensity::Compact => out.push_str(" h-9"),
            FormDensity::Comfortable => out.push_str(" h-12"),
        }
    }
    out
}

/// HTML5 input `type=` attribute. Constrained — adding a variant is a
/// doctrine review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    /// Plain text.
    Text,
    /// Email address — browsers may surface mailto autofill + soft validation.
    Email,
    /// Telephone — surfaces a numeric keypad on mobile.
    Tel,
    /// URL.
    Url,
}

impl InputType {
    const fn html(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Email => "email",
            Self::Tel => "tel",
            Self::Url => "url",
        }
    }
}

/// Single-line text input with a bound visible label.
pub struct TextInput<'a> {
    /// Stable HTML `id`. Used both as the input id and the label `for`
    /// target. Should be page-unique.
    pub id: &'a str,
    /// Form field name (POST key). Must match the server handler.
    pub name: &'a str,
    /// Visible label text. Required — accessibility floor.
    pub label: &'a str,
    /// HTML5 input type.
    pub input_type: InputType,
    /// Placeholder hint (optional).
    pub placeholder: Option<&'a str>,
    /// HTML5 maxlength (advisory; server validates the real cap).
    pub max_length: Option<usize>,
    /// `required` attribute.
    pub required: bool,
    /// Visual chrome. Defaults to `Rounded` (the back-compat SaaS shape).
    pub style: FormStyle,
    /// Vertical density. Defaults to `Comfortable` (h-12).
    pub density: FormDensity,
}

impl TextInput<'_> {
    /// Render label + input pair.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = input_classes(self.style, self.density, false);
        html! {
            div class="space-y-2" data-loom-form-style=(form_style_attr(self.style)) {
                label class=(LABEL_CLASSES) for=(self.id) { (self.label) }
                input
                    type=(self.input_type.html())
                    id=(self.id)
                    name=(self.name)
                    class=(class)
                    placeholder=[self.placeholder]
                    maxlength=[self.max_length.map(|n| n.to_string())]
                    required[self.required];
            }
        }
    }
}

fn form_style_attr(style: FormStyle) -> &'static str {
    match style {
        FormStyle::Rounded => "rounded",
        FormStyle::Editorial => "editorial",
        FormStyle::Minimal => "minimal",
    }
}

/// Multi-line textarea with a bound visible label.
pub struct TextArea<'a> {
    /// Stable HTML `id`.
    pub id: &'a str,
    /// Form field name.
    pub name: &'a str,
    /// Visible label text.
    pub label: &'a str,
    /// Placeholder hint.
    pub placeholder: Option<&'a str>,
    /// HTML5 maxlength (advisory).
    pub max_length: Option<usize>,
    /// `required` attribute.
    pub required: bool,
    /// Visual chrome. Defaults to `Rounded`.
    pub style: FormStyle,
    /// Density (unused for multiline — kept for symmetry with `TextInput`).
    pub density: FormDensity,
}

impl TextArea<'_> {
    /// Render label + textarea pair.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = input_classes(self.style, self.density, true);
        html! {
            div class="space-y-2" data-loom-form-style=(form_style_attr(self.style)) {
                label class=(LABEL_CLASSES) for=(self.id) { (self.label) }
                textarea
                    id=(self.id)
                    name=(self.name)
                    class=(class)
                    placeholder=[self.placeholder]
                    maxlength=[self.max_length.map(|n| n.to_string())]
                    required[self.required] {}
            }
        }
    }
}

/// One option in a `<select>`.
pub struct SelectOption<'a> {
    /// `value=` attribute. Submitted as the form value.
    pub value: &'a str,
    /// Visible option text.
    pub label: &'a str,
}

/// `<select>` with a bound visible label.
pub struct Select<'a> {
    /// Stable HTML `id`.
    pub id: &'a str,
    /// Form field name.
    pub name: &'a str,
    /// Visible label text.
    pub label: &'a str,
    /// Options. Order is preserved.
    pub options: &'a [SelectOption<'a>],
    /// Visual chrome. Defaults to `Rounded`.
    pub style: FormStyle,
    /// Vertical density. Defaults to `Comfortable`.
    pub density: FormDensity,
}

impl Select<'_> {
    /// Render label + select pair.
    #[must_use]
    pub fn render(&self) -> Markup {
        let class = input_classes(self.style, self.density, false);
        html! {
            div class="space-y-2" data-loom-form-style=(form_style_attr(self.style)) {
                label class=(LABEL_CLASSES) for=(self.id) { (self.label) }
                select id=(self.id) name=(self.name) class=(class) {
                    @for opt in self.options {
                        option value=(opt.value) { (opt.label) }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_input_binds_label_to_id() {
        let i = TextInput {
            id: "contact-email",
            name: "email",
            label: "Email Address",
            input_type: InputType::Email,
            placeholder: Some("you@example.com"),
            max_length: Some(200),
            required: true,
            style: FormStyle::default(),
            density: FormDensity::default(),
        };
        let s = i.render().into_string();
        assert!(s.contains(r#"for="contact-email""#));
        assert!(s.contains(r#"id="contact-email""#));
        assert!(s.contains(r#"name="email""#));
        assert!(s.contains(r#"type="email""#));
        assert!(s.contains(r#"placeholder="you@example.com""#));
        assert!(s.contains(r#"maxlength="200""#));
        assert!(s.contains("required"));
        assert!(s.contains(">Email Address<"));
        // Default style is Rounded — rounded-md class present.
        assert!(s.contains("rounded-md"));
        assert!(s.contains("bg-slate-50"));
        assert!(s.contains(r#"data-loom-form-style="rounded""#));
    }

    #[test]
    fn text_input_optional_attrs_omitted_when_none() {
        let i = TextInput {
            id: "x",
            name: "x",
            label: "X",
            input_type: InputType::Text,
            placeholder: None,
            max_length: None,
            required: false,
            style: FormStyle::default(),
            density: FormDensity::default(),
        };
        let s = i.render().into_string();
        assert!(!s.contains("placeholder"));
        assert!(!s.contains("maxlength"));
        assert!(!s.contains(" required"));
    }

    #[test]
    fn text_input_editorial_style_strips_rounded_and_bg() {
        let i = TextInput {
            id: "x",
            name: "x",
            label: "X",
            input_type: InputType::Text,
            placeholder: None,
            max_length: None,
            required: false,
            style: FormStyle::Editorial,
            density: FormDensity::default(),
        };
        let s = i.render().into_string();
        // Editorial style: no rounded, no slate-50 bg.
        assert!(!s.contains("rounded-md"));
        assert!(!s.contains("bg-slate-50"));
        assert!(s.contains("rounded-none"));
        assert!(s.contains("border-b"));
        assert!(s.contains("bg-transparent"));
        assert!(s.contains(r#"data-loom-form-style="editorial""#));
    }

    #[test]
    fn text_input_minimal_style_no_resting_border() {
        let i = TextInput {
            id: "x",
            name: "x",
            label: "X",
            input_type: InputType::Text,
            placeholder: None,
            max_length: None,
            required: false,
            style: FormStyle::Minimal,
            density: FormDensity::default(),
        };
        let s = i.render().into_string();
        assert!(s.contains("border-transparent"));
        assert!(s.contains("focus-visible:border-slate-400"));
        assert!(s.contains(r#"data-loom-form-style="minimal""#));
    }

    #[test]
    fn text_input_compact_density_uses_h9() {
        let i = TextInput {
            id: "x",
            name: "x",
            label: "X",
            input_type: InputType::Text,
            placeholder: None,
            max_length: None,
            required: false,
            style: FormStyle::default(),
            density: FormDensity::Compact,
        };
        let s = i.render().into_string();
        assert!(s.contains("h-9"));
        assert!(!s.contains("h-12"));
    }

    #[test]
    fn text_input_comfortable_density_uses_h12() {
        let i = TextInput {
            id: "x",
            name: "x",
            label: "X",
            input_type: InputType::Text,
            placeholder: None,
            max_length: None,
            required: false,
            style: FormStyle::default(),
            density: FormDensity::Comfortable,
        };
        let s = i.render().into_string();
        assert!(s.contains("h-12"));
        assert!(!s.contains("h-9"));
    }

    #[test]
    fn textarea_renders_with_required() {
        let t = TextArea {
            id: "msg",
            name: "message",
            label: "Message",
            placeholder: Some("Your message..."),
            max_length: Some(5000),
            required: true,
            style: FormStyle::default(),
            density: FormDensity::default(),
        };
        let s = t.render().into_string();
        assert!(s.contains("<textarea"));
        assert!(s.contains(r#"for="msg""#));
        assert!(s.contains(r#"id="msg""#));
        assert!(s.contains(r#"maxlength="5000""#));
        assert!(s.contains("required"));
        // Textarea uses min-h-[150px] regardless of density.
        assert!(s.contains("min-h-[150px]"));
        assert!(s.contains("resize-none"));
    }

    #[test]
    fn textarea_editorial_style_strips_pill_chrome() {
        let t = TextArea {
            id: "x",
            name: "x",
            label: "X",
            placeholder: None,
            max_length: None,
            required: false,
            style: FormStyle::Editorial,
            density: FormDensity::default(),
        };
        let s = t.render().into_string();
        assert!(!s.contains("rounded-md"));
        assert!(s.contains("rounded-none"));
        assert!(s.contains("border-b"));
        assert!(s.contains("bg-transparent"));
    }

    #[test]
    fn select_renders_options_in_order() {
        let opts = [
            SelectOption {
                value: "",
                label: "Pick one",
            },
            SelectOption {
                value: "a",
                label: "Alpha",
            },
            SelectOption {
                value: "b",
                label: "Beta",
            },
        ];
        let sel = Select {
            id: "service",
            name: "service",
            label: "Service",
            options: &opts,
            style: FormStyle::default(),
            density: FormDensity::default(),
        };
        let s = sel.render().into_string();
        assert!(s.contains(r#"<select id="service""#));
        let pick_pos = s.find("Pick one").unwrap();
        let alpha_pos = s.find("Alpha").unwrap();
        let beta_pos = s.find("Beta").unwrap();
        assert!(pick_pos < alpha_pos);
        assert!(alpha_pos < beta_pos);
    }

    #[test]
    fn select_editorial_style_strips_pill_chrome() {
        let opts = [SelectOption { value: "a", label: "Alpha" }];
        let sel = Select {
            id: "x",
            name: "x",
            label: "X",
            options: &opts,
            style: FormStyle::Editorial,
            density: FormDensity::default(),
        };
        let s = sel.render().into_string();
        assert!(!s.contains("rounded-md"));
        assert!(!s.contains("bg-slate-50"));
        assert!(s.contains("rounded-none"));
        assert!(s.contains("border-b"));
    }

    #[test]
    fn form_style_default_is_rounded() {
        // Back-compat guarantee: code that omits the style field via
        // FormStyle::default() gets the legacy SaaS shape.
        assert!(matches!(FormStyle::default(), FormStyle::Rounded));
        assert!(matches!(FormDensity::default(), FormDensity::Comfortable));
    }

    #[test]
    fn input_type_emits_correct_html_attr() {
        for (it, expected) in [
            (InputType::Text, "text"),
            (InputType::Email, "email"),
            (InputType::Tel, "tel"),
            (InputType::Url, "url"),
        ] {
            assert_eq!(it.html(), expected);
        }
    }
}
