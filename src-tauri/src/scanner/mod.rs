//! Read-only filesystem scanner.
//!
//! Guarantees:
//! - Never writes, renames or deletes anything.
//! - Never follows symbolic links (uses `symlink_metadata` throughout).
//! - Never crosses into another volume (mount points are counted and skipped).
//! - Counts allocated blocks, not apparent size, and counts hard-linked files
//!   once.
//! - Unreadable folders are recorded as issues; the scan continues.
//!
//! The scanner knows nothing about Tauri. Progress is reported through a
//! callback so it can be tested in isolation.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rayon::prelude::*;

use crate::classifier::{Classifier, Context, FsProbe, Probe, Resolved};
use crate::error::describe_io_error;
use crate::model::{
    Category, CategoryTotal, DetectedLocation, DirectorySize, FileEntry, ScanIssue, ScanProgress,
    ScanSummary,
};

const MAX_ERROR_SAMPLES: usize = 200;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub id: String,
    pub root: PathBuf,
    pub home: PathBuf,
    pub largest_files_limit: usize,
    pub locations_limit: usize,
    pub heavy_dirs_limit: usize,
    /// Apply rules matched by the root's ancestors (e.g. scanning a folder
    /// inside `~/Library/Caches` inherits "Application caches"). Disable to
    /// classify relative to the root only.
    pub inherit_ancestor_context: bool,
}

impl ScanOptions {
    pub fn new(id: impl Into<String>, root: impl Into<PathBuf>, home: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            root: root.into(),
            home: home.into(),
            largest_files_limit: 500,
            locations_limit: 300,
            heavy_dirs_limit: 200,
            inherit_ancestor_context: true,
        }
    }
}

/// Aggregated statistics for one directory (recursive).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirStats {
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub largest_child_dir: u64,
    /// Bytes of files directly in this folder (excluding subfolders).
    pub direct_bytes: u64,
    /// False if the folder itself could not be listed.
    pub readable: bool,
}

pub type DirIndex = HashMap<PathBuf, DirStats>;

#[derive(Debug)]
pub struct ScanOutcome {
    pub summary: ScanSummary,
    pub index: DirIndex,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Scan was cancelled")]
    Cancelled,
    #[error("Cannot scan {path}: {message}")]
    InvalidRoot { path: String, message: String },
    #[error("Failed to start scan workers: {0}")]
    ThreadPool(String),
}

pub fn now_ms() -> i64 {
    to_ms(SystemTime::now()).unwrap_or(0)
}

pub fn to_ms(t: SystemTime) -> Option<i64> {
    t.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

pub fn allocated_bytes(meta: &Metadata) -> u64 {
    // st_blocks is always in 512-byte units, independent of the fs block size.
    meta.blocks().saturating_mul(512)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FileCandidate {
    bytes: u64,
    path: PathBuf,
    logical: u64,
    modified: Option<i64>,
}

struct LocationCandidate {
    ctx: Context,
    bytes: u64,
    files: u64,
}

#[derive(Default, Clone, Copy)]
struct Agg {
    bytes: u64,
    files: u64,
    dirs: u64,
}

struct Shared<'a> {
    opts: &'a ScanOptions,
    classifier: Classifier,
    root_dev: u64,
    cancel: &'a AtomicBool,
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    symlinks: AtomicU64,
    skipped_volumes: AtomicU64,
    errors: AtomicU64,
    error_samples: Mutex<Vec<ScanIssue>>,
    category_bytes: [AtomicU64; Category::COUNT],
    category_files: [AtomicU64; Category::COUNT],
    index: Mutex<DirIndex>,
    top_files: Mutex<BinaryHeap<Reverse<FileCandidate>>>,
    top_threshold: AtomicU64,
    seen_inodes: Mutex<HashSet<(u64, u64)>>,
    locations: Mutex<Vec<LocationCandidate>>,
    current: Mutex<PathBuf>,
}

/// Probe that answers from directory listings the scanner already has.
struct ScanProbe<'a> {
    dir: &'a Path,
    own: &'a HashSet<OsString>,
    parent: Option<&'a Path>,
    parent_names: Option<&'a HashSet<OsString>>,
}

impl Probe for ScanProbe<'_> {
    fn has_child(&self, dir: &Path, name: &str) -> bool {
        if dir == self.dir {
            return self.own.contains(&OsString::from(name));
        }
        if let (Some(parent), Some(names)) = (self.parent, self.parent_names) {
            if dir == parent {
                return names.contains(&OsString::from(name));
            }
        }
        FsProbe.has_child(dir, name)
    }
}

impl Shared<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn record_error(&self, path: &Path, err: &std::io::Error) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        let mut samples = self.error_samples.lock().unwrap_or_else(|e| e.into_inner());
        if samples.len() < MAX_ERROR_SAMPLES {
            samples.push(ScanIssue {
                path: path.display().to_string(),
                message: describe_io_error(err),
            });
        }
    }

    fn add_category(&self, cat: Category, bytes: u64, files: u64) {
        self.category_bytes[cat.index()].fetch_add(bytes, Ordering::Relaxed);
        self.category_files[cat.index()].fetch_add(files, Ordering::Relaxed);
    }

    fn consider_file(&self, candidate: FileCandidate) {
        let limit = self.opts.largest_files_limit;
        if limit == 0 || candidate.bytes < self.top_threshold.load(Ordering::Relaxed) {
            return;
        }
        let mut heap = self.top_files.lock().unwrap_or_else(|e| e.into_inner());
        heap.push(Reverse(candidate));
        if heap.len() > limit {
            heap.pop();
        }
        if heap.len() == limit {
            if let Some(Reverse(min)) = heap.peek() {
                self.top_threshold.store(min.bytes, Ordering::Relaxed);
            }
        }
    }

    /// Allocated size of a file, counting each hard-linked inode once.
    fn file_bytes(&self, meta: &Metadata) -> u64 {
        if meta.nlink() > 1 {
            let mut seen = self.seen_inodes.lock().unwrap_or_else(|e| e.into_inner());
            if !seen.insert((meta.dev(), meta.ino())) {
                return 0;
            }
        }
        allocated_bytes(meta)
    }

    fn scan_dir(
        &self,
        path: &Path,
        dir_meta: &Metadata,
        inherited: Option<Context>,
        parent_names: Option<&HashSet<OsString>>,
    ) -> Agg {
        if self.cancelled() {
            return Agg::default();
        }
        self.dirs.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut cur) = self.current.try_lock() {
            cur.clear();
            cur.push(path);
        }

        let own_bytes = allocated_bytes(dir_meta);
        self.bytes.fetch_add(own_bytes, Ordering::Relaxed);

        let reader = match fs::read_dir(path) {
            Ok(r) => r,
            Err(err) => {
                self.record_error(path, &err);
                let cat = inherited
                    .as_ref()
                    .map_or(Category::Unknown, |c| c.rule.category);
                self.add_category(cat, own_bytes, 0);
                self.insert_index(
                    path,
                    DirStats {
                        bytes: own_bytes,
                        readable: false,
                        ..Default::default()
                    },
                );
                return Agg {
                    bytes: own_bytes,
                    files: 0,
                    dirs: 1,
                };
            }
        };

        let mut entries = Vec::new();
        for entry in reader {
            match entry {
                Ok(e) => entries.push(e),
                Err(err) => self.record_error(path, &err),
            }
        }
        let names: HashSet<OsString> = entries.iter().map(|e| e.file_name()).collect();

        let probe = ScanProbe {
            dir: path,
            own: &names,
            parent: path.parent(),
            parent_names,
        };
        let (ctx, newly_matched) = self.classifier.descend(path, inherited, &probe);

        let dir_category = ctx.as_ref().map_or(Category::Unknown, |c| c.rule.category);
        let mut local_bytes = [0u64; Category::COUNT];
        let mut local_files = [0u64; Category::COUNT];
        local_bytes[dir_category.index()] += own_bytes;

        let mut agg = Agg {
            bytes: own_bytes,
            files: 0,
            dirs: 1,
        };
        let mut subdirs: Vec<(PathBuf, Metadata)> = Vec::new();

        for entry in entries {
            let child = entry.path();
            let meta = match fs::symlink_metadata(&child) {
                Ok(m) => m,
                Err(err) => {
                    self.record_error(&child, &err);
                    continue;
                }
            };
            let ft = meta.file_type();
            if ft.is_dir() {
                if meta.dev() != self.root_dev {
                    self.skipped_volumes.fetch_add(1, Ordering::Relaxed);
                } else {
                    subdirs.push((child, meta));
                }
                continue;
            }
            let bytes = if ft.is_symlink() {
                self.symlinks.fetch_add(1, Ordering::Relaxed);
                allocated_bytes(&meta)
            } else {
                self.file_bytes(&meta)
            };
            agg.bytes += bytes;
            agg.files += 1;
            self.files.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(bytes, Ordering::Relaxed);

            let cat = self
                .classifier
                .resolve_file(&child, ctx.as_ref())
                .category();
            local_bytes[cat.index()] += bytes;
            local_files[cat.index()] += 1;

            if ft.is_file() {
                self.consider_file(FileCandidate {
                    bytes,
                    logical: meta.len(),
                    modified: meta.modified().ok().and_then(to_ms),
                    path: child,
                });
            }
        }

        for cat in Category::ALL {
            let (b, f) = (local_bytes[cat.index()], local_files[cat.index()]);
            if b > 0 || f > 0 {
                self.add_category(cat, b, f);
            }
        }

        let direct_bytes = agg.bytes;
        let children: Vec<Agg> = subdirs
            .par_iter()
            .map(|(p, m)| self.scan_dir(p, m, ctx.clone(), Some(&names)))
            .collect();

        let mut largest_child_dir = 0;
        for c in &children {
            agg.bytes += c.bytes;
            agg.files += c.files;
            agg.dirs += c.dirs;
            largest_child_dir = largest_child_dir.max(c.bytes);
        }

        self.insert_index(
            path,
            DirStats {
                bytes: agg.bytes,
                files: agg.files,
                dirs: agg.dirs,
                largest_child_dir,
                direct_bytes,
                readable: true,
            },
        );

        if newly_matched {
            if let Some(ctx) = ctx {
                self.locations
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(LocationCandidate {
                        ctx,
                        bytes: agg.bytes,
                        files: agg.files,
                    });
            }
        }
        agg
    }

    fn insert_index(&self, path: &Path, stats: DirStats) {
        self.index
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_path_buf(), stats);
    }

    fn progress(&self, started: Instant) -> ScanProgress {
        let current = self
            .current
            .lock()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        ScanProgress {
            scan_id: self.opts.id.clone(),
            files_scanned: self.files.load(Ordering::Relaxed),
            dirs_scanned: self.dirs.load(Ordering::Relaxed),
            bytes_scanned: self.bytes.load(Ordering::Relaxed),
            current_path: current,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }
}

/// Validate and canonicalize a scan root. The root itself may be reached
/// through a symlink (e.g. `/tmp`), but nothing inside it is followed.
pub fn resolve_root(root: &Path) -> Result<PathBuf, ScanError> {
    let invalid = |message: String| ScanError::InvalidRoot {
        path: root.display().to_string(),
        message,
    };
    let canonical = root
        .canonicalize()
        .map_err(|e| invalid(describe_io_error(&e)))?;
    let meta = fs::metadata(&canonical).map_err(|e| invalid(describe_io_error(&e)))?;
    if !meta.is_dir() {
        return Err(invalid("Not a folder".to_string()));
    }
    Ok(canonical)
}

/// Run a full scan. Blocks until finished; call from a background thread.
pub fn scan(
    opts: &ScanOptions,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(ScanProgress) + Sync),
) -> Result<ScanOutcome, ScanError> {
    let started_at_ms = now_ms();
    let started = Instant::now();
    let root = resolve_root(&opts.root)?;
    let root_meta = fs::symlink_metadata(&root).map_err(|e| ScanError::InvalidRoot {
        path: root.display().to_string(),
        message: describe_io_error(&e),
    })?;

    let mut classifier = Classifier::new(&opts.home);
    if !opts.inherit_ancestor_context {
        classifier = classifier.with_floor(&root);
    }
    let inherited = root
        .parent()
        .and_then(|p| classifier.context_at(p, &FsProbe));

    let shared = Shared {
        opts,
        classifier,
        root_dev: root_meta.dev(),
        cancel,
        files: AtomicU64::new(0),
        dirs: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        symlinks: AtomicU64::new(0),
        skipped_volumes: AtomicU64::new(0),
        errors: AtomicU64::new(0),
        error_samples: Mutex::new(Vec::new()),
        category_bytes: std::array::from_fn(|_| AtomicU64::new(0)),
        category_files: std::array::from_fn(|_| AtomicU64::new(0)),
        index: Mutex::new(HashMap::new()),
        top_files: Mutex::new(BinaryHeap::new()),
        top_threshold: AtomicU64::new(0),
        seen_inodes: Mutex::new(HashSet::new()),
        locations: Mutex::new(Vec::new()),
        current: Mutex::new(root.clone()),
    };

    // Deep trees (nested node_modules) need generous stacks for recursion.
    let pool = rayon::ThreadPoolBuilder::new()
        .stack_size(16 * 1024 * 1024)
        .thread_name(|i| format!("diskpilot-scan-{i}"))
        .build()
        .map_err(|e| ScanError::ThreadPool(e.to_string()))?;

    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(PROGRESS_INTERVAL);
                if !done.load(Ordering::Relaxed) {
                    on_progress(shared.progress(started));
                }
            }
        });
        pool.install(|| shared.scan_dir(&root, &root_meta, inherited, None));
        done.store(true, Ordering::Relaxed);
    });

    if shared.cancelled() {
        return Err(ScanError::Cancelled);
    }
    on_progress(shared.progress(started));

    let finished_at_ms = now_ms();
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(build_outcome(
        shared,
        root,
        started_at_ms,
        finished_at_ms,
        duration_ms,
    ))
}

fn build_outcome(
    shared: Shared<'_>,
    root: PathBuf,
    started_at_ms: i64,
    finished_at_ms: i64,
    duration_ms: u64,
) -> ScanOutcome {
    let classifier = shared.classifier.clone();
    let opts = shared.opts;

    let mut categories: Vec<CategoryTotal> = Category::ALL
        .iter()
        .map(|&category| CategoryTotal {
            category,
            bytes: shared.category_bytes[category.index()].load(Ordering::Relaxed),
            file_count: shared.category_files[category.index()].load(Ordering::Relaxed),
        })
        .filter(|c| c.bytes > 0)
        .collect();
    categories.sort_by_key(|c| Reverse(c.bytes));

    let heap = shared
        .top_files
        .into_inner()
        .unwrap_or_else(|e| e.into_inner());
    let mut files: Vec<FileCandidate> = heap.into_iter().map(|Reverse(c)| c).collect();
    files.sort_by_key(|c| Reverse(c.bytes));
    let largest_files = files
        .into_iter()
        .map(|c| FileEntry {
            name: c
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            classification: classifier.classify_path(&c.path, false, &FsProbe),
            path: c.path.display().to_string(),
            bytes: c.bytes,
            logical_bytes: c.logical,
            modified_ms: c.modified,
        })
        .collect();

    let mut locations = shared
        .locations
        .into_inner()
        .unwrap_or_else(|e| e.into_inner());
    locations.sort_by_key(|l| Reverse(l.bytes));
    locations.truncate(opts.locations_limit);
    let detected_locations = locations
        .into_iter()
        .map(|l| DetectedLocation {
            path: l.ctx.path.display().to_string(),
            bytes: l.bytes,
            file_count: l.files,
            classification: Resolved::Dir(l.ctx).to_classification(),
        })
        .collect();

    let index = shared.index.into_inner().unwrap_or_else(|e| e.into_inner());
    let heavy_directories = heavy_directories(&index, &root, opts.heavy_dirs_limit)
        .into_iter()
        .map(|(path, stats)| DirectorySize {
            classification: classifier.classify_path(&path, true, &FsProbe),
            path: path.display().to_string(),
            bytes: stats.bytes,
            file_count: stats.files,
        })
        .collect();

    let error_samples = shared
        .error_samples
        .into_inner()
        .unwrap_or_else(|e| e.into_inner());
    let total_bytes = index.get(&root).map_or(0, |s| s.bytes);

    let summary = ScanSummary {
        id: opts.id.clone(),
        root: root.display().to_string(),
        started_at_ms,
        finished_at_ms,
        duration_ms,
        total_bytes,
        file_count: shared.files.load(Ordering::Relaxed),
        dir_count: shared.dirs.load(Ordering::Relaxed),
        symlink_count: shared.symlinks.load(Ordering::Relaxed),
        skipped_volumes: shared.skipped_volumes.load(Ordering::Relaxed),
        error_count: shared.errors.load(Ordering::Relaxed),
        error_samples,
        categories,
        largest_files,
        detected_locations,
        heavy_directories,
    };
    ScanOutcome { summary, index }
}

/// Folders that hold most of their data directly (in their own files rather
/// than in subfolders). These are where storage physically accumulates, as
/// opposed to the chain of ancestors above them.
pub fn heavy_directories(index: &DirIndex, root: &Path, limit: usize) -> Vec<(PathBuf, DirStats)> {
    let mut heavy: Vec<(PathBuf, DirStats)> = index
        .iter()
        .filter(|(path, s)| {
            path.as_path() != root && s.bytes > 0 && s.direct_bytes.saturating_mul(2) >= s.bytes
        })
        .map(|(p, s)| (p.clone(), *s))
        .collect();
    heavy.sort_by_key(|(_, s)| Reverse(s.bytes));
    heavy.truncate(limit);
    heavy
}
