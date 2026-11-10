//! Classification engine.
//!
//! Pure logic: given a path (and a way to check whether sibling/child entries
//! exist), decide which category it belongs to and why. The classifier has no
//! side effects and never executes any operation.

pub mod rules;
pub mod windows_paths;
pub mod windows_rules;

use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::model::{Category, Classification, Confidence};
use rules::{DirRule, FileRule, Matcher, Root, RuleKind, DIR_RULES, FILE_RULES};
pub use windows_paths::WindowsRoots;
use windows_paths::{eq_ignore_case, WinPath};
use windows_rules::{WINDOWS_DIR_RULES, WINDOWS_FILE_RULES, WINDOWS_NAMED_FILES};

/// Answers "does `dir` directly contain an entry called `name`?".
/// Abstracted so the scanner can answer from entries it already read and tests
/// can run without touching the filesystem.
pub trait Probe {
    fn has_child(&self, dir: &Path, name: &str) -> bool;

    /// Does `dir` directly contain a file with one of these extensions
    /// (compared ignoring case)? Lists the folder's names only; nothing is
    /// opened and no link is followed. Asked only after a rule's folder name
    /// matched, so it is rare.
    fn has_child_with_extension(&self, dir: &Path, extensions: &[&str]) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|e| {
            Path::new(&e.file_name())
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|ext| extensions.iter().any(|x| x.eq_ignore_ascii_case(ext)))
        })
    }
}

/// Probe backed by the real filesystem. Uses `symlink_metadata`, so it never
/// follows symbolic links.
pub struct FsProbe;

impl Probe for FsProbe {
    fn has_child(&self, dir: &Path, name: &str) -> bool {
        dir.join(name).symlink_metadata().is_ok()
    }
}

/// The directory rule in effect for a subtree, plus where it matched.
#[derive(Debug, Clone)]
pub struct Context {
    pub rule: &'static DirRule,
    pub path: Arc<Path>,
}

/// A cheap, allocation-free classification result used in hot scan loops.
/// Convert to a [`Classification`] only for results sent to the UI.
#[derive(Debug, Clone)]
pub enum Resolved {
    Dir(Context),
    File(&'static FileRule),
    Unmatched,
}

impl Resolved {
    pub fn category(&self) -> Category {
        match self {
            Resolved::Dir(ctx) => ctx.rule.category,
            Resolved::File(rule) => rule.category,
            Resolved::Unmatched => Category::Unknown,
        }
    }

    pub fn to_classification(&self) -> Classification {
        match self {
            Resolved::Dir(ctx) => Classification {
                category: ctx.rule.category,
                label: ctx.rule.label.to_string(),
                explanation: ctx.rule.explanation.to_string(),
                rule_id: ctx.rule.id.to_string(),
                confidence: ctx.rule.confidence,
                matched_path: Some(ctx.path.display().to_string()),
            },
            Resolved::File(rule) => Classification {
                category: rule.category,
                label: rule.label.to_string(),
                explanation: rule.explanation.to_string(),
                rule_id: rule.id.to_string(),
                confidence: rule.confidence,
                matched_path: None,
            },
            Resolved::Unmatched => unmatched(),
        }
    }
}

/// Classifies many paths that share ancestors, evaluating each folder's
/// rules once. Gives the same results as [`Classifier::classify_path`].
pub struct CachedClassifier<'a> {
    classifier: &'a Classifier,
    probe: &'a dyn Probe,
    contexts: HashMap<PathBuf, Option<Context>>,
}

impl<'a> CachedClassifier<'a> {
    pub fn new(classifier: &'a Classifier, probe: &'a dyn Probe) -> Self {
        Self {
            classifier,
            probe,
            contexts: HashMap::new(),
        }
    }

    /// Same as [`Classifier::context_at`].
    pub fn context_at(&mut self, dir: &Path) -> Option<Context> {
        if self.classifier.windows.is_some() {
            return self.classifier.context_at(dir, self.probe);
        }
        if let Some(ctx) = self.contexts.get(dir) {
            return ctx.clone();
        }
        let above = dir.parent().and_then(|p| self.context_at(p));
        let ctx = if self.classifier.below_floor(dir) {
            above
        } else {
            self.classifier.descend(dir, above, self.probe).0
        };
        self.contexts.insert(dir.to_path_buf(), ctx.clone());
        ctx
    }

    /// Same as [`Classifier::classify_path`].
    pub fn classify_path(&mut self, path: &Path, is_dir: bool) -> Classification {
        // Windows paths are cut from their text so they work on any host;
        // they go through the uncached path, which handles that.
        if self.classifier.windows.is_some() {
            return self.classifier.classify_path(path, is_dir, self.probe);
        }
        if is_dir {
            match self.context_at(path) {
                Some(ctx) => Resolved::Dir(ctx).to_classification(),
                None => unmatched(),
            }
        } else {
            let parent_ctx = path.parent().and_then(|p| self.context_at(p));
            self.classifier
                .resolve_file(path, parent_ctx.as_ref())
                .to_classification()
        }
    }
}

pub fn unmatched() -> Classification {
    Classification {
        category: Category::Unknown,
        label: "Unclassified".to_string(),
        explanation: "No DiskPilot rule matched this item. That does not make it safe to remove; inspect it before taking any action.".to_string(),
        rule_id: "unmatched".to_string(),
        confidence: Confidence::Low,
        matched_path: None,
    }
}

#[derive(Debug, Clone)]
pub struct Classifier {
    home: PathBuf,
    /// When set, ancestors above this folder are ignored when computing
    /// inherited context (classify relative to a scan root only).
    floor: Option<PathBuf>,
    /// Set when classifying Windows paths: the rule table is the Windows one,
    /// names compare ignoring case, and these are the well-known folders.
    windows: Option<Arc<WindowsRoots>>,
}

impl Classifier {
    /// A classifier for this computer: the macOS rules on macOS, the Windows
    /// rules (with roots from the environment) on Windows.
    pub fn new(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        #[cfg(windows)]
        {
            let roots = WindowsRoots::from_env(&home);
            Self::windows(roots)
        }
        #[cfg(not(windows))]
        {
            Self::macos(home)
        }
    }

    /// A classifier using the macOS rule table, on any platform.
    pub fn macos(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            floor: None,
            windows: None,
        }
    }

    /// A classifier using the Windows rule table, on any platform. Paths may
    /// use either separator, so it is testable on macOS with Windows paths.
    pub fn windows(roots: WindowsRoots) -> Self {
        Self {
            home: roots.home().to_path_buf(),
            floor: None,
            windows: Some(Arc::new(roots)),
        }
    }

    /// Whether this classifier uses the Windows rules.
    pub fn is_windows(&self) -> bool {
        self.windows.is_some()
    }

    /// Whether `path` is `home/<child>` or inside it, comparing the way the
    /// platform does (ignoring case on Windows).
    pub fn is_in_home_folder(&self, path: &Path, child: &str) -> bool {
        match &self.windows {
            None => path.starts_with(self.home.join(child)),
            Some(roots) => {
                let lossy = path.to_string_lossy();
                let p = WinPath::parse(&lossy);
                let home = roots.parts(Root::Home);
                p.parts.len() > home.len()
                    && p.starts_with_parts(home)
                    && eq_ignore_case(p.parts[home.len()], child)
            }
        }
    }

    pub fn with_floor(mut self, floor: impl Into<PathBuf>) -> Self {
        self.floor = Some(floor.into());
        self
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Find the first directory rule matching `path` itself (ancestors are not
    /// considered; see [`Classifier::context_at`]).
    pub fn match_dir(&self, path: &Path, probe: &dyn Probe) -> Option<&'static DirRule> {
        if let Some(roots) = &self.windows {
            let lossy = path.to_string_lossy();
            let p = WinPath::parse(&lossy);
            return WINDOWS_DIR_RULES
                .iter()
                .chain(shared_project_rules())
                .find(|rule| self.matches_windows(roots, &rule.matcher, path, &p, probe));
        }
        DIR_RULES
            .iter()
            .find(|rule| self.matches(&rule.matcher, path, probe))
    }

    fn matches_windows(
        &self,
        roots: &WindowsRoots,
        matcher: &Matcher,
        path: &Path,
        p: &WinPath,
        probe: &dyn Probe,
    ) -> bool {
        let home = roots.parts(Root::Home);
        let name_in = |names: &[&str]| {
            p.file_name()
                .is_some_and(|n| names.iter().any(|x| eq_ignore_case(n, x)))
        };
        let parent = || p.parent_path(path);
        match matcher {
            Matcher::HomeRelative(parts) => p.is_root_plus(home, parts),
            Matcher::HomeRelativeAny(options) => {
                options.iter().any(|parts| p.is_root_plus(home, parts))
            }
            Matcher::Absolute(abs) => {
                let lossy = Cow::Borrowed(*abs);
                let want = WinPath::parse(&lossy);
                p.is_root_plus(&want.parts, &[])
            }
            Matcher::Name(names) => name_in(names),
            Matcher::NameWithSibling { names, siblings } => {
                name_in(names)
                    && parent().is_some_and(|dir| siblings.iter().any(|s| probe.has_child(dir, s)))
            }
            Matcher::NameWithSiblingExt { names, extensions } => {
                name_in(names)
                    && parent().is_some_and(|dir| probe.has_child_with_extension(dir, extensions))
            }
            Matcher::Contains(children) => {
                !p.is_root_plus(home, &[]) && children.iter().any(|c| probe.has_child(path, c))
            }
            Matcher::NameUnder {
                names,
                parent: parent_name,
                contains,
            } => {
                name_in(names)
                    && p.parts.len() >= 2
                    && eq_ignore_case(p.parts[p.parts.len() - 2], parent_name)
                    && probe.has_child(path, contains)
            }
            Matcher::Under(options) => options.iter().any(|(root, parts)| match root {
                Root::AnyDrive => p.is_drive_plus(parts),
                root => p.is_root_plus(roots.parts(*root), parts),
            }),
            Matcher::Tail(options) => options.iter().any(|parts| p.ends_with(parts)),
        }
    }

    fn matches(&self, matcher: &Matcher, path: &Path, probe: &dyn Probe) -> bool {
        match matcher {
            Matcher::HomeRelative(parts) => match path.strip_prefix(&self.home) {
                Ok(rel) => components_equal(rel, parts),
                Err(_) => false,
            },
            Matcher::HomeRelativeAny(options) => match path.strip_prefix(&self.home) {
                Ok(rel) => options.iter().any(|parts| components_equal(rel, parts)),
                Err(_) => false,
            },
            Matcher::Absolute(abs) => path == Path::new(abs),
            Matcher::Name(names) => file_name_in(path, names),
            Matcher::NameWithSibling { names, siblings } => {
                file_name_in(path, names)
                    && path
                        .parent()
                        .is_some_and(|parent| siblings.iter().any(|s| probe.has_child(parent, s)))
            }
            Matcher::Contains(children) => {
                path != self.home && children.iter().any(|c| probe.has_child(path, c))
            }
            Matcher::NameUnder {
                names,
                parent,
                contains,
            } => {
                file_name_in(path, names)
                    && path
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|n| n == *parent)
                    && probe.has_child(path, contains)
            }
            // Only the Windows table uses these; they behave the same here.
            Matcher::Under(options) => options.iter().any(|(root, parts)| {
                *root == Root::Home
                    && path
                        .strip_prefix(&self.home)
                        .is_ok_and(|rel| components_equal(rel, parts))
            }),
            Matcher::Tail(options) => options.iter().any(|parts| {
                let names: Vec<&OsStr> = path.iter().collect();
                names.len() >= parts.len()
                    && names[names.len() - parts.len()..]
                        .iter()
                        .zip(parts.iter())
                        .all(|(n, p)| *n == OsStr::new(p))
            }),
            Matcher::NameWithSiblingExt { names, extensions } => {
                file_name_in(path, names)
                    && path
                        .parent()
                        .is_some_and(|parent| probe.has_child_with_extension(parent, extensions))
            }
        }
    }

    /// Given the context inherited from the parent, compute the context for
    /// the directory `path`. Returns the new context and whether a rule matched
    /// at `path` itself.
    pub fn descend(
        &self,
        path: &Path,
        inherited: Option<Context>,
        probe: &dyn Probe,
    ) -> (Option<Context>, bool) {
        if let Some(ctx) = &inherited {
            if ctx.rule.kind == RuleKind::Leaf {
                return (inherited, false);
            }
        }
        match self.match_dir(path, probe) {
            Some(rule) => (
                Some(Context {
                    rule,
                    path: Arc::from(path),
                }),
                true,
            ),
            None => (inherited, false),
        }
    }

    /// The context in effect at directory `dir`, evaluating every ancestor from
    /// the filesystem root down (inclusive of `dir`).
    pub fn context_at(&self, dir: &Path, probe: &dyn Probe) -> Option<Context> {
        if self.windows.is_some() {
            return self.context_at_windows(dir, probe);
        }
        let mut chain: Vec<&Path> = dir.ancestors().collect();
        chain.reverse();
        let mut ctx = None;
        for ancestor in chain {
            if self.below_floor(ancestor) {
                continue;
            }
            ctx = self.descend(ancestor, ctx, probe).0;
        }
        ctx
    }

    /// [`Classifier::context_at`] for Windows paths: ancestors are cut from
    /// the path text, so both separators and `\\?\` paths work on any
    /// platform, and the floor compares ignoring case.
    fn context_at_windows(&self, dir: &Path, probe: &dyn Probe) -> Option<Context> {
        let lossy = dir.to_string_lossy();
        let floor = self.floor.as_ref().map(|f| f.to_string_lossy());
        let floor = floor.as_deref().map(WinPath::parse);
        let mut ctx = None;
        for ancestor in windows_paths::ancestors(&lossy) {
            if floor
                .as_ref()
                .is_some_and(|f| !WinPath::parse(ancestor).starts_with_parts(&f.parts))
            {
                continue;
            }
            ctx = self.descend(Path::new(ancestor), ctx, probe).0;
        }
        ctx
    }

    /// Whether `dir` is above the floor, so its rules are ignored.
    fn below_floor(&self, dir: &Path) -> bool {
        self.floor.as_ref().is_some_and(|f| !dir.starts_with(f))
    }

    /// Classify a file given the context of its parent directory.
    pub fn resolve_file(&self, path: &Path, ctx: Option<&Context>) -> Resolved {
        match ctx {
            Some(ctx) if ctx.rule.kind == RuleKind::Leaf || !ctx.rule.file_rules_apply => {
                Resolved::Dir(ctx.clone())
            }
            Some(ctx) => match self.match_file(path) {
                Some(rule) => Resolved::File(rule),
                None => Resolved::Dir(ctx.clone()),
            },
            None => match self.match_file(path) {
                Some(rule) => Resolved::File(rule),
                None => Resolved::Unmatched,
            },
        }
    }

    /// Classify any path from scratch (used outside the scan loop).
    pub fn classify_path(&self, path: &Path, is_dir: bool, probe: &dyn Probe) -> Classification {
        if is_dir {
            match self.context_at(path, probe) {
                Some(ctx) => Resolved::Dir(ctx).to_classification(),
                None => unmatched(),
            }
        } else {
            let lossy = path.to_string_lossy();
            let parent = if self.windows.is_some() {
                WinPath::parse(&lossy).parent_path(path)
            } else {
                path.parent()
            };
            let parent_ctx = parent.and_then(|p| self.context_at(p, probe));
            self.resolve_file(path, parent_ctx.as_ref())
                .to_classification()
        }
    }
}

impl Classifier {
    /// Match a file against the platform's file rules: on Windows, system
    /// files by name first, then extensions.
    pub fn match_file(&self, path: &Path) -> Option<&'static FileRule> {
        if self.windows.is_none() {
            return match_file(path);
        }
        let lossy = path.to_string_lossy();
        let name = WinPath::parse(&lossy).file_name()?;
        if let Some(named) = WINDOWS_NAMED_FILES
            .iter()
            .find(|r| r.names.iter().any(|n| eq_ignore_case(n, name)))
        {
            return Some(&named.rule);
        }
        let (_, ext) = name.rsplit_once('.')?;
        WINDOWS_FILE_RULES
            .iter()
            .find(|rule| rule.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
    }
}

/// The project-level rules at the end of the macOS table (`node_modules`,
/// `target`, `.venv`, `.git` and the rest). They match by folder name, so
/// Windows uses them too, after its own table.
pub fn shared_project_rules() -> &'static [DirRule] {
    static START: OnceLock<usize> = OnceLock::new();
    let start = *START.get_or_init(|| {
        DIR_RULES
            .iter()
            .position(|r| r.id == "node.node-modules")
            .expect("the macOS table has a project section")
    });
    &DIR_RULES[start..]
}

/// Match a file against the extension rules (case-insensitive).
pub fn match_file(path: &Path) -> Option<&'static FileRule> {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)?
        .to_ascii_lowercase();
    FILE_RULES
        .iter()
        .find(|rule| rule.extensions.contains(&ext.as_str()))
}

fn file_name_in(path: &Path, names: &[&str]) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|n| names.contains(&n))
}

/// Compares path components; a `*` part matches any single component
/// (for example a browser profile folder).
fn components_equal(rel: &Path, parts: &[&str]) -> bool {
    let mut comps = rel.components();
    for part in parts {
        match comps.next() {
            Some(Component::Normal(c)) if *part == "*" || c == OsStr::new(part) => {}
            _ => return false,
        }
    }
    comps.next().is_none()
}

#[cfg(test)]
mod tests {
    #[test]
    fn cached_classifier_matches_classify_path() {
        let home = std::env::temp_dir();
        let probe = FsProbe;
        let paths = [
            home.join("Library/Caches/com.example/a/b.bin"),
            home.join("Library/Caches/com.example/c.bin"),
            home.join("Library/Containers/com.x/Data/Library/Caches/d"),
            home.join("project/node_modules/x/index.js"),
            home.join("Documents/report.pdf"),
            PathBuf::from("/"),
            PathBuf::from("relative/path.txt"),
        ];
        for classifier in [
            Classifier::new(&home),
            Classifier::new(&home).with_floor(home.join("Library")),
        ] {
            let mut cached = CachedClassifier::new(&classifier, &probe);
            for _ in 0..2 {
                for p in &paths {
                    for is_dir in [false, true] {
                        assert_eq!(
                            format!("{:?}", cached.classify_path(p, is_dir)),
                            format!("{:?}", classifier.classify_path(p, is_dir, &probe)),
                            "{} dir={is_dir}",
                            p.display()
                        );
                    }
                }
            }
        }
    }

    use super::*;
    use std::collections::HashSet;

    /// In-memory probe: a set of existing paths.
    struct FakeProbe(HashSet<PathBuf>);

    impl FakeProbe {
        fn new(paths: &[&str]) -> Self {
            Self(paths.iter().map(PathBuf::from).collect())
        }
        fn empty() -> Self {
            Self(HashSet::new())
        }
    }

    impl Probe for FakeProbe {
        fn has_child(&self, dir: &Path, name: &str) -> bool {
            self.0.contains(&dir.join(name))
        }
    }

    fn classifier() -> Classifier {
        Classifier::macos("/Users/test")
    }

    fn dir(path: &str, probe: &FakeProbe) -> Classification {
        classifier().classify_path(Path::new(path), true, probe)
    }

    fn file(path: &str, probe: &FakeProbe) -> Classification {
        classifier().classify_path(Path::new(path), false, probe)
    }

    #[test]
    fn every_rule_has_an_explanation_and_unique_id() {
        let mut ids = HashSet::new();
        for rule in DIR_RULES {
            assert!(
                !rule.explanation.trim().is_empty(),
                "{} lacks explanation",
                rule.id
            );
            assert!(!rule.label.trim().is_empty(), "{} lacks label", rule.id);
            assert!(ids.insert(rule.id), "duplicate rule id {}", rule.id);
        }
        for rule in FILE_RULES {
            assert!(
                !rule.explanation.trim().is_empty(),
                "{} lacks explanation",
                rule.id
            );
            assert!(ids.insert(rule.id), "duplicate rule id {}", rule.id);
        }
        assert!(!unmatched().explanation.is_empty());
    }

    #[test]
    fn wildcard_parts_match_one_folder_of_any_name() {
        let p = FakeProbe::empty();
        let chrome = "/Users/test/Library/Application Support/Google/Chrome";
        for profile in ["Default", "Profile 9"] {
            assert_eq!(
                dir(&format!("{chrome}/{profile}/Code Cache"), &p).rule_id,
                "browser.chromium-caches"
            );
            // Offline site data is its own, review-first rule.
            assert_eq!(
                dir(
                    &format!("{chrome}/{profile}/Service Worker/CacheStorage"),
                    &p
                )
                .rule_id,
                "web.offline-site-data"
            );
        }
        // `*` stands for exactly one folder, never zero or two.
        assert_ne!(
            dir(&format!("{chrome}/Code Cache"), &p).rule_id,
            "browser.chromium-caches"
        );
        assert_ne!(
            dir(&format!("{chrome}/a/b/Code Cache"), &p).rule_id,
            "browser.chromium-caches"
        );
        // Browser profile data outside the cache folders stays app data.
        assert_ne!(
            dir(&format!("{chrome}/Default"), &p).category,
            Category::Cache
        );
    }

    #[test]
    fn app_specific_cache_rules_win_over_the_generic_app_rule() {
        let p = FakeProbe::empty();
        let support = "/Users/test/Library/Application Support";
        assert_eq!(
            dir(&format!("{support}/Code/Code Cache"), &p).rule_id,
            "vscode.caches"
        );
        assert_eq!(
            dir(&format!("{support}/Slack/Code Cache"), &p).rule_id,
            "apps.chromium-caches"
        );
        assert_eq!(
            dir(&format!("{support}/discord/Cache/Cache_Data"), &p).rule_id,
            "apps.chromium-caches"
        );
        let chromium = FakeProbe::new(&[
            "/Users/test/.someapp/browser/Default/Cache/Cache_Data/index",
            "/Users/test/Documents/research/Cache_Data/index",
        ]);
        assert_eq!(
            dir(
                "/Users/test/.someapp/browser/Default/Cache/Cache_Data",
                &chromium
            )
            .rule_id,
            "chromium.http-cache"
        );
        // Without Chromium's index, or outside a Cache folder, it is someone's
        // own folder that happens to share the name.
        assert_ne!(
            dir("/Users/test/.someapp/browser/Default/Cache/Cache_Data", &p).rule_id,
            "chromium.http-cache"
        );
        assert_ne!(
            dir("/Users/test/Documents/research/Cache_Data", &chromium).rule_id,
            "chromium.http-cache"
        );
    }

    #[test]
    fn chat_media_and_new_tool_locations_are_recognised() {
        let p = FakeProbe::empty();
        let groups = "/Users/test/Library/Group Containers";
        let whatsapp = dir(
            &format!("{groups}/group.net.whatsapp.WhatsApp.shared/Message/Media"),
            &p,
        );
        assert_eq!(whatsapp.rule_id, "chat.whatsapp-media");
        assert_eq!(whatsapp.category, Category::UserData);
        assert_eq!(
            dir(
                &format!(
                    "{groups}/6N38VWS5BX.ru.keepcoder.Telegram/appstore/account-123/postbox/media"
                ),
                &p
            )
            .rule_id,
            "chat.telegram-media"
        );
        assert_eq!(
            dir("/Users/test/Library/Caches/ms-playwright", &p).rule_id,
            "playwright.browsers"
        );
        assert_eq!(dir("/Users/test/go/pkg/mod", &p).rule_id, "go.mod-cache");
        assert_eq!(
            dir("/Users/test/.asdf/installs", &p).rule_id,
            "asdf.installs"
        );
        assert_eq!(
            dir("/Users/test/.local/share/mise/installs", &p).rule_id,
            "asdf.installs"
        );
    }

    #[test]
    fn home_relative_rules_match_exact_paths() {
        let p = FakeProbe::empty();
        assert_eq!(
            dir("/Users/test/Library/Caches", &p).rule_id,
            "library.caches"
        );
        assert_eq!(dir("/Users/test/.npm", &p).rule_id, "npm.cache");
        assert_eq!(
            dir("/Users/test/Library/Developer/Xcode/DerivedData", &p).category,
            Category::BuildArtifacts
        );
    }

    #[test]
    fn rules_do_not_match_other_users_or_partial_names() {
        let p = FakeProbe::empty();
        assert_eq!(dir("/Users/other/.npm", &p).category, Category::Unknown);
        assert_eq!(
            dir("/Users/test/.npmrc-dir", &p).category,
            Category::Unknown
        );
        assert_eq!(
            dir("/Users/test/LibraryX/Caches", &p).category,
            Category::Unknown
        );
    }

    #[test]
    fn descendants_inherit_classification_with_matched_path() {
        let p = FakeProbe::empty();
        let c = dir("/Users/test/Library/Caches/com.example.app/data", &p);
        assert_eq!(c.rule_id, "library.caches");
        assert_eq!(
            c.matched_path.as_deref(),
            Some("/Users/test/Library/Caches")
        );
    }

    #[test]
    fn specific_rules_win_inside_containers() {
        let p = FakeProbe::empty();
        assert_eq!(
            dir("/Users/test/Library/Caches/Yarn", &p).rule_id,
            "yarn.cache"
        );
        assert_eq!(
            dir("/Users/test/Library/Containers/com.docker.docker/Data", &p).category,
            Category::DockerStorage
        );
        assert_eq!(
            dir("/Users/test/Documents/app/node_modules", &p).rule_id,
            "node.node-modules"
        );
    }

    #[test]
    fn go_caches_are_recognized_and_gopath_sources_stay_user_data() {
        let p = FakeProbe::new(&["/Users/test/go/src/github.com/me/app/.git"]);
        let module = dir("/Users/test/go/pkg/mod/golang.org/x/text@v0.1.0", &p);
        assert_eq!(module.rule_id, "go.mod-cache");
        assert_eq!(module.category, Category::Cache);
        assert_eq!(
            module.matched_path.as_deref(),
            Some("/Users/test/go/pkg/mod")
        );
        assert_eq!(
            dir("/Users/test/Library/Caches/go-build", &p).rule_id,
            "go.build-cache"
        );
        assert_eq!(dir("/Users/test/go/bin", &p).rule_id, "go.home");
        assert_eq!(
            dir("/Users/test/go/src/github.com/me/app", &p).rule_id,
            "project.code"
        );
    }

    #[test]
    fn leaf_rules_stop_nested_matching() {
        // A `dist` folder inside node_modules (next to a package's package.json)
        // belongs to node_modules, not to "project build output".
        let p = FakeProbe::new(&["/Users/test/code/app/node_modules/react/package.json"]);
        let c = dir("/Users/test/code/app/node_modules/react/dist", &p);
        assert_eq!(c.rule_id, "node.node-modules");
        assert_eq!(
            c.matched_path.as_deref(),
            Some("/Users/test/code/app/node_modules")
        );
    }

    #[test]
    fn sibling_rules_require_the_sibling() {
        let none = FakeProbe::empty();
        assert_eq!(
            dir("/Users/test/code/app/target", &none).category,
            Category::Unknown
        );
        let cargo = FakeProbe::new(&["/Users/test/code/app/Cargo.toml"]);
        assert_eq!(
            dir("/Users/test/code/app/target", &cargo).rule_id,
            "rust.target"
        );
        let pkg = FakeProbe::new(&["/Users/test/code/web/package.json"]);
        assert_eq!(
            dir("/Users/test/code/web/.next", &pkg).rule_id,
            "node.next-build"
        );
        assert_eq!(
            dir("/Users/test/code/web/dist", &pkg).confidence,
            Confidence::Medium
        );
    }

    #[test]
    fn git_projects_are_user_data_but_not_the_home_folder() {
        let p = FakeProbe::new(&["/Users/test/code/app/.git", "/Users/test/.git"]);
        assert_eq!(dir("/Users/test/code/app", &p).rule_id, "project.code");
        assert_eq!(dir("/Users/test", &p).category, Category::Unknown);
        assert_eq!(
            dir("/Users/test/code/app/.git", &p).rule_id,
            "git.repository-data"
        );
    }

    #[test]
    fn file_rules_apply_only_where_allowed() {
        let p = FakeProbe::empty();
        // No context: extension decides.
        assert_eq!(
            file("/Users/test/big.dmg", &p).category,
            Category::BinaryData
        );
        // Documents allows refinement.
        assert_eq!(
            file("/Users/test/Documents/model.SAFETENSORS", &p).rule_id,
            "file.ml-model"
        );
        // Downloads does not: everything in Downloads is a download.
        assert_eq!(
            file("/Users/test/Downloads/x.dmg", &p).category,
            Category::Downloads
        );
        // Leaf context always wins.
        assert_eq!(
            file("/Users/test/.npm/_cacache/a.tgz", &p).rule_id,
            "npm.cache"
        );
        // Unknown extension, no context.
        assert_eq!(
            file("/Users/test/notes.xyz", &p).category,
            Category::Unknown
        );
    }

    #[test]
    fn large_files_are_never_classified_by_size() {
        // Classification depends only on path rules; there is no size input.
        let p = FakeProbe::empty();
        let c = file("/Users/test/Documents/huge-unknown-file", &p);
        assert_eq!(c.category, Category::UserData);
        assert_eq!(c.rule_id, "home.documents");
    }

    #[test]
    fn system_paths_are_system_files() {
        let p = FakeProbe::empty();
        assert_eq!(dir("/System/Library", &p).category, Category::SystemFiles);
        assert_eq!(dir("/private/var/folders/ab", &p).category, Category::Cache);
        assert_eq!(dir("/private/etc", &p).category, Category::SystemFiles);
    }

    #[test]
    fn tool_homes_are_containers_for_specific_rules() {
        let p = FakeProbe::empty();
        assert_eq!(dir("/Users/test/.cargo/bin", &p).rule_id, "cargo.home");
        assert_eq!(
            dir("/Users/test/.cargo/registry/src", &p).rule_id,
            "cargo.registry"
        );
        assert_eq!(
            dir("/Users/test/.claude/projects", &p).rule_id,
            "ai.claude-code"
        );
        assert_eq!(
            dir("/Users/test/.local/share/claude/versions", &p).rule_id,
            "ai.claude-code-versions"
        );
    }

    #[test]
    fn editor_caches_are_separated_from_chat_history() {
        let p = FakeProbe::empty();
        let base = "/Users/test/Library/Application Support/Cursor";
        assert_eq!(
            dir(&format!("{base}/CachedData/x"), &p).rule_id,
            "cursor.caches"
        );
        assert_eq!(
            dir(&format!("{base}/Code Cache"), &p).category,
            Category::Cache
        );
        assert_eq!(
            dir(&format!("{base}/User/workspaceStorage/abc"), &p).rule_id,
            "cursor.workspace-storage"
        );
        assert_eq!(dir(&format!("{base}/User"), &p).rule_id, "cursor.app-data");
    }

    #[test]
    fn game_engine_flutter_and_container_tools_are_recognised() {
        let p = FakeProbe::new(&[
            "/Users/test/code/app/pubspec.yaml",
            "/Users/test/games/Shooter/ProjectSettings",
            "/Users/test/games/Arena/Config",
        ]);
        assert_eq!(
            dir("/Users/test/code/app/build", &p).rule_id,
            "flutter.build"
        );
        assert_eq!(
            dir("/Users/test/code/app/.dart_tool", &p).rule_id,
            "flutter.dart-tool"
        );
        assert_eq!(
            dir("/Users/test/games/Shooter/Library", &p).rule_id,
            "unity.library"
        );
        assert_eq!(
            dir("/Users/test/games/Arena/DerivedDataCache", &p).rule_id,
            "unreal.project-ddc"
        );
        assert_eq!(
            dir("/Users/test/.pub-cache/hosted/pub.dev", &p).rule_id,
            "flutter.pub-cache-hosted"
        );
        assert_eq!(
            dir("/Users/test/.pub-cache/bin", &p).rule_id,
            "flutter.pub-cache"
        );
        assert_eq!(
            dir(
                "/Users/test/Library/Application Support/Epic/UnrealEngine/Common/DerivedDataCache",
                &p
            )
            .rule_id,
            "unreal.shared-ddc"
        );
        assert_eq!(
            dir("/Users/test/.local/share/containers/podman/machine", &p).rule_id,
            "podman.storage"
        );
        assert_eq!(
            dir("/Users/test/.minikube/cache/iso", &p).rule_id,
            "minikube.cache"
        );
        assert_eq!(
            dir("/Users/test/.minikube/machines", &p).rule_id,
            "minikube.home"
        );
    }

    #[test]
    fn iphone_backups_are_user_data() {
        let p = FakeProbe::empty();
        let c = dir(
            "/Users/test/Library/Application Support/MobileSync/Backup/00008110-0001",
            &p,
        );
        assert_eq!(c.rule_id, "devices.iphone-backups");
        assert_eq!(c.category, Category::UserData);
    }

    #[test]
    fn unity_and_unreal_rules_need_their_project_markers() {
        let none = FakeProbe::empty();
        // A plain folder named Library or DerivedDataCache is not a game project.
        assert_ne!(
            dir("/Users/test/games/Shooter/Library", &none).rule_id,
            "unity.library"
        );
        assert_ne!(
            dir("/Users/test/games/Arena/DerivedDataCache", &none).rule_id,
            "unreal.project-ddc"
        );
        // The home Library is never a Unity cache.
        let home_marker = FakeProbe::new(&["/Users/test/ProjectSettings"]);
        assert_ne!(
            dir("/Users/test/Library", &home_marker).rule_id,
            "unity.library"
        );
    }

    #[test]
    fn windsurf_caches_are_separated_from_chat_history() {
        let p = FakeProbe::empty();
        let base = "/Users/test/Library/Application Support/Windsurf";
        assert_eq!(
            dir(&format!("{base}/CachedData/x"), &p).rule_id,
            "windsurf.caches"
        );
        assert_eq!(dir(&format!("{base}/logs"), &p).rule_id, "windsurf.logs");
        assert_eq!(
            dir(&format!("{base}/User/workspaceStorage/abc"), &p).rule_id,
            "windsurf.workspace-storage"
        );
        assert_eq!(
            dir(&format!("{base}/User"), &p).rule_id,
            "windsurf.app-data"
        );
    }

    #[test]
    fn floor_ignores_ancestor_context() {
        let p = FakeProbe::empty();
        let floored = Classifier::macos("/Users/test").with_floor("/Users/test/Library/Caches/x");
        let c = floored.classify_path(Path::new("/Users/test/Library/Caches/x/y"), true, &p);
        assert_eq!(c.category, Category::Unknown);
    }
}

/// The Windows rule table, tested with Windows paths on every platform.
#[cfg(test)]
mod windows_tests {
    use super::*;
    use std::collections::HashSet;

    const HOME: &str = r"C:\Users\Test";

    /// In-memory probe for Windows paths: entries compare ignoring case and
    /// separators.
    struct WinProbe(HashSet<String>);

    fn key(text: &str) -> String {
        WinPath::parse(text).parts.join("\\").to_lowercase()
    }

    impl WinProbe {
        fn new(paths: &[&str]) -> Self {
            Self(paths.iter().map(|p| key(p)).collect())
        }
        fn empty() -> Self {
            Self::new(&[])
        }
    }

    impl Probe for WinProbe {
        fn has_child(&self, dir: &Path, name: &str) -> bool {
            self.0
                .contains(&key(&format!("{}\\{name}", dir.to_string_lossy())))
        }
        fn has_child_with_extension(&self, dir: &Path, extensions: &[&str]) -> bool {
            let prefix = format!("{}\\", key(&dir.to_string_lossy()));
            self.0.iter().any(|p| {
                p.strip_prefix(&prefix).is_some_and(|name| {
                    !name.contains('\\')
                        && name
                            .rsplit_once('.')
                            .is_some_and(|(_, e)| extensions.contains(&e))
                })
            })
        }
    }

    fn classifier() -> Classifier {
        Classifier::windows(WindowsRoots::for_home(HOME))
    }

    fn dir(path: &str, probe: &WinProbe) -> Classification {
        classifier().classify_path(Path::new(path), true, probe)
    }

    fn file(path: &str) -> Classification {
        classifier().classify_path(Path::new(path), false, &WinProbe::empty())
    }

    fn rule(path: &str) -> String {
        dir(path, &WinProbe::empty()).rule_id
    }

    #[test]
    fn every_windows_rule_has_an_explanation_and_unique_id() {
        let mut ids = HashSet::new();
        for rule in WINDOWS_DIR_RULES {
            assert!(!rule.explanation.trim().is_empty(), "{}", rule.id);
            assert!(!rule.label.trim().is_empty(), "{}", rule.id);
            assert!(ids.insert(rule.id), "duplicate rule id {}", rule.id);
        }
        // Shared project rules follow the table and must not repeat an id.
        for rule in shared_project_rules() {
            assert!(ids.insert(rule.id), "{} is in both tables", rule.id);
            assert!(
                matches!(
                    rule.matcher,
                    Matcher::Name(_)
                        | Matcher::NameWithSibling { .. }
                        | Matcher::Contains(_)
                        | Matcher::NameUnder { .. }
                ),
                "{} is not a project rule",
                rule.id
            );
        }
        for rule in WINDOWS_FILE_RULES
            .iter()
            .chain(WINDOWS_NAMED_FILES.iter().map(|n| &n.rule))
        {
            assert!(!rule.explanation.trim().is_empty(), "{}", rule.id);
        }
    }

    #[test]
    fn rule_ids_shared_with_macos_mean_the_same_thing() {
        for rule in WINDOWS_DIR_RULES {
            if let Some(mac) = DIR_RULES.iter().find(|r| r.id == rule.id) {
                assert_eq!(mac.category, rule.category, "{}", rule.id);
            }
        }
    }

    #[test]
    fn temporary_files_and_system_areas() {
        assert_eq!(
            rule(r"C:\Users\Test\AppData\Local\Temp"),
            "windows.user-temp"
        );
        assert_eq!(
            rule(r"C:\Users\Test\AppData\Local\Temp\setup-123"),
            "windows.user-temp"
        );
        assert_eq!(rule(r"C:\Windows\Temp"), "windows.temp");
        assert_eq!(rule(r"C:\Windows.old"), "windows.old");
        assert_eq!(rule(r"C:\Windows.old\Users\Test"), "windows.old");
        assert_eq!(rule(r"C:\$WINDOWS.~BT"), "windows.upgrade-files");
        assert_eq!(rule(r"C:\$Windows.~WS\Sources"), "windows.upgrade-files");
        assert_eq!(rule(r"C:\$GetCurrent"), "windows.upgrade-files");
        assert_eq!(
            rule(r"C:\Windows\SoftwareDistribution\Download"),
            "windows.update-cache"
        );
        assert_eq!(
            rule(
                r"C:\Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"
            ),
            "windows.delivery-optimization"
        );
        assert_eq!(rule(r"C:\Windows\WinSxS"), "windows.component-store");
        assert_eq!(rule(r"C:\Windows\Installer"), "windows.installer-cache");
        assert_eq!(rule(r"C:\Windows\Minidump"), "windows.memory-dumps");
        assert_eq!(rule(r"C:\Windows\System32"), "windows.system");
        assert_eq!(
            dir(r"C:\Windows\System32", &WinProbe::empty()).category,
            Category::SystemFiles
        );
        assert_eq!(
            rule(r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive"),
            "windows.error-reports-system"
        );
        assert_eq!(
            rule(r"C:\ProgramData\Package Cache\{abc}"),
            "visualstudio.package-cache"
        );
        assert_eq!(rule(r"D:\$Recycle.Bin\S-1-5-21"), "windows.recycle-bin");
        assert_eq!(
            rule(r"E:\System Volume Information"),
            "windows.system-volume-information"
        );
        assert_eq!(rule(r"C:\NVIDIA\DisplayDriver"), "gpu.driver-installers");
        // A drive other than the system drive has no Windows folder.
        assert_ne!(rule(r"D:\Windows.old"), "windows.old");
        assert_eq!(rule(r"C:\Program Files\App"), "windows.program-files");
    }

    #[test]
    fn matching_ignores_case_separators_and_verbatim_prefixes() {
        for p in [
            r"c:\users\test\appdata\local\TEMP",
            "C:/Users/Test/AppData/Local/Temp",
            r"\\?\C:\Users\Test\AppData\Local\Temp",
            r"C:\USERS\TEST\APPDATA\LOCAL\TEMP\",
        ] {
            assert_eq!(rule(p), "windows.user-temp", "{p}");
        }
        assert_eq!(rule(r"c:\windows\winsxs"), "windows.component-store");
        // Other users' profiles are not this user's caches.
        assert_ne!(rule(r"C:\Users\Other\AppData\Local\npm-cache"), "npm.cache");
        assert_ne!(
            rule(r"C:\Users\Test\AppData\Local\Tempest"),
            "windows.user-temp"
        );
    }

    #[test]
    fn roots_come_from_the_environment_when_they_differ() {
        let roots = WindowsRoots::new(
            HOME,
            r"D:\Profiles\Test\Local",
            r"D:\Profiles\Test\Roaming",
            r"C:\ProgramData",
            r"C:\WINNT",
            r"T:\tmp",
            "C:",
        );
        let c = Classifier::windows(roots);
        let p = WinProbe::empty();
        let id = |path: &str| c.classify_path(Path::new(path), true, &p).rule_id;
        assert_eq!(id(r"T:\tmp"), "windows.user-temp");
        assert_eq!(id(r"D:\Profiles\Test\Local\npm-cache"), "npm.cache");
        assert_eq!(id(r"C:\WINNT\WinSxS"), "windows.component-store");
        assert_eq!(id(r"D:\Profiles\Test\Roaming\Code\logs"), "vscode.logs");
    }

    #[test]
    fn browser_caches_never_include_profile_data() {
        let local = r"C:\Users\Test\AppData\Local";
        for (browser, profile) in [
            (r"Google\Chrome\User Data", "Default"),
            (r"Microsoft\Edge\User Data", "Profile 2"),
            (r"BraveSoftware\Brave-Browser\User Data", "Default"),
            (r"Vivaldi\User Data", "Default"),
        ] {
            for cache in ["Cache", "Code Cache", "GPUCache"] {
                assert_eq!(
                    rule(&format!(r"{local}\{browser}\{profile}\{cache}")),
                    "browser.chromium-caches",
                    "{browser} {cache}"
                );
            }
            assert_eq!(
                rule(&format!(
                    r"{local}\{browser}\{profile}\Service Worker\CacheStorage"
                )),
                "web.offline-site-data"
            );
            for private in [
                "",
                r"\History",
                r"\Cookies",
                r"\Network",
                r"\Login Data",
                r"\Local Storage",
                r"\IndexedDB",
            ] {
                let c = dir(
                    &format!(r"{local}\{browser}\{profile}{private}"),
                    &WinProbe::empty(),
                );
                assert_ne!(c.category, Category::Cache, "{browser}{private}");
            }
        }
        assert_eq!(
            rule(r"C:\Users\Test\AppData\Local\Opera Software\Opera Stable\Cache"),
            "browser.chromium-caches"
        );
        assert_eq!(
            rule(
                r"C:\Users\Test\AppData\Local\Mozilla\Firefox\Profiles\abcd.default-release\cache2"
            ),
            "browser.firefox-cache"
        );
        // The roaming Firefox profile holds logins and history.
        let roaming = dir(
            r"C:\Users\Test\AppData\Roaming\Mozilla\Firefox\Profiles\abcd.default-release",
            &WinProbe::empty(),
        );
        assert_ne!(roaming.category, Category::Cache);
    }

    #[test]
    fn chat_and_meeting_apps() {
        let roaming = r"C:\Users\Test\AppData\Roaming";
        let local = r"C:\Users\Test\AppData\Local";
        assert_eq!(
            rule(&format!(r"{roaming}\Microsoft\Teams\Cache")),
            "teams.classic-cache"
        );
        assert_eq!(
            rule(&format!(
                r"{local}\Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams"
            )),
            "teams.cache"
        );
        assert_eq!(
            rule(&format!(r"{roaming}\Slack\Code Cache")),
            "apps.chromium-caches"
        );
        assert_eq!(
            rule(&format!(r"{roaming}\discord\Cache\Cache_Data")),
            "apps.chromium-caches"
        );
        assert_eq!(rule(&format!(r"{roaming}\Zoom\logs")), "zoom.logs");
        assert_eq!(rule(&format!(r"{local}\Spotify\Data")), "spotify.cache");
        // Telegram: the media cache is separate from the login keys.
        assert_eq!(
            rule(&format!(r"{roaming}\Telegram Desktop\tdata\user_data")),
            "chat.telegram-desktop-media"
        );
        assert_eq!(
            rule(&format!(
                r"{roaming}\Telegram Desktop\tdata\user_data#2\cache"
            )),
            "chat.telegram-desktop-media"
        );
        let keys = dir(
            &format!(r"{roaming}\Telegram Desktop\tdata\D877F783D5D3EF8C"),
            &WinProbe::empty(),
        );
        assert_eq!(keys.rule_id, "chat.telegram-desktop");
        assert_eq!(keys.category, Category::UserData);
        let whatsapp = dir(
            &format!(r"{local}\Packages\5319275A.WhatsAppDesktop_cv1g1gvanyjgm\LocalState\shared"),
            &WinProbe::empty(),
        );
        assert_eq!(whatsapp.rule_id, "chat.whatsapp-desktop");
        assert_eq!(whatsapp.category, Category::UserData);
    }

    #[test]
    fn games_and_graphics_caches() {
        assert_eq!(
            rule(r"C:\Program Files (x86)\Steam\steamapps\shadercache\570"),
            "steam.shader-cache"
        );
        assert_eq!(
            rule(r"D:\SteamLibrary\steamapps\shadercache"),
            "steam.shader-cache"
        );
        assert_eq!(
            rule(r"D:\SteamLibrary\steamapps\downloading"),
            "steam.partial-downloads"
        );
        assert_eq!(
            rule(r"C:\Program Files (x86)\Steam\depotcache"),
            "steam.download-cache"
        );
        assert_eq!(
            rule(r"C:\Program Files (x86)\Steam\appcache\httpcache"),
            "steam.download-cache"
        );
        assert_eq!(
            rule(r"C:\Users\Test\AppData\Local\Steam\htmlcache"),
            "steam.web-cache"
        );
        assert_eq!(
            rule(r"C:\Users\Test\AppData\Local\EpicGamesLauncher\Saved\webcache_4430"),
            "epic.webcache"
        );
        // Games themselves are not caches.
        assert_eq!(rule(r"D:\SteamLibrary\steamapps\common\Game"), "unmatched");
        for p in [
            r"C:\Users\Test\AppData\Local\D3DSCache",
            r"C:\Users\Test\AppData\Local\NVIDIA\DXCache",
            r"C:\Users\Test\AppData\Local\NVIDIA\GLCache",
            r"C:\Users\Test\AppData\Local\AMD\DxCache",
        ] {
            assert_eq!(rule(p), "gpu.shader-cache", "{p}");
        }
    }

    #[test]
    fn windows_diagnostics_and_explorer_caches() {
        let local = r"C:\Users\Test\AppData\Local";
        assert_eq!(rule(&format!(r"{local}\CrashDumps")), "windows.crash-dumps");
        assert_eq!(
            rule(&format!(r"{local}\Microsoft\Windows\WER\ReportQueue")),
            "windows.error-reports"
        );
        assert_eq!(
            rule(&format!(r"{local}\Microsoft\Windows\Explorer")),
            "windows.explorer-cache"
        );
        assert_eq!(
            file(&format!(
                r"{local}\Microsoft\Windows\Explorer\thumbcache_256.db"
            ))
            .rule_id,
            "windows.explorer-cache"
        );
        assert_eq!(
            rule(&format!(r"{local}\Microsoft\Windows\INetCache")),
            "windows.inet-cache"
        );
    }

    #[test]
    fn package_caches_and_toolchains() {
        let home = HOME;
        let local = r"C:\Users\Test\AppData\Local";
        for (path, id) in [
            (format!(r"{local}\npm-cache"), "npm.cache"),
            (format!(r"{local}\pnpm\store\v3"), "pnpm.store-home"),
            (r"D:\.pnpm-store".to_string(), "pnpm.store-home"),
            (format!(r"{local}\Yarn\Cache\v6"), "yarn.cache"),
            (format!(r"{home}\.nuget\packages"), "nuget.packages"),
            (format!(r"{local}\NuGet\v3-cache"), "nuget.http-cache"),
            (format!(r"{local}\pip\Cache"), "pip.cache"),
            (format!(r"{local}\uv\cache"), "uv.cache"),
            (format!(r"{local}\pypoetry\Cache\artifacts"), "poetry.cache"),
            (
                format!(r"{local}\pypoetry\Cache\virtualenvs\app-x"),
                "poetry.virtualenvs",
            ),
            (format!(r"{home}\miniconda3\pkgs"), "conda.pkgs"),
            (format!(r"{home}\.cargo\registry"), "cargo.registry"),
            (format!(r"{home}\.cargo\bin"), "cargo.home"),
            (format!(r"{home}\.gradle\caches"), "gradle.caches"),
            (format!(r"{home}\.m2\repository"), "maven.repository"),
            (format!(r"{local}\go-build"), "go.build-cache"),
            (format!(r"{home}\go\pkg\mod"), "go.mod-cache"),
            (format!(r"{local}\Composer"), "composer.cache"),
            (format!(r"{home}\.bundle\cache"), "bundler.cache"),
            (
                format!(r"{local}\Android\Sdk\system-images"),
                "android.system-images",
            ),
            (format!(r"{local}\Android\Sdk\platforms"), "android.sdk"),
            (format!(r"{home}\.android\avd"), "android.avds"),
            (
                format!(r"{local}\Microsoft\VisualStudio\17.0_abc\ComponentModelCache"),
                "visualstudio.component-cache",
            ),
            (
                format!(r"{local}\JetBrains\IntelliJIdea2025.2\caches"),
                "jetbrains.caches",
            ),
            (format!(r"{local}\Unity\cache"), "unity.cache"),
            (format!(r"{local}\Docker\wsl\disk"), "docker.desktop-vm"),
            (
                r"C:\Users\Test\AppData\Roaming\Code\User\workspaceStorage".to_string(),
                "vscode.workspace-storage",
            ),
            (
                r"C:\Users\Test\AppData\Roaming\Cursor\CachedData".to_string(),
                "cursor.caches",
            ),
            (format!(r"{home}\.claude\projects"), "ai.claude-code"),
        ] {
            assert_eq!(rule(&path), id, "{path}");
        }
    }

    #[test]
    fn project_output_is_recognized_on_windows() {
        let p = WinProbe::new(&[
            r"C:\Users\Test\code\web\package.json",
            r"C:\Users\Test\code\cli\Cargo.toml",
            r"C:\Users\Test\code\py\pyproject.toml",
            r"C:\Users\Test\code\App\App.csproj",
            r"C:\Users\Test\code\App\.git",
        ]);
        assert_eq!(
            dir(r"C:\Users\Test\code\web\node_modules", &p).rule_id,
            "node.node-modules"
        );
        assert_eq!(
            dir(r"C:\Users\Test\code\web\NODE_MODULES", &p).rule_id,
            "node.node-modules"
        );
        assert_eq!(
            dir(r"C:\Users\Test\code\web\.next", &p).rule_id,
            "node.next-build"
        );
        assert_eq!(
            dir(r"C:\Users\Test\code\cli\target", &p).rule_id,
            "rust.target"
        );
        assert_eq!(
            dir(r"C:\Users\Test\code\py\.venv", &p).rule_id,
            "python.venv"
        );
        assert_eq!(
            dir(r"C:\Users\Test\code\App\bin", &p).rule_id,
            "dotnet.build-output"
        );
        assert_eq!(
            dir(r"C:\Users\Test\code\App\obj\Debug", &p).rule_id,
            "dotnet.build-output"
        );
        assert_eq!(dir(r"C:\Users\Test\code\App", &p).rule_id, "project.code");
        // Without a project file next to it, bin is someone's own folder.
        assert_ne!(
            dir(r"C:\Users\Test\code\web\bin", &p).rule_id,
            "dotnet.build-output"
        );
        assert_ne!(
            dir(r"C:\Users\Test\code\other\target", &p).rule_id,
            "rust.target"
        );
        // The profile folder itself is never a code project.
        let home_git = WinProbe::new(&[r"C:\Users\Test\.git"]);
        assert_ne!(dir(HOME, &home_git).rule_id, "project.code");
    }

    #[test]
    fn user_folders_and_files() {
        assert_eq!(rule(r"C:\Users\Test\Downloads"), "home.downloads");
        assert_eq!(rule(r"C:\Users\Test\Videos\Trips"), "home.videos");
        assert_eq!(
            rule(r"C:\Users\Test\OneDrive - Contoso\Docs"),
            "windows.onedrive"
        );
        assert_eq!(
            rule(r"C:\Users\Test\AppData\Local\SomeApp"),
            "windows.local-appdata"
        );
        assert_eq!(
            rule(r"C:\Users\Test\AppData\Roaming\SomeApp"),
            "windows.roaming-appdata"
        );
        assert_eq!(
            rule(r"C:\Users\Test\AppData\LocalLow\Game"),
            "windows.appdata"
        );
        // Everything in Downloads is a download, installers included.
        assert_eq!(
            file(r"C:\Users\Test\Downloads\Setup.EXE").category,
            Category::Downloads
        );
        assert_eq!(
            file(r"C:\Users\Test\Documents\tool.msi").rule_id,
            "file.installer"
        );
        assert_eq!(
            file(r"C:\Users\Test\Documents\Ubuntu.iso").rule_id,
            "file.disk-image"
        );
        assert_eq!(
            file(r"C:\Users\Test\Documents\disk.VHDX").rule_id,
            "file.vm-disk"
        );
        // System files at the drive root are recognized by name.
        assert_eq!(file(r"C:\hiberfil.sys").rule_id, "windows.hibernation-file");
        assert_eq!(file(r"C:\pagefile.sys").rule_id, "windows.page-file");
        assert_eq!(file(r"C:\swapfile.sys").category, Category::SystemFiles);
        // Files in a leaf take the folder's classification.
        assert_eq!(
            file(r"C:\Users\Test\AppData\Local\npm-cache\_cacache\x.exe").rule_id,
            "npm.cache"
        );
    }

    #[test]
    fn floor_ignores_ancestor_context_ignoring_case() {
        let c = classifier().with_floor(r"c:\users\test\appdata\local\temp\x");
        let got = c.classify_path(
            Path::new(r"C:\Users\Test\AppData\Local\Temp\x\y"),
            true,
            &WinProbe::empty(),
        );
        assert_eq!(got.category, Category::Unknown);
    }

    #[test]
    fn downloads_folder_check_ignores_case() {
        let c = classifier();
        assert!(c.is_in_home_folder(Path::new(r"c:\users\test\downloads\a.exe"), "Downloads"));
        assert!(c.is_in_home_folder(Path::new(r"C:\Users\Test\Downloads"), "Downloads"));
        assert!(!c.is_in_home_folder(Path::new(r"C:\Users\Test\Downloads2\a"), "Downloads"));
        assert!(!c.is_in_home_folder(Path::new(r"C:\Users\Other\Downloads\a.exe"), "Downloads"));
    }
}
