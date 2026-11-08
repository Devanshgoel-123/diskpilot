//! Emptying the Recycle Bin on Windows. Same flow and interface as the macOS
//! Trash module: [`inventory`] makes a single-use preview of what is in the
//! Recycle Bin on every drive, and [`empty`] permanently deletes only the
//! items named in it, one at a time, through the same Windows file operation
//! Explorer uses, so the Recycle Bin view stays in step.
//!
//! An item put back or deleted after the preview is skipped, and an item that
//! was recycled after the preview is never touched.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::model::{ActionStatus, CleanupItemResult, CleanupRecord, ItemStatus};
use crate::scanner::{measure, now_ms};

pub const EVENT_TRASH_PROGRESS: &str = "trash://progress";
/// A preview can be executed for this long after it was made.
pub const PREVIEW_TTL_MS: i64 = 15 * 60 * 1000;

/// Not a folder on Windows: the Recycle Bin spans every drive. Kept so the
/// commands read the same on both systems.
pub fn trash_dir(_home: &Path) -> PathBuf {
    PathBuf::from("Recycle Bin")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    /// Where the item was before it was recycled, shown to the user.
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub is_dir: bool,
    /// The item's path inside the Recycle Bin (`X:\$Recycle.Bin\<user>\$R…`).
    /// Emptying deletes exactly this item and nothing else.
    #[serde(skip)]
    pub id: OsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrashAccess {
    Granted,
    Denied,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashPreview {
    pub id: String,
    pub created_at_ms: i64,
    pub access: TrashAccess,
    /// Largest first.
    pub entries: Vec<TrashEntry>,
    pub total_bytes: u64,
    pub total_files: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashProgress {
    pub preview_id: String,
    pub entries_done: usize,
    pub entries_total: usize,
    pub bytes_removed: u64,
    pub files_removed: u64,
    pub current: Option<String>,
    pub not_removed: Vec<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashResult {
    pub record: CleanupRecord,
    pub files_removed: u64,
    pub cancelled: bool,
    pub permission_denied: bool,
}

#[derive(Default)]
pub struct TrashSlot {
    pub previews: HashMap<String, TrashPreview>,
    pub running: Option<Arc<AtomicBool>>,
}

fn other(e: trash::Error) -> io::Error {
    io::Error::other(e.to_string())
}

/// Files in an item and the bytes they occupy, measured in the Recycle Bin.
fn measure_item(path: &Path) -> (u64, u64, bool) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return (0, 0, false);
    };
    if !meta.is_dir() {
        return (measure(path), 1, false);
    }
    let mut files = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(_) => files += 1,
                Err(_) => {}
            }
        }
    }
    (measure(path), files, true)
}

/// List the Recycle Bin on every drive.
pub fn inventory(_trash: &Path, id: String, now: i64) -> io::Result<TrashPreview> {
    let items = trash::os_limited::list().map_err(other)?;
    let mut entries: Vec<TrashEntry> = items
        .into_iter()
        .map(|item| {
            let (bytes, files, is_dir) = measure_item(Path::new(&item.id));
            TrashEntry {
                name: item.original_path().display().to_string(),
                bytes,
                files,
                is_dir,
                id: item.id,
            }
        })
        .collect();
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    Ok(TrashPreview {
        id,
        created_at_ms: now,
        access: TrashAccess::Granted,
        total_bytes: entries.iter().map(|e| e.bytes).sum(),
        total_files: entries.iter().map(|e| e.files).sum(),
        entries,
    })
}

fn plural(n: u64, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn not_removed(results: &[CleanupItemResult]) -> Vec<usize> {
    results
        .iter()
        .enumerate()
        .filter(|(_, r)| r.status != ItemStatus::Succeeded)
        .map(|(i, _)| i)
        .collect()
}

/// Permanently delete the items named in `preview`.
pub fn empty(
    preview: &TrashPreview,
    _trash: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(TrashProgress),
) -> TrashResult {
    let started = now_ms();
    let total = preview.entries.len();
    let mut results: Vec<CleanupItemResult> = Vec::with_capacity(total);
    let (mut bytes_done, mut files_done) = (0u64, 0u64);
    let mut permission_denied = false;
    let mut cancelled = false;

    // What is in the Recycle Bin now, by item path.
    let current: HashMap<OsString, trash::TrashItem> = match trash::os_limited::list() {
        Ok(items) => items.into_iter().map(|i| (i.id.clone(), i)).collect(),
        Err(_) => HashMap::new(),
    };

    for (i, entry) in preview.entries.iter().enumerate() {
        let operation = format!("Delete permanently from the Recycle Bin: {}", entry.name);
        let skipped = |reason: &str| CleanupItemResult {
            target: entry.name.clone(),
            operation: operation.clone(),
            bytes: 0,
            status: ItemStatus::Skipped,
            error: Some(reason.to_string()),
        };
        if cancelled || cancel.load(Ordering::Relaxed) {
            cancelled = true;
            results.push(skipped(
                "Not removed: emptying was stopped. It is still in the Recycle Bin.",
            ));
            continue;
        }
        let Some(item) = current.get(&entry.id) else {
            results.push(skipped(
                "No longer in the Recycle Bin. It was restored or removed after the preview.",
            ));
            continue;
        };
        progress(TrashProgress {
            preview_id: preview.id.clone(),
            entries_done: i,
            entries_total: total,
            bytes_removed: bytes_done,
            files_removed: files_done,
            current: Some(entry.name.clone()),
            not_removed: not_removed(&results),
        });
        match trash::os_limited::purge_all([item]) {
            Ok(()) if !Path::new(&entry.id).exists() => {
                bytes_done += entry.bytes;
                files_done += entry.files;
                results.push(CleanupItemResult {
                    target: entry.name.clone(),
                    operation,
                    bytes: entry.bytes,
                    status: ItemStatus::Succeeded,
                    error: None,
                });
            }
            outcome => {
                let message = match outcome {
                    Err(e) => e.to_string(),
                    Ok(()) => "Windows did not delete it. A program may be using it.".into(),
                };
                permission_denied |= message.contains("denied");
                results.push(CleanupItemResult {
                    status: ItemStatus::Failed,
                    error: Some(message),
                    ..skipped("")
                });
            }
        }
    }

    progress(TrashProgress {
        preview_id: preview.id.clone(),
        entries_done: total,
        entries_total: total,
        bytes_removed: bytes_done,
        files_removed: files_done,
        current: None,
        not_removed: not_removed(&results),
    });

    TrashResult {
        record: CleanupRecord {
            id: format!("trash-{started}"),
            kind: "trash".into(),
            title: format!("Emptied the Recycle Bin ({})", plural(total as u64, "item")),
            started_at_ms: started,
            finished_at_ms: now_ms(),
            status: ActionStatus::from_items(&results),
            bytes_freed: bytes_done,
            bytes_trashed: 0,
            items: results,
        },
        files_removed: files_done,
        cancelled,
        permission_denied,
    }
}
