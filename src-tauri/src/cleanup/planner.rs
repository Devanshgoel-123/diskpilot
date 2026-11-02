//! Builds a previewable cleanup plan from recommendation ids.
//!
//! The UI sends only ids; paths are always taken from the engine's own
//! recommendations, never from the UI.

use std::path::Path;

use crate::model::{CleanupMethod, CleanupPlan, PlanItem, PlanRejection, Recommendation};
use crate::recommend::policies::policy_for;
use crate::safety;

/// Plans expire so a stale preview can't be executed later.
pub const PLAN_TTL_MS: i64 = 10 * 60 * 1000;

pub fn build_plan(
    id: String,
    recommendations: &[Recommendation],
    selected_ids: &[String],
    method: CleanupMethod,
    scan_root: &Path,
    home: &Path,
    now_ms: i64,
) -> CleanupPlan {
    let mut items = Vec::new();
    let mut rejected = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let mut seen = std::collections::HashSet::new();
    for rid in selected_ids {
        if !seen.insert(rid) {
            continue;
        }
        let reject = |path: &str, reason: String| PlanRejection {
            recommendation_id: rid.clone(),
            path: path.to_string(),
            reason,
        };
        let Some(rec) = recommendations.iter().find(|r| &r.id == rid) else {
            rejected.push(reject(
                "",
                "Recommendation not found; rescan and try again.".into(),
            ));
            continue;
        };
        if !rec.methods.contains(&method) {
            let reason = if rec.methods.is_empty() {
                "DiskPilot does not remove this item automatically.".to_string()
            } else {
                format!("“{}” is not offered for this item.", method.describe())
            };
            rejected.push(reject(&rec.path, reason));
            continue;
        }
        // Cleanup is confined to the scanned folder and to the home folder.
        let authorized = safety::authorize_destructive(&rec.path, scan_root, home)
            .and_then(|p| safety::ensure_within(&p, home).map(|_| p));
        if let Err(e) = authorized {
            rejected.push(reject(&rec.path, e.to_string()));
            continue;
        }
        if let Some(note) = policy_for(&rec.classification.rule_id).and_then(|p| p.before_you_start)
        {
            if !warnings.iter().any(|w| w == note) {
                warnings.push(note.to_string());
            }
        }
        items.push(PlanItem {
            recommendation_id: rec.id.clone(),
            title: rec.title.clone(),
            path: rec.path.clone(),
            bytes: rec.bytes,
            file_count: rec.file_count,
            tier: rec.tier,
            operation: method.describe().to_string(),
        });
    }

    match method {
        CleanupMethod::Trash => warnings.push(
            "Items go to the Trash. Their space is freed only when you empty the Trash; until then you can put them back."
                .into(),
        ),
        CleanupMethod::Delete => warnings.push(
            "Permanently deleted items cannot be restored by DiskPilot.".into(),
        ),
    }

    CleanupPlan {
        id,
        created_at_ms: now_ms,
        method,
        total_bytes: items.iter().map(|i| i.bytes).sum(),
        items,
        rejected,
        warnings,
    }
}

/// Results older than this are re-measured when a preview is built.
pub const REMEASURE_AFTER_MS: i64 = 10 * 60 * 1000;

/// Replace scanned sizes with what is on disk now, so a preview built from
/// older (for example, saved) results shows current figures. Items that no
/// longer exist move to `rejected`. `measure` returns `None` for a missing
/// path.
pub fn refresh_sizes(plan: &mut CleanupPlan, measure: impl Fn(&Path) -> Option<u64>) {
    let items = std::mem::take(&mut plan.items);
    for mut item in items {
        match measure(Path::new(&item.path)) {
            Some(bytes) => {
                item.bytes = bytes;
                plan.items.push(item);
            }
            None => plan.rejected.push(PlanRejection {
                recommendation_id: item.recommendation_id,
                path: item.path,
                reason: "No longer exists. It was moved or removed after the scan.".into(),
            }),
        }
    }
    plan.total_bytes = plan.items.iter().map(|i| i.bytes).sum();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Classification, Confidence, Tier};

    fn rec(path: &str, rule: &str, methods: Vec<CleanupMethod>) -> Recommendation {
        Recommendation {
            id: path.into(),
            title: "t".into(),
            path: path.into(),
            bytes: 100,
            file_count: 1,
            tier: Tier::LikelySafe,
            confidence: Confidence::High,
            classification: Classification {
                category: Category::Cache,
                label: "l".into(),
                explanation: "e".into(),
                rule_id: rule.into(),
                confidence: Confidence::High,
                matched_path: None,
            },
            detected_because: String::new(),
            safe_because: String::new(),
            risks: String::new(),
            proposed_action: String::new(),
            methods,
            alternative: None,
            estimated_reclaim_bytes: 100,
            last_activity_ms: None,
            notes: vec![],
            source: "scan".into(),
        }
    }

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(home.join("proj/node_modules")).unwrap();
        std::fs::create_dir_all(home.join("Library/Caches/com.x")).unwrap();
        (dir, home)
    }

    #[test]
    fn plans_only_known_actionable_items_inside_home() {
        let (_d, home) = fixture();
        let nm = home.join("proj/node_modules").display().to_string();
        let cache = home.join("Library/Caches/com.x").display().to_string();
        let recs = vec![
            rec(&nm, "node.node-modules", vec![CleanupMethod::Trash]),
            rec(
                &cache,
                "library.caches",
                vec![CleanupMethod::Trash, CleanupMethod::Delete],
            ),
            rec(
                "/System/Library",
                "system.system",
                vec![CleanupMethod::Trash],
            ),
            rec(
                &home.join("Docker").display().to_string(),
                "docker.desktop-vm",
                vec![],
            ),
        ];
        let ids: Vec<String> = recs
            .iter()
            .map(|r| r.id.clone())
            .chain(["bogus".into()])
            .collect();
        let plan = build_plan(
            "p".into(),
            &recs,
            &ids,
            CleanupMethod::Trash,
            &home,
            &home,
            0,
        );
        let planned: Vec<&str> = plan.items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(planned, vec![nm.as_str(), cache.as_str()]);
        assert_eq!(plan.rejected.len(), 3);
        assert_eq!(plan.total_bytes, 200);
        assert!(plan.warnings.iter().any(|w| w.contains("Trash")));
        assert!(plan.warnings.iter().any(|w| w.contains("Quit")));
    }

    #[test]
    fn permanent_delete_requires_the_policy_to_allow_it() {
        let (_d, home) = fixture();
        let nm = home.join("proj/node_modules").display().to_string();
        let recs = vec![rec(&nm, "node.node-modules", vec![CleanupMethod::Trash])];
        let plan = build_plan(
            "p".into(),
            &recs,
            &[nm],
            CleanupMethod::Delete,
            &home,
            &home,
            0,
        );
        assert!(plan.items.is_empty());
        assert_eq!(plan.rejected.len(), 1);
    }

    #[test]
    fn rejects_protected_and_symlinked_targets() {
        let (_d, home) = fixture();
        std::os::unix::fs::symlink(home.join("proj"), home.join("link")).unwrap();
        let via_link = home.join("link/node_modules").display().to_string();
        let caches_root = home.join("Library/Caches").display().to_string();
        let recs = vec![
            rec(&via_link, "node.node-modules", vec![CleanupMethod::Trash]),
            rec(&caches_root, "library.caches", vec![CleanupMethod::Trash]),
        ];
        let ids = vec![via_link, caches_root];
        let plan = build_plan(
            "p".into(),
            &recs,
            &ids,
            CleanupMethod::Trash,
            &home,
            &home,
            0,
        );
        assert!(plan.items.is_empty(), "{:?}", plan.items);
        assert_eq!(plan.rejected.len(), 2);
    }

    #[test]
    fn refresh_sizes_remeasures_and_drops_missing_items() {
        let (_d, home) = fixture();
        let nm = home.join("proj/node_modules").display().to_string();
        let cache = home.join("Library/Caches/com.x").display().to_string();
        let recs = vec![
            rec(&nm, "node.node-modules", vec![CleanupMethod::Trash]),
            rec(&cache, "library.caches", vec![CleanupMethod::Trash]),
        ];
        let ids = vec![nm.clone(), cache.clone()];
        let mut plan = build_plan(
            "p".into(),
            &recs,
            &ids,
            CleanupMethod::Trash,
            &home,
            &home,
            0,
        );
        assert_eq!(plan.items.len(), 2);
        refresh_sizes(&mut plan, |p| (p == Path::new(&nm)).then_some(42));
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].bytes, 42);
        assert_eq!(plan.total_bytes, 42);
        assert_eq!(plan.rejected.len(), 1);
        assert_eq!(plan.rejected[0].path, cache);
    }
}
