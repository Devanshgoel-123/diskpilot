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
pub mod go;
pub mod models;
pub mod node;
pub mod toolchains;
pub mod versions;
/// Xcode exists only on macOS.
#[cfg(not(windows))]
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

/// The folder that holds apps' data, relative to the home folder:
/// Application Support on macOS, `%APPDATA%` on Windows.
pub const APP_DATA: &str = if cfg!(windows) {
    "AppData/Roaming"
} else {
    "Library/Application Support"
};

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

    /// A location named relative to the home folder. On Windows, names that
    /// start with `AppData/Roaming` or `AppData/Local` resolve to the
    /// profile's `%APPDATA%` and `%LOCALAPPDATA%` folders, and `/` becomes
    /// `\`, so one table entry serves both the default and a redirected
    /// profile.
    pub fn home_path(&self, rel: &str) -> PathBuf {
        #[cfg(windows)]
        {
            let (roaming, local) = windows_app_data(self.home);
            resolve_app_data(rel, &roaming, &local)
                .unwrap_or_else(|| self.home.join(rel.replace('/', "\\")))
        }
        #[cfg(not(windows))]
        self.home.join(rel)
    }

    /// A folder named by an environment variable (`NVM_HOME`, `SCOOP`,
    /// `ANDROID_HOME`), only when analyzing this user's own home folder: the
    /// variable describes the person running DiskPilot, not a test fixture
    /// or another profile.
    pub fn env_path(&self, var: &str) -> Option<PathBuf> {
        if dirs::home_dir().as_deref() != Some(self.home) {
            return None;
        }
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
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

#[cfg_attr(not(windows), allow(dead_code))]
/// Map a home-relative `AppData/Roaming/...` or `AppData/Local/...` name to
/// the given roaming and local folders. `None` for any other name.
pub fn resolve_app_data(rel: &str, roaming: &Path, local: &Path) -> Option<PathBuf> {
    let rel = rel.replace('\\', "/");
    let (base, rest) = if let Some(rest) = rel.strip_prefix("AppData/Roaming") {
        (roaming, rest)
    } else {
        (local, rel.strip_prefix("AppData/Local")?)
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return None;
    }
    let mut out = base.to_path_buf();
    out.extend(rest.split('/').filter(|c| !c.is_empty()));
    Some(out)
}

/// `%APPDATA%` and `%LOCALAPPDATA%` for `home`: the known folders when `home`
/// is this user's profile, otherwise the default layout under `home` (tests
/// and other profiles).
#[cfg(windows)]
pub fn windows_app_data(home: &Path) -> (PathBuf, PathBuf) {
    use std::sync::OnceLock;
    static REAL: OnceLock<Option<(PathBuf, PathBuf, PathBuf)>> = OnceLock::new();
    let real = REAL.get_or_init(|| {
        Some((
            dirs::home_dir()?,
            dirs::config_dir()?,
            dirs::data_local_dir()?,
        ))
    });
    match real {
        Some((h, roaming, local)) if h == home => (roaming.clone(), local.clone()),
        _ => (
            home.join("AppData").join("Roaming"),
            home.join("AppData").join("Local"),
        ),
    }
}

/// A Windows location from a path table: `%VAR%/rest` starts at an
/// environment variable such as `%ProgramFiles%`; anything else is relative
/// to the home folder (see [`Ctx::home_path`]). An unset variable gives an
/// empty path, which never exists.
pub fn win_location(ctx: &Ctx, entry: &str) -> PathBuf {
    match split_env(entry) {
        Some((var, rest)) => std::env::var_os(var)
            .map(|base| {
                let mut out = PathBuf::from(base);
                out.extend(rest.split('/').filter(|c| !c.is_empty()));
                out
            })
            .unwrap_or_default(),
        None => ctx.home_path(entry),
    }
}

/// `("ProgramFiles", "Docker/x")` for `"%ProgramFiles%/Docker/x"`.
pub fn split_env(entry: &str) -> Option<(&str, &str)> {
    let (var, rest) = entry.strip_prefix('%')?.split_once('%')?;
    (!var.is_empty()).then_some((var, rest))
}

/// Keep a child process from opening a console window on Windows. GUI apps
/// have no console, so every console program would otherwise flash one.
pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
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
    let mut command = std::process::Command::new(program);
    command.args(args);
    run_command(command, timeout)
}

/// [`run_tool`] for a command that also needs its environment or working
/// folder set.
pub fn run_command(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    const MAX_OUTPUT: u64 = 32 * 1024 * 1024;
    hide_console(&mut command);
    let mut child = command
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

/// The installed version of an app: `CFBundleShortVersionString` from a
/// macOS app bundle, or the `version` an Electron app on Windows keeps in
/// `resources\app\package.json`.
pub fn app_version(app: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        electron_version(&read_small(
            &app.join("resources").join("app").join("package.json"),
            1024 * 1024,
        )?)
    }
    #[cfg(not(windows))]
    bundle_version(app)
}

/// The `version` field of an Electron app's `package.json`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn electron_version(package_json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Package {
        version: Option<String>,
    }
    serde_json::from_str::<Package>(package_json)
        .ok()?
        .version
        .filter(|v| !v.is_empty())
}

#[cfg(not(windows))]
fn bundle_version(app: &Path) -> Option<String> {
    let value = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    value
        .as_dictionary()?
        .get("CFBundleShortVersionString")?
        .as_string()
        .map(str::to_string)
}

/// Decode a `file://` URI into a path (percent-decoding included). On
/// Windows, `file:///c%3A/Users/a` becomes `C:\Users\a` and
/// `file://server/share` a UNC path.
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let decoded = decode_file_uri(uri)?;
    Some(PathBuf::from(if cfg!(windows) {
        windows_uri_path(&decoded)
    } else {
        decoded
    }))
}

/// The Windows path for a decoded `file://` URI body.
pub fn windows_uri_path(decoded: &str) -> String {
    let b = decoded.as_bytes();
    // "/c:/x" -> "C:\x"
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        let mut out = String::with_capacity(decoded.len());
        out.push(b[1].to_ascii_uppercase() as char);
        out.push_str(&decoded[2..].replace('/', "\\"));
        if out.len() == 2 {
            out.push('\\');
        }
        return out;
    }
    // "server/share/x" (the authority part of file://server/share) -> UNC.
    if !decoded.starts_with('/') && !decoded.is_empty() {
        return format!("\\\\{}", decoded.replace('/', "\\"));
    }
    decoded.replace('/', "\\")
}

/// Percent-decode the part of a `file://` URI after the scheme.
fn decode_file_uri(uri: &str) -> Option<String> {
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
    String::from_utf8(out).ok()
}

/// Analyzer text is written for macOS; on Windows the Trash is the Recycle
/// Bin and the computer is a PC.
pub fn local_words(text: &str) -> String {
    if cfg!(windows) {
        windows_words(text)
    } else {
        text.to_string()
    }
}

/// `text` with macOS terms replaced by their Windows names.
pub fn windows_words(text: &str) -> String {
    const WORDS: &[(&str, &str)] = &[
        ("Trash", "Recycle Bin"),
        ("this Mac", "this PC"),
        ("your Mac", "your PC"),
        ("macOS", "Windows"),
        ("Finder", "File Explorer"),
        ("A disk image or package", "An installer"),
        ("installed in Applications", "installed"),
        ("was found in Applications", "is installed"),
    ];
    let mut out = text.to_string();
    for (mac, windows) in WORDS {
        if out.contains(mac) {
            out = out.replace(mac, windows);
        }
    }
    out
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
            detected_because: local_words(&self.detected_because),
            safe_because: local_words(self.safe_because),
            risks: local_words(self.risks),
            proposed_action: local_words(self.proposed_action),
            methods: self.methods.to_vec(),
            alternative: self.alternative.map(local_words),
            estimated_reclaim_bytes: self.bytes,
            last_activity_ms: self.last_activity_ms,
            notes: self.notes.iter().map(|n| local_words(n)).collect(),
            source: self.source.to_string(),
            marker: None,
            clear_in_app: None,
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
        #[cfg(not(windows))]
        xcode::analyze(ctx),
        toolchains::rust(ctx),
        toolchains::python(ctx),
        go::analyze(ctx),
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
    fn decodes_windows_file_uris() {
        assert_eq!(
            windows_uri_path(&decode_file_uri("file:///c%3A/Users/a/My%20Project").unwrap()),
            "C:\\Users\\a\\My Project"
        );
        assert_eq!(windows_uri_path("/d:"), "D:\\");
        assert_eq!(
            windows_uri_path(&decode_file_uri("file://server/share/x").unwrap()),
            "\\\\server\\share\\x"
        );
    }

    #[test]
    fn maps_app_data_names_to_known_folders() {
        let roaming = Path::new("/r");
        let local = Path::new("/l");
        assert_eq!(
            resolve_app_data("AppData/Roaming/Code/User", roaming, local).unwrap(),
            Path::new("/r").join("Code").join("User")
        );
        assert_eq!(
            resolve_app_data("AppData/Local", roaming, local).unwrap(),
            Path::new("/l")
        );
        assert_eq!(
            resolve_app_data("AppData\\Local\\go-build", roaming, local).unwrap(),
            Path::new("/l").join("go-build")
        );
        assert!(resolve_app_data("AppData/LocalLow/x", roaming, local).is_none());
        assert!(resolve_app_data(".cursor/extensions", roaming, local).is_none());
    }

    #[test]
    fn splits_environment_prefixes() {
        assert_eq!(
            split_env("%ProgramFiles%/Docker/x"),
            Some(("ProgramFiles", "/Docker/x"))
        );
        assert_eq!(split_env("AppData/Local"), None);
        assert_eq!(split_env("%%/x"), None);
    }

    #[test]
    fn reads_electron_versions_and_windows_wording() {
        assert_eq!(
            electron_version(r#"{"name":"code","version":"1.95.3"}"#).as_deref(),
            Some("1.95.3")
        );
        assert!(electron_version("{}").is_none());
        assert_eq!(
            windows_words("Move the old version to the Trash"),
            "Move the old version to the Recycle Bin"
        );
        assert_eq!(
            windows_words("Folders for X 2 are also on this Mac."),
            "Folders for X 2 are also on this PC."
        );
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
