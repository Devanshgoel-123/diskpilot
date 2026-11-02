//! Older versions, leftovers, installers and duplicate downloads.
//!
//! Every finding rests on evidence that can be shown to the person: a newer
//! version of the same thing sits next to it, the app it belongs to is not
//! installed, the app an installer installs is already in Applications, or a
//! file is byte-for-byte identical to the one it was downloaded next to.
//! Nothing is flagged only because it is old.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::{
    modified_ms, name_of, outermost_total, subdirs, Ctx, ItemState, RecSpec, ToolItem, ToolReport,
    ToolSection, DAY_MS,
};
use crate::model::{CleanupMethod, Confidence, MarkerKind, Recommendation, Tier};

const SOURCE: &str = "versions";
const TRASH: &[CleanupMethod] = &[CleanupMethod::Trash];
const TRASH_OR_DELETE: &[CleanupMethod] = &[CleanupMethod::Trash, CleanupMethod::Delete];
/// Installers for apps that are not installed are mentioned after this long.
const OLD_INSTALLER_DAYS: i64 = 30;

// ---- Versions ------------------------------------------------------------

/// Numeric components of the first version-like run in `s`
/// ("gradle-8.14.3-all" -> [8, 14, 3]). Requires at least one dot.
pub fn parse_version(s: &str) -> Option<Vec<u64>> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            let run = s[i..j].trim_end_matches('.');
            if run.contains('.') {
                let parts: Option<Vec<u64>> = run.split('.').map(|p| p.parse().ok()).collect();
                if parts.is_some() {
                    return parts;
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

fn version_string(v: &[u64]) -> String {
    v.iter().map(u64::to_string).collect::<Vec<_>>().join(".")
}

/// Lowercase letters and digits only.
fn normalize(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

struct Found {
    items: Vec<ToolItem>,
    recs: Vec<Recommendation>,
    min_recommended_bytes: u64,
}

impl Found {
    fn new(min_recommended_bytes: u64) -> Self {
        Self {
            items: Vec::new(),
            recs: Vec::new(),
            min_recommended_bytes,
        }
    }

    fn item(&mut self, label: String, path: &Path, bytes: u64, detail: String, state: ItemState) {
        self.items.push(ToolItem {
            label,
            path: Some(path.display().to_string()),
            bytes,
            detail,
            state,
            last_used_ms: modified_ms(path),
            recommendation_id: None,
        });
    }

    fn flag(&mut self, item: ToolItem, rec: Recommendation) {
        let mut item = item;
        if rec.bytes >= self.min_recommended_bytes {
            item.recommendation_id = Some(rec.id.clone());
            self.recs.push(rec);
        }
        self.items.push(item);
    }

    fn section(self, title: &str, description: &str) -> Option<(ToolSection, Vec<Recommendation>)> {
        if self.items.is_empty() {
            return None;
        }
        let mut items = self.items;
        items.sort_by_key(|i| (i.state == ItemState::Current, std::cmp::Reverse(i.bytes)));
        Some((
            ToolSection {
                title: title.into(),
                description: description.into(),
                items,
            },
            self.recs,
        ))
    }
}

/// What a flagged item is and what to do with it.
struct Finding<'a> {
    title: String,
    path: &'a Path,
    tier: Tier,
    methods: &'a [CleanupMethod],
    detected_because: String,
    safe_because: &'a str,
    risks: &'a str,
    proposed_action: &'a str,
    alternative: Option<String>,
    marker: (MarkerKind, String),
    state: ItemState,
}

impl Finding<'_> {
    fn record(self, ctx: &Ctx, found: &mut Found) {
        let bytes = ctx.size(self.path);
        if bytes == 0 {
            return;
        }
        let (kind, detail) = self.marker;
        let rec = RecSpec {
            source: SOURCE,
            title: self.title.clone(),
            path: self.path,
            bytes,
            tier: self.tier,
            methods: self.methods,
            detected_because: self.detected_because,
            safe_because: self.safe_because,
            risks: self.risks,
            proposed_action: self.proposed_action,
            alternative: self.alternative.as_deref(),
            last_activity_ms: modified_ms(self.path),
            notes: Vec::new(),
        }
        .build(ctx)
        .marked(kind, detail.clone());
        // The evidence (a newer version next to it, a missing app, identical
        // bytes) is direct, unlike a guess from the path alone.
        let rec = Recommendation {
            confidence: Confidence::High,
            ..rec
        };
        let item = ToolItem {
            label: self.title,
            path: Some(self.path.display().to_string()),
            bytes,
            detail,
            state: self.state,
            last_used_ms: modified_ms(self.path),
            recommendation_id: None,
        };
        found.flag(item, rec);
    }
}

// ---- JetBrains IDEs and Android Studio -----------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum IdeData {
    Caches,
    Logs,
    Settings,
}

impl IdeData {
    fn noun(self) -> &'static str {
        match self {
            IdeData::Caches => "caches",
            IdeData::Logs => "logs",
            IdeData::Settings => "settings",
        }
    }
}

const IDE_ROOTS: &[(&str, IdeData, bool)] = &[
    ("Library/Caches/JetBrains", IdeData::Caches, false),
    ("Library/Logs/JetBrains", IdeData::Logs, false),
    (
        "Library/Application Support/JetBrains",
        IdeData::Settings,
        false,
    ),
    // Android Studio keeps its folders under Google; only its own count.
    ("Library/Caches/Google", IdeData::Caches, true),
    ("Library/Logs/Google", IdeData::Logs, true),
    (
        "Library/Application Support/Google",
        IdeData::Settings,
        true,
    ),
];

const IDE_NAMES: &[(&str, &str)] = &[
    ("IntelliJIdea", "IntelliJ IDEA"),
    ("IdeaIC", "IntelliJ IDEA CE"),
    ("PyCharm", "PyCharm"),
    ("PyCharmCE", "PyCharm CE"),
    ("WebStorm", "WebStorm"),
    ("GoLand", "GoLand"),
    ("CLion", "CLion"),
    ("Rider", "Rider"),
    ("RubyMine", "RubyMine"),
    ("PhpStorm", "PhpStorm"),
    ("RustRover", "RustRover"),
    ("DataGrip", "DataGrip"),
    ("DataSpell", "DataSpell"),
    ("AppCode", "AppCode"),
    ("Aqua", "Aqua"),
    ("Writerside", "Writerside"),
    ("AndroidStudio", "Android Studio"),
];

/// Split "DataGrip2025.2" into ("DataGrip", [2025, 2]).
fn split_ide_folder(name: &str) -> Option<(&str, Vec<u64>)> {
    let pos = name.find(|c: char| c.is_ascii_digit())?;
    let (product, rest) = name.split_at(pos);
    if product.is_empty() || !product.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let year = rest.split('.').next()?;
    if year.len() != 4 || !rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some((product, parse_version(rest)?))
}

fn ide_display(product: &str) -> String {
    IDE_NAMES
        .iter()
        .find(|(k, _)| *k == product)
        .map_or_else(|| product.to_string(), |(_, v)| (*v).to_string())
}

/// Whether an installed app looks like this IDE (IntelliJ IDEA Ultimate counts
/// for "IntelliJIdea", PyCharm Professional for "PyCharm").
fn ide_installed(product: &str, apps: &[String]) -> bool {
    let display = normalize(&ide_display(product));
    let key = display.strip_suffix("ce").unwrap_or(&display);
    apps.iter().any(|a| normalize(a).starts_with(key))
}

fn ides(ctx: &Ctx, opts: &Options) -> Option<(ToolSection, Vec<Recommendation>)> {
    // product -> [(version, kind, path)]
    let mut by_product: BTreeMap<String, Vec<(Vec<u64>, IdeData, PathBuf)>> = BTreeMap::new();
    for (rel, kind, android_only) in IDE_ROOTS {
        for dir in subdirs(&ctx.home_path(rel)) {
            let name = name_of(&dir);
            let Some((product, version)) = split_ide_folder(&name) else {
                continue;
            };
            if *android_only != (product == "AndroidStudio") {
                continue;
            }
            by_product
                .entry(product.to_string())
                .or_default()
                .push((version, *kind, dir));
        }
    }

    let mut found = Found::new(opts.min_recommended_bytes);
    for (product, entries) in by_product {
        let display = ide_display(&product);
        let installed = ide_installed(&product, opts.apps);
        let Some(newest) = entries.iter().map(|(v, _, _)| v.clone()).max() else {
            continue;
        };
        let newest_s = version_string(&newest);
        for (version, kind, path) in entries {
            let v = version_string(&version);
            let title = format!("{display} {v} {}", kind.noun());
            let settings = kind == IdeData::Settings;
            if version < newest {
                Finding {
                    title,
                    path: &path,
                    tier: if settings { Tier::ReviewFirst } else { Tier::LikelySafe },
                    methods: if settings { TRASH } else { TRASH_OR_DELETE },
                    detected_because: format!(
                        "Folder for {display} {v}. Folders for {display} {newest_s} are also on this Mac, so a newer version has been in use."
                    ),
                    safe_because: if settings {
                        "A newer version imports settings from the previous one the first time it opens, so these are a copy."
                    } else {
                        "Each version keeps its own caches and logs. The newer version does not read these."
                    },
                    risks: if settings {
                        "Settings, plugins and keymaps you changed only in this version and never carried over are lost."
                    } else {
                        "If you still open this older version, it rebuilds its indexes on the next launch."
                    },
                    proposed_action: "Move the older version's folder to the Trash",
                    alternative: None,
                    marker: (MarkerKind::OlderVersion, format!("{display} {newest_s} is newer")),
                    state: ItemState::Superseded,
                }
                .record(ctx, &mut found);
            } else if !installed {
                Finding {
                    title,
                    path: &path,
                    tier: if settings { Tier::ReviewFirst } else { Tier::LikelySafe },
                    methods: if settings { TRASH } else { TRASH_OR_DELETE },
                    detected_because: format!(
                        "Folder for {display} {v}. No {display} app was found in Applications."
                    ),
                    safe_because: if settings {
                        "Nothing uses these settings while the IDE is not installed."
                    } else {
                        "Caches and logs are only used by the IDE, which is not installed."
                    },
                    risks: if settings {
                        "If you reinstall the IDE later, it starts with default settings."
                    } else {
                        "If the IDE is installed somewhere DiskPilot does not look, it rebuilds its indexes on the next launch."
                    },
                    proposed_action: "Move the folder to the Trash",
                    alternative: None,
                    marker: (MarkerKind::Leftover, format!("{display} is not installed")),
                    state: ItemState::Orphaned,
                }
                .record(ctx, &mut found);
            } else {
                let bytes = ctx.size(&path);
                found.item(
                    title,
                    &path,
                    bytes,
                    "Current version".into(),
                    ItemState::Current,
                );
            }
        }
    }
    found.section(
        "JetBrains IDEs and Android Studio",
        "Each IDE version keeps its own caches, logs and settings, and they stay after an update.",
    )
}

// ---- Xcode device support -------------------------------------------------

const DEVICE_SUPPORT: &[&str] = &["iOS", "watchOS", "tvOS", "visionOS", "xrOS"];

fn device_support(ctx: &Ctx, opts: &Options) -> Option<(ToolSection, Vec<Recommendation>)> {
    let mut found = Found::new(opts.min_recommended_bytes);
    for platform in DEVICE_SUPPORT {
        let dir = ctx.home_path(&format!("Library/Developer/Xcode/{platform} DeviceSupport"));
        let entries: Vec<(Vec<u64>, PathBuf)> = subdirs(&dir)
            .into_iter()
            .filter_map(|p| {
                // "17.0 (21A329)" or "iPhone15,2 17.0.1 (21A350)"
                let name = name_of(&p);
                let v = name
                    .split_whitespace()
                    .filter(|t| t.chars().all(|c| c.is_ascii_digit() || c == '.'))
                    .find_map(|t| {
                        if t.contains('.') {
                            parse_version(t)
                        } else {
                            t.parse().ok().map(|n| vec![n])
                        }
                    })?;
                Some((v, p))
            })
            .collect();
        let Some(newest) = entries.iter().map(|(v, _)| v.clone()).max() else {
            continue;
        };
        let newest_s = version_string(&newest);
        for (version, path) in entries {
            let title = format!("{platform} {} device support", name_of(&path));
            if version < newest {
                Finding {
                    title,
                    path: &path,
                    tier: Tier::LikelySafe,
                    methods: TRASH_OR_DELETE,
                    detected_because: format!(
                        "Xcode copied these debugging symbols from a device running {platform} {}. Support for {platform} {newest_s} is also present.",
                        version_string(&version)
                    ),
                    safe_because: "Xcode copies the symbols again from a device running this version when it connects.",
                    risks: "Connecting a device that still runs this version takes a few extra minutes the first time.",
                    proposed_action: "Move the folder to the Trash",
                    alternative: None,
                    marker: (MarkerKind::OlderVersion, format!("{platform} {newest_s} support is newer")),
                    state: ItemState::Superseded,
                }
                .record(ctx, &mut found);
            } else {
                let bytes = ctx.size(&path);
                found.item(
                    title,
                    &path,
                    bytes,
                    "Newest version".into(),
                    ItemState::Current,
                );
            }
        }
    }
    found.section(
        "Xcode device support",
        "Debugging symbols Xcode copies from each iPhone, Watch or Apple TV version you connect.",
    )
}

// ---- Android SDK and Gradle ----------------------------------------------

/// Flag every version folder in `dir` older than the newest one.
#[allow(clippy::too_many_arguments)]
fn versioned_dirs(
    ctx: &Ctx,
    found: &mut Found,
    dir: &Path,
    what: &str,
    tier: Tier,
    methods: &'static [CleanupMethod],
    safe_because: &'static str,
    risks: &'static str,
) {
    let entries: Vec<(Vec<u64>, PathBuf)> = subdirs(dir)
        .into_iter()
        .filter_map(|p| Some((parse_version(&name_of(&p))?, p)))
        .collect();
    let Some(newest) = entries.iter().map(|(v, _)| v.clone()).max() else {
        return;
    };
    let newest_s = version_string(&newest);
    for (version, path) in entries {
        let v = version_string(&version);
        let title = format!("{what} {v}");
        if version < newest {
            Finding {
                title,
                path: &path,
                tier,
                methods,
                detected_because: format!(
                    "{what} {v} and the newer {newest_s} are both installed."
                ),
                safe_because,
                risks,
                proposed_action: "Move the older version to the Trash",
                alternative: None,
                marker: (
                    MarkerKind::OlderVersion,
                    format!("{what} {newest_s} is newer"),
                ),
                state: ItemState::Superseded,
            }
            .record(ctx, found);
        } else {
            let bytes = ctx.size(&path);
            found.item(
                title,
                &path,
                bytes,
                "Newest version".into(),
                ItemState::Current,
            );
        }
    }
}

fn android(ctx: &Ctx, opts: &Options) -> Option<(ToolSection, Vec<Recommendation>)> {
    let sdk = ctx.home_path("Library/Android/sdk");
    let mut found = Found::new(opts.min_recommended_bytes);
    for (sub, what) in [
        ("build-tools", "Android build tools"),
        ("ndk", "Android NDK"),
    ] {
        versioned_dirs(
            ctx,
            &mut found,
            &sdk.join(sub),
            what,
            Tier::ReviewFirst,
            TRASH,
            "Android Studio and Gradle download a version again when a project asks for it.",
            "A project that pins this version downloads it again on its next build.",
        );
    }
    found.section(
        "Android SDK",
        "Android Studio installs new SDK tools next to old ones and does not remove them.",
    )
}

fn gradle(ctx: &Ctx, opts: &Options) -> Option<(ToolSection, Vec<Recommendation>)> {
    let mut found = Found::new(opts.min_recommended_bytes);
    versioned_dirs(
        ctx,
        &mut found,
        &ctx.home_path(".gradle/wrapper/dists"),
        "Gradle",
        Tier::ReviewFirst,
        TRASH,
        "The Gradle wrapper downloads a distribution again when a project needs it.",
        "A project that pins this Gradle version downloads it again on its next build.",
    );
    // Per-version caches: ~/.gradle/caches/8.5, ~/.gradle/caches/8.14.3
    versioned_dirs(
        ctx,
        &mut found,
        &ctx.home_path(".gradle/caches"),
        "Gradle cache for",
        Tier::LikelySafe,
        TRASH_OR_DELETE,
        "Each Gradle version keeps its own cache here, and newer versions do not read older ones.",
        "A project still on this Gradle version rebuilds the cache on its next build.",
    );
    found.section(
        "Gradle",
        "Gradle keeps each distribution and cache version it has used.",
    )
}

// ---- Language versions (nvm, pyenv, rbenv) --------------------------------

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().trim_start_matches('v').to_string())
}

struct Runtime {
    name: &'static str,
    dir: &'static str,
    /// Files naming the version in use, which is never flagged.
    in_use: &'static [&'static str],
    /// Group versions by this many leading components (1: major, 2: minor).
    group: usize,
    uninstall: &'static str,
    methods: &'static [CleanupMethod],
    risks: &'static str,
}

/// (parsed version, folder name without a leading "v", folder)
type VersionDir = (Vec<u64>, String, PathBuf);

const RUNTIMES: &[Runtime] = &[
    Runtime {
        name: "Node.js",
        dir: ".nvm/versions/node",
        in_use: &[".nvm/alias/default"],
        group: 1,
        uninstall: "nvm uninstall",
        methods: TRASH,
        risks: "Global npm packages installed under this version are removed with it.",
    },
    Runtime {
        name: "Python",
        dir: ".pyenv/versions",
        in_use: &[".pyenv/version"],
        group: 2,
        uninstall: "pyenv uninstall",
        // Virtual environments point at the interpreter; leave it to pyenv.
        methods: &[],
        risks: "Virtual environments created with this interpreter stop working.",
    },
    Runtime {
        name: "Ruby",
        dir: ".rbenv/versions",
        in_use: &[".rbenv/version"],
        group: 2,
        uninstall: "rbenv uninstall",
        methods: &[],
        risks: "Gems installed for this version are removed with it.",
    },
];

fn runtimes(ctx: &Ctx, opts: &Options) -> Option<(ToolSection, Vec<Recommendation>)> {
    let mut found = Found::new(opts.min_recommended_bytes);
    for rt in RUNTIMES {
        let in_use: HashSet<String> = rt
            .in_use
            .iter()
            .filter_map(|f| read_trimmed(&ctx.home_path(f)))
            .flat_map(|s| s.lines().map(|l| l.trim().to_string()).collect::<Vec<_>>())
            .collect();
        // Only plain version folders ("3.11.4", "v20.1.0"); skip names like
        // "miniconda3-latest" or "3.12.0/envs/x".
        let mut groups: BTreeMap<Vec<u64>, Vec<VersionDir>> = BTreeMap::new();
        for p in subdirs(&ctx.home_path(rt.dir)) {
            let name = name_of(&p);
            let bare = name.trim_start_matches('v');
            if !bare.chars().all(|c| c.is_ascii_digit() || c == '.') {
                continue;
            }
            let Some(v) = parse_version(bare) else {
                continue;
            };
            let key = v.iter().take(rt.group).copied().collect();
            groups
                .entry(key)
                .or_default()
                .push((v, bare.to_string(), p));
        }
        for entries in groups.into_values() {
            let Some(newest) = entries.iter().map(|(v, _, _)| v.clone()).max() else {
                continue;
            };
            let newest_s = version_string(&newest);
            for (version, name, path) in entries {
                let label = format!("{} {name}", rt.name);
                let used = in_use
                    .iter()
                    .any(|u| u == &name || (u.contains('.') && name.starts_with(&format!("{u}."))));
                if version < newest && !used {
                    Finding {
                        title: label,
                        path: &path,
                        tier: Tier::ReviewFirst,
                        methods: rt.methods,
                        detected_because: format!(
                            "{} {name} and the newer {newest_s} in the same release line are both installed, and {name} is not the default.",
                            rt.name
                        ),
                        safe_because: "Patch releases in the same line are compatible, so projects can use the newer one.",
                        risks: rt.risks,
                        proposed_action: if rt.methods.is_empty() {
                            "Remove it with the version manager"
                        } else {
                            "Move the older version to the Trash"
                        },
                        alternative: Some(format!("{} {name}", rt.uninstall)),
                        marker: (MarkerKind::OlderVersion, format!("{} {newest_s} is newer", rt.name)),
                        state: ItemState::Superseded,
                    }
                    .record(ctx, &mut found);
                } else {
                    let bytes = ctx.size(&path);
                    let detail = if used {
                        "Default version"
                    } else {
                        "Newest in its line"
                    };
                    found.item(label, &path, bytes, detail.into(), ItemState::Current);
                }
            }
        }
    }
    found.section(
        "Language versions",
        "Node.js (nvm), Python (pyenv) and Ruby (rbenv) keep every version you install.",
    )
}

// ---- Homebrew -------------------------------------------------------------

const BREW_PREFIXES: &[&str] = &["/opt/homebrew", "/usr/local"];

fn homebrew(ctx: &Ctx, opts: &Options) -> Option<(ToolSection, Vec<Recommendation>)> {
    let mut found = Found::new(opts.min_recommended_bytes);
    for prefix in opts.brew_prefixes {
        for (kind, root) in [
            ("Cellar", prefix.join("Cellar")),
            ("Caskroom", prefix.join("Caskroom")),
        ] {
            for package in subdirs(&root) {
                let versions: Vec<PathBuf> = subdirs(&package)
                    .into_iter()
                    .filter(|p| !name_of(p).starts_with('.'))
                    .collect();
                if versions.len() < 2 {
                    continue;
                }
                let name = name_of(&package);
                // The linked version is current; otherwise the newest.
                let linked = fs::read_link(prefix.join("opt").join(&name))
                    .ok()
                    .and_then(|t| t.file_name().map(|n| n.to_string_lossy().into_owned()));
                let newest = versions
                    .iter()
                    .max_by_key(|p| parse_version(&name_of(p)).unwrap_or_default())
                    .map(|p| name_of(p));
                let current = if kind == "Cellar" {
                    linked.or(newest)
                } else {
                    newest
                };
                let Some(current) = current else { continue };
                for path in versions {
                    let v = name_of(&path);
                    let title = format!("Homebrew {name} {v}");
                    if v == current {
                        let bytes = ctx.size(&path);
                        found.item(
                            title,
                            &path,
                            bytes,
                            "Version in use".into(),
                            ItemState::Current,
                        );
                        continue;
                    }
                    Finding {
                        title,
                        path: &path,
                        tier: Tier::ReviewFirst,
                        methods: &[],
                        detected_because: format!(
                            "Homebrew keeps {name} {v} after upgrading; {current} is the version in use."
                        ),
                        safe_because: "Homebrew runs the version in use; older ones stay until `brew cleanup`.",
                        risks: "Something pinned to this exact version would need it reinstalled.",
                        proposed_action: "Remove it with Homebrew",
                        alternative: Some(format!("brew cleanup {name}")),
                        marker: (MarkerKind::OlderVersion, format!("{name} {current} is in use")),
                        state: ItemState::Superseded,
                    }
                    .record(ctx, &mut found);
                }
            }
        }
    }
    found.section(
        "Homebrew",
        "Older package versions Homebrew keeps until `brew cleanup` runs. DiskPilot shows the command; Homebrew removes them.",
    )
}

// ---- Downloads: installers and duplicates ---------------------------------

const INSTALLER_EXTS: &[&str] = &["dmg", "pkg", "mpkg", "xip"];
/// Words in installer names that say nothing about which app it installs.
const NOISE: &[&str] = &[
    "installer",
    "install",
    "setup",
    "macos",
    "mac",
    "osx",
    "darwin",
    "arm64",
    "aarch64",
    "x64",
    "x8664",
    "intel",
    "universal",
    "apple",
    "silicon",
    "latest",
    "release",
    "stable",
    "beta",
];
const APP_ALIASES: &[(&str, &str)] = &[
    ("vscode", "visualstudiocode"),
    ("code", "visualstudiocode"),
    ("chrome", "googlechrome"),
    ("teams", "microsoftteams"),
    ("word", "microsoftword"),
    ("excel", "microsoftexcel"),
    ("iterm", "iterm2"),
];

/// The app name an installer file name refers to: "Cursor-darwin-arm64.dmg"
/// -> "cursor", "ChatGPTInstaller.dmg" -> "chatgpt".
pub fn installer_key(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut key = String::new();
    for token in stem.split(|c: char| !c.is_ascii_alphanumeric()) {
        let t = token.to_ascii_lowercase();
        if t.is_empty() || t.chars().all(|c| c.is_ascii_digit()) || NOISE.contains(&t.as_str()) {
            continue;
        }
        // Version-like tokens: "v1", "4b2".
        if t.starts_with('v') && t[1..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        key.push_str(&t);
    }
    for suffix in ["installer", "setup", "install"] {
        if let Some(k) = key.strip_suffix(suffix) {
            key = k.to_string();
        }
    }
    APP_ALIASES
        .iter()
        .find(|(a, _)| *a == key)
        .map_or(key, |(_, full)| (*full).to_string())
}

/// The installed app an installer is for, by name.
fn installed_app_for(file_name: &str, apps: &[String]) -> Option<String> {
    let key = installer_key(file_name);
    if key.len() < 3 {
        return None;
    }
    apps.iter()
        .filter(|a| {
            let n = normalize(a);
            n.len() >= 3
                && (n == key || (key.len() >= 4 && n.starts_with(&key)) || key.starts_with(&n))
        })
        .max_by_key(|a| a.len())
        .cloned()
}

/// "Report (2).pdf" -> "Report.pdf"
pub fn original_name(file_name: &str) -> Option<String> {
    let path = Path::new(file_name);
    let stem = path.file_stem()?.to_str()?;
    let open = stem.strip_suffix(')')?.rfind(" (")?;
    let n = &stem[open + 2..stem.len() - 1];
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let base = &stem[..open];
    Some(match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{base}.{ext}"),
        None => base.to_string(),
    })
}

/// Byte-for-byte comparison of two regular files.
fn same_contents(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::symlink_metadata(a), fs::symlink_metadata(b)) else {
        return false;
    };
    if !ma.is_file() || !mb.is_file() || ma.len() != mb.len() || ma.len() == 0 {
        return false;
    }
    let (Ok(fa), Ok(fb)) = (fs::File::open(a), fs::File::open(b)) else {
        return false;
    };
    let (mut ra, mut rb) = (
        std::io::BufReader::with_capacity(1 << 16, fa),
        std::io::BufReader::with_capacity(1 << 16, fb),
    );
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let (Ok(na), Ok(nb)) = (read_full(&mut ra, &mut ba), read_full(&mut rb, &mut bb)) else {
            return false;
        };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

fn downloads(ctx: &Ctx, opts: &Options) -> Option<(ToolSection, Vec<Recommendation>)> {
    let dir = ctx.home_path("Downloads");
    let Ok(entries) = fs::read_dir(&dir) else {
        return None;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    files.sort();

    let mut found = Found::new(opts.min_recommended_bytes);
    let mut flagged: HashSet<PathBuf> = HashSet::new();

    // Duplicates first: "name (1).ext" identical to "name.ext".
    for path in &files {
        let name = name_of(path);
        let Some(original) = original_name(&name) else {
            continue;
        };
        let original_path = dir.join(&original);
        if !same_contents(path, &original_path) {
            continue;
        }
        Finding {
            title: format!("Duplicate download · {name}"),
            path,
            tier: Tier::LikelySafe,
            methods: TRASH,
            detected_because: format!(
                "Identical, byte for byte, to {original} in the same folder. Browsers add a number when the same file is downloaded twice."
            ),
            safe_because: "The original copy stays where it is.",
            risks: "None for the contents. Anything that refers to this copy by name would need the original's name.",
            proposed_action: "Move the copy to the Trash",
            alternative: None,
            marker: (MarkerKind::Duplicate, format!("Same as {original}")),
            state: ItemState::Orphaned,
        }
        .record(ctx, &mut found);
        flagged.insert(path.clone());
    }

    for path in &files {
        if flagged.contains(path) {
            continue;
        }
        let name = name_of(path);
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if !INSTALLER_EXTS.contains(&ext.as_str()) {
            continue;
        }
        if let Some(app) = installed_app_for(&name, opts.apps) {
            Finding {
                title: format!("Installer · {name}"),
                path,
                tier: Tier::LikelySafe,
                methods: TRASH,
                detected_because: format!(
                    "A disk image or package in Downloads whose name matches {app}, which is installed in Applications."
                ),
                safe_because: "Apps don't need their installer after they are installed and usually update themselves.",
                risks: "To reinstall this exact version later, you would download it again.",
                proposed_action: "Move the installer to the Trash",
                alternative: None,
                marker: (MarkerKind::Installer, format!("{app} is installed")),
                state: ItemState::Orphaned,
            }
            .record(ctx, &mut found);
            continue;
        }
        let Some(days) = modified_ms(path).map(|m| (ctx.now_ms - m) / DAY_MS) else {
            continue;
        };
        if days >= OLD_INSTALLER_DAYS {
            Finding {
                title: format!("Installer · {name}"),
                path,
                tier: Tier::ReviewFirst,
                methods: TRASH,
                detected_because: format!(
                    "A disk image or package downloaded {days} days ago. DiskPilot found no installed app with a matching name."
                ),
                safe_because: "Installers can be downloaded again from where they came from.",
                risks: "If you never installed it, or it is hard to find again, keep it.",
                proposed_action: "Move the installer to the Trash",
                alternative: None,
                marker: (MarkerKind::Installer, format!("Downloaded {days} days ago")),
                state: ItemState::Stale,
            }
            .record(ctx, &mut found);
        }
    }
    found.section(
        "Downloads",
        "Installers for apps that are already installed, and files downloaded twice.",
    )
}

// ---- Report ---------------------------------------------------------------

/// What the analysis needs from outside the home folder.
pub struct Options<'a> {
    /// Names of installed apps.
    pub apps: &'a [String],
    /// Homebrew prefixes to look in.
    pub brew_prefixes: &'a [PathBuf],
    /// Smaller findings are listed in the report but not recommended.
    pub min_recommended_bytes: u64,
}

pub const MIN_RECOMMENDED_BYTES: u64 = 1_000_000;

pub fn analyze_with(ctx: &Ctx, env: &Options) -> (ToolReport, Vec<Recommendation>) {
    let mut sections = Vec::new();
    let mut recs = Vec::new();
    for (section, mut r) in [
        ides(ctx, env),
        device_support(ctx, env),
        android(ctx, env),
        gradle(ctx, env),
        runtimes(ctx, env),
        homebrew(ctx, env),
        downloads(ctx, env),
    ]
    .into_iter()
    .flatten()
    {
        sections.push(section);
        recs.append(&mut r);
    }
    let roots: Vec<(PathBuf, u64)> = sections
        .iter()
        .flat_map(|s| &s.items)
        .filter_map(|i| Some((PathBuf::from(i.path.as_ref()?), i.bytes)))
        .collect();
    let summary = if recs.is_empty() {
        "No older versions, leftovers or spare installers found.".to_string()
    } else {
        format!(
            "{} item(s) replaced by something newer or no longer needed.",
            recs.len()
        )
    };
    (
        ToolReport {
            id: SOURCE.into(),
            name: "Older versions and leftovers".into(),
            detected: !sections.is_empty(),
            version: None,
            total_bytes: outermost_total(&roots),
            summary,
            sections,
        },
        recs,
    )
}

pub fn analyze(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let prefixes: Vec<PathBuf> = BREW_PREFIXES.iter().map(PathBuf::from).collect();
    analyze_with(
        ctx,
        &Options {
            apps: &crate::apps::installed_names(ctx.home),
            brew_prefixes: &prefixes,
            min_recommended_bytes: MIN_RECOMMENDED_BYTES,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ctx, write};
    use super::*;

    fn env(apps: &[String]) -> Options<'_> {
        Options {
            apps,
            brew_prefixes: &[],
            min_recommended_bytes: 0,
        }
    }

    fn titles(recs: &[Recommendation]) -> Vec<&str> {
        let mut t: Vec<&str> = recs.iter().map(|r| r.title.as_str()).collect();
        t.sort_unstable();
        t
    }

    #[test]
    fn parses_versions_from_names() {
        assert_eq!(parse_version("gradle-8.14.3-all"), Some(vec![8, 14, 3]));
        assert_eq!(parse_version("17.0 (21A329)"), Some(vec![17, 0]));
        assert_eq!(parse_version("2025.2"), Some(vec![2025, 2]));
        assert_eq!(parse_version("python3"), None);
        assert_eq!(
            split_ide_folder("DataGrip2025.2"),
            Some(("DataGrip", vec![2025, 2]))
        );
        assert_eq!(
            split_ide_folder("PyCharmCE2023.1"),
            Some(("PyCharmCE", vec![2023, 1]))
        );
        assert_eq!(split_ide_folder("Toolbox"), None);
        assert_eq!(split_ide_folder("consentOptions"), None);
    }

    #[test]
    fn reads_app_names_from_installer_names() {
        assert_eq!(installer_key("ChatGPTInstaller.dmg"), "chatgpt");
        assert_eq!(installer_key("Cursor-darwin-arm64.dmg"), "cursor");
        assert_eq!(installer_key("Slack-4.41.105-macOS.dmg"), "slack");
        assert_eq!(
            installer_key("VSCode-darwin-universal.zip"),
            "visualstudiocode"
        );
        assert_eq!(installer_key("googlechrome.dmg"), "googlechrome");
        let apps = vec![
            "Google Chrome".to_string(),
            "Slack".to_string(),
            "Notes".to_string(),
        ];
        assert_eq!(
            installed_app_for("googlechrome.dmg", &apps).as_deref(),
            Some("Google Chrome")
        );
        assert_eq!(
            installed_app_for("Slack-4.41.105-macOS.dmg", &apps).as_deref(),
            Some("Slack")
        );
        assert_eq!(installed_app_for("Zoom.pkg", &apps), None);
        assert_eq!(
            original_name("Report (2).pdf").as_deref(),
            Some("Report.pdf")
        );
        assert_eq!(original_name("Report (draft).pdf"), None);
        assert_eq!(original_name("Report.pdf"), None);
    }

    #[test]
    fn flags_older_ide_folders_and_uninstalled_ides() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        for rel in [
            "Library/Caches/JetBrains/PyCharm2024.3/x",
            "Library/Caches/JetBrains/PyCharm2025.1/x",
            "Library/Application Support/JetBrains/PyCharm2024.3/x",
            "Library/Caches/JetBrains/DataGrip2025.2/x",
            "Library/Caches/Google/Chrome/x",
        ] {
            write(&home.join(rel), 5000);
        }
        let apps = vec!["PyCharm Professional Edition".to_string()];
        let (report, recs) = analyze_with(&ctx(home), &env(&apps));
        assert_eq!(
            titles(&recs),
            vec![
                "DataGrip 2025.2 caches",
                "PyCharm 2024.3 caches",
                "PyCharm 2024.3 settings"
            ]
        );
        let old_cache = recs
            .iter()
            .find(|r| r.title == "PyCharm 2024.3 caches")
            .unwrap();
        assert_eq!(old_cache.tier, Tier::LikelySafe);
        assert_eq!(
            old_cache.marker.as_ref().unwrap().kind,
            MarkerKind::OlderVersion
        );
        let settings = recs
            .iter()
            .find(|r| r.title == "PyCharm 2024.3 settings")
            .unwrap();
        assert_eq!(settings.tier, Tier::ReviewFirst);
        assert_eq!(settings.methods, vec![CleanupMethod::Trash]);
        let leftover = recs
            .iter()
            .find(|r| r.title.starts_with("DataGrip"))
            .unwrap();
        assert_eq!(leftover.marker.as_ref().unwrap().kind, MarkerKind::Leftover);
        // The current PyCharm version is listed but not flagged.
        let items: Vec<&ToolItem> = report.sections.iter().flat_map(|s| &s.items).collect();
        assert!(items
            .iter()
            .any(|i| i.label == "PyCharm 2025.1 caches" && i.state == ItemState::Current));
    }

    #[test]
    fn keeps_newest_device_support_sdk_and_gradle() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        for rel in [
            "Library/Developer/Xcode/iOS DeviceSupport/16.4 (20E247)/s",
            "Library/Developer/Xcode/iOS DeviceSupport/iPhone15,2 17.0.1 (21A350)/s",
            "Library/Android/sdk/build-tools/35.0.0/s",
            "Library/Android/sdk/build-tools/36.0.0/s",
            ".gradle/wrapper/dists/gradle-8.5-bin/s",
            ".gradle/wrapper/dists/gradle-8.14.3-all/s",
        ] {
            write(&home.join(rel), 5000);
        }
        let (_, recs) = analyze_with(&ctx(home), &env(&[]));
        assert_eq!(
            titles(&recs),
            vec![
                "Android build tools 35.0.0",
                "Gradle 8.5",
                "iOS 16.4 (20E247) device support"
            ]
        );
    }

    #[test]
    fn older_patch_versions_but_never_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        for rel in [
            ".nvm/versions/node/v20.1.0/bin/node",
            ".nvm/versions/node/v20.11.0/bin/node",
            ".nvm/versions/node/v22.11.0/bin/node",
            ".pyenv/versions/3.11.4/bin/python",
            ".pyenv/versions/3.11.9/bin/python",
            ".pyenv/versions/3.12.1/bin/python",
            ".pyenv/versions/miniconda3-latest/bin/python",
        ] {
            write(&home.join(rel), 5000);
        }
        // The default Node version is the older patch; it must not be flagged.
        fs::create_dir_all(home.join(".nvm/alias")).unwrap();
        fs::write(home.join(".nvm/alias/default"), "20.1.0\n").unwrap();
        let (_, recs) = analyze_with(&ctx(home), &env(&[]));
        assert_eq!(titles(&recs), vec!["Python 3.11.4"]);
        let py = &recs[0];
        assert!(py.methods.is_empty());
        assert_eq!(py.alternative.as_deref(), Some("pyenv uninstall 3.11.4"));
    }

    #[test]
    fn duplicate_downloads_and_installers_for_installed_apps() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let dl = home.join("Downloads");
        write(&dl.join("Slack-4.41.105-macOS.dmg"), 7000);
        write(&dl.join("Report.pdf"), 6000);
        write(&dl.join("Report (1).pdf"), 6000);
        // Same size, different bytes: not a duplicate.
        write(&dl.join("Notes.txt"), 3000);
        fs::write(dl.join("Notes (1).txt"), vec![2u8; 3000]).unwrap();
        write(&dl.join("Unknown.pkg"), 4000);
        let apps = vec!["Slack".to_string()];
        let (_, recs) = analyze_with(&ctx(home), &env(&apps));
        assert_eq!(
            titles(&recs),
            vec![
                "Duplicate download · Report (1).pdf",
                "Installer · Slack-4.41.105-macOS.dmg",
                "Installer · Unknown.pkg"
            ]
        );
        assert!(recs.iter().all(|r| r.methods == vec![CleanupMethod::Trash]));
        let tier = |t: &str| recs.iter().find(|r| r.title.ends_with(t)).unwrap().tier;
        assert_eq!(tier("Slack-4.41.105-macOS.dmg"), Tier::LikelySafe);
        // No matching app: only mentioned because it is old, and for review.
        assert_eq!(tier("Unknown.pkg"), Tier::ReviewFirst);
    }

    #[test]
    fn homebrew_versions_other_than_the_linked_one() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("brew");
        for rel in [
            "Cellar/pkgconf/2.5.1/bin/x",
            "Cellar/pkgconf/3.0.10/bin/x",
            "Cellar/git/2.51.0/bin/x",
        ] {
            write(&prefix.join(rel), 5000);
        }
        fs::create_dir_all(prefix.join("opt")).unwrap();
        std::os::unix::fs::symlink("../Cellar/pkgconf/3.0.10", prefix.join("opt/pkgconf")).unwrap();
        let home = dir.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let prefixes = vec![prefix];
        let (_, recs) = analyze_with(
            &ctx(&home),
            &Options {
                apps: &[],
                brew_prefixes: &prefixes,
                min_recommended_bytes: 0,
            },
        );
        assert_eq!(titles(&recs), vec!["Homebrew pkgconf 2.5.1"]);
        assert!(recs[0].methods.is_empty());
        assert_eq!(recs[0].alternative.as_deref(), Some("brew cleanup pkgconf"));
    }

    #[test]
    fn small_findings_are_listed_but_not_recommended() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        write(&home.join("Library/Android/sdk/build-tools/35.0.0/s"), 5000);
        write(&home.join("Library/Android/sdk/build-tools/36.0.0/s"), 5000);
        let (report, recs) = analyze_with(
            &ctx(home),
            &Options {
                apps: &[],
                brew_prefixes: &[],
                min_recommended_bytes: MIN_RECOMMENDED_BYTES,
            },
        );
        assert!(recs.is_empty());
        let old = report
            .sections
            .iter()
            .flat_map(|s| &s.items)
            .find(|i| i.label == "Android build tools 35.0.0")
            .unwrap();
        assert_eq!(old.state, ItemState::Superseded);
        assert!(old.recommendation_id.is_none());
    }
}
