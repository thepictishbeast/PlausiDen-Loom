//! `loom cms-new` subcommand — scaffold a single `cms/*.json` file
//! from a kind/template (`landing` / `explainer` / `form`).
//!
//! Extracted from `main.rs` as part of the bloat-reduction work
//! (Loom issue #3). Body kept byte-for-byte identical to the
//! pre-extraction implementation; visibility on `cmd_cms_new` and
//! `CmsNewError` was widened to `pub` so the entrypoint can call /
//! pattern-match them through the `mod cms_new;` boundary.

/// `loom cms-new` errors.
#[derive(Debug)]
pub enum CmsNewError {
    Conflict(std::path::PathBuf),
    UnknownKind(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for CmsNewError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub fn cmd_cms_new(
    kind: &str,
    out: &std::path::Path,
    title: &str,
    path: &str,
    force: bool,
) -> Result<(), CmsNewError> {
    if out.exists() && !force {
        return Err(CmsNewError::Conflict(out.to_path_buf()));
    }
    let page = match kind {
        "landing" => cms_template_landing(title, path),
        "explainer" => cms_template_explainer(title, path),
        "form" => cms_template_form(title, path),
        other => return Err(CmsNewError::UnknownKind(other.to_owned())),
    };
    let json = serde_json::to_string_pretty(&page)
        .map_err(|e| CmsNewError::Io(std::io::Error::other(format!("serialize: {e}"))))?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, format!("{json}\n"))?;
    println!("  ok     {kind} template → {}", out.display());
    Ok(())
}

fn cms_standard_nav() -> Vec<loom_cms_render::CmsNavLink> {
    vec![
        loom_cms_render::CmsNavLink {
            label: "Battle Feed".to_owned(),
            href: "/".to_owned(),
            data_backend: "list-challenges".to_owned(),
            current: false,
        },
        loom_cms_render::CmsNavLink {
            label: "Leaderboard".to_owned(),
            href: "/leaderboard.html".to_owned(),
            data_backend: "list-leaderboard".to_owned(),
            current: false,
        },
        loom_cms_render::CmsNavLink {
            label: "My Wins".to_owned(),
            href: "/my-wins.html".to_owned(),
            data_backend: "list-touches".to_owned(),
            current: false,
        },
        loom_cms_render::CmsNavLink {
            label: "Profile".to_owned(),
            href: "/profile.html".to_owned(),
            data_backend: "view-profile".to_owned(),
            current: false,
        },
    ]
}

fn cms_template_landing(title: &str, path: &str) -> loom_cms_render::CmsPage {
    use loom_cms_render::{
        CmsAvatar, CmsCard, CmsCardStat, CmsComposerSize, CmsPage, CmsPromptAction, CmsSection,
        HeroCta,
    };
    CmsPage {
        brand: None,
        brand_logo: None,
        theme: None,
        chrome: None,
        content_width: None,
        nav_actions: vec![],
        schema: Some("../cms-schema.json".to_owned()),
        title: title.to_owned(),
        description: format!("{title} — describe the page in 120 chars max."),
        path: path.to_owned(),
        nav_links: cms_standard_nav(),
        dev_devtools: false,
        footer: None,
        site_origin: None,
        social_image: None,
        sections: vec![
            CmsSection::Hero {
                eyebrow: Some("New".to_owned()),
                title: title.to_owned(),
                lede: Some("One-sentence summary that conveys the value of this page.".to_owned()),
                cta: Some(HeroCta {
                    label: "Get started".to_owned(),
                    href: "/post-skill".to_owned(),
                    data_backend: "post-skill".to_owned(),
                }),
            },
            CmsSection::Composer {
                prompt: "What did you nail today?".to_owned(),
                submit_endpoint: "/post-skill".to_owned(),
                actions: vec![
                    CmsPromptAction::UploadClip,
                    CmsPromptAction::ChallengeOpponent,
                    CmsPromptAction::GoLive,
                ],
                avatar: CmsAvatar::Initials {
                    letters: "DA".to_owned(),
                },
                size: CmsComposerSize::Comfortable,
            },
            CmsSection::CardFeed {
                heading: Some("Featured".to_owned()),
                items: (1..=3)
                    .map(|i| CmsCard {
                        avatar: CmsAvatar::Initials {
                            letters: format!("S{i}"),
                        },
                        title: format!("Sample card {i}"),
                        host: Some(format!("Hosted by @sample · {i}d left")),
                        stats: vec![
                            CmsCardStat {
                                label: "Votes".to_owned(),
                                value: "—".to_owned(),
                            },
                            CmsCardStat {
                                label: "Pot".to_owned(),
                                value: "—".to_owned(),
                            },
                        ],
                        href: format!("/c/sample-{i}"),
                        data_backend: "view-challenge".to_owned(),
                        tag: Some("Sample".to_owned()),
                        tone: None,
                        media: None,
                    })
                    .collect(),
            },
        ],
    }
}

fn cms_template_explainer(title: &str, path: &str) -> loom_cms_render::CmsPage {
    use loom_cms_render::{CmsPage, CmsSection};
    CmsPage {
        brand: None,
        brand_logo: None,
        theme: None,
        chrome: None,
        content_width: None,
        nav_actions: vec![],
        schema: Some("../cms-schema.json".to_owned()),
        title: title.to_owned(),
        description: format!("{title} — explainer / about / FAQ page."),
        path: path.to_owned(),
        nav_links: cms_standard_nav(),
        dev_devtools: false,
        footer: None,
        site_origin: None,
        social_image: None,
        sections: vec![
            CmsSection::Hero {
                eyebrow: None,
                title: title.to_owned(),
                lede: Some("One-sentence summary that conveys the page's purpose.".to_owned()),
                cta: None,
            },
            CmsSection::Group {
                title: "Section A".to_owned(),
                body: vec![
                    "Body text. Replace with your own copy.".to_owned(),
                    "Another paragraph in the same group section.".to_owned(),
                ],
            },
            CmsSection::Group {
                title: "Section B".to_owned(),
                body: vec!["More body text. Each Group is a card.".to_owned()],
            },
            CmsSection::Group {
                title: "Section C".to_owned(),
                body: vec!["Third explainer block.".to_owned()],
            },
        ],
    }
}

fn cms_template_form(title: &str, path: &str) -> loom_cms_render::CmsPage {
    use loom_cms_render::{
        CmsFormField, CmsFormStep, CmsFormStepState, CmsFormStyle, CmsFormSubmit, CmsPage,
        CmsSection,
    };
    CmsPage {
        brand: None,
        brand_logo: None,
        theme: None,
        chrome: None,
        content_width: None,
        nav_actions: vec![],
        schema: Some("../cms-schema.json".to_owned()),
        title: title.to_owned(),
        description: format!("{title} — form / submission page."),
        path: path.to_owned(),
        nav_links: cms_standard_nav(),
        dev_devtools: false,
        footer: None,
        site_origin: None,
        social_image: None,
        sections: vec![
            CmsSection::Hero {
                eyebrow: None,
                title: title.to_owned(),
                lede: Some("Describe what this form does in one sentence.".to_owned()),
                cta: None,
            },
            CmsSection::Form {
                legend: "Submit".to_owned(),
                style: CmsFormStyle::default(),
                submit: CmsFormSubmit {
                    label: "Submit".to_owned(),
                    secondary_label: Some("Cancel".to_owned()),
                    action: "/post-skill".to_owned(),
                    data_backend: "post-skill".to_owned(),
                },
                steps: vec![CmsFormStep {
                    label: "Details".to_owned(),
                    state: CmsFormStepState::Current,
                    fields: vec![
                        CmsFormField::Text {
                            name: "name".to_owned(),
                            label: "Name".to_owned(),
                            hint: None,
                            placeholder: Some("Your name".to_owned()),
                            max_length: Some(120),
                            required: true,
                        },
                        CmsFormField::Textarea {
                            name: "message".to_owned(),
                            label: "Message".to_owned(),
                            hint: Some("Replace with your own field set.".to_owned()),
                            placeholder: None,
                            max_length: None,
                            rows: 4,
                            required: false,
                        },
                    ],
                }],
            },
        ],
    }
}

#[cfg(test)]
mod cmd_cms_new_tests {
    use super::*;

    fn unique(label: &str) -> std::path::PathBuf {
        crate::test_support::unique_tmp("loom-cms-new", label).with_extension("json")
    }

    #[test]
    fn unknown_kind_errors() {
        let out = unique("unknown");
        let r = cmd_cms_new("widget", &out, "X", "/x", false);
        assert!(matches!(r, Err(CmsNewError::UnknownKind(_))));
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let out = unique("overwrite");
        std::fs::write(&out, "existing").expect("write");
        let r = cmd_cms_new("landing", &out, "X", "/x", false);
        assert!(matches!(r, Err(CmsNewError::Conflict(_))));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn force_overwrites() {
        let out = unique("force");
        std::fs::write(&out, "old").expect("write");
        cmd_cms_new("landing", &out, "X", "/x", true).expect("ok");
        let got = std::fs::read_to_string(&out).expect("read");
        assert!(got.starts_with('{'));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn landing_template_round_trips() {
        let out = unique("landing-rt");
        cmd_cms_new("landing", &out, "Landing T", "/landing", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        let page: loom_cms_render::CmsPage = serde_json::from_str(&raw).expect("parse");
        assert_eq!(page.title, "Landing T");
        assert_eq!(page.path, "/landing");
        assert_eq!(page.nav_links.len(), 4);
        assert_eq!(page.sections.len(), 3);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn explainer_template_round_trips() {
        let out = unique("explainer-rt");
        cmd_cms_new("explainer", &out, "About SkillShots", "/about", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        let page: loom_cms_render::CmsPage = serde_json::from_str(&raw).expect("parse");
        assert_eq!(page.sections.len(), 4);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn form_template_round_trips() {
        let out = unique("form-rt");
        cmd_cms_new("form", &out, "Settings", "/settings", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        let page: loom_cms_render::CmsPage = serde_json::from_str(&raw).expect("parse");
        assert_eq!(page.sections.len(), 2);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn output_carries_schema_reference() {
        let out = unique("schema-ref");
        cmd_cms_new("landing", &out, "X", "/x", false).expect("ok");
        let raw = std::fs::read_to_string(&out).expect("read");
        assert!(raw.contains(r#""$schema""#));
        let _ = std::fs::remove_file(&out);
    }
}
