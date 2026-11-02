//! Data kept by AI coding agents and the editors they run in: caches, logs,
//! leftover update downloads, stale database backups, checkpoints, and
//! history for projects that no longer exist.
//!
//! Every location is described in a table so the rules are easy to review.
//! Caches, logs and update downloads are regenerable and recommended as
//! likely safe. Backups, checkpoints and history for deleted projects need
//! the user's judgment. Conversation history, sign-in and settings are
//! reported but never recommended. Other apps' databases are only read,
//! never modified.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use super::agents::{self, CLAUDE_CODE};
use super::{
    app_version, exists, modified_ms, name_of, outermost_total, subdirs, Ctx, ItemState, RecSpec,
    ToolItem, ToolReport, ToolSection,
};
use crate::docker::actions::human;
use crate::model::{CleanupMethod, MarkerKind, Recommendation, Tier};

/// What a location holds, which decides whether and how it is recommended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Rebuilt automatically when needed.
    Cache,
    /// Diagnostic logs.
    Logs,
    /// A downloaded app update kept by the updater.
    Update,
    /// A backup copy of a state database; recommended once it is old.
    Backup,
    /// File snapshots used to undo an agent's edits.
    Checkpoints,
    /// Conversation history: reported, never recommended.
    History,
    /// Sign-in, settings and app state: reported, never recommended.
    Config,
}

pub struct Entry {
    pub label: &'static str,
    /// Relative to the home folder.
    pub rel: &'static str,
    pub kind: Kind,
    pub detail: &'static str,
}

pub struct Agent {
    /// Report id; also the recommendation source.
    pub id: &'static str,
    pub name: &'static str,
    /// What to close before cleaning up.
    pub quit: &'static str,
    pub entries: &'static [Entry],
}

/// Items smaller than this are listed but not recommended.
const MIN_RECOMMENDED_BYTES: u64 = 1_000_000;
/// A database backup older than this is considered stale.
pub const BACKUP_STALE_DAYS: i64 = 30;

const fn e(label: &'static str, rel: &'static str, kind: Kind, detail: &'static str) -> Entry {
    Entry {
        label,
        rel,
        kind,
        detail,
    }
}

use Kind::*;

pub const CLAUDE_CODE_DATA: Agent = Agent {
    id: "claude-code",
    name: "Claude Code",
    quit: "Close running Claude Code sessions first.",
    entries: &[
        e(
            "MCP and debug logs",
            "Library/Caches/claude-cli-nodejs",
            Logs,
            "Per-project MCP server logs written by Claude Code.",
        ),
        e(
            "Debug logs",
            ".claude/debug",
            Logs,
            "Debug output from past sessions.",
        ),
        e(
            "Shell snapshots",
            ".claude/shell-snapshots",
            Cache,
            "Shell environment captured when a session starts; each session takes a new one.",
        ),
        e(
            "Cache",
            ".claude/cache",
            Cache,
            "Downloaded changelog and model metadata.",
        ),
        e(
            "Edit checkpoints",
            ".claude/file-history",
            Checkpoints,
            "Copies of files Claude Code edited, used to rewind changes in a session.",
        ),
        e(
            "Pasted content",
            ".claude/paste-cache",
            History,
            "Large pastes referenced from session transcripts.",
        ),
        e(
            "Prompt history",
            ".claude/history.jsonl",
            History,
            "Prompts you typed, used for history search.",
        ),
        e(
            "Plugins",
            ".claude/plugins",
            Config,
            "Installed plugins and marketplaces.",
        ),
        e(
            "Settings",
            ".claude/settings.json",
            Config,
            "Your Claude Code settings.",
        ),
    ],
};

pub const CLAUDE_DESKTOP: Agent = Agent {
    id: "claude-desktop",
    name: "Claude app",
    quit: "Quit the Claude app first.",
    entries: &[
        e(
            "Web cache",
            "Library/Application Support/Claude/Cache",
            Cache,
            "Chromium network cache.",
        ),
        e(
            "Code cache",
            "Library/Application Support/Claude/Code Cache",
            Cache,
            "Compiled JavaScript cache.",
        ),
        e(
            "GPU cache",
            "Library/Application Support/Claude/GPUCache",
            Cache,
            "Compiled graphics shaders.",
        ),
        e(
            "Graphics cache",
            "Library/Application Support/Claude/DawnGraphiteCache",
            Cache,
            "WebGPU shader cache.",
        ),
        e(
            "WebGPU cache",
            "Library/Application Support/Claude/DawnWebGPUCache",
            Cache,
            "WebGPU pipeline cache.",
        ),
        e(
            "App cache",
            "Library/Caches/com.anthropic.claudefordesktop",
            Cache,
            "Network and update metadata cache.",
        ),
        e(
            "Downloaded update",
            "Library/Caches/com.anthropic.claudefordesktop.ShipIt",
            Update,
            "An app update downloaded by the updater.",
        ),
        e(
            "Logs",
            "Library/Logs/Claude",
            Logs,
            "Diagnostic logs, including MCP server logs.",
        ),
        e(
            "App data",
            "Library/Application Support/Claude",
            Config,
            "Sign-in, settings, connectors and local state.",
        ),
    ],
};

pub const CODEX: Agent = Agent {
    id: "codex",
    name: "Codex",
    quit: "Quit the Codex app and close running Codex sessions first.",
    entries: &[
        e(
            "CLI logs",
            ".codex/log",
            Logs,
            "Logs written by the Codex CLI.",
        ),
        e(
            "App logs",
            "Library/Logs/com.openai.codex",
            Logs,
            "Diagnostic logs from the Codex app.",
        ),
        e(
            "App cache",
            "Library/Caches/com.openai.codex",
            Cache,
            "Network cache and update metadata for the Codex app.",
        ),
        e(
            "Downloaded update",
            "Library/Caches/com.openai.codex.ShipIt",
            Update,
            "An app update downloaded by the updater.",
        ),
        e(
            "Sessions",
            ".codex/sessions",
            History,
            "Saved sessions, used by codex resume.",
        ),
        e(
            "Archived sessions",
            ".codex/archived_sessions",
            History,
            "Sessions you archived.",
        ),
        e(
            "Prompt history",
            ".codex/history.jsonl",
            History,
            "Prompts you typed.",
        ),
        e(
            "Configuration",
            ".codex/config.toml",
            Config,
            "Your Codex settings.",
        ),
        e(
            "Sign-in",
            ".codex/auth.json",
            Config,
            "Codex sign-in. Never removed.",
        ),
    ],
};

pub const COPILOT: Agent = Agent {
    id: "copilot",
    name: "GitHub Copilot",
    quit: "Quit your editor and close running Copilot CLI sessions first.",
    entries: &[
        e(
            "CLI logs",
            ".copilot/logs",
            Logs,
            "Logs written by the Copilot CLI.",
        ),
        e(
            "CLI sessions",
            ".copilot/session-state",
            History,
            "Copilot CLI sessions, used to resume them.",
        ),
        e(
            "CLI configuration",
            ".copilot/config.json",
            Config,
            "Copilot CLI settings and trusted folders.",
        ),
        e(
            "Sign-in",
            ".config/github-copilot",
            Config,
            "Copilot sign-in shared by editors. Never removed.",
        ),
    ],
};

/// Cleanable data in Cursor beyond the editor caches in [`super::editors`].
pub const CURSOR_EXTRA: Agent = Agent {
    id: "cursor",
    name: "Cursor",
    quit: "Quit Cursor first.",
    entries: &[
        e("Downloaded update", "Library/Caches/com.todesktop.230313mzl4w4u92.ShipIt", Update, "A Cursor update downloaded by the updater. Cursor downloads it again if it is still needed."),
        e("Network cache", "Library/Caches/com.todesktop.230313mzl4w4u92", Cache, "HTTP cache for Cursor's updater and web views."),
        e("Compile cache", "Library/Caches/cursor-compile-cache", Cache, "Compiled code cache for Cursor's background processes."),
        e("Graphics cache", "Library/Application Support/Cursor/DawnGraphiteCache", Cache, "WebGPU shader cache."),
        e("WebGPU cache", "Library/Application Support/Cursor/DawnWebGPUCache", Cache, "WebGPU pipeline cache."),
        e("State database backup", "Library/Application Support/Cursor/User/globalStorage/state.vscdb.backup", Backup, "A copy of Cursor's state database, used only to recover from corruption."),
    ],
};

/// Cleanable data in VS Code beyond the editor caches in [`super::editors`].
pub const VSCODE_EXTRA: Agent = Agent {
    id: "vscode",
    name: "VS Code",
    quit: "Quit VS Code first.",
    entries: &[
        e(
            "Downloaded update",
            "Library/Caches/com.microsoft.VSCode.ShipIt",
            Update,
            "A VS Code update downloaded by the updater.",
        ),
        e(
            "Graphics cache",
            "Library/Application Support/Code/DawnGraphiteCache",
            Cache,
            "WebGPU shader cache.",
        ),
        e(
            "WebGPU cache",
            "Library/Application Support/Code/DawnWebGPUCache",
            Cache,
            "WebGPU pipeline cache.",
        ),
        e(
            "State database backup",
            "Library/Application Support/Code/User/globalStorage/state.vscdb.backup",
            Backup,
            "A copy of VS Code's state database, used only to recover from corruption.",
        ),
    ],
};

/// Sections, recommendations and measured roots collected for one report.
#[derive(Default)]
pub struct Collected {
    pub sections: Vec<ToolSection>,
    pub recs: Vec<Recommendation>,
    pub roots: Vec<(PathBuf, u64)>,
}

impl Collected {
    fn push(&mut self, section: Option<ToolSection>) {
        if let Some(s) = section.filter(|s| !s.items.is_empty()) {
            self.sections.push(s);
        }
    }

    fn reviewable(&self) -> (usize, u64) {
        (
            self.recs.len(),
            self.recs.iter().map(|r| r.estimated_reclaim_bytes).sum(),
        )
    }
}

fn recommend(
    ctx: &Ctx,
    agent: &Agent,
    entry: &Entry,
    path: &Path,
    bytes: u64,
) -> Option<Recommendation> {
    if bytes < MIN_RECOMMENDED_BYTES {
        return None;
    }
    let last = modified_ms(path);
    let (tier, methods, why, safe, risks): (Tier, &[CleanupMethod], String, &str, &str) =
        match entry.kind {
            Cache => (
                Tier::LikelySafe,
                &[CleanupMethod::Trash, CleanupMethod::Delete],
                format!(
                    "A cache {} rebuilds when it needs it. {}",
                    agent.name, entry.detail
                ),
                "Caches are regenerated automatically.",
                "The app may be slower the first time it rebuilds the cache.",
            ),
            Logs => (
                Tier::LikelySafe,
                &[CleanupMethod::Trash, CleanupMethod::Delete],
                format!("Diagnostic logs. {}", entry.detail),
                "Logs are only read when troubleshooting; new ones are written as needed.",
                "Past logs are no longer available if you need to report a problem.",
            ),
            Update => (
                Tier::LikelySafe,
                &[CleanupMethod::Trash, CleanupMethod::Delete],
                format!(
                    "{} The updater keeps the download after installing it.",
                    entry.detail
                ),
                "The installed app does not use the downloaded copy.",
                "If an update is waiting to be installed, the app downloads it again.",
            ),
            Backup => {
                let days = last.map(|ms| ctx.days_since(ms))?;
                if days < BACKUP_STALE_DAYS {
                    return None;
                }
                (
                    Tier::ReviewFirst,
                    &[CleanupMethod::Trash],
                    format!("{} This copy was last updated {days} days ago.", entry.detail),
                    "The app works from its main database; the backup is only read after corruption, and this one is out of date.",
                    "If the main database becomes corrupted, this older copy is not available to restore from.",
                )
            }
            Checkpoints => (
                Tier::ReviewFirst,
                &[CleanupMethod::Trash],
                entry.detail.to_string(),
                "Only needed to rewind edits in past sessions.",
                "You can no longer rewind file edits made in earlier sessions.",
            ),
            History | Config => return None,
        };
    Some(
        RecSpec {
            source: agent.id,
            title: format!("{} · {}", agent.name, entry.label),
            path,
            bytes,
            tier,
            methods,
            detected_because: why,
            safe_because: safe,
            risks,
            proposed_action: "Move it to the Trash",
            alternative: None,
            last_activity_ms: last,
            notes: vec![agent.quit.to_string()],
        }
        .build(ctx),
    )
}

/// Walk an agent's table, adding "Cleanable data" and "History and settings"
/// sections.
pub fn collect_entries(ctx: &Ctx, agent: &Agent, out: &mut Collected) {
    let mut cleanable = Vec::new();
    let mut kept = Vec::new();
    for entry in agent.entries {
        let path = ctx.home_path(entry.rel);
        if !exists(&path) {
            continue;
        }
        let bytes = ctx.size(&path);
        out.roots.push((path.clone(), bytes));
        let rec = recommend(ctx, agent, entry, &path, bytes);
        let item = ToolItem {
            label: entry.label.into(),
            path: Some(path.display().to_string()),
            bytes,
            detail: entry.detail.into(),
            state: match entry.kind {
                Backup if rec.is_some() => ItemState::Stale,
                _ => ItemState::Info,
            },
            last_used_ms: modified_ms(&path),
            recommendation_id: rec.as_ref().map(|r| r.id.clone()),
        };
        out.recs.extend(rec);
        match entry.kind {
            History | Config => kept.push(item),
            _ => cleanable.push(item),
        }
    }
    cleanable.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    out.push(Some(ToolSection {
        title: "Cleanable data".into(),
        description: "Caches, logs, downloaded updates and backups. Regenerable items are marked likely safe.".into(),
        items: cleanable,
    }));
    out.push(Some(ToolSection {
        title: "History and settings".into(),
        description: "Conversation history, sign-in and settings. Listed for reference; never recommended for removal.".into(),
        items: kept,
    }));
}

fn summary(name: &str, c: &Collected) -> String {
    let (n, bytes) = c.reviewable();
    if n == 0 {
        format!("Nothing to clean up in {name}'s data.")
    } else {
        format!(
            "{n} item{} worth reviewing, {}.",
            if n == 1 { "" } else { "s" },
            human(bytes)
        )
    }
}

fn report(
    id: &str,
    name: &str,
    detected: bool,
    version: Option<String>,
    c: Collected,
) -> (ToolReport, Vec<Recommendation>) {
    let text = if detected {
        summary(name, &c)
    } else {
        format!("{name} was not found for this user.")
    };
    (
        ToolReport {
            id: id.into(),
            name: name.into(),
            detected,
            version,
            total_bytes: outermost_total(&c.roots),
            summary: text,
            sections: c.sections,
        },
        c.recs,
    )
}

// ---- Claude Code ------------------------------------------------------------

#[derive(Deserialize)]
struct TranscriptLine {
    cwd: Option<String>,
}

/// The working directory recorded in a project's transcripts.
fn transcript_cwd(project: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(project)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    for file in files.iter().take(5) {
        let Ok(f) = std::fs::File::open(file) else {
            continue;
        };
        for line in BufReader::new(f).lines().take(50).map_while(Result::ok) {
            if !line.contains("\"cwd\"") {
                continue;
            }
            if let Ok(TranscriptLine { cwd: Some(cwd) }) = serde_json::from_str(&line) {
                return Some(PathBuf::from(cwd));
            }
        }
    }
    None
}

fn claude_projects(ctx: &Ctx, out: &mut Collected) {
    let root = ctx.home_path(".claude/projects");
    let mut items = Vec::new();
    for dir in subdirs(&root) {
        let bytes = ctx.size(&dir);
        let last = modified_ms(&dir);
        let cwd = transcript_cwd(&dir);
        let (label, detail, state) = match &cwd {
            Some(p) if exists(p) => (name_of(p), p.display().to_string(), ItemState::Current),
            Some(p) => (
                name_of(p),
                format!("{} (folder no longer exists)", p.display()),
                ItemState::Orphaned,
            ),
            None => (
                name_of(&dir),
                "Project folder unknown".into(),
                ItemState::Info,
            ),
        };
        let mut recommendation_id = None;
        if state == ItemState::Orphaned && bytes >= MIN_RECOMMENDED_BYTES {
            let rec = RecSpec {
                source: CLAUDE_CODE_DATA.id,
                title: format!("Claude Code sessions · {label}"),
                path: &dir,
                bytes,
                tier: Tier::ReviewFirst,
                methods: &[CleanupMethod::Trash],
                detected_because: format!(
                    "Session transcripts and project memory for {detail}."
                ),
                safe_because: "The folder these sessions ran in has been deleted or moved, so they cannot be resumed from it.",
                risks: "The conversations and any project memory are lost. If the folder was moved or renamed, its history does not follow it.",
                proposed_action: "Move the project's sessions to the Trash",
                alternative: Some("Claude Code can remove old transcripts itself: set cleanupPeriodDays in ~/.claude/settings.json."),
                last_activity_ms: last,
                notes: vec![CLAUDE_CODE_DATA.quit.to_string()],
            }
            .build(ctx)
            .marked(MarkerKind::Leftover, "Project folder no longer exists");
            recommendation_id = Some(rec.id.clone());
            out.recs.push(rec);
        }
        items.push(ToolItem {
            label,
            path: Some(dir.display().to_string()),
            bytes,
            detail,
            state,
            last_used_ms: last,
            recommendation_id,
        });
    }
    if items.is_empty() {
        return;
    }
    out.roots.push((root.clone(), ctx.size(&root)));
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    out.push(Some(ToolSection {
        title: "Sessions by project".into(),
        description: "Transcripts and memory per project. Sessions for folders that no longer exist are flagged.".into(),
        items,
    }));
}

pub fn claude_code(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let mut c = Collected::default();
    let mut version = None;
    if let Some(v) = agents::versions(ctx, &CLAUDE_CODE) {
        version = v.current.clone();
        c.roots.push(v.root);
        c.recs.extend(v.recs);
        c.push(Some(v.section));
    }
    claude_projects(ctx, &mut c);
    collect_entries(ctx, &CLAUDE_CODE_DATA, &mut c);
    let detected = exists(&ctx.home_path(".claude")) || version.is_some();
    report("claude-code", "Claude Code", detected, version, c)
}

// ---- Claude app ---------------------------------------------------------------

pub fn claude_desktop(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let app = PathBuf::from("/Applications/Claude.app");
    let mut c = Collected::default();
    collect_entries(ctx, &CLAUDE_DESKTOP, &mut c);
    let detected = !c.roots.is_empty();
    report(
        "claude-desktop",
        "Claude app",
        detected,
        detected.then(|| app_version(&app)).flatten(),
        c,
    )
}

// ---- Codex ------------------------------------------------------------------

pub fn codex(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let app = PathBuf::from("/Applications/Codex.app");
    let mut c = Collected::default();
    collect_entries(ctx, &CODEX, &mut c);
    let detected = !c.roots.is_empty();
    report(
        "codex",
        "Codex",
        detected,
        detected.then(|| app_version(&app)).flatten(),
        c,
    )
}

// ---- GitHub Copilot -----------------------------------------------------------

struct EditorDirs {
    name: &'static str,
    global_storage: &'static str,
    extensions: &'static str,
}

const COPILOT_EDITORS: &[EditorDirs] = &[
    EditorDirs {
        name: "VS Code",
        global_storage: "Library/Application Support/Code/User/globalStorage",
        extensions: ".vscode/extensions",
    },
    EditorDirs {
        name: "Cursor",
        global_storage: "Library/Application Support/Cursor/User/globalStorage",
        extensions: ".cursor/extensions",
    },
];

fn is_copilot_id(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "github.copilot" || n == "github.copilot-chat"
}

/// Copilot extension data per editor, flagged when the extension it belongs
/// to is no longer installed in that editor.
fn copilot_extension_data(ctx: &Ctx, out: &mut Collected) {
    let mut items = Vec::new();
    for editor in COPILOT_EDITORS {
        let ext_dir = ctx.home_path(editor.extensions);
        let ext_folders = subdirs(&ext_dir);
        let installed: Vec<String> = ext_folders
            .iter()
            .map(|p| name_of(p).to_ascii_lowercase())
            .collect();
        for folder in &ext_folders {
            if !name_of(folder)
                .to_ascii_lowercase()
                .starts_with("github.copilot")
            {
                continue;
            }
            let bytes = ctx.size(folder);
            out.roots.push((folder.clone(), bytes));
            items.push(ToolItem {
                label: format!("{} extension · {}", editor.name, name_of(folder)),
                path: Some(folder.display().to_string()),
                bytes,
                detail: "Installed extension.".into(),
                state: ItemState::Current,
                last_used_ms: modified_ms(folder),
                recommendation_id: None,
            });
        }
        for dir in subdirs(&ctx.home_path(editor.global_storage)) {
            let id = name_of(&dir);
            if !is_copilot_id(&id) {
                continue;
            }
            let bytes = ctx.size(&dir);
            out.roots.push((dir.clone(), bytes));
            let prefix = format!("{}-", id.to_ascii_lowercase());
            // Only judge when the editor's extension folder exists at all.
            let orphaned = exists(&ext_dir) && !installed.iter().any(|n| n.starts_with(&prefix));
            let mut recommendation_id = None;
            if orphaned && bytes >= MIN_RECOMMENDED_BYTES {
                let rec = RecSpec {
                    source: COPILOT.id,
                    title: format!("{} data for uninstalled {id}", editor.name),
                    path: &dir,
                    bytes,
                    tier: Tier::ReviewFirst,
                    methods: &[CleanupMethod::Trash],
                    detected_because: format!(
                        "Storage for the {id} extension in {}, which no longer has that extension installed.",
                        editor.name
                    ),
                    safe_because: "Extension storage is only read by the extension it belongs to.",
                    risks: "If you reinstall the extension, its chat history and indexes start fresh.",
                    proposed_action: "Move the extension's storage to the Trash",
                    alternative: None,
                    last_activity_ms: modified_ms(&dir),
                    notes: vec![format!("Quit {} first.", editor.name)],
                }
                .build(ctx)
            .marked(MarkerKind::Leftover, "Extension is not installed");
                recommendation_id = Some(rec.id.clone());
                out.recs.push(rec);
            }
            items.push(ToolItem {
                label: format!("{} storage · {id}", editor.name),
                path: Some(dir.display().to_string()),
                bytes,
                detail: if orphaned {
                    "Extension no longer installed".into()
                } else {
                    "Chat sessions, indexes and settings for the extension.".into()
                },
                state: if orphaned {
                    ItemState::Orphaned
                } else {
                    ItemState::Info
                },
                last_used_ms: modified_ms(&dir),
                recommendation_id,
            });
        }
    }
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    out.push(Some(ToolSection {
        title: "Editor extensions".into(),
        description: "Copilot extensions and their storage in each editor. Storage left behind by an uninstalled extension is flagged.".into(),
        items,
    }));
}

pub fn copilot(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let mut c = Collected::default();
    copilot_extension_data(ctx, &mut c);
    collect_entries(ctx, &COPILOT, &mut c);
    let detected = !c.roots.is_empty();
    report("copilot", "GitHub Copilot", detected, None, c)
}

// ---- Editor extras ------------------------------------------------------------

/// Cursor's and VS Code's update downloads, extra caches and stale backups.
pub fn editor_extras(ctx: &Ctx, agent: &Agent) -> Collected {
    let mut c = Collected::default();
    collect_entries(ctx, agent, &mut c);
    c
}

/// Read-only statistics about Cursor's chat history database.
pub struct ChatDbStats {
    pub chats: u64,
    pub oldest_ms: Option<i64>,
}

/// Count stored chats without modifying the database. Opened read-only, so
/// it is safe while Cursor is running.
pub fn cursor_chat_stats(db: &Path) -> Option<ChatDbStats> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    conn.busy_timeout(std::time::Duration::from_millis(500))
        .ok()?;
    conn.query_row(
        "SELECT count(*), min(json_extract(value, '$.createdAt')) FROM cursorDiskKV \
         WHERE key >= 'composerData:' AND key < 'composerData;'",
        [],
        |row| {
            Ok(ChatDbStats {
                chats: row.get::<_, i64>(0)?.max(0) as u64,
                oldest_ms: row.get::<_, Option<i64>>(1)?,
            })
        },
    )
    .ok()
}

fn group_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// An informational item describing Cursor's chat history database.
pub fn cursor_chat_item(ctx: &Ctx, global_storage: &Path) -> Option<ToolItem> {
    let db = global_storage.join("state.vscdb");
    let meta = db.symlink_metadata().ok()?;
    let bytes = meta.len()
        + ["state.vscdb-wal", "state.vscdb-shm"]
            .iter()
            .filter_map(|n| global_storage.join(n).symlink_metadata().ok())
            .map(|m| m.len())
            .sum::<u64>();
    let mut detail = String::from(
        "Settings, extension state and every past chat. Delete old chats in Cursor to reduce it; DiskPilot never edits another app's database.",
    );
    if let Some(stats) = cursor_chat_stats(&db) {
        let age = stats
            .oldest_ms
            .map(|ms| format!(", the oldest from {} days ago", ctx.days_since(ms)))
            .unwrap_or_default();
        detail = format!(
            "{} chats stored{age}. {detail}",
            group_thousands(stats.chats)
        );
    }
    Some(ToolItem {
        label: "Chat history database".into(),
        path: Some(db.display().to_string()),
        bytes,
        detail,
        state: ItemState::Info,
        last_used_ms: modified_ms(&db),
        recommendation_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ctx, write};
    use super::*;

    fn home() -> (tempfile::TempDir, PathBuf) {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        (dir, home)
    }

    fn set_mtime(path: &Path, days_ago: i64, now_ms: i64) {
        let secs = (now_ms - days_ago * super::super::DAY_MS) / 1000;
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn caches_logs_and_updates_are_likely_safe_history_is_never_recommended() {
        let (_d, home) = home();
        write(&home.join(".codex/log/codex-tui.log"), 2_000_000);
        write(
            &home.join("Library/Caches/com.openai.codex/Cache.db"),
            3_000_000,
        );
        write(
            &home.join(".codex/sessions/2026/01/01/rollout.jsonl"),
            5_000_000,
        );
        write(&home.join(".codex/auth.json"), 2_000_000);
        write(&home.join(".codex/history.jsonl"), 100);

        let (report, recs) = codex(&ctx(&home));
        assert!(report.detected);
        assert_eq!(recs.len(), 2);
        assert!(recs
            .iter()
            .all(|r| r.tier == Tier::LikelySafe && r.source == "codex"));
        assert!(!recs
            .iter()
            .any(|r| r.path.contains("sessions") || r.path.contains("auth")));
        let kept = report
            .sections
            .iter()
            .find(|s| s.title == "History and settings")
            .unwrap();
        assert!(kept.items.iter().all(|i| i.recommendation_id.is_none()));
        assert!(report.summary.contains("2 items"));
    }

    #[test]
    fn small_items_are_listed_but_not_recommended() {
        let (_d, home) = home();
        write(&home.join(".copilot/logs/a.log"), 10);
        let (report, recs) = copilot(&ctx(&home));
        assert!(report.detected);
        assert!(recs.is_empty());
        assert_eq!(report.sections[0].items.len(), 1);
    }

    #[test]
    fn database_backups_are_recommended_only_when_stale() {
        let (_d, home) = home();
        let c = ctx(&home);
        let gs = home.join("Library/Application Support/Cursor/User/globalStorage");
        write(&gs.join("state.vscdb.backup"), 2_000_000);

        set_mtime(&gs.join("state.vscdb.backup"), 3, c.now_ms);
        assert!(editor_extras(&c, &CURSOR_EXTRA).recs.is_empty());

        set_mtime(&gs.join("state.vscdb.backup"), 120, c.now_ms);
        let extras = editor_extras(&c, &CURSOR_EXTRA);
        assert_eq!(extras.recs.len(), 1);
        assert_eq!(extras.recs[0].tier, Tier::ReviewFirst);
        assert_eq!(extras.recs[0].methods, vec![CleanupMethod::Trash]);
        assert!(extras.recs[0].detected_because.contains("120 days"));
    }

    #[test]
    fn claude_sessions_for_deleted_folders_are_flagged() {
        let (_d, home) = home();
        let live = home.join("code/live");
        std::fs::create_dir_all(&live).unwrap();
        let projects = home.join(".claude/projects");
        for (name, cwd) in [("live", live.clone()), ("gone", home.join("code/gone"))] {
            let dir = projects.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let line = serde_json::json!({"type": "user", "cwd": cwd});
            std::fs::write(
                dir.join("s.jsonl"),
                format!("{{\"type\":\"mode\"}}\n{line}\n{}", "x".repeat(1_100_000)),
            )
            .unwrap();
        }
        write(&home.join(".claude/file-history/abc/f"), 1_500_000);
        write(&home.join(".claude/history.jsonl"), 1_500_000);

        let (report, recs) = claude_code(&ctx(&home));
        assert!(report.detected);
        let sessions = report
            .sections
            .iter()
            .find(|s| s.title == "Sessions by project")
            .unwrap();
        let gone = sessions.items.iter().find(|i| i.label == "gone").unwrap();
        assert_eq!(gone.state, ItemState::Orphaned);
        let alive = sessions.items.iter().find(|i| i.label == "live").unwrap();
        assert_eq!(alive.state, ItemState::Current);

        // The deleted project's sessions and the checkpoints, nothing else.
        assert_eq!(recs.len(), 2);
        assert!(recs.iter().all(|r| r.tier == Tier::ReviewFirst));
        assert!(recs.iter().any(|r| r.path.ends_with("projects/gone")));
        assert!(recs.iter().any(|r| r.path.ends_with("file-history")));
        assert!(!recs.iter().any(|r| r.path.ends_with("history.jsonl")));
    }

    #[test]
    fn copilot_storage_is_orphaned_only_when_the_extension_is_gone() {
        let (_d, home) = home();
        let gs = home.join("Library/Application Support/Code/User/globalStorage");
        write(&gs.join("github.copilot-chat/index.db"), 2_000_000);
        write(&gs.join("github.copilot/x.json"), 2_000_000);
        write(
            &home.join(".vscode/extensions/github.copilot-1.300.0/package.json"),
            100,
        );

        let (report, recs) = copilot(&ctx(&home));
        assert!(report.detected);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].path.ends_with("github.copilot-chat"));
        assert_eq!(recs[0].tier, Tier::ReviewFirst);
    }

    #[test]
    fn reads_cursor_chat_counts_without_writing() {
        let (_d, home) = home();
        let db = home.join("state.vscdb");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE cursorDiskKV (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
                 INSERT INTO cursorDiskKV VALUES ('composerData:a', '{\"createdAt\": 1000}');
                 INSERT INTO cursorDiskKV VALUES ('composerData:b', '{\"createdAt\": 500}');
                 INSERT INTO cursorDiskKV VALUES ('bubbleId:a:1', 'x');",
            )
            .unwrap();
        }
        let before = std::fs::read(&db).unwrap();
        let stats = cursor_chat_stats(&db).unwrap();
        assert_eq!(stats.chats, 2);
        assert_eq!(stats.oldest_ms, Some(500));
        assert_eq!(std::fs::read(&db).unwrap(), before);
        assert!(cursor_chat_stats(&home.join("missing.vscdb")).is_none());
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(group_thousands(7), "7");
        assert_eq!(group_thousands(1339), "1,339");
        assert_eq!(group_thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn absent_agents_are_not_detected() {
        let (_d, home) = home();
        let c = ctx(&home);
        for (report, recs) in [claude_desktop(&c), codex(&c), copilot(&c)] {
            assert!(!report.detected, "{}", report.id);
            assert!(recs.is_empty());
        }
    }
}
