//! Typed Tauri commands: the only interface between the UI and the engine.
//!
//! Every path argument is validated by `safety` before use. The only commands
//! that change anything are `execute_cleanup_plan` (files, via the cleanup
//! executor) and `docker_execute` (predefined Docker operations). Both require
//! a preview first.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::classifier::Classifier;
use crate::cleanup::{executor, planner};
use crate::disk;
use crate::docker::{self, actions, cli::DockerCli, DockerOverview, DockerState};
use crate::error::{AppError, AppResult};
use crate::explorer;
use crate::model::{
    AppEnvironment, CleanupMethod, CleanupPlan, CleanupRecord, DirectoryListing, DiskInfo,
    ItemDetails, ItemStatus, Recommendation, ScanFinished, ScanRecord, ScanStatus, ScanSummary,
};
use crate::recommend;
use crate::safety::{self, SafetyError};
use crate::scanner::{self, now_ms, ScanError, ScanOptions, ScanOutcome};
use crate::state::AppState;

pub const EVENT_SCAN_PROGRESS: &str = "scan://progress";
pub const EVENT_SCAN_FINISHED: &str = "scan://finished";

/// Run blocking work off the main thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Other(format!("Background task failed: {e}")))?
}

#[tauri::command]
pub fn get_environment(state: State<'_, AppState>) -> AppEnvironment {
    AppEnvironment {
        home_dir: state.home.display().to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        read_only: false,
    }
}

#[tauri::command]
pub fn get_disk_info(state: State<'_, AppState>) -> AppResult<DiskInfo> {
    disk::disk_info(&state.home).map_err(|e| AppError::io(&state.home, &e))
}

// ---- Scanning ------------------------------------------------------------

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
            let classifier = Classifier::new(&opts.home);
            let recs =
                recommend::recommend(&outcome.summary, &outcome.index, &classifier, now_ms());
            let mut slot = state.slot();
            slot.last = Some(Arc::new(outcome));
            slot.recommendations = Arc::new(recs);
            slot.running = None;
            drop(slot);
            state.plans().clear();
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
pub async fn list_directory(
    state: State<'_, AppState>,
    path: Option<String>,
) -> AppResult<DirectoryListing> {
    let outcome = last_scan(&state)?;
    let home = state.home.clone();
    blocking(move || {
        let root = PathBuf::from(&outcome.summary.root);
        let path = match path {
            Some(raw) => path_in_scan(&raw, &outcome)?,
            None => root.clone(),
        };
        explorer::list_directory(&path, &root, &outcome.index, &Classifier::new(&home))
    })
    .await
}

#[tauri::command]
pub async fn get_item_details(state: State<'_, AppState>, path: String) -> AppResult<ItemDetails> {
    let outcome = last_scan(&state)?;
    let home = state.home.clone();
    blocking(move || {
        let path = path_in_scan(&path, &outcome)?;
        explorer::item_details(&path, &outcome.index, &Classifier::new(&home))
    })
    .await
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

// ---- Recommendations & cleanup -------------------------------------------

#[tauri::command]
pub fn get_recommendations(state: State<'_, AppState>) -> Vec<Recommendation> {
    state.slot().recommendations.as_ref().clone()
}

/// Build a previewable plan. Paths come from the engine's recommendations,
/// never from the UI. Nothing is changed.
#[tauri::command]
pub fn create_cleanup_plan(
    state: State<'_, AppState>,
    ids: Vec<String>,
    method: CleanupMethod,
) -> AppResult<CleanupPlan> {
    let outcome = last_scan(&state)?;
    let recs = state.slot().recommendations.clone();
    let now = now_ms();
    let plan = planner::build_plan(
        format!("cleanup-{now}"),
        &recs,
        &ids,
        method,
        Path::new(&outcome.summary.root),
        &state.home,
        now,
    );
    let mut plans = state.plans();
    plans.retain(|_, p| now - p.created_at_ms < planner::PLAN_TTL_MS);
    plans.insert(plan.id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command]
pub fn discard_cleanup_plan(state: State<'_, AppState>, plan_id: String) {
    state.plans().remove(&plan_id);
}

/// Execute a previously previewed plan, once. The user confirms in the UI;
/// the engine enforces that the plan exists, is fresh, and re-authorizes
/// every path immediately before acting on it.
#[tauri::command]
pub async fn execute_cleanup_plan(
    state: State<'_, AppState>,
    plan_id: String,
) -> AppResult<CleanupRecord> {
    let plan = state.plans().remove(&plan_id).ok_or_else(|| {
        AppError::Other("This preview has expired. Review the cleanup again.".into())
    })?;
    if now_ms() - plan.created_at_ms > planner::PLAN_TTL_MS {
        return Err(AppError::Other(
            "This preview has expired. Review the cleanup again.".into(),
        ));
    }
    if plan.items.is_empty() {
        return Err(AppError::Other("Nothing to clean up in this plan.".into()));
    }
    let outcome = last_scan(&state)?;
    let home = state.home.clone();
    let history = state.history.clone();
    let record = blocking(move || {
        let record = executor::execute(
            &plan,
            Path::new(&outcome.summary.root),
            &home,
            &executor::RealFileOps,
        );
        if let Some(h) = history {
            h.record_cleanup(&record)?;
        }
        Ok(record)
    })
    .await?;

    // Drop recommendations that were acted on.
    let done: std::collections::HashSet<&str> = record
        .items
        .iter()
        .filter(|i| i.status == ItemStatus::Succeeded)
        .map(|i| i.target.as_str())
        .collect();
    let mut slot = state.slot();
    let remaining: Vec<Recommendation> = slot
        .recommendations
        .iter()
        .filter(|r| !done.contains(r.path.as_str()))
        .cloned()
        .collect();
    slot.recommendations = Arc::new(remaining);
    Ok(record)
}

#[tauri::command]
pub fn get_cleanup_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<CleanupRecord>> {
    match &state.history {
        Some(h) => Ok(h.recent_cleanups(limit.unwrap_or(50).min(500))?),
        None => Ok(Vec::new()),
    }
}

// ---- Docker --------------------------------------------------------------

fn docker_error(e: docker::cli::DockerError) -> AppError {
    AppError::Other(e.to_string())
}

fn fetch_docker(home: &Path) -> AppResult<(Option<DockerCli>, DockerOverview)> {
    let cli = DockerCli::locate(home);
    let status = docker::status(cli.as_ref());
    let overview = match (&cli, status.state) {
        (Some(cli), DockerState::Running) => docker::overview(cli, status).map_err(docker_error)?,
        _ => DockerOverview {
            status,
            usage: Default::default(),
            images: vec![],
            containers: vec![],
            volumes: vec![],
            build_cache: vec![],
        },
    };
    Ok((cli, overview))
}

#[tauri::command]
pub async fn docker_overview(state: State<'_, AppState>) -> AppResult<DockerOverview> {
    let home = state.home.clone();
    blocking(move || fetch_docker(&home).map(|(_, o)| o)).await
}

#[tauri::command]
pub async fn docker_preview(
    state: State<'_, AppState>,
    action: actions::DockerAction,
) -> AppResult<actions::DockerActionPreview> {
    let home = state.home.clone();
    blocking(move || {
        let (_, o) = fetch_docker(&home)?;
        actions::preview(&action, &o).map_err(docker_error)
    })
    .await
}

/// Run a predefined Docker action after the user confirmed its preview.
/// Preconditions are re-checked against fresh Docker state.
#[tauri::command]
pub async fn docker_execute(
    state: State<'_, AppState>,
    action: actions::DockerAction,
) -> AppResult<actions::DockerActionResult> {
    let home = state.home.clone();
    let history = state.history.clone();
    blocking(move || {
        let (cli, fresh) = fetch_docker(&home)?;
        let cli = cli.ok_or_else(|| docker_error(docker::cli::DockerError::NotInstalled))?;
        let result = actions::execute(&cli, &action, &fresh).map_err(docker_error)?;
        if let (Some(h), Some(record)) = (history, &result.record) {
            h.record_cleanup(record)?;
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn docker_inspect(
    state: State<'_, AppState>,
    kind: String,
    id: String,
) -> AppResult<String> {
    let home = state.home.clone();
    blocking(move || {
        let cli = DockerCli::locate(&home)
            .ok_or_else(|| docker_error(docker::cli::DockerError::NotInstalled))?;
        actions::inspect(&cli, &kind, &id).map_err(docker_error)
    })
    .await
}

#[tauri::command]
pub async fn docker_diagnostics(
    state: State<'_, AppState>,
) -> AppResult<Vec<actions::DiagnosticFinding>> {
    let home = state.home.clone();
    blocking(move || {
        let (_, o) = fetch_docker(&home)?;
        if o.status.state != DockerState::Running {
            return Err(docker_error(docker::cli::DockerError::NotRunning));
        }
        let raw = home.join("Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw");
        let vm_disk = raw
            .symlink_metadata()
            .ok()
            .filter(|m| m.is_file())
            .map(|m| (scanner::allocated_bytes(&m), m.len()));
        Ok(actions::diagnostics(&o, vm_disk))
    })
    .await
}
