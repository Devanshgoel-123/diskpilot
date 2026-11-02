use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::history::History;
use crate::model::{CleanupPlan, Recommendation};
use crate::scanner::ScanOutcome;

#[derive(Default)]
pub struct ScanSlot {
    /// Cancellation flag of the scan currently running, if any.
    pub running: Option<Arc<AtomicBool>>,
    /// The most recent completed scan.
    pub last: Option<Arc<ScanOutcome>>,
    /// Recommendations for `last`; items removed by a cleanup are dropped.
    pub recommendations: Arc<Vec<Recommendation>>,
}

pub struct AppState {
    pub home: PathBuf,
    pub scan: Mutex<ScanSlot>,
    /// Previewed, not yet executed cleanup plans (single use, short-lived).
    pub plans: Mutex<HashMap<String, CleanupPlan>>,
    /// `None` if the history database could not be opened; the app still works.
    pub history: Option<Arc<History>>,
}

impl AppState {
    pub fn slot(&self) -> MutexGuard<'_, ScanSlot> {
        self.scan.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn plans(&self) -> MutexGuard<'_, HashMap<String, CleanupPlan>> {
        self.plans.lock().unwrap_or_else(|e| e.into_inner())
    }
}
