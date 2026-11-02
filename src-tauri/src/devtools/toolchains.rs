//! Rust (rustup toolchains, Cargo caches, target folders) and Python
//! (virtual environments, package caches, Hugging Face models).

use std::path::{Path, PathBuf};

use super::{
    exists, modified_ms, name_of, outermost_total, subdirs, Ctx, ItemState, ToolItem, ToolReport,
    ToolSection, STALE_DAYS,
};
use crate::model::Recommendation;

fn latest_mtime(dir: &Path, files: &[&str]) -> Option<i64> {
    files.iter().filter_map(|f| modified_ms(&dir.join(f))).max()
}

/// `default_toolchain` from rustup's settings.toml.
fn rustup_default(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join(".rustup/settings.toml")).ok()?;
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "default_toolchain").then(|| v.trim().trim_matches('"').to_string())
    })
}

/// Project folders whose build output the scan detected, with activity state.
fn project_items(ctx: &Ctx, rule: &str, manifests: &[&str], hint: &str) -> Vec<ToolItem> {
    let mut items: Vec<ToolItem> = ctx
        .detected(rule)
        .into_iter()
        .map(|(path, bytes)| {
            let project = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let activity = latest_mtime(&project, manifests);
            let days = activity.map(|ms| ctx.days_since(ms));
            let stale = days.is_some_and(|d| d >= STALE_DAYS);
            ToolItem {
                label: name_of(&project),
                path: Some(path.display().to_string()),
                bytes,
                detail: match days {
                    Some(d) if stale => format!("{hint} unchanged for {d} days"),
                    Some(d) => format!("{hint} changed {d} days ago"),
                    None => String::new(),
                },
                state: if stale {
                    ItemState::Stale
                } else {
                    ItemState::Current
                },
                last_used_ms: activity,
                recommendation_id: Some(path.display().to_string()),
            }
        })
        .collect();
    items.sort_by_key(|i| (i.state != ItemState::Stale, std::cmp::Reverse(i.bytes)));
    items
}

fn path_items(ctx: &Ctx, entries: &[(&str, &str, &str)]) -> Vec<ToolItem> {
    entries
        .iter()
        .filter_map(|(label, rel, detail)| {
            let path = ctx.home_path(rel);
            exists(&path).then(|| ToolItem {
                label: (*label).into(),
                bytes: ctx.size(&path),
                path: Some(path.display().to_string()),
                detail: (*detail).into(),
                state: ItemState::Info,
                last_used_ms: modified_ms(&path),
                recommendation_id: None,
            })
        })
        .collect()
}

fn roots_of(sections: &[ToolSection]) -> Vec<(PathBuf, u64)> {
    sections
        .iter()
        .flat_map(|s| s.items.iter())
        .filter_map(|i| i.path.as_ref().map(|p| (PathBuf::from(p), i.bytes)))
        .collect()
}

pub fn rust(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let mut sections = Vec::new();
    let default = rustup_default(ctx.home);

    let toolchains: Vec<ToolItem> = subdirs(&ctx.home_path(".rustup/toolchains"))
        .into_iter()
        .map(|d| {
            let name = name_of(&d);
            let is_default = default.as_deref() == Some(name.as_str());
            ToolItem {
                detail: if is_default {
                    "Default toolchain".into()
                } else {
                    format!("Remove with `rustup toolchain uninstall {name}` if unused")
                },
                bytes: ctx.size(&d),
                path: Some(d.display().to_string()),
                label: name,
                state: if is_default {
                    ItemState::Current
                } else {
                    ItemState::Info
                },
                last_used_ms: modified_ms(&d),
                recommendation_id: None,
            }
        })
        .collect();
    if !toolchains.is_empty() {
        sections.push(ToolSection {
            title: "Toolchains".into(),
            description: "Installed with rustup. DiskPilot does not remove toolchains itself."
                .into(),
            items: toolchains,
        });
    }

    let targets = project_items(
        ctx,
        "rust.target",
        &["Cargo.toml", "Cargo.lock"],
        "Cargo.toml",
    );
    if !targets.is_empty() {
        sections.push(ToolSection {
            title: "Build output (target/)".into(),
            description: "Per-project build folders found by the last scan; each is under Recommendations (`cargo clean`).".into(),
            items: targets,
        });
    }

    let caches = path_items(
        ctx,
        &[
            (
                "Cargo registry",
                ".cargo/registry",
                "Downloaded crates; re-downloaded when needed",
            ),
            (
                "Cargo git checkouts",
                ".cargo/git",
                "Git dependencies; re-fetched when needed",
            ),
        ],
    );
    if !caches.is_empty() {
        sections.push(ToolSection {
            title: "Cargo caches".into(),
            description: "Shared download caches.".into(),
            items: caches,
        });
    }

    let detected = !sections.is_empty();
    let summary = if detected {
        format!(
            "{} toolchain(s), {} project build folder(s).",
            sections
                .iter()
                .find(|s| s.title == "Toolchains")
                .map_or(0, |s| s.items.len()),
            sections
                .iter()
                .find(|s| s.title.starts_with("Build output"))
                .map_or(0, |s| s.items.len())
        )
    } else {
        "No Rust tooling found.".into()
    };
    let total = outermost_total(&roots_of(&sections));
    (
        ToolReport {
            id: "rust".into(),
            name: "Rust".into(),
            detected,
            version: default,
            total_bytes: total,
            summary,
            sections,
        },
        Vec::new(),
    )
}

pub fn python(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let mut sections = Vec::new();
    let venvs = project_items(
        ctx,
        "python.venv",
        &[
            "pyproject.toml",
            "requirements.txt",
            "setup.py",
            "Pipfile",
            "uv.lock",
            "poetry.lock",
        ],
        "Dependency files",
    );
    if !venvs.is_empty() {
        sections.push(ToolSection {
            title: "Virtual environments".into(),
            description: "Project environments found by the last scan; each is under Recommendations.".into(),
            items: venvs,
        });
    }
    let caches = path_items(
        ctx,
        &[
            (
                "pip cache",
                "Library/Caches/pip",
                "Clean with `pip cache purge`",
            ),
            ("uv cache", ".cache/uv", "Clean with `uv cache clean`"),
            (
                "Poetry cache",
                "Library/Caches/pypoetry",
                "Clean with `poetry cache clear --all .`",
            ),
        ],
    );
    if !caches.is_empty() {
        sections.push(ToolSection {
            title: "Package caches".into(),
            description: "Downloaded packages, regenerable.".into(),
            items: caches,
        });
    }
    let hf = ctx.home_path(".cache/huggingface/hub");
    let mut models: Vec<ToolItem> = subdirs(&hf)
        .into_iter()
        .filter(|d| name_of(d).starts_with("models--") || name_of(d).starts_with("datasets--"))
        .map(|d| {
            let name = name_of(&d);
            let (kind, rest) = name.split_once("--").unwrap_or(("", &name));
            ToolItem {
                label: rest.replacen("--", "/", 1),
                bytes: ctx.size(&d),
                path: Some(d.display().to_string()),
                detail: format!(
                    "Hugging Face {}; slow to download again",
                    kind.trim_end_matches('s')
                ),
                state: ItemState::Info,
                last_used_ms: modified_ms(&d),
                recommendation_id: None,
            }
        })
        .collect();
    models.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    if !models.is_empty() {
        sections.push(ToolSection {
            title: "Hugging Face models and datasets".into(),
            description: "Downloaded model weights. Remove with `huggingface-cli delete-cache` if no longer needed.".into(),
            items: models,
        });
    }
    let detected = !sections.is_empty();
    let total = outermost_total(&roots_of(&sections));
    (
        ToolReport {
            id: "python".into(),
            name: "Python".into(),
            detected,
            version: None,
            total_bytes: total,
            summary: if detected {
                format!(
                    "{} location(s) analyzed.",
                    sections.iter().map(|s| s.items.len()).sum::<usize>()
                )
            } else {
                "No Python tooling found.".into()
            },
            sections,
        },
        Vec::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ctx, write};
    use super::*;

    #[test]
    fn marks_the_default_rust_toolchain() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(
            &home.join(".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc"),
            100,
        );
        write(
            &home.join(".rustup/toolchains/nightly-aarch64-apple-darwin/bin/rustc"),
            100,
        );
        std::fs::write(
            home.join(".rustup/settings.toml"),
            "version = \"12\"\ndefault_toolchain = \"stable-aarch64-apple-darwin\"\n",
        )
        .unwrap();
        let (report, _) = rust(&ctx(&home));
        let tc = &report.sections[0].items;
        assert_eq!(
            tc.iter().filter(|i| i.state == ItemState::Current).count(),
            1
        );
        assert_eq!(
            report.version.as_deref(),
            Some("stable-aarch64-apple-darwin")
        );
    }

    #[test]
    fn lists_hugging_face_models() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(
            &home.join(".cache/huggingface/hub/models--org--model-x/blobs/a"),
            5000,
        );
        let (report, _) = python(&ctx(&home));
        let models = report
            .sections
            .iter()
            .find(|s| s.title.starts_with("Hugging"))
            .unwrap();
        assert_eq!(models.items[0].label, "org/model-x");
    }
}
