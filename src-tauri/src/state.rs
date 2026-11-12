use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use crate::assistant::Conversation;
use crate::devtools::DevToolsReport;
use crate::history::History;
use crate::model::{CleanupPlan, Recommendation};
use crate::recommend::drop_nested_actionable;
use crate::scanner::ScanOutcome;

#[derive(Default)]
pub struct ScanSlot {
    /// Cancellation flag of the scan currently running, if any.
    pub running: Option<Arc<AtomicBool>>,
    /// The most recent completed scan.
    pub last: Option<Arc<ScanOutcome>>,
    /// Recommendations for `last`; items removed by a cleanup are dropped.
    pub recommendations: Arc<Vec<Recommendation>>,
    /// Latest developer-tools analysis and the recommendations it produced.
    pub devtools: Option<Arc<DevToolsReport>>,
    pub devtools_recommendations: Arc<Vec<Recommendation>>,
}

impl ScanSlot {
    /// Scan and analyzer recommendations combined, without duplicates or
    /// actionable items nested inside other actionable items. When both found
    /// the same path, the analyzer's version wins: it knows the tool and
    /// explains the item more specifically.
    pub fn all_recommendations(&self) -> Vec<Recommendation> {
        let mut seen = std::collections::HashSet::new();
        let combined: Vec<Recommendation> = self
            .devtools_recommendations
            .iter()
            .chain(self.recommendations.iter())
            .filter(|r| seen.insert(r.id.clone()))
            .cloned()
            .collect();
        let mut out = drop_nested_actionable(combined);
        out.sort_by(|a, b| a.tier.cmp(&b.tier).then(b.bytes.cmp(&a.bytes)));
        out
    }

    /// Forget recommendations whose paths were cleaned up.
    pub fn remove_paths(&mut self, done: &std::collections::HashSet<String>) {
        let keep = |v: &Arc<Vec<Recommendation>>| {
            Arc::new(
                v.iter()
                    .filter(|r| !done.contains(&r.path))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        self.recommendations = keep(&self.recommendations);
        self.devtools_recommendations = keep(&self.devtools_recommendations);
    }
}

/// Whether the scan saved by the previous launch has been loaded. It loads on
/// a background thread so the window can open first; reading the scan slot
/// waits for it.
#[derive(Default)]
pub struct Restore {
    done: Mutex<bool>,
    ready: Condvar,
}

impl Restore {
    /// Longest a reader waits for the saved scan before going on without it.
    const LIMIT: Duration = Duration::from_secs(10);

    pub fn finished() -> Self {
        Restore {
            done: Mutex::new(true),
            ready: Condvar::new(),
        }
    }

    pub fn finish(&self) {
        *self.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.ready.notify_all();
    }

    pub fn wait(&self) {
        let done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self
            .ready
            .wait_timeout_while(done, Self::LIMIT, |done| !*done)
            .unwrap_or_else(|e| e.into_inner());
    }
}

pub struct AppState {
    pub home: PathBuf,
    pub scan: Mutex<ScanSlot>,
    /// Set once the previous launch's scan is in `scan` (or there was none).
    pub restore: Restore,
    /// Previewed, not yet executed cleanup plans (single use, short-lived).
    pub plans: Mutex<HashMap<String, CleanupPlan>>,
    /// `None` if the history database could not be opened; the app still works.
    pub history: Option<Arc<History>>,
    /// The assistant conversation (kept in memory only).
    pub assistant: Mutex<Conversation>,
    /// Where the last scan is saved between launches; `None` if unavailable.
    pub data_dir: Option<PathBuf>,
    /// Trash previews and the empty-Trash operation in progress.
    pub trash: Mutex<crate::trash::TrashSlot>,
    /// The last duplicate search and the one in progress.
    pub duplicates: Mutex<crate::duplicates::DuplicateSlot>,
    /// Docker previews shown to the person, by id, with when they were made.
    /// Executing names one; it is single use and short-lived.
    pub docker_reviews: Mutex<HashMap<String, (crate::docker::actions::Reviewed, i64)>>,
}

impl AppState {
    /// The scan slot, once the previous launch's results have loaded.
    pub fn slot(&self) -> MutexGuard<'_, ScanSlot> {
        self.restore.wait();
        self.scan.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn trash(&self) -> MutexGuard<'_, crate::trash::TrashSlot> {
        self.trash.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn duplicates(&self) -> MutexGuard<'_, crate::duplicates::DuplicateSlot> {
        self.duplicates.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn docker_reviews(
        &self,
    ) -> MutexGuard<'_, HashMap<String, (crate::docker::actions::Reviewed, i64)>> {
        self.docker_reviews
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    pub fn plans(&self) -> MutexGuard<'_, HashMap<String, CleanupPlan>> {
        self.plans.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Classification, CleanupMethod, Confidence, Tier};

    fn rec(path: &str, tier: Tier, source: &str) -> Recommendation {
        Recommendation {
            id: path.into(),
            title: path.into(),
            path: path.into(),
            bytes: 10,
            file_count: 1,
            tier,
            confidence: Confidence::High,
            classification: Classification {
                category: Category::Cache,
                label: String::new(),
                explanation: String::new(),
                rule_id: String::new(),
                confidence: Confidence::High,
                matched_path: None,
            },
            detected_because: String::new(),
            safe_because: String::new(),
            risks: String::new(),
            proposed_action: String::new(),
            methods: vec![CleanupMethod::Trash],
            alternative: None,
            estimated_reclaim_bytes: 10,
            last_activity_ms: None,
            notes: vec![],
            source: source.into(),
            marker: None,
            clear_in_app: None,
        }
    }

    #[test]
    fn analyzer_version_of_a_path_wins() {
        let slot = ScanSlot {
            recommendations: Arc::new(vec![rec("/u/a", Tier::LikelySafe, "scan")]),
            devtools_recommendations: Arc::new(vec![rec("/u/a", Tier::LikelySafe, "cursor")]),
            ..Default::default()
        };
        let all = slot.all_recommendations();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].source, "cursor");
    }

    #[test]
    fn merges_without_duplicates_or_nesting_and_forgets_cleaned_paths() {
        let mut slot = ScanSlot {
            recommendations: Arc::new(vec![
                rec("/h/a", Tier::ReviewFirst, "scan"),
                rec("/h/b", Tier::LikelySafe, "scan"),
            ]),
            devtools_recommendations: Arc::new(vec![
                rec("/h/a", Tier::ReviewFirst, "cursor"),
                rec("/h/b/inner", Tier::LikelySafe, "xcode"),
                rec("/h/c", Tier::ReviewFirst, "cursor"),
            ]),
            ..Default::default()
        };
        let all = slot.all_recommendations();
        let paths: Vec<&str> = all.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["/h/b", "/h/a", "/h/c"]);

        slot.remove_paths(&["/h/a".to_string()].into_iter().collect());
        assert!(!slot.all_recommendations().iter().any(|r| r.path == "/h/a"));
    }
}

#[cfg(test)]
mod restore_tests {
    use super::Restore;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn readers_wait_until_the_saved_scan_is_in() {
        let r = Arc::new(Restore::default());
        let r2 = Arc::clone(&r);
        let t = Instant::now();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            r2.finish();
        });
        r.wait();
        assert!(t.elapsed() >= Duration::from_millis(40));
        // Once finished, waiting returns at once.
        let t = Instant::now();
        r.wait();
        assert!(t.elapsed() < Duration::from_millis(20));
        Restore::finished().wait();
    }
}
