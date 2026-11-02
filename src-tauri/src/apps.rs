//! Which installed application a folder belongs to, and that application's
//! own icon.
//!
//! Icons are rendered from the apps installed on this Mac through
//! `NSWorkspace`, so DiskPilot ships no third-party logos. Resolution is pure
//! path matching against an index of installed app bundles: a path received
//! from the UI is never read, only compared.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: String,
    pub path: PathBuf,
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
}

/// Drop invisible direction marks some apps put in their names.
fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .collect::<String>()
        .trim()
        .to_string()
}

fn plist_string(dict: &plist::Dictionary, key: &str) -> Option<String> {
    dict.get(key)?.as_string().map(str::to_string)
}

impl AppIndex {
    pub fn add(&mut self, app: AppInfo) {
        let i = self.apps.len();
        for key in [
            app.bundle_id.to_lowercase(),
            app.name.to_lowercase(),
            app.path
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
        ] {
            if !key.is_empty() {
                self.by_key.entry(key).or_insert(i);
            }
        }
        self.apps.push(app);
    }

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

    /// Index the standard application folders, one level of subfolders deep.
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

    fn lookup(&self, key: &str) -> Option<&AppInfo> {
        self.by_key.get(&key.to_lowercase()).map(|&i| &self.apps[i])
    }

    /// The installed app a path belongs to, if any.
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

fn index(home: &Path) -> &'static AppIndex {
    static INDEX: OnceLock<AppIndex> = OnceLock::new();
    INDEX.get_or_init(|| AppIndex::build(home))
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
                    icon_png(&app.path, 64.0).map(|png| {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, id: &str, path: &str) -> AppInfo {
        AppInfo {
            name: name.into(),
            bundle_id: id.into(),
            path: PathBuf::from(path),
        }
    }

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

    #[test]
    fn strips_direction_marks_from_names() {
        assert_eq!(clean_name("\u{200e}WhatsApp"), "WhatsApp");
    }

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
