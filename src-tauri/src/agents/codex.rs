//! Codex: `~/.codex/config.toml` (approval and sandbox settings, trusted
//! projects, MCP servers, plugins), `~/.codex/rules` (commands you approved
//! for good) and skills in `~/.codex/skills` and `~/.agents/skills`.

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item};

use super::skills;
use super::{
    cleanup_items, connection_target, redact, tilde, AgentReport, Connection, Input, Permission,
    PermissionKind, Plugin, SkillOrigin,
};
use crate::devtools::{agent_data, exists, read_small, Ctx};
use crate::error::AppError;
use crate::model::Recommendation;

pub const ID: &str = "codex";
const NAME: &str = "Codex";
const CONFIG: &str = ".codex/config.toml";

fn read_config(ctx: &Ctx) -> Option<DocumentMut> {
    read_small(&ctx.home_path(CONFIG), 4 * 1024 * 1024)?
        .parse()
        .ok()
}

fn str_of(item: Option<&Item>) -> Option<String> {
    item.and_then(Item::as_str).map(str::to_string)
}

fn mode_risk(key: &str, value: &str) -> Option<String> {
    match (key, value) {
        ("approval_policy", "never") => Some("Codex never asks before running commands.".into()),
        ("sandbox_mode", "danger-full-access") => {
            Some("Commands run with full access to your Mac, outside the sandbox.".into())
        }
        ("network_access", "true") => Some("Sandboxed commands can reach the internet.".into()),
        _ => None,
    }
}

fn mode(label: &str, key: &str, value: String, source: &str) -> Permission {
    Permission {
        id: None,
        kind: PermissionKind::Mode,
        label: label.into(),
        risk: mode_risk(key, &value),
        value,
        source: source.into(),
    }
}

fn settings(source: &str, out: &mut Vec<Permission>, prefix: &str, table: &toml_edit::Table) {
    if let Some(item) = table.get("approval_policy") {
        let value = match item.as_str() {
            Some(s) => s.to_string(),
            None => "Custom".into(),
        };
        out.push(mode(
            &format!("{prefix}Approval"),
            "approval_policy",
            value,
            source,
        ));
    }
    if let Some(v) = str_of(table.get("sandbox_mode")) {
        out.push(mode(&format!("{prefix}Sandbox"), "sandbox_mode", v, source));
    }
    if let Some(net) = table
        .get("sandbox_workspace_write")
        .and_then(|t| t.get("network_access"))
        .and_then(Item::as_bool)
    {
        out.push(mode(
            &format!("{prefix}Network in sandbox"),
            "network_access",
            net.to_string(),
            source,
        ));
    }
}

/// `prefix_rule(pattern=["git", "push"], decision="allow")` → ("git push", "allow").
pub fn parse_rule(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if !line.starts_with("prefix_rule(") {
        return None;
    }
    let pattern_start = line.find("pattern")?;
    let open = line[pattern_start..].find('[')? + pattern_start;
    let close = line[open..].find(']')? + open;
    let words: Vec<String> = line[open + 1..close]
        .split(',')
        .map(|w| w.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect();
    let decision = line
        .find("decision")
        .and_then(|i| {
            let rest = &line[i..];
            let q = rest.find('"')?;
            let end = rest[q + 1..].find('"')?;
            Some(rest[q + 1..q + 1 + end].to_string())
        })
        .unwrap_or_else(|| "allow".into());
    (!words.is_empty()).then(|| (words.join(" "), decision))
}

fn rules(ctx: &Ctx, out: &mut Vec<Permission>) {
    let dir = ctx.home_path(".codex/rules");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rules"))
        .collect();
    files.sort();
    for file in files {
        let Some(text) = read_small(&file, 4 * 1024 * 1024) else {
            continue;
        };
        let source = tilde(&file, ctx.home);
        for line in text.lines() {
            let Some((cmd, decision)) = parse_rule(line) else {
                continue;
            };
            let kind = match decision.as_str() {
                "forbidden" | "deny" => PermissionKind::Deny,
                "prompt" => PermissionKind::Ask,
                _ => PermissionKind::Allow,
            };
            let rule = format!("Bash({cmd})");
            out.push(Permission {
                id: None,
                kind,
                label: "Command".into(),
                risk: (kind == PermissionKind::Allow)
                    .then(|| super::claude::rule_risk(&rule))
                    .flatten(),
                value: redact(&cmd),
                source: source.clone(),
            });
        }
    }
}

fn permissions(ctx: &Ctx, doc: Option<&DocumentMut>, out: &mut Vec<Permission>) {
    let source = tilde(&ctx.home_path(CONFIG), ctx.home);
    if let Some(doc) = doc {
        settings(&source, out, "", doc.as_table());
        if let Some(profiles) = doc.get("profiles").and_then(Item::as_table_like) {
            for (name, p) in profiles.iter() {
                if let Some(t) = p.as_table() {
                    settings(&source, out, &format!("Profile {name} · "), t);
                }
            }
        }
        if let Some(projects) = doc.get("projects").and_then(Item::as_table_like) {
            for (path, p) in projects.iter() {
                if let Some(level) = p.get("trust_level").and_then(Item::as_str) {
                    out.push(Permission {
                        id: None,
                        kind: PermissionKind::Trust,
                        label: format!("Project ({level})"),
                        value: tilde(Path::new(path), ctx.home),
                        source: source.clone(),
                        risk: None,
                    });
                }
            }
        }
    }
    rules(ctx, out);
}

fn connections(ctx: &Ctx, doc: Option<&DocumentMut>) -> Vec<Connection> {
    let source = tilde(&ctx.home_path(CONFIG), ctx.home);
    let Some(servers) = doc
        .and_then(|d| d.get("mcp_servers"))
        .and_then(Item::as_table_like)
    else {
        return Vec::new();
    };
    servers
        .iter()
        .map(|(name, s)| {
            let url = s.get("url").and_then(Item::as_str);
            let args: Vec<String> = s
                .get("args")
                .and_then(Item::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            Connection {
                id: format!("{ID}|{name}"),
                name: name.into(),
                transport: if url.is_some() {
                    "Remote"
                } else {
                    "Local command"
                }
                .into(),
                target: connection_target(s.get("command").and_then(Item::as_str), &args, url),
                scope: "All projects".into(),
                enabled: s.get("enabled").and_then(Item::as_bool).unwrap_or(true),
                source: source.clone(),
            }
        })
        .collect()
}

fn plugins(ctx: &Ctx, doc: Option<&DocumentMut>) -> Vec<Plugin> {
    let Some(table) = doc
        .and_then(|d| d.get("plugins"))
        .and_then(Item::as_table_like)
    else {
        return Vec::new();
    };
    let cache = ctx.home_path(".codex/plugins/cache");
    table
        .iter()
        .map(|(id, p)| {
            let (name, publisher) = match id.split_once('@') {
                Some((n, m)) => (n.to_string(), Some(m.to_string())),
                None => (id.to_string(), None),
            };
            let dir = publisher
                .as_ref()
                .map(|m| cache.join(m).join(&name))
                .filter(|d| exists(d));
            Plugin {
                id: id.into(),
                name,
                publisher,
                version: None,
                enabled: Some(p.get("enabled").and_then(Item::as_bool).unwrap_or(true)),
                bytes: dir.as_deref().map_or(0, |d| ctx.size(d)),
                last_used_ms: None,
                path: dir.map(|d| d.display().to_string()),
                kind: "plugin".into(),
            }
        })
        .collect()
}

/// Codex is installed when its app or command is, or it has been used. A
/// `~/.codex/skills` folder alone (left by a skill installer) does not count.
pub fn installed(ctx: &Ctx) -> bool {
    const BINARIES: &[&str] = &[
        "/opt/homebrew/bin/codex",
        "/usr/local/bin/codex",
        "/Applications/Codex.app",
    ];
    const HOME: &[&str] = &[
        ".local/bin/codex",
        ".npm-global/bin/codex",
        ".codex/config.toml",
        ".codex/auth.json",
        ".codex/sessions",
        "Applications/Codex.app",
    ];
    // `Path::exists` follows links, so a launcher left pointing at an
    // uninstalled Codex does not count.
    BINARIES.iter().any(|p| Path::new(p).exists()) || HOME.iter().any(|p| ctx.home_path(p).exists())
}

pub fn analyze(input: &Input) -> (AgentReport, Vec<Recommendation>) {
    let ctx = input.ctx;
    let doc = read_config(ctx);
    let mut permissions_out = Vec::new();
    permissions(ctx, doc.as_ref(), &mut permissions_out);

    let sessions = [
        ctx.home_path(".codex/sessions"),
        ctx.home_path(".codex/archived_sessions"),
    ];
    let known = sessions.iter().any(|d| exists(d));
    let usage = if known {
        skills::usage_from_sessions(&sessions, ctx.now_ms)
    } else {
        Default::default()
    };
    let mut found = skills::find(&ctx.home_path(".codex/skills"), SkillOrigin::User, None);
    found.extend(skills::find(
        &ctx.home_path(".codex/skills/.system"),
        SkillOrigin::BuiltIn,
        None,
    ));
    found.extend(skills::find(
        &ctx.home_path(".agents/skills"),
        SkillOrigin::User,
        None,
    ));
    let (skills_out, mut recs) = skills::rows(ctx, ID, NAME, found, &usage, known, |f| {
        crate::devtools::name_of(&f.dir)
    });

    let (tool, tool_recs) = super::tool_report(input, ID, agent_data::codex);
    let mut cleanup = cleanup_items(&tool);
    super::push_skill_cleanup(&mut cleanup, &skills_out);
    recs.extend(tool_recs);

    let detected = installed(ctx);
    (
        AgentReport {
            id: ID.into(),
            name: NAME.into(),
            detected,
            version: tool.version.clone(),
            permissions: permissions_out,
            connections: connections(ctx, doc.as_ref()),
            plugins: plugins(ctx, doc.as_ref()),
            skills: skills_out,
            cleanup,
            usage_note: if known {
                "Skill use is estimated from the last 120 days of Codex sessions.".into()
            } else {
                "No Codex sessions found, so skill use is unknown.".into()
            },
            total_bytes: tool.total_bytes,
        },
        recs,
    )
}

// ---- Changes -------------------------------------------------------------------

/// Codex can turn a server off without removing it (`enabled = false`), so
/// disconnecting and reconnecting only flip that setting.
pub fn set_connection(input: &Input, id: &str, enabled: bool) -> Result<(), AppError> {
    let ctx = input.ctx;
    let Some(name) = id.strip_prefix(&format!("{ID}|")) else {
        return Err(AppError::Other("Unknown connection.".into()));
    };
    super::edit::edit_toml(&ctx.home_path(CONFIG), input.data_dir, ID, &|doc| {
        let Some(server) = doc
            .get_mut("mcp_servers")
            .and_then(Item::as_table_like_mut)
            .and_then(|t| t.get_mut(name))
            .and_then(Item::as_table_like_mut)
        else {
            return Err(AppError::Other(format!(
                "{name} is no longer in config.toml."
            )));
        };
        let current = server
            .get("enabled")
            .and_then(Item::as_bool)
            .unwrap_or(true);
        if current == enabled {
            return Ok(false);
        }
        if enabled {
            server.remove("enabled");
        } else {
            server.insert("enabled", toml_edit::value(false));
        }
        Ok(true)
    })
    .map(|_| ())
}

pub fn set_plugin(input: &Input, plugin_id: &str, enabled: bool) -> Result<(), AppError> {
    let ctx = input.ctx;
    super::edit::edit_toml(&ctx.home_path(CONFIG), input.data_dir, ID, &|doc| {
        let Some(p) = doc
            .get_mut("plugins")
            .and_then(Item::as_table_like_mut)
            .and_then(|t| t.get_mut(plugin_id))
            .and_then(Item::as_table_like_mut)
        else {
            return Err(AppError::Other("That plugin is not in config.toml.".into()));
        };
        if p.get("enabled").and_then(Item::as_bool).unwrap_or(true) == enabled {
            return Ok(false);
        }
        p.insert("enabled", toml_edit::value(enabled));
        Ok(true)
    })
    .map(|_| ())
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
    fn a_skills_folder_alone_is_not_an_install() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(
            &home.join(".codex/skills/x/SKILL.md"),
            "---\nname: x\n---\n",
        );
        let c = ctx(&home);
        let input = Input {
            ctx: &c,
            data_dir: None,
            tools: &[],
        };
        if !Path::new("/opt/homebrew/bin/codex").exists()
            && !Path::new("/usr/local/bin/codex").exists()
            && !Path::new("/Applications/Codex.app").exists()
        {
            assert!(!analyze(&input).0.detected);
        }
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::os::unix::fs::symlink(home.join("gone/codex"), home.join(".local/bin/codex")).unwrap();
        if !Path::new("/opt/homebrew/bin/codex").exists()
            && !Path::new("/usr/local/bin/codex").exists()
            && !Path::new("/Applications/Codex.app").exists()
        {
            assert!(
                !analyze(&input).0.detected,
                "a dangling launcher is not an install"
            );
        }
        write(&home.join(".codex/config.toml"), "");
        assert!(analyze(&input).0.detected);
    }

    #[test]
    fn parses_approved_command_rules() {
        assert_eq!(
            parse_rule(r#"prefix_rule(pattern=["git", "push"], decision="allow")"#),
            Some(("git push".into(), "allow".into()))
        );
        assert_eq!(parse_rule("# comment"), None);
    }

    #[test]
    fn reads_settings_servers_and_flips_enabled_keeping_comments() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(
            &home.join(CONFIG),
            "# mine\napproval_policy = \"never\"\nsandbox_mode = \"danger-full-access\"\n\n[projects.\"/p\"]\ntrust_level = \"trusted\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\nargs = [\"-y\", \"docs-mcp\"]\nenv = { TOKEN = \"secret\" }\n",
        );
        write(
            &home.join(".codex/rules/default.rules"),
            "prefix_rule(pattern=[\"rm\", \"-rf\"], decision=\"allow\")\n",
        );
        let c = ctx(&home);
        let data = home.join("data");
        let input = Input {
            ctx: &c,
            data_dir: Some(&data),
            tools: &[],
        };
        let (r, _) = analyze(&input);
        assert!(r.detected);
        assert_eq!(r.permissions.iter().filter(|p| p.risk.is_some()).count(), 3);
        assert!(r
            .permissions
            .iter()
            .any(|p| p.kind == PermissionKind::Trust));
        assert_eq!(r.connections.len(), 1);
        assert_eq!(r.connections[0].target, "npx docs-mcp");
        assert!(!serde_json::to_string(&r).unwrap().contains("secret"));

        set_connection(&input, "codex|docs", false).unwrap();
        let text = std::fs::read_to_string(home.join(CONFIG)).unwrap();
        assert!(text.starts_with("# mine\n") && text.contains("enabled = false"));
        assert!(!analyze(&input).0.connections[0].enabled);
        set_connection(&input, "codex|docs", true).unwrap();
        assert!(!std::fs::read_to_string(home.join(CONFIG))
            .unwrap()
            .contains("enabled"));
        assert!(set_connection(&input, "codex|missing", false).is_err());
    }
}
