//! AI coding agent CLIs: installed versions (current vs. superseded) and
//! data folders. Claude Code, Codex and Copilot have their own reports in
//! [`super::agent_data`]; this report covers the remaining agents.

use std::path::{Path, PathBuf};

use super::{
    exists, modified_ms, name_of, outermost_total, Ctx, ItemState, RecSpec, ToolItem, ToolReport,
    ToolSection,
};
use crate::model::{CleanupMethod, MarkerKind, Recommendation, Tier};

pub struct VersionedCli {
    pub name: &'static str,
    pub versions_dir: &'static str,
    /// Launcher symlink pointing into the current version.
    pub launcher: &'static str,
    /// Report the recommendations belong to.
    pub source: &'static str,
}

pub const CLAUDE_CODE: VersionedCli = VersionedCli {
    name: "Claude Code",
    versions_dir: ".local/share/claude/versions",
    launcher: ".local/bin/claude",
    source: "claude-code",
};

const CLIS: &[VersionedCli] = &[VersionedCli {
    name: "Cursor Agent CLI",
    versions_dir: ".local/share/cursor-agent/versions",
    launcher: ".local/bin/cursor-agent",
    source: "ai-agents",
}];

const DATA_DIRS: &[(&str, &str, &str)] = &[
    (
        "Cursor agent data",
        ".cursor/projects",
        "Per-project agent state for Cursor.",
    ),
    (
        "Gemini CLI data",
        ".gemini",
        "Configuration and session data.",
    ),
];

/// The version folder (a direct child of `versions`) the launcher points into.
fn current_version(ctx: &Ctx, cli: &VersionedCli) -> Option<PathBuf> {
    let versions = ctx.home_path(cli.versions_dir);
    let target = std::fs::read_link(ctx.home_path(cli.launcher)).ok()?;
    let target = if target.is_absolute() {
        target
    } else {
        ctx.home_path(cli.launcher).parent()?.join(target)
    };
    let rel = target.strip_prefix(&versions).ok()?;
    let first = rel.components().next()?;
    Some(versions.join(first))
}

/// Version entries may be folders or single binaries.
fn version_entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() || t.is_file()))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// A CLI's installed versions, with the superseded ones recommended for review.
pub struct Versions {
    pub section: ToolSection,
    pub recs: Vec<Recommendation>,
    pub root: (PathBuf, u64),
    pub current: Option<String>,
}

pub fn versions(ctx: &Ctx, cli: &VersionedCli) -> Option<Versions> {
    let dir = ctx.home_path(cli.versions_dir);
    if !exists(&dir) {
        return None;
    }
    let current = current_version(ctx, cli);
    let mut items = Vec::new();
    let mut recs = Vec::new();
    for entry in version_entries(&dir) {
        let bytes = ctx.size(&entry);
        let version = name_of(&entry);
        let state = match &current {
            Some(c) if c == &entry => ItemState::Current,
            Some(_) => ItemState::Superseded,
            None => ItemState::Info,
        };
        let mut recommendation_id = None;
        if state == ItemState::Superseded && bytes > 0 {
            let rec = RecSpec {
                source: cli.source,
                title: format!("Old {} version · {}", cli.name, version),
                path: &entry,
                bytes,
                tier: Tier::ReviewFirst,
                methods: &[CleanupMethod::Trash],
                detected_because: format!(
                    "{} keeps previous versions after updating. The launcher (~/{}) points to a different version.",
                    cli.name, cli.launcher
                ),
                safe_because: "The command runs the current version; older versions are only kept as a fallback.",
                risks: "You lose the ability to roll back to this version without reinstalling it.",
                proposed_action: "Move the old version to the Trash",
                alternative: None,
                last_activity_ms: modified_ms(&entry),
                notes: Vec::new(),
            }
            .build(ctx)
            .marked(MarkerKind::OlderVersion, "A newer version is in use");
            recommendation_id = Some(rec.id.clone());
            recs.push(rec);
        }
        items.push(ToolItem {
            label: version,
            path: Some(entry.display().to_string()),
            bytes,
            detail: match state {
                ItemState::Current => "Current version".into(),
                ItemState::Superseded => "Previous version".into(),
                _ => "Launcher not found; cannot tell which version is current".into(),
            },
            state,
            last_used_ms: modified_ms(&entry),
            recommendation_id,
        });
    }
    let bytes: u64 = items.iter().map(|i| i.bytes).sum();
    Some(Versions {
        section: ToolSection {
            title: format!("{} versions", cli.name),
            description: format!("Installed in ~/{}.", cli.versions_dir),
            items,
        },
        recs,
        root: (dir, bytes),
        current: current.as_deref().map(name_of),
    })
}

pub fn analyze(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let mut sections = Vec::new();
    let mut recs = Vec::new();
    let mut roots: Vec<(PathBuf, u64)> = Vec::new();
    let mut versions_summary = Vec::new();

    for cli in CLIS {
        let Some(mut v) = versions(ctx, cli) else {
            continue;
        };
        versions_summary.push(format!(
            "{} ({} installed)",
            cli.name,
            v.section.items.len()
        ));
        roots.push(v.root);
        recs.append(&mut v.recs);
        sections.push(v.section);
    }

    let mut data_items = Vec::new();
    for (label, rel, detail) in DATA_DIRS {
        let path = ctx.home_path(rel);
        if !exists(&path) {
            continue;
        }
        let bytes = ctx.size(&path);
        roots.push((path.clone(), bytes));
        data_items.push(ToolItem {
            label: (*label).into(),
            path: Some(path.display().to_string()),
            bytes,
            detail: (*detail).into(),
            state: ItemState::Info,
            last_used_ms: modified_ms(&path),
            recommendation_id: None,
        });
    }
    if !data_items.is_empty() {
        sections.push(ToolSection {
            title: "Agent data".into(),
            description: "Conversation history and configuration. Not recommended for removal."
                .into(),
            items: data_items,
        });
    }

    let detected = !sections.is_empty();
    let summary = if detected {
        if versions_summary.is_empty() {
            "Agent data found.".into()
        } else {
            format!("{}.", versions_summary.join(", "))
        }
    } else {
        "No other AI agent CLIs found.".into()
    };
    (
        ToolReport {
            id: "ai-agents".into(),
            name: "Other AI agents".into(),
            detected,
            version: None,
            total_bytes: outermost_total(&roots),
            summary,
            sections,
        },
        recs,
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ctx, write};
    use super::*;

    #[cfg(unix)]
    #[test]
    fn current_version_comes_from_the_launcher_symlink() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let vdir = home.join(".local/share/claude/versions");
        write(&vdir.join("2.1.312"), 4000);
        write(&vdir.join("2.1.316"), 4000);
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::os::unix::fs::symlink(vdir.join("2.1.316"), home.join(".local/bin/claude")).unwrap();

        let v = versions(&ctx(&home), &CLAUDE_CODE).unwrap();
        let (items, recs) = (&v.section.items, &v.recs);
        assert_eq!(v.current.as_deref(), Some("2.1.316"));
        assert_eq!(
            items.iter().find(|i| i.label == "2.1.316").unwrap().state,
            ItemState::Current
        );
        assert_eq!(
            items.iter().find(|i| i.label == "2.1.312").unwrap().state,
            ItemState::Superseded
        );
        assert_eq!(recs.len(), 1);
        assert!(recs[0].path.ends_with("2.1.312"));
    }

    #[test]
    fn without_a_launcher_nothing_is_recommended() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(
            &home.join(".local/share/cursor-agent/versions/2026.01.01-a/cursor-agent"),
            10,
        );
        write(
            &home.join(".local/share/cursor-agent/versions/2026.02.01-b/cursor-agent"),
            10,
        );
        let (report, recs) = analyze(&ctx(&home));
        assert!(recs.is_empty());
        assert_eq!(report.name, "Other AI agents");
        assert!(report.sections[0]
            .items
            .iter()
            .all(|i| i.state == ItemState::Info));
    }
}
