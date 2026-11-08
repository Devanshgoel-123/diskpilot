//! Which installed application a folder belongs to, and that application's
//! own icon.
//!
//! Icons are rendered from the apps installed on this computer (through
//! `NSWorkspace` on macOS, the shell on Windows), so DiskPilot ships no
//! third-party logos. Resolution is pure path matching against an index of
//! installed apps: a path received from the UI is never read, only compared.
//! On Windows the index is built from the registry (see `apps_windows.rs`).

use std::collections::HashMap;
#[cfg(not(windows))]
use std::path::Component;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

#[cfg(any(windows, test))]
#[path = "apps_windows.rs"]
pub mod win;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    /// The bundle id on macOS. On Windows, the program's Uninstall key name
    /// or a Store app's package family name.
    pub bundle_id: String,
    /// The app bundle on macOS. On Windows, the install folder, or the
    /// registry entry's path when the program registered no folder.
    pub path: PathBuf,
    #[cfg(windows)]
    pub win: win::WinApp,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBadge {
    pub name: String,
    /// PNG data URL.
    pub icon: String,
}

/// Folders under `~/Library` whose first child is named after an app's
/// bundle id or name.
#[cfg_attr(windows, allow(dead_code))]
const LIBRARY_DIRS: &[&str] = &[
    "Application Support",
    "Caches",
    "Containers",
    "Group Containers",
    "Logs",
    "HTTPStorages",
    "WebKit",
    "Saved Application State",
    "Preferences",
    "Application Scripts",
    "Cookies",
];

/// Folders whose name does not match the app that owns them, as
/// (path relative to home, lowercase, bundle id).
#[cfg_attr(windows, allow(dead_code))]
const ALIASES: &[(&str, &str)] = &[
    (
        "library/application support/google/chrome",
        "com.google.chrome",
    ),
    ("library/caches/google/chrome", "com.google.chrome"),
    (
        "library/application support/bravesoftware/brave-browser",
        "com.brave.browser",
    ),
    (
        "library/caches/bravesoftware/brave-browser",
        "com.brave.browser",
    ),
    (
        "library/application support/microsoft edge",
        "com.microsoft.edgemac",
    ),
    ("library/application support/firefox", "org.mozilla.firefox"),
    ("library/caches/firefox", "org.mozilla.firefox"),
    ("library/application support/code", "com.microsoft.vscode"),
    (
        "library/application support/arc",
        "company.thebrowser.browser",
    ),
    (
        "library/application support/docker desktop",
        "com.docker.docker",
    ),
    ("library/developer/xcode", "com.apple.dt.xcode"),
    ("library/developer/coresimulator", "com.apple.dt.xcode"),
    (".cursor", "com.todesktop.230313mzl4w4u92"),
    (".vscode", "com.microsoft.vscode"),
    (".claude", "com.anthropic.claudefordesktop"),
    (".local/share/claude", "com.anthropic.claudefordesktop"),
    (".codex", "com.openai.codex"),
    (".docker", "com.docker.docker"),
    (".orbstack", "dev.kdrag0n.macvirt"),
];

#[derive(Default)]
pub struct AppIndex {
    apps: Vec<AppInfo>,
    by_key: HashMap<String, usize>,
    /// Whether the list of Store apps could be read (Windows).
    #[cfg(windows)]
    store_known: bool,
}

/// Drop invisible direction marks some apps put in their names.
pub(crate) fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(not(windows))]
fn plist_string(dict: &plist::Dictionary, key: &str) -> Option<String> {
    dict.get(key)?.as_string().map(str::to_string)
}

impl AppIndex {
    pub fn add(&mut self, app: AppInfo) {
        let i = self.apps.len();
        #[allow(unused_mut)]
        let mut keys = vec![
            app.bundle_id.to_lowercase(),
            app.name.to_lowercase(),
            app.path
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
        ];
        #[cfg(windows)]
        keys.extend(win::extra_keys(&app));
        for key in keys {
            if !key.is_empty() {
                self.by_key.entry(key).or_insert(i);
            }
        }
        self.apps.push(app);
    }

    #[cfg(not(windows))]
    fn read_bundle(path: &Path) -> Option<AppInfo> {
        let value = plist::Value::from_file(path.join("Contents/Info.plist")).ok()?;
        let dict = value.as_dictionary()?;
        let bundle_id = plist_string(dict, "CFBundleIdentifier")?;
        let name = plist_string(dict, "CFBundleDisplayName")
            .or_else(|| plist_string(dict, "CFBundleName"))
            .map(|n| clean_name(&n))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        Some(AppInfo {
            name,
            bundle_id,
            path: path.to_path_buf(),
        })
    }

    /// Index the installed programs and Store apps.
    #[cfg(windows)]
    pub fn build(home: &Path) -> Self {
        win::build_index(home)
    }

    /// Index the standard application folders, one level of subfolders deep.
    #[cfg(not(windows))]
    pub fn build(home: &Path) -> Self {
        let mut index = Self::default();
        let roots = [
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            home.join("Applications"),
        ];
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            let mut found: Vec<PathBuf> = Vec::new();
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|e| e == "app") {
                    found.push(p);
                } else if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    if let Ok(inner) = std::fs::read_dir(&p) {
                        found.extend(
                            inner
                                .flatten()
                                .map(|e| e.path())
                                .filter(|p| p.extension().is_some_and(|e| e == "app")),
                        );
                    }
                }
            }
            found.sort();
            for p in found {
                if let Some(app) = Self::read_bundle(&p) {
                    index.add(app);
                }
            }
        }
        index
    }

    #[cfg(not(windows))]
    fn by_bundle_prefix(&self, key: &str) -> Option<&AppInfo> {
        let key = key.to_lowercase();
        self.apps
            .iter()
            .filter(|a| key.starts_with(&format!("{}.", a.bundle_id.to_lowercase())))
            .max_by_key(|a| a.bundle_id.len())
    }

    /// Whether an installed app shares the vendor part (`com.vendor`) of a
    /// bundle id. Used to avoid calling a vendor's helper data a leftover.
    #[cfg(not(windows))]
    fn shares_vendor(&self, bundle_like: &str) -> bool {
        let vendor: Vec<&str> = bundle_like.split('.').take(2).collect();
        if vendor.len() < 2 {
            return false;
        }
        let prefix = format!("{}.", vendor.join(".").to_lowercase());
        self.apps
            .iter()
            .any(|a| a.bundle_id.to_lowercase().starts_with(&prefix))
    }

    fn lookup(&self, key: &str) -> Option<&AppInfo> {
        self.by_key.get(&key.to_lowercase()).map(|&i| &self.apps[i])
    }

    /// The installed app a path belongs to, if any.
    #[cfg(windows)]
    pub fn resolve(&self, home: &Path, path: &Path) -> Option<&AppInfo> {
        self.resolve_windows(home, path)
    }

    /// The installed app a path belongs to, if any.
    #[cfg(not(windows))]
    pub fn resolve(&self, home: &Path, path: &Path) -> Option<&AppInfo> {
        // Inside an app bundle that is in the index.
        if let Some(app) = self.apps.iter().find(|a| path.starts_with(&a.path)) {
            return Some(app);
        }
        let rel = path.strip_prefix(home).ok()?;
        let parts: Vec<String> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let lower = parts.join("/").to_lowercase();
        for (prefix, id) in ALIASES {
            if lower == *prefix || lower.starts_with(&format!("{prefix}/")) {
                if let Some(app) = self.lookup(id) {
                    return Some(app);
                }
            }
        }
        if parts.len() >= 3 && parts[0] == "Library" && LIBRARY_DIRS.contains(&parts[1].as_str()) {
            let mut key = parts[2].as_str();
            for suffix in [".ShipIt", ".savedState", ".plist", ".binarycookies"] {
                key = key.strip_suffix(suffix).unwrap_or(key);
            }
            if let Some(app) = self.lookup(key) {
                return Some(app);
            }
            // Helpers and extensions: com.vendor.App.helper belongs to com.vendor.App.
            if let Some(app) = self.by_bundle_prefix(key) {
                return Some(app);
            }
            // Group containers are prefixed with a team id: TEAMID.com.vendor.app
            if parts[1] == "Group Containers" {
                if let Some((_, rest)) = key.split_once('.') {
                    return self.lookup(rest);
                }
            }
        }
        None
    }
}

/// Names of the installed apps (from the standard application folders).
pub fn installed_names(home: &Path) -> Vec<String> {
    index(home).apps.iter().map(|a| a.name.clone()).collect()
}

static INDEX: Mutex<Option<Arc<AppIndex>>> = Mutex::new(None);

/// The index of installed apps, built on first use.
fn index(home: &Path) -> Arc<AppIndex> {
    let mut slot = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    slot.get_or_insert_with(|| Arc::new(AppIndex::build(home)))
        .clone()
}

/// Rebuild the index, so programs installed or uninstalled since are seen.
/// Reading the registry takes milliseconds.
#[cfg(windows)]
fn refresh_index(home: &Path) -> Arc<AppIndex> {
    let fresh = Arc::new(AppIndex::build(home));
    *INDEX.lock().unwrap_or_else(|e| e.into_inner()) = Some(fresh.clone());
    fresh
}

/// Render an app's icon as PNG data, `px` pixels square.
#[cfg(target_os = "macos")]
pub fn icon_png(app: &Path, px: f64) -> Option<Vec<u8>> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    objc2::rc::autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let image = workspace.iconForFile(&NSString::from_str(app.to_str()?));
        image.setSize(NSSize::new(px, px));
        let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(px, px));
        // SAFETY: `rect` is a valid, live NSRect; no context or hints are passed.
        let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
        let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
        // SAFETY: an empty properties dictionary is valid for PNG output.
        let data = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        Some(data.to_vec())
    })
}

#[cfg(not(target_os = "macos"))]
pub fn icon_png(_app: &Path, _px: f64) -> Option<Vec<u8>> {
    None
}

/// An installed app's icon, 64 pixels square.
fn app_icon_png(app: &AppInfo) -> Option<Vec<u8>> {
    #[cfg(windows)]
    return win::icon_png(app, 64);
    #[cfg(not(windows))]
    icon_png(&app.path, 64.0)
}

/// Maximum number of paths resolved per request.
pub const MAX_PATHS: usize = 500;

/// Resolve paths to app badges (name and icon). Icons are rendered once per
/// app and cached for the life of the process.
pub fn badges(home: &Path, paths: &[String]) -> HashMap<String, AppBadge> {
    static ICONS: OnceLock<Mutex<HashMap<PathBuf, Option<String>>>> = OnceLock::new();
    let icons = ICONS.get_or_init(Default::default);
    let index = index(home);
    let mut out = HashMap::new();
    for raw in paths.iter().take(MAX_PATHS) {
        let Some(app) = index.resolve(home, Path::new(raw)) else {
            continue;
        };
        let icon = {
            let mut cache = icons.lock().unwrap_or_else(|e| e.into_inner());
            cache
                .entry(app.path.clone())
                .or_insert_with(|| {
                    use base64::Engine;
                    app_icon_png(app).map(|png| {
                        format!(
                            "data:image/png;base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(png)
                        )
                    })
                })
                .clone()
        };
        if let Some(icon) = icon {
            out.insert(
                raw.clone(),
                AppBadge {
                    name: app.name.clone(),
                    icon,
                },
            );
        }
    }
    out
}

/// Replace a bundle identifier at the end of a title ("Application caches ·
/// com.tinyspeck.slackmacgap") with the installed app's name ("Slack").
pub fn name_apps_in_titles(home: &Path, recs: &mut [crate::model::Recommendation]) {
    name_apps_with(&index(home), home, recs);
}

fn name_apps_with(index: &AppIndex, home: &Path, recs: &mut [crate::model::Recommendation]) {
    for rec in recs.iter_mut() {
        let Some((head, tail)) = rec.title.rsplit_once(" · ") else {
            continue;
        };
        if !tail.contains('.') {
            continue;
        }
        if let Some(app) = index.resolve(home, Path::new(&rec.path)) {
            rec.title = format!("{head} · {}", app.name);
        }
    }
}

// ---- Storage per application ----------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDataLocation {
    pub path: String,
    /// The Library folder it lives in, e.g. "Caches".
    pub kind: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFootprint {
    pub name: String,
    pub bundle_id: String,
    pub app_path: String,
    /// Size of the app bundle; `None` for apps on the read-only system volume.
    pub app_bytes: Option<u64>,
    pub data_bytes: u64,
    pub data_modified_ms: Option<i64>,
    pub locations: Vec<AppDataLocation>,
    /// The publisher, on Windows; `None` on macOS.
    pub publisher: Option<String>,
    /// The installed version, on Windows; `None` on macOS.
    pub version: Option<String>,
    /// Windows: the exact command Uninstall starts (the program's own
    /// uninstaller), shown before it runs. `None` when Uninstall opens
    /// Settings > Apps instead (Store apps, unusual entries), and on macOS.
    pub uninstall_command: Option<String>,
}

/// What Uninstall did on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UninstallOutcome {
    /// The program's own uninstaller started; it asks its own questions.
    Uninstaller,
    /// Settings > Apps > Installed apps opened.
    Settings,
}

/// Start an installed program's own uninstaller (Windows), after the person
/// confirmed. `id` is the `bundleId` from [`app_storage`]; the command is
/// read from the registry again, never taken from the window. DiskPilot
/// never deletes a program's folder itself.
pub fn uninstall(id: &str) -> Result<UninstallOutcome, String> {
    #[cfg(windows)]
    return win::uninstall(id);
    #[cfg(not(windows))]
    {
        let _ = id;
        Err("DiskPilot does not uninstall apps on this system.".into())
    }
}

/// Data in `~/Library` named for an app that is not installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub bytes: u64,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStorageReport {
    pub apps: Vec<AppFootprint>,
    pub leftovers: Vec<Leftover>,
    /// False when there is no scan covering `~/Library`; data sizes are then 0.
    pub data_scanned: bool,
}

/// Library folders whose bundle-named children only installed apps create.
#[cfg_attr(windows, allow(dead_code))]
const LEFTOVER_DIRS: &[&str] = &["Containers", "Group Containers"];

/// Leftovers smaller than this are not listed.
pub(crate) const MIN_LEFTOVER_BYTES: u64 = 1_000_000;

/// A reverse-DNS name such as `com.vendor.app`, not owned by Apple.
#[cfg_attr(windows, allow(dead_code))]
fn bundle_like(name: &str) -> Option<String> {
    let mut key = name;
    for suffix in [".ShipIt", ".savedState", ".plist", ".binarycookies"] {
        key = key.strip_suffix(suffix).unwrap_or(key);
    }
    let parts: Vec<&str> = key.split('.').collect();
    let valid = parts.len() >= 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        });
    let lower = key.to_lowercase();
    (valid && !lower.starts_with("com.apple.") && !lower.starts_with("group.com.apple."))
        .then(|| key.to_string())
}

pub(crate) fn size_of(path: &Path, index: &crate::scanner::DirIndex) -> Option<u64> {
    let meta = path.symlink_metadata().ok()?;
    if meta.is_dir() {
        index.get(path).map(|s| s.bytes)
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(meta.blocks() * 512)
        }
        #[cfg(not(unix))]
        Some(meta.len())
    }
}

pub(crate) fn bundle_bytes(path: &Path) -> Option<u64> {
    type Measured = (Option<std::time::SystemTime>, u64);
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Measured>>> = OnceLock::new();
    if path.starts_with("/System") {
        return None;
    }
    let mtime = path.metadata().and_then(|m| m.modified()).ok();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((t, b)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(path) {
        if *t == mtime {
            return Some(*b);
        }
    }
    let bytes = crate::scanner::measure(path);
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.to_path_buf(), (mtime, bytes));
    Some(bytes)
}

/// One app's row: its data folders, largest first, and their latest change.
fn footprint(
    app: &AppInfo,
    mut locations: Vec<AppDataLocation>,
    app_bytes: Option<u64>,
) -> AppFootprint {
    locations.sort_by_key(|l| std::cmp::Reverse(l.bytes));
    let data_modified_ms = locations
        .iter()
        .filter_map(|l| {
            Path::new(&l.path)
                .symlink_metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(crate::scanner::to_ms)
        })
        .max();
    #[cfg(windows)]
    let (publisher, version, uninstall_command) = (
        app.win.publisher.clone(),
        app.win.version.clone(),
        app.win.uninstaller.as_ref().and_then(|u| u.command()),
    );
    #[cfg(not(windows))]
    let (publisher, version, uninstall_command) = (None, None, None);
    AppFootprint {
        name: app.name.clone(),
        bundle_id: app.bundle_id.clone(),
        app_path: app.path.display().to_string(),
        app_bytes,
        data_bytes: locations.iter().map(|l| l.bytes).sum(),
        data_modified_ms,
        locations,
        publisher,
        version,
        uninstall_command,
    }
}

/// Storage per installed application: its bundle plus its folders in
/// `~/Library`, and data left behind by apps that are no longer installed.
/// Data sizes come from the scan index; nothing is modified.
#[cfg(windows)]
pub fn app_storage(home: &Path, scan: Option<&crate::scanner::DirIndex>) -> AppStorageReport {
    win::app_storage(home, scan)
}

/// Storage per installed application: its bundle plus its folders in
/// `~/Library`, and data left behind by apps that are no longer installed.
/// Data sizes come from the scan index; nothing is modified.
#[cfg(not(windows))]
pub fn app_storage(home: &Path, scan: Option<&crate::scanner::DirIndex>) -> AppStorageReport {
    use rayon::prelude::*;

    let index = index(home);
    let library = home.join("Library");
    let data_scanned = scan.is_some_and(|i| i.contains_key(&library));
    let empty = crate::scanner::DirIndex::new();
    let scan = scan.unwrap_or(&empty);

    let mut per_app: HashMap<PathBuf, Vec<AppDataLocation>> = HashMap::new();
    let mut leftovers = Vec::new();
    if data_scanned {
        for dir in LIBRARY_DIRS {
            let Ok(entries) = std::fs::read_dir(library.join(dir)) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            // Vendor folders such as Application Support/Google hold one app
            // each per subfolder (Google/Chrome); look one level deeper there.
            let mut nested = Vec::new();
            paths.retain(|p| {
                let rel = p
                    .strip_prefix(home)
                    .map(|r| r.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let vendor = index.resolve(home, p).is_none()
                    && ALIASES
                        .iter()
                        .any(|(a, _)| a.starts_with(&format!("{rel}/")));
                if vendor {
                    if let Ok(inner) = std::fs::read_dir(p) {
                        nested.extend(inner.flatten().map(|e| e.path()));
                    }
                }
                !vendor
            });
            paths.extend(nested);
            for path in paths {
                let Some(bytes) = size_of(&path, scan) else {
                    continue;
                };
                if bytes == 0 {
                    continue;
                }
                if let Some(app) = index.resolve(home, &path) {
                    per_app
                        .entry(app.path.clone())
                        .or_default()
                        .push(AppDataLocation {
                            path: path.display().to_string(),
                            kind: (*dir).to_string(),
                            bytes,
                        });
                    continue;
                }
                // Only app sandbox containers count as leftovers: they are
                // created by installed apps alone, while caches and support
                // folders with bundle-style names often belong to CLI tools.
                if !LEFTOVER_DIRS.contains(dir) {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let candidate = if *dir == "Group Containers" {
                    name.split_once('.').and_then(|(_, rest)| bundle_like(rest))
                } else {
                    bundle_like(&name)
                };
                if let Some(id) = candidate {
                    if bytes >= MIN_LEFTOVER_BYTES && !index.shares_vendor(&id) {
                        leftovers.push(Leftover {
                            name: id,
                            path: path.display().to_string(),
                            kind: (*dir).to_string(),
                            bytes,
                            modified_ms: path
                                .symlink_metadata()
                                .ok()
                                .and_then(|m| m.modified().ok())
                                .and_then(crate::scanner::to_ms),
                        });
                    }
                }
            }
        }
    }

    let mut apps: Vec<AppFootprint> = index
        .apps
        .par_iter()
        .map(|app| {
            footprint(
                app,
                per_app.get(&app.path).cloned().unwrap_or_default(),
                bundle_bytes(&app.path),
            )
        })
        // System apps without data of their own are not interesting here.
        .filter(|a| a.app_bytes.is_some() || a.data_bytes > 0)
        .collect();
    apps.sort_by_key(|a| std::cmp::Reverse(a.app_bytes.unwrap_or(0) + a.data_bytes));
    leftovers.sort_by_key(|l| std::cmp::Reverse(l.bytes));
    AppStorageReport {
        apps,
        leftovers,
        data_scanned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // macOS paths and bundle ids; Windows has its own tests in apps_windows.rs.
    #[cfg(not(windows))]
    fn app(name: &str, id: &str, path: &str) -> AppInfo {
        AppInfo {
            name: name.into(),
            bundle_id: id.into(),
            path: PathBuf::from(path),
            #[cfg(windows)]
            win: Default::default(),
        }
    }

    // macOS paths and bundle ids; Windows has its own tests in apps_windows.rs.
    #[cfg(not(windows))]
    fn sample() -> AppIndex {
        let mut i = AppIndex::default();
        i.add(app(
            "Google Chrome",
            "com.google.Chrome",
            "/Applications/Google Chrome.app",
        ));
        i.add(app(
            "Cursor",
            "com.todesktop.230313mzl4w4u92",
            "/Applications/Cursor.app",
        ));
        i.add(app(
            "Stocks",
            "com.apple.stocks",
            "/System/Applications/Stocks.app",
        ));
        i.add(app(
            "Slack",
            "com.tinyspeck.slackmacgap",
            "/Applications/Slack.app",
        ));
        i.add(app(
            "Brave Browser",
            "com.brave.Browser",
            "/Applications/Brave Browser.app",
        ));
        i
    }

    // macOS paths and bundle ids; Windows has its own tests in apps_windows.rs.
    #[cfg(not(windows))]
    #[test]
    fn resolves_library_folders_by_name_bundle_id_and_alias() {
        let i = sample();
        let home = Path::new("/Users/me");
        let name = |p: &str| i.resolve(home, &home.join(p)).map(|a| a.name.clone());
        assert_eq!(
            name("Library/Application Support/Cursor/User").as_deref(),
            Some("Cursor")
        );
        assert_eq!(
            name("Library/Caches/com.todesktop.230313mzl4w4u92.ShipIt").as_deref(),
            Some("Cursor")
        );
        assert_eq!(name(".cursor/extensions").as_deref(), Some("Cursor"));
        assert_eq!(
            name("Library/Containers/com.apple.stocks/Data").as_deref(),
            Some("Stocks")
        );
        assert_eq!(
            name("Library/Application Support/Google/Chrome/Default").as_deref(),
            Some("Google Chrome")
        );
        assert_eq!(
            name("Library/Application Support/BraveSoftware/Brave-Browser").as_deref(),
            Some("Brave Browser")
        );
        assert_eq!(
            name("Library/Application Support/Slack").as_deref(),
            Some("Slack")
        );
        assert_eq!(
            name("Library/Preferences/com.tinyspeck.slackmacgap.plist").as_deref(),
            Some("Slack")
        );
        assert_eq!(
            name("Library/Group Containers/ABCDE12345.com.tinyspeck.slackmacgap").as_deref(),
            Some("Slack")
        );
    }

    // macOS paths and bundle ids; Windows has its own tests in apps_windows.rs.
    #[cfg(not(windows))]
    #[test]
    fn helpers_resolve_to_their_app_and_vendor_data_is_not_a_leftover() {
        let i = sample();
        let home = Path::new("/Users/me");
        assert_eq!(
            i.resolve(
                home,
                &home.join("Library/Caches/com.tinyspeck.slackmacgap.helper")
            )
            .map(|a| a.name.as_str()),
            Some("Slack")
        );
        assert!(i.shares_vendor("com.google.Keystone"));
        assert!(!i.shares_vendor("com.microsoft.Excel"));
    }

    #[test]
    fn recognizes_bundle_like_names() {
        assert_eq!(
            bundle_like("com.microsoft.Excel").as_deref(),
            Some("com.microsoft.Excel")
        );
        assert_eq!(
            bundle_like("com.vendor.app.plist").as_deref(),
            Some("com.vendor.app")
        );
        assert!(bundle_like("com.apple.Safari").is_none());
        assert!(bundle_like("Google").is_none());
        assert!(bundle_like("node.js").is_none());
    }

    // macOS paths and bundle ids; Windows has its own tests in apps_windows.rs.
    #[cfg(not(windows))]
    #[test]
    fn app_storage_attributes_library_folders_and_finds_leftovers() {
        let dir = crate::test_support::sandbox();
        let home = crate::platform::canonicalize(dir.path()).unwrap();
        let mut scan = crate::scanner::DirIndex::new();
        let stats = |bytes| crate::scanner::DirStats {
            bytes,
            files: 1,
            dirs: 0,
            largest_child_dir: 0,
            direct_bytes: bytes,
            readable: true,
        };
        let lib = home.join("Library");
        for (rel, bytes) in [
            ("Containers/com.example.gone", 5_000_000u64),
            ("Caches/com.example.tiny", 10),
            ("Caches/com.apple.Safari", 9_000_000),
        ] {
            std::fs::create_dir_all(lib.join(rel)).unwrap();
            scan.insert(lib.join(rel), stats(bytes));
        }
        scan.insert(lib.clone(), stats(14_000_010));
        let report = app_storage(&home, Some(&scan));
        assert!(report.data_scanned);
        let names: Vec<&str> = report.leftovers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["com.example.gone"]);

        let unscanned = app_storage(&home, None);
        assert!(!unscanned.data_scanned);
        assert!(unscanned.leftovers.is_empty());
    }

    // macOS paths and bundle ids; Windows has its own tests in apps_windows.rs.
    #[cfg(not(windows))]
    #[test]
    fn names_installed_apps_in_titles() {
        let i = sample();
        let home = Path::new("/Users/me");
        let rec = |title: &str, path: &str| crate::model::Recommendation {
            title: title.into(),
            path: home.join(path).display().to_string(),
            ..crate::test_support::recommendation()
        };
        let mut recs = vec![
            rec(
                "Application caches · com.tinyspeck.slackmacgap",
                "Library/Caches/com.tinyspeck.slackmacgap",
            ),
            rec(
                "Application caches · com.unknown.app",
                "Library/Caches/com.unknown.app",
            ),
            rec("Rust build output", "code/target"),
        ];
        name_apps_with(&i, home, &mut recs);
        assert_eq!(recs[0].title, "Application caches · Slack");
        assert_eq!(recs[1].title, "Application caches · com.unknown.app");
        assert_eq!(recs[2].title, "Rust build output");
    }

    #[test]
    fn strips_direction_marks_from_names() {
        assert_eq!(clean_name("\u{200e}WhatsApp"), "WhatsApp");
    }

    // macOS paths and bundle ids; Windows has its own tests in apps_windows.rs.
    #[cfg(not(windows))]
    #[test]
    fn unrelated_paths_and_uninstalled_apps_do_not_resolve() {
        let i = sample();
        let home = Path::new("/Users/me");
        for p in [
            "Documents/Cursor",
            "Library/Caches",
            "Library/Caches/unknown.app.id",
            ".claude",
            "/tmp/Cursor",
        ] {
            assert!(i.resolve(home, &home.join(p)).is_none(), "{p}");
        }
        assert_eq!(
            i.resolve(
                home,
                Path::new("/Applications/Cursor.app/Contents/Resources")
            )
            .map(|a| a.name.as_str()),
            Some("Cursor")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn renders_a_png_for_an_installed_system_app() {
        let png = icon_png(Path::new("/System/Applications/Calculator.app"), 32.0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
