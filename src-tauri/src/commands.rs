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

use crate::adjust;
use crate::assistant::{
    self, keychain, provider::Provider, tools::ToolContext, AssistantReply, AssistantStatus,
    ModelInfo,
};
use crate::classifier::Classifier;
use crate::cleanup::{executor, planner};
use crate::devtools::{self, DevToolsReport};
use crate::disk;
use crate::docker::{self, actions, cli::DockerCli, DockerOverview, DockerState};
use crate::duplicates::{self, DuplicateReport};
use crate::error::{AppError, AppResult};
use crate::explorer;
use crate::model::{
    AppEnvironment, CleanupMethod, CleanupPlan, CleanupRecord, DirectoryListing, DiskInfo,
    ItemDetails, ItemStatus, Recommendation, ScanFinished, ScanRecord, ScanStatus, ScanSummary,
};
use crate::recommend;
use crate::safety::{self, SafetyError};
use crate::scanner::{self, now_ms, ScanError, ScanOptions, ScanOutcome};
use crate::snapshot;
use crate::state::{AppState, ScanSlot};
use crate::trash;

pub const EVENT_SCAN_PROGRESS: &str = "scan://progress";
pub const EVENT_SCAN_FINISHED: &str = "scan://finished";
/// Results for everything except app containers, which are still being
/// measured. Payload: the scan id. `scan://finished` follows.
pub const EVENT_SCAN_PARTIAL: &str = "scan://partial";
/// The last scan's numbers changed because DiskPilot removed something.
pub const EVENT_SCAN_UPDATED: &str = "scan://updated";

/// Run blocking work off the main thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Other(format!("Background task failed: {e}")))?
}

/// Installed-app names and icons for the given paths. Paths are only matched
/// against the index of installed apps, never read.
#[tauri::command]
pub async fn get_app_badges(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> AppResult<std::collections::HashMap<String, crate::apps::AppBadge>> {
    let home = state.home.clone();
    blocking(move || Ok(crate::apps::badges(&home, &paths))).await
}

/// Storage per installed application and data left by uninstalled ones.
#[tauri::command]
pub async fn get_app_storage(
    state: State<'_, AppState>,
) -> AppResult<crate::apps::AppStorageReport> {
    let home = state.home.clone();
    let outcome = state.slot().last.clone();
    blocking(move || {
        Ok(crate::apps::app_storage(
            &home,
            outcome.as_ref().map(|o| &o.index),
        ))
    })
    .await
}

#[tauri::command]
pub fn get_environment(state: State<'_, AppState>) -> AppEnvironment {
    AppEnvironment {
        home_dir: state.home.display().to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        read_only: false,
        update_blocked: std::env::current_exe()
            .ok()
            .and_then(|exe| update_blocked(&exe)),
    }
}

/// An update replaces the app where it is. From the disk image (read-only),
/// or from a copy macOS opened in a random read-only location because it was
/// never moved out of Downloads (App Translocation), that cannot work.
fn update_blocked(exe: &Path) -> Option<String> {
    let s = exe.to_string_lossy();
    let message = "To update, quit DiskPilot, drag it into Applications, and open it from there.";
    (s.contains("/AppTranslocation/") || s.starts_with("/Volumes/")).then(|| message.into())
}

// ---- Background schedule ----------------------------------------------------

#[tauri::command]
pub fn get_schedule(state: State<'_, AppState>) -> crate::schedule::Schedule {
    state
        .data_dir
        .as_deref()
        .map(crate::schedule::load)
        .unwrap_or_default()
}

/// Save the background settings. When the last reminder was shown is kept
/// from what is stored; the window cannot change it.
#[tauri::command]
pub fn set_schedule(
    state: State<'_, AppState>,
    schedule: crate::schedule::Schedule,
) -> AppResult<crate::schedule::Schedule> {
    let dir = state
        .data_dir
        .as_deref()
        .ok_or_else(|| AppError::Other("DiskPilot cannot save settings on this Mac.".into()))?;
    let saved = crate::schedule::Schedule {
        last_reminder_ms: crate::schedule::load(dir).last_reminder_ms,
        ..schedule
    };
    crate::schedule::save(dir, &saved).map_err(|e| AppError::io(dir, &e))?;
    Ok(saved)
}

/// Show the reminder now, so people can see what it looks like and check
/// that macOS lets DiskPilot send notifications.
#[tauri::command]
pub fn send_test_reminder(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let (title, body) = crate::schedule::current_reminder(&state, now_ms()).unwrap_or_else(|| {
        (
            "DiskPilot reminders are on".into(),
            "Once a week, DiskPilot will tell you when there is space worth clearing.".into(),
        )
    });
    if crate::schedule::notify(&app, &title, &body) {
        Ok(())
    } else {
        Err(AppError::Other(
            "macOS did not show the notification. Allow notifications for DiskPilot in System Settings > Notifications.".into(),
        ))
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
    let opts = ScanOptions::new(id.clone(), root, state.home.clone()).deferring_app_containers();
    std::thread::Builder::new()
        .name("diskpilot-scan".into())
        .spawn(move || run_scan(app, opts, cancel))
        .map_err(|e| AppError::Other(format!("Could not start scan thread: {e}")))?;
    Ok(id)
}

fn run_scan(app: AppHandle, opts: ScanOptions, cancel: Arc<AtomicBool>) {
    let started_at_ms = now_ms();
    let emitter = app.clone();
    let state = app.state::<AppState>();
    // Results on screen before this scan, restored if it is cancelled or
    // fails after publishing partial results.
    let previous = std::sync::Mutex::new(None);
    let result = scanner::scan_with_partial(
        &opts,
        &cancel,
        &move |p| {
            let _ = emitter.emit(EVENT_SCAN_PROGRESS, p);
        },
        &|partial| {
            let recs = scan_recommendations(&opts.home, &partial);
            let mut slot = state.slot();
            let before = (slot.last.take(), std::mem::take(&mut slot.recommendations));
            *previous.lock().unwrap_or_else(|e| e.into_inner()) = Some(before);
            slot.last = Some(Arc::new(partial));
            slot.recommendations = Arc::new(recs);
            drop(slot);
            state.plans().clear();
            let _ = app.emit(EVENT_SCAN_PARTIAL, &opts.id);
        },
    );
    let previous = previous.into_inner().unwrap_or_else(|e| e.into_inner());

    let mut saved: Option<Arc<ScanOutcome>> = None;
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
            let recs = scan_recommendations(&opts.home, &outcome);
            let outcome = Arc::new(outcome);
            let mut slot = state.slot();
            slot.last = Some(outcome.clone());
            slot.recommendations = Arc::new(recs);
            slot.running = None;
            drop(slot);
            // Plans made from partial results stay valid: each holds its own
            // paths and re-checks them before acting.
            if previous.is_none() {
                state.plans().clear();
            }
            saved = Some(outcome);
            (ScanStatus::Completed, None, record)
        }
        Err(err) => {
            let mut slot = state.slot();
            slot.running = None;
            if let Some((last, recommendations)) = previous {
                slot.last = last;
                slot.recommendations = recommendations;
                drop(slot);
                state.plans().clear();
            }
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
    // Save after telling the UI, so a large index doesn't delay the results.
    if let (Some(outcome), Some(dir)) = (saved, &state.data_dir) {
        if let Err(e) = snapshot::save(dir, &outcome) {
            eprintln!("diskpilot: could not save scan results: {e}");
        }
    }
}

/// Recommendations derived from a scan. Recomputed for a restored scan too,
/// so improvements to the rules apply to saved results.
fn scan_recommendations(home: &Path, outcome: &ScanOutcome) -> Vec<Recommendation> {
    let classifier = Classifier::new(home);
    let mut recs = recommend::recommend(&outcome.summary, &outcome.index, &classifier, now_ms());
    crate::apps::name_apps_in_titles(home, &mut recs);
    recs
}

/// A scan slot filled from a saved scan.
pub fn restored_slot(home: &Path, outcome: ScanOutcome) -> ScanSlot {
    ScanSlot {
        recommendations: Arc::new(scan_recommendations(home, &outcome)),
        last: Some(Arc::new(outcome)),
        ..Default::default()
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedScanInfo {
    /// Size of the saved results file, if one exists.
    pub file_bytes: Option<u64>,
}

#[tauri::command]
pub fn get_saved_scan_info(state: State<'_, AppState>) -> SavedScanInfo {
    SavedScanInfo {
        file_bytes: state.data_dir.as_deref().and_then(snapshot::file_size),
    }
}

/// Delete the saved results and forget the current ones. The next scan starts
/// from nothing.
#[tauri::command]
pub fn clear_saved_scan(state: State<'_, AppState>) -> AppResult<()> {
    let mut slot = state.slot();
    if slot.running.is_some() {
        return Err(AppError::ScanInProgress);
    }
    if let Some(dir) = &state.data_dir {
        snapshot::remove(dir).map_err(|e| AppError::io(dir, &e))?;
    }
    slot.last = None;
    slot.recommendations = Arc::new(Vec::new());
    drop(slot);
    state.plans().clear();
    Ok(())
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

/// Async so the wait for the previous launch's results happens off the main thread.
#[tauri::command(async)]
pub fn is_scan_running(state: State<'_, AppState>) -> bool {
    state.slot().running.is_some()
}

/// One type's totals in the summary sent to the window, without its files.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileKindTotals {
    pub kind: crate::filekinds::FileKind,
    pub bytes: u64,
    pub file_count: u64,
    /// How many files the scan listed for this type (`get_file_kind_files`).
    pub listed: usize,
}

/// The scan summary as the window receives it. The 2,000 largest files per
/// type were most of its size (about 1.2 MB of JSON for a home folder), so
/// each type carries its totals and the window asks for one type's files
/// when it shows them.
pub fn summary_for_window(summary: &ScanSummary) -> serde_json::Value {
    let totals: Vec<FileKindTotals> = summary
        .file_kinds
        .iter()
        .map(|g| FileKindTotals {
            kind: g.kind,
            bytes: g.bytes,
            file_count: g.file_count,
            listed: g.files.len(),
        })
        .collect();
    let rest = ScanSummary {
        file_kinds: Vec::new(),
        ..summary.clone()
    };
    let mut value = serde_json::to_value(rest).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "fileKinds".into(),
            serde_json::to_value(totals).unwrap_or_default(),
        );
    }
    value
}

/// Async so the wait for the previous launch's results happens off the main thread.
#[tauri::command(async)]
pub fn get_scan_summary(state: State<'_, AppState>) -> Option<serde_json::Value> {
    let last = state.slot().last.clone()?;
    Some(summary_for_window(&last.summary))
}

/// The files the last scan listed for one type, largest first.
#[tauri::command(async)]
pub fn get_file_kind_files(
    state: State<'_, AppState>,
    kind: crate::filekinds::FileKind,
) -> Vec<crate::model::FileEntry> {
    let Some(last) = state.slot().last.clone() else {
        return Vec::new();
    };
    last.summary
        .file_kinds
        .iter()
        .find(|g| g.kind == kind)
        .map(|g| g.files.clone())
        .unwrap_or_default()
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

/// Opens the app whose storage a Cleanup item points at, so people clear it
/// there. Only the apps listed in the cleanup policies can be opened.
#[tauri::command]
pub fn open_owner_app(rule_id: String) -> AppResult<()> {
    let app = crate::recommend::policies::owner_app(&rule_id)
        .ok_or_else(|| AppError::Other("DiskPilot does not open an app for this item.".into()))?;
    for id in app.bundle_ids {
        let opened = std::process::Command::new("/usr/bin/open")
            .args(["-b", id])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if opened {
            return Ok(());
        }
    }
    Err(AppError::Other(format!(
        "{} is not installed on this Mac, or macOS could not open it.",
        app.name
    )))
}

/// A Quick Look thumbnail (JPEG or PNG bytes) of a file DiskPilot listed, so people
/// can see a photo, video or document before deciding to remove it.
///
/// Quick Look renders text, PDFs and documents legibly, so this must not be
/// a way to read arbitrary files: only a path that Files by type or the
/// duplicate search listed is accepted, exactly as listed, and only while it
/// is still that same regular file with no symbolic link anywhere on its way.
#[tauri::command]
pub async fn file_thumbnail(
    state: State<'_, AppState>,
    path: String,
    size: f64,
) -> AppResult<tauri::ipc::Response> {
    ensure_listed(&state, &path)?;
    let cache = state.thumbs.clone();
    blocking(move || {
        let path = previewable_file(&path)?;
        if let Some(saved) = cache.as_ref().and_then(|c| c.get(&path, size)) {
            return Ok(tauri::ipc::Response::new(saved));
        }
        let made = crate::preview::thumbnail_png(&path, size)
            .ok_or_else(|| AppError::Other("macOS could not make a preview of this file".into()))?;
        if let Some(c) = &cache {
            c.put(&path, size, &made);
        }
        Ok(tauri::ipc::Response::new(made))
    })
    .await
}

/// Opens a file DiskPilot listed in its default app (Preview for photos,
/// QuickTime for videos), so people can look at it properly before deciding.
///
/// The same rules as `file_thumbnail` apply, and only types that open to be
/// looked at are accepted (see `opens_to_view`): never an app, script,
/// installer or archive.
#[tauri::command]
pub async fn open_listed_file(state: State<'_, AppState>, path: String) -> AppResult<()> {
    ensure_listed(&state, &path)?;
    blocking(move || {
        let path = previewable_file(&path)?;
        if !opens_to_view(&path) {
            return Err(AppError::Other(
                "DiskPilot opens photos, videos, audio and documents only. Use Show in Finder for this file.".into(),
            ));
        }
        let status = std::process::Command::new("/usr/bin/open")
            .arg("--")
            .arg(&path)
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| AppError::io(&path, &e))?;
        if status.success() {
            Ok(())
        } else {
            Err(AppError::Other(format!(
                "macOS has no app to open {}",
                path.display()
            )))
        }
    })
    .await
}

/// Only paths that Files by type or the duplicate search listed, exactly as
/// listed, may be previewed or opened.
fn ensure_listed(state: &AppState, path: &str) -> AppResult<()> {
    let listed = {
        let slot = state.slot();
        let dups = state.duplicates();
        let by_type = slot.last.iter().flat_map(|o| {
            o.summary
                .file_kinds
                .iter()
                .flat_map(|g| g.files.iter().map(|f| f.path.as_str()))
        });
        let duplicates = dups.report.iter().flat_map(|r| {
            r.groups
                .iter()
                .flat_map(|g| g.files.iter().map(|f| f.path.as_str()))
        });
        by_type.chain(duplicates).any(|p| p == path)
    };
    if listed {
        Ok(())
    } else {
        Err(AppError::Other(
            "Only files DiskPilot listed can be previewed".into(),
        ))
    }
}

/// Whether opening the file only shows it: photos, videos, audio and
/// documents. Archives (opening one unpacks it), installers and disk images
/// (they mount or run), apps and scripts are refused. The type comes from
/// the extension, which is also what macOS uses to choose the app.
fn opens_to_view(path: &Path) -> bool {
    use crate::filekinds::FileKind;
    path.file_name().and_then(FileKind::of).is_some_and(|k| {
        matches!(
            k,
            FileKind::Images | FileKind::Videos | FileKind::Audio | FileKind::Documents
        )
    })
}

/// The path, if it is still a regular file reached without any symbolic
/// link (a folder on the way swapped for a link would lead elsewhere).
fn previewable_file(raw: &str) -> AppResult<PathBuf> {
    let path = safety::parse_user_path(raw)?;
    let meta = path
        .symlink_metadata()
        .map_err(|_| SafetyError::Missing(path.display().to_string()))?;
    if !meta.is_file() {
        return Err(AppError::Other("Only files can be previewed".into()));
    }
    if path.canonicalize().ok().as_deref() != Some(path.as_path()) {
        return Err(AppError::Other(
            "This file moved since it was listed".into(),
        ));
    }
    Ok(path)
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

/// Async so the wait for the previous launch's results happens off the main thread.
#[tauri::command(async)]
pub fn get_recommendations(state: State<'_, AppState>) -> Vec<Recommendation> {
    state.slot().all_recommendations()
}

/// Cleanup is confined to the last scan's folder, or to the home folder when
/// only the developer-tools analysis has run.
fn cleanup_root(state: &AppState) -> PathBuf {
    state
        .slot()
        .last
        .as_ref()
        .map(|o| PathBuf::from(&o.summary.root))
        .unwrap_or_else(|| state.home.clone())
}

// ---- Developer tools -----------------------------------------------------

#[tauri::command]
pub async fn analyze_dev_tools(state: State<'_, AppState>) -> AppResult<DevToolsReport> {
    let outcome = state.slot().last.clone();
    let home = state.home.clone();
    let (report, recs) = blocking(move || {
        let ctx = devtools::Ctx {
            home: &home,
            index: outcome.as_ref().map(|o| &o.index),
            summary: outcome.as_ref().map(|o| &o.summary),
            classifier: Classifier::new(&home),
            now_ms: now_ms(),
        };
        Ok(devtools::analyze(&ctx))
    })
    .await?;
    let report = Arc::new(report);
    let mut slot = state.slot();
    slot.devtools = Some(report.clone());
    slot.devtools_recommendations = Arc::new(recs);
    Ok(report.as_ref().clone())
}

#[tauri::command]
pub fn get_dev_tools_report(state: State<'_, AppState>) -> Option<DevToolsReport> {
    state.slot().devtools.as_ref().map(|r| r.as_ref().clone())
}

/// Build a previewable plan. Paths come from the engine's recommendations,
/// never from the UI. Nothing is changed.
#[tauri::command]
pub async fn create_cleanup_plan(
    state: State<'_, AppState>,
    ids: Vec<String>,
    method: CleanupMethod,
) -> AppResult<CleanupPlan> {
    let (recs, scanned_at) = {
        let slot = state.slot();
        let at = slot.last.as_ref().map(|o| o.summary.finished_at_ms);
        (slot.all_recommendations(), at)
    };
    let root = cleanup_root(&state);
    let now = now_ms();
    let mut plan = planner::build_plan(
        unique_id("cleanup", now),
        &recs,
        &ids,
        method,
        &root,
        &state.home,
        now,
    );
    if scanned_at.is_none_or(|at| now - at > planner::REMEASURE_AFTER_MS) {
        plan = blocking(move || {
            planner::refresh_sizes(&mut plan, |p| {
                p.symlink_metadata().ok().map(|_| scanner::measure(p))
            });
            Ok(plan)
        })
        .await?;
    }
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
    app: AppHandle,
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
    let root = cleanup_root(&state);
    let home = state.home.clone();
    let history = state.history.clone();
    let planned: std::collections::HashMap<String, (u64, Option<crate::model::Category>)> = plan
        .items
        .iter()
        .map(|i| (i.path.clone(), (i.bytes, i.category)))
        .collect();
    let trashed = plan.method == CleanupMethod::Trash;
    let reporter = app.clone();
    let record = blocking(move || {
        let app = reporter;
        let report = |p: executor::CleanupProgress| {
            let _ = app.emit(executor::EVENT_CLEANUP_PROGRESS, p);
        };
        let record =
            executor::execute_with_progress(&plan, &root, &home, &executor::RealFileOps, &report);
        if let Some(h) = history {
            h.record_cleanup(&record)?;
        }
        Ok(record)
    })
    .await?;

    // Drop recommendations that were acted on.
    let done: std::collections::HashSet<String> = record
        .items
        .iter()
        .filter(|i| i.status == ItemStatus::Succeeded)
        .map(|i| i.target.clone())
        .collect();
    state.slot().remove_paths(&done);
    if let Some(report) = state.duplicates().report.as_mut() {
        duplicates::forget(report, &done, &state.home);
    }
    let removals = done
        .iter()
        .map(|p| {
            let (bytes, category) = planned.get(p).copied().unwrap_or((0, None));
            adjust::Removal {
                path: PathBuf::from(p),
                bytes,
                category,
                trashed,
            }
        })
        .collect();
    reflect_removals(app, removals).await;
    Ok(record)
}

/// Takes what DiskPilot just removed off the last scan's numbers, rebuilds
/// its recommendations, tells the window (scan://updated) and saves the
/// result, so nothing waits for the next scan. Does nothing if a newer scan
/// finished meanwhile, since that one already measured the change.
async fn reflect_removals(app: AppHandle, removals: Vec<adjust::Removal>) {
    if removals.is_empty() {
        return;
    }
    let _ = blocking(move || {
        let state = app.state::<AppState>();
        let Some(before) = state.slot().last.clone() else {
            return Ok(());
        };
        let outcome = adjust::apply(&before, &removals, &state.home);
        let recs = scan_recommendations(&state.home, &outcome);
        let outcome = Arc::new(outcome);
        {
            let mut slot = state.slot();
            if !slot.last.as_ref().is_some_and(|l| Arc::ptr_eq(l, &before)) {
                return Ok(());
            }
            slot.last = Some(outcome.clone());
            let gone: std::collections::HashSet<String> = removals
                .iter()
                .map(|r| r.path.display().to_string())
                .collect();
            slot.recommendations = Arc::new(recs);
            slot.remove_paths(&gone);
        }
        let _ = app.emit(EVENT_SCAN_UPDATED, ());
        if let Some(dir) = &state.data_dir {
            if let Err(e) = snapshot::save(dir, &outcome) {
                eprintln!("diskpilot: could not save updated scan results: {e}");
            }
        }
        Ok(())
    })
    .await;
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

/// Totals of every cleanup on this Mac, for the space-cleared stats. Local
/// only: nothing is sent anywhere.
#[tauri::command]
pub fn get_cleanup_activity(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::history::CleanupActivity>> {
    match &state.history {
        Some(h) => Ok(h.cleanup_activity()?),
        None => Ok(Vec::new()),
    }
}

// ---- Duplicates ----------------------------------------------------------

/// Search the scanned folder (or the home folder) for files with identical
/// contents. Read-only; reports progress on `duplicates://progress`.
#[tauri::command]
pub async fn find_duplicates(
    app: AppHandle,
    state: State<'_, AppState>,
    min_bytes: Option<u64>,
) -> AppResult<DuplicateReport> {
    let cancel = {
        let mut slot = state.duplicates();
        if slot.running.is_some() {
            return Err(AppError::Other(
                "A duplicate search is already running.".into(),
            ));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        slot.running = Some(cancel.clone());
        cancel
    };
    let root = cleanup_root(&state);
    let home = state.home.clone();
    let min = min_bytes.unwrap_or(duplicates::DEFAULT_MIN_BYTES);
    let outcome = blocking(move || {
        Ok(duplicates::find(&root, &home, min, &cancel, &mut |p| {
            let _ = app.emit(duplicates::EVENT_DUPLICATES_PROGRESS, p);
        }))
    })
    .await;
    let mut slot = state.duplicates();
    slot.running = None;
    if let Ok(report) = &outcome {
        slot.report = Some(report.clone());
    }
    outcome
}

#[tauri::command]
pub fn cancel_find_duplicates(state: State<'_, AppState>) -> bool {
    match &state.duplicates().running {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// The last duplicate search, if any, so the page survives navigation.
#[tauri::command]
pub fn get_duplicates(state: State<'_, AppState>) -> Option<DuplicateReport> {
    state.duplicates().report.clone()
}

/// A previewable plan to move the chosen copies to the Trash. Paths must come
/// from the last search; every group keeps a copy, and each chosen copy is
/// compared byte for byte with one that stays. Nothing is changed.
#[tauri::command]
pub async fn plan_duplicate_cleanup(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> AppResult<CleanupPlan> {
    let report = state
        .duplicates()
        .report
        .clone()
        .ok_or_else(|| AppError::Other("Search for duplicates first.".into()))?;
    let root = cleanup_root(&state);
    let home = state.home.clone();
    let now = now_ms();
    let plan = blocking(move || {
        let classifier = Classifier::new(&home);
        let classify = |p: &Path| classifier.classify_path(p, false, &crate::classifier::FsProbe);
        let (recs, rejected) = duplicates::removal_recommendations(&report, &paths, &classify);
        let kept = duplicates::kept_copies(&report, &paths);
        let ids: Vec<String> = recs.iter().map(|r| r.id.clone()).collect();
        let mut plan = planner::build_plan(
            unique_id("duplicates", now),
            &recs,
            &ids,
            CleanupMethod::Trash,
            &root,
            &home,
            now,
        );
        for item in &mut plan.items {
            item.keep_copy = kept.get(&item.path).cloned();
        }
        plan.rejected.extend(rejected);
        Ok(plan)
    })
    .await?;
    let mut plans = state.plans();
    plans.retain(|_, p| now - p.created_at_ms < planner::PLAN_TTL_MS);
    plans.insert(plan.id.clone(), plan.clone());
    Ok(plan)
}

/// A previewable plan to move files chosen in Files by type to the Trash.
/// Paths must be listed by the last scan and unchanged since. Nothing is
/// changed here.
#[tauri::command]
pub async fn plan_file_kind_cleanup(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> AppResult<CleanupPlan> {
    let outcome = last_scan(&state)?;
    let root = cleanup_root(&state);
    let home = state.home.clone();
    let now = now_ms();
    let plan = blocking(move || {
        let classifier = Classifier::new(&home);
        let classify = |p: &Path| classifier.classify_path(p, false, &crate::classifier::FsProbe);
        let (recs, rejected) = crate::filekinds::removal_recommendations(
            &outcome.summary.file_kinds,
            &paths,
            &classify,
        );
        let ids: Vec<String> = recs.iter().map(|r| r.id.clone()).collect();
        let mut plan = planner::build_plan(
            unique_id("file-kinds", now),
            &recs,
            &ids,
            CleanupMethod::Trash,
            &root,
            &home,
            now,
        );
        plan.rejected.extend(rejected);
        Ok(plan)
    })
    .await?;
    let mut plans = state.plans();
    plans.retain(|_, p| now - p.created_at_ms < planner::PLAN_TTL_MS);
    plans.insert(plan.id.clone(), plan.clone());
    Ok(plan)
}

// ---- Credentials in AI transcripts -----------------------------------------

/// Look for credentials pasted into AI agents' transcripts. Read-only and
/// offline; findings carry masked values only.
#[tauri::command]
pub async fn check_ai_transcripts(
    state: State<'_, AppState>,
) -> AppResult<crate::secrets::SecretsReport> {
    let home = state.home.clone();
    blocking(move || Ok(crate::secrets::check(&home, &AtomicBool::new(false)))).await
}

// ---- Trash ---------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashSummary {
    pub access: trash::TrashAccess,
    pub items: usize,
    pub total_bytes: u64,
    pub total_files: u64,
}

/// What is in the Trash right now. Read-only.
#[tauri::command]
pub async fn get_trash_summary(state: State<'_, AppState>) -> AppResult<TrashSummary> {
    let dir = trash::trash_dir(&state.home);
    let p = blocking(move || {
        trash::inventory(&dir, String::new(), now_ms()).map_err(|e| AppError::io(&dir, &e))
    })
    .await?;
    Ok(TrashSummary {
        access: p.access,
        items: p.entries.len(),
        total_bytes: p.total_bytes,
        total_files: p.total_files,
    })
}

/// List the Trash as a single-use preview. Only the entries in it can be
/// removed by [`empty_trash`]. Nothing is changed.
#[tauri::command]
pub async fn preview_trash(state: State<'_, AppState>) -> AppResult<trash::TrashPreview> {
    let dir = trash::trash_dir(&state.home);
    let now = now_ms();
    let preview = blocking(move || {
        trash::inventory(&dir, unique_id("trash", now), now).map_err(|e| AppError::io(&dir, &e))
    })
    .await?;
    let mut slot = state.trash();
    slot.previews
        .retain(|_, p| now - p.created_at_ms < trash::PREVIEW_TTL_MS);
    slot.previews.insert(preview.id.clone(), preview.clone());
    Ok(preview)
}

#[tauri::command]
pub fn discard_trash_preview(state: State<'_, AppState>, preview_id: String) {
    state.trash().previews.remove(&preview_id);
}

/// Permanently remove the previewed entries, once, after the user confirmed.
/// Progress is emitted as `trash://progress`; the result is recorded in history.
#[tauri::command]
pub async fn empty_trash(
    app: AppHandle,
    state: State<'_, AppState>,
    preview_id: String,
) -> AppResult<trash::TrashResult> {
    let expired = || AppError::Other("This preview has expired. Review the Trash again.".into());
    let (preview, cancel) = {
        let mut slot = state.trash();
        if slot.running.is_some() {
            return Err(AppError::Other(
                "The Trash is already being emptied.".into(),
            ));
        }
        let preview = slot.previews.remove(&preview_id).ok_or_else(expired)?;
        if now_ms() - preview.created_at_ms > trash::PREVIEW_TTL_MS {
            return Err(expired());
        }
        if preview.entries.is_empty() {
            return Err(AppError::Other("The Trash is already empty.".into()));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        slot.running = Some(cancel.clone());
        (preview, cancel)
    };
    let dir = trash::trash_dir(&state.home);
    let history = state.history.clone();
    let reporter = app.clone();
    let outcome = blocking(move || {
        let app = reporter;
        let result = trash::empty(&preview, &dir, &cancel, &mut |p| {
            let _ = app.emit(trash::EVENT_TRASH_PROGRESS, p);
        });
        if let Some(h) = history {
            if let Err(e) = h.record_cleanup(&result.record) {
                eprintln!("diskpilot: could not record emptying the Trash: {e}");
            }
        }
        Ok(result)
    })
    .await;
    state.trash().running = None;
    if let Ok(result) = &outcome {
        let removals = result
            .record
            .items
            .iter()
            .filter(|i| i.status == ItemStatus::Succeeded)
            .map(|i| adjust::Removal {
                path: PathBuf::from(&i.target),
                bytes: i.bytes,
                category: Some(crate::model::Category::UserData),
                trashed: false,
            })
            .collect();
        reflect_removals(app, removals).await;
    }
    outcome
}

/// Ask a running empty-Trash operation to stop after the current file.
#[tauri::command]
pub fn cancel_empty_trash(state: State<'_, AppState>) -> bool {
    match &state.trash().running {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Whether DiskPilot can read folders macOS protects (Mail, Messages, Photos).
#[tauri::command]
pub fn get_full_disk_access(state: State<'_, AppState>) -> crate::privacy::FullDiskAccess {
    crate::privacy::full_disk_access(&state.home)
}

/// Open System Settings at Privacy & Security > Full Disk Access.
#[tauri::command]
pub fn open_full_disk_access_settings() -> AppResult<()> {
    const PANE: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";
    std::process::Command::new("/usr/bin/open")
        .arg(PANE)
        .status()
        .map_err(|e| AppError::Other(format!("Could not open System Settings: {e}")))
        .map(|_| ())
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

/// How long a reviewed Docker action can wait before it must be reviewed again.
const DOCKER_REVIEW_TTL_MS: i64 = 10 * 60 * 1000;

/// The container an action stops or removes, if any.
fn target_container(action: &actions::DockerAction) -> Option<&str> {
    match action {
        actions::DockerAction::StopContainer { id }
        | actions::DockerAction::RemoveContainer { id, .. } => Some(id),
        _ => None,
    }
}

/// Fresh Docker state, with the target container's `--rm` flag read.
fn fetch_docker_for(
    home: &Path,
    action: &actions::DockerAction,
) -> AppResult<(Option<DockerCli>, DockerOverview)> {
    let (cli, mut o) = fetch_docker(home)?;
    if let (Some(cli), Some(id)) = (&cli, target_container(action)) {
        actions::validate_id(id).map_err(docker_error)?;
        docker::mark_auto_remove(cli, &mut o, id).map_err(docker_error)?;
    }
    Ok((cli, o))
}

#[tauri::command]
pub async fn docker_preview(
    state: State<'_, AppState>,
    action: actions::DockerAction,
) -> AppResult<actions::DockerActionPreview> {
    let home = state.home.clone();
    let mut preview = blocking(move || {
        let (_, o) = fetch_docker_for(&home, &action)?;
        actions::preview(&action, &o).map_err(docker_error)
    })
    .await?;
    let now = now_ms();
    preview.id = unique_id("docker", now);
    let mut reviews = state.docker_reviews();
    reviews.retain(|_, (_, at)| now - *at < DOCKER_REVIEW_TTL_MS);
    reviews.insert(preview.id.clone(), (actions::Reviewed::of(&preview), now));
    Ok(preview)
}

/// Run the Docker action the person reviewed (by preview id) and confirmed,
/// with the name they typed when a volume is deleted. Preconditions and the
/// exact commands are re-checked against fresh Docker state.
#[tauri::command]
pub async fn docker_execute(
    state: State<'_, AppState>,
    preview_id: String,
    typed: String,
) -> AppResult<actions::DockerActionResult> {
    let (reviewed, at) = state
        .docker_reviews()
        .remove(&preview_id)
        .ok_or_else(|| AppError::Other("This preview has expired. Review it again.".into()))?;
    if now_ms() - at > DOCKER_REVIEW_TTL_MS {
        return Err(AppError::Other(
            "This preview has expired. Review it again.".into(),
        ));
    }
    let home = state.home.clone();
    let history = state.history.clone();
    blocking(move || {
        let (cli, fresh) = fetch_docker_for(&home, &reviewed.action)?;
        let cli = cli.ok_or_else(|| docker_error(docker::cli::DockerError::NotInstalled))?;
        let result = actions::execute(&cli, &reviewed, &typed, &fresh).map_err(docker_error)?;
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

// ---- Assistant (optional) ------------------------------------------------

#[tauri::command]
pub fn assistant_status() -> AssistantStatus {
    assistant::status(keychain::load)
}

/// Store a provider's API key in the macOS Keychain. The key is never sent
/// back to the UI.
#[tauri::command]
pub fn assistant_set_key(provider: Provider, key: String) -> AppResult<AssistantStatus> {
    keychain::store(provider, &key).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(assistant_status())
}

#[tauri::command]
pub fn assistant_clear_key(
    state: State<'_, AppState>,
    provider: Provider,
) -> AppResult<AssistantStatus> {
    keychain::clear(provider).map_err(|e| AppError::Other(e.to_string()))?;
    *state.assistant.lock().unwrap_or_else(|e| e.into_inner()) = Default::default();
    Ok(assistant_status())
}

#[tauri::command]
pub fn assistant_reset(state: State<'_, AppState>) {
    *state.assistant.lock().unwrap_or_else(|e| e.into_inner()) = Default::default();
}

fn provider_key(provider: Provider) -> AppResult<String> {
    keychain::load(provider).map(|(key, _)| key).ok_or_else(|| {
        AppError::Other(assistant::AssistantError::NotConfigured(provider).to_string())
    })
}

/// Check the stored key and list the provider's models that can use
/// DiskPilot's tools. Spends no tokens.
#[tauri::command]
pub async fn assistant_models(provider: Provider) -> AppResult<Vec<ModelInfo>> {
    let api_key = provider_key(provider)?;
    tauri::async_runtime::spawn_blocking(move || {
        assistant::client::HttpTransport::new()
            .and_then(|t| assistant::list_models(&t, provider, &api_key))
    })
    .await
    .map_err(|e| AppError::Other(format!("Background task failed: {e}")))?
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Ask the assistant a question. It reads DiskPilot's data through read-only
/// tools; it cannot change anything.
#[tauri::command]
pub async fn assistant_ask(
    state: State<'_, AppState>,
    question: String,
    provider: Provider,
    model: String,
) -> AppResult<AssistantReply> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err(AppError::Other("Ask a question first.".into()));
    }
    let api_key = provider_key(provider)?;
    let ctx = {
        let slot = state.slot();
        ToolContext {
            home: state.home.clone(),
            scan: slot.last.clone(),
            recommendations: slot.all_recommendations(),
            devtools: slot.devtools.clone(),
            disk: disk::disk_info(&state.home).ok(),
        }
    };
    let mut convo = std::mem::take(&mut *state.assistant.lock().unwrap_or_else(|e| e.into_inner()));
    let (convo, result) = tauri::async_runtime::spawn_blocking(move || {
        let result = assistant::client::HttpTransport::new()
            .map_err(assistant::AssistantError::from)
            .and_then(|t| {
                assistant::ask(&t, provider, &api_key, &mut convo, &model, &question, &ctx)
            });
        (convo, result)
    })
    .await
    .map_err(|e| AppError::Other(format!("Background task failed: {e}")))?;
    *state.assistant.lock().unwrap_or_else(|e| e.into_inner()) = convo;
    result.map_err(|e| AppError::Other(e.to_string()))
}

/// An id for a plan or preview that cannot collide with another one made in
/// the same millisecond: the time plus 64 random bits.
fn unique_id(prefix: &str, now: i64) -> String {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    let random = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !random {
        // Never expected on macOS; fall back to a process-wide counter.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        bytes = NEXT
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .to_be_bytes();
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{prefix}-{now}-{hex}")
}

#[cfg(test)]
mod tests {
    use super::{opens_to_view, previewable_file, summary_for_window};

    #[test]
    fn the_window_gets_file_kind_totals_without_the_files() {
        let cls = serde_json::json!({ "category": "userData", "label": "x", "explanation": "x",
            "ruleId": "x", "confidence": "high", "matchedPath": "/Users/t" });
        let file = |p: &str, b: u64| {
            serde_json::json!({ "path": p, "name": p, "bytes": b,
            "logicalBytes": b, "modifiedMs": 1, "classification": cls })
        };
        let summary: crate::model::ScanSummary = serde_json::from_value(serde_json::json!({
            "id": "s", "root": "/Users/t", "startedAtMs": 0, "finishedAtMs": 1, "durationMs": 1,
            "totalBytes": 300, "fileCount": 9, "dirCount": 1, "symlinkCount": 0,
            "skippedVolumes": 0, "errorCount": 0, "errorSamples": [], "categories": [],
            "largestFiles": [file("/Users/t/a.mov", 200)], "detectedLocations": [],
            "heavyDirectories": [],
            "fileKinds": [{ "kind": "videos", "bytes": 300, "fileCount": 7,
                "files": [file("/Users/t/a.mov", 200), file("/Users/t/b.mov", 100)] }]
        }))
        .unwrap();
        let v = summary_for_window(&summary);
        let kinds = v["fileKinds"].as_array().unwrap();
        assert_eq!(kinds.len(), 1);
        assert_eq!(kinds[0]["kind"], "videos");
        assert_eq!(kinds[0]["bytes"], 300);
        assert_eq!(kinds[0]["fileCount"], 7);
        assert_eq!(kinds[0]["listed"], 2);
        assert!(kinds[0].get("files").is_none());
        // Everything else is as the scanner made it.
        assert_eq!(v["totalBytes"], 300);
        assert_eq!(v["largestFiles"][0]["path"], "/Users/t/a.mov");
        // The saved summary still has the files.
        assert_eq!(summary.file_kinds[0].files.len(), 2);
    }

    #[test]
    fn opens_photos_videos_and_documents_only() {
        let yes = [
            "beach.png",
            "clip.MOV",
            "notes.pdf",
            "song.m4a",
            "IMG_1.HEIC",
        ];
        let no = [
            "run.command",
            "Install.sh",
            "Tool",
            "App.app",
            "archive.zip",
            "Setup.dmg",
            "setup.pkg",
            "link.webloc",
            ".hidden.jpg",
        ];
        for n in yes {
            assert!(opens_to_view(std::path::Path::new(n)), "{n}");
        }
        for n in no {
            assert!(!opens_to_view(std::path::Path::new(n)), "{n}");
        }
    }

    #[test]
    fn previews_only_regular_files_reached_without_links() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let photos = home.join("Photos");
        std::fs::create_dir_all(&photos).unwrap();
        std::fs::write(photos.join("a.jpg"), "x").unwrap();
        std::fs::create_dir_all(home.join("elsewhere")).unwrap();
        std::fs::write(home.join("elsewhere/secret.txt"), "key").unwrap();
        std::os::unix::fs::symlink(home.join("elsewhere"), home.join("linked")).unwrap();
        std::os::unix::fs::symlink(home.join("elsewhere/secret.txt"), photos.join("b.jpg"))
            .unwrap();
        let s = |p: std::path::PathBuf| p.display().to_string();

        assert!(previewable_file(&s(photos.join("a.jpg"))).is_ok());
        // A folder on the way that is a link, the file itself a link, a folder.
        assert!(previewable_file(&s(home.join("linked/secret.txt"))).is_err());
        assert!(previewable_file(&s(photos.join("b.jpg"))).is_err());
        assert!(previewable_file(&s(photos.clone())).is_err());
        assert!(previewable_file(&s(photos.join("../elsewhere/secret.txt"))).is_err());
        assert!(previewable_file(&s(photos.join("missing.jpg"))).is_err());
    }

    #[test]
    fn updates_need_a_writable_app_location() {
        use super::update_blocked;
        use std::path::Path;
        let app = "/Applications/DiskPilot.app/Contents/MacOS/diskpilot";
        assert!(update_blocked(Path::new(app)).is_none());
        assert!(update_blocked(Path::new(
            "/Volumes/DiskPilot/DiskPilot.app/Contents/MacOS/diskpilot"
        ))
        .is_some());
        assert!(update_blocked(Path::new(
            "/private/var/folders/x/T/AppTranslocation/1A2B/d/DiskPilot.app/Contents/MacOS/diskpilot"
        ))
        .is_some());
    }
}
