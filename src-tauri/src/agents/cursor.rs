//! Cursor: `~/.cursor/cli-config.json` (agent permissions), `~/.cursor/mcp.json`
//! (MCP servers), extensions and skills.

use std::path::PathBuf;

use serde_json::Value;

use super::skills;
use super::{
    cleanup_items, disconnect_json_mcp, editor_extensions, json_mcp_connections, redact, tilde,
    AgentReport, Input, Permission, PermissionKind, SkillOrigin,
};
use crate::devtools::{editors, exists, read_small, subdirs, Ctx};
use crate::error::AppError;
use crate::model::Recommendation;

pub const ID: &str = "cursor";
const NAME: &str = "Cursor";
const CLI_CONFIG: &str = ".cursor/cli-config.json";

fn mcp_files(ctx: &Ctx) -> Vec<PathBuf> {
    vec![ctx.home_path(".cursor/mcp.json")]
}

/// Why a Cursor allow rule deserves a look: `Shell(*)`, `Write(**)` and
/// the like, or a dangerous command.
pub fn rule_risk(rule: &str) -> Option<String> {
    let tool = rule.split('(').next().unwrap_or(rule);
    let inner = rule
        .strip_prefix(tool)
        .and_then(|s| s.strip_prefix('('))
        .and_then(|s| s.strip_suffix(')'))
        .map(str::trim);
    let any = inner.is_none() || matches!(inner, Some("*" | "**" | "**/*"));
    match tool {
        "Shell" if any => Some("Runs any shell command without asking.".into()),
        "Shell" => super::claude::rule_risk(&format!("Bash({})", inner.unwrap_or(""))),
        "Write" if any => Some("Edits any file without asking.".into()),
        _ => None,
    }
}

fn permissions(ctx: &Ctx) -> Vec<Permission> {
    let path = ctx.home_path(CLI_CONFIG);
    let Some(doc) =
        read_small(&path, 4 * 1024 * 1024).and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Vec::new();
    };
    let source = tilde(&path, ctx.home);
    let mut out = Vec::new();
    let mode = |label: &str, value: String, risk: Option<String>| Permission {
        id: None,
        kind: PermissionKind::Mode,
        label: label.into(),
        value,
        source: source.clone(),
        risk,
    };
    if let Some(m) = doc.get("approvalMode").and_then(Value::as_str) {
        let risky = matches!(
            m,
            "unrestricted" | "yolo" | "auto" | "run-everything" | "runEverything"
        );
        out.push(mode(
            "Approval",
            m.into(),
            risky.then(|| "The agent runs commands without asking.".into()),
        ));
    }
    if let Some(m) = doc.pointer("/sandbox/mode").and_then(Value::as_str) {
        let off = matches!(m, "disabled" | "off" | "none");
        out.push(mode(
            "Sandbox",
            m.into(),
            off.then(|| "Commands run outside the sandbox.".into()),
        ));
    }
    if let Some(n) = doc
        .pointer("/sandbox/networkAccess")
        .and_then(Value::as_str)
    {
        out.push(mode("Network in sandbox", n.into(), None));
    }
    for (key, kind) in [
        ("allow", PermissionKind::Allow),
        ("deny", PermissionKind::Deny),
    ] {
        let rules = doc
            .pointer(&format!("/permissions/{key}"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for rule in rules.iter().filter_map(Value::as_str) {
            out.push(Permission {
                id: Some(format!("{ID}|{key}|{rule}")),
                kind,
                label: rule.split('(').next().unwrap_or(rule).to_string(),
                value: redact(rule),
                source: source.clone(),
                risk: (kind == PermissionKind::Allow)
                    .then(|| rule_risk(rule))
                    .flatten(),
            });
        }
    }
    out
}

pub fn analyze(input: &Input) -> (AgentReport, Vec<Recommendation>) {
    let ctx = input.ctx;
    let mut connections = Vec::new();
    for f in mcp_files(ctx) {
        connections.extend(json_mcp_connections(ID, &f, ctx.home, "All projects"));
    }

    // Agent transcripts live in ~/.cursor/projects/<project>/agent-transcripts.
    let transcripts: Vec<PathBuf> = subdirs(&ctx.home_path(".cursor/projects"))
        .into_iter()
        .map(|p| p.join("agent-transcripts"))
        .filter(|p| exists(p))
        .collect();
    let known = !transcripts.is_empty();
    let usage = if known {
        skills::usage_from_sessions(&transcripts, ctx.now_ms)
    } else {
        Default::default()
    };
    let mut found = skills::find(&ctx.home_path(".cursor/skills"), SkillOrigin::User, None);
    found.extend(skills::find(
        &ctx.home_path(".cursor/skills-cursor"),
        SkillOrigin::BuiltIn,
        None,
    ));
    let (skills_out, mut recs) = skills::rows(ctx, ID, NAME, found, &usage, known, |f| {
        crate::devtools::name_of(&f.dir)
    });

    let (tool, tool_recs) = super::tool_report(input, ID, editors::cursor);
    let mut cleanup = cleanup_items(&tool);
    super::push_skill_cleanup(&mut cleanup, &skills_out);
    recs.extend(tool_recs);

    (
        AgentReport {
            id: ID.into(),
            name: NAME.into(),
            detected: tool.detected || exists(&ctx.home_path(".cursor")),
            version: tool.version.clone(),
            permissions: permissions(ctx),
            connections,
            plugins: editor_extensions(ctx, &ctx.home_path(".cursor/extensions")),
            skills: skills_out,
            cleanup,
            usage_note: if known {
                "Skill use is estimated from the last 120 days of Cursor agent transcripts.".into()
            } else {
                "Cursor keeps no record of skill use that DiskPilot can read.".into()
            },
            total_bytes: tool.total_bytes,
        },
        recs,
    )
}

pub fn disconnect(input: &Input, id: &str) -> Result<(), AppError> {
    disconnect_json_mcp(input, ID, &mcp_files(input.ctx), id)
}

/// Remove rules from `cli-config.json` by id, in one edit.
pub fn remove_permissions(input: &Input, ids: &[String]) -> Result<(), AppError> {
    let mut rules: Vec<(&str, &str)> = Vec::new();
    for id in ids {
        let mut parts = id.splitn(3, '|');
        let (Some(ID), Some(key @ ("allow" | "deny")), Some(rule)) =
            (parts.next(), parts.next(), parts.next())
        else {
            return Err(AppError::Other("Unknown rule.".into()));
        };
        rules.push((key, rule));
    }
    let changed = super::edit::edit_json(
        &input.ctx.home_path(CLI_CONFIG),
        input.data_dir,
        ID,
        &|doc| {
            let mut any = false;
            for (key, rule) in &rules {
                if let Some(list) = doc
                    .pointer_mut(&format!("/permissions/{key}"))
                    .and_then(Value::as_array_mut)
                {
                    let before = list.len();
                    list.retain(|v| v.as_str() != Some(rule));
                    any |= before != list.len();
                }
            }
            Ok(any)
        },
    )?;
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
    use std::path::Path;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn reads_permissions_servers_extensions_and_disconnects() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(
            &home.join(CLI_CONFIG),
            r#"{"permissions":{"allow":["Shell(*)","Read(**)"],"deny":[]},"approvalMode":"allowlist","sandbox":{"mode":"disabled","networkAccess":"allow"}}"#,
        );
        write(
            &home.join(".cursor/mcp.json"),
            "{\n  // mine\n  \"mcpServers\": {\n    \"pg\": {\"command\": \"npx\", \"args\": [\"-y\", \"pg-mcp\", \"postgres://u:pw@h/db\"]},\n    \"web\": {\"url\": \"https://mcp.example.com/sse\", \"headers\": {\"Authorization\": \"Bearer t\"}},\n  }\n}\n",
        );
        write(
            &home.join(".cursor/extensions/extensions.json"),
            r#"[{"identifier":{"id":"golang.go"},"version":"0.58.0","relativeLocation":"golang.go-0.58.0-universal","metadata":{"publisherDisplayName":"Go Team"}},
                {"identifier":{"id":"evil.x"},"version":"1","relativeLocation":"../../etc"}]"#,
        );
        write(
            &home.join(".cursor/extensions/golang.go-0.58.0-universal/package.json"),
            "{}",
        );
        write(
            &home.join(".cursor/skills-cursor/review/SKILL.md"),
            "---\nname: review\n---\n",
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
        assert_eq!(r.permissions.iter().filter(|p| p.risk.is_some()).count(), 2);
        assert_eq!(
            r.connections.len(),
            2,
            "JSON with comments and trailing commas is read"
        );
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("pw@") && !json.contains("Bearer"));
        assert_eq!(
            r.plugins.len(),
            1,
            "registry entries outside the folder are ignored"
        );
        assert_eq!(r.plugins[0].publisher.as_deref(), Some("Go Team"));
        assert_eq!(r.skills[0].origin, SkillOrigin::BuiltIn);
        assert!(r.skills[0].recommendation_id.is_none());

        let pg = r
            .connections
            .iter()
            .find(|c| c.name == "pg")
            .unwrap()
            .id
            .clone();
        // The file has comments, so it cannot be rewritten as JSON safely.
        assert!(disconnect(&input, &pg).is_err());
        write(
            &home.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"pg":{"command":"npx"}}}"#,
        );
        disconnect(&input, &pg).unwrap();
        assert!(analyze(&input).0.connections.is_empty());
        assert!(disconnect(&input, "cursor|/etc/passwd|x").is_err());

        remove_permission(&input, "cursor|allow|Shell(*)").unwrap();
        assert!(remove_permission(&input, "cursor|allow|Shell(*)").is_err());
    }
}
