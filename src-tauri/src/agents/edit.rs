//! The only changes DiskPilot makes to an agent's configuration.
//!
//! Every change follows the same steps:
//! 1. read the file (refusing anything over 64 MB),
//! 2. apply the change to the parsed document, keeping key order,
//!    comments (TOML) and everything else as it was,
//! 3. save a copy of the original in DiskPilot's data folder,
//! 4. write the new version to a temporary file next to it with the same
//!    permissions and rename it over the original, but only if the file
//!    did not change since step 1 (the agent may be writing it too);
//!    otherwise start again, up to three times.
//!
//! An MCP server that is disconnected by removing it from a JSON file is
//! kept in DiskPilot's data folder so it can be reconnected exactly as it
//! was. Those copies can hold the server's credentials, so the file is
//! readable only by the user, like the agent's own config.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

const MAX_CONFIG: u64 = 64 * 1024 * 1024;
const BACKUPS_KEPT: usize = 20;
const STASH_FILE: &str = "agent-disconnected.json";

type Result<T> = std::result::Result<T, AppError>;

fn other(msg: impl Into<String>) -> AppError {
    AppError::Other(msg.into())
}

/// The file to edit: a symlinked config (dotfiles) is edited at its target.
fn real_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|e| AppError::io(path, &e))
}

struct Snapshot {
    bytes: Vec<u8>,
    modified: Option<SystemTime>,
    len: u64,
    mode: u32,
}

fn read(path: &Path) -> Result<Snapshot> {
    let meta = std::fs::metadata(path).map_err(|e| AppError::io(path, &e))?;
    if !meta.is_file() || meta.len() > MAX_CONFIG {
        return Err(other(format!(
            "{} is not a config file DiskPilot can edit.",
            path.display()
        )));
    }
    let bytes = std::fs::read(path).map_err(|e| AppError::io(path, &e))?;
    Ok(Snapshot {
        len: bytes.len() as u64,
        bytes,
        modified: meta.modified().ok(),
        mode: crate::fsmode::mode(&meta),
    })
}

fn unchanged(path: &Path, snap: &Snapshot) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() == snap.len && m.modified().ok() == snap.modified)
        .unwrap_or(false)
}

fn backup(data_dir: Option<&Path>, agent: &str, path: &Path, bytes: &[u8]) -> Result<()> {
    let Some(data_dir) = data_dir else {
        return Err(other("DiskPilot's data folder is unavailable, so no backup can be made. Nothing was changed."));
    };
    let dir = data_dir.join("agent-config-backups");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::io(&dir, &e))?;
    let _ = crate::fsmode::set_mode(&dir, 0o700);
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().trim_start_matches('.').to_string())
        .unwrap_or_else(|| "config".into());
    let prefix = format!("{agent}-{file}-");
    let stamp = crate::scanner::to_ms(SystemTime::now()).unwrap_or(0);
    let target = dir.join(format!("{prefix}{stamp}.bak"));
    write_private(&target, bytes)?;
    // Keep the newest few backups of this file.
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut old: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
            })
            .collect();
        old.sort();
        let excess = old.len().saturating_sub(BACKUPS_KEPT);
        for p in old.into_iter().take(excess) {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic(path, bytes, 0o600)
}

fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let dir = path.parent().ok_or_else(|| other("Invalid path"))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.diskpilot-{}", std::process::id()));
    let result = (|| {
        let mut f = crate::fsmode::create_with(
            std::fs::OpenOptions::new().write(true).create_new(true),
            mode,
        )
        .open(&tmp)
        .map_err(|e| AppError::io(&tmp, &e))?;
        f.write_all(bytes).map_err(|e| AppError::io(&tmp, &e))?;
        f.sync_all().map_err(|e| AppError::io(&tmp, &e))?;
        crate::fsmode::set_mode(&tmp, mode).map_err(|e| AppError::io(&tmp, &e))?;
        std::fs::rename(&tmp, path).map_err(|e| AppError::io(path, &e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Apply `change` to a file's text. `change` returns the new text, or `None`
/// when there is nothing to do.
fn edit_text(
    path: &Path,
    data_dir: Option<&Path>,
    agent: &str,
    change: &dyn Fn(&str) -> Result<Option<String>>,
) -> Result<bool> {
    let path = real_path(path)?;
    for _ in 0..3 {
        let snap = read(&path)?;
        let text = String::from_utf8(snap.bytes.clone())
            .map_err(|_| other(format!("{} is not UTF-8 text.", path.display())))?;
        let Some(new_text) = change(&text)? else {
            return Ok(false);
        };
        if !unchanged(&path, &snap) {
            continue;
        }
        backup(data_dir, agent, &path, &snap.bytes)?;
        if !unchanged(&path, &snap) {
            continue;
        }
        write_atomic(&path, new_text.as_bytes(), snap.mode)?;
        return Ok(true);
    }
    Err(other(format!(
        "{} kept changing while DiskPilot tried to update it. Quit the agent and try again.",
        path.display()
    )))
}

/// Edit a JSON config. Key order is kept; the file is written back with two
/// spaces of indentation, the way the agents write it.
pub fn edit_json(
    path: &Path,
    data_dir: Option<&Path>,
    agent: &str,
    change: &dyn Fn(&mut Value) -> Result<bool>,
) -> Result<bool> {
    edit_text(path, data_dir, agent, &|text| {
        let mut doc: Value = serde_json::from_str(text)
            .map_err(|_| other(format!("{} has comments or is not plain JSON, so DiskPilot left it unchanged. Make this change in the agent instead.", path.display())))?;
        if !change(&mut doc)? {
            return Ok(None);
        }
        let mut out = serde_json::to_string_pretty(&doc).map_err(|e| other(e.to_string()))?;
        if text.ends_with('\n') {
            out.push('\n');
        }
        Ok(Some(out))
    })
}

/// Edit a TOML config, keeping comments and formatting.
pub fn edit_toml(
    path: &Path,
    data_dir: Option<&Path>,
    agent: &str,
    change: &dyn Fn(&mut toml_edit::DocumentMut) -> Result<bool>,
) -> Result<bool> {
    edit_text(path, data_dir, agent, &|text| {
        let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| {
            other(format!(
                "{} could not be read, so DiskPilot left it unchanged: {e}",
                path.display()
            ))
        })?;
        if !change(&mut doc)? {
            return Ok(None);
        }
        Ok(Some(doc.to_string()))
    })
}

// ---- Disconnected servers ----------------------------------------------------

/// A server removed from a JSON config, kept so it can be put back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stashed {
    pub id: String,
    pub agent: String,
    /// The config file it was removed from.
    pub file: PathBuf,
    /// Keys leading to the object that held it, e.g. ["mcpServers"].
    pub parent: Vec<String>,
    pub name: String,
    pub scope: String,
    pub value: Value,
    /// Where it was among its siblings, so it goes back in the same place.
    #[serde(default)]
    pub position: Option<usize>,
    pub at_ms: i64,
}

pub fn stashed(data_dir: &Path) -> Vec<Stashed> {
    let path = data_dir.join(STASH_FILE);
    std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_stash(data_dir: &Path, items: &[Stashed]) -> Result<()> {
    std::fs::create_dir_all(data_dir).map_err(|e| AppError::io(data_dir, &e))?;
    let bytes = serde_json::to_vec_pretty(items).map_err(|e| other(e.to_string()))?;
    write_private(&data_dir.join(STASH_FILE), &bytes)
}

fn object_at<'a>(
    doc: &'a mut Value,
    keys: &[String],
) -> Option<&'a mut serde_json::Map<String, Value>> {
    let mut cur = doc;
    for k in keys {
        cur = cur.as_object_mut()?.get_mut(k)?;
    }
    cur.as_object_mut()
}

/// Remove `parent.name` from a JSON config and keep it for reconnecting.
#[allow(clippy::too_many_arguments)]
pub fn disconnect_json(
    data_dir: Option<&Path>,
    agent: &str,
    file: &Path,
    parent: &[String],
    name: &str,
    scope: &str,
    now_ms: i64,
) -> Result<()> {
    let data = data_dir
        .ok_or_else(|| other("DiskPilot's data folder is unavailable. Nothing was changed."))?;
    let removed = std::cell::RefCell::new(None);
    let changed = edit_json(file, data_dir, agent, &|doc| {
        let Some(obj) = object_at(doc, parent) else {
            return Ok(false);
        };
        let position = obj.keys().position(|k| k == name);
        // `shift_remove` keeps the order of the remaining servers.
        let v = obj.shift_remove(name);
        let found = v.is_some();
        *removed.borrow_mut() = v.map(|v| (v, position));
        Ok(found)
    })?;
    let Some((value, position)) = removed.into_inner().filter(|_| changed) else {
        return Err(other(format!("{name} is no longer in {}.", file.display())));
    };
    let mut items = stashed(data);
    let id = format!("{agent}:{}:{name}:{now_ms}", parent.join("/"));
    items.push(Stashed {
        id,
        agent: agent.into(),
        file: file.to_path_buf(),
        parent: parent.to_vec(),
        name: name.into(),
        scope: scope.into(),
        value,
        position,
        at_ms: now_ms,
    });
    save_stash(data, &items)
}

/// Put a disconnected server back where it was.
pub fn reconnect(data_dir: Option<&Path>, id: &str) -> Result<()> {
    let data = data_dir.ok_or_else(|| other("DiskPilot's data folder is unavailable."))?;
    let mut items = stashed(data);
    let Some(pos) = items.iter().position(|s| s.id == id) else {
        return Err(other("That server is no longer in DiskPilot's list."));
    };
    let item = items[pos].clone();
    edit_json(&item.file, data_dir, &item.agent, &|doc| {
        let mut cur = &mut *doc;
        for k in &item.parent {
            let obj = cur.as_object_mut().ok_or_else(|| {
                other("The config file changed shape; reconnect it in the agent instead.")
            })?;
            cur = obj
                .entry(k.clone())
                .or_insert_with(|| Value::Object(Default::default()));
        }
        let obj = cur.as_object_mut().ok_or_else(|| {
            other("The config file changed shape; reconnect it in the agent instead.")
        })?;
        if obj.contains_key(&item.name) {
            // Already added back by hand: nothing to restore.
            return Ok(false);
        }
        // Put it back where it was; anything added since stays after it.
        let at = item.position.unwrap_or(obj.len()).min(obj.len());
        let rest: Vec<(String, Value)> = {
            let keys: Vec<String> = obj.keys().skip(at).cloned().collect();
            keys.into_iter()
                .filter_map(|k| obj.shift_remove(&k).map(|v| (k, v)))
                .collect()
        };
        obj.insert(item.name.clone(), item.value.clone());
        obj.extend(rest);
        Ok(true)
    })?;
    items.remove(pos);
    save_stash(data, &items)
}

/// Forget a disconnected server without restoring it.
pub fn forget(data_dir: Option<&Path>, id: &str) -> Result<()> {
    let data = data_dir.ok_or_else(|| other("DiskPilot's data folder is unavailable."))?;
    let mut items = stashed(data);
    items.retain(|s| s.id != id);
    save_stash(data, &items)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    #[cfg(unix)]
    fn json_edits_keep_key_order_permissions_and_a_backup() {
        let dir = crate::test_support::sandbox();
        let data = dir.path().join("data");
        let cfg = dir.path().join("claude.json");
        std::fs::write(
            &cfg,
            "{\n  \"z\": 1,\n  \"mcpServers\": {\n    \"b\": {\"type\": \"stdio\", \"command\": \"x\", \"env\": {\"TOKEN\": \"s\"}},\n    \"a\": {\"type\": \"http\", \"url\": \"https://h\"}\n  },\n  \"m\": true\n}\n",
        )
        .unwrap();
        std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o600)).unwrap();

        disconnect_json(
            Some(&data),
            "claude-code",
            &cfg,
            &["mcpServers".into()],
            "b",
            "All projects",
            1,
        )
        .unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.find("\"z\"").unwrap() < text.find("\"mcpServers\"").unwrap());
        assert!(!text.contains("\"b\""));
        assert!(text.ends_with("}\n"));
        assert_eq!(
            std::fs::metadata(&cfg).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let backups: Vec<_> = std::fs::read_dir(data.join("agent-config-backups"))
            .unwrap()
            .collect();
        assert_eq!(backups.len(), 1);
        let stash = data.join(STASH_FILE);
        assert_eq!(
            std::fs::metadata(&stash).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let items = stashed(&data);
        assert_eq!(items.len(), 1);
        reconnect(Some(&data), &items[0].id).unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(
            text.find("\"b\"").unwrap() < text.find("\"a\"").unwrap(),
            "back in its old place"
        );
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["mcpServers"]["b"]["env"]["TOKEN"], "s");
        assert!(stashed(&data).is_empty());
    }

    #[test]
    fn a_missing_server_is_an_error_and_nothing_is_written() {
        let dir = crate::test_support::sandbox();
        let data = dir.path().join("data");
        let cfg = dir.path().join("mcp.json");
        std::fs::write(&cfg, "{\"mcpServers\":{}}").unwrap();
        assert!(disconnect_json(
            Some(&data),
            "cursor",
            &cfg,
            &["mcpServers".into()],
            "x",
            "",
            1
        )
        .is_err());
        assert_eq!(
            std::fs::read_to_string(&cfg).unwrap(),
            "{\"mcpServers\":{}}"
        );
        assert!(!data.join("agent-config-backups").exists());
    }

    #[test]
    fn toml_edits_keep_comments() {
        let dir = crate::test_support::sandbox();
        let cfg = dir.path().join("config.toml");
        std::fs::write(
            &cfg,
            "# my settings\nmodel = \"o3\"\n\n[mcp_servers.docs]\ncommand = \"npx\" # launcher\n",
        )
        .unwrap();
        edit_toml(&cfg, Some(&dir.path().join("d")), "codex", &|doc| {
            doc["mcp_servers"]["docs"]["enabled"] = toml_edit::value(false);
            Ok(true)
        })
        .unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.starts_with("# my settings\n"));
        assert!(text.contains("# launcher"));
        assert!(text.contains("enabled = false"));
    }

    #[test]
    #[cfg(unix)]
    fn a_symlinked_config_is_edited_at_its_target() {
        let dir = crate::test_support::sandbox();
        let target = dir.path().join("dotfiles-settings.json");
        let link = dir.path().join("settings.json");
        std::fs::write(&target, "{\"a\":1}").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        edit_json(&link, Some(&dir.path().join("d")), "claude-code", &|doc| {
            doc["a"] = Value::from(2);
            Ok(true)
        })
        .unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(std::fs::read_to_string(&target).unwrap().contains("2"));
    }
}
