//! Developer tool analysis (Phase 4).
//!
//! Each analyzer inspects one tool's on-disk layout and produces a report of
//! what it found (versions, per-project data, caches) plus recommendations for
//! items that are demonstrably unused: storage for projects that no longer
//! exist, superseded versions, and so on. Analyzers only read; anything they
//! recommend goes through the regular planner, safety gate and executor.

pub mod agent_data;
pub mod agents;
pub mod editors;
pub mod models;
pub mod node;
pub mod toolchains;
pub mod versions;
pub mod xcode;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::classifier::{Classifier, FsProbe};
use crate::model::{CleanupMethod, Recommendation, ScanSummary, Tier};
use crate::scanner::{measure, to_ms, DirIndex};

/// What an item's state means for cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemState {
    /// In use (the version that runs, a project that exists).
    Current,
    /// Exists and is used, but has not been touched for a long time.
    Stale,
    /// Belongs to something that no longer exists.
    Orphaned,
    /// Replaced by a newer version.
    Superseded,
    /// Informational.
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolItem {
    pub label: String,
    pub path: Option<String>,
    pub bytes: u64,
    pub detail: String,
    pub state: ItemState,
    pub last_used_ms: Option<i64>,
    /// Set when the item can be cleaned up through Recommendations.
    pub recommendation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSection {
    pub title: String,
    pub description: String,
    pub items: Vec<ToolItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReport {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub version: Option<String>,
    /// Storage attributed to the tool (outermost locations, counted once).
    pub total_bytes: u64,
    pub summary: String,
    pub sections: Vec<ToolSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevToolsReport {
    pub generated_at_ms: i64,
    pub tools: Vec<ToolReport>,
    /// Bytes in items the analyzers recommend reviewing.
    pub reclaimable_bytes: u64,
}

/// Inputs shared by all analyzers.
pub struct Ctx<'a> {
    pub home: &'a Path,
    pub index: Option<&'a DirIndex>,
    pub summary: Option<&'a ScanSummary>,
    pub classifier: Classifier,
    pub now_ms: i64,
}

pub const DAY_MS: i64 = 86_400_000;
pub const STALE_DAYS: i64 = 90;

impl Ctx<'_> {
    /// Size from the last scan when available, otherwise measured now.
    pub fn size(&self, path: &Path) -> u64 {
        if let Some(stats) = self.index.and_then(|i| i.get(path)) {
            return stats.bytes;
        }
        measure(path)
    }

    pub fn home_path(&self, rel: &str) -> PathBuf {
        self.home.join(rel)
    }

    pub fn days_since(&self, ms: i64) -> i64 {
        (self.now_ms - ms) / DAY_MS
    }

    /// Paths of locations the scan detected for a classifier rule.
    pub fn detected(&self, rule_id: &str) -> Vec<(PathBuf, u64)> {
        self.summary
            .map(|s| {
                s.detected_locations
                    .iter()
                    .filter(|l| l.classification.rule_id == rule_id)
                    .map(|l| (PathBuf::from(&l.path), l.bytes))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Read a small text file written by another tool. Anything larger than
/// `max` bytes is ignored rather than loaded into memory.
pub fn read_small(path: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Run a system tool (absolute path, no shell) and return its stdout if it
/// succeeds within `timeout`. A hung tool is killed instead of blocking the
/// analysis, and output is capped.
pub fn run_tool(program: &str, args: &[&str], timeout: std::time::Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    const MAX_OUTPUT: u64 = 32 * 1024 * 1024;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.by_ref().take(MAX_OUTPUT).read_to_string(&mut s);
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
        s
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(_) => return None,
        }
    };
    let out = reader.join().ok()?;
    status.success().then_some(out)
}

pub fn modified_ms(path: &Path) -> Option<i64> {
    path.symlink_metadata()
        .ok()?
        .modified()
        .ok()
        .and_then(to_ms)
}

pub fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Real (non-symlink) subdirectories of `dir`, sorted by name.
pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

pub fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Read `CFBundleShortVersionString` from an app bundle, if present.
pub fn app_version(app: &Path) -> Option<String> {
    let value = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    value
        .as_dictionary()?
        .get("CFBundleShortVersionString")?
        .as_string()
        .map(str::to_string)
}

/// Decode a `file://` URI into a path (percent-decoding included).
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// Builder for recommendations produced by analyzers.
pub struct RecSpec<'a> {
    pub source: &'a str,
    pub title: String,
    pub path: &'a Path,
    pub bytes: u64,
    pub tier: Tier,
    pub methods: &'a [CleanupMethod],
    pub detected_because: String,
    pub safe_because: &'a str,
    pub risks: &'a str,
    pub proposed_action: &'a str,
    pub alternative: Option<&'a str>,
    pub last_activity_ms: Option<i64>,
    pub notes: Vec<String>,
}

impl RecSpec<'_> {
    pub fn build(self, ctx: &Ctx) -> Recommendation {
        let is_dir = self.path.symlink_metadata().is_ok_and(|m| m.is_dir());
        let classification = ctx.classifier.classify_path(self.path, is_dir, &FsProbe);
        Recommendation {
            id: self.path.display().to_string(),
            title: self.title,
            path: self.path.display().to_string(),
            bytes: self.bytes,
            file_count: 0,
            tier: self.tier,
            confidence: classification.confidence,
            classification,
            detected_because: self.detected_because,
            safe_because: self.safe_because.to_string(),
            risks: self.risks.to_string(),
            proposed_action: self.proposed_action.to_string(),
            methods: self.methods.to_vec(),
            alternative: self.alternative.map(str::to_string),
            estimated_reclaim_bytes: self.bytes,
            last_activity_ms: self.last_activity_ms,
            notes: self.notes,
            source: self.source.to_string(),
            marker: None,
        }
    }
}

/// Run every analyzer.
pub fn analyze(ctx: &Ctx) -> (DevToolsReport, Vec<Recommendation>) {
    let mut tools = Vec::new();
    let mut recs = Vec::new();
    for (report, mut r) in [
        editors::cursor(ctx),
        editors::vscode(ctx),
        editors::windsurf(ctx),
        agent_data::claude_code(ctx),
        agent_data::claude_desktop(ctx),
        agent_data::codex(ctx),
        agent_data::copilot(ctx),
        agents::analyze(ctx),
        node::analyze(ctx),
        xcode::analyze(ctx),
        toolchains::rust(ctx),
        toolchains::python(ctx),
        models::analyze(ctx),
        versions::analyze(ctx),
    ] {
        tools.push(report);
        recs.append(&mut r);
    }
    tools.sort_by_key(|t| (!t.detected, std::cmp::Reverse(t.total_bytes)));
    let reclaimable_bytes = recs.iter().map(|r| r.estimated_reclaim_bytes).sum();
    (
        DevToolsReport {
            generated_at_ms: ctx.now_ms,
            tools,
            reclaimable_bytes,
        },
        recs,
    )
}

/// Sum sizes of paths, ignoring any path nested inside another.
pub fn outermost_total(items: &[(PathBuf, u64)]) -> u64 {
    items
        .iter()
        .filter(|(p, _)| !items.iter().any(|(q, _)| p != q && p.starts_with(q)))
        .map(|(_, b)| b)
        .sum()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn ctx(home: &Path) -> Ctx<'_> {
        Ctx {
            home,
            index: None,
            summary: None,
            classifier: Classifier::new(home).with_floor(home),
            now_ms: 1_800_000_000_000,
        }
    }

    pub fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![1u8; bytes]).unwrap();
    }

    #[test]
    fn decodes_file_uris() {
        assert_eq!(
            file_uri_to_path("file:///Users/a/My%20Project").unwrap(),
            PathBuf::from("/Users/a/My Project")
        );
        assert!(file_uri_to_path("vscode-remote://ssh-remote+host/x").is_none());
    }

    #[test]
    fn outermost_total_skips_nested() {
        let items = vec![
            (PathBuf::from("/a"), 10),
            (PathBuf::from("/a/b"), 5),
            (PathBuf::from("/c"), 1),
        ];
        assert_eq!(outermost_total(&items), 11);
    }
}
