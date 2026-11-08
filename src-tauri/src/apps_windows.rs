//! Installed programs on Windows, as Settings > Apps lists them.
//!
//! Desktop programs come from the registry's Uninstall keys (all users, both
//! 64-bit and 32-bit views, and the current user), filtered the way Windows
//! filters them: system components, updates and entries without an
//! uninstaller are hidden. Microsoft Store apps come from the per-user
//! package repository in the registry, which is much faster than asking
//! PowerShell (`Get-AppxPackage`).
//!
//! DiskPilot never deletes a program's folder. "Uninstall" starts the
//! program's own uninstaller, exactly as registered, or opens Settings.
//! The parsers here are plain logic, tested on every platform.

use std::path::{Component, Path, PathBuf};

/// A program's entry under an Uninstall key.
#[derive(Debug, Clone, Default)]
pub struct UninstallEntry {
    /// The entry's key name; DiskPilot's id for the program.
    pub key: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub display_icon: Option<String>,
    pub uninstall: Option<String>,
    pub quiet_uninstall: Option<String>,
    /// `EstimatedSize`, in KB.
    pub estimated_kb: Option<u32>,
    pub system_component: bool,
    pub has_parent: bool,
    pub release_type: Option<String>,
}

/// Whether Settings > Apps would list this entry.
pub fn listed(e: &UninstallEntry) -> bool {
    let update = e.release_type.as_deref().is_some_and(|t| {
        matches!(
            t.to_ascii_lowercase().as_str(),
            "security update" | "update rollup" | "hotfix" | "update"
        )
    });
    !e.name.trim().is_empty()
        && !e.system_component
        && !e.has_parent
        && !update
        && (e.uninstall.is_some() || e.quiet_uninstall.is_some())
}

/// Lowercase, backslash-separated, without a trailing separator.
fn key_of(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    let trimmed = s.trim_end_matches('\\');
    if trimmed.ends_with(':') {
        format!("{trimmed}\\")
    } else {
        trimmed.to_string()
    }
}

/// `path` equals or is inside `dir`, ignoring case as NTFS does.
pub fn within(path: &Path, dir: &Path) -> bool {
    let (p, d) = (key_of(path), key_of(dir));
    p == d || p.starts_with(&format!("{}\\", d.trim_end_matches('\\')))
}

/// A usable install folder from `InstallLocation`. Broken entries point at
/// a drive root or a shared folder such as Program Files; using one of those
/// would count every other program as part of this one.
pub fn install_dir(raw: &str, shared: &[PathBuf]) -> Option<PathBuf> {
    let s = raw.trim().trim_matches('"').trim();
    let path = PathBuf::from(s.trim_end_matches(['\\', '/']));
    let normal = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    let absolute = s.len() > 2 && s.as_bytes()[1] == b':' || s.starts_with("\\\\");
    if !absolute || normal == 0 {
        return None;
    }
    // The folder must not be, or contain, a shared folder.
    if shared.iter().any(|d| within(d, &path)) {
        return None;
    }
    Some(path)
}

/// The file in a `DisplayIcon` value: `"C:\App\app.exe",0` -> `C:\App\app.exe`.
pub fn icon_file(raw: &str) -> String {
    let s = raw.trim();
    let s = match s.rsplit_once(',') {
        Some((file, index))
            if !index.trim().is_empty()
                && index
                    .trim()
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit()) =>
        {
            file
        }
        _ => s,
    };
    s.trim().trim_matches('"').to_string()
}

/// A name without its version or architecture, for matching data folders:
/// "Mozilla Firefox (x64 en-US)" -> "mozilla firefox", "7-Zip 23.01 (x64)" -> "7-zip".
pub fn plain_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let head = lower.split('(').next().unwrap_or_default();
    let mut words: Vec<&str> = head.split_whitespace().collect();
    while let Some(last) = words.last() {
        let versionish = last.starts_with(|c: char| c.is_ascii_digit())
            || (last.starts_with('v') && last[1..].starts_with(|c: char| c.is_ascii_digit()))
            || matches!(*last, "x64" | "x86" | "arm64" | "64-bit" | "32-bit" | "-");
        if versionish && words.len() > 1 {
            words.pop();
        } else {
            break;
        }
    }
    words.join(" ")
}

/// The family name (`Name_PublisherId`) of a package full name
/// (`Name_Version_Architecture_ResourceId_PublisherId`), for main packages
/// only: resource packages and bundles share their app's family.
pub fn package_family(full_name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = full_name.split('_').collect();
    if parts.len() != 5 || parts[0].is_empty() || parts[4].is_empty() || !parts[3].is_empty() {
        return None;
    }
    Some((format!("{}_{}", parts[0], parts[4]), parts[1].to_string()))
}

/// Whether a folder under `AppData\Local\Packages` looks like a package
/// family's data that only a Store app creates, and is not Windows' own.
pub fn third_party_family(name: &str) -> bool {
    let Some((app, publisher)) = name.rsplit_once('_') else {
        return false;
    };
    let lower = app.to_ascii_lowercase();
    publisher.len() == 13
        && publisher.chars().all(|c| c.is_ascii_alphanumeric())
        && !app.is_empty()
        && !lower.starts_with("microsoft")
        && !lower.starts_with("windows")
}

/// Folder names under AppData that hold Windows' own data, not an app's.
pub const NOT_APP_DATA: &[&str] = &[
    "packages",
    "programs",
    "temp",
    "crashdumps",
    "d3dscache",
    "connecteddevicesplatform",
    "comms",
    "publishers",
    "virtualstore",
];

/// Data folders whose name does not match the program that owns them, as
/// (path relative to home, lowercase, `/`-separated; plain program name).
pub const ALIASES: &[(&str, &str)] = &[
    ("appdata/local/google/chrome", "google chrome"),
    ("appdata/local/bravesoftware/brave-browser", "brave"),
    ("appdata/local/microsoft/edge", "microsoft edge"),
    ("appdata/roaming/mozilla/firefox", "mozilla firefox"),
    ("appdata/local/mozilla/firefox", "mozilla firefox"),
    ("appdata/roaming/code", "microsoft visual studio code"),
    (".vscode", "microsoft visual studio code"),
    (".cursor", "cursor"),
    ("appdata/local/docker", "docker desktop"),
    ("appdata/roaming/docker", "docker desktop"),
    (".docker", "docker desktop"),
    (".claude", "claude"),
    ("appdata/roaming/telegram desktop", "telegram desktop"),
];

/// How a program is uninstalled, worked out from its registry entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uninstaller {
    /// The program's own uninstaller: what runs and its command line.
    Program { exe: PathBuf, args: String },
    /// No usable uninstaller command: Settings > Apps > Installed apps.
    Settings,
}

impl Uninstaller {
    /// The exact command, as shown before it runs.
    pub fn command(&self) -> Option<String> {
        match self {
            Uninstaller::Program { exe, args } if args.is_empty() => {
                Some(format!("\"{}\"", exe.display()))
            }
            Uninstaller::Program { exe, args } => Some(format!("\"{}\" {args}", exe.display())),
            Uninstaller::Settings => None,
        }
    }
}

/// Programs that only run other commands. An uninstall string that starts
/// one of these is left to Settings, so DiskPilot never runs a script.
const INTERPRETERS: &[&str] = &[
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wscript.exe",
    "cscript.exe",
    "mshta.exe",
    "rundll32.exe",
];

fn msi_product(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = start + s[start..].find('}')?;
    let guid = &s[start..=end];
    let valid = guid.len() == 38
        && guid[1..37]
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-');
    valid.then_some(guid)
}

/// Parse an `UninstallString` into the program to start and its command
/// line. Handles quoted paths, unquoted paths with spaces (the longest
/// existing `.exe` prefix wins), and Windows Installer entries, which
/// become `msiexec.exe /X{product}` (uninstall, not the repair dialog that
/// `/I` opens). `system32` is where `msiexec.exe` lives.
pub fn parse_uninstall(raw: &str, exists: impl Fn(&Path) -> bool, system32: &Path) -> Uninstaller {
    let s = raw.trim();
    let lower = s.to_ascii_lowercase();
    let msiexec = |args: &str| match msi_product(args) {
        Some(guid) => Uninstaller::Program {
            exe: system32.join("msiexec.exe"),
            args: format!("/X{guid}"),
        },
        None => Uninstaller::Settings,
    };

    let (exe, args) = if let Some(rest) = s.strip_prefix('"') {
        let Some(end) = rest.find('"') else {
            return Uninstaller::Settings;
        };
        (
            PathBuf::from(&rest[..end]),
            rest[end + 1..].trim().to_string(),
        )
    } else if lower.starts_with("msiexec") {
        return msiexec(s);
    } else {
        // Unquoted: try each ".exe" followed by a space or the end, longest first.
        let ends: Vec<usize> = lower
            .match_indices(".exe")
            .map(|(i, _)| i + 4)
            .filter(|&e| e == s.len() || s[e..].starts_with(' '))
            .collect();
        match ends.iter().rev().find(|&&e| exists(Path::new(&s[..e]))) {
            Some(&e) => (PathBuf::from(&s[..e]), s[e..].trim().to_string()),
            None => return Uninstaller::Settings,
        }
    };

    let exe_text = exe.to_string_lossy().to_ascii_lowercase();
    let file = exe_text.rsplit(['\\', '/']).next().unwrap_or_default();
    if file == "msiexec.exe" || file == "msiexec" {
        return msiexec(&args);
    }
    let absolute = exe.to_string_lossy().as_bytes().get(1) == Some(&b':');
    if !absolute || INTERPRETERS.contains(&file) || !exists(&exe) {
        return Uninstaller::Settings;
    }
    Uninstaller::Program { exe, args }
}

#[cfg(windows)]
pub use imp::*;

#[cfg(windows)]
mod imp {
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS,
    };

    use super::*;
    use crate::apps::{AppIndex, AppInfo};
    use crate::winshell::{self, RegKey};

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const PACKAGES: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";

    /// What DiskPilot knows about a program beyond its name.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct WinApp {
        pub publisher: Option<String>,
        pub version: Option<String>,
        pub install_dir: Option<PathBuf>,
        pub estimated_bytes: Option<u64>,
        /// The file whose icon is shown (`DisplayIcon`), for desktop programs.
        pub icon: Option<PathBuf>,
        pub uninstaller: Option<Uninstaller>,
        /// A Microsoft Store app; `bundle_id` is its package family name.
        pub store: bool,
        /// Part of Windows (installed under the Windows folder).
        pub system: bool,
    }

    fn env_path(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    pub fn system_root() -> PathBuf {
        env_path("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
    }

    pub fn program_data() -> PathBuf {
        env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
    }

    /// Folders shared by many programs; never one program's install folder.
    fn shared_folders(home: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = [
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "CommonProgramFiles",
            "CommonProgramFiles(x86)",
            "SystemRoot",
            "ProgramData",
            "APPDATA",
            "LOCALAPPDATA",
        ]
        .iter()
        .filter_map(|v| env_path(v))
        .collect();
        out.push(home.to_path_buf());
        if let Some(local) = env_path("LOCALAPPDATA") {
            out.push(local.join("Programs"));
        }
        out
    }

    fn read_entries(
        root: windows::Win32::System::Registry::HKEY,
        view: REG_SAM_FLAGS,
    ) -> Vec<UninstallEntry> {
        let Some(key) = RegKey::open(root, UNINSTALL, view) else {
            return Vec::new();
        };
        key.subkey_names()
            .into_iter()
            .filter_map(|name| {
                let k = key.subkey(&name)?;
                Some(UninstallEntry {
                    name: k.string("DisplayName").unwrap_or_default(),
                    publisher: k.string("Publisher"),
                    version: k.string("DisplayVersion"),
                    install_location: k.string("InstallLocation"),
                    display_icon: k.string("DisplayIcon"),
                    uninstall: k.string("UninstallString"),
                    quiet_uninstall: k.string("QuietUninstallString"),
                    estimated_kb: k.dword("EstimatedSize"),
                    system_component: k.dword("SystemComponent") == Some(1),
                    has_parent: k.string("ParentKeyName").is_some(),
                    release_type: k.string("ReleaseType"),
                    key: name,
                })
            })
            .collect()
    }

    /// Every listed desktop program: all users (64-bit, then 32-bit view),
    /// then the current user. The first entry with a key name wins.
    pub fn uninstall_entries() -> Vec<UninstallEntry> {
        let mut seen = HashSet::new();
        let mut names = HashSet::new();
        let mut out = Vec::new();
        for (root, view) in [
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, REG_SAM_FLAGS(0)),
        ] {
            for e in read_entries(root, view) {
                if !listed(&e) || !seen.insert(e.key.to_lowercase()) {
                    continue;
                }
                // The same program registered twice (both views) once.
                let same = (e.name.to_lowercase(), e.version.clone());
                if names.insert(same) {
                    out.push(e);
                }
            }
        }
        out
    }

    fn uninstaller(e: &UninstallEntry) -> Uninstaller {
        let system32 = system_root().join("System32");
        let raw = e.uninstall.as_deref().or(e.quiet_uninstall.as_deref());
        raw.map_or(Uninstaller::Settings, |raw| {
            parse_uninstall(raw, |p| p.is_file(), &system32)
        })
    }

    fn desktop_app(e: UninstallEntry, shared: &[PathBuf]) -> AppInfo {
        let install_dir = e
            .install_location
            .as_deref()
            .and_then(|raw| install_dir(raw, shared))
            .filter(|d| d.is_dir());
        let icon = e
            .display_icon
            .as_deref()
            .map(icon_file)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file());
        let system = install_dir
            .as_deref()
            .is_some_and(|d| within(d, &system_root()));
        let path = install_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!(r"{UNINSTALL}\{}", e.key)));
        AppInfo {
            name: crate::apps::clean_name(&e.name),
            bundle_id: e.key.clone(),
            path,
            win: WinApp {
                uninstaller: Some(uninstaller(&e)),
                publisher: e.publisher,
                version: e.version,
                install_dir,
                estimated_bytes: e
                    .estimated_kb
                    .filter(|&kb| kb > 0)
                    .map(|kb| u64::from(kb) * 1024),
                icon,
                store: false,
                system,
            },
        }
    }

    /// Store app names resolved so far, by package full name. Resolving
    /// reads the package's resources, so it is done once per package.
    fn display_name(full: &str, raw: Option<&str>, fallback: &str) -> String {
        static NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        let names = NAMES.get_or_init(Default::default);
        if let Some(n) = names.lock().unwrap_or_else(|e| e.into_inner()).get(full) {
            return n.clone();
        }
        let resolved = raw
            .and_then(|r| {
                if r.starts_with("@{") {
                    winshell::indirect_string(r)
                } else {
                    Some(r.to_string())
                }
            })
            .filter(|n| !n.starts_with("@{") && !n.starts_with("ms-resource:"))
            .unwrap_or_else(|| fallback.rsplit('.').next().unwrap_or(fallback).to_string());
        names
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(full.to_string(), resolved.clone());
        resolved
    }

    /// Store apps registered for this user that have their own data folder
    /// (frameworks and resource packages have none). Returns the apps and
    /// whether the package repository could be read at all.
    fn store_apps() -> (Vec<AppInfo>, bool) {
        let Some(repo) = RegKey::open(HKEY_CURRENT_USER, PACKAGES, REG_SAM_FLAGS(0)) else {
            return (Vec::new(), false);
        };
        let full_names = repo.subkey_names();
        let data: HashSet<String> = dirs::data_local_dir()
            .and_then(|d| std::fs::read_dir(d.join("Packages")).ok())
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let windows = system_root();
        let mut seen = HashSet::new();
        let mut apps = Vec::new();
        for full in &full_names {
            let Some((family, version)) = package_family(full) else {
                continue;
            };
            if !data.contains(&family.to_lowercase()) || !seen.insert(family.to_lowercase()) {
                continue;
            }
            let Some(key) = repo.subkey(full) else {
                continue;
            };
            let root = key
                .string("PackageRootFolder")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute());
            let name_part = family.split('_').next().unwrap_or(&family).to_string();
            let name = display_name(full, key.string("DisplayName").as_deref(), &name_part);
            let system = root.as_deref().is_none_or(|r| within(r, &windows));
            apps.push(AppInfo {
                name: crate::apps::clean_name(&name),
                path: root
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(format!(r"shell:AppsFolder\{family}"))),
                bundle_id: family,
                win: WinApp {
                    version: Some(version),
                    install_dir: root,
                    uninstaller: Some(Uninstaller::Settings),
                    store: true,
                    system,
                    ..Default::default()
                },
            });
        }
        (apps, !full_names.is_empty())
    }

    /// The index of installed programs and Store apps. Also reports whether
    /// the Store package list was read, which leftover detection needs.
    pub fn build_index(home: &Path) -> AppIndex {
        let shared = shared_folders(home);
        let mut index = AppIndex::default();
        let mut desktop: Vec<AppInfo> = uninstall_entries()
            .into_iter()
            .map(|e| desktop_app(e, &shared))
            .collect();
        desktop.sort_by_key(|a| a.name.to_lowercase());
        for app in desktop {
            index.add(app);
        }
        let (store, known) = store_apps();
        index.store_known = known;
        for app in store {
            index.add(app);
        }
        index
    }

    /// Extra lookup keys for a program: its plain name and install folder name.
    pub fn extra_keys(app: &AppInfo) -> Vec<String> {
        let mut keys = vec![plain_name(&app.name)];
        if let Some(dir) = &app.win.install_dir {
            if let Some(n) = dir.file_name() {
                keys.push(n.to_string_lossy().to_lowercase());
            }
        }
        if app.win.store {
            if let Some(n) = app.bundle_id.split('_').next() {
                keys.push(n.to_lowercase());
            }
        }
        keys
    }

    /// The icon of an installed program or Store app, as PNG.
    pub fn icon_png(app: &AppInfo, px: u32) -> Option<Vec<u8>> {
        if app.win.store {
            let manifest =
                std::fs::read_to_string(app.win.install_dir.as_ref()?.join("AppxManifest.xml"))
                    .ok()?;
            let id = application_id(&manifest)?;
            let item = format!(r"shell:AppsFolder\{}!{id}", app.bundle_id);
            return winshell::shell_image_png(item.as_ref(), px, winshell::SIIGBF_ICONONLY);
        }
        let file = app.win.icon.as_ref()?;
        winshell::shell_image_png(file.as_os_str(), px, winshell::SIIGBF_ICONONLY)
    }

    /// Start the program's own uninstaller, or open Settings > Apps when it
    /// has none DiskPilot can start. The entry is looked up again by id;
    /// the command never comes from the window.
    pub fn uninstall(id: &str) -> Result<crate::apps::UninstallOutcome, String> {
        use crate::apps::UninstallOutcome;
        let how = match uninstall_entries().into_iter().find(|e| e.key == id) {
            Some(e) => uninstaller(&e),
            None => {
                let (store, _) = store_apps();
                if store.iter().any(|a| a.bundle_id == id) {
                    Uninstaller::Settings
                } else {
                    return Err("This program is no longer installed.".into());
                }
            }
        };
        match how {
            Uninstaller::Program { exe, args } => {
                let params = (!args.is_empty()).then(|| std::ffi::OsString::from(&args));
                winshell::shell_open(exe.as_os_str(), params.as_deref(), false)
                    .map(|()| UninstallOutcome::Uninstaller)
                    .map_err(|e| {
                        if winshell::is_cancelled(&e) {
                            "The uninstaller did not start.".to_string()
                        } else {
                            format!("Windows could not start the uninstaller: {}", e.message())
                        }
                    })
            }
            Uninstaller::Settings => open_apps_settings().map(|()| UninstallOutcome::Settings),
        }
    }

    /// Path components below `base`, lowercased, if `path` is inside it.
    fn relative_parts(path: &Path, base: &Path) -> Option<Vec<String>> {
        if !within(path, base) {
            return None;
        }
        let skip = base.components().count();
        Some(
            path.components()
                .skip(skip)
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
                    _ => None,
                })
                .collect(),
        )
    }

    impl AppIndex {
        /// The installed program a path belongs to: inside its install
        /// folder, or a data folder named after it in AppData or ProgramData.
        pub(crate) fn resolve_windows(&self, home: &Path, path: &Path) -> Option<&AppInfo> {
            let inside = self
                .apps
                .iter()
                .filter_map(|a| Some((a, a.win.install_dir.as_deref()?)))
                .filter(|(_, d)| within(path, d))
                .max_by_key(|(_, d)| d.as_os_str().len());
            if let Some((app, _)) = inside {
                return Some(app);
            }
            if let Some(app) = self.apps.iter().find(|a| a.path == path) {
                return Some(app);
            }
            let parts = relative_parts(path, home).or_else(|| {
                let mut p = relative_parts(path, &program_data())?;
                p.insert(0, "programdata".into());
                Some(p)
            })?;
            let lower = parts.join("/");
            for (prefix, name) in ALIASES {
                if lower == *prefix || lower.starts_with(&format!("{prefix}/")) {
                    if let Some(app) = self.lookup(name) {
                        return Some(app);
                    }
                }
            }
            let p: Vec<&str> = parts.iter().map(String::as_str).collect();
            match p.as_slice() {
                ["appdata", "local", "packages", family, ..] => self.lookup(family),
                ["appdata", "roaming" | "local" | "locallow", name, rest @ ..]
                | ["programdata", name, rest @ ..] => self.data_owner(name, rest.first().copied()),
                _ => None,
            }
        }

        /// A data folder's program, by its name or, for vendor folders such
        /// as `Google\Chrome`, by the vendor and child names.
        fn data_owner(&self, name: &str, child: Option<&str>) -> Option<&AppInfo> {
            if NOT_APP_DATA.contains(&name) {
                return None;
            }
            self.lookup(name).or_else(|| {
                let child = child?;
                self.lookup(&format!("{name} {child}"))
                    .or_else(|| self.lookup(child))
            })
        }
    }

    /// Storage per installed program: its install folder (or the size it
    /// registered) plus its folders in AppData and ProgramData, and data
    /// left by Store apps that are no longer installed. Data sizes come from
    /// the scan index; nothing is modified.
    pub fn app_storage(
        home: &Path,
        scan: Option<&crate::scanner::DirIndex>,
    ) -> crate::apps::AppStorageReport {
        use crate::apps::{
            bundle_bytes, footprint, size_of, AppDataLocation, AppStorageReport, Leftover,
            MIN_LEFTOVER_BYTES,
        };
        use rayon::prelude::*;

        let index = crate::apps::refresh_index(home);
        let appdata = home.join("AppData");
        let data_scanned = scan.is_some_and(|i| i.contains_key(&appdata));
        let empty = crate::scanner::DirIndex::new();
        let scan = scan.unwrap_or(&empty);
        let install_dirs: Vec<&Path> = index
            .apps
            .iter()
            .filter_map(|a| a.win.install_dir.as_deref())
            .collect();

        let mut per_app: HashMap<PathBuf, Vec<AppDataLocation>> = HashMap::new();
        let mut leftovers = Vec::new();
        let roots = [
            (appdata.join("Roaming"), "Roaming"),
            (appdata.join("Local"), "Local"),
            (appdata.join("LocalLow"), "LocalLow"),
            (appdata.join("Local").join("Packages"), "Packages"),
            (program_data(), "ProgramData"),
        ];
        for (root, kind) in roots.iter().filter(|_| data_scanned) {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            let packages = *kind == "Packages";
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !packages && NOT_APP_DATA.contains(&name.to_lowercase().as_str()) {
                    continue;
                }
                let mut paths = vec![entry.path()];
                // Vendor folders (Google, Microsoft) hold one app per subfolder.
                if !packages
                    && index.resolve(home, &paths[0]).is_none()
                    && entry.file_type().is_ok_and(|t| t.is_dir())
                {
                    paths = std::fs::read_dir(entry.path())
                        .map(|inner| inner.flatten().map(|e| e.path()).collect())
                        .unwrap_or_default();
                }
                for path in paths {
                    // A program installed in AppData is counted as the program.
                    if install_dirs
                        .iter()
                        .any(|d| within(&path, d) || within(d, &path))
                    {
                        continue;
                    }
                    let Some(bytes) = size_of(&path, scan).filter(|&b| b > 0) else {
                        continue;
                    };
                    if let Some(app) = index.resolve(home, &path) {
                        per_app
                            .entry(app.path.clone())
                            .or_default()
                            .push(AppDataLocation {
                                path: path.display().to_string(),
                                kind: (*kind).to_string(),
                                bytes,
                            });
                        continue;
                    }
                    // Only Store app data counts as a leftover: Windows creates
                    // these folders for installed Store apps alone, while other
                    // AppData folders often belong to tools with no uninstaller.
                    if packages
                        && index.store_known
                        && bytes >= MIN_LEFTOVER_BYTES
                        && third_party_family(&name)
                    {
                        leftovers.push(Leftover {
                            name: name.split('_').next().unwrap_or(&name).to_string(),
                            path: path.display().to_string(),
                            kind: (*kind).to_string(),
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

        let mut apps: Vec<_> = index
            .apps
            .par_iter()
            .map(|app| {
                let bytes = if app.win.system {
                    None
                } else {
                    app.win
                        .install_dir
                        .as_deref()
                        .and_then(bundle_bytes)
                        .filter(|&b| b > 0)
                        .or(app.win.estimated_bytes)
                };
                footprint(
                    app,
                    per_app.get(&app.path).cloned().unwrap_or_default(),
                    bytes,
                )
            })
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

    /// The AppUserModelID of an installed Store app, from its manifest.
    fn store_app_id(family: &str) -> Option<String> {
        let repo = RegKey::open(HKEY_CURRENT_USER, PACKAGES, REG_SAM_FLAGS(0))?;
        repo.subkey_names().iter().find_map(|full| {
            let (f, _) = package_family(full)?;
            if !f.eq_ignore_ascii_case(family) {
                return None;
            }
            let root = PathBuf::from(repo.subkey(full)?.string("PackageRootFolder")?);
            let manifest = std::fs::read_to_string(root.join("AppxManifest.xml")).ok()?;
            Some(format!("{f}!{}", application_id(&manifest)?))
        })
    }

    /// Open Telegram or WhatsApp, whose storage people clear inside the app:
    /// the desktop program if it is installed, then the Store app, then the
    /// app's link scheme.
    pub fn open_owner_app(name: &str) -> Result<(), String> {
        let roaming = dirs::data_dir().unwrap_or_default();
        let local = dirs::data_local_dir().unwrap_or_default();
        let (programs, family, scheme) = match name {
            "Telegram" => (
                vec![roaming.join("Telegram Desktop").join("Telegram.exe")],
                "TelegramMessengerLLP.TelegramDesktop_t4vj0pshhgkwm",
                "tg://",
            ),
            "WhatsApp" => (
                vec![local.join("WhatsApp").join("WhatsApp.exe")],
                "5319275A.WhatsAppDesktop_cv1g1gvanyjgm",
                "whatsapp:",
            ),
            _ => return Err("DiskPilot does not open an app for this item.".into()),
        };
        for exe in programs.iter().filter(|p| p.is_file()) {
            if winshell::shell_open(exe.as_os_str(), None, true).is_ok() {
                return Ok(());
            }
        }
        if let Some(aumid) = store_app_id(family) {
            let item = format!(r"shell:AppsFolder\{aumid}");
            if winshell::shell_open(item.as_ref(), None, true).is_ok() {
                return Ok(());
            }
        }
        winshell::shell_open(scheme.as_ref(), None, true).map_err(|_| {
            format!("{name} is not installed on this PC, or Windows could not open it.")
        })
    }

    pub fn open_apps_settings() -> Result<(), String> {
        winshell::shell_open("ms-settings:appsfeatures".as_ref(), None, true)
            .map_err(|e| format!("Windows could not open Settings: {}", e.message()))
    }
}

/// The first `<Application Id="...">` in a package manifest.
pub fn application_id(manifest: &str) -> Option<String> {
    let start = manifest.find("<Application ")?;
    let tag = &manifest[start..start + manifest[start..].find('>')?];
    let at = tag.find(" Id=\"")? + 5;
    let id = &tag[at..at + tag[at..].find('"')?];
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> UninstallEntry {
        UninstallEntry {
            key: name.into(),
            name: name.into(),
            uninstall: Some(r"C:\App\uninstall.exe".into()),
            ..Default::default()
        }
    }

    #[test]
    fn lists_entries_like_settings_does() {
        assert!(listed(&entry("App")));
        assert!(!listed(&UninstallEntry {
            system_component: true,
            ..entry("App")
        }));
        assert!(!listed(&UninstallEntry {
            has_parent: true,
            ..entry("KB123")
        }));
        assert!(!listed(&UninstallEntry {
            release_type: Some("Security Update".into()),
            ..entry("KB123")
        }));
        assert!(!listed(&UninstallEntry {
            uninstall: None,
            ..entry("App")
        }));
        assert!(!listed(&entry(" ")));
    }

    #[test]
    fn install_dirs_never_cover_shared_folders() {
        let shared = vec![
            PathBuf::from(r"C:\Program Files"),
            PathBuf::from(r"C:\Users\me"),
            PathBuf::from(r"C:\Users\me\AppData\Local\Programs"),
        ];
        assert_eq!(
            install_dir(r#""C:\Program Files\App\""#, &shared),
            Some(PathBuf::from(r"C:\Program Files\App"))
        );
        assert_eq!(
            install_dir(r"C:\Users\me\AppData\Local\Programs\cursor", &shared),
            Some(PathBuf::from(r"C:\Users\me\AppData\Local\Programs\cursor"))
        );
        for bad in [
            r"C:\",
            r"C:\Program Files\",
            r"c:\program files",
            r"C:\Users",
            r"C:\Users\me\AppData\Local\Programs",
            "",
            "App",
        ] {
            assert_eq!(install_dir(bad, &shared), None, "{bad}");
        }
    }

    #[test]
    fn within_ignores_case_and_partial_names() {
        assert!(within(
            Path::new(r"C:\Apps\Foo\bin"),
            Path::new(r"c:\apps\foo")
        ));
        assert!(within(
            Path::new(r"C:\Apps\Foo"),
            Path::new(r"C:\Apps\Foo\")
        ));
        assert!(!within(
            Path::new(r"C:\Apps\Foobar"),
            Path::new(r"C:\Apps\Foo")
        ));
        assert!(within(Path::new(r"C:\Apps"), Path::new(r"C:\")));
    }

    #[test]
    fn reads_icon_files() {
        assert_eq!(icon_file(r#""C:\App\app.exe",0"#), r"C:\App\app.exe");
        assert_eq!(icon_file(r"C:\App\app.exe,-101"), r"C:\App\app.exe");
        assert_eq!(icon_file(r"C:\App\app.ico"), r"C:\App\app.ico");
        assert_eq!(icon_file(r"C:\A,B\app.ico"), r"C:\A,B\app.ico");
    }

    #[test]
    fn plain_names_drop_versions() {
        assert_eq!(plain_name("Mozilla Firefox (x64 en-US)"), "mozilla firefox");
        assert_eq!(plain_name("7-Zip 23.01 (x64)"), "7-zip");
        assert_eq!(plain_name("Python 3.12.1 (64-bit)"), "python");
        assert_eq!(plain_name("Notepad++ v8.6"), "notepad++");
        assert_eq!(plain_name("Telegram Desktop"), "telegram desktop");
        assert_eq!(plain_name("2048"), "2048");
    }

    #[test]
    fn package_families() {
        assert_eq!(
            package_family("5319275A.WhatsAppDesktop_2.2407.10.0_x64__cv1g1gvanyjgm"),
            Some((
                "5319275A.WhatsAppDesktop_cv1g1gvanyjgm".into(),
                "2.2407.10.0".into()
            ))
        );
        assert_eq!(
            package_family("Microsoft.Foo_1.0.0.0_neutral_split.scale-100_8wekyb3d8bbwe"),
            None
        );
        assert_eq!(
            package_family("Microsoft.Foo_1.0.0.0_neutral_~_8wekyb3d8bbwe"),
            None
        );
        assert_eq!(package_family("nonsense"), None);
        assert!(third_party_family("5319275A.WhatsAppDesktop_cv1g1gvanyjgm"));
        assert!(!third_party_family(
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe"
        ));
        assert!(!third_party_family(
            "windows.immersivecontrolpanel_cw5n1h2txyewy"
        ));
        assert!(!third_party_family("SomeFolder"));
    }

    #[test]
    fn finds_the_application_id() {
        let manifest = r#"<Package><Applications><Application Id="App" Executable="x.exe"><x/></Application></Applications></Package>"#;
        assert_eq!(application_id(manifest).as_deref(), Some("App"));
        assert_eq!(application_id("<Package/>"), None);
    }

    #[test]
    fn parses_uninstall_strings_without_a_shell() {
        let sys = Path::new(r"C:\Windows\System32");
        let exists = |p: &Path| {
            [
                r"C:\Program Files\My App\uninst.exe",
                r"C:\Program Files\My App\uninst.exe x\real.exe",
                r"C:\Windows\System32\cmd.exe",
            ]
            .iter()
            .any(|e| Path::new(e) == p)
        };
        let program = |exe: &str, args: &str| Uninstaller::Program {
            exe: PathBuf::from(exe),
            args: args.into(),
        };

        assert_eq!(
            parse_uninstall(r#""C:\Program Files\My App\uninst.exe" /S"#, exists, sys),
            program(r"C:\Program Files\My App\uninst.exe", "/S")
        );
        assert_eq!(
            parse_uninstall(r"C:\Program Files\My App\uninst.exe --remove", exists, sys),
            program(r"C:\Program Files\My App\uninst.exe", "--remove")
        );
        // The longest existing ".exe" prefix wins.
        assert_eq!(
            parse_uninstall(
                r"C:\Program Files\My App\uninst.exe x\real.exe /q",
                exists,
                sys
            ),
            program(r"C:\Program Files\My App\uninst.exe x\real.exe", "/q")
        );
        // Windows Installer: always uninstall (/X), never the repair dialog.
        let msiexec = sys.join("msiexec.exe");
        let msi = Uninstaller::Program {
            exe: msiexec.clone(),
            args: "/X{23170F69-40C1-2702-2301-000001000000}".into(),
        };
        assert_eq!(
            parse_uninstall(
                "MsiExec.exe /I{23170F69-40C1-2702-2301-000001000000}",
                exists,
                sys
            ),
            msi
        );
        assert_eq!(
            parse_uninstall(
                r#""C:\Windows\System32\MsiExec.exe" /X{23170F69-40C1-2702-2301-000001000000} /qn"#,
                exists,
                sys
            ),
            msi
        );
        assert_eq!(
            msi.command().unwrap(),
            format!(
                "\"{}\" /X{{23170F69-40C1-2702-2301-000001000000}}",
                msiexec.display()
            )
        );
        // Anything else is left to Settings.
        for raw in [
            "MsiExec.exe /I{not-a-guid}",
            r#""C:\Windows\System32\cmd.exe" /c del C:\x"#,
            r"C:\Missing\uninstall.exe",
            r#""C:\Program Files\My App\uninst.exe"#,
            r#""uninst.exe" /S"#,
            "",
        ] {
            assert_eq!(
                parse_uninstall(raw, exists, sys),
                Uninstaller::Settings,
                "{raw}"
            );
        }
    }

    #[cfg(windows)]
    fn app(name: &str, dir: Option<&str>) -> crate::apps::AppInfo {
        crate::apps::AppInfo {
            name: name.into(),
            bundle_id: format!("{{{name}}}"),
            path: dir.map_or_else(|| PathBuf::from(name), PathBuf::from),
            win: WinApp {
                install_dir: dir.map(PathBuf::from),
                ..Default::default()
            },
        }
    }

    #[cfg(windows)]
    #[test]
    fn resolves_install_folders_and_appdata_by_name() {
        let mut index = crate::apps::AppIndex::default();
        index.add(app("Google Chrome", None));
        index.add(app("Telegram Desktop", None));
        index.add(app("Discord", Some(r"C:\Users\me\AppData\Local\Discord")));
        index.add(app("7-Zip 23.01 (x64)", Some(r"C:\Program Files\7-Zip")));
        index.add(app("Microsoft Teams", None));
        let home = Path::new(r"C:\Users\me");
        let name = |p: &str| index.resolve(home, Path::new(p)).map(|a| a.name.clone());
        assert_eq!(
            name(r"C:\Program Files\7-Zip\7z.exe").as_deref(),
            Some("7-Zip 23.01 (x64)")
        );
        assert_eq!(
            name(r"C:\Users\me\AppData\Local\Discord\app-1.0").as_deref(),
            Some("Discord")
        );
        assert_eq!(
            name(r"C:\Users\me\AppData\Local\Google\Chrome\User Data").as_deref(),
            Some("Google Chrome")
        );
        assert_eq!(
            name(r"C:\Users\me\AppData\Roaming\Telegram Desktop\tdata").as_deref(),
            Some("Telegram Desktop")
        );
        assert_eq!(
            name(r"C:\Users\me\AppData\Roaming\Microsoft\Teams").as_deref(),
            Some("Microsoft Teams")
        );
        assert_eq!(
            name(r"C:\Users\me\AppData\Roaming\7-zip").as_deref(),
            Some("7-Zip 23.01 (x64)")
        );
        for p in [
            r"C:\Users\me\AppData\Local\Temp\x",
            r"C:\Users\me\Documents\Discord",
            r"C:\Program Files\7-Zipper",
            r"C:\Users\me\AppData\Local\Unknown",
        ] {
            assert_eq!(name(p), None, "{p}");
        }
    }

    /// Reads this PC's real registry: the runner has many programs installed.
    #[cfg(windows)]
    #[test]
    fn lists_the_programs_installed_on_this_pc() {
        let home = dirs::home_dir().unwrap();
        let index = build_index(&home);
        assert!(!index.apps.is_empty(), "no installed programs found");
        for a in index.apps.iter().take(15) {
            eprintln!(
                "{} | {} | {:?} | {:?} | {:?}",
                a.name,
                a.bundle_id,
                a.win.install_dir,
                a.win.uninstaller.as_ref().and_then(|u| u.command()),
                a.win.store
            );
        }
        let with_icon = index
            .apps
            .iter()
            .filter(|a| icon_png(a, 32).is_some())
            .count();
        eprintln!("{} programs, {with_icon} with an icon", index.apps.len());
        assert!(with_icon > 0);
    }
}
