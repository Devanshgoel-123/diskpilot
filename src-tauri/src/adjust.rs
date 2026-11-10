//! Keeps the last scan's numbers true after DiskPilot removes something, so
//! totals, categories, folder sizes and lists change at once instead of at
//! the next scan.
//!
//! Only what DiskPilot itself removed is subtracted, by the sizes it
//! measured. Moving an item to the Trash moves its bytes into `~/.Trash`
//! (still on disk, now in the Trash's category); deleting it, or emptying
//! the Trash, takes them off the totals. Items moved to the Trash leave the
//! lists (largest files, Files by type) rather than reappearing under
//! `~/.Trash`. Anything else that changed on disk still needs a scan.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::model::Category;
use crate::scanner::{DirIndex, ScanOutcome};

/// One item DiskPilot removed.
#[derive(Debug, Clone)]
pub struct Removal {
    pub path: PathBuf,
    /// Allocated bytes, as planned or as actually removed.
    pub bytes: u64,
    /// The category the scan counted these bytes under, if known.
    pub category: Option<Category>,
    /// Moved to `~/.Trash` rather than deleted.
    pub trashed: bool,
}

/// `outcome` with `removals` taken out. The index is copied once per call,
/// so pass every item of a cleanup together.
pub fn apply(outcome: &ScanOutcome, removals: &[Removal], home: &Path) -> ScanOutcome {
    let mut summary = outcome.summary.clone();
    let mut index: DirIndex = outcome.index.clone();
    let root = PathBuf::from(&summary.root);
    let trash = home.join(".Trash");
    // On Windows the Recycle Bin is a hidden folder at the top of each drive,
    // outside the user folder, so a recycled item leaves the totals like a
    // deleted one. (A whole-drive scan sees it there again at the next scan.)
    let trash_counted = cfg!(not(windows)) && trash.starts_with(&root) && trash != root;

    let mut gone: HashSet<PathBuf> = HashSet::new();
    for r in removals {
        if !r.path.starts_with(&root) || r.path == root || gone.contains(&r.path) {
            continue;
        }
        // Already inside something removed in this same cleanup.
        if r.path.ancestors().skip(1).any(|a| gone.contains(a)) {
            continue;
        }
        // A folder's own measurement is more exact than the plan's.
        let stats = index.get(&r.path).copied();
        let (bytes, files, dirs) = match stats {
            // `dirs` counts the folder itself.
            Some(s) => (s.bytes, s.files, s.dirs),
            None => (r.bytes, 1, 0),
        };
        let is_file = stats.is_none();
        let into_trash = r.trashed && trash_counted && !r.path.starts_with(&trash);

        subtract_from_ancestors(&mut index, &r.path, &root, bytes, files, dirs, is_file);
        if into_trash {
            add_to_ancestors(&mut index, &trash, &root, bytes, files, dirs, is_file);
        }

        let from = r.category.unwrap_or(Category::Unknown);
        let to = into_trash.then_some(Category::UserData);
        move_category(&mut summary.categories, from, to, bytes, files);
        if !into_trash {
            summary.total_bytes = summary.total_bytes.saturating_sub(bytes);
            summary.file_count = summary.file_count.saturating_sub(files);
            summary.dir_count = summary.dir_count.saturating_sub(dirs);
        }

        for loc in &mut summary.detected_locations {
            let p = Path::new(&loc.path);
            if r.path.starts_with(p) && p != r.path {
                loc.bytes = loc.bytes.saturating_sub(bytes);
                loc.file_count = loc.file_count.saturating_sub(files);
            }
            if into_trash && trash.starts_with(p) {
                loc.bytes += bytes;
                loc.file_count += files;
            }
        }
        if is_file {
            if let Some(parent) = r.path.parent() {
                for d in &mut summary.heavy_directories {
                    if Path::new(&d.path) == parent {
                        d.bytes = d.bytes.saturating_sub(bytes);
                        d.file_count = d.file_count.saturating_sub(1);
                    }
                }
            }
        }
        for group in &mut summary.file_kinds {
            if let Some(i) = group
                .files
                .iter()
                .position(|f| Path::new(&f.path) == r.path)
            {
                let f = group.files.remove(i);
                group.bytes = group.bytes.saturating_sub(f.bytes);
                group.file_count = group.file_count.saturating_sub(1);
            }
        }
        gone.insert(r.path.clone());
    }
    if gone.is_empty() {
        return ScanOutcome { summary, index };
    }

    let under_gone = |p: &Path| p.ancestors().any(|a| gone.contains(a));
    index.retain(|p, _| !under_gone(p));
    summary
        .largest_files
        .retain(|f| !under_gone(Path::new(&f.path)));
    summary
        .heavy_directories
        .retain(|d| !under_gone(Path::new(&d.path)));
    summary
        .detected_locations
        .retain(|l| !under_gone(Path::new(&l.path)));
    for group in &mut summary.file_kinds {
        // Listed files inside a removed folder go too; files too small to be
        // listed stay in the type's total, which only a scan can correct.
        let before: u64 = group.files.iter().map(|f| f.bytes).sum();
        let count = group.files.len();
        group.files.retain(|f| !under_gone(Path::new(&f.path)));
        let after: u64 = group.files.iter().map(|f| f.bytes).sum();
        group.bytes = group.bytes.saturating_sub(before - after);
        group.file_count = group
            .file_count
            .saturating_sub((count - group.files.len()) as u64);
    }
    summary.file_kinds.retain(|g| g.file_count > 0);
    summary.categories.retain(|c| c.bytes > 0);
    summary
        .categories
        .sort_by_key(|c| std::cmp::Reverse(c.bytes));
    summary
        .detected_locations
        .sort_by_key(|l| std::cmp::Reverse(l.bytes));
    ScanOutcome { summary, index }
}

fn subtract_from_ancestors(
    index: &mut DirIndex,
    path: &Path,
    root: &Path,
    bytes: u64,
    files: u64,
    dirs: u64,
    is_file: bool,
) {
    for (depth, a) in path.ancestors().skip(1).enumerate() {
        if let Some(s) = index.get_mut(a) {
            s.bytes = s.bytes.saturating_sub(bytes);
            s.files = s.files.saturating_sub(files);
            s.dirs = s.dirs.saturating_sub(dirs);
            if depth == 0 && is_file {
                s.direct_bytes = s.direct_bytes.saturating_sub(bytes);
            }
        }
        if a == root {
            break;
        }
    }
}

fn add_to_ancestors(
    index: &mut DirIndex,
    trash: &Path,
    root: &Path,
    bytes: u64,
    files: u64,
    dirs: u64,
    is_file: bool,
) {
    for (depth, a) in trash.ancestors().enumerate() {
        if let Some(s) = index.get_mut(a) {
            s.bytes += bytes;
            s.files += files;
            s.dirs += dirs;
            if depth == 0 && is_file {
                s.direct_bytes += bytes;
            }
        }
        if a == root {
            break;
        }
    }
}

fn move_category(
    totals: &mut Vec<crate::model::CategoryTotal>,
    from: Category,
    to: Option<Category>,
    bytes: u64,
    files: u64,
) {
    if let Some(t) = totals.iter_mut().find(|c| c.category == from) {
        t.bytes = t.bytes.saturating_sub(bytes);
        t.file_count = t.file_count.saturating_sub(files);
    }
    if let Some(to) = to {
        match totals.iter_mut().find(|c| c.category == to) {
            Some(t) => {
                t.bytes += bytes;
                t.file_count += files;
            }
            None => totals.push(crate::model::CategoryTotal {
                category: to,
                bytes,
                file_count: files,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{scan, ScanOptions};
    use std::fs;
    use std::sync::atomic::AtomicBool;

    fn scan_home(home: &Path) -> ScanOutcome {
        let mut opts = ScanOptions::new("t", home, home);
        opts.inherit_ancestor_context = false;
        scan(&opts, &AtomicBool::new(false), &|_| {}).unwrap()
    }

    fn category_of(o: &ScanOutcome, path: &Path) -> Option<Category> {
        let p = path.display().to_string();
        o.summary
            .detected_locations
            .iter()
            .find(|l| l.path == p)
            .map(|l| l.classification.category)
            .or_else(|| {
                o.summary
                    .file_kinds
                    .iter()
                    .flat_map(|g| &g.files)
                    .find(|f| f.path == p)
                    .map(|f| f.classification.category)
            })
    }

    /// The numbers a person sees, to compare an adjusted scan with a new one.
    fn seen(o: &ScanOutcome, dirs: &[&Path]) -> String {
        let mut cats: Vec<_> = o
            .summary
            .categories
            .iter()
            .map(|c| (format!("{:?}", c.category), c.bytes, c.file_count))
            .collect();
        cats.sort();
        let kinds: Vec<_> = o
            .summary
            .file_kinds
            .iter()
            .map(|g| {
                (
                    format!("{:?}", g.kind),
                    g.bytes,
                    g.file_count,
                    g.files.len(),
                )
            })
            .collect();
        let sizes: Vec<_> = dirs
            .iter()
            .map(|d| o.index.get(*d).map(|s| (s.bytes, s.files)))
            .collect();
        format!(
            "total {} files {} dirs {}\ncategories {cats:?}\nkinds {kinds:?}\nsizes {sizes:?}\nlargest {}",
            o.summary.total_bytes,
            o.summary.file_count,
            o.summary.dir_count,
            o.summary
                .largest_files
                .iter()
                .filter(|f| !f.path.contains("/.Trash/"))
                .count()
        )
    }

    #[test]
    fn matches_a_new_scan_after_deleting_and_trashing() {
        let dir = crate::test_support::sandbox();
        let home = crate::platform::canonicalize(dir.path()).unwrap();
        let proj = home.join("code/app");
        let modules = proj.join("node_modules/pkg");
        fs::create_dir_all(&modules).unwrap();
        fs::write(proj.join("package.json"), "{}").unwrap();
        fs::write(modules.join("index.js"), vec![b'x'; 300_000]).unwrap();
        fs::write(modules.join("big.bin"), vec![b'y'; 900_000]).unwrap();
        let photos = home.join("Pictures");
        fs::create_dir_all(&photos).unwrap();
        let photo = photos.join("beach.jpg");
        fs::write(&photo, vec![b'z'; 640_000]).unwrap();
        fs::write(photos.join("keep.jpg"), vec![b'k'; 320_000]).unwrap();
        fs::create_dir_all(home.join(".Trash")).unwrap();

        let before = scan_home(&home);
        let node_modules = proj.join("node_modules");
        let removals = vec![
            Removal {
                path: node_modules.clone(),
                bytes: 0,
                category: category_of(&before, &node_modules),
                trashed: false,
            },
            Removal {
                path: photo.clone(),
                bytes: before
                    .summary
                    .file_kinds
                    .iter()
                    .flat_map(|g| &g.files)
                    .find(|f| f.path == photo.display().to_string())
                    .map(|f| f.bytes)
                    .unwrap(),
                category: category_of(&before, &photo),
                trashed: true,
            },
        ];
        assert!(
            removals.iter().all(|r| r.category.is_some()),
            "{removals:?}"
        );

        // What actually happens on disk.
        fs::remove_dir_all(&node_modules).unwrap();
        // On Windows the Recycle Bin is outside the scan, so the photo leaves it.
        #[cfg(windows)]
        fs::remove_file(&photo).unwrap();
        #[cfg(not(windows))]
        fs::rename(&photo, home.join(".Trash/beach.jpg")).unwrap();

        let adjusted = apply(&before, &removals, &home);
        let rescanned = scan_home(&home);
        let dirs = [
            home.as_path(),
            proj.as_path(),
            photos.as_path(),
            &home.join(".Trash"),
        ];
        assert_eq!(seen(&adjusted, &dirs), seen(&rescanned, &dirs));
        assert!(!adjusted.index.contains_key(&node_modules));
        assert!(adjusted
            .summary
            .largest_files
            .iter()
            .all(|f| !f.path.starts_with(&node_modules.display().to_string())));
    }

    #[test]
    fn emptying_the_trash_takes_its_bytes_off_the_totals() {
        let dir = crate::test_support::sandbox();
        let home = crate::platform::canonicalize(dir.path()).unwrap();
        let trash = home.join(".Trash");
        fs::create_dir_all(trash.join("old")).unwrap();
        fs::write(trash.join("old/a.bin"), vec![b'a'; 500_000]).unwrap();
        fs::write(trash.join("b.bin"), vec![b'b'; 200_000]).unwrap();
        fs::write(home.join("notes.txt"), "x").unwrap();

        let before = scan_home(&home);
        let bytes_b = 200_000u64.next_multiple_of(4096);
        let removals: Vec<_> = [trash.join("old"), trash.join("b.bin")]
            .into_iter()
            .map(|path| Removal {
                bytes: if path.ends_with("b.bin") { bytes_b } else { 0 },
                path,
                category: Some(Category::UserData),
                trashed: false,
            })
            .collect();
        fs::remove_dir_all(trash.join("old")).unwrap();
        fs::remove_file(trash.join("b.bin")).unwrap();

        let adjusted = apply(&before, &removals, &home);
        let rescanned = scan_home(&home);
        assert_eq!(
            seen(&adjusted, &[home.as_path(), trash.as_path()]),
            seen(&rescanned, &[home.as_path(), trash.as_path()])
        );
    }

    #[test]
    fn ignores_paths_outside_the_scan_and_repeats() {
        let dir = crate::test_support::sandbox();
        let home = crate::platform::canonicalize(dir.path()).unwrap();
        fs::write(home.join("a.txt"), "x").unwrap();
        let before = scan_home(&home);
        let outside = Removal {
            path: PathBuf::from("/elsewhere/file"),
            bytes: 10,
            category: None,
            trashed: false,
        };
        let after = apply(&before, &[outside], &home);
        assert_eq!(
            seen(&after, &[home.as_path()]),
            seen(&before, &[home.as_path()])
        );
    }
}
