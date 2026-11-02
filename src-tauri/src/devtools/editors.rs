//! Cursor and VS Code: workspace storage per project, installed vs. leftover
//! extension folders, and caches.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::agent_data::{self, Agent};
use super::{
    app_version, exists, file_uri_to_path, modified_ms, name_of, outermost_total, subdirs, Ctx,
    ItemState, RecSpec, ToolItem, ToolReport, ToolSection,
};
use crate::model::{CleanupMethod, MarkerKind, Recommendation, Tier};

struct EditorPaths {
    id: &'static str,
    extras: &'static Agent,
    name: &'static str,
    app_bundle: PathBuf,
    app_support: PathBuf,
    extensions: PathBuf,
}

pub fn cursor(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    analyze(
        ctx,
        EditorPaths {
            id: "cursor",
            extras: &agent_data::CURSOR_EXTRA,
            name: "Cursor",
            app_bundle: PathBuf::from("/Applications/Cursor.app"),
            app_support: ctx.home_path("Library/Application Support/Cursor"),
            extensions: ctx.home_path(".cursor/extensions"),
        },
    )
}

pub fn vscode(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    analyze(
        ctx,
        EditorPaths {
            id: "vscode",
            extras: &agent_data::VSCODE_EXTRA,
            name: "VS Code",
            app_bundle: PathBuf::from("/Applications/Visual Studio Code.app"),
            app_support: ctx.home_path("Library/Application Support/Code"),
            extensions: ctx.home_path(".vscode/extensions"),
        },
    )
}

const CACHE_DIRS: &[&str] = &[
    "Cache",
    "CachedData",
    "Code Cache",
    "GPUCache",
    "CachedExtensionVSIXs",
    "CachedProfilesData",
    "logs",
];

#[derive(Deserialize)]
struct WorkspaceJson {
    folder: Option<String>,
    workspace: Option<String>,
}

/// Where a workspace-storage folder points, if it is a local path.
enum Target {
    Local(PathBuf),
    Remote(String),
    Unknown,
}

fn workspace_target(dir: &Path) -> Target {
    let Ok(text) = std::fs::read_to_string(dir.join("workspace.json")) else {
        return Target::Unknown;
    };
    let Ok(ws) = serde_json::from_str::<WorkspaceJson>(&text) else {
        return Target::Unknown;
    };
    match ws.folder.or(ws.workspace) {
        Some(uri) => match file_uri_to_path(&uri) {
            Some(p) => Target::Local(p),
            None => Target::Remote(uri.split("://").next().unwrap_or("remote").to_string()),
        },
        None => Target::Unknown,
    }
}

#[derive(Deserialize)]
struct ExtensionEntry {
    #[serde(rename = "relativeLocation")]
    relative_location: Option<String>,
    location: Option<ExtensionLocation>,
}

#[derive(Deserialize)]
struct ExtensionLocation {
    path: Option<String>,
}

/// Extension folders the editor's own registry (`extensions.json`) refers to.
fn registered_extensions(ext_dir: &Path) -> Option<HashSet<String>> {
    let text = std::fs::read_to_string(ext_dir.join("extensions.json")).ok()?;
    let entries: Vec<ExtensionEntry> = serde_json::from_str(&text).ok()?;
    Some(
        entries
            .into_iter()
            .filter_map(|e| {
                e.relative_location.or_else(|| {
                    e.location
                        .and_then(|l| l.path)
                        .map(|p| name_of(Path::new(&p)))
                })
            })
            .collect(),
    )
}

/// Display name for a workspace target: the folder name, the stem of a
/// `.code-workspace` file, or "Untitled workspace" for the editor's own
/// generated multi-root workspace files.
fn workspace_label(target: &Path) -> String {
    match target.file_name().and_then(|n| n.to_str()) {
        Some("workspace.json") => "Untitled multi-root workspace".into(),
        Some(n) if n.ends_with(".code-workspace") => n.trim_end_matches(".code-workspace").into(),
        _ => name_of(target),
    }
}

/// Orphaned workspace storage smaller than this is listed but not turned into
/// a recommendation, to keep Recommendations readable.
const MIN_RECOMMENDED_BYTES: u64 = 1_000_000;

fn analyze(ctx: &Ctx, p: EditorPaths) -> (ToolReport, Vec<Recommendation>) {
    let detected = exists(&p.app_support) || exists(&p.extensions) || exists(&p.app_bundle);
    let mut recs = Vec::new();
    let mut sections = Vec::new();
    if !detected {
        return (
            ToolReport {
                id: p.id.into(),
                name: p.name.into(),
                detected,
                version: None,
                total_bytes: 0,
                summary: format!("{} is not installed for this user.", p.name),
                sections,
            },
            recs,
        );
    }

    // ---- Workspace storage -------------------------------------------------
    let mut ws_items = Vec::new();
    let mut orphaned = (0usize, 0u64);
    for dir in subdirs(&p.app_support.join("User/workspaceStorage")) {
        let bytes = ctx.size(&dir);
        let last_used = modified_ms(&dir.join("state.vscdb")).or_else(|| modified_ms(&dir));
        let (label, detail, state) = match workspace_target(&dir) {
            Target::Local(project) if exists(&project) => (
                workspace_label(&project),
                project.display().to_string(),
                ItemState::Current,
            ),
            Target::Local(project) => (
                workspace_label(&project),
                format!("{} (project no longer exists)", project.display()),
                ItemState::Orphaned,
            ),
            Target::Remote(kind) => (
                format!("Remote workspace ({kind})"),
                String::new(),
                ItemState::Info,
            ),
            Target::Unknown => (
                "Empty window or unknown workspace".into(),
                String::new(),
                ItemState::Info,
            ),
        };
        let mut recommendation_id = None;
        if state == ItemState::Orphaned {
            orphaned.0 += 1;
            orphaned.1 += bytes;
        }
        if state == ItemState::Orphaned && bytes >= MIN_RECOMMENDED_BYTES {
            let rec = RecSpec {
                source: p.id,
                title: format!("{} workspace storage · {}", p.name, label),
                path: &dir,
                bytes,
                tier: Tier::ReviewFirst,
                methods: &[CleanupMethod::Trash],
                detected_because: format!(
                    "{}'s workspace storage for {}, a folder that no longer exists.",
                    p.name, detail
                ),
                safe_because: "The project folder it belongs to has been deleted or moved, so the editor will not open this state again.",
                risks: "Contains that project's editor state and agent/chat history. If the project was only moved or renamed, its history is lost when reopened from the new location.",
                proposed_action: "Move the workspace storage folder to the Trash",
                alternative: None,
                last_activity_ms: last_used,
                notes: vec![format!("Quit {} first.", p.name)],
            }
            .build(ctx)
            .marked(MarkerKind::Leftover, "Project folder no longer exists");
            recommendation_id = Some(rec.id.clone());
            recs.push(rec);
        }
        ws_items.push(ToolItem {
            label,
            path: Some(dir.display().to_string()),
            bytes,
            detail,
            state,
            last_used_ms: last_used,
            recommendation_id,
        });
    }
    ws_items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    if !ws_items.is_empty() {
        sections.push(ToolSection {
            title: "Workspace storage".into(),
            description: "Per-project editor state, including agent and chat history. Storage for projects that no longer exist is flagged.".into(),
            items: ws_items,
        });
    }

    // ---- Extensions --------------------------------------------------------
    let registered = registered_extensions(&p.extensions);
    let mut ext_items = Vec::new();
    for dir in subdirs(&p.extensions) {
        let name = name_of(&dir);
        if name.starts_with('.') {
            continue;
        }
        let bytes = ctx.size(&dir);
        let state = match &registered {
            Some(set) if set.contains(&name) => ItemState::Current,
            Some(_) => ItemState::Superseded,
            None => ItemState::Info,
        };
        let mut recommendation_id = None;
        if state == ItemState::Superseded && bytes > 0 {
            let rec = RecSpec {
                source: p.id,
                title: format!("Leftover {} extension · {}", p.name, name),
                path: &dir,
                bytes,
                tier: Tier::ReviewFirst,
                methods: &[CleanupMethod::Trash],
                detected_because: format!(
                    "An extension folder that {}'s extension registry (extensions.json) no longer lists, typically an older version left behind by an update or an uninstalled extension.",
                    p.name
                ),
                safe_because: "The editor does not load extensions that are not in its registry.",
                risks: "If the registry is out of date, the editor may need to reinstall the extension.",
                proposed_action: "Move the extension folder to the Trash",
                alternative: None,
                last_activity_ms: modified_ms(&dir),
                notes: vec![format!("Quit {} first.", p.name)],
            }
            .build(ctx)
            .marked(MarkerKind::Leftover, "Not in the editor's extension list");
            recommendation_id = Some(rec.id.clone());
            recs.push(rec);
        }
        ext_items.push(ToolItem {
            label: name,
            path: Some(dir.display().to_string()),
            bytes,
            detail: match state {
                ItemState::Current => "Installed".into(),
                ItemState::Superseded => "Not in the extension registry".into(),
                _ => String::new(),
            },
            state,
            last_used_ms: modified_ms(&dir),
            recommendation_id,
        });
    }
    ext_items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    let leftover = ext_items
        .iter()
        .filter(|i| i.state == ItemState::Superseded)
        .count();
    if !ext_items.is_empty() {
        sections.push(ToolSection {
            title: "Extensions".into(),
            description: "Installed extensions, and folders the editor no longer uses.".into(),
            items: ext_items,
        });
    }

    // ---- Caches & logs -----------------------------------------------------
    let cache_items: Vec<ToolItem> = CACHE_DIRS
        .iter()
        .map(|c| p.app_support.join(c))
        .filter(|d| exists(d))
        .map(|d| ToolItem {
            label: name_of(&d),
            bytes: ctx.size(&d),
            path: Some(d.display().to_string()),
            detail: "Rebuilt automatically. Available under Recommendations after a scan.".into(),
            state: ItemState::Info,
            last_used_ms: modified_ms(&d),
            recommendation_id: None,
        })
        .collect();
    if !cache_items.is_empty() {
        sections.push(ToolSection {
            title: "Caches and logs".into(),
            description: "Regenerable data inside the app's support folder.".into(),
            items: cache_items,
        });
    }

    // ---- Chat database, update downloads, extra caches, backups ------------
    let mut extras = agent_data::editor_extras(ctx, p.extras);
    let chat = (p.id == "cursor")
        .then(|| agent_data::cursor_chat_item(ctx, &p.app_support.join("User/globalStorage")))
        .flatten();
    if let Some(chat) = chat {
        if let Some(kept) = extras
            .sections
            .iter_mut()
            .find(|s| s.title == "History and settings")
        {
            kept.items.insert(0, chat);
        } else {
            extras.sections.push(ToolSection {
                title: "History and settings".into(),
                description: "Conversation history and settings. Listed for reference; never recommended for removal.".into(),
                items: vec![chat],
            });
        }
    }
    let extra_count = extras.recs.len();
    sections.append(&mut extras.sections);
    recs.append(&mut extras.recs);
    let mut roots = vec![
        (p.app_support.clone(), ctx.size(&p.app_support)),
        (p.extensions.clone(), ctx.size(&p.extensions)),
    ];
    roots.append(&mut extras.roots);

    let total_bytes = outermost_total(&roots);
    let mut summary = format!(
        "{} workspaces, {} extension folders.",
        sections
            .iter()
            .find(|s| s.title == "Workspace storage")
            .map_or(0, |s| s.items.len()),
        sections
            .iter()
            .find(|s| s.title == "Extensions")
            .map_or(0, |s| s.items.len()),
    );
    if orphaned.0 > 0 {
        summary.push_str(&format!(
            " {} workspace storage folder{} belong to projects that no longer exist.",
            orphaned.0,
            if orphaned.0 == 1 { "" } else { "s" }
        ));
    }
    if leftover > 0 {
        summary.push_str(&format!(
            " {leftover} leftover extension folder{}.",
            if leftover == 1 { "" } else { "s" }
        ));
    }
    if extra_count > 0 {
        summary.push_str(&format!(
            " {extra_count} cache, update or backup item{} to review.",
            if extra_count == 1 { "" } else { "s" }
        ));
    }

    (
        ToolReport {
            id: p.id.into(),
            name: p.name.into(),
            detected,
            version: app_version(&p.app_bundle),
            total_bytes,
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

    #[test]
    fn flags_workspaces_of_deleted_projects_and_leftover_extensions() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let live = home.join("code/live");
        std::fs::create_dir_all(&live).unwrap();
        let ws = home.join("Library/Application Support/Cursor/User/workspaceStorage");
        write(&ws.join("aaa/state.vscdb"), 5000);
        std::fs::write(
            ws.join("aaa/workspace.json"),
            format!(r#"{{"folder":"file://{}"}}"#, live.display()),
        )
        .unwrap();
        write(&ws.join("bbb/state.vscdb"), 2_000_000);
        std::fs::write(
            ws.join("bbb/workspace.json"),
            format!(
                r#"{{"folder":"file://{}/code/gone%20away"}}"#,
                home.display()
            ),
        )
        .unwrap();
        let ext = home.join(".cursor/extensions");
        write(&ext.join("pub.tool-1.0.0/package.json"), 100);
        write(&ext.join("pub.tool-1.1.0/package.json"), 100);
        std::fs::write(
            ext.join("extensions.json"),
            r#"[{"identifier":{"id":"pub.tool"},"relativeLocation":"pub.tool-1.1.0"}]"#,
        )
        .unwrap();

        let c = ctx(&home);
        let (report, recs) = cursor(&c);
        assert!(report.detected);
        let ws_section = &report.sections[0];
        let gone = ws_section
            .items
            .iter()
            .find(|i| i.label == "gone away")
            .unwrap();
        assert_eq!(gone.state, ItemState::Orphaned);
        assert!(gone.recommendation_id.is_some());
        let alive = ws_section.items.iter().find(|i| i.label == "live").unwrap();
        assert_eq!(alive.state, ItemState::Current);

        assert_eq!(recs.len(), 2);
        assert!(recs.iter().all(|r| r.methods == vec![CleanupMethod::Trash]));
        assert!(recs.iter().any(|r| r.path.ends_with("pub.tool-1.0.0")));
        assert!(!recs.iter().any(|r| r.path.ends_with("pub.tool-1.1.0")));
        assert!(report.summary.contains("no longer exist"));
    }

    #[test]
    fn labels_generated_workspaces() {
        assert_eq!(
            workspace_label(Path::new("/x/Workspaces/123/workspace.json")),
            "Untitled multi-root workspace"
        );
        assert_eq!(workspace_label(Path::new("/x/app.code-workspace")), "app");
        assert_eq!(workspace_label(Path::new("/x/my-app")), "my-app");
    }

    #[test]
    fn absent_editor_is_reported_as_not_detected() {
        let dir = crate::test_support::sandbox();
        let (report, recs) = vscode(&ctx(dir.path()));
        // The real /Applications bundle may exist on the test machine; data
        // folders in the sandbox do not, so nothing is recommended.
        assert!(recs.is_empty());
        assert!(report.sections.is_empty());
    }
}
