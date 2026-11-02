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
use policies::{policy_for, Expand, Policy, DOWNLOAD_INSTALLER_EXTENSIONS};

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
    if p.rule_id.starts_with("pnpm.") {
        notes.push(
            "Estimated reclaim is an upper bound: files hard-linked into projects stay on disk."
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
        estimated_reclaim_bytes: if p.methods.is_empty() && p.alternative.is_none() {
            0
        } else {
            c.bytes
        },
        last_activity_ms: last_activity,
        notes,
    }
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
    let downloads = classifier.home().join("Downloads");
    summary
        .largest_files
        .iter()
        .filter(|f| f.bytes >= 50_000_000)
        .filter(|f| Path::new(&f.path).starts_with(&downloads))
        .filter(|f| {
            Path::new(&f.path)
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .is_some_and(|e| DOWNLOAD_INSTALLER_EXTENSIONS.contains(&e.as_str()))
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
            proposed_action: "Move the file to the Trash".to_string(),
            methods: vec![crate::model::CleanupMethod::Trash],
            alternative: None,
            estimated_reclaim_bytes: f.bytes,
            last_activity_ms: f.modified_ms,
            notes: Vec::new(),
        })
        .collect()
}

/// An actionable item inside another actionable item would be counted and
/// removed twice; keep only the outer one.
fn drop_nested_actionable(recs: Vec<Recommendation>) -> Vec<Recommendation> {
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
        recommend(s, index, &Classifier::new(HOME), 0)
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
    fn every_policy_references_a_real_rule_and_explains_risk() {
        use crate::classifier::rules::DIR_RULES;
        for p in policies::POLICIES {
            assert!(
                DIR_RULES.iter().any(|r| r.id == p.rule_id),
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
}
