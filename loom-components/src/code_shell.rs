//! Typed `CodeShell` primitive — terminal-style code/output block.
//!
//! The preamble names `code terminal-style audit chain proof` as one
//! of the substrate's core editorial-composition primitives, alongside
//! `split_hero`, `kv_pair`, and `pull_quote`. `HeroEditorial` and
//! `PullQuote` docstrings already reference it as an intended
//! decoration-slot content type. This primitive closes that gap.
//!
//! Shape commitments enforced by the test suite:
//!
//! * Renders as semantic `<pre><code>` (NOT a `<div>` masquerading
//!   as code). Honors HTML spec for preformatted code blocks.
//! * No fake macOS traffic-light circles — the kind of red / yellow /
//!   green rounded-full triplet that screams "design mock-up" but
//!   adds no information.
//! * No gradient header bar.
//! * No "copy" button decoration — that's runtime JS; this is a
//!   typed primitive.
//! * Per-line kind annotation: `Command` lines get a prompt prefix
//!   (`$ ` by default), `Output` lines are indented to align,
//!   `Comment` lines render dimmed + italic, `Error` lines pick up
//!   the warn / error color.
//! * AMOLED variant per `[[dark-theme-amoled-true-black]]` —
//!   true `#000` background, slate-100 ink.

use maud::{Markup, html};
use serde::{Deserialize, Serialize};

/// Color tone for the shell surface + ink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeShellTone {
    /// Slate-900 ink on slate-50 surface. Default for light pages.
    Slate,
    /// Slate-100 ink on AMOLED true-black surface. The editorial
    /// terminal look.
    Amoled,
}

/// Chrome treatment around the code block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeShellChrome {
    /// No header — just the code block, framed by a single border.
    /// Use for inline audit-chain proof on editorial body.
    Minimal,
    /// Text-only header showing the shell name or filename
    /// (e.g. "bash", "forge build", "/etc/postfix/main.cf"). No
    /// traffic-light circles, no gradient.
    Header,
}

/// Semantic role of a single line in the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeShellLineKind {
    /// User-typed shell command. Renders with the prompt prefix.
    Command,
    /// Program output (stdout / stderr non-error). Aligned to the
    /// command text indent.
    Output,
    /// Annotation by the author. Dimmed, italicized, prefixed with
    /// `#` so the line reads as a real shell comment.
    Comment,
    /// Error line. Picks up the warn / error color so failures stand
    /// out from successful output.
    Error,
}

/// One line of the shell transcript.
#[derive(Debug, Clone, Copy)]
pub struct CodeShellLine<'a> {
    /// Line role.
    pub kind: CodeShellLineKind,
    /// Line text. Rendered as-is (no markdown / no inline tags).
    pub text: &'a str,
}

/// Terminal-style code block.
///
/// Example rendered transcript:
///
/// ```text
/// ┌────────────────────────────────────────────┐
/// │  forge build                               │
/// ├────────────────────────────────────────────┤
/// │  $ forge build --json                      │
/// │    discipline-strict: 0 findings           │
/// │    semver-enforcement: 0 findings          │
/// │    trait-consistency: 0 findings           │
/// │  # all phases clean.                       │
/// └────────────────────────────────────────────┘
/// ```
pub struct CodeShell<'a> {
    /// Header title — shell name, filename, or label. Used only
    /// when `chrome == Header`. `None` collapses to chrome-less even
    /// if chrome is Header.
    pub title: Option<&'a str>,
    /// Prompt prefix for `Command` lines. `None` defaults to `"$"`.
    /// Pass `Some("›")` for a custom prompt or `Some(">")` for PowerShell.
    pub prompt: Option<&'a str>,
    /// Transcript lines in order.
    pub lines: &'a [CodeShellLine<'a>],
    /// Color tone.
    pub tone: CodeShellTone,
    /// Chrome treatment.
    pub chrome: CodeShellChrome,
}

impl CodeShell<'_> {
    /// Render as `<pre><code>` framed by the configured chrome.
    #[must_use]
    pub fn render(&self) -> Markup {
        let prompt = self.prompt.unwrap_or("$");
        let (
            surface,
            border,
            header_surface,
            header_color,
            command_color,
            output_color,
            comment_color,
            error_color,
        ) = match self.tone {
            CodeShellTone::Slate => (
                "bg-slate-50",
                "border-slate-200",
                "bg-slate-100",
                "text-slate-600",
                "text-slate-900",
                "text-slate-700",
                "text-slate-500",
                "text-red-700",
            ),
            CodeShellTone::Amoled => (
                "bg-black",
                "border-slate-800",
                "bg-slate-900",
                "text-slate-300",
                "text-slate-100",
                "text-slate-300",
                "text-slate-500",
                "text-red-400",
            ),
        };
        let outer = format!("border {border} {surface} overflow-hidden");
        let header_class = format!(
            "px-4 py-2 text-xs font-mono uppercase tracking-widest border-b {border} {header_surface} {header_color}"
        );
        let pre_class = "p-4 overflow-x-auto text-sm leading-relaxed";
        let show_header = matches!(self.chrome, CodeShellChrome::Header) && self.title.is_some();
        html! {
            div class=(outer) {
                @if show_header {
                    @if let Some(title) = self.title {
                        div class=(header_class) {
                            (title)
                        }
                    }
                }
                pre class=(pre_class) {
                    code class="font-mono" {
                        @for line in self.lines {
                            (render_line(line, prompt, command_color, output_color, comment_color, error_color))
                        }
                    }
                }
            }
        }
    }
}

fn render_line(
    line: &CodeShellLine<'_>,
    prompt: &str,
    command_color: &str,
    output_color: &str,
    comment_color: &str,
    error_color: &str,
) -> Markup {
    match line.kind {
        CodeShellLineKind::Command => {
            let cls = format!("block {command_color}");
            html! {
                span class=(cls) {
                    span class="opacity-60 select-none" {
                        (prompt) " "
                    }
                    (line.text)
                }
                "\n"
            }
        }
        CodeShellLineKind::Output => {
            let cls = format!("block {output_color} pl-4");
            html! {
                span class=(cls) {
                    (line.text)
                }
                "\n"
            }
        }
        CodeShellLineKind::Comment => {
            let cls = format!("block italic {comment_color}");
            html! {
                span class=(cls) {
                    "# "
                    (line.text)
                }
                "\n"
            }
        }
        CodeShellLineKind::Error => {
            let cls = format!("block {error_color} pl-4");
            html! {
                span class=(cls) {
                    (line.text)
                }
                "\n"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> &'static [CodeShellLine<'static>] {
        &[
            CodeShellLine {
                kind: CodeShellLineKind::Command,
                text: "forge build --json",
            },
            CodeShellLine {
                kind: CodeShellLineKind::Output,
                text: "discipline-strict: 0 findings",
            },
            CodeShellLine {
                kind: CodeShellLineKind::Comment,
                text: "all phases clean.",
            },
        ]
    }

    #[test]
    fn renders_semantic_pre_code() {
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(s.contains("<pre"));
        assert!(s.contains("<code"));
        assert!(s.contains("font-mono"));
    }

    #[test]
    fn command_line_gets_prompt_prefix() {
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(s.contains(">$ <"));
        assert!(s.contains(">forge build --json<"));
    }

    #[test]
    fn custom_prompt_replaces_default() {
        let s = CodeShell {
            title: None,
            prompt: Some("›"),
            lines: &[CodeShellLine {
                kind: CodeShellLineKind::Command,
                text: "ls",
            }],
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(s.contains(">› <"));
        assert!(!s.contains(">$ <"));
    }

    #[test]
    fn output_line_indented() {
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: &[CodeShellLine {
                kind: CodeShellLineKind::Output,
                text: "hello world",
            }],
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(s.contains("pl-4"));
        assert!(s.contains(">hello world<"));
    }

    #[test]
    fn comment_line_dimmed_italic_with_hash_prefix() {
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: &[CodeShellLine {
                kind: CodeShellLineKind::Comment,
                text: "all phases clean.",
            }],
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(s.contains("italic"));
        assert!(s.contains("text-slate-500"));
        assert!(s.contains("# all phases clean."));
    }

    #[test]
    fn error_line_takes_warn_color() {
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: &[CodeShellLine {
                kind: CodeShellLineKind::Error,
                text: "error: build failed",
            }],
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(s.contains("text-red-700"));
        assert!(s.contains(">error: build failed<"));
    }

    #[test]
    fn header_chrome_renders_title_when_present() {
        let s = CodeShell {
            title: Some("forge build"),
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Header,
        }
        .render()
        .into_string();
        assert!(s.contains(">forge build<"));
        // Header gets monospace uppercase tracked treatment.
        assert!(s.contains("font-mono uppercase tracking-widest"));
    }

    #[test]
    fn header_chrome_collapses_when_title_none() {
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Header,
        }
        .render()
        .into_string();
        // No header div should be emitted at all.
        assert!(!s.contains("uppercase tracking-widest"));
    }

    #[test]
    fn minimal_chrome_omits_header_even_with_title() {
        let s = CodeShell {
            title: Some("would-be-header"),
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(!s.contains(">would-be-header<"));
    }

    #[test]
    fn amoled_tone_uses_true_black_surface() {
        let s = CodeShell {
            title: Some("h"),
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Amoled,
            chrome: CodeShellChrome::Header,
        }
        .render()
        .into_string();
        assert!(s.contains("bg-black"));
        assert!(s.contains("text-slate-100"));
        // Error red shifts brighter on dark background.
        let with_error = CodeShell {
            title: None,
            prompt: None,
            lines: &[CodeShellLine {
                kind: CodeShellLineKind::Error,
                text: "x",
            }],
            tone: CodeShellTone::Amoled,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(with_error.contains("text-red-400"));
        assert!(!with_error.contains("text-red-700"));
    }

    #[test]
    fn no_saas_trope_ornaments() {
        // The shape guarantee that distinguishes this primitive from
        // the design-mock-up shell aesthetic: no fake traffic-light
        // circles, no gradient header bar, no copy-button decoration.
        let s = CodeShell {
            title: Some("h"),
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Header,
        }
        .render()
        .into_string();
        // No traffic-light triplet: red+yellow+green rounded-full
        // glyphs commonly appear together in mock-up shells.
        assert!(!s.contains("rounded-full"));
        // No gradient header.
        assert!(!s.contains("linear-gradient"));
        assert!(!s.contains("bg-gradient"));
        // No animation classes.
        assert!(!s.contains("animate-"));
        // No shadow ornaments.
        assert!(!s.contains("shadow-"));
        // No copy-button data attribute / role.
        assert!(!s.contains("data-copy"));
        assert!(!s.contains("aria-label=\"Copy"));
    }

    #[test]
    fn full_transcript_renders_in_order() {
        let s = CodeShell {
            title: Some("audit"),
            prompt: None,
            lines: lines(),
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Header,
        }
        .render()
        .into_string();
        let cmd_idx = s.find("forge build --json").expect("cmd");
        let out_idx = s.find("discipline-strict").expect("out");
        let comment_idx = s.find("all phases clean").expect("comment");
        assert!(cmd_idx < out_idx);
        assert!(out_idx < comment_idx);
    }

    #[test]
    fn prompt_glyph_marked_unselectable() {
        // The prompt prefix should be marked `select-none` so users
        // copying the transcript get the command WITHOUT the prompt.
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: &[CodeShellLine {
                kind: CodeShellLineKind::Command,
                text: "ls",
            }],
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        assert!(s.contains("select-none"));
    }

    #[test]
    fn empty_lines_renders_clean() {
        let s = CodeShell {
            title: None,
            prompt: None,
            lines: &[],
            tone: CodeShellTone::Slate,
            chrome: CodeShellChrome::Minimal,
        }
        .render()
        .into_string();
        // Still emits the wrapper shell, just empty.
        assert!(s.contains("<pre"));
        assert!(s.contains("</pre>"));
    }
}
