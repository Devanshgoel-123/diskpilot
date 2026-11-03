//! Emptying the user's Trash.
//!
//! Flow: [`inventory`] lists what is in the Trash right now and becomes a
//! single-use preview the user reviews. [`empty`] then permanently removes
//! only the entries named in that preview. Anything moved to the Trash after
//! the preview is left alone, and an entry that is no longer there is skipped.
//!
//! Removal walks each entry itself, file by file, so it can report real
//! progress, stop between files when the user cancels, and record exactly what
//! could not be removed. Symbolic links are removed as links and never
//! followed, and the walk never crosses onto another volume.

use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::describe_io_error;
use crate::model::{ActionStatus, CleanupItemResult, CleanupRecord, ItemStatus};
use crate::scanner::{allocated_bytes, now_ms};

pub const EVENT_TRASH_PROGRESS: &str = "trash://progress";
/// A preview can be executed for this long after it was made.
pub const PREVIEW_TTL_MS: i64 = 15 * 60 * 1000;
/// Finder's own bookkeeping file. It is not shown and never counted.
const FINDER_METADATA: &str = ".DS_Store";
const PROGRESS_EVERY: Duration = Duration::from_millis(40);

/// The Trash for the home volume. Trash on external volumes is not covered.
pub fn trash_dir(home: &Path) -> PathBuf {
    home.join(".Trash")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    /// The entry's name inside the Trash (one path component).
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrashAccess {
    Granted,
    /// macOS privacy protection blocks reading the Trash until DiskPilot has
    /// Full Disk Access.
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
    /// The entry being removed now.
    pub current: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashResult {
    pub record: CleanupRecord,
    pub files_removed: u64,
    /// The user stopped the operation before every entry was handled.
    pub cancelled: bool,
    /// At least one failure was a permission error.
    pub permission_denied: bool,
}

/// Previews waiting to be executed and the cancellation flag of a running empty.
#[derive(Default)]
pub struct TrashSlot {
    pub previews: HashMap<String, TrashPreview>,
    pub running: Option<Arc<AtomicBool>>,
}

/// Bytes a file actually frees when removed: hard-linked files keep their
/// data alive through the other link, so they count as zero.
fn freed_bytes(meta: &Metadata) -> u64 {
    if !meta.is_dir() && meta.nlink() > 1 {
        0
    } else {
        allocated_bytes(meta)
    }
}

/// Size and file count of one entry, without following links or leaving its volume.
fn measure_entry(path: &Path, meta: &Metadata) -> (u64, u64) {
    if !meta.is_dir() {
        return (freed_bytes(meta), 1);
    }
    let dev = meta.dev();
    let (mut bytes, mut files) = (allocated_bytes(meta), 0);
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let Ok(m) = entry.metadata() else { continue };
            if m.is_dir() {
                if m.dev() == dev {
                    bytes += allocated_bytes(&m);
                    stack.push(entry.path());
                }
            } else {
                bytes += freed_bytes(&m);
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// List the Trash. A missing Trash folder is an empty Trash.
pub fn inventory(trash: &Path, id: String, now: i64) -> io::Result<TrashPreview> {
    let mut preview = TrashPreview {
        id,
        created_at_ms: now,
        access: TrashAccess::Granted,
        entries: Vec::new(),
        total_bytes: 0,
        total_files: 0,
    };
    match fs::symlink_metadata(trash) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(io::Error::other("The Trash is not a folder")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(preview),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            preview.access = TrashAccess::Denied;
            return Ok(preview);
        }
        Err(e) => return Err(e),
    }
    let rd = match fs::read_dir(trash) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            preview.access = TrashAccess::Denied;
            return Ok(preview);
        }
        Err(e) => return Err(e),
    };
    for entry in rd.flatten() {
        let name = entry.file_name();
        if name == FINDER_METADATA {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let (bytes, files) = measure_entry(&entry.path(), &meta);
        preview.entries.push(TrashEntry {
            name: name.to_string_lossy().into_owned(),
            bytes,
            files,
            is_dir: meta.is_dir(),
        });
    }
    preview
        .entries
        .sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    preview.total_bytes = preview.entries.iter().map(|e| e.bytes).sum();
    preview.total_files = preview.entries.iter().map(|e| e.files).sum();
    Ok(preview)
}

/// Resolve a previewed name to a path directly inside the Trash, or refuse.
fn entry_path(trash: &Path, name: &str) -> Option<PathBuf> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(_)), None) if name != FINDER_METADATA => Some(trash.join(name)),
        _ => None,
    }
}

#[derive(Default)]
struct Walk {
    bytes: u64,
    files: u64,
    failures: u64,
    first_error: Option<io::Error>,
    permission_denied: bool,
    cancelled: bool,
}

impl Walk {
    fn fail(&mut self, e: io::Error) {
        self.failures += 1;
        if e.kind() == io::ErrorKind::PermissionDenied {
            self.permission_denied = true;
        }
        if self.first_error.is_none() {
            self.first_error = Some(e);
        }
    }
}

/// Remove `path` and everything under it. Returns true when nothing is left.
fn remove_tree(
    path: &Path,
    dev: u64,
    cancel: &AtomicBool,
    walk: &mut Walk,
    tick: &mut dyn FnMut(&Walk),
) -> bool {
    if cancel.load(Ordering::Relaxed) {
        walk.cancelled = true;
        return false;
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return true,
        Err(e) => {
            walk.fail(e);
            return false;
        }
    };
    if meta.is_dir() {
        if meta.dev() != dev {
            walk.fail(io::Error::other(
                "Is on a different volume; DiskPilot does not follow it",
            ));
            return false;
        }
        let mut all = true;
        match fs::read_dir(path) {
            Ok(rd) => {
                for child in rd {
                    match child {
                        Ok(c) => all &= remove_tree(&c.path(), dev, cancel, walk, tick),
                        Err(e) => {
                            walk.fail(e);
                            all = false;
                        }
                    }
                    if walk.cancelled {
                        return false;
                    }
                }
            }
            Err(e) => {
                walk.fail(e);
                return false;
            }
        }
        if !all {
            return false;
        }
        match fs::remove_dir(path) {
            Ok(()) => {
                walk.bytes += allocated_bytes(&meta);
                true
            }
            Err(e) => {
                walk.fail(e);
                false
            }
        }
    } else {
        // Files and symbolic links alike: removing a link never touches its target.
        match fs::remove_file(path) {
            Ok(()) => {
                walk.bytes += freed_bytes(&meta);
                walk.files += 1;
                tick(walk);
                true
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => true,
            Err(e) => {
                walk.fail(e);
                false
            }
        }
    }
}

fn plural(n: u64, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// Permanently remove the entries named in `preview` from `trash`.
pub fn empty(
    preview: &TrashPreview,
    trash: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(TrashProgress),
) -> TrashResult {
    let started = now_ms();
    let total = preview.entries.len();
    let mut results = Vec::with_capacity(total);
    let (mut bytes_done, mut files_done) = (0u64, 0u64);
    let mut permission_denied = false;
    let mut cancelled = false;
    let mut last_tick = Instant::now() - PROGRESS_EVERY;

    let trash_ok = fs::symlink_metadata(trash)
        .map(|m| m.is_dir())
        .unwrap_or(false);

    for (i, entry) in preview.entries.iter().enumerate() {
        let operation = format!("Delete permanently from the Trash: {}", entry.name);
        let target = trash.join(&entry.name).display().to_string();
        let skipped = |reason: &str| CleanupItemResult {
            target: target.clone(),
            operation: operation.clone(),
            bytes: 0,
            status: ItemStatus::Skipped,
            error: Some(reason.to_string()),
        };
        if cancelled || cancel.load(Ordering::Relaxed) {
            cancelled = true;
            results.push(skipped(
                "Not removed: emptying was stopped. It is still in the Trash.",
            ));
            continue;
        }
        let Some(path) = entry_path(trash, &entry.name).filter(|_| trash_ok) else {
            results.push(skipped("Refused: not an item directly inside the Trash"));
            continue;
        };
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                results.push(skipped(
                    "No longer in the Trash. It was put back or removed after the preview.",
                ));
                continue;
            }
            Err(e) => {
                permission_denied |= e.kind() == io::ErrorKind::PermissionDenied;
                results.push(CleanupItemResult {
                    status: ItemStatus::Failed,
                    error: Some(describe_io_error(&e)),
                    ..skipped("")
                });
                continue;
            }
        };

        progress(TrashProgress {
            preview_id: preview.id.clone(),
            entries_done: i,
            entries_total: total,
            bytes_removed: bytes_done,
            files_removed: files_done,
            current: Some(entry.name.clone()),
        });
        let mut walk = Walk::default();
        let mut tick = |w: &Walk| {
            if last_tick.elapsed() >= PROGRESS_EVERY {
                last_tick = Instant::now();
                progress(TrashProgress {
                    preview_id: preview.id.clone(),
                    entries_done: i,
                    entries_total: total,
                    bytes_removed: bytes_done + w.bytes,
                    files_removed: files_done + w.files,
                    current: Some(entry.name.clone()),
                });
            }
        };
        let gone = remove_tree(&path, meta.dev(), cancel, &mut walk, &mut tick);
        bytes_done += walk.bytes;
        files_done += walk.files;
        permission_denied |= walk.permission_denied;
        cancelled |= walk.cancelled;

        results.push(if walk.cancelled && walk.files == 0 && walk.failures == 0 {
            skipped("Not removed: emptying was stopped. It is still in the Trash.")
        } else if gone {
            CleanupItemResult {
                target,
                operation,
                bytes: walk.bytes,
                status: ItemStatus::Succeeded,
                error: None,
            }
        } else {
            let reason = if walk.cancelled && walk.failures == 0 {
                "Emptying was stopped part way through this item".to_string()
            } else {
                let e = walk.first_error.as_ref().map(describe_io_error);
                format!(
                    "{} could not be removed{}",
                    plural(walk.failures, "file or folder"),
                    e.map(|e| format!(": {e}")).unwrap_or_default()
                )
            };
            let removed = if walk.files > 0 {
                format!(" {} were removed.", plural(walk.files, "file"))
            } else {
                String::new()
            };
            CleanupItemResult {
                target,
                operation,
                bytes: walk.bytes,
                status: ItemStatus::Failed,
                error: Some(format!("{reason}.{removed}")),
            }
        });
    }

    progress(TrashProgress {
        preview_id: preview.id.clone(),
        entries_done: total,
        entries_total: total,
        bytes_removed: bytes_done,
        files_removed: files_done,
        current: None,
    });

    TrashResult {
        record: CleanupRecord {
            id: format!("trash-{started}"),
            kind: "trash".into(),
            title: format!("Emptied the Trash ({})", plural(total as u64, "item")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = crate::test_support::sandbox();
        let trash = dir.path().canonicalize().unwrap().join(".Trash");
        fs::create_dir_all(trash.join("Old Project/src")).unwrap();
        fs::write(trash.join("Old Project/src/a.rs"), vec![1u8; 20_000]).unwrap();
        fs::write(trash.join("Old Project/b.txt"), vec![1u8; 5_000]).unwrap();
        fs::write(trash.join("Installer.dmg"), vec![1u8; 60_000]).unwrap();
        fs::write(trash.join(FINDER_METADATA), b"x").unwrap();
        (dir, trash)
    }

    fn run(preview: &TrashPreview, trash: &Path, cancel: &AtomicBool) -> TrashResult {
        empty(preview, trash, cancel, &mut |_| {})
    }

    #[test]
    fn lists_entries_largest_first_without_finder_metadata() {
        let (_d, trash) = fixture();
        let p = inventory(&trash, "p".into(), 0).unwrap();
        assert_eq!(p.access, TrashAccess::Granted);
        let names: Vec<_> = p.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Installer.dmg", "Old Project"]);
        assert_eq!(p.total_files, 3);
        assert!(p.total_bytes >= 85_000);
    }

    #[test]
    fn missing_trash_is_empty() {
        let dir = crate::test_support::sandbox();
        let p = inventory(&dir.path().join(".Trash"), "p".into(), 0).unwrap();
        assert!(p.entries.is_empty());
        assert_eq!(p.total_bytes, 0);
    }

    #[test]
    fn empties_only_previewed_entries() {
        let (_d, trash) = fixture();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        fs::write(trash.join("Added later.txt"), b"keep me").unwrap();
        let mut events = 0;
        let r = empty(&preview, &trash, &AtomicBool::new(false), &mut |_| {
            events += 1
        });
        assert_eq!(r.record.status, ActionStatus::Succeeded);
        assert_eq!(r.files_removed, 3);
        assert_eq!(r.record.bytes_freed, preview.total_bytes);
        assert!(events >= 2);
        assert!(!trash.join("Old Project").exists());
        assert!(!trash.join("Installer.dmg").exists());
        assert!(trash.join("Added later.txt").exists());
        assert!(trash.join(FINDER_METADATA).exists());
    }

    #[test]
    fn skips_entries_that_disappeared() {
        let (_d, trash) = fixture();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        fs::remove_file(trash.join("Installer.dmg")).unwrap();
        let r = run(&preview, &trash, &AtomicBool::new(false));
        assert_eq!(r.record.status, ActionStatus::Partial);
        assert_eq!(r.record.items[0].status, ItemStatus::Skipped);
        assert!(r.record.items[0]
            .error
            .as_deref()
            .unwrap()
            .contains("No longer in the Trash"));
        assert_eq!(r.record.items[1].status, ItemStatus::Succeeded);
    }

    #[test]
    fn never_follows_links_out_of_the_trash() {
        let (d, trash) = fixture();
        let outside = d.path().canonicalize().unwrap().join("Documents");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("thesis.pdf"), b"precious").unwrap();
        std::os::unix::fs::symlink(&outside, trash.join("Old Project/docs")).unwrap();
        std::os::unix::fs::symlink(&outside, trash.join("Documents alias")).unwrap();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        let r = run(&preview, &trash, &AtomicBool::new(false));
        assert_eq!(r.record.status, ActionStatus::Succeeded);
        assert!(!trash.join("Documents alias").exists());
        assert!(outside.join("thesis.pdf").exists());
    }

    #[test]
    fn refuses_names_that_leave_the_trash() {
        let (d, trash) = fixture();
        let victim = d.path().canonicalize().unwrap().join("victim.txt");
        fs::write(&victim, b"x").unwrap();
        let mut preview = inventory(&trash, "p".into(), 0).unwrap();
        for name in ["../victim.txt", "/etc/hosts", "Old Project/src", ""] {
            preview.entries.push(TrashEntry {
                name: name.into(),
                bytes: 1,
                files: 1,
                is_dir: false,
            });
        }
        let r = run(&preview, &trash, &AtomicBool::new(false));
        assert!(victim.exists());
        assert!(r.record.items[2..]
            .iter()
            .all(|i| i.status == ItemStatus::Skipped));
    }

    #[test]
    fn stops_when_cancelled_and_leaves_the_rest() {
        let (_d, trash) = fixture();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        let cancel = AtomicBool::new(false);
        let r = empty(&preview, &trash, &cancel, &mut |p| {
            if p.entries_done == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(r.cancelled);
        assert_eq!(r.record.status, ActionStatus::Partial);
        assert!(!trash.join("Installer.dmg").exists());
        assert!(trash.join("Old Project").exists());
    }

    #[test]
    fn reports_files_it_cannot_remove_and_continues() {
        let (_d, trash) = fixture();
        let locked = trash.join("Old Project/src");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        let r = run(&preview, &trash, &AtomicBool::new(false));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(r.record.status, ActionStatus::Partial);
        assert!(r.permission_denied);
        let project = &r.record.items[1];
        assert_eq!(project.status, ItemStatus::Failed);
        assert!(project
            .error
            .as_deref()
            .unwrap()
            .contains("could not be removed"));
        // b.txt was removed; a.rs stayed behind in the protected folder.
        assert!(!trash.join("Old Project/b.txt").exists());
        assert!(locked.join("a.rs").exists());
        assert!(project.bytes > 0 && project.bytes < 25_000);
        assert_eq!(
            r.record.bytes_freed,
            r.record.items[0].bytes + project.bytes
        );
    }
}
