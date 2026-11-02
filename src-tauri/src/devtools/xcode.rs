//! Xcode: DerivedData per project, archives, device support and simulators.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use super::{
    app_version, exists, modified_ms, name_of, outermost_total, subdirs, Ctx, ItemState, RecSpec,
    ToolItem, ToolReport, ToolSection,
};
use crate::model::{CleanupMethod, Recommendation, Tier};

/// The project a DerivedData folder was built from (`WorkspacePath` in its
/// info.plist).
fn derived_data_workspace(dir: &Path) -> Option<PathBuf> {
    let value = plist::Value::from_file(dir.join("info.plist")).ok()?;
    value
        .as_dictionary()?
        .get("WorkspacePath")?
        .as_string()
        .map(PathBuf::from)
}

#[derive(Debug, Deserialize)]
struct SimctlDevices {
    devices: std::collections::BTreeMap<String, Vec<SimDevice>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimDevice {
    pub name: String,
    pub udid: String,
    pub state: String,
    #[serde(default)]
    pub is_available: bool,
    pub data_path: Option<String>,
    pub availability_error: Option<String>,
    pub last_booted_at: Option<String>,
}

/// Parse `xcrun simctl list devices --json` into (runtime, device) pairs.
pub fn parse_simctl(json: &str) -> Option<Vec<(String, SimDevice)>> {
    let parsed: SimctlDevices = serde_json::from_str(json).ok()?;
    Some(
        parsed
            .devices
            .into_iter()
            .flat_map(|(runtime, devices)| {
                let pretty = runtime
                    .rsplit('.')
                    .next()
                    .unwrap_or(&runtime)
                    .replacen('-', " ", 1)
                    .replace('-', ".");
                devices.into_iter().map(move |d| (pretty.clone(), d))
            })
            .collect(),
    )
}

/// Full Xcode (not just Command Line Tools) is needed for simctl.
fn has_full_xcode() -> bool {
    Command::new("/usr/bin/xcode-select")
        .arg("-p")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(".app/"))
        .unwrap_or(false)
}

fn simulators(ctx: &Ctx) -> Option<ToolSection> {
    if !has_full_xcode() {
        return None;
    }
    let out = Command::new("/usr/bin/xcrun")
        .args(["simctl", "list", "devices", "--json"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let devices = parse_simctl(&String::from_utf8_lossy(&out.stdout))?;
    let mut items: Vec<ToolItem> = devices
        .into_iter()
        .map(|(runtime, d)| {
            let path = d.data_path.as_deref().map(PathBuf::from);
            let bytes = path.as_deref().map_or(0, |p| ctx.size(p));
            ToolItem {
                label: format!("{} · {}", d.name, runtime),
                detail: if d.is_available {
                    format!("{} · {}", d.state, d.udid)
                } else {
                    format!(
                        "Unavailable{}",
                        d.availability_error
                            .map(|e| format!(": {e}"))
                            .unwrap_or_default()
                    )
                },
                state: if d.is_available {
                    ItemState::Info
                } else {
                    ItemState::Orphaned
                },
                path: path.map(|p| p.display().to_string()),
                bytes,
                last_used_ms: d.last_booted_at.as_deref().and_then(parse_iso8601_ms),
                recommendation_id: None,
            }
        })
        .collect();
    items.sort_by_key(|i| (i.state != ItemState::Orphaned, std::cmp::Reverse(i.bytes)));
    Some(ToolSection {
        title: "Simulators".into(),
        description: "Unavailable simulators belong to runtimes that are no longer installed. Remove them with `xcrun simctl delete unavailable`; DiskPilot does not delete simulator files directly.".into(),
        items,
    })
}

/// Parse "2026-10-01T12:34:56Z" (simctl's format) into Unix milliseconds.
fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let s = s.trim_end_matches('Z').replace('T', " ");
    crate::docker::parse::parse_timestamp(&format!("{s} +0000"))
}

pub fn analyze(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let dev = ctx.home_path("Library/Developer");
    let xcode_dir = dev.join("Xcode");
    let app = PathBuf::from("/Applications/Xcode.app");
    let detected = exists(&xcode_dir) || exists(&app) || exists(&dev.join("CoreSimulator"));
    let mut sections = Vec::new();
    let mut recs = Vec::new();
    let mut roots: Vec<(PathBuf, u64)> = Vec::new();

    // ---- DerivedData per project ---------------------------------------------
    let dd = xcode_dir.join("DerivedData");
    let mut dd_items = Vec::new();
    for dir in subdirs(&dd) {
        if name_of(&dir) == "ModuleCache.noindex" {
            continue;
        }
        let bytes = ctx.size(&dir);
        let workspace = derived_data_workspace(&dir);
        let gone = workspace.as_deref().is_some_and(|w| !exists(w));
        let last = modified_ms(&dir);
        let mut recommendation_id = None;
        if gone && bytes > 0 {
            let rec = RecSpec {
                source: "xcode",
                title: format!("DerivedData for a deleted project · {}", name_of(&dir)),
                path: &dir,
                bytes,
                tier: Tier::LikelySafe,
                methods: &[CleanupMethod::Trash, CleanupMethod::Delete],
                detected_because: format!(
                    "Xcode build data for {}, which no longer exists.",
                    workspace
                        .as_deref()
                        .map(|w| w.display().to_string())
                        .unwrap_or_default()
                ),
                safe_because:
                    "DerivedData is build output, and the project it was built from is gone.",
                risks:
                    "None for the deleted project; if it was only moved, Xcode rebuilds the data.",
                proposed_action: "Remove this project's DerivedData",
                alternative: None,
                last_activity_ms: last,
                notes: Vec::new(),
            }
            .build(ctx);
            recommendation_id = Some(rec.id.clone());
            recs.push(rec);
        }
        dd_items.push(ToolItem {
            label: name_of(&dir)
                .split('-')
                .next()
                .unwrap_or_default()
                .to_string(),
            path: Some(dir.display().to_string()),
            bytes,
            detail: match &workspace {
                Some(w) if gone => format!("{} (no longer exists)", w.display()),
                Some(w) => w.display().to_string(),
                None => "Workspace unknown".into(),
            },
            state: if gone {
                ItemState::Orphaned
            } else {
                ItemState::Current
            },
            last_used_ms: last,
            recommendation_id,
        });
    }
    dd_items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    if !dd_items.is_empty() {
        roots.push((dd.clone(), dd_items.iter().map(|i| i.bytes).sum()));
        sections.push(ToolSection {
            title: "DerivedData".into(),
            description:
                "Build data per project. Folders for projects that no longer exist are flagged."
                    .into(),
            items: dd_items,
        });
    }

    // ---- Device support & archives (informational) ----------------------------
    for (title, rel, description) in [
        (
            "iOS device support",
            "iOS DeviceSupport",
            "Symbols copied from devices, one folder per iOS version. Removable per version under Recommendations.",
        ),
        (
            "Archives",
            "Archives",
            "Release archives with debug symbols. Keep the ones for builds you still support.",
        ),
    ] {
        let base = xcode_dir.join(rel);
        let items: Vec<ToolItem> = subdirs(&base)
            .into_iter()
            .map(|d| ToolItem {
                label: name_of(&d),
                bytes: ctx.size(&d),
                path: Some(d.display().to_string()),
                detail: String::new(),
                state: ItemState::Info,
                last_used_ms: modified_ms(&d),
                recommendation_id: None,
            })
            .collect();
        if !items.is_empty() {
            roots.push((base, items.iter().map(|i| i.bytes).sum()));
            sections.push(ToolSection { title: title.into(), description: description.into(), items });
        }
    }

    if let Some(sims) = simulators(ctx) {
        let core = dev.join("CoreSimulator");
        roots.push((core.clone(), ctx.size(&core)));
        sections.push(sims);
    }

    let orphaned = sections
        .iter()
        .flat_map(|s| s.items.iter())
        .filter(|i| i.state == ItemState::Orphaned)
        .count();
    let summary = if !detected {
        "Xcode is not installed and no Xcode data was found.".into()
    } else if orphaned > 0 {
        format!("{orphaned} items belong to projects or runtimes that no longer exist.")
    } else {
        "No leftovers from deleted projects found.".into()
    };

    (
        ToolReport {
            id: "xcode".into(),
            name: "Xcode".into(),
            detected,
            version: app_version(&app),
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

    #[test]
    fn flags_derived_data_for_deleted_projects() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let dd = home.join("Library/Developer/Xcode/DerivedData");
        let live_proj = home.join("code/Live.xcodeproj");
        std::fs::create_dir_all(&live_proj).unwrap();
        for (name, ws) in [
            ("Live-abc", live_proj.display().to_string()),
            (
                "Gone-def",
                home.join("code/Gone.xcodeproj").display().to_string(),
            ),
        ] {
            write(&dd.join(name).join("Build/out.o"), 3000);
            let mut d = plist::Dictionary::new();
            d.insert("WorkspacePath".into(), plist::Value::String(ws));
            plist::Value::Dictionary(d)
                .to_file_xml(dd.join(name).join("info.plist"))
                .unwrap();
        }
        let (report, recs) = analyze(&ctx(&home));
        assert!(report.detected);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].path.ends_with("Gone-def"));
        assert_eq!(recs[0].tier, Tier::LikelySafe);
        let items = &report.sections[0].items;
        assert_eq!(
            items
                .iter()
                .filter(|i| i.state == ItemState::Orphaned)
                .count(),
            1
        );
    }

    #[test]
    fn parses_simctl_output() {
        let json = r#"{"devices":{
          "com.apple.CoreSimulator.SimRuntime.iOS-17-2":[
            {"name":"iPhone 15","udid":"A1","state":"Shutdown","isAvailable":true,"dataPath":"/x/A1/data","lastBootedAt":"2026-10-01T12:00:00Z"}],
          "com.apple.CoreSimulator.SimRuntime.iOS-16-0":[
            {"name":"iPhone 14","udid":"B2","state":"Shutdown","isAvailable":false,"availabilityError":"runtime profile not found","dataPath":"/x/B2/data"}]}}"#;
        let devices = parse_simctl(json).unwrap();
        assert_eq!(devices.len(), 2);
        assert!(devices
            .iter()
            .any(|(rt, d)| rt == "iOS 17.2" && d.is_available));
        assert!(devices
            .iter()
            .any(|(_, d)| !d.is_available && d.availability_error.is_some()));
        assert!(parse_iso8601_ms("2026-10-01T12:00:00Z").is_some());
    }
}
