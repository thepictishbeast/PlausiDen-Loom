//! Typed `Composer` primitive — feed-top compose bar.
//!
//! The pattern Facebook / X / LinkedIn / Instagram all converged on:
//! a horizontally-packed bar at the top of the feed with an avatar
//! slot on the left, a single-line "what's on your mind?" prompt in
//! the middle, and 1–3 typed action buttons on the right. Tapping
//! the prompt navigates to the full composer route (or opens a
//! modal in JS-augmented contexts).
//!
//! The component renders a ZERO-JS shell that works in pure HTML —
//! the prompt is a real `<a>` to `submit_endpoint`, the action
//! buttons are real `<a>`s. Progressive enhancement (auto-focus,
//! modal expansion, drag-and-drop file upload) layers on top via
//! optional script; the shell itself is unconditionally functional.
//!
//! API SHAPE
//! ---------
//! - `prompt`             required, the visible call-to-action
//! - `submit_endpoint`    required, where the prompt-link points
//! - `actions`            up to 3 typed PromptAction entries
//! - `avatar`             typed enum (none / initials / image)
//! - `size`               compact (nav-row) | comfortable (feed-top)
//!
//! No `extra_classes` slot. No raw HTML in any text field. No
//! arbitrary URL accepted for `submit_endpoint` — see SECURITY note
//! on the struct.
//!
//! USAGE
//! -----
//! ```no_run
//! use loom_components::composer::{
//!     Composer, ComposerAvatar, ComposerSize, PromptAction,
//! };
//!
//! let c = Composer {
//!     prompt: "What did you nail today?",
//!     submit_endpoint: "/post-skill",
//!     actions: vec![
//!         PromptAction::UploadClip,
//!         PromptAction::ChallengeOpponent,
//!     ],
//!     avatar: ComposerAvatar::Initials("DA"),
//!     size: ComposerSize::Comfortable,
//! };
//! let _markup = c.render();
//! ```

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Visible density. Compact nests inside a header row; Comfortable
/// is the FB-style feed-top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposerSize {
    /// Tight padding, single-line.
    Compact,
    /// Generous padding, taller, a bit more visual weight.
    Comfortable,
}

impl ComposerSize {
    const fn data_attr(self) -> &'static str {
        match self {
            ComposerSize::Compact => "compact",
            ComposerSize::Comfortable => "comfortable",
        }
    }
}

/// Avatar slot. Pre-typed so the component owns rendering the
/// circle treatment instead of accepting raw markup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposerAvatar<'a> {
    /// No avatar slot — the prompt sits flush left.
    None,
    /// Display 1–3 letters in a circle. Useful pre-photo.
    Initials(&'a str),
    /// Image URL (validated same-origin path or https://).
    Image {
        /// Image asset URL.
        src: &'a str,
        /// Required alt for screen readers.
        alt: &'a str,
    },
}

/// One prompt-bar action button. Closed enum: each variant is a
/// well-known SkillShots action.
///
/// REGRESSION-GUARD: do NOT add a `Custom(&str)` variant. If a new
/// action is needed, add a typed variant + tested icon mapping.
/// Open variants reintroduce the raw-class problem this primitive
/// exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptAction {
    /// "Upload clip" — opens the upload flow.
    UploadClip,
    /// "Challenge" — opens the challenge-creation flow.
    ChallengeOpponent,
    /// "Live" — opens live-stream composer.
    GoLive,
    /// "Photo" — image-only post.
    PhotoOnly,
}

impl PromptAction {
    const fn label(self) -> &'static str {
        match self {
            PromptAction::UploadClip => "Upload clip",
            PromptAction::ChallengeOpponent => "Challenge",
            PromptAction::GoLive => "Live",
            PromptAction::PhotoOnly => "Photo",
        }
    }
    /// Backend key (data-backend value) — must match a key in
    /// `backends.toml`. Forge's phantom_button phase verifies.
    const fn backend(self) -> &'static str {
        match self {
            PromptAction::UploadClip => "post-skill",
            PromptAction::ChallengeOpponent => "challenge-create",
            PromptAction::GoLive => "live-start",
            PromptAction::PhotoOnly => "post-photo",
        }
    }
    /// Inline SVG glyph. 24×24, currentColor stroke. Path-only —
    /// no <style>, no <script>, no external <use>.
    const fn icon_svg(self) -> &'static str {
        match self {
            PromptAction::UploadClip => "<svg viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4\"/><polyline points=\"17 8 12 3 7 8\"/><line x1=\"12\" y1=\"3\" x2=\"12\" y2=\"15\"/></svg>",
            PromptAction::ChallengeOpponent => "<svg viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><polygon points=\"12 2 15 8.5 22 9.3 17 14.1 18.2 21 12 17.8 5.8 21 7 14.1 2 9.3 9 8.5 12 2\"/></svg>",
            PromptAction::GoLive => "<svg viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"3\"/><path d=\"M19.07 4.93a10 10 0 0 1 0 14.14M4.93 19.07a10 10 0 0 1 0-14.14\"/></svg>",
            PromptAction::PhotoOnly => "<svg viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z\"/><circle cx=\"12\" cy=\"13\" r=\"4\"/></svg>",
        }
    }
}

/// Validation: accept only same-origin paths (`/foo`) or `https://`.
/// Used by both `submit_endpoint` and `Avatar::Image::src`.
#[must_use]
pub fn is_safe_url(p: &str) -> bool {
    if p.is_empty() {
        return false;
    }
    if p.starts_with("https://") {
        return !p.contains('\n') && !p.contains('\r');
    }
    if !p.starts_with('/') {
        return false;
    }
    if p.starts_with("//") {
        // Protocol-relative — refuse. Caller must explicitly pass
        // https://host/path if cross-origin is intended.
        return false;
    }
    if p.contains("://") {
        return false;
    }
    !p.chars().any(|c| (c as u32) < 0x20)
}

/// Feed-top composer.
///
/// SECURITY: `submit_endpoint` is interpolated into the prompt
/// link's `href` attribute. The component validates via
/// [`is_safe_url`] before render — invalid URLs cause render to
/// substitute a `data-invalid="true"` placeholder that the
/// phantom_button forge phase will catch. This way a typo or
/// malicious data-action that slipped through CMS validation is
/// surfaced at build time.
pub struct Composer<'a> {
    /// Visible call-to-action text. Required.
    pub prompt: &'a str,
    /// Where the prompt link navigates. Required. MUST satisfy
    /// [`is_safe_url`] (same-origin path or https://).
    pub submit_endpoint: &'a str,
    /// Up to 3 typed action buttons. Empty list is allowed —
    /// renders prompt-only.
    pub actions: Vec<PromptAction>,
    /// Avatar slot.
    pub avatar: ComposerAvatar<'a>,
    /// Density.
    pub size: ComposerSize,
}

impl Composer<'_> {
    /// Render the composer card. Uses Maud's auto-escaping for
    /// every text slot; no raw user input is ever interpolated as
    /// markup.
    #[must_use]
    pub fn render(&self) -> Markup {
        let endpoint_safe = is_safe_url(self.submit_endpoint);
        let endpoint = if endpoint_safe {
            self.submit_endpoint
        } else {
            "#invalid-endpoint"
        };
        // Truncate actions to 3 — UI doesn't have room for more,
        // and forcing a hard cap here keeps the component honest.
        let actions: Vec<&PromptAction> = self.actions.iter().take(3).collect();
        html! {
            section
                class="loom-composer"
                data-loom-composer
                data-size=(self.size.data_attr())
                data-invalid=[(!endpoint_safe).then_some("true")]
                aria-label="Compose"
            {
                div class="loom-composer__row" {
                    @match &self.avatar {
                        ComposerAvatar::None => {}
                        ComposerAvatar::Initials(letters) => {
                            div class="loom-composer__avatar" data-avatar="initials" aria-hidden="true" {
                                (*letters)
                            }
                        }
                        ComposerAvatar::Image { src, alt } => {
                            @if is_safe_url(src) {
                                img
                                    class="loom-composer__avatar"
                                    data-avatar="image"
                                    src=(*src)
                                    alt=(*alt)
                                    width="40"
                                    height="40"
                                    loading="lazy"
                                    decoding="async";
                            } @else {
                                div class="loom-composer__avatar" data-avatar="invalid-image" aria-hidden="true" {
                                    "?"
                                }
                            }
                        }
                    }
                    a
                        class="loom-composer__prompt"
                        href=(endpoint)
                        data-backend="post-skill"
                    {
                        (self.prompt)
                    }
                }
                @if !actions.is_empty() {
                    nav class="loom-composer__actions" aria-label="Quick actions" {
                        @for action in actions {
                            a
                                class="loom-composer__action"
                                href=(endpoint)
                                data-backend=(action.backend())
                                aria-label=(action.label())
                            {
                                span class="loom-composer__action-icon" {
                                    (maud::PreEscaped(action.icon_svg()))
                                }
                                span class="loom-composer__action-label" {
                                    (action.label())
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(c: &Composer<'_>) -> String {
        c.render().into_string()
    }

    #[test]
    fn safe_url_accepts_same_origin_paths() {
        assert!(is_safe_url("/post"));
        assert!(is_safe_url("/post/skill?id=42"));
        assert!(is_safe_url("/"));
    }

    #[test]
    fn safe_url_accepts_https() {
        assert!(is_safe_url("https://example.com/x"));
    }

    #[test]
    fn safe_url_rejects_protocol_relative() {
        assert!(!is_safe_url("//evil.example.com/x"));
    }

    #[test]
    fn safe_url_rejects_javascript_scheme() {
        assert!(!is_safe_url("javascript:alert(1)"));
    }

    #[test]
    fn safe_url_rejects_http() {
        assert!(!is_safe_url("http://example.com/x"));
    }

    #[test]
    fn safe_url_rejects_crlf_smuggling() {
        assert!(!is_safe_url("/post\r\nLocation: /evil"));
        assert!(!is_safe_url("/post\nfoo"));
    }

    #[test]
    fn safe_url_rejects_empty() {
        assert!(!is_safe_url(""));
    }

    #[test]
    fn renders_prompt_with_endpoint() {
        let c = Composer {
            prompt: "What did you nail today?",
            submit_endpoint: "/post-skill",
            actions: vec![],
            avatar: ComposerAvatar::None,
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        assert!(html.contains("What did you nail today?"));
        assert!(html.contains(r#"href="/post-skill""#));
        assert!(html.contains(r#"data-size="comfortable""#));
    }

    #[test]
    fn invalid_endpoint_substitutes_placeholder() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "javascript:alert(1)",
            actions: vec![],
            avatar: ComposerAvatar::None,
            size: ComposerSize::Compact,
        };
        let html = render_to_string(&c);
        assert!(html.contains(r#"data-invalid="true""#));
        assert!(html.contains(r##"href="#invalid-endpoint""##));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn caps_actions_at_three() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![
                PromptAction::UploadClip,
                PromptAction::ChallengeOpponent,
                PromptAction::GoLive,
                PromptAction::PhotoOnly,
            ],
            avatar: ComposerAvatar::None,
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        // "Photo" should be the 4th action and excluded.
        assert!(html.contains("Upload clip"));
        assert!(html.contains("Challenge"));
        assert!(html.contains("Live"));
        assert!(!html.contains(">Photo<"));
    }

    #[test]
    fn empty_actions_omits_nav() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![],
            avatar: ComposerAvatar::None,
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        assert!(!html.contains("loom-composer__actions"));
    }

    #[test]
    fn avatar_initials_renders_letters() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![],
            avatar: ComposerAvatar::Initials("DA"),
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        assert!(html.contains(r#"data-avatar="initials""#));
        assert!(html.contains(">DA<"));
    }

    #[test]
    fn avatar_image_emits_img_with_dims() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![],
            avatar: ComposerAvatar::Image {
                src: "/u/42.jpg",
                alt: "Dax",
            },
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        assert!(html.contains(r#"src="/u/42.jpg""#));
        assert!(html.contains(r#"alt="Dax""#));
        assert!(html.contains(r#"width="40""#));
        assert!(html.contains(r#"height="40""#));
        assert!(html.contains(r#"loading="lazy""#));
    }

    #[test]
    fn avatar_image_invalid_url_falls_back_to_placeholder() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![],
            avatar: ComposerAvatar::Image {
                src: "javascript:alert(1)",
                alt: "evil",
            },
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        assert!(html.contains(r#"data-avatar="invalid-image""#));
        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn action_carries_backend_attribute() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![PromptAction::UploadClip],
            avatar: ComposerAvatar::None,
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        assert!(html.contains(r#"data-backend="post-skill""#));
    }

    #[test]
    fn action_aria_label_present() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![PromptAction::ChallengeOpponent],
            avatar: ComposerAvatar::None,
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        assert!(html.contains(r#"aria-label="Challenge""#));
    }

    #[test]
    fn icon_svg_is_path_only_no_script() {
        for action in [
            PromptAction::UploadClip,
            PromptAction::ChallengeOpponent,
            PromptAction::GoLive,
            PromptAction::PhotoOnly,
        ] {
            let svg = action.icon_svg();
            assert!(!svg.contains("<script"), "icon contains script: {action:?}");
            assert!(!svg.contains("<style"), "icon contains style: {action:?}");
            assert!(!svg.contains("xlink:href"), "icon contains <use>: {action:?}");
        }
    }

    #[test]
    fn classes_are_loom_namespaced() {
        let c = Composer {
            prompt: "x",
            submit_endpoint: "/x",
            actions: vec![PromptAction::UploadClip],
            avatar: ComposerAvatar::Initials("DA"),
            size: ComposerSize::Comfortable,
        };
        let html = render_to_string(&c);
        // Every class= must contain only loom-* tokens.
        for class_match in regex_lite_class_iter(&html) {
            for tok in class_match.split_whitespace() {
                assert!(
                    tok.is_empty() || tok.starts_with("loom-"),
                    "non-loom class token: {tok} in {html}"
                );
            }
        }
    }

    fn regex_lite_class_iter(s: &str) -> Vec<String> {
        // Tiny class= extractor — avoids pulling in regex crate
        // for this assertion.
        let mut out = vec![];
        let mut cursor = 0usize;
        while let Some(idx) = s[cursor..].find(r#"class=""#) {
            let start = cursor + idx + r#"class=""#.len();
            if let Some(end) = s[start..].find('"') {
                out.push(s[start..start + end].to_owned());
                cursor = start + end + 1;
            } else {
                break;
            }
        }
        out
    }
}
