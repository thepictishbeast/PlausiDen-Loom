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

const INPUT_CLASSES: &str = "flex w-full rounded-md border border-input px-3 py-2 text-base ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 md:text-sm h-12 bg-slate-50";
const TEXTAREA_CLASSES: &str = "flex w-full rounded-md border border-input px-3 py-2 text-base ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 md:text-sm min-h-[150px] bg-slate-50 resize-none";
const SELECT_CLASSES: &str = "flex w-full rounded-md border border-input px-3 py-2 text-base ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 md:text-sm h-12 bg-slate-50";
const LABEL_CLASSES: &str = "text-sm font-medium leading-none";

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
}

impl TextInput<'_> {
    /// Render label + input pair.
    #[must_use]
    pub fn render(&self) -> Markup {
        html! {
            div class="space-y-2" {
                label class=(LABEL_CLASSES) for=(self.id) { (self.label) }
                input
                    type=(self.input_type.html())
                    id=(self.id)
                    name=(self.name)
                    class=(INPUT_CLASSES)
                    placeholder=[self.placeholder]
                    maxlength=[self.max_length.map(|n| n.to_string())]
                    required[self.required];
            }
        }
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
}

impl TextArea<'_> {
    /// Render label + textarea pair.
    #[must_use]
    pub fn render(&self) -> Markup {
        html! {
            div class="space-y-2" {
                label class=(LABEL_CLASSES) for=(self.id) { (self.label) }
                textarea
                    id=(self.id)
                    name=(self.name)
                    class=(TEXTAREA_CLASSES)
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
}

impl Select<'_> {
    /// Render label + select pair.
    #[must_use]
    pub fn render(&self) -> Markup {
        html! {
            div class="space-y-2" {
                label class=(LABEL_CLASSES) for=(self.id) { (self.label) }
                select id=(self.id) name=(self.name) class=(SELECT_CLASSES) {
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
        };
        let s = i.render().into_string();
        assert!(!s.contains("placeholder"));
        assert!(!s.contains("maxlength"));
        assert!(!s.contains(" required"));
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
        };
        let s = t.render().into_string();
        assert!(s.contains("<textarea"));
        assert!(s.contains(r#"for="msg""#));
        assert!(s.contains(r#"id="msg""#));
        assert!(s.contains(r#"maxlength="5000""#));
        assert!(s.contains("required"));
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
