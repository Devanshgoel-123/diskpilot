//! Google Antigravity: MCP servers in `~/.gemini/config/mcp_config.json`
//! (older versions: `~/.gemini/antigravity/mcp_config.json`), agent settings
//! in the editor's `settings.json`, extensions and skills.

use std::path::PathBuf;

use serde_json::Value;

use super::skills;
use super::{
    cleanup_items, disconnect_json_mcp, editor_extensions, json_mcp_connections, parse_jsonc,
    redact, tilde, AgentReport, Input, Permission, PermissionKind, SkillOrigin,
};
use crate::devtools::{editors, exists, read_small, Ctx};
use crate::error::AppError;
use crate::model::Recommendation;

pub const ID: &str = "antigravity";
const NAME: &str = "Antigravity";
/// Antigravity's settings, in Application Support or `%APPDATA%`.
fn settings_path(ctx: &Ctx) -> PathBuf {
    ctx.home_path(&format!(
        "{}/Antigravity/User/settings.json",
        crate::devtools::APP_DATA
    ))
}
const SKILL_DIRS: &[&str] = &[
    ".gemini/skills",
    ".gemini/config/skills",
    ".gemini/antigravity/skills",
    ".gemini/antigravity-cli/skills",
];

fn mcp_files(ctx: &Ctx) -> Vec<PathBuf> {
    vec![
        ctx.home_path(".gemini/config/mcp_config.json"),
        ctx.home_path(".gemini/antigravity/mcp_config.json"),
    ]
}

/// Words in a setting's name that make it about what the agent may do.
const PERMISSION_WORDS: &[&str] = &[
    "allow",
    "deny",
    "policy",
    "autoexecut",
    "autorun",
    "auto_run",
    "terminal",
    "review",
    "confirm",
    "permission",
    "approval",
    "sandbox",
    "browser",
    "secure",
];

fn setting_risk(key: &str, value: &Value) -> Option<String> {
    let k = key.to_ascii_lowercase();
    let v = value
        .as_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if (k.contains("terminal") || k.contains("execut") || k.contains("autorun"))
        && (matches!(v.as_str(), "turbo" | "always" | "auto" | "always_proceed")
            || value == &Value::Bool(true))
    {
        return Some("The agent runs terminal commands without asking.".into());
    }
    if k.contains("review") && matches!(v.as_str(), "always_proceed" | "never" | "agent_decides") {
        return Some("The agent's changes are applied without your review.".into());
    }
    None
}

fn permissions(ctx: &Ctx) -> Vec<Permission> {
    let path = settings_path(ctx);
    let Some(doc) = read_small(&path, 4 * 1024 * 1024).and_then(|t| parse_jsonc(&t)) else {
        return Vec::new();
    };
    let source = tilde(&path, ctx.home);
    let mut out = Vec::new();
    let Some(map) = doc.as_object() else {
        return out;
    };
    for (key, value) in map {
        let k = key.to_ascii_lowercase();
        let about_agent = k.starts_with("antigravity")
            || k.contains("agent")
            || k.starts_with("jetski")
            || k.starts_with("cascade");
        if !about_agent || !PERMISSION_WORDS.iter().any(|w| k.contains(w)) {
            continue;
        }
        let label = key.rsplit('.').next().unwrap_or(key).to_string();
        match value {
            Value::Array(items) => {
                let kind = if k.contains("deny") || k.contains("block") {
                    PermissionKind::Deny
                } else {
                    PermissionKind::Allow
                };
                for item in items.iter().filter_map(Value::as_str) {
                    out.push(Permission {
                        id: None,
                        kind,
                        label: label.clone(),
                        value: redact(item),
                        source: source.clone(),
                        risk: (kind == PermissionKind::Allow && matches!(item, "*" | "**"))
                            .then(|| "Allows everything.".into()),
                    });
                }
            }
            Value::String(_) | Value::Bool(_) | Value::Number(_) => out.push(Permission {
                id: None,
                kind: PermissionKind::Mode,
                label,
                value: match value {
                    Value::String(s) => redact(s),
                    other => other.to_string(),
                },
                source: source.clone(),
                risk: setting_risk(key, value),
            }),
            _ => {}
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
    let mut found = Vec::new();
    for d in SKILL_DIRS {
        found.extend(skills::find(&ctx.home_path(d), SkillOrigin::User, None));
    }
    // Antigravity stores conversations in a binary format, so skill use
    // cannot be read; skills are listed without a last-used date.
    let (skills_out, mut recs) =
        skills::rows(ctx, ID, NAME, found, &Default::default(), false, |f| {
            crate::devtools::name_of(&f.dir)
        });
    let (tool, tool_recs) = super::tool_report(input, ID, editors::antigravity);
    let cleanup = cleanup_items(&tool);
    recs.extend(tool_recs);

    let detected = tool.detected
        || exists(&ctx.home_path(".gemini/antigravity"))
        || exists(&ctx.home_path(".gemini/config/mcp_config.json"));
    (
        AgentReport {
            id: ID.into(),
            name: NAME.into(),
            detected,
            version: tool.version.clone(),
            permissions: permissions(ctx),
            connections,
            plugins: editor_extensions(ctx, &ctx.home_path(".antigravity/extensions")),
            skills: skills_out,
            cleanup,
            usage_note: "Antigravity keeps no record of skill use that DiskPilot can read, so no skill is marked inactive.".into(),
            total_bytes: tool.total_bytes,
        },
        recs,
    )
}

pub fn disconnect(input: &Input, id: &str) -> Result<(), AppError> {
    disconnect_json_mcp(input, ID, &mcp_files(input.ctx), id)
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
    fn reads_servers_from_both_locations_settings_and_skills() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(
            &home.join(".gemini/config/mcp_config.json"),
            r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","fs-mcp"],"disabled":true}}}"#,
        );
        write(
            &home.join(".gemini/antigravity/mcp_config.json"),
            r#"{"mcpServers":{"web":{"serverUrl":"https://x.example.com/mcp"}}}"#,
        );
        let c = ctx(&home);
        write(
            &settings_path(&c),
            "{\n  \"editor.fontSize\": 13,\n  \"antigravity.agent.terminalExecutionPolicy\": \"turbo\", // yolo\n  \"antigravity.agent.terminal.allowList\": [\"git status\"],\n}\n",
        );
        write(
            &home.join(".gemini/skills/notes/SKILL.md"),
            "---\nname: notes\ndescription: Notes.\n---\n",
        );

        let data = home.join("data");
        let input = Input {
            ctx: &c,
            data_dir: Some(&data),
            tools: &[],
        };
        let (r, recs) = analyze(&input);
        assert!(r.detected);
        assert_eq!(r.connections.len(), 2);
        assert!(
            !r.connections
                .iter()
                .find(|c| c.name == "fs")
                .unwrap()
                .enabled
        );
        assert_eq!(
            r.connections
                .iter()
                .find(|c| c.name == "web")
                .unwrap()
                .target,
            "x.example.com"
        );
        assert_eq!(r.permissions.len(), 2, "only agent settings are listed");
        assert!(r.permissions.iter().any(|p| p.risk.is_some()));
        assert_eq!(r.skills.len(), 1);
        assert_eq!(r.skills[0].state, super::super::SkillState::Unknown);
        assert!(
            recs.iter().all(|r| !r.path.contains("skills")),
            "skills are never inactive without a record"
        );

        let web = r
            .connections
            .iter()
            .find(|c| c.name == "web")
            .unwrap()
            .id
            .clone();
        disconnect(&input, &web).unwrap();
        assert_eq!(analyze(&input).0.connections.len(), 1);
    }
}
