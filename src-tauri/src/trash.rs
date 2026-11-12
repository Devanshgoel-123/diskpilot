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
/// Emitted while the Trash is being listed and measured, before the preview.
pub const EVENT_TRASH_LISTING: &str = "trash://listing";
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
    /// The entry's device and inode at preview time. Emptying skips an entry
    /// whose name now belongs to a different item, so only what was reviewed
    /// is removed. `None` only in tests.
    #[serde(skip)]
    pub identity: Option<(u64, u64)>,
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
    /// Indexes of finished entries that were not fully removed.
    pub not_removed: Vec<usize>,
}

/// How far listing the Trash has got: what has been measured so far.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashListing {
    pub preview_id: String,
    /// Top-level items measured completely.
    pub entries_done: usize,
    pub entries_total: usize,
    /// Size and files counted so far, including the item being measured.
    pub bytes: u64,
    pub files: u64,
    /// The item being measured now.
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

/// Size and file count of one entry, without following links or leaving its
/// volume. `tick` gets the running totals as files are counted.
fn measure_entry(path: &Path, meta: &Metadata, tick: &mut dyn FnMut(u64, u64)) -> (u64, u64) {
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
        tick(bytes, files);
    }
    (bytes, files)
}

/// List the Trash. A missing Trash folder is an empty Trash.
pub fn inventory(trash: &Path, id: String, now: i64) -> io::Result<TrashPreview> {
    inventory_with_progress(trash, id, now, &mut |_| {})
}

/// [`inventory`], reporting what has been measured so far as it goes (at most
/// every 40 ms, plus once at the start and once at the end).
pub fn inventory_with_progress(
    trash: &Path,
    id: String,
    now: i64,
    report: &mut dyn FnMut(&TrashListing),
) -> io::Result<TrashPreview> {
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
    let items: Vec<_> = rd
        .flatten()
        .filter(|e| e.file_name() != FINDER_METADATA)
        .collect();
    let mut listing = TrashListing {
        preview_id: preview.id.clone(),
        entries_done: 0,
        entries_total: items.len(),
        bytes: 0,
        files: 0,
        current: None,
    };
    report(&listing);
    let mut last = Instant::now();
    for entry in items {
        let name = entry.file_name();
        let Ok(meta) = entry.metadata() else {
            listing.entries_done += 1;
            continue;
        };
        listing.current = Some(name.to_string_lossy().into_owned());
        let (base_bytes, base_files) = (listing.bytes, listing.files);
        let (bytes, files) = measure_entry(&entry.path(), &meta, &mut |b, f| {
            if last.elapsed() >= PROGRESS_EVERY {
                last = Instant::now();
                listing.bytes = base_bytes + b;
                listing.files = base_files + f;
                report(&listing);
            }
        });
        listing.entries_done += 1;
        listing.bytes = base_bytes + bytes;
        listing.files = base_files + files;
        if last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            report(&listing);
        }
        preview.entries.push(TrashEntry {
            name: name.to_string_lossy().into_owned(),
            bytes,
            files,
            is_dir: meta.is_dir(),
            identity: Some((meta.dev(), meta.ino())),
        });
    }
    preview
        .entries
        .sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    preview.total_bytes = preview.entries.iter().map(|e| e.bytes).sum();
    preview.total_files = preview.entries.iter().map(|e| e.files).sum();
    listing.current = None;
    report(&listing);
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

/// Let the owner change a folder they own but made read-only, as Finder does
/// when it empties the Trash. Tools such as Go and DotSlash write their caches
/// read-only, and the user already chose to delete these items. Only the
/// owner's own permission bits change; locked (immutable) items and items
/// owned by someone else stay as they are and are reported. Returns whether
/// the mode changed, so it can be put back if the folder is not removed.
fn unlock_dir(path: &Path, meta: &Metadata) -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    let me = unsafe { libc::geteuid() };
    let mode = meta.mode();
    meta.uid() == me && mode & 0o700 != 0o700 && chmod_nofollow(path, (mode | 0o700) & 0o7777)
}

/// chmod that never follows a symbolic link: if the folder was swapped for a
/// link since it was checked, the link itself is left alone and nothing
/// outside the Trash changes.
fn chmod_nofollow(path: &Path, mode: u32) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let Ok(mode) = libc::mode_t::try_from(mode) else {
        return false;
    };
    // SAFETY: `c_path` is NUL-terminated and outlives the call.
    unsafe {
        libc::fchmodat(
            libc::AT_FDCWD,
            c_path.as_ptr(),
            mode,
            libc::AT_SYMLINK_NOFOLLOW,
        ) == 0
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
        let unlocked = unlock_dir(path, &meta);
        // Put the owner's original permissions back if this folder stays.
        let relock = || {
            if unlocked {
                chmod_nofollow(path, meta.mode() & 0o7777);
            }
        };
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
                        relock();
                        return false;
                    }
                }
            }
            Err(e) => {
                walk.fail(e);
                relock();
                return false;
            }
        }
        if !all {
            relock();
            return false;
        }
        match fs::remove_dir(path) {
            Ok(()) => {
                walk.bytes += allocated_bytes(&meta);
                true
            }
            Err(e) => {
                walk.fail(e);
                relock();
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
    let digits = n.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{grouped} {word}{}", if n == 1 { "" } else { "s" })
}

fn not_removed(results: &[CleanupItemResult]) -> Vec<usize> {
    results
        .iter()
        .enumerate()
        .filter(|(_, r)| r.status != ItemStatus::Succeeded)
        .map(|(i, _)| i)
        .collect()
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

    // The Trash folder itself, checked again before every entry so it cannot
    // be swapped for a link to another folder while emptying runs.
    let trash_id = |m: &Metadata| (m.dev(), m.ino());
    let trash_start = fs::symlink_metadata(trash)
        .ok()
        .filter(Metadata::is_dir)
        .map(|m| trash_id(&m));

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
        let trash_ok = trash_start.is_some()
            && fs::symlink_metadata(trash)
                .ok()
                .filter(Metadata::is_dir)
                .map(|m| trash_id(&m))
                == trash_start;
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
        if entry
            .identity
            .is_some_and(|id| id != (meta.dev(), meta.ino()))
        {
            results.push(skipped(
                "Not removed: this name now belongs to a different item than the one you reviewed.",
            ));
            continue;
        }

        let not_removed = not_removed(&results);
        progress(TrashProgress {
            preview_id: preview.id.clone(),
            entries_done: i,
            entries_total: total,
            bytes_removed: bytes_done,
            files_removed: files_done,
            current: Some(entry.name.clone()),
            not_removed: not_removed.clone(),
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
                    not_removed: not_removed.clone(),
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
        not_removed: not_removed(&results),
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
    fn listing_reports_what_was_measured_so_far() {
        let (_d, trash) = fixture();
        let mut seen: Vec<TrashListing> = Vec::new();
        let p =
            inventory_with_progress(&trash, "p".into(), 0, &mut |l| seen.push(l.clone())).unwrap();
        let first = seen.first().unwrap();
        assert_eq!(
            (first.entries_done, first.entries_total, first.files),
            (0, 2, 0)
        );
        let last = seen.last().unwrap();
        assert_eq!((last.entries_done, last.entries_total), (2, 2));
        assert_eq!((last.bytes, last.files), (p.total_bytes, p.total_files));
        assert_eq!(last.current, None);
        assert!(seen
            .windows(2)
            .all(|w| w[0].files <= w[1].files && w[0].bytes <= w[1].bytes));
        assert!(seen.iter().all(|l| l.preview_id == "p"));
    }

    #[test]
    fn counts_read_with_thousands_separators() {
        assert_eq!(plural(1, "file"), "1 file");
        assert_eq!(plural(999, "file"), "999 files");
        assert_eq!(plural(7282, "file"), "7,282 files");
        assert_eq!(plural(1_234_567, "item"), "1,234,567 items");
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
        let mut last = None;
        let r = empty(&preview, &trash, &AtomicBool::new(false), &mut |p| {
            last = Some(p)
        });
        assert_eq!(last.unwrap().not_removed, [0]);
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
                identity: None,
            });
        }
        let r = run(&preview, &trash, &AtomicBool::new(false));
        assert!(victim.exists());
        assert!(r.record.items[2..]
            .iter()
            .all(|i| i.status == ItemStatus::Skipped));
    }

    #[test]
    fn skips_a_different_item_that_took_a_reviewed_name() {
        let (_d, trash) = fixture();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        let name = preview.entries[0].name.clone();
        let path = trash.join(&name);
        // Put back the reviewed item, then trash a different one with the same name.
        if path.is_dir() {
            fs::remove_dir_all(&path).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        fs::write(&path, b"never reviewed").unwrap();
        let r = run(&preview, &trash, &AtomicBool::new(false));
        assert!(path.exists(), "an unreviewed item must stay");
        let item = r
            .record
            .items
            .iter()
            .find(|i| i.target.ends_with(&name))
            .unwrap();
        assert_eq!(item.status, ItemStatus::Skipped);
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

    fn set_locked(path: &Path, locked: bool) {
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path; chflags only reads it.
        let rc = unsafe { libc::chflags(c.as_ptr(), if locked { libc::UF_IMMUTABLE } else { 0 }) };
        assert_eq!(rc, 0);
    }

    #[test]
    fn removes_folders_the_owner_made_read_only() {
        // Go and DotSlash caches are full of read-only folders.
        let (_d, trash) = fixture();
        let src = trash.join("Old Project/src");
        fs::set_permissions(&src, fs::Permissions::from_mode(0o555)).unwrap();
        fs::set_permissions(trash.join("Old Project"), fs::Permissions::from_mode(0o500)).unwrap();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        let r = run(&preview, &trash, &AtomicBool::new(false));
        assert_eq!(r.record.status, ActionStatus::Succeeded);
        assert!(!trash.join("Old Project").exists());
    }

    #[test]
    fn a_folder_that_stays_gets_its_read_only_mode_back() {
        use std::os::unix::fs::MetadataExt;
        let (_d, trash) = fixture();
        let src = trash.join("Old Project/src");
        let locked = src.join("a.rs");
        set_locked(&locked, true);
        fs::set_permissions(&src, fs::Permissions::from_mode(0o555)).unwrap();
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        let r = run(&preview, &trash, &AtomicBool::new(false));
        let mode = fs::symlink_metadata(&src).unwrap().mode() & 0o777;
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();
        set_locked(&locked, false);

        assert_eq!(r.record.status, ActionStatus::Partial);
        assert!(locked.exists());
        assert_eq!(
            mode, 0o555,
            "the folder left in the Trash keeps its permissions"
        );
    }

    #[test]
    fn reports_files_it_cannot_remove_and_continues() {
        let (_d, trash) = fixture();
        let locked = trash.join("Old Project/src/a.rs");
        // A file the user locked in Finder (immutable) is left alone.
        set_locked(&locked, true);
        let preview = inventory(&trash, "p".into(), 0).unwrap();
        let r = run(&preview, &trash, &AtomicBool::new(false));
        set_locked(&locked, false);

        assert_eq!(r.record.status, ActionStatus::Partial);
        assert!(r.permission_denied);
        let project = &r.record.items[1];
        assert_eq!(project.status, ItemStatus::Failed);
        assert!(project
            .error
            .as_deref()
            .unwrap()
            .contains("could not be removed"));
        // b.txt was removed; the locked a.rs stayed behind.
        assert!(!trash.join("Old Project/b.txt").exists());
        assert!(locked.exists());
        assert!(project.bytes > 0 && project.bytes < 25_000);
        assert_eq!(
            r.record.bytes_freed,
            r.record.items[0].bytes + project.bytes
        );
    }
}
