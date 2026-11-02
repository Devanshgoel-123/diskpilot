//! Classification engine.
//!
//! Pure logic: given a path (and a way to check whether sibling/child entries
//! exist), decide which category it belongs to and why. The classifier has no
//! side effects and never executes any operation.

pub mod rules;

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::model::{Category, Classification, Confidence};
use rules::{DirRule, FileRule, Matcher, RuleKind, DIR_RULES, FILE_RULES};

/// Answers "does `dir` directly contain an entry called `name`?".
/// Abstracted so the scanner can answer from entries it already read and tests
/// can run without touching the filesystem.
pub trait Probe {
    fn has_child(&self, dir: &Path, name: &str) -> bool;
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
}

impl Classifier {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            floor: None,
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
        DIR_RULES
            .iter()
            .find(|rule| self.matches(&rule.matcher, path, probe))
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
        let mut chain: Vec<&Path> = dir.ancestors().collect();
        chain.reverse();
        let mut ctx = None;
        for ancestor in chain {
            if self
                .floor
                .as_ref()
                .is_some_and(|f| !ancestor.starts_with(f))
            {
                continue;
            }
            ctx = self.descend(ancestor, ctx, probe).0;
        }
        ctx
    }

    /// Classify a file given the context of its parent directory.
    pub fn resolve_file(&self, path: &Path, ctx: Option<&Context>) -> Resolved {
        match ctx {
            Some(ctx) if ctx.rule.kind == RuleKind::Leaf || !ctx.rule.file_rules_apply => {
                Resolved::Dir(ctx.clone())
            }
            Some(ctx) => match match_file(path) {
                Some(rule) => Resolved::File(rule),
                None => Resolved::Dir(ctx.clone()),
            },
            None => match match_file(path) {
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
            let parent_ctx = path.parent().and_then(|p| self.context_at(p, probe));
            self.resolve_file(path, parent_ctx.as_ref())
                .to_classification()
        }
    }
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

fn components_equal(rel: &Path, parts: &[&str]) -> bool {
    let mut comps = rel.components();
    for part in parts {
        match comps.next() {
            Some(Component::Normal(c)) if c == OsStr::new(part) => {}
            _ => return false,
        }
    }
    comps.next().is_none()
}

#[cfg(test)]
mod tests {
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
        Classifier::new("/Users/test")
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
    fn floor_ignores_ancestor_context() {
        let p = FakeProbe::empty();
        let floored = Classifier::new("/Users/test").with_floor("/Users/test/Library/Caches/x");
        let c = floored.classify_path(Path::new("/Users/test/Library/Caches/x/y"), true, &p);
        assert_eq!(c.category, Category::Unknown);
    }
}
