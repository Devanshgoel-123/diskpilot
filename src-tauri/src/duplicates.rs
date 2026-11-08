//! Finds files with identical contents.
//!
//! Files are grouped by size, then by a SHA-256 of their first 64 KiB, then by
//! a SHA-256 of their whole contents. Only files whose full hashes match are
//! reported. Before any copy is moved to the Trash, it is compared byte for
//! byte with a copy that stays ([`same_contents`]).
//!
//! The search only reads. It never follows symbolic links, never leaves the
//! starting volume, and skips places where removing a copy could break
//! something: hidden folders, `~/Library`, app and document bundles, and
//! dependency folders and package stores such as the Go module cache. Hard
//! links to the same file are counted once, and APFS clones are flagged
//! because removing a clone frees little or no space.

use std::collections::{HashMap, HashSet};
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::platform;
use crate::scanner::{now_ms, to_ms};

pub const EVENT_DUPLICATES_PROGRESS: &str = "duplicates://progress";
pub const DEFAULT_MIN_BYTES: u64 = 1024 * 1024;
const PREFIX_BYTES: usize = 64 * 1024;
const MAX_GROUPS: usize = 1000;

/// Folders whose contents belong to an app or a document package.
const BUNDLE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "bundle",
    "framework",
    "kext",
    "plugin",
    "xpc",
    "pkg",
    "mpkg",
    "photoslibrary",
    "musiclibrary",
    "tvlibrary",
    "imovielibrary",
    "fcpbundle",
    "logicx",
    "band",
    "xcodeproj",
    "xcworkspace",
    "xcarchive",
    "playground",
    "sparsebundle",
    "pages",
    "numbers",
    "key",
    "rtfd",
    "lrlibrary",
    "lrdata",
    "aplibrary",
    "photolibrary",
    "cocatalog",
    "cosessiondb",
    "cardboard",
    "screenflow",
    "utm",
    "pvm",
    "vmwarevm",
];
/// Dependency and package-manager folders: their files are managed by a tool,
/// and removing one copy breaks the package it belongs to.
const SKIPPED_DIRS: &[&str] = &[
    "node_modules",
    "bower_components",
    "Pods",
    "Carthage",
    "DerivedData",
    "target",
    "venv",
    "site-packages",
    "vendor",
    "__pycache__",
];
/// Package stores, and media that an app manages in its own library,
/// relative to the home folder. Removing a file from a Music, TV or Photo
/// Booth library leaves a broken entry in that app.
pub(crate) const SKIPPED_HOME_DIRS: &[&str] = &[
    "Library",
    "go/pkg",
    "Applications",
    "miniconda3",
    "anaconda3",
    "Music/Music",
    "Music/iTunes",
    "Movies/TV",
    "Pictures/Photo Booth Library",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFile {
    pub path: String,
    pub modified_ms: Option<i64>,
    /// Suggested copy to keep; exactly one per group.
    pub keep: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// SHA-256 of the contents, hex.
    pub id: String,
    pub name: String,
    /// Size of each copy.
    pub bytes: u64,
    pub files: Vec<DuplicateFile>,
    /// Space freed by removing every copy but one, excluding clones.
    pub reclaimable_bytes: u64,
    /// Some copies are APFS clones that share storage with another file, so
    /// removing them frees less than their size.
    pub shares_storage: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateReport {
    pub root: String,
    pub min_bytes: u64,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub files_examined: u64,
    pub bytes_hashed: u64,
    pub groups: Vec<DuplicateGroup>,
    pub reclaimable_bytes: u64,
    /// More groups were found than are listed.
    pub truncated: bool,
    pub cancelled: bool,
    /// Folders or files that could not be read.
    pub unreadable: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateProgress {
    /// "listing" or "comparing".
    pub phase: &'static str,
    pub files_examined: u64,
    pub bytes_hashed: u64,
    pub bytes_to_hash: u64,
    pub current: Option<String>,
}

#[derive(Default)]
pub struct DuplicateSlot {
    pub running: Option<std::sync::Arc<AtomicBool>>,
    pub report: Option<DuplicateReport>,
}

struct Candidate {
    path: PathBuf,
    size: u64,
    modified_ms: Option<i64>,
}

fn skip_dir(name: &str, path: &Path, home: &Path) -> bool {
    skipped_dir_name(name) || SKIPPED_HOME_DIRS.iter().any(|d| path == home.join(d))
}

/// A hidden folder, a dependency folder, or an app or document bundle: its
/// files are managed by an app or a tool, not by the person.
pub(crate) fn skipped_dir_name(name: &str) -> bool {
    if name.starts_with('.') || SKIPPED_DIRS.contains(&name) {
        return true;
    }
    match name.rsplit_once('.') {
        Some((_, ext)) => BUNDLE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// Regular files of at least `min_bytes` under `root`, one per inode.
fn list_files(
    root: &Path,
    home: &Path,
    min_bytes: u64,
    cancel: &AtomicBool,
    unreadable: &mut u64,
    tick: &mut dyn FnMut(u64, &Path),
) -> Vec<Candidate> {
    let Ok(root_meta) = root.symlink_metadata() else {
        *unreadable += 1;
        return Vec::new();
    };
    let dev = platform::device(&root_meta);
    let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
    let mut out = Vec::new();
    let mut examined = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            *unreadable += 1;
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(meta) = path.symlink_metadata() else {
                *unreadable += 1;
                continue;
            };
            if platform::device(&meta) != dev || meta.file_type().is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if meta.is_dir() {
                if !skip_dir(&name, &path, home) {
                    stack.push(path);
                }
                continue;
            }
            // Hidden files, and cloud files whose data is not on this computer:
            // reading one to compare it would download it.
            if !meta.is_file()
                || name.starts_with('.')
                || platform::hidden(&meta)
                || !platform::is_local(&meta)
            {
                continue;
            }
            examined += 1;
            if examined % 500 == 0 {
                tick(examined, &path);
            }
            if meta.len() < min_bytes {
                continue;
            }
            // Hard links to one file are not copies. Windows needs to open the
            // file to know its identity, so it is only asked for files that
            // are big enough to matter.
            let id = platform::identity(&meta).or_else(|| platform::identity_at(&path));
            if id.is_some_and(|id| !seen_inodes.insert((id.volume, id.index))) {
                continue;
            }
            out.push(Candidate {
                modified_ms: meta.modified().ok().and_then(to_ms),
                size: meta.len(),
                path,
            });
        }
    }
    tick(examined, root);
    out
}

/// SHA-256 of the first `limit` bytes (or the whole file), counting bytes read.
fn hash_file(
    path: &Path,
    limit: Option<usize>,
    cancel: &AtomicBool,
    read: &mut u64,
) -> io::Result<[u8; 32]> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut left = limit.unwrap_or(usize::MAX);
    while left > 0 {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let want = buf.len().min(left);
        let n = f.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        *read += n as u64;
        left -= n;
    }
    Ok(hasher.finalize().into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// APFS clone information: the clone id, and whether the file may share
/// blocks with another file.
#[cfg(target_os = "macos")]
fn clone_info(path: &Path) -> Option<(u64, bool)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    const ATTR_BIT_MAP_COUNT: u16 = 5;
    const FSOPT_NOFOLLOW: u32 = 0x1;
    const FSOPT_ATTR_CMN_EXTENDED: u32 = 0x20;
    const ATTR_CMNEXT_CLONEID: u32 = 0x100;
    const ATTR_CMNEXT_EXT_FLAGS: u32 = 0x200;
    const EF_MAY_SHARE_BLOCKS: u64 = 0x1;

    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut list = libc::attrlist {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: 0,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: ATTR_CMNEXT_CLONEID | ATTR_CMNEXT_EXT_FLAGS,
    };
    let mut buf = [0u8; 4 + 8 + 8];
    // SAFETY: `list` and `buf` are valid for the duration of the call and the
    // buffer length passed matches the buffer.
    let rc = unsafe {
        libc::getattrlist(
            c.as_ptr(),
            (&mut list as *mut libc::attrlist).cast(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED,
        )
    };
    if rc != 0 {
        return None;
    }
    let id = u64::from_ne_bytes(buf[4..12].try_into().ok()?);
    let flags = u64::from_ne_bytes(buf[12..20].try_into().ok()?);
    Some((id, flags & EF_MAY_SHARE_BLOCKS != 0))
}

#[cfg(not(target_os = "macos"))]
fn clone_info(_path: &Path) -> Option<(u64, bool)> {
    None
}

/// Which copy to suggest keeping: one outside Downloads, then the oldest,
/// then the shortest path.
fn suggest_keep(files: &mut [DuplicateFile], home: &Path) {
    let downloads = home.join("Downloads");
    let best = files
        .iter()
        .enumerate()
        .min_by_key(|(_, f)| {
            (
                Path::new(&f.path).starts_with(&downloads),
                f.modified_ms.unwrap_or(i64::MAX),
                f.path.len(),
            )
        })
        .map(|(i, _)| i);
    for (i, f) in files.iter_mut().enumerate() {
        f.keep = Some(i) == best;
    }
}

/// Groups of two or more candidates with the same key. Candidates without a
/// key (unreadable) are dropped.
fn group_by<K: std::hash::Hash + Eq>(
    items: Vec<Candidate>,
    mut key: impl FnMut(&Candidate) -> Option<K>,
) -> Vec<(K, Vec<Candidate>)> {
    let mut map: HashMap<K, Vec<Candidate>> = HashMap::new();
    for c in items {
        if let Some(k) = key(&c) {
            map.entry(k).or_default().push(c);
        }
    }
    map.into_iter().filter(|(_, g)| g.len() > 1).collect()
}

pub fn find(
    root: &Path,
    home: &Path,
    min_bytes: u64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&DuplicateProgress),
) -> DuplicateReport {
    let started = now_ms();
    let mut unreadable = 0u64;
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut examined_total = 0u64;
    let files = list_files(
        root,
        home,
        min_bytes.max(1),
        cancel,
        &mut unreadable,
        &mut |n, p| {
            examined_total = n;
            if last.elapsed() >= Duration::from_millis(80) {
                last = Instant::now();
                progress(&DuplicateProgress {
                    phase: "listing",
                    files_examined: n,
                    bytes_hashed: 0,
                    bytes_to_hash: 0,
                    current: Some(p.display().to_string()),
                });
            }
        },
    );

    // Same size first; only those can be identical.
    let by_size = group_by(files, |c| Some(c.size));
    let bytes_to_hash: u64 = by_size.iter().flat_map(|(_, g)| g).map(|c| c.size).sum();
    let mut hashed = 0u64;
    let mut report_progress = |hashed: u64, p: &Path, force: bool| {
        if force || last.elapsed() >= Duration::from_millis(80) {
            last = Instant::now();
            progress(&DuplicateProgress {
                phase: "comparing",
                files_examined: examined_total,
                bytes_hashed: hashed,
                bytes_to_hash,
                current: Some(p.display().to_string()),
            });
        }
    };

    let mut groups = Vec::new();
    for (_, same_size) in by_size {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let small = same_size[0].size as usize <= PREFIX_BYTES;
        let by_prefix = group_by(same_size, |c| {
            let h = hash_file(&c.path, Some(PREFIX_BYTES), cancel, &mut hashed);
            if h.is_err() && !cancel.load(Ordering::Relaxed) {
                unreadable += 1;
            }
            h.ok().map(|h| hex(&h))
        });
        for (prefix, candidates) in by_prefix {
            let by_full = if small {
                // The prefix hash already covered the whole file.
                vec![(prefix, candidates)]
            } else {
                group_by(candidates, |c| {
                    report_progress(hashed, &c.path, false);
                    let h = hash_file(&c.path, None, cancel, &mut hashed);
                    if h.is_err() && !cancel.load(Ordering::Relaxed) {
                        unreadable += 1;
                    }
                    h.ok().map(|h| hex(&h))
                })
            };
            for (id, same) in by_full {
                groups.push(make_group(id, same, home));
            }
        }
    }
    report_progress(hashed, root, true);

    groups.sort_by(|a, b| {
        b.reclaimable_bytes
            .cmp(&a.reclaimable_bytes)
            .then(a.id.cmp(&b.id))
    });
    let truncated = groups.len() > MAX_GROUPS;
    groups.truncate(MAX_GROUPS);
    DuplicateReport {
        root: root.display().to_string(),
        min_bytes,
        started_at_ms: started,
        finished_at_ms: now_ms(),
        files_examined: examined_total,
        bytes_hashed: hashed,
        reclaimable_bytes: groups.iter().map(|g| g.reclaimable_bytes).sum(),
        groups,
        truncated,
        cancelled: cancel.load(Ordering::Relaxed),
        unreadable,
    }
}

fn make_group(id: String, same: Vec<Candidate>, home: &Path) -> DuplicateGroup {
    let bytes = same[0].size;
    let clones: Vec<Option<(u64, bool)>> = same.iter().map(|c| clone_info(&c.path)).collect();
    let shares_storage = clones.iter().flatten().any(|(_, shared)| *shared);
    // Copies with the same clone id share one set of blocks.
    let distinct = clones
        .iter()
        .enumerate()
        .map(|(i, c)| {
            c.map(|(id, _)| id)
                .filter(|id| *id != 0)
                .unwrap_or(i as u64 | 1 << 63)
        })
        .collect::<HashSet<u64>>()
        .len() as u64;
    let name = same[0]
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut files: Vec<DuplicateFile> = same
        .into_iter()
        .map(|c| DuplicateFile {
            path: c.path.display().to_string(),
            modified_ms: c.modified_ms,
            keep: false,
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    suggest_keep(&mut files, home);
    DuplicateGroup {
        id,
        name,
        bytes,
        reclaimable_bytes: bytes * distinct.saturating_sub(1),
        shares_storage,
        files,
    }
}

/// For each selected copy, the copy that stays in its group: the first file
/// that was not selected, the same choice [`removal_recommendations`] makes.
pub fn kept_copies(report: &DuplicateReport, selected: &[String]) -> HashMap<String, String> {
    let chosen: HashSet<&str> = selected.iter().map(String::as_str).collect();
    let mut kept = HashMap::new();
    for g in &report.groups {
        let Some(keep) = g.files.iter().find(|f| !chosen.contains(f.path.as_str())) else {
            continue;
        };
        for f in g.files.iter().filter(|f| chosen.contains(f.path.as_str())) {
            kept.insert(f.path.clone(), keep.path.clone());
        }
    }
    kept
}

/// True when both files exist, are regular files of the same size, and have
/// the same bytes. Used right before a copy is moved to the Trash.
pub fn same_contents(a: &Path, b: &Path) -> io::Result<bool> {
    let regular = |m: &Metadata| m.file_type().is_file();
    let (ma, mb) = (a.symlink_metadata()?, b.symlink_metadata()?);
    if !regular(&ma) || !regular(&mb) || ma.len() != mb.len() {
        return Ok(false);
    }
    let id = |p: &Path, m: &Metadata| platform::identity(m).or_else(|| platform::identity_at(p));
    if let (Some(ia), Some(ib)) = (id(a, &ma), id(b, &mb)) {
        if (ia.volume, ia.index) == (ib.volume, ib.index) {
            return Ok(false);
        }
    }
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 256 * 1024], vec![0u8; 256 * 1024]);
    loop {
        let n = read_full(&mut fa, &mut ba)?;
        let m = read_full(&mut fb, &mut bb)?;
        if n != m || ba[..n] != bb[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn read_full(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = f.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Remove files that no longer exist (moved to the Trash) from the report.
/// Groups left with a single copy are dropped.
pub fn forget(report: &mut DuplicateReport, gone: &HashSet<String>, home: &Path) {
    for g in &mut report.groups {
        g.files.retain(|f| !gone.contains(&f.path));
        if !g.files.iter().any(|f| f.keep) {
            suggest_keep(&mut g.files, home);
        }
        g.reclaimable_bytes = g.bytes * (g.files.len() as u64).saturating_sub(1);
    }
    report.groups.retain(|g| g.files.len() > 1);
    report.reclaimable_bytes = report.groups.iter().map(|g| g.reclaimable_bytes).sum();
}

/// Recommendations for the copies the user chose to move to the Trash, and
/// the reasons for any that cannot be. Every group keeps at least one copy,
/// and each chosen copy is compared byte for byte with a copy that stays.
pub fn removal_recommendations(
    report: &DuplicateReport,
    selected: &[String],
    classify: &dyn Fn(&Path) -> crate::model::Classification,
) -> (
    Vec<crate::model::Recommendation>,
    Vec<crate::model::PlanRejection>,
) {
    use crate::model::{CleanupMethod, PlanRejection, Recommendation, Tier};

    let chosen: HashSet<&str> = selected.iter().map(String::as_str).collect();
    let reject = |path: &str, reason: &str| PlanRejection {
        recommendation_id: path.to_string(),
        path: path.to_string(),
        reason: reason.to_string(),
    };
    let mut recs = Vec::new();
    let mut rejected = Vec::new();
    let mut known = HashSet::new();
    for g in &report.groups {
        let (remove, keep): (Vec<&DuplicateFile>, Vec<&DuplicateFile>) = g
            .files
            .iter()
            .partition(|f| chosen.contains(f.path.as_str()));
        if remove.is_empty() {
            continue;
        }
        known.extend(remove.iter().map(|f| f.path.as_str()));
        let Some(kept) = keep.first() else {
            for f in remove {
                rejected.push(reject(
                    &f.path,
                    "Every copy was selected. Keep at least one.",
                ));
            }
            continue;
        };
        for f in remove {
            match same_contents(Path::new(&f.path), Path::new(&kept.path)) {
                Ok(true) => {}
                Ok(false) => {
                    rejected.push(reject(
                        &f.path,
                        "It no longer matches the copy you are keeping. Search again.",
                    ));
                    continue;
                }
                Err(e) => {
                    rejected.push(reject(&f.path, &format!("Could not compare it: {e}.")));
                    continue;
                }
            }
            let path = Path::new(&f.path);
            recs.push(Recommendation {
                id: f.path.clone(),
                title: format!("Extra copy of {}", g.name),
                path: f.path.clone(),
                bytes: g.bytes,
                file_count: 1,
                tier: Tier::ReviewFirst,
                confidence: crate::model::Confidence::High,
                classification: classify(path),
                detected_because: format!("Same SHA-256 and the same bytes as {}.", kept.path),
                safe_because: "An identical copy stays where it is.".into(),
                risks: "Apps or shortcuts that open this exact location will no longer find it."
                    .into(),
                proposed_action: "Move to Trash".into(),
                methods: vec![CleanupMethod::Trash],
                alternative: None,
                estimated_reclaim_bytes: if g.shares_storage { 0 } else { g.bytes },
                last_activity_ms: f.modified_ms,
                notes: Vec::new(),
                source: "duplicates".into(),
                marker: None,
                clear_in_app: None,
            });
        }
    }
    for p in selected {
        if !known.contains(p.as_str()) {
            rejected.push(reject(p, "Not part of the last duplicate search."));
        }
    }
    (recs, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn run(root: &Path, min: u64) -> DuplicateReport {
        find(root, root, min, &AtomicBool::new(false), &mut |_| {})
    }

    fn big(fill: u8, len: usize) -> Vec<u8> {
        vec![fill; len]
    }

    #[test]
    fn groups_only_identical_contents() {
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        let a = big(7, 200_000);
        let mut b = a.clone();
        *b.last_mut().unwrap() = 8; // same size and prefix, different end
        write(&root.join("one/a.bin"), &a);
        write(&root.join("two/a copy.bin"), &a);
        write(&root.join("Downloads/a.bin"), &a);
        write(&root.join("three/b.bin"), &b);
        write(&root.join("small1"), b"tiny");
        write(&root.join("small2"), b"tiny");

        let r = run(&root, 1000);
        assert_eq!(r.groups.len(), 1);
        let g = &r.groups[0];
        assert_eq!(g.files.len(), 3);
        assert_eq!(g.reclaimable_bytes, 400_000);
        assert_eq!(g.files.iter().filter(|f| f.keep).count(), 1);
        assert!(!g
            .files
            .iter()
            .find(|f| f.keep)
            .unwrap()
            .path
            .contains("/Downloads/"));
        assert_eq!(g.id.len(), 64);
    }

    #[test]
    fn small_files_compare_by_whole_contents() {
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("x"), &big(1, 5000));
        write(&root.join("y"), &big(1, 5000));
        write(&root.join("z"), &big(2, 5000));
        let r = run(&root, 1);
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].files.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn skips_hard_links_hidden_folders_bundles_and_symlinks() {
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        let data = big(3, 10_000);
        write(&root.join("a"), &data);
        std::fs::hard_link(root.join("a"), root.join("a-link")).unwrap();
        write(&root.join(".hidden/a"), &data);
        write(&root.join("Thing.app/Contents/a"), &data);
        write(&root.join("node_modules/pkg/a"), &data);
        write(&root.join("Library/a"), &data);
        write(&root.join("go/pkg/mod/x@v1/a"), &data);
        std::os::unix::fs::symlink(root.join("a"), root.join("a-sym")).unwrap();
        let r = run(&root, 1);
        assert!(r.groups.is_empty(), "{:?}", r.groups);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clones_free_no_extra_space() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("orig"), &big(9, 300_000));
        let src = CString::new(root.join("orig").as_os_str().as_bytes()).unwrap();
        let dst = CString::new(root.join("clone").as_os_str().as_bytes()).unwrap();
        // SAFETY: both are valid C strings.
        if unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), 0) } != 0 {
            return; // Not APFS; nothing to check.
        }
        let r = run(&root, 1);
        let g = &r.groups[0];
        assert!(g.shares_storage);
        assert_eq!(g.reclaimable_bytes, 0);
    }

    #[test]
    fn verifies_contents_before_removal() {
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("a"), &big(4, 300_000));
        write(&root.join("b"), &big(4, 300_000));
        let mut c = big(4, 300_000);
        c[299_999] = 0;
        write(&root.join("c"), &c);
        assert!(same_contents(&root.join("a"), &root.join("b")).unwrap());
        assert!(!same_contents(&root.join("a"), &root.join("c")).unwrap());
        assert!(!same_contents(&root.join("a"), &root.join("a")).unwrap());
    }

    #[test]
    fn removal_keeps_one_copy_and_rechecks_contents() {
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("a"), &big(6, 10_000));
        write(&root.join("b"), &big(6, 10_000));
        write(&root.join("c"), &big(6, 10_000));
        let r = run(&root, 1);
        let p = |n: &str| root.join(n).display().to_string();
        let classify = |path: &Path| {
            crate::classifier::Classifier::new(&root).classify_path(
                path,
                false,
                &crate::classifier::FsProbe,
            )
        };

        let (recs, rejected) = removal_recommendations(&r, &[p("b"), p("zzz")], &classify);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].methods, vec![crate::model::CleanupMethod::Trash]);
        assert_eq!(rejected.len(), 1);

        let (recs, rejected) = removal_recommendations(&r, &[p("a"), p("b"), p("c")], &classify);
        assert!(recs.is_empty());
        assert_eq!(rejected.len(), 3);

        // A copy edited after the search is not removed.
        write(&root.join("c"), &big(7, 10_000));
        let (recs, rejected) = removal_recommendations(&r, &[p("c")], &classify);
        assert!(recs.is_empty());
        assert!(rejected[0].reason.contains("no longer matches"));
    }

    #[test]
    fn forgetting_removed_copies_updates_groups() {
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        write(&root.join("a"), &big(5, 10_000));
        write(&root.join("b"), &big(5, 10_000));
        let mut r = run(&root, 1);
        let gone: HashSet<String> = [root.join("b").display().to_string()].into();
        forget(&mut r, &gone, &root);
        assert!(r.groups.is_empty());
        assert_eq!(r.reclaimable_bytes, 0);
    }
}
