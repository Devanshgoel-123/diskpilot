use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::history::History;
use crate::scanner::ScanOutcome;

#[derive(Default)]
pub struct ScanSlot {
    /// Cancellation flag of the scan currently running, if any.
    pub running: Option<Arc<AtomicBool>>,
    /// The most recent completed scan.
    pub last: Option<Arc<ScanOutcome>>,
}

pub struct AppState {
    pub home: PathBuf,
    pub scan: Mutex<ScanSlot>,
    /// `None` if the history database could not be opened; the app still works.
    pub history: Option<History>,
}

impl AppState {
    pub fn slot(&self) -> std::sync::MutexGuard<'_, ScanSlot> {
        self.scan.lock().unwrap_or_else(|e| e.into_inner())
    }
}
