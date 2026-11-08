//! Recommendation engine.
//!
//! Turns scan results into explained recommendations. It only *describes*
//! possible actions; it cannot execute anything. Execution requires a plan
//! (`cleanup::planner`) and explicit user confirmation.

pub mod policies;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::classifier::{Classifier, FsProbe};
use crate::model::{Classification, Recommendation, ScanSummary, Tier};
use crate::scanner::{to_ms, DirIndex};
use policies::{
    policy_for, Expand, Policy, DOWNLOAD_INSTALLER_EXTENSIONS,
    WINDOWS_DOWNLOAD_INSTALLER_EXTENSIONS,
};

const DAY_MS: i64 = 86_400_000;
/// A project untouched this long is called out as possibly inactive.
const STALE_DAYS: i64 = 90;

struct Candidate<'a> {
    path: PathBuf,
    bytes: u64,
    files: u64,
    policy: &'a Policy,
    classification: Classification,
}

/// Build recommendations from a completed scan.
pub fn recommend(
    summary: &ScanSummary,
    index: &DirIndex,
    classifier: &Classifier,
    now_ms: i64,
) -> Vec<Recommendation> {
    let detected: HashSet<&str> = summary
        .detected_locations
        .iter()
        .map(|l| l.path.as_str())
        .collect();

    // Locations whose children are recommended individually.
    let mut expand_parents: HashMap<PathBuf, (&Policy, &Classification)> = HashMap::new();
    let mut candidates: Vec<Candidate> = Vec::new();

    for loc in &summary.detected_locations {
        let Some(policy) = policy_for(&loc.classification.rule_id) else {
            continue;
        };
        let path = PathBuf::from(&loc.path);
        match policy.expand {
            Expand::Whole => {
                if loc.bytes >= policy.min_bytes {
                    candidates.push(Candidate {
                        path,
                        bytes: loc.bytes,
                        files: loc.file_count,
                        policy,
                        classification: loc.classification.clone(),
                    });
                }
            }
            Expand::Children => {
                expand_parents.insert(path, (policy, &loc.classification));
            }
            Expand::ChildrenOf(sub) => {
                expand_parents.insert(path.join(sub), (policy, &loc.classification));
            }
        }
    }

    // One pass over the index to find children of expanded locations.
    if !expand_parents.is_empty() {
        for (path, stats) in index {
            let Some(parent) = path.parent() else {
                continue;
            };
            let Some((policy, parent_class)) = expand_parents.get(parent) else {
                continue;
            };
            // A child with its own detection (e.g. Caches/Yarn) gets its own rec.
            if detected.contains(path.to_string_lossy().as_ref()) || stats.bytes < policy.min_bytes
            {
                continue;
            }
            let mut classification = (*parent_class).clone();
            classification.matched_path = Some(parent.display().to_string());
            candidates.push(Candidate {
                path: path.clone(),
                bytes: stats.bytes,
                files: stats.files,
                policy,
                classification,
            });
        }
    }

    let mut recs: Vec<Recommendation> = candidates.into_iter().map(|c| build(c, now_ms)).collect();
    recs.extend(downloads_installers(summary, classifier));

    let mut recs = drop_nested_actionable(recs);
    recs.sort_by(|a, b| a.tier.cmp(&b.tier).then(b.bytes.cmp(&a.bytes)));
    recs
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn build(c: Candidate, now_ms: i64) -> Recommendation {
    let p = c.policy;
    let mut notes = Vec::new();
    let mut title = c.classification.label.clone();
    if p.expand != Expand::Whole {
        title = format!("{} · {}", title, display_name(&c.path));
    }

    let mut last_activity = None;
    if p.rule_id == "node.node-modules" {
        if let Some(project) = c.path.parent() {
            title = format!("node_modules · {}", display_name(project));
            last_activity = project_activity_ms(project);
            if let Some(ms) = last_activity {
                let days = (now_ms - ms) / DAY_MS;
                if days >= STALE_DAYS {
                    notes.push(format!(
                        "The project's package files have not changed in {days} days; it may be inactive."
                    ));
                }
            }
        }
    }
    if p.rule_id == "devices.iphone-backups" {
        if let Some(info) = backup_info(&c.path) {
            if let Some(name) = &info.device_name {
                title = format!("{} · {}", c.classification.label, name);
            }
            let mut device = Vec::new();
            if let Some(model) = &info.product_type {
                device.push(model.clone());
            }
            if let Some(version) = &info.product_version {
                device.push(format!("iOS {version}"));
            }
            if !device.is_empty() {
                notes.push(format!("Device: {}.", device.join(", ")));
            }
            if let Some(ms) = info.last_backup_ms {
                last_activity = Some(ms);
                let days = (now_ms - ms) / DAY_MS;
                notes.push(format!("Last backed up {days} days ago."));
            }
        }
    }
    if p.rule_id.starts_with("pnpm.") {
        notes.push(
            "Estimated reclaim is an upper bound: files hard-linked into projects stay on disk."
                .to_string(),
        );
    }
    // Most of WinSxS is hard links into System32; its size says little about
    // what DISM can free, so no estimate is claimed.
    let unknown_reclaim = p.rule_id == "windows.component-store";
    if unknown_reclaim {
        notes.push(
            "Its size overstates what cleanup frees, because most files are shared with System32. DISM /AnalyzeComponentStore reports the real figure."
                .to_string(),
        );
    }

    Recommendation {
        id: c.path.display().to_string(),
        title,
        path: c.path.display().to_string(),
        bytes: c.bytes,
        file_count: c.files,
        tier: p.tier,
        confidence: c.classification.confidence,
        detected_because: format!(
            "Recognized as {}. {}",
            c.classification.label, c.classification.explanation
        ),
        classification: c.classification,
        safe_because: p.safe_because.to_string(),
        risks: p.risks.to_string(),
        proposed_action: p.proposed_action.to_string(),
        methods: p.methods.to_vec(),
        alternative: p.alternative.map(str::to_string),
        estimated_reclaim_bytes: if unknown_reclaim
            || (p.methods.is_empty() && p.alternative.is_none())
        {
            0
        } else {
            c.bytes
        },
        last_activity_ms: last_activity,
        notes,
        source: "scan".into(),
        marker: None,
        clear_in_app: policies::owner_app(p.rule_id).map(|a| a.name.to_string()),
    }
}

/// What a device backup's Info.plist says about it.
#[derive(Debug, Default, PartialEq)]
pub struct BackupInfo {
    pub device_name: Option<String>,
    pub product_type: Option<String>,
    pub product_version: Option<String>,
    pub last_backup_ms: Option<i64>,
}

/// Read a Finder/iTunes backup's Info.plist. Large files are skipped rather
/// than loaded (the plist can embed app data).
pub fn backup_info(backup: &Path) -> Option<BackupInfo> {
    const MAX_PLIST: u64 = 16 * 1024 * 1024;
    let file = backup.join("Info.plist");
    let meta = std::fs::metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > MAX_PLIST {
        return None;
    }
    let value = plist::Value::from_file(&file).ok()?;
    let dict = value.as_dictionary()?;
    let text = |key: &str| {
        dict.get(key)
            .and_then(|v| v.as_string())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Some(BackupInfo {
        device_name: text("Device Name").or_else(|| text("Display Name")),
        product_type: text("Product Type"),
        product_version: text("Product Version"),
        last_backup_ms: dict
            .get("Last Backup Date")
            .and_then(|v| v.as_date())
            .and_then(|d| to_ms(std::time::SystemTime::from(d))),
    })
}

/// Most recent modification among a project's dependency manifests.
fn project_activity_ms(project: &Path) -> Option<i64> {
    [
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lockb",
    ]
    .iter()
    .filter_map(|f| project.join(f).symlink_metadata().ok())
    .filter_map(|m| m.modified().ok().and_then(to_ms))
    .max()
}

/// Large installers and archives sitting in ~/Downloads.
fn downloads_installers(summary: &ScanSummary, classifier: &Classifier) -> Vec<Recommendation> {
    let (extensions, bin) = if classifier.is_windows() {
        (WINDOWS_DOWNLOAD_INSTALLER_EXTENSIONS, "Recycle Bin")
    } else {
        (DOWNLOAD_INSTALLER_EXTENSIONS, "Trash")
    };
    summary
        .largest_files
        .iter()
        .filter(|f| f.bytes >= 50_000_000)
        .filter(|f| classifier.is_in_home_folder(Path::new(&f.path), "Downloads"))
        .filter(|f| {
            Path::new(&f.name)
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .is_some_and(|e| extensions.contains(&e.as_str()))
        })
        .map(|f| Recommendation {
            id: f.path.clone(),
            title: format!("Installer or archive · {}", f.name),
            path: f.path.clone(),
            bytes: f.bytes,
            file_count: 1,
            tier: Tier::ReviewFirst,
            confidence: f.classification.confidence,
            detected_because: "An installer, disk image or archive in your Downloads folder."
                .to_string(),
            classification: classifier.classify_path(Path::new(&f.path), false, &FsProbe),
            safe_because: "Installers are usually not needed after the app is installed.".to_string(),
            risks: "This may be the only copy of what it contains, or an installer you cannot download again.".to_string(),
            proposed_action: format!("Move the file to the {bin}"),
            methods: vec![crate::model::CleanupMethod::Trash],
            alternative: None,
            estimated_reclaim_bytes: f.bytes,
            last_activity_ms: f.modified_ms,
            notes: Vec::new(),
            source: "scan".into(),
            marker: None,
            clear_in_app: None,
        })
        .collect()
}

/// An actionable item inside another actionable item would be counted and
/// removed twice; keep only the outer one.
pub fn drop_nested_actionable(recs: Vec<Recommendation>) -> Vec<Recommendation> {
    let actionable: Vec<PathBuf> = recs
        .iter()
        .filter(|r| !r.methods.is_empty())
        .map(|r| PathBuf::from(&r.path))
        .collect();
    recs.into_iter()
        .filter(|r| {
            if r.methods.is_empty() {
                return true;
            }
            let p = Path::new(&r.path);
            !actionable
                .iter()
                .any(|outer| p != outer && p.starts_with(outer))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CleanupMethod, Confidence, DetectedLocation, FileEntry};
    use crate::scanner::DirStats;

    const HOME: &str = "/Users/test";

    fn class(rule: &str, label: &str) -> Classification {
        Classification {
            category: crate::model::Category::Cache,
            label: label.into(),
            explanation: "x".into(),
            rule_id: rule.into(),
            confidence: Confidence::High,
            matched_path: None,
        }
    }

    fn loc(path: &str, bytes: u64, rule: &str) -> DetectedLocation {
        DetectedLocation {
            path: path.into(),
            bytes,
            file_count: 10,
            container: false,
            classification: class(rule, rule),
        }
    }

    fn summary(locations: Vec<DetectedLocation>, files: Vec<FileEntry>) -> ScanSummary {
        ScanSummary {
            id: "s".into(),
            root: HOME.into(),
            started_at_ms: 0,
            finished_at_ms: 0,
            duration_ms: 0,
            total_bytes: 0,
            file_count: 0,
            dir_count: 0,
            symlink_count: 0,
            skipped_volumes: 0,
            error_count: 0,
            error_samples: vec![],
            categories: vec![],
            largest_files: files,
            detected_locations: locations,
            heavy_directories: vec![],
            file_kinds: Vec::new(),
        }
    }

    fn stats(bytes: u64) -> DirStats {
        DirStats {
            bytes,
            files: 1,
            readable: true,
            ..Default::default()
        }
    }

    fn run(s: &ScanSummary, index: &DirIndex) -> Vec<Recommendation> {
        recommend(s, index, &Classifier::macos(HOME), 0)
    }

    #[test]
    fn maps_rules_to_tiers_and_methods() {
        let s = summary(
            vec![
                loc(
                    "/Users/test/Library/Developer/Xcode/DerivedData",
                    2_000_000_000,
                    "xcode.derived-data",
                ),
                loc(
                    "/Users/test/code/a/node_modules",
                    300_000_000,
                    "node.node-modules",
                ),
                loc(
                    "/Users/test/Library/Containers/com.docker.docker",
                    9_000_000_000,
                    "docker.desktop-vm",
                ),
            ],
            vec![],
        );
        let recs = run(&s, &DirIndex::new());
        assert_eq!(recs.len(), 3);
        // Sorted by tier, not size.
        assert_eq!(recs[0].tier, Tier::LikelySafe);
        assert_eq!(recs[1].tier, Tier::ReviewFirst);
        assert_eq!(recs[1].methods, vec![CleanupMethod::Trash]);
        assert_eq!(recs[2].tier, Tier::Keep);
        assert!(recs[2].methods.is_empty());
        assert!(recs
            .iter()
            .all(|r| !r.detected_because.is_empty() && !r.risks.is_empty()));
    }

    #[test]
    fn small_items_and_unknown_rules_are_ignored() {
        let s = summary(
            vec![
                loc("/Users/test/.npm", 1_000, "npm.cache"),
                loc("/Users/test/Documents", 50_000_000_000, "home.documents"),
            ],
            vec![],
        );
        assert!(run(&s, &DirIndex::new()).is_empty());
    }

    #[test]
    fn caches_expand_per_app_and_skip_separately_detected_children() {
        let caches = "/Users/test/Library/Caches";
        let s = summary(
            vec![
                loc(caches, 5_000_000_000, "library.caches"),
                loc("/Users/test/Library/Caches/Yarn", 900_000_000, "yarn.cache"),
            ],
            vec![],
        );
        let mut index = DirIndex::new();
        index.insert(
            PathBuf::from(format!("{caches}/com.app.big")),
            stats(800_000_000),
        );
        index.insert(
            PathBuf::from(format!("{caches}/com.app.tiny")),
            stats(1_000),
        );
        index.insert(PathBuf::from(format!("{caches}/Yarn")), stats(900_000_000));
        index.insert(
            PathBuf::from(format!("{caches}/com.app.big/nested")),
            stats(700_000_000),
        );
        let recs = run(&s, &index);
        let paths: Vec<&str> = recs.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"/Users/test/Library/Caches/com.app.big"));
        assert!(paths.contains(&"/Users/test/Library/Caches/Yarn"));
        assert!(!paths.contains(&caches));
        assert!(!paths.contains(&"/Users/test/Library/Caches/com.app.tiny"));
        assert!(!paths.contains(&"/Users/test/Library/Caches/com.app.big/nested"));
        assert_eq!(recs.iter().filter(|r| r.path.ends_with("Yarn")).count(), 1);
    }

    #[test]
    fn go_module_cache_is_left_to_go_and_the_build_cache_is_offered() {
        use crate::cleanup::planner::build_plan;
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let at = |rel: &str| {
            std::fs::create_dir_all(home.join(rel)).unwrap();
            home.join(rel).display().to_string()
        };
        let (caches, build_cache, modcache) = (
            at("Library/Caches"),
            at("Library/Caches/go-build"),
            at("go/pkg/mod"),
        );
        let (build_cache, modcache) = (build_cache.as_str(), modcache.as_str());
        let s = summary(
            vec![
                loc(&caches, 5_000_000_000, "library.caches"),
                loc(build_cache, 900_000_000, "go.build-cache"),
                loc(modcache, 3_000_000_000, "go.mod-cache"),
            ],
            vec![],
        );
        let mut index = DirIndex::new();
        index.insert(PathBuf::from(build_cache), stats(900_000_000));
        let recs = recommend(&s, &index, &Classifier::new(&home), 0);
        let find = |p: &str| recs.iter().find(|r| r.path == p).unwrap();

        let modules = find(modcache);
        assert_eq!(modules.tier, Tier::ReviewFirst);
        assert!(modules.methods.is_empty());
        assert_eq!(modules.alternative.as_deref(), Some("go clean -modcache"));
        let build = find(build_cache);
        assert_eq!(build.tier, Tier::LikelySafe);
        assert_eq!(build.alternative.as_deref(), Some("go clean -cache"));
        assert_eq!(recs.iter().filter(|r| r.path == build_cache).count(), 1);

        let ids = [modcache.to_string(), build_cache.to_string()];
        for method in [CleanupMethod::Trash, CleanupMethod::Delete] {
            let plan = build_plan("p".into(), &recs, &ids, method, &home, &home, 0);
            let planned: Vec<&str> = plan.items.iter().map(|i| i.path.as_str()).collect();
            assert_eq!(planned, vec![build_cache]);
            assert_eq!(plan.rejected[0].path, modcache);
        }
    }

    #[test]
    fn nested_actionable_items_are_not_double_counted() {
        let s = summary(
            vec![
                loc("/Users/test/.cache", 3_000_000_000, "xdg.cache"),
                loc(
                    "/Users/test/code/a/node_modules",
                    300_000_000,
                    "node.node-modules",
                ),
                loc(
                    "/Users/test/code/a/node_modules/x/node_modules",
                    200_000_000,
                    "node.node-modules",
                ),
            ],
            vec![],
        );
        let recs = run(&s, &DirIndex::new());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].path, "/Users/test/code/a/node_modules");
    }

    #[test]
    fn downloads_installers_are_review_first() {
        let file = |name: &str, bytes| FileEntry {
            path: format!("/Users/test/Downloads/{name}"),
            name: name.into(),
            bytes,
            logical_bytes: bytes,
            modified_ms: None,
            classification: class("home.downloads", "Downloads"),
        };
        let s = summary(
            vec![],
            vec![
                file("Big.dmg", 900_000_000),
                file("movie.mov", 900_000_000),
                file("small.zip", 10),
            ],
        );
        let recs = run(&s, &DirIndex::new());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].tier, Tier::ReviewFirst);
        assert!(recs[0].title.contains("Big.dmg"));
    }

    #[test]
    fn iphone_backups_are_listed_per_device_with_details() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Backup");
        let backup = root.join("00008110-000A1B2C3D4E5F6A");
        std::fs::create_dir_all(&backup).unwrap();
        let mut info = plist::Dictionary::new();
        info.insert("Device Name".into(), "Test iPhone".into());
        info.insert("Product Type".into(), "iPhone14,5".into());
        info.insert("Product Version".into(), "17.4".into());
        let date = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        info.insert("Last Backup Date".into(), plist::Value::Date(date.into()));
        plist::Value::Dictionary(info)
            .to_file_xml(backup.join("Info.plist"))
            .unwrap();

        let s = summary(
            vec![loc(
                root.to_str().unwrap(),
                9_000_000_000,
                "devices.iphone-backups",
            )],
            vec![],
        );
        let mut index = DirIndex::new();
        index.insert(backup.clone(), stats(8_000_000_000));
        let now = 1_700_000_000_000 + 40 * DAY_MS;
        let recs = recommend(&s, &index, &Classifier::macos(HOME), now);
        assert_eq!(recs.len(), 1);
        let r = &recs[0];
        assert_eq!(r.title, format!("{} · Test iPhone", r.classification.label));
        assert_eq!(r.tier, Tier::ReviewFirst);
        assert_eq!(r.methods, vec![CleanupMethod::Trash]);
        assert_eq!(r.last_activity_ms, Some(1_700_000_000_000));
        assert!(r.notes.iter().any(|n| n == "Device: iPhone14,5, iOS 17.4."));
        assert!(r.notes.iter().any(|n| n == "Last backed up 40 days ago."));
    }

    #[test]
    fn backup_without_info_plist_has_no_details() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(backup_info(dir.path()), None);
    }

    #[test]
    fn every_policy_references_a_real_rule_and_explains_risk() {
        use crate::classifier::rules::DIR_RULES;
        use crate::classifier::windows_rules::WINDOWS_DIR_RULES;
        let mut ids = std::collections::HashSet::new();
        for p in policies::POLICIES.iter().chain(policies::WINDOWS_POLICIES) {
            assert!(ids.insert(p.rule_id), "two policies for {}", p.rule_id);
            assert!(
                DIR_RULES
                    .iter()
                    .chain(WINDOWS_DIR_RULES)
                    .any(|r| r.id == p.rule_id),
                "unknown rule {}",
                p.rule_id
            );
            assert!(!p.risks.is_empty(), "{} lacks risks", p.rule_id);
            if p.tier != Tier::Keep {
                assert!(!p.safe_because.is_empty(), "{} lacks rationale", p.rule_id);
            } else {
                assert!(
                    p.methods.is_empty(),
                    "keep-tier {} must not be actionable",
                    p.rule_id
                );
            }
            if p.methods.contains(&CleanupMethod::Delete) {
                assert_ne!(p.tier, Tier::Keep);
            }
        }
    }

    #[test]
    fn windows_installers_in_downloads_are_review_first() {
        use crate::classifier::WindowsRoots;
        let file = |name: &str| FileEntry {
            path: format!(r"C:\Users\Test\Downloads\{name}"),
            name: name.into(),
            bytes: 900_000_000,
            logical_bytes: 900_000_000,
            modified_ms: None,
            classification: class("home.downloads", "Downloads"),
        };
        let s = summary(
            vec![],
            vec![
                file("Setup.EXE"),
                file("App.msix"),
                file("Big.dmg"),
                file("movie.mp4"),
            ],
        );
        let c = Classifier::windows(WindowsRoots::for_home(r"C:\Users\Test"));
        let recs = recommend(&s, &DirIndex::new(), &c, 0);
        let titles: Vec<&str> = recs.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(recs.len(), 2, "{titles:?}");
        assert!(recs.iter().all(|r| r.tier == Tier::ReviewFirst
            && r.proposed_action == "Move the file to the Recycle Bin"));
    }

    #[test]
    fn windows_system_areas_point_to_windows_tools_and_are_never_actionable() {
        let s = summary(
            vec![
                loc(
                    r"C:\Windows\WinSxS",
                    9_000_000_000,
                    "windows.component-store",
                ),
                loc(r"C:\Windows.old", 20_000_000_000, "windows.old"),
                loc(
                    r"C:\Windows\SoftwareDistribution\Download",
                    2_000_000_000,
                    "windows.update-cache",
                ),
                loc(
                    r"C:\Windows\Installer",
                    5_000_000_000,
                    "windows.installer-cache",
                ),
            ],
            vec![],
        );
        let recs = run(&s, &DirIndex::new());
        assert_eq!(recs.len(), 4);
        assert!(recs.iter().all(|r| r.methods.is_empty()));
        let find = |p: &str| recs.iter().find(|r| r.path == p).unwrap();
        let sxs = find(r"C:\Windows\WinSxS");
        assert!(sxs
            .alternative
            .as_deref()
            .unwrap()
            .contains("/StartComponentCleanup"));
        // Hard links make WinSxS look bigger than what cleanup frees.
        assert_eq!(sxs.estimated_reclaim_bytes, 0);
        assert!(sxs
            .notes
            .iter()
            .any(|n| n.contains("AnalyzeComponentStore")));
        assert_eq!(
            find(r"C:\Windows.old").estimated_reclaim_bytes,
            20_000_000_000
        );
        assert_eq!(find(r"C:\Windows\Installer").tier, Tier::Keep);
    }

    #[test]
    fn windows_policies_never_act_on_system_areas() {
        use crate::classifier::rules::Matcher;
        use crate::classifier::rules::Root;
        use crate::classifier::windows_rules::WINDOWS_DIR_RULES;
        let system = [
            Root::SystemRoot,
            Root::ProgramData,
            Root::SystemDrive,
            Root::AnyDrive,
        ];
        for rule in WINDOWS_DIR_RULES {
            let Some(policy) = policies::policy_for(rule.id) else {
                continue;
            };
            let in_system = matches!(&rule.matcher, Matcher::Under(opts)
                if opts.iter().any(|(root, _)| system.contains(root)));
            let steam = rule.id.starts_with("steam.") && rule.id != "steam.web-cache";
            if in_system || steam {
                assert!(
                    policy.methods.is_empty(),
                    "{} must not be actionable",
                    rule.id
                );
            }
            if rule.category == crate::model::Category::SystemFiles {
                assert!(policy.methods.is_empty(), "{}", rule.id);
                assert!(
                    policy.alternative.is_some() || policy.tier == Tier::Keep,
                    "{} names no Windows tool",
                    rule.id
                );
            }
        }
    }
}
