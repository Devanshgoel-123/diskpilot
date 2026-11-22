//! Claude Code: `~/.claude/settings*.json` (permissions, hooks, plugins),
//! `~/.claude.json` (MCP servers, skill and plugin use) and
//! `~/.claude/plugins` (installed plugins and their cached versions).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::skills::{self, Usage};
use super::{
    cleanup_items, connection_target, redact, tilde, AgentReport, Connection, Input, Permission,
    PermissionKind, Plugin, SkillOrigin,
};
use crate::devtools::{agent_data, exists, modified_ms, read_small, Ctx, RecSpec};
use crate::error::AppError;
use crate::model::{CleanupMethod, MarkerKind, Recommendation, Tier};

pub const ID: &str = "claude-code";
const NAME: &str = "Claude Code";
const SETTINGS: &[&str] = &[".claude/settings.json", ".claude/settings.local.json"];
const STATE: &str = ".claude.json";
/// `~/.claude.json` is rewritten often and can be large.
const MAX_STATE: u64 = 64 * 1024 * 1024;

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&read_small(path, MAX_STATE)?).ok()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Why an allow rule deserves a look, if it does.
pub fn rule_risk(rule: &str) -> Option<String> {
    let r = rule.trim();
    let tool = r.split('(').next().unwrap_or(r);
    let inner = r
        .strip_prefix(tool)
        .and_then(|s| s.strip_prefix('('))
        .and_then(|s| s.strip_suffix(')'))
        .map(str::trim);
    let any = inner.is_none() || matches!(inner, Some("*" | "**" | ":*" | "*:*"));
    match tool {
        "Bash" if any => Some("Runs any shell command without asking.".into()),
        "Bash" => {
            let cmd = inner.unwrap_or("");
            let first = cmd.split_whitespace().next().unwrap_or("");
            if matches!(first, "rm" | "sudo" | "dd" | "chmod" | "chown" | "mv")
                || cmd.contains("git push")
                || cmd.contains("--force")
                || cmd.contains("curl") && cmd.contains('|')
            {
                Some("Can change or delete things without asking.".into())
            } else {
                None
            }
        }
        "Write" | "Edit" | "MultiEdit" if any => Some("Edits any file without asking.".into()),
        "WebFetch" if any => Some("Fetches any website without asking.".into()),
        _ => None,
    }
}

fn permissions(ctx: &Ctx, out: &mut Vec<Permission>) {
    for rel in SETTINGS {
        let path = ctx.home_path(rel);
        let Some(doc) = read_json(&path) else {
            continue;
        };
        let source = tilde(&path, ctx.home);
        let perms = doc.get("permissions");
        if let Some(mode) = perms
            .and_then(|p| p.get("defaultMode"))
            .and_then(Value::as_str)
        {
            out.push(Permission {
                id: None,
                kind: PermissionKind::Mode,
                label: "Default mode".into(),
                value: mode.into(),
                source: source.clone(),
                risk: (mode == "bypassPermissions")
                    .then(|| "Every tool runs without asking.".to_string()),
            });
        }
        if doc
            .get("skipDangerousModePermissionPrompt")
            .and_then(Value::as_bool)
            == Some(true)
        {
            out.push(Permission {
                id: None,
                kind: PermissionKind::Mode,
                label: "Skip-permissions warning".into(),
                value: "Turned off".into(),
                source: source.clone(),
                risk: Some("Bypass mode can be turned on without a warning.".into()),
            });
        }
        for (key, kind) in [
            ("allow", PermissionKind::Allow),
            ("ask", PermissionKind::Ask),
            ("deny", PermissionKind::Deny),
        ] {
            for rule in strings(perms.and_then(|p| p.get(key))) {
                out.push(Permission {
                    id: Some(format!("{ID}|{rel}|{key}|{rule}")),
                    kind,
                    label: rule_label(&rule),
                    value: redact(&rule),
                    source: source.clone(),
                    risk: (kind == PermissionKind::Allow)
                        .then(|| rule_risk(&rule))
                        .flatten(),
                });
            }
        }
        for dir in strings(perms.and_then(|p| p.get("additionalDirectories"))) {
            out.push(Permission {
                id: None,
                kind: PermissionKind::Trust,
                label: "Extra folder".into(),
                value: dir,
                source: source.clone(),
                risk: None,
            });
        }
        if let Some(hooks) = doc.get("hooks").and_then(Value::as_object) {
            for (event, list) in hooks {
                let n = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|m| m.get("hooks").and_then(Value::as_array).map_or(1, Vec::len))
                            .sum::<usize>()
                    })
                    .unwrap_or(0);
                if n > 0 {
                    out.push(Permission {
                        id: None,
                        kind: PermissionKind::Hook,
                        label: format!("{event} hook"),
                        value: format!(
                            "{n} command{} run automatically",
                            if n == 1 { "" } else { "s" }
                        ),
                        source: source.clone(),
                        risk: None,
                    });
                }
            }
        }
    }
}

/// `Bash(git add *)` → `Bash`; `mcp__github__create_issue` → `github`.
fn rule_label(rule: &str) -> String {
    let tool = rule.split('(').next().unwrap_or(rule);
    if let Some(rest) = tool.strip_prefix("mcp__") {
        return format!("MCP · {}", rest.split("__").next().unwrap_or(rest));
    }
    tool.to_string()
}

fn server_connection(
    name: &str,
    v: &Value,
    scope: &str,
    enabled: bool,
    id: String,
    source: &str,
) -> Connection {
    let url = v.get("url").and_then(Value::as_str);
    let args = strings(v.get("args"));
    let transport = match v.get("type").and_then(Value::as_str) {
        Some("http" | "sse") => "Remote",
        Some(_) => "Local command",
        None if url.is_some() => "Remote",
        None => "Local command",
    };
    Connection {
        id,
        name: name.into(),
        transport: transport.into(),
        target: connection_target(v.get("command").and_then(Value::as_str), &args, url),
        scope: scope.into(),
        enabled,
        source: source.into(),
    }
}

fn connections(ctx: &Ctx, state: Option<&Value>, out: &mut Vec<Connection>) {
    let Some(state) = state else { return };
    let source = tilde(&ctx.home_path(STATE), ctx.home);
    if let Some(servers) = state.get("mcpServers").and_then(Value::as_object) {
        for (name, v) in servers {
            out.push(server_connection(
                name,
                v,
                "All projects",
                true,
                format!("{ID}|user|{name}"),
                &source,
            ));
        }
    }
    if let Some(projects) = state.get("projects").and_then(Value::as_object) {
        for (project, p) in projects {
            let disabled: HashSet<String> =
                strings(p.get("disabledMcpServers")).into_iter().collect();
            let Some(servers) = p.get("mcpServers").and_then(Value::as_object) else {
                continue;
            };
            for (name, v) in servers {
                out.push(server_connection(
                    name,
                    v,
                    &tilde(Path::new(project), ctx.home),
                    !disabled.contains(name),
                    format!("{ID}|project|{project}|{name}"),
                    &source,
                ));
            }
        }
    }
}

fn usage_map(state: Option<&Value>, key: &str) -> HashMap<String, Usage> {
    state
        .and_then(|s| s.get(key))
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        Usage {
                            count: v.get("usageCount").and_then(Value::as_u64).unwrap_or(0),
                            last_ms: v.get("lastUsedAt").and_then(Value::as_i64).unwrap_or(0),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

struct Installed {
    id: String,
    name: String,
    marketplace: Option<String>,
    version: Option<String>,
    path: Option<PathBuf>,
}

fn installed_plugins(ctx: &Ctx) -> Vec<Installed> {
    let Some(doc) = read_json(&ctx.home_path(".claude/plugins/installed_plugins.json")) else {
        return Vec::new();
    };
    let Some(map) = doc.get("plugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    map.iter()
        .map(|(id, entries)| {
            let e = entries.as_array().and_then(|a| a.first());
            let (name, marketplace) = match id.split_once('@') {
                Some((n, m)) => (n.to_string(), Some(m.to_string())),
                None => (id.clone(), None),
            };
            Installed {
                id: id.clone(),
                name,
                marketplace,
                version: e
                    .and_then(|e| e.get("version"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                path: e
                    .and_then(|e| e.get("installPath"))
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    // Only trust install paths inside the plugin cache.
                    .filter(|p| p.starts_with(ctx.home_path(".claude/plugins"))),
            }
        })
        .collect()
}

/// Plugin versions in `~/.claude/plugins/cache/<marketplace>/<plugin>/` that
/// no installed plugin points to: older versions and leftovers of updates.
fn stale_plugin_versions(
    ctx: &Ctx,
    installed: &[Installed],
    recs: &mut Vec<Recommendation>,
) -> Vec<crate::devtools::ToolItem> {
    use crate::devtools::{subdirs, ItemState, ToolItem};
    let in_use: HashSet<&Path> = installed.iter().filter_map(|p| p.path.as_deref()).collect();
    let installed_ids: HashSet<(String, String)> = installed
        .iter()
        .filter_map(|p| Some((p.marketplace.clone()?, p.name.clone())))
        .collect();
    let cache = ctx.home_path(".claude/plugins/cache");
    let mut items = Vec::new();
    for market in subdirs(&cache) {
        for plugin in subdirs(&market) {
            let key = (
                crate::devtools::name_of(&market),
                crate::devtools::name_of(&plugin),
            );
            let plugin_installed = installed_ids.contains(&key);
            for version in subdirs(&plugin) {
                if in_use.contains(version.as_path()) {
                    continue;
                }
                let bytes = ctx.size(&version);
                if bytes == 0 {
                    continue;
                }
                let label = format!("{} · {}", key.1, crate::devtools::name_of(&version));
                let why = if plugin_installed {
                    "A version of this plugin that is no longer the installed one. Claude Code keeps old versions after updating."
                } else {
                    "Files of a plugin that is no longer installed."
                };
                let rec = RecSpec {
                    source: ID,
                    title: format!("Unused Claude Code plugin files · {label}"),
                    path: &version,
                    bytes,
                    tier: Tier::LikelySafe,
                    methods: &[CleanupMethod::Trash],
                    detected_because: why.to_string(),
                    safe_because: "Claude Code loads plugins from the path in installed_plugins.json, which points elsewhere.",
                    risks: "None for the installed plugins. If this version is needed again, Claude Code downloads it.",
                    proposed_action: "Move the folder to the Trash",
                    alternative: None,
                    last_activity_ms: modified_ms(&version),
                    notes: vec!["Close running Claude Code sessions first.".into()],
                }
                .build(ctx)
                .marked(
                    if plugin_installed { MarkerKind::OlderVersion } else { MarkerKind::Leftover },
                    if plugin_installed { "A newer version is installed" } else { "Plugin not installed" },
                );
                items.push(ToolItem {
                    label,
                    path: Some(version.display().to_string()),
                    bytes,
                    detail: why.into(),
                    state: if plugin_installed {
                        ItemState::Superseded
                    } else {
                        ItemState::Orphaned
                    },
                    last_used_ms: modified_ms(&version),
                    recommendation_id: Some(rec.id.clone()),
                });
                recs.push(rec);
            }
        }
    }
    items
}

pub fn analyze(input: &Input) -> (AgentReport, Vec<Recommendation>) {
    let ctx = input.ctx;
    let state = read_json(&ctx.home_path(STATE));
    let settings = read_json(&ctx.home_path(SETTINGS[0]));
    let detected = exists(&ctx.home_path(".claude")) || state.is_some();

    let mut permissions_out = Vec::new();
    permissions(ctx, &mut permissions_out);
    let mut connections_out = Vec::new();
    connections(ctx, state.as_ref(), &mut connections_out);

    let skill_usage = usage_map(state.as_ref(), "skillUsage");
    let plugin_usage = usage_map(state.as_ref(), "pluginUsage");
    let usage_known = state
        .as_ref()
        .is_some_and(|s| s.get("skillUsage").is_some());

    // Plugins.
    let installed = installed_plugins(ctx);
    let enabled_map = settings
        .as_ref()
        .and_then(|s| s.get("enabledPlugins"))
        .and_then(Value::as_object);
    let mut plugins = Vec::new();
    for p in &installed {
        let prefix = format!("{}:", p.name);
        let skill_last = skill_usage
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(_, u)| u.last_ms)
            .max();
        let last = plugin_usage
            .get(&p.id)
            .map(|u| u.last_ms)
            .into_iter()
            .chain(skill_last)
            .filter(|&ms| ms > 0)
            .max();
        plugins.push(Plugin {
            id: p.id.clone(),
            name: p.name.clone(),
            publisher: p.marketplace.clone(),
            version: p.version.clone(),
            enabled: Some(
                enabled_map
                    .and_then(|m| m.get(&p.id))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            bytes: p.path.as_deref().map_or(0, |path| ctx.size(path)),
            last_used_ms: last,
            path: p.path.as_ref().map(|x| x.display().to_string()),
            kind: "plugin".into(),
        });
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));

    // Skills: the user's own, then each installed plugin's.
    let mut found = skills::find(&ctx.home_path(".claude/skills"), SkillOrigin::User, None);
    for p in &installed {
        if let Some(path) = &p.path {
            found.extend(skills::find(
                &path.join("skills"),
                SkillOrigin::Plugin,
                Some(&p.name),
            ));
        }
    }
    let (skills_out, mut recs) = skills::rows(
        ctx,
        ID,
        NAME,
        found,
        &skill_usage,
        usage_known,
        |f| match &f.plugin {
            Some(plugin) => format!("{plugin}:{}", f.name),
            None => f.name.clone(),
        },
    );

    // Cleanup: caches, logs, old versions, data for deleted projects, plus
    // unused plugin versions and inactive skills.
    let (tool, tool_recs) = super::tool_report(input, ID, agent_data::claude_code);
    let mut cleanup = cleanup_items(&tool);
    cleanup.extend(stale_plugin_versions(ctx, &installed, &mut recs));
    super::push_skill_cleanup(&mut cleanup, &skills_out);
    recs.extend(tool_recs);

    (
        AgentReport {
            id: ID.into(),
            name: NAME.into(),
            detected,
            version: tool.version.clone(),
            permissions: permissions_out,
            connections: connections_out,
            plugins,
            skills: skills_out,
            cleanup,
            usage_note: if usage_known {
                "Skill use comes from Claude Code's own record.".into()
            } else {
                "Claude Code has not recorded any skill use yet.".into()
            },
            total_bytes: tool.total_bytes,
        },
        recs,
    )
}

// ---- Changes -------------------------------------------------------------------

pub fn disconnect(input: &Input, id: &str) -> Result<(), AppError> {
    let ctx = input.ctx;
    let parts: Vec<&str> = id.split('|').collect();
    let (parent, name, scope): (Vec<String>, &str, String) = match parts.as_slice() {
        [ID, "user", name] => (vec!["mcpServers".into()], name, "All projects".into()),
        [ID, "project", project, name] => (
            vec!["projects".into(), (*project).into(), "mcpServers".into()],
            name,
            tilde(Path::new(project), ctx.home),
        ),
        _ => return Err(AppError::Other("Unknown connection.".into())),
    };
    super::edit::disconnect_json(
        input.data_dir,
        ID,
        &ctx.home_path(STATE),
        &parent,
        name,
        &scope,
        ctx.now_ms,
    )
}

pub fn set_plugin(input: &Input, plugin_id: &str, enabled: bool) -> Result<(), AppError> {
    let ctx = input.ctx;
    if !installed_plugins(ctx).iter().any(|p| p.id == plugin_id) {
        return Err(AppError::Other("That plugin is not installed.".into()));
    }
    super::edit::edit_json(&ctx.home_path(SETTINGS[0]), input.data_dir, ID, &|doc| {
        let obj = doc
            .as_object_mut()
            .ok_or_else(|| AppError::Other("settings.json is not a JSON object.".into()))?;
        let map = obj
            .entry("enabledPlugins")
            .or_insert_with(|| Value::Object(Default::default()));
        let map = map
            .as_object_mut()
            .ok_or_else(|| AppError::Other("enabledPlugins is not an object.".into()))?;
        if map.get(plugin_id).and_then(Value::as_bool) == Some(enabled) {
            return Ok(false);
        }
        map.insert(plugin_id.to_string(), Value::Bool(enabled));
        Ok(true)
    })
    .map(|_| ())
}

/// Remove permission rules by id, one edit (and one backup) per file.
pub fn remove_permissions(input: &Input, ids: &[String]) -> Result<(), AppError> {
    let ctx = input.ctx;
    let mut by_file: std::collections::BTreeMap<&str, Vec<(&str, &str)>> = Default::default();
    for id in ids {
        let mut parts = id.splitn(4, '|');
        let (Some(ID), Some(rel), Some(key), Some(rule)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(AppError::Other("Unknown rule.".into()));
        };
        let Some(rel) = SETTINGS.iter().find(|s| **s == rel) else {
            return Err(AppError::Other("Unknown rule.".into()));
        };
        if !matches!(key, "allow" | "ask" | "deny") {
            return Err(AppError::Other("Unknown rule.".into()));
        }
        by_file.entry(rel).or_default().push((key, rule));
    }
    let mut changed = false;
    for (rel, rules) in by_file {
        changed |= super::edit::edit_json(&ctx.home_path(rel), input.data_dir, ID, &|doc| {
            let Some(perms) = doc.get_mut("permissions").and_then(Value::as_object_mut) else {
                return Ok(false);
            };
            let mut any = false;
            for (key, rule) in &rules {
                if let Some(list) = perms.get_mut(*key).and_then(Value::as_array_mut) {
                    let before = list.len();
                    list.retain(|v| v.as_str() != Some(rule));
                    any |= list.len() != before;
                }
            }
            Ok(any)
        })?;
    }
    if !changed {
        return Err(AppError::Other(
            "Those rules are no longer in the settings.".into(),
        ));
    }
    Ok(())
}

pub fn remove_permission(input: &Input, id: &str) -> Result<(), AppError> {
    remove_permissions(input, &[id.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devtools::tests::ctx;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn broad_allow_rules_are_flagged() {
        assert!(rule_risk("Bash").is_some());
        assert!(rule_risk("Bash(*)").is_some());
        assert!(rule_risk("Bash(rm -rf build)").is_some());
        assert!(rule_risk("Bash(git push origin main)").is_some());
        assert!(rule_risk("Bash(git add *)").is_none());
        assert!(rule_risk("Edit").is_some());
        assert!(rule_risk("WebFetch(domain:docs.rs)").is_none());
        assert!(rule_risk("Read").is_none());
    }

    #[test]
    fn reads_permissions_servers_plugins_and_skill_use() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let now = ctx(&home).now_ms;
        let day = crate::devtools::DAY_MS;
        let plugin_dir = home.join(".claude/plugins/cache/mk/tools/1.2.0");
        write(
            &home.join(".claude/settings.json"),
            r#"{"permissions":{"allow":["Bash(*)","Read"],"deny":["Bash(sudo *)"],"defaultMode":"acceptEdits"},
                "enabledPlugins":{"tools@mk":true},
                "hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"x"}]}]}}"#,
        );
        write(
            &home.join(".claude/plugins/installed_plugins.json"),
            &format!(
                r#"{{"version":2,"plugins":{{"tools@mk":[{{"installPath":{},"version":"1.2.0"}}]}}}}"#,
                // Escaped, so a Windows path's backslashes stay valid JSON.
                Value::from(plugin_dir.display().to_string())
            ),
        );
        write(
            &plugin_dir.join("skills/lint/SKILL.md"),
            "---\nname: lint\ndescription: Lints.\n---\n",
        );
        write(
            &home.join(".claude/plugins/cache/mk/tools/1.1.0/big.bin"),
            &"x".repeat(2000),
        );
        write(
            &home.join(".claude/skills/old-skill/SKILL.md"),
            "---\nname: old-skill\ndescription: Old.\n---\n",
        );
        write(
            &home.join(".claude/skills/fresh/SKILL.md"),
            "---\nname: fresh\ndescription: New.\n---\n",
        );
        // Installed long ago.
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(90 * 86_400);
        for s in ["old-skill", "fresh"] {
            let f = std::fs::File::options()
                .write(true)
                .open(home.join(format!(".claude/skills/{s}/SKILL.md")))
                .unwrap();
            f.set_modified(long_ago).unwrap();
        }
        write(
            &home.join(".claude.json"),
            &format!(
                r#"{{"mcpServers":{{"gh":{{"type":"stdio","command":"npx","args":["-y","@x/gh"],"env":{{"GITHUB_TOKEN":"ghp_secret"}}}}}},
                    "projects":{{"/p":{{"mcpServers":{{"db":{{"type":"http","url":"https://u:pw@db.example.com/mcp"}}}},"disabledMcpServers":["db"]}}}},
                    "skillUsage":{{"fresh":{{"usageCount":4,"lastUsedAt":{}}},"old-skill":{{"usageCount":1,"lastUsedAt":{}}},"tools:lint":{{"usageCount":2,"lastUsedAt":{}}}}}}}"#,
                now - day,
                now - 60 * day,
                now - 2 * day
            ),
        );

        let c = ctx(&home);
        let (report, recs) = analyze(&Input {
            ctx: &c,
            data_dir: None,
            tools: &[],
        });
        assert!(report.detected);

        let allow: Vec<_> = report
            .permissions
            .iter()
            .filter(|p| p.kind == PermissionKind::Allow)
            .collect();
        assert_eq!(allow.len(), 2);
        assert!(allow
            .iter()
            .find(|p| p.value == "Bash(*)")
            .unwrap()
            .risk
            .is_some());
        assert!(report
            .permissions
            .iter()
            .any(|p| p.kind == PermissionKind::Deny));
        assert!(report
            .permissions
            .iter()
            .any(|p| p.kind == PermissionKind::Hook));

        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("ghp_secret"),
            "env values never reach the report"
        );
        assert!(
            !json.contains("pw@"),
            "URL passwords never reach the report"
        );
        let gh = report.connections.iter().find(|c| c.name == "gh").unwrap();
        assert_eq!(gh.target, "npx @x/gh");
        assert!(gh.enabled);
        let db = report.connections.iter().find(|c| c.name == "db").unwrap();
        assert_eq!((db.target.as_str(), db.enabled), ("db.example.com", false));

        assert_eq!(report.plugins.len(), 1);
        assert_eq!(report.plugins[0].enabled, Some(true));
        assert!(
            report.plugins[0].last_used_ms.is_some(),
            "plugin skill use counts for the plugin"
        );

        let state = |n: &str| report.skills.iter().find(|s| s.name == n).unwrap().state;
        assert_eq!(state("fresh"), super::super::SkillState::Active);
        assert_eq!(state("old-skill"), super::super::SkillState::Inactive);
        assert_eq!(state("lint"), super::super::SkillState::Active);
        let lint = report.skills.iter().find(|s| s.name == "lint").unwrap();
        assert_eq!(lint.origin, SkillOrigin::Plugin);

        // Paths use backslashes on Windows.
        let slash = |p: &str| p.replace('\\', "/");
        assert!(recs
            .iter()
            .any(|r| slash(&r.path).ends_with("skills/old-skill")));
        assert!(recs.iter().any(|r| slash(&r.path).ends_with("tools/1.1.0")));
        assert!(
            !recs.iter().any(|r| slash(&r.path).ends_with("tools/1.2.0")),
            "the installed version is kept"
        );
        assert!(
            !recs.iter().any(|r| slash(&r.path).contains("lint")),
            "plugin skills go with the plugin"
        );
    }

    #[test]
    fn changes_are_made_by_id_and_backed_up() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let data = home.join("data");
        write(
            &home.join(".claude/settings.json"),
            r#"{"permissions":{"allow":["Bash(*)","Read"]},"enabledPlugins":{"tools@mk":true}}"#,
        );
        write(
            &home.join(".claude/plugins/installed_plugins.json"),
            r#"{"plugins":{"tools@mk":[{}]}}"#,
        );
        write(
            &home.join(".claude.json"),
            r#"{"mcpServers":{"gh":{"command":"npx"}},"other":1}"#,
        );
        let c = ctx(&home);
        let input = Input {
            ctx: &c,
            data_dir: Some(&data),
            tools: &[],
        };

        remove_permission(&input, &format!("{ID}|.claude/settings.json|allow|Bash(*)")).unwrap();
        assert!(
            remove_permissions(
                &input,
                &[format!("{ID}|.claude/settings.json|allow|Bash(*)")]
            )
            .is_err(),
            "removing a rule that is gone is reported"
        );
        set_plugin(&input, "tools@mk", false).unwrap();
        assert!(set_plugin(&input, "nope@mk", false).is_err());
        disconnect(&input, &format!("{ID}|user|gh")).unwrap();
        assert!(remove_permission(&input, &format!("{ID}|../../etc/x|allow|Read")).is_err());

        let s: Value = serde_json::from_str(
            &std::fs::read_to_string(home.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(s["permissions"]["allow"], serde_json::json!(["Read"]));
        assert_eq!(s["enabledPlugins"]["tools@mk"], false);
        let st: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).unwrap())
                .unwrap();
        assert!(st["mcpServers"].as_object().unwrap().is_empty());
        assert_eq!(st["other"], 1);
        assert_eq!(super::super::edit::stashed(&data).len(), 1);
    }
}
