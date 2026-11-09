//! Executes a confirmed cleanup plan.
//!
//! Items are handled in batches of [`BATCH`]: each batch is re-authorized
//! immediately before it is touched, then moved to the Trash with one Finder
//! request (one request per item took about a second each). Permanent
//! deletion uses `std::fs::remove_dir_all`, which does not follow symbolic
//! links. Failures are recorded per item and never abort the remaining items.

use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::describe_io_error;
use crate::model::{
    ActionStatus, CleanupItemResult, CleanupMethod, CleanupPlan, CleanupRecord, ItemStatus,
};
use crate::safety;
use crate::scanner::now_ms;

/// The two primitive operations. A trait so tests can observe calls and so no
/// other code needs to call filesystem removal functions directly.
pub trait FileOps: Sync {
    fn trash(&self, path: &Path) -> io::Result<()>;
    fn delete(&self, path: &Path) -> io::Result<()>;
    /// Moves several items to the Trash, with one result per path in order.
    fn trash_many(&self, paths: &[PathBuf]) -> Vec<io::Result<()>> {
        paths.iter().map(|p| self.trash(p)).collect()
    }
}

/// How many items are checked and moved to the Trash together.
pub const BATCH: usize = 100;

/// The Tauri event that reports a running cleanup.
pub const EVENT_CLEANUP_PROGRESS: &str = "cleanup://progress";

/// How far a cleanup has got, sent while it runs.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupProgress {
    pub plan_id: String,
    pub items_done: usize,
    pub items_total: usize,
    pub bytes_done: u64,
    /// The last item handled, by name.
    pub current: Option<String>,
}

pub struct RealFileOps;

/// Move `path` to the Trash. Finder does it first, so "Put Back" works. If
/// macOS has not let DiskPilot ask Finder (the user declined, or the app's
/// signature changed), the system file manager moves it instead: no
/// permission needed, the item still lands in the Trash, but Finder may not
/// offer "Put Back" for it.
fn move_to_trash(path: &Path) -> io::Result<()> {
    match trash::delete(path) {
        Ok(()) => Ok(()),
        Err(e) if finder_refused(&e.to_string()) => file_manager_trash(path),
        Err(e) => Err(io::Error::other(e.to_string())),
    }
}

/// Finder's "not authorized to send Apple events" (-1743), or Finder not
/// running (-600). AppleScript puts the code in brackets; matching that form
/// keeps a file name such as "IMG-6001.jpg" in some other error from
/// counting. A timeout (-1712) is not a refusal:
/// Finder may still finish moving the item, so it is reported, not retried.
fn finder_refused(message: &str) -> bool {
    message.contains("Not authorized to send Apple events")
        || message.contains("(-1743)")
        || message.contains("(-600)")
}

/// The system file manager's Trash, used when Finder may not be asked.
fn file_manager_trash(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut ctx = trash::TrashContext::default();
        ctx.set_delete_method(DeleteMethod::NsFileManager);
        ctx.delete(path)
            .map_err(|e| io::Error::other(e.to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    trash::delete(path).map_err(|e| io::Error::other(e.to_string()))
}

impl FileOps for RealFileOps {
    fn trash(&self, path: &Path) -> io::Result<()> {
        move_to_trash(path)
    }

    /// One Finder request for the whole batch. If Finder refuses, the system
    /// file manager moves each item. If the batch fails for another reason,
    /// Finder stops at the item it could not move, so each item still here
    /// is tried on its own and gets its own result; one the batch already
    /// moved is no longer where it was.
    fn trash_many(&self, paths: &[PathBuf]) -> Vec<io::Result<()>> {
        if paths.len() < 2 {
            return paths.iter().map(|p| self.trash(p)).collect();
        }
        match trash::delete_all(paths) {
            Ok(()) => paths.iter().map(|_| Ok(())).collect(),
            Err(e) if finder_refused(&e.to_string()) => {
                paths.iter().map(|p| file_manager_trash(p)).collect()
            }
            Err(_) => paths
                .iter()
                .map(|p| {
                    if p.symlink_metadata().is_err() {
                        Ok(())
                    } else {
                        move_to_trash(p)
                    }
                })
                .collect(),
        }
    }

    fn delete(&self, path: &Path) -> io::Result<()> {
        let meta = path.symlink_metadata()?;
        if meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to delete a symbolic link",
            ));
        }
        if meta.is_dir() {
            if let Some(reason) = removal_blocker(path, &meta)? {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, reason));
            }
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
    }
}

/// Why `remove_dir_all` must not run on `dir`, checked before anything is
/// removed, so a deletion either removes the whole tree or nothing. It
/// refuses when:
/// - a folder inside is on another device (a disk image or network share
///   mounted inside would have that volume's contents erased);
/// - this user cannot remove `dir` itself from its parent, or cannot remove
///   entries from a folder inside, such as a module in Go's read-only module
///   cache;
/// - an item is locked in Finder, append-only or protected by the system;
/// - an item carries an access control list, which can deny deletion in ways
///   the permission bits do not show;
/// - a sticky folder holds another user's items.
///
/// Links are never followed.
fn removal_blocker(dir: &Path, meta: &fs::Metadata) -> io::Result<Option<&'static str>> {
    const READ_ONLY: &str = "it contains read-only folders, so DiskPilot will not delete it";
    let parent_ok = dir.parent().is_some_and(writable);
    if !parent_ok {
        return Ok(Some(
            "the folder that holds it is read-only, so DiskPilot will not delete it",
        ));
    }
    if let Some(reason) = item_blocker(dir, meta) {
        return Ok(Some(reason));
    }
    let uid = unsafe { libc::geteuid() };
    let mut stack = vec![(dir.to_path_buf(), meta.mode())];
    while let Some((d, mode)) = stack.pop() {
        if !writable(&d) {
            return Ok(Some(READ_ONLY));
        }
        let sticky = mode & u32::from(libc::S_ISVTX) != 0;
        for entry in fs::read_dir(&d)? {
            let path = entry?.path();
            let m = path.symlink_metadata()?;
            if let Some(reason) = item_blocker(&path, &m) {
                return Ok(Some(reason));
            }
            if sticky && m.uid() != uid {
                return Ok(Some(
                    "it contains items owned by another user, so DiskPilot will not delete it",
                ));
            }
            if m.is_dir() {
                if m.dev() != meta.dev() {
                    return Ok(Some(
                        "it contains another mounted volume, so DiskPilot will not delete it",
                    ));
                }
                stack.push((path, m.mode()));
            }
        }
    }
    Ok(None)
}

/// Why this one item cannot be removed: flags or an access control list.
#[cfg(target_os = "macos")]
fn item_blocker(path: &Path, meta: &fs::Metadata) -> Option<&'static str> {
    use std::os::macos::fs::MetadataExt as _;
    let locked = libc::UF_IMMUTABLE | libc::UF_APPEND | libc::SF_IMMUTABLE | libc::SF_APPEND;
    let flags = meta.st_flags();
    if flags & locked != 0 {
        return Some("it contains locked items, so DiskPilot will not delete it");
    }
    // SF_RESTRICTED from <sys/stat.h>: protected by System Integrity Protection.
    const SF_RESTRICTED: u32 = 0x0008_0000;
    if flags & SF_RESTRICTED != 0 {
        return Some("it contains items macOS protects, so DiskPilot will not delete it");
    }
    if has_acl(path) {
        return Some("it contains items with access rules (ACLs), so DiskPilot will not delete it");
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn item_blocker(_path: &Path, _meta: &fs::Metadata) -> Option<&'static str> {
    None
}

/// Whether `path` itself (not a link's target) has an extended ACL.
#[cfg(target_os = "macos")]
fn has_acl(path: &Path) -> bool {
    extern "C" {
        fn acl_get_link_np(path: *const libc::c_char, ty: libc::c_int) -> *mut libc::c_void;
        fn acl_free(obj: *mut libc::c_void) -> libc::c_int;
    }
    const ACL_TYPE_EXTENDED: libc::c_int = 0x0000_0100;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return true;
    };
    // SAFETY: `c_path` is NUL-terminated and outlives the call; a non-null
    // result is an ACL we own and free once.
    unsafe {
        let acl = acl_get_link_np(c_path.as_ptr(), ACL_TYPE_EXTENDED);
        if acl.is_null() {
            return false;
        }
        acl_free(acl);
        true
    }
}

/// Whether this user may remove entries from `dir`, by the kernel's own
/// check of ownership and permission bits. ACLs are checked separately.
fn writable(dir: &Path) -> bool {
    let Ok(c_path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c_path` is a valid NUL-terminated string for the whole call.
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

/// The most often progress is reported while deleting one item at a time.
pub const PROGRESS_EVERY: Duration = Duration::from_millis(80);

/// The authorized path of an item, or why it must be left alone.
fn check(item: &crate::model::PlanItem, scan_root: &Path, home: &Path) -> Result<PathBuf, String> {
    let path = safety::authorize_destructive(&item.path, scan_root, home)
        .and_then(|p| safety::ensure_within(&p, home).map(|_| p))
        .map_err(|e| e.to_string())?;
    // A file that became a folder since the review is not what the person chose.
    if item.was_file && !path.symlink_metadata().is_ok_and(|m| m.is_file()) {
        return Err("It changed since you reviewed it. Scan again.".into());
    }
    // An extra copy of a duplicate goes only while the copy that stays is
    // still there with the same bytes.
    if let Some(keep) = &item.keep_copy {
        if !crate::duplicates::same_contents(&path, Path::new(keep)).unwrap_or(false) {
            return Err(format!(
                "The copy you are keeping ({keep}) has changed or is gone. Search for duplicates again."
            ));
        }
    }
    Ok(path)
}

fn pending(plan: &CleanupPlan, item: &crate::model::PlanItem) -> CleanupItemResult {
    CleanupItemResult {
        target: item.path.clone(),
        operation: format!("{}: {}", plan.method.describe(), item.path),
        bytes: 0,
        status: ItemStatus::Failed,
        error: None,
    }
}

fn skipped(plan: &CleanupPlan, item: &crate::model::PlanItem, reason: String) -> CleanupItemResult {
    CleanupItemResult {
        status: ItemStatus::Skipped,
        error: Some(reason),
        ..pending(plan, item)
    }
}

pub fn execute(
    plan: &CleanupPlan,
    scan_root: &Path,
    home: &Path,
    ops: &dyn FileOps,
) -> CleanupRecord {
    execute_with_progress(plan, scan_root, home, ops, &|_| {})
}

/// [`execute`], reporting progress after each batch (and at most every
/// [`PROGRESS_EVERY`] while deleting).
pub fn execute_with_progress(
    plan: &CleanupPlan,
    scan_root: &Path,
    home: &Path,
    ops: &dyn FileOps,
    progress: &dyn Fn(CleanupProgress),
) -> CleanupRecord {
    let started = now_ms();
    let total = plan.items.len();
    let mut results = Vec::with_capacity(total);
    let mut bytes_done = 0u64;
    let mut last_report = Instant::now();

    for batch in plan.items.chunks(BATCH) {
        // Check every item of the batch just before acting on it.
        let mut checked: Vec<(CleanupItemResult, Option<PathBuf>)> = batch
            .iter()
            .map(|item| match check(item, scan_root, home) {
                Ok(path) => (pending(plan, item), Some(path)),
                Err(reason) => (skipped(plan, item, reason), None),
            })
            .collect();

        let ready: Vec<PathBuf> = checked.iter().filter_map(|(_, p)| p.clone()).collect();
        let mut outcomes = match plan.method {
            CleanupMethod::Trash => ops.trash_many(&ready),
            CleanupMethod::Delete => ready
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let r = ops.delete(p);
                    if last_report.elapsed() >= PROGRESS_EVERY {
                        last_report = Instant::now();
                        progress(CleanupProgress {
                            plan_id: plan.id.clone(),
                            items_done: results.len() + i + 1,
                            items_total: total,
                            bytes_done,
                            current: p.file_name().map(|n| n.to_string_lossy().into_owned()),
                        });
                    }
                    r
                })
                .collect(),
        }
        .into_iter();

        for ((result, path), item) in checked.iter_mut().zip(batch) {
            if path.is_none() {
                continue;
            }
            match outcomes.next() {
                Some(Ok(())) => {
                    result.status = ItemStatus::Succeeded;
                    result.bytes = item.bytes;
                    bytes_done += item.bytes;
                }
                Some(Err(e)) => {
                    result.status = ItemStatus::Failed;
                    result.error = Some(describe_io_error(&e));
                }
                None => {
                    result.status = ItemStatus::Failed;
                    result.error = Some("DiskPilot lost track of this item.".into());
                }
            }
        }
        let current = batch
            .last()
            .and_then(|i| Path::new(&i.path).file_name())
            .map(|n| n.to_string_lossy().into_owned());
        results.extend(checked.into_iter().map(|(r, _)| r));
        last_report = Instant::now();
        progress(CleanupProgress {
            plan_id: plan.id.clone(),
            items_done: results.len(),
            items_total: total,
            bytes_done,
            current,
        });
    }

    let done: u64 = results
        .iter()
        .filter(|r| r.status == ItemStatus::Succeeded)
        .map(|r| r.bytes)
        .sum();
    let (bytes_freed, bytes_trashed) = match plan.method {
        CleanupMethod::Trash => (0, done),
        CleanupMethod::Delete => (done, 0),
    };
    CleanupRecord {
        id: plan.id.clone(),
        kind: "files".into(),
        title: format!(
            "{} {} item{}",
            plan.method.describe(),
            plan.items.len(),
            if plan.items.len() == 1 { "" } else { "s" }
        ),
        started_at_ms: started,
        finished_at_ms: now_ms(),
        status: ActionStatus::from_items(&results),
        bytes_freed,
        bytes_trashed,
        items: results,
    }
}

#[cfg(test)]
mod finder_tests {
    use super::finder_refused;

    #[test]
    fn recognises_finder_refusing() {
        assert!(finder_refused("Error during a `trash` operation: Os { code: 1, description: \"The AppleScript exited with error. stderr: 29:96: execution error: Not authorized to send Apple events to Finder. (-1743)\\n\" }"));
        assert!(!finder_refused("No such file or directory (os error 2)"));
        let wrap = |inner: &str| {
            format!("Error during a `trash` operation: Os {{ code: 1, description: \"The AppleScript exited with error. stderr: {inner}\\n\" }}")
        };
        assert!(finder_refused(&wrap(
            "execution error: Finder got an error: Application isn’t running. (-600)"
        )));
        // A file name that merely contains "-600", in an unrelated error.
        assert!(!finder_refused(&wrap(
            "execution error: Can’t get file \"Macintosh HD:Users:a:IMG-6001.jpg\". (-1728)"
        )));
        // Finder timed out: it may still move the item, so no second attempt.
        assert!(!finder_refused(&wrap(
            "execution error: Finder got an error: AppleEvent timed out. (-1712)"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlanItem, Tier};
    use std::sync::Mutex;

    /// Records trash calls instead of touching the real Trash.
    #[derive(Default)]
    struct FakeTrash(Mutex<Vec<std::path::PathBuf>>);

    /// Records how items were grouped, and fails the paths it is told to.
    #[derive(Default)]
    struct BatchTrash {
        batches: Mutex<Vec<usize>>,
        fail: Vec<std::path::PathBuf>,
    }

    impl FileOps for BatchTrash {
        fn trash(&self, _: &Path) -> io::Result<()> {
            unreachable!("batches only")
        }
        fn delete(&self, _: &Path) -> io::Result<()> {
            unreachable!("trash only")
        }
        fn trash_many(&self, paths: &[std::path::PathBuf]) -> Vec<io::Result<()>> {
            self.batches.lock().unwrap().push(paths.len());
            paths
                .iter()
                .map(|p| {
                    if self.fail.contains(p) {
                        Err(io::Error::other("locked"))
                    } else {
                        Ok(())
                    }
                })
                .collect()
        }
    }

    #[test]
    fn trashes_in_batches_and_reports_progress() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let files: Vec<_> = (0..(BATCH * 2 + 5))
            .map(|i| {
                let p = home.join(format!("f{i}.jpg"));
                fs::write(&p, "x").unwrap();
                p
            })
            .collect();
        let missing = home.join("gone.jpg");
        let mut paths: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        paths.insert(3, &missing);
        let ops = BatchTrash {
            fail: vec![files[7].clone()],
            ..Default::default()
        };
        let seen = Mutex::new(Vec::new());
        let record = execute_with_progress(
            &plan(CleanupMethod::Trash, &paths),
            &home,
            &home,
            &ops,
            &|p| seen.lock().unwrap().push(p),
        );

        // Three requests for 206 items, not 206; the missing one is never sent.
        assert_eq!(*ops.batches.lock().unwrap(), vec![BATCH - 1, BATCH, 6]);
        // Results keep the plan's order, each with its own outcome.
        assert_eq!(record.items.len(), paths.len());
        assert_eq!(record.items[3].status, ItemStatus::Skipped);
        assert_eq!(record.items[8].status, ItemStatus::Failed);
        assert_eq!(record.items[8].target, files[7].display().to_string());
        let ok = record
            .items
            .iter()
            .filter(|i| i.status == ItemStatus::Succeeded)
            .count();
        assert_eq!(ok, files.len() - 1);
        assert_eq!(record.bytes_trashed, 10 * ok as u64);
        assert_eq!(record.status, ActionStatus::Partial);

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3, "one report per batch");
        assert!(seen.windows(2).all(|w| w[0].items_done < w[1].items_done));
        let last = seen.last().unwrap();
        assert_eq!(
            (last.items_done, last.items_total),
            (paths.len(), paths.len())
        );
        assert_eq!(last.bytes_done, record.bytes_trashed);
    }

    impl FileOps for FakeTrash {
        fn trash(&self, path: &Path) -> io::Result<()> {
            self.0.lock().unwrap().push(path.to_path_buf());
            Ok(())
        }
        fn delete(&self, path: &Path) -> io::Result<()> {
            RealFileOps.delete(path)
        }
    }

    fn plan(method: CleanupMethod, paths: &[&Path]) -> CleanupPlan {
        CleanupPlan {
            id: "plan".into(),
            created_at_ms: 0,
            method,
            items: paths
                .iter()
                .map(|p| PlanItem {
                    recommendation_id: p.display().to_string(),
                    title: "t".into(),
                    path: p.display().to_string(),
                    bytes: 10,
                    file_count: 1,
                    tier: Tier::LikelySafe,
                    operation: method.describe().into(),
                    keep_copy: None,
                    was_file: false,
                })
                .collect(),
            rejected: vec![],
            total_bytes: 10 * paths.len() as u64,
            warnings: vec![],
        }
    }

    #[test]
    fn deletes_planned_paths_and_records_results() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let target = home.join("proj/node_modules");
        fs::create_dir_all(target.join("pkg")).unwrap();
        fs::write(target.join("pkg/index.js"), "x").unwrap();
        let missing = home.join("gone");

        let record = execute(
            &plan(CleanupMethod::Delete, &[&target, &missing]),
            &home,
            &home,
            &RealFileOps,
        );
        assert!(!target.exists());
        assert!(home.join("proj").exists(), "parent must survive");
        assert_eq!(record.status, ActionStatus::Partial);
        assert_eq!(record.bytes_freed, 10);
        assert_eq!(record.items[0].status, ItemStatus::Succeeded);
        assert_eq!(record.items[1].status, ItemStatus::Skipped);
        assert!(record.items[0]
            .operation
            .starts_with("Delete permanently: "));
    }

    #[test]
    fn an_extra_copy_stays_when_the_kept_copy_is_gone_or_changed() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let (a, b, c) = (home.join("a.bin"), home.join("b.bin"), home.join("c.bin"));
        for p in [&a, &b, &c] {
            fs::write(p, "same bytes").unwrap();
        }
        let mut p = plan(CleanupMethod::Trash, &[&a, &b]);
        p.items[0].keep_copy = Some(home.join("missing.bin").display().to_string());
        p.items[1].keep_copy = Some(c.display().to_string());
        fs::write(&c, "edited....").unwrap(); // same length, different bytes
        let fake = FakeTrash::default();
        let record = execute(&p, &home, &home, &fake);
        assert!(fake.0.lock().unwrap().is_empty(), "nothing may be trashed");
        assert!(record.items.iter().all(|i| i.status == ItemStatus::Skipped));

        // With the kept copy intact, the extra copy goes.
        fs::write(&c, "same bytes").unwrap();
        let p = {
            let mut p = plan(CleanupMethod::Trash, &[&b]);
            p.items[0].keep_copy = Some(c.display().to_string());
            p
        };
        execute(&p, &home, &home, &fake);
        assert_eq!(*fake.0.lock().unwrap(), vec![b.clone()]);
    }

    #[test]
    fn deleting_a_folder_never_follows_links_out_of_it() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let outside = home.join("precious");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        let target = home.join("cache");
        fs::create_dir_all(&target).unwrap();
        std::os::unix::fs::symlink(&outside, target.join("link")).unwrap();

        let record = execute(
            &plan(CleanupMethod::Delete, &[&target]),
            &home,
            &home,
            &RealFileOps,
        );
        assert_eq!(record.status, ActionStatus::Succeeded);
        assert!(!target.exists());
        assert!(
            outside.join("keep.txt").exists(),
            "symlink target must be untouched"
        );
    }

    #[test]
    fn deleting_a_tree_with_read_only_folders_touches_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        // Go's module cache: extracted modules are 0555 so builds cannot edit them.
        let target = home.join("go/pkg/mod");
        let module = target.join("golang.org/x/text@v0.1.0");
        fs::create_dir_all(module.join("language")).unwrap();
        fs::write(module.join("language/tags.go"), "package language").unwrap();
        fs::write(target.join("a-writable-file"), "x").unwrap();
        let set_mode = |mode| {
            for d in [module.join("language"), module.clone()] {
                fs::set_permissions(d, fs::Permissions::from_mode(mode)).unwrap();
            }
        };
        set_mode(0o555);

        let record = execute(
            &plan(CleanupMethod::Delete, &[&target]),
            &home,
            &home,
            &RealFileOps,
        );
        let left_alone =
            target.join("a-writable-file").exists() && module.join("language/tags.go").exists();
        set_mode(0o755);
        assert!(
            left_alone,
            "a failed delete must not remove part of the tree"
        );
        assert_eq!(record.status, ActionStatus::Failed);
        assert!(record.items[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("read-only")));
    }

    /// Each way a tree can stop `remove_dir_all` partway. For every one,
    /// Delete must refuse and leave the whole tree in place.
    #[test]
    #[cfg(target_os = "macos")]
    fn deleting_refuses_trees_that_could_only_be_half_removed() {
        use std::ffi::CString;
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        fn chflags(p: &Path, flags: u32) {
            let c = CString::new(p.as_os_str().as_bytes()).unwrap();
            assert_eq!(
                unsafe { libc::chflags(c.as_ptr(), flags) },
                0,
                "chflags {p:?}"
            );
        }
        fn acl(p: &Path, rule: &str) {
            assert!(Command::new("chmod")
                .args(["+a", rule])
                .arg(p)
                .status()
                .unwrap()
                .success());
        }
        fn no_acl(p: &Path) {
            let _ = Command::new("chmod").arg("-N").arg(p).status();
        }

        type Setup = fn(&Path) -> Box<dyn FnOnce()>;
        let cases: [(&str, Setup, &str); 5] = [
            (
                "a locked file",
                |t| {
                    let f = t.join("sub/f.txt");
                    chflags(&f, libc::UF_IMMUTABLE);
                    Box::new(move || chflags(&f, 0))
                },
                "locked",
            ),
            (
                "an append-only file",
                |t| {
                    let f = t.join("sub/f.txt");
                    chflags(&f, libc::UF_APPEND);
                    Box::new(move || chflags(&f, 0))
                },
                "locked",
            ),
            (
                "a file whose ACL denies deletion",
                |t| {
                    let f = t.join("sub/f.txt");
                    acl(&f, "everyone deny delete");
                    Box::new(move || no_acl(&f))
                },
                "access rules",
            ),
            (
                "a folder whose ACL denies deleting its items",
                |t| {
                    let d = t.join("sub");
                    acl(&d, "everyone deny delete_child");
                    Box::new(move || no_acl(&d))
                },
                "access rules",
            ),
            (
                "a read-only parent folder",
                |t| {
                    let parent = t.parent().unwrap().to_path_buf();
                    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();
                    Box::new(move || {
                        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap()
                    })
                },
                "read-only",
            ),
        ];

        for (what, setup, reason) in cases {
            let dir = crate::test_support::sandbox();
            let home = dir.path().canonicalize().unwrap();
            let target = home.join("holder/cache");
            fs::create_dir_all(target.join("sub")).unwrap();
            for f in ["a_sibling.txt", "z_sibling.txt", "sub/f.txt"] {
                fs::write(target.join(f), "x").unwrap();
            }
            let undo = setup(&target);
            let record = execute(
                &plan(CleanupMethod::Delete, &[&target]),
                &home,
                &home,
                &RealFileOps,
            );
            let intact = ["a_sibling.txt", "z_sibling.txt", "sub/f.txt"]
                .iter()
                .all(|f| target.join(f).exists());
            undo();
            assert!(intact, "{what}: a refused delete must leave every file");
            assert_eq!(record.status, ActionStatus::Failed, "{what}");
            let error = record.items[0].error.clone().unwrap_or_default();
            assert!(error.contains(reason), "{what}: {error}");
        }
    }

    #[test]
    fn a_file_that_became_a_folder_since_the_review_is_skipped() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let video = home.join("Movies/trip.mp4");
        fs::create_dir_all(&video).unwrap(); // was a file when planned
        fs::write(video.join("inside.txt"), "x").unwrap();
        let mut p = plan(CleanupMethod::Trash, &[&video]);
        p.items[0].was_file = true;
        let ops = FakeTrash::default();
        let record = execute(&p, &home, &home, &ops);
        assert!(ops.0.lock().unwrap().is_empty());
        assert_eq!(record.items[0].status, ItemStatus::Skipped);
    }

    #[test]
    fn re_checks_safety_at_execution_time() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let real = home.join("real");
        fs::create_dir_all(real.join("cache")).unwrap();
        // The folder was swapped for a symlink after planning.
        fs::create_dir_all(home.join("swapped_src/cache")).unwrap();
        std::os::unix::fs::symlink(home.join("swapped_src"), home.join("swapped")).unwrap();

        let ops = FakeTrash::default();
        let record = execute(
            &plan(
                CleanupMethod::Trash,
                &[&real.join("cache"), &home.join("swapped/cache")],
            ),
            &home,
            &home,
            &ops,
        );
        assert_eq!(ops.0.lock().unwrap().len(), 1);
        assert_eq!(record.items[1].status, ItemStatus::Skipped);
        assert_eq!(record.bytes_trashed, 10);
        assert_eq!(record.bytes_freed, 0);
    }

    #[test]
    fn refuses_protected_paths_even_if_planned() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        fs::create_dir_all(home.join("Library/Caches")).unwrap();
        let ops = FakeTrash::default();
        let record = execute(
            &plan(CleanupMethod::Trash, &[&home.join("Library/Caches"), &home]),
            &home,
            &home,
            &ops,
        );
        assert!(ops.0.lock().unwrap().is_empty());
        assert_eq!(record.status, ActionStatus::Failed);
    }
}
