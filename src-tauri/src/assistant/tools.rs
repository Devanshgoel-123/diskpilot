//! Read-only tools the assistant can call.
//!
//! Every tool answers from data DiskPilot already has (the last scan, its
//! recommendations, the developer-tools report) or from read-only commands
//! (`docker system df`). None of them can change anything. Paths are shown
//! relative to the home folder (`~`), and file contents are never read.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::classifier::Classifier;
use crate::devtools::DevToolsReport;
use crate::docker::{self, cli::DockerCli, DockerState};
use crate::explorer;
use crate::model::{DiskInfo, Recommendation, Tier};
use crate::safety;
use crate::scanner::ScanOutcome;

/// Snapshot of what the assistant may read.
pub struct ToolContext {
    pub home: PathBuf,
    pub scan: Option<Arc<ScanOutcome>>,
    pub recommendations: Vec<Recommendation>,
    pub devtools: Option<Arc<DevToolsReport>>,
    pub disk: Option<DiskInfo>,
}

/// Result of running one tool.
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    /// Recommendation ids the model asked the UI to highlight.
    pub highlight: Vec<String>,
}

pub const HIGHLIGHT_TOOL: &str = "highlight_recommendations";

/// Tool definitions sent to the API. Order is fixed so the prompt prefix stays
/// cacheable.
pub fn definitions() -> Value {
    json!([
        {
            "name": "get_storage_overview",
            "description": "Disk capacity, the last scan (folder, time, totals) and storage per category, plus the largest recognized items. Call this first for general questions.",
            "input_schema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "list_recommendations",
            "description": "Cleanup recommendations from the last scan and the developer-tools analysis, largest first. Each has an id, tier (likelySafe, reviewFirst, keep), size, risks and what DiskPilot would do.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "tier": {"type": "string", "enum": ["likelySafe", "reviewFirst", "keep"], "description": "Only this tier."},
                    "contains": {"type": "string", "description": "Only items whose title or path contains this text (case-insensitive)."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "additionalProperties": false
            }
        },
        {
            "name": "find_largest",
            "description": "The largest files, or the folders where data accumulates, from the last scan. Optionally filter by category or path text.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["files", "folders"]},
                    "category": {"type": "string", "description": "Category id, e.g. cache, buildArtifacts, developerTools, dockerStorage, applicationData, logs, downloads, userData, binaryData, systemFiles, unknown."},
                    "contains": {"type": "string", "description": "Only paths containing this text (case-insensitive)."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                },
                "required": ["kind"],
                "additionalProperties": false
            }
        },
        {
            "name": "list_folder",
            "description": "Contents of one folder inside the scanned folder with sizes and categories, largest first. Use ~ for the home folder.",
            "input_schema": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_dev_tools_report",
            "description": "Per-tool analysis of Cursor, VS Code, AI coding agents, Node.js, Xcode, Rust and Python: storage, versions, and items for deleted projects or superseded versions.",
            "input_schema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "get_docker_usage",
            "description": "Docker disk usage: images (with the containers using them), containers, volumes and build cache. Read-only.",
            "input_schema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": HIGHLIGHT_TOOL,
            "description": "Show specific recommendations to the user as cards they can review. This does not clean anything: the user previews and confirms in DiskPilot. Use ids returned by list_recommendations.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "ids": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 20}
                },
                "required": ["ids"],
                "additionalProperties": false
            }
        }
    ])
}

fn tilde(path: &str, home: &Path) -> String {
    let h = home.display().to_string();
    if path == h {
        "~".into()
    } else if let Some(rest) = path.strip_prefix(&format!("{h}/")) {
        format!("~/{rest}")
    } else {
        path.to_string()
    }
}

/// How a path is shown to the model. Anything inside a private area (Mail,
/// Messages, Photos and so on) is replaced by the area itself, so file names
/// there never leave the Mac.
fn shown(path: &str, home: &Path) -> (String, Option<&'static str>) {
    match crate::privacy::private_area(Path::new(path), home) {
        Some((root, label)) => (tilde(&root.display().to_string(), home), Some(label)),
        None => (tilde(path, home), None),
    }
}

fn private_what(label: &str) -> String {
    format!("{label} (private: contents are not shared with the assistant)")
}

fn gb(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.0} MB", b / 1e6)
    } else {
        format!("{:.0} KB", b / 1e3)
    }
}

fn limit(input: &Value, default: usize) -> usize {
    input
        .get("limit")
        .and_then(Value::as_u64)
        .map_or(default, |n| n.clamp(1, 50) as usize)
}

fn text_filter(input: &Value) -> Option<String> {
    input
        .get("contains")
        .and_then(Value::as_str)
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty())
}

fn ok(v: Value) -> ToolOutput {
    ToolOutput {
        content: v.to_string(),
        is_error: false,
        highlight: Vec::new(),
    }
}

fn err(msg: &str) -> ToolOutput {
    ToolOutput {
        content: msg.to_string(),
        is_error: true,
        highlight: Vec::new(),
    }
}

const NO_SCAN: &str = "No scan has been run yet. Ask the user to click Scan first.";

pub fn run(ctx: &ToolContext, name: &str, input: &Value) -> ToolOutput {
    let home = ctx.home.as_path();
    match name {
        "get_storage_overview" => {
            let disk = ctx.disk.as_ref().map(|d| {
                json!({"total": gb(d.total_bytes), "used": gb(d.used_bytes), "available": gb(d.available_bytes)})
            });
            let Some(scan) = &ctx.scan else {
                return ok(json!({"disk": disk, "scan": null, "note": NO_SCAN}));
            };
            let s = &scan.summary;
            let categories: Vec<Value> = s
                .categories
                .iter()
                .map(
                    |c| json!({"category": c.category, "size": gb(c.bytes), "files": c.file_count}),
                )
                .collect();
            let top: Vec<Value> = s
                .detected_locations
                .iter()
                .filter(|l| !l.container)
                .take(12)
                .map(|l| {
                    let (path, private) = shown(&l.path, home);
                    let what = private.map_or(l.classification.label.clone(), private_what);
                    json!({"what": what, "path": path, "size": gb(l.bytes)})
                })
                .collect();
            ok(json!({
                "disk": disk,
                "scan": {
                    "folder": tilde(&s.root, home),
                    "finished_at_ms": s.finished_at_ms,
                    "total": gb(s.total_bytes),
                    "files": s.file_count,
                    "unreadable_items": s.error_count
                },
                "categories": categories,
                "largest_recognized_items": top
            }))
        }
        "list_recommendations" => {
            let tier = input.get("tier").and_then(Value::as_str);
            let contains = text_filter(input);
            let items: Vec<Value> = ctx
                .recommendations
                .iter()
                .filter(|r| match tier {
                    Some("likelySafe") => r.tier == Tier::LikelySafe,
                    Some("reviewFirst") => r.tier == Tier::ReviewFirst,
                    Some("keep") => r.tier == Tier::Keep,
                    _ => true,
                })
                // Filter on what the model is allowed to see, so the filter
                // cannot be used to probe names inside a private area.
                .filter(|r| {
                    contains.as_ref().is_none_or(|c| {
                        let (path, private) = shown(&r.path, home);
                        let title = if private.is_some() { "" } else { r.title.as_str() };
                        title.to_lowercase().contains(c) || path.to_lowercase().contains(c)
                    })
                })
                .take(limit(input, 20))
                .map(|r| {
                    let (path, private) = shown(&r.path, home);
                    let title = private.map_or(r.title.clone(), |l| format!("Item in {l}"));
                    json!({
                        "id": r.id,
                        "title": title,
                        "path": path,
                        "size": gb(r.bytes),
                        "tier": r.tier,
                        "why_safe": r.safe_because,
                        "risks": r.risks,
                        "diskpilot_can": if r.methods.is_empty() { json!("nothing (manual only)") } else { json!(r.methods) },
                        "manual_command": r.alternative,
                    })
                })
                .collect();
            if ctx.scan.is_none() && items.is_empty() {
                return ok(json!({"recommendations": [], "note": NO_SCAN}));
            }
            ok(json!({"total": ctx.recommendations.len(), "recommendations": items}))
        }
        "find_largest" => {
            let Some(scan) = &ctx.scan else {
                return err(NO_SCAN);
            };
            let category = input.get("category").and_then(Value::as_str);
            let contains = text_filter(input);
            let n = limit(input, 15);
            let matches = |path: &str, cat: &crate::model::Category| {
                category.is_none_or(|c| {
                    serde_json::to_value(cat)
                        .ok()
                        .and_then(|v| v.as_str().map(|s| s == c))
                        == Some(true)
                }) && contains
                    .as_ref()
                    .is_none_or(|c| shown(path, home).0.to_lowercase().contains(c))
            };
            let items: Vec<Value> = if input.get("kind").and_then(Value::as_str) == Some("folders")
            {
                scan.summary
                    .heavy_directories
                    .iter()
                    .filter(|d| matches(&d.path, &d.classification.category))
                    .take(n)
                    .map(|d| {
                        let (path, private) = shown(&d.path, home);
                        let what = private.map_or(d.classification.label.clone(), private_what);
                        json!({"path": path, "size": gb(d.bytes), "files": d.file_count, "what": what})
                    })
                    .collect()
            } else {
                scan.summary
                    .largest_files
                    .iter()
                    .filter(|f| matches(&f.path, &f.classification.category))
                    .take(n)
                    .map(|f| {
                        let (path, private) = shown(&f.path, home);
                        match private {
                            Some(l) => json!({"path": path, "size": gb(f.bytes), "what": private_what(l)}),
                            None => json!({"path": path, "size": gb(f.bytes), "modified_ms": f.modified_ms, "what": f.classification.label}),
                        }
                    })
                    .collect()
            };
            // Several files from one private area collapse into one entry.
            let mut seen = std::collections::HashSet::new();
            let items: Vec<Value> = items
                .into_iter()
                .filter(|i| {
                    let private = i["what"].as_str().is_some_and(|w| w.contains("(private:"));
                    !private || seen.insert(i["path"].to_string())
                })
                .collect();
            ok(json!({"items": items}))
        }
        "list_folder" => {
            let Some(scan) = &ctx.scan else {
                return err(NO_SCAN);
            };
            let raw = input.get("path").and_then(Value::as_str).unwrap_or("~");
            let expanded = if raw == "~" {
                home.display().to_string()
            } else if let Some(rest) = raw.strip_prefix("~/") {
                home.join(rest).display().to_string()
            } else {
                raw.to_string()
            };
            let root = PathBuf::from(&scan.summary.root);
            // Resolve the real path first (case and symlinks), so the private
            // area check sees the folder that will actually be listed.
            let path = match safety::parse_user_path(&expanded)
                .and_then(|p| {
                    std::fs::canonicalize(&p)
                        .map_err(|_| safety::SafetyError::Missing(p.display().to_string()))
                })
                .and_then(|p| safety::ensure_within(&p, &root).map(|_| p))
            {
                Ok(p) => p,
                Err(e) => return err(&e.to_string()),
            };
            let private = crate::privacy::private_area(&path, home);
            match explorer::list_directory(&path, &root, &scan.index, &Classifier::new(home)) {
                Ok(listing) if private.is_some() => {
                    let (area, label) = private.unwrap_or_default();
                    ok(json!({
                        "path": tilde(&area.display().to_string(), home),
                        "size": listing.bytes.map(gb),
                        "what": private_what(label),
                        "note": "DiskPilot does not share what is inside this folder. The user can look in the Storage page.",
                    }))
                }
                Ok(listing) => ok(json!({
                    "path": tilde(&listing.path, home),
                    "size": listing.bytes.map(gb),
                    "entries": listing.entries.iter().take(40).map(|e| json!({
                        "name": e.name,
                        "kind": e.kind,
                        "size": e.bytes.map(gb),
                        "what": e.classification.label,
                    })).collect::<Vec<_>>(),
                    "more_entries": listing.entries.len().saturating_sub(40) as u64 + listing.omitted_count,
                })),
                Err(e) => err(&e.to_string()),
            }
        }
        "get_dev_tools_report" => match &ctx.devtools {
            None => ok(
                json!({"note": "The developer tools analysis has not run yet. It runs when the user opens Developer Tools."}),
            ),
            Some(r) => ok(json!({
                "tools": r.tools.iter().filter(|t| t.detected).map(|t| json!({
                    "tool": t.name,
                    "version": t.version,
                    "size": gb(t.total_bytes),
                    "summary": t.summary,
                    "flagged_items": t.sections.iter().flat_map(|s| s.items.iter())
                        .filter(|i| i.recommendation_id.is_some())
                        .take(10)
                        .map(|i| json!({"label": i.label, "size": gb(i.bytes), "state": i.state, "recommendation_id": i.recommendation_id}))
                        .collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "worth_reviewing": gb(r.reclaimable_bytes),
            })),
        },
        "get_docker_usage" => {
            let cli = DockerCli::locate(home);
            let status = docker::status(cli.as_ref());
            if status.state != DockerState::Running {
                return ok(json!({"docker": status.state, "note": status.message}));
            }
            let Some(cli) = cli else {
                return err("Docker CLI not found");
            };
            match docker::overview(&cli, status) {
                Ok(o) => ok(json!({
                    "usage": {
                        "images": gb(o.usage.images.size_bytes),
                        "containers": gb(o.usage.containers.size_bytes),
                        "volumes": gb(o.usage.volumes.size_bytes),
                        "build_cache": gb(o.usage.build_cache.size_bytes),
                        "reclaimable_build_cache": gb(o.usage.build_cache.reclaimable_bytes),
                    },
                    "images": o.images.iter().take(20).map(|i| json!({
                        "name": if i.dangling { "<untagged>".to_string() } else { format!("{}:{}", i.repository, i.tag) },
                        "frees_if_removed": gb(i.unique_bytes),
                        "used_by": i.used_by,
                    })).collect::<Vec<_>>(),
                    "containers": o.containers.iter().map(|c| json!({"name": c.name, "image": c.image, "running": c.running})).collect::<Vec<_>>(),
                    "volumes": o.volumes.iter().map(|v| json!({"name": v.name, "size": v.size_bytes.map(gb), "used_by": v.used_by})).collect::<Vec<_>>(),
                    "note": "Removal happens in DiskPilot's Docker page, after a preview the person confirms. DiskPilot deletes a volume only when the person ticks it (or picks it in the Volumes list) and types the name to confirm; volumes another container uses are never deleted."
                })),
                Err(e) => err(&e.to_string()),
            }
        }
        HIGHLIGHT_TOOL => {
            let ids: Vec<String> = input
                .get("ids")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let known: Vec<String> = ids
                .into_iter()
                .filter(|id| ctx.recommendations.iter().any(|r| &r.id == id))
                .collect();
            if known.is_empty() {
                return err("None of those ids match a current recommendation. Call list_recommendations to get valid ids.");
            }
            ToolOutput {
                content: json!({"shown_to_user": known.len()}).to_string(),
                is_error: false,
                highlight: known,
            }
        }
        _ => err("Unknown tool"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ToolContext {
        ToolContext {
            home: PathBuf::from("/Users/test"),
            scan: None,
            recommendations: Vec::new(),
            devtools: None,
            disk: None,
        }
    }

    #[test]
    fn definitions_are_well_formed_and_include_no_write_tools() {
        let defs = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&HIGHLIGHT_TOOL));
        for forbidden in [
            "delete", "remove", "trash", "execute", "run", "shell", "write",
        ] {
            assert!(!names.iter().any(|n| n.contains(forbidden)), "{forbidden}");
        }
        for d in defs.as_array().unwrap() {
            assert_eq!(d["input_schema"]["type"], "object");
            assert!(!d["description"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn tools_explain_missing_scan_instead_of_failing_silently() {
        let out = run(&ctx(), "find_largest", &json!({"kind": "files"}));
        assert!(out.is_error);
        assert!(out.content.contains("Scan"));
        let overview = run(&ctx(), "get_storage_overview", &json!({}));
        assert!(!overview.is_error);
    }

    #[test]
    fn highlight_only_accepts_known_ids() {
        let out = run(&ctx(), HIGHLIGHT_TOOL, &json!({"ids": ["/etc/passwd"]}));
        assert!(out.is_error);
        assert!(out.highlight.is_empty());
    }

    #[test]
    fn private_areas_are_shown_as_totals_only() {
        let home = Path::new("/Users/test");
        let (path, label) = shown(
            "/Users/test/Library/Messages/Attachments/ab/IMG_0001.heic",
            home,
        );
        assert_eq!(
            (path.as_str(), label),
            ("~/Library/Messages", Some("Messages"))
        );
        let (path, label) = shown("/Users/test/Downloads/a.dmg", home);
        assert_eq!((path.as_str(), label), ("~/Downloads/a.dmg", None));
    }

    #[test]
    fn paths_are_shown_relative_to_home() {
        let home = Path::new("/Users/test");
        assert_eq!(tilde("/Users/test/Library", home), "~/Library");
        assert_eq!(tilde("/Users/test", home), "~");
        assert_eq!(tilde("/Users/tester/x", home), "/Users/tester/x");
    }
}
