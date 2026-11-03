//! Executes a confirmed cleanup plan.
//!
//! Each item is re-authorized immediately before it is touched. Permanent
//! deletion uses `std::fs::remove_dir_all`, which does not follow symbolic
//! links. Failures are recorded per item and never abort the remaining items.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

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
}

pub struct RealFileOps;

impl FileOps for RealFileOps {
    fn trash(&self, path: &Path) -> io::Result<()> {
        trash::delete(path).map_err(|e| io::Error::other(e.to_string()))
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
            if contains_other_volume(path, meta.dev())? {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "it contains another mounted volume, so DiskPilot will not delete it",
                ));
            }
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
    }
}

/// Whether any folder under `dir` is on a different device, such as a disk
/// image or network share mounted inside it. `remove_dir_all` would descend
/// into it and erase that volume's contents. Links are never followed.
fn contains_other_volume(dir: &Path, dev: u64) -> io::Result<bool> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d)? {
            let path = entry?.path();
            let meta = path.symlink_metadata()?;
            if meta.is_dir() {
                if meta.dev() != dev {
                    return Ok(true);
                }
                stack.push(path);
            }
        }
    }
    Ok(false)
}

pub fn execute(
    plan: &CleanupPlan,
    scan_root: &Path,
    home: &Path,
    ops: &dyn FileOps,
) -> CleanupRecord {
    let started = now_ms();
    let mut results = Vec::with_capacity(plan.items.len());

    for item in &plan.items {
        let operation = format!("{}: {}", plan.method.describe(), item.path);
        let authorized = safety::authorize_destructive(&item.path, scan_root, home)
            .and_then(|p| safety::ensure_within(&p, home).map(|_| p));
        let path = match authorized {
            Ok(p) => p,
            Err(e) => {
                results.push(CleanupItemResult {
                    target: item.path.clone(),
                    operation,
                    bytes: 0,
                    status: ItemStatus::Skipped,
                    error: Some(e.to_string()),
                });
                continue;
            }
        };
        // An extra copy of a duplicate goes only while the copy that stays
        // is still there with the same bytes.
        if let Some(keep) = &item.keep_copy {
            if !crate::duplicates::same_contents(&path, Path::new(keep)).unwrap_or(false) {
                results.push(CleanupItemResult {
                    target: item.path.clone(),
                    operation,
                    bytes: 0,
                    status: ItemStatus::Skipped,
                    error: Some(format!(
                        "The copy you are keeping ({keep}) has changed or is gone. Search for duplicates again."
                    )),
                });
                continue;
            }
        }
        let outcome = match plan.method {
            CleanupMethod::Trash => ops.trash(&path),
            CleanupMethod::Delete => ops.delete(&path),
        };
        results.push(match outcome {
            Ok(()) => CleanupItemResult {
                target: item.path.clone(),
                operation,
                bytes: item.bytes,
                status: ItemStatus::Succeeded,
                error: None,
            },
            Err(e) => CleanupItemResult {
                target: item.path.clone(),
                operation,
                bytes: 0,
                status: ItemStatus::Failed,
                error: Some(describe_io_error(&e)),
            },
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
mod tests {
    use super::*;
    use crate::model::{PlanItem, Tier};
    use std::sync::Mutex;

    /// Records trash calls instead of touching the real Trash.
    #[derive(Default)]
    struct FakeTrash(Mutex<Vec<std::path::PathBuf>>);

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
