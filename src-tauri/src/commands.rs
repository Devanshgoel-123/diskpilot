//! Typed Tauri commands: the only interface between the UI and the engine.
//!
//! Every path argument is validated by `safety` before use. No command in
//! Phase 1 modifies the filesystem.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::classifier::Classifier;
use crate::disk;
use crate::error::{AppError, AppResult};
use crate::explorer;
use crate::model::{
    AppEnvironment, DirectoryListing, DiskInfo, ItemDetails, ScanFinished, ScanRecord, ScanStatus,
    ScanSummary,
};
use crate::safety::{self, SafetyError};
use crate::scanner::{self, now_ms, ScanError, ScanOptions, ScanOutcome};
use crate::state::AppState;

pub const EVENT_SCAN_PROGRESS: &str = "scan://progress";
pub const EVENT_SCAN_FINISHED: &str = "scan://finished";

#[tauri::command]
pub fn get_environment(state: State<'_, AppState>) -> AppEnvironment {
    AppEnvironment {
        home_dir: state.home.display().to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        read_only: true,
    }
}

#[tauri::command]
pub fn get_disk_info(state: State<'_, AppState>) -> AppResult<DiskInfo> {
    disk::disk_info(&state.home).map_err(|e| AppError::io(&state.home, &e))
}

/// Start a background scan. Returns the scan id immediately; progress and
/// completion arrive as events.
#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    root: Option<String>,
) -> AppResult<String> {
    let root = match root {
        Some(raw) => safety::parse_user_path(&raw)?,
        None => state.home.clone(),
    };
    let root = scanner::resolve_root(&root).map_err(|e| AppError::Other(e.to_string()))?;

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = state.slot();
        if slot.running.is_some() {
            return Err(AppError::ScanInProgress);
        }
        slot.running = Some(cancel.clone());
    }

    let id = format!("scan-{}", now_ms());
    let opts = ScanOptions::new(id.clone(), root, state.home.clone());
    std::thread::Builder::new()
        .name("diskpilot-scan".into())
        .spawn(move || run_scan(app, opts, cancel))
        .map_err(|e| AppError::Other(format!("Could not start scan thread: {e}")))?;
    Ok(id)
}

fn run_scan(app: AppHandle, opts: ScanOptions, cancel: Arc<AtomicBool>) {
    use tauri::Manager;
    let started_at_ms = now_ms();
    let emitter = app.clone();
    let result = scanner::scan(&opts, &cancel, &move |p| {
        let _ = emitter.emit(EVENT_SCAN_PROGRESS, p);
    });

    let state = app.state::<AppState>();
    let (status, error, record) = match result {
        Ok(outcome) => {
            let s = &outcome.summary;
            let record = ScanRecord {
                id: s.id.clone(),
                root: s.root.clone(),
                started_at_ms: s.started_at_ms,
                finished_at_ms: s.finished_at_ms,
                duration_ms: s.duration_ms,
                total_bytes: s.total_bytes,
                file_count: s.file_count,
                dir_count: s.dir_count,
                error_count: s.error_count,
                status: ScanStatus::Completed,
            };
            let mut slot = state.slot();
            slot.last = Some(Arc::new(outcome));
            slot.running = None;
            (ScanStatus::Completed, None, record)
        }
        Err(err) => {
            state.slot().running = None;
            let status = if matches!(err, ScanError::Cancelled) {
                ScanStatus::Cancelled
            } else {
                ScanStatus::Failed
            };
            let finished = now_ms();
            let record = ScanRecord {
                id: opts.id.clone(),
                root: opts.root.display().to_string(),
                started_at_ms,
                finished_at_ms: finished,
                duration_ms: u64::try_from(finished - started_at_ms).unwrap_or(0),
                total_bytes: 0,
                file_count: 0,
                dir_count: 0,
                error_count: 0,
                status,
            };
            let error = (status == ScanStatus::Failed).then(|| err.to_string());
            (status, error, record)
        }
    };

    if let Some(history) = &state.history {
        if let Err(e) = history.record(&record) {
            eprintln!("diskpilot: failed to record scan history: {e}");
        }
    }
    let _ = app.emit(
        EVENT_SCAN_FINISHED,
        ScanFinished {
            scan_id: opts.id,
            status,
            error,
        },
    );
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> bool {
    match &state.slot().running {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

#[tauri::command]
pub fn is_scan_running(state: State<'_, AppState>) -> bool {
    state.slot().running.is_some()
}

#[tauri::command]
pub fn get_scan_summary(state: State<'_, AppState>) -> Option<ScanSummary> {
    state.slot().last.as_ref().map(|o| o.summary.clone())
}

fn last_scan(state: &AppState) -> AppResult<Arc<ScanOutcome>> {
    state.slot().last.clone().ok_or(AppError::NoScan)
}

/// Validate a UI-supplied path and confine it to the last scan's root.
fn path_in_scan(raw: &str, outcome: &ScanOutcome) -> AppResult<PathBuf> {
    let path = safety::parse_user_path(raw)?;
    safety::ensure_within(&path, Path::new(&outcome.summary.root))?;
    Ok(path)
}

#[tauri::command]
pub fn list_directory(
    state: State<'_, AppState>,
    path: Option<String>,
) -> AppResult<DirectoryListing> {
    let outcome = last_scan(&state)?;
    let root = PathBuf::from(&outcome.summary.root);
    let path = match path {
        Some(raw) => path_in_scan(&raw, &outcome)?,
        None => root.clone(),
    };
    let classifier = Classifier::new(&state.home);
    explorer::list_directory(&path, &root, &outcome.index, &classifier)
}

#[tauri::command]
pub fn get_item_details(state: State<'_, AppState>, path: String) -> AppResult<ItemDetails> {
    let outcome = last_scan(&state)?;
    let path = path_in_scan(&path, &outcome)?;
    let classifier = Classifier::new(&state.home);
    explorer::item_details(&path, &outcome.index, &classifier)
}

/// Reveal an item in Finder. Uses `open -R` with the path passed as a single
/// argument (no shell), and only for paths inside the scanned folder.
#[tauri::command]
pub fn reveal_in_finder(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let outcome = last_scan(&state)?;
    let path = path_in_scan(&path, &outcome)?;
    if path.symlink_metadata().is_err() {
        return Err(SafetyError::Missing(path.display().to_string()).into());
    }
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(|e| AppError::io(&path, &e))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Other(format!(
            "Finder could not reveal {}",
            path.display()
        )))
    }
}

#[tauri::command]
pub fn get_scan_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<ScanRecord>> {
    match &state.history {
        Some(h) => Ok(h.recent(limit.unwrap_or(20).min(500))?),
        None => Ok(Vec::new()),
    }
}
