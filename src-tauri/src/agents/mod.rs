//! What each AI coding agent on this Mac is allowed to do and what it has
//! installed: permission rules, MCP connections, plugins and skills, plus
//! the data it leaves behind that can be cleaned up.
//!
//! Reading never changes anything. The only changes this module makes are
//! the ones in [`edit`]: disconnecting or reconnecting an MCP server,
//! turning a plugin on or off and removing a permission rule. Each one
//! rewrites a single config file atomically after saving a backup. Skills
//! and caches are never deleted here; they become recommendations and go
//! through the regular review, safety gate and Trash like everything else.
//!
//! Secrets stay out of the report: environment variables, headers and
//! command arguments of MCP servers are never read into it, and passwords
//! inside URLs or permission rules are masked.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod cursor;
pub mod edit;
pub mod skills;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::devtools::{Ctx, ToolItem, ToolReport};
use crate::model::Recommendation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    /// A setting that decides how much the agent may do without asking.
    Mode,
    Allow,
    Ask,
    Deny,
    /// A folder or project the agent trusts.
    Trust,
    /// Commands the agent runs by itself (hooks).
    Hook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    /// Stable id used to remove the rule; `None` when it cannot be removed.
    pub id: Option<String>,
    pub kind: PermissionKind,
    pub label: String,
    pub value: String,
    /// The file it comes from, with `~` for the home folder.
    pub source: String,
    /// Why this rule or setting deserves a look, if it does.
    pub risk: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    /// "Local command" or "Remote".
    pub transport: String,
    /// The program or host it talks to, without arguments or credentials.
    pub target: String,
    /// "All projects" or the project it belongs to.
    pub scope: String,
    pub enabled: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub id: String,
    pub name: String,
    /// Marketplace or publisher.
    pub publisher: Option<String>,
    pub version: Option<String>,
    /// `None` when DiskPilot cannot tell or change it.
    pub enabled: Option<bool>,
    pub bytes: u64,
    pub last_used_ms: Option<i64>,
    pub path: Option<String>,
    /// "plugin" or "extension".
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillOrigin {
    /// Installed by the user; can be moved to the Trash.
    User,
    /// Comes with a plugin; removed with the plugin.
    Plugin,
    /// Ships with the agent.
    BuiltIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillState {
    Active,
    /// Not used for [`skills::INACTIVE_DAYS`] days, or never used.
    Inactive,
    /// The agent keeps no record DiskPilot can read.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub bytes: u64,
    pub origin: SkillOrigin,
    /// The plugin it comes with, for plugin skills.
    pub plugin: Option<String>,
    pub last_used_ms: Option<i64>,
    pub uses: Option<u64>,
    pub state: SkillState,
    pub recommendation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReport {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub version: Option<String>,
    pub permissions: Vec<Permission>,
    pub connections: Vec<Connection>,
    pub plugins: Vec<Plugin>,
    pub skills: Vec<Skill>,
    /// Data that can be cleaned up, each item backed by a recommendation.
    pub cleanup: Vec<ToolItem>,
    /// How skill use is known: a sentence for the UI.
    pub usage_note: String,
    pub total_bytes: u64,
}

/// An MCP server DiskPilot disconnected and can put back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disconnected {
    pub id: String,
    pub agent: String,
    pub name: String,
    pub scope: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsReport {
    pub generated_at_ms: i64,
    pub agents: Vec<AgentReport>,
    pub disconnected: Vec<Disconnected>,
}

/// Inputs for one analysis.
pub struct Input<'a> {
    pub ctx: &'a Ctx<'a>,
    /// DiskPilot's own data folder (backups, disconnected servers).
    pub data_dir: Option<&'a Path>,
    /// The Developer Tools analysis, when it has run. Its per-tool reports
    /// are reused instead of measuring the same folders again; their
    /// recommendations are already registered.
    pub tools: &'a [ToolReport],
}

/// An existing tool report from Developer Tools, or a fresh one.
pub fn tool_report(
    input: &Input,
    id: &str,
    compute: fn(&Ctx) -> (ToolReport, Vec<Recommendation>),
) -> (ToolReport, Vec<Recommendation>) {
    match input.tools.iter().find(|t| t.id == id) {
        Some(t) => (t.clone(), Vec::new()),
        None => compute(input.ctx),
    }
}

pub fn analyze(input: &Input) -> (AgentsReport, Vec<Recommendation>) {
    type Analyzer = fn(&Input) -> (AgentReport, Vec<Recommendation>);
    let analyzers: [Analyzer; 4] = [
        claude::analyze,
        codex::analyze,
        cursor::analyze,
        antigravity::analyze,
    ];
    // Each agent reads its own files; run them side by side.
    let results: Vec<(AgentReport, Vec<Recommendation>)> = std::thread::scope(|s| {
        let handles: Vec<_> = analyzers
            .iter()
            .map(|f| s.spawn(move || f(input)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("agent analyzer panicked"))
            .collect()
    });
    let mut agents = Vec::new();
    let mut recs = Vec::new();
    for (a, r) in results {
        agents.push(a);
        recs.extend(r);
    }
    let disconnected = input
        .data_dir
        .map(edit::stashed)
        .unwrap_or_default()
        .into_iter()
        .map(|s| Disconnected {
            id: s.id,
            agent: s.agent,
            name: s.name,
            scope: s.scope,
            at_ms: s.at_ms,
        })
        .collect();
    (
        AgentsReport {
            generated_at_ms: input.ctx.now_ms,
            agents,
            disconnected,
        },
        recs,
    )
}

/// Cleanup items from an existing tool report: everything it recommends.
pub fn cleanup_items(report: &ToolReport) -> Vec<ToolItem> {
    let mut items: Vec<ToolItem> = report
        .sections
        .iter()
        .flat_map(|s| s.items.iter())
        .filter(|i| i.recommendation_id.is_some())
        .cloned()
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    items
}

/// `path` with `~` for the home folder, for display.
pub fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Mask passwords in `scheme://user:password@host` and values of
/// `KEY=value` / `--token value` style secrets inside free text.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // user:password@ inside URLs.
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')')
            .unwrap_or(tail.len());
        let authority_end = tail[..end].find('/').unwrap_or(end);
        let authority = &tail[..authority_end];
        if let Some(at) = authority.rfind('@') {
            let creds = &authority[..at];
            match creds.find(':') {
                Some(c) => {
                    out.push_str(&creds[..c]);
                    out.push_str(":•••");
                }
                None => out.push_str("•••"),
            }
            out.push_str(&tail[at..authority_end]);
            rest = &tail[authority_end..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    redact_assignments(&out)
}

const SECRET_WORDS: &[&str] = &[
    "key",
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "auth",
    "bearer",
    "credential",
];

fn looks_secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SECRET_WORDS.iter().any(|w| n.contains(w))
}

/// `API_KEY=abc` → `API_KEY=•••`; `--token abc` → `--token •••`.
fn redact_assignments(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut mask_next = false;
    for word in text.split(' ') {
        if mask_next && !word.is_empty() {
            out.push("•••".into());
            mask_next = false;
            continue;
        }
        if let Some((name, _)) = word.split_once('=') {
            if looks_secret_name(name.trim_start_matches('-')) {
                out.push(format!("{name}=•••"));
                continue;
            }
        }
        if word.starts_with("--") && looks_secret_name(word) {
            mask_next = true;
        }
        out.push(word.to_string());
    }
    out.join(" ")
}

/// What an MCP server talks to: the host of a URL, or the program name plus
/// the package it runs (for `npx`/`uvx`-style launchers). Never arguments
/// that could hold credentials.
pub fn connection_target(command: Option<&str>, args: &[String], url: Option<&str>) -> String {
    if let Some(url) = url {
        let after = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
        let host = after
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('@')
            .next()
            .unwrap_or("");
        return host.to_string();
    }
    let Some(command) = command else {
        return String::new();
    };
    let program = Path::new(command)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| command.to_string());
    const LAUNCHERS: &[&str] = &[
        "npx", "uvx", "bunx", "pnpx", "pipx", "docker", "node", "python3", "python", "deno", "bun",
        "uv",
    ];
    if LAUNCHERS.contains(&program.as_str()) {
        let package = args.iter().find(|a| {
            !a.starts_with('-')
                && !matches!(a.as_str(), "run" | "exec" | "dlx" | "x" | "tool")
                && a.len() <= 80
                && !a.contains('=')
                && a.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "@/._-:".contains(c))
        });
        if let Some(p) = package {
            // A path to a script shows only its file name.
            let shown = if p.starts_with('/') || p.starts_with('.') {
                Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                p.clone()
            };
            return format!("{program} {shown}");
        }
    }
    program
}

/// Add a cleanup row for each skill that has a recommendation.
pub fn push_skill_cleanup(cleanup: &mut Vec<ToolItem>, skills: &[Skill]) {
    for s in skills {
        if let Some(id) = &s.recommendation_id {
            cleanup.push(ToolItem {
                label: format!("Skill · {}", s.name),
                path: Some(s.path.clone()),
                bytes: s.bytes,
                detail: "Inactive skill".into(),
                state: crate::devtools::ItemState::Stale,
                last_used_ms: s.last_used_ms,
                recommendation_id: Some(id.clone()),
            });
        }
    }
    cleanup.sort_by_key(|i| std::cmp::Reverse(i.bytes));
}

/// Parse JSON that may contain `//` and `/* */` comments and trailing
/// commas, as editor settings files do. Only for reading.
pub fn parse_jsonc(text: &str) -> Option<serde_json::Value> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let (mut i, mut in_str) = (0, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
        } else if c == b'"' {
            in_str = true;
            out.push(c);
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else if c == b',' {
            let next = b[i + 1..].iter().find(|x| !x.is_ascii_whitespace());
            if !matches!(next, Some(b'}') | Some(b']')) {
                out.push(c);
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    serde_json::from_slice(&out).ok()
}

/// MCP servers in a `{"mcpServers": {...}}` file (Cursor, Antigravity).
pub fn json_mcp_connections(agent: &str, path: &Path, home: &Path, scope: &str) -> Vec<Connection> {
    let Some(doc) =
        crate::devtools::read_small(path, 8 * 1024 * 1024).and_then(|t| parse_jsonc(&t))
    else {
        return Vec::new();
    };
    let Some(servers) = doc.get("mcpServers").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let source = tilde(path, home);
    servers
        .iter()
        .map(|(name, v)| {
            let url = v
                .get("url")
                .or_else(|| v.get("serverUrl"))
                .or_else(|| v.get("httpUrl"))
                .and_then(serde_json::Value::as_str);
            let args: Vec<String> = v
                .get("args")
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            Connection {
                id: format!("{agent}|{}|{name}", path.display()),
                name: name.clone(),
                transport: if url.is_some() {
                    "Remote"
                } else {
                    "Local command"
                }
                .into(),
                target: connection_target(
                    v.get("command").and_then(serde_json::Value::as_str),
                    &args,
                    url,
                ),
                scope: scope.into(),
                enabled: v.get("disabled").and_then(serde_json::Value::as_bool) != Some(true),
                source: source.clone(),
            }
        })
        .collect()
}

/// Disconnect a server listed by [`json_mcp_connections`], but only from
/// one of the agent's own MCP files.
pub fn disconnect_json_mcp(
    input: &Input,
    agent: &str,
    files: &[PathBuf],
    id: &str,
) -> Result<(), crate::error::AppError> {
    for file in files {
        let prefix = format!("{agent}|{}|", file.display());
        if let Some(name) = id.strip_prefix(&prefix) {
            let scope = json_mcp_connections(agent, file, input.ctx.home, "")
                .into_iter()
                .find(|c| c.name == name)
                .map(|_| "All projects".to_string())
                .ok_or_else(|| {
                    crate::error::AppError::Other(format!("{name} is no longer configured."))
                })?;
            return edit::disconnect_json(
                input.data_dir,
                agent,
                file,
                &["mcpServers".into()],
                name,
                &scope,
                input.ctx.now_ms,
            );
        }
    }
    Err(crate::error::AppError::Other("Unknown connection.".into()))
}

/// Extensions in a VS Code-style folder, from its `extensions.json`.
pub fn editor_extensions(ctx: &Ctx, dir: &Path) -> Vec<Plugin> {
    let Some(list) = crate::devtools::read_small(&dir.join("extensions.json"), 16 * 1024 * 1024)
        .and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok())
    else {
        return Vec::new();
    };
    let mut out: Vec<Plugin> = list
        .iter()
        .filter_map(|e| {
            let id = e.pointer("/identifier/id")?.as_str()?.to_string();
            let rel = e
                .get("relativeLocation")
                .and_then(serde_json::Value::as_str)?;
            // Never follow a registry entry outside the extensions folder.
            if rel.contains('/') || rel.starts_with('.') {
                return None;
            }
            let path = dir.join(rel);
            let installed = e
                .pointer("/metadata/installedTimestamp")
                .and_then(serde_json::Value::as_i64);
            Some(Plugin {
                name: id.split('.').nth(1).unwrap_or(&id).to_string(),
                publisher: e
                    .pointer("/metadata/publisherDisplayName")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .or_else(|| id.split('.').next().map(str::to_string)),
                version: e
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                enabled: None,
                bytes: if crate::devtools::exists(&path) {
                    ctx.size(&path)
                } else {
                    0
                },
                last_used_ms: None,
                path: Some(path.display().to_string()),
                kind: "extension".into(),
                id: format!("{id}|{}", installed.unwrap_or(0)),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_in_urls_and_secret_flags_are_masked() {
        assert_eq!(
            redact("Bash(E2E_DATABASE_URL=\"postgres://app:hunter2@127.0.0.1:5433/db\" npx playwright test)"),
            "Bash(E2E_DATABASE_URL=\"postgres://app:•••@127.0.0.1:5433/db\" npx playwright test)"
        );
        assert_eq!(
            redact("Bash(OPENAI_API_KEY=sk-123 run)"),
            "Bash(OPENAI_API_KEY=••• run)"
        );
        assert_eq!(
            redact("tool --token abc --verbose"),
            "tool --token ••• --verbose"
        );
        assert_eq!(redact("Bash(git push *)"), "Bash(git push *)");
        assert_eq!(redact("https://example.com/a@b"), "https://example.com/a@b");
    }

    #[test]
    fn targets_never_include_arguments_or_credentials() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            connection_target(
                Some("npx"),
                &args(&["-y", "@modelcontextprotocol/server-github", "--token", "x"]),
                None
            ),
            "npx @modelcontextprotocol/server-github"
        );
        assert_eq!(
            connection_target(
                Some("/usr/local/bin/my-server"),
                &args(&["--api-key", "secret"]),
                None
            ),
            "my-server"
        );
        assert_eq!(
            connection_target(None, &[], Some("https://user:pw@mcp.example.com/sse?key=1")),
            "mcp.example.com"
        );
        assert_eq!(
            connection_target(Some("node"), &args(&["/Users/me/secret/server.js"]), None),
            "node server.js"
        );
    }
}
