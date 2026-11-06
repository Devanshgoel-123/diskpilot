//! Integration tests: run the real scanner against a temporary directory tree.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use diskpilot_lib::classifier::Classifier;
use diskpilot_lib::explorer;
use diskpilot_lib::model::{Category, EntryKind};
use diskpilot_lib::scanner::{scan, scan_with_partial, ScanError, ScanOptions, ScanOutcome};

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
}

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![7u8; bytes]).unwrap();
}

/// A fake home folder with a few recognizable developer artifacts.
fn fixture() -> Fixture {
    // Not the system temp dir: on macOS it lives under /private/var/folders,
    // which the classifier (correctly) treats as system cache.
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let home = dir.path().canonicalize().unwrap().join("home");
    write(&home.join("Library/Caches/com.example/blob.bin"), 200_000);
    write(
        &home.join("Library/Developer/Xcode/DerivedData/App-x/Build/out.o"),
        300_000,
    );
    write(&home.join("code/web/package.json"), 100);
    write(&home.join("code/web/node_modules/react/index.js"), 50_000);
    write(&home.join("code/web/node_modules/react/package.json"), 100);
    write(
        &home.join("code/web/node_modules/react/dist/react.js"),
        80_000,
    );
    write(&home.join("code/web/.next/cache/a.pack"), 120_000);
    write(&home.join("Downloads/installer.dmg"), 400_000);
    write(&home.join("Documents/notes.txt"), 1_000);
    write(&home.join("mystery.bin.xyz"), 10_000);
    Fixture { _dir: dir, home }
}

fn opts(f: &Fixture) -> ScanOptions {
    let mut o = ScanOptions::new("test", &f.home, &f.home);
    o.inherit_ancestor_context = false;
    o
}

fn run(f: &Fixture) -> ScanOutcome {
    let opts = opts(f);
    scan(&opts, &AtomicBool::new(false), &|_| {}).expect("scan succeeds")
}

fn category_bytes(o: &ScanOutcome, cat: Category) -> u64 {
    o.summary
        .categories
        .iter()
        .find(|c| c.category == cat)
        .map_or(0, |c| c.bytes)
}

#[test]
fn folder_sizes_add_up() {
    let f = fixture();
    let o = run(&f);
    let s = &o.summary;
    assert_eq!(s.file_count, 10);
    assert_eq!(s.error_count, 0);
    // Every byte is attributed to exactly one category.
    let cat_sum: u64 = s.categories.iter().map(|c| c.bytes).sum();
    assert_eq!(cat_sum, s.total_bytes);
    // Parent size equals the sum of its children plus its own entry.
    let code = o.index[&f.home.join("code")].bytes;
    let web = o.index[&f.home.join("code/web")].bytes;
    assert!(code >= web && web > 0);
    // Allocated sizes are at least the logical sizes written.
    assert!(o.index[&f.home.join("Downloads")].bytes >= 400_000);
}

#[test]
fn classifies_developer_artifacts() {
    let f = fixture();
    let o = run(&f);
    assert!(category_bytes(&o, Category::BuildArtifacts) >= 420_000); // DerivedData + .next
    assert!(category_bytes(&o, Category::Cache) >= 200_000);
    assert!(category_bytes(&o, Category::Downloads) >= 400_000);
    assert!(category_bytes(&o, Category::DeveloperTools) >= 130_000); // node_modules

    let rules: Vec<&str> = o
        .summary
        .detected_locations
        .iter()
        .map(|l| l.classification.rule_id.as_str())
        .collect();
    assert!(rules.contains(&"node.node-modules"));
    assert!(rules.contains(&"node.next-build"));
    assert!(rules.contains(&"xcode.derived-data"));
    // The dist/ inside node_modules is part of node_modules, not separate.
    assert!(!rules.contains(&"node.dist"));
}

#[test]
fn largest_files_are_sorted_and_not_assumed_disposable() {
    let f = fixture();
    let o = run(&f);
    let files = &o.summary.largest_files;
    assert_eq!(files[0].name, "installer.dmg");
    assert!(files.windows(2).all(|w| w[0].bytes >= w[1].bytes));
    let mystery = files.iter().find(|f| f.name == "mystery.bin.xyz").unwrap();
    assert_eq!(mystery.classification.category, Category::Unknown);
}

#[test]
fn symlinks_are_not_followed() {
    let f = fixture();
    let outside = f.home.parent().unwrap().join("outside");
    write(&outside.join("huge.bin"), 1_000_000);
    std::os::unix::fs::symlink(&outside, f.home.join("link-to-outside")).unwrap();
    let o = run(&f);
    assert_eq!(o.summary.symlink_count, 1);
    assert!(o.summary.total_bytes < 1_000_000 + 1_500_000);
    assert!(!o.summary.largest_files.iter().any(|f| f.name == "huge.bin"));
}

#[test]
fn hard_links_are_counted_once() {
    let f = fixture();
    let a = f.home.join("Documents/big.dat");
    write(&a, 500_000);
    let baseline = run(&f).summary.total_bytes;
    fs::hard_link(&a, f.home.join("Documents/big-link.dat")).unwrap();
    let with_link = run(&f).summary.total_bytes;
    // Only the directory entry may grow; the 500 KB payload is not doubled.
    assert!(with_link < baseline + 100_000, "{baseline} -> {with_link}");
}

#[test]
fn unreadable_folders_are_reported_not_fatal() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let locked = f.home.join("locked");
    write(&locked.join("secret.txt"), 100);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let o = run(&f);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    // Running as root (some CI containers) can read anyway.
    if o.summary.error_count > 0 {
        assert!(o
            .summary
            .error_samples
            .iter()
            .any(|e| e.path.ends_with("locked")));
        assert!(!o.index[&locked].readable);
    }
}

#[test]
fn cancellation_stops_the_scan() {
    let f = fixture();
    let opts = opts(&f);
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        scan(&opts, &cancel, &|_| {}),
        Err(ScanError::Cancelled)
    ));
}

#[test]
fn reports_progress() {
    let f = fixture();
    let opts = opts(&f);
    let seen = Mutex::new(Vec::new());
    scan(&opts, &AtomicBool::new(false), &|p| {
        seen.lock().unwrap().push(p)
    })
    .unwrap();
    let seen = seen.into_inner().unwrap();
    let last = seen.last().expect("final progress event");
    assert_eq!(last.files_scanned, 10);
    assert_eq!(last.scan_id, "test");
}

#[test]
fn rejects_missing_root() {
    let opts = ScanOptions::new("test", "/definitely/not/here", "/tmp");
    assert!(matches!(
        scan(&opts, &AtomicBool::new(false), &|_| {}),
        Err(ScanError::InvalidRoot { .. })
    ));
}

#[test]
fn explorer_lists_children_with_sizes_and_classifications() {
    let f = fixture();
    let o = run(&f);
    let classifier = Classifier::new(&f.home).with_floor(&f.home);
    let web = f.home.join("code/web");
    let listing = explorer::list_directory(&web, &f.home, &o.index, &classifier).unwrap();
    assert_eq!(
        listing.parent.as_deref(),
        Some(f.home.join("code").to_str().unwrap())
    );
    let nm = listing
        .entries
        .iter()
        .find(|e| e.name == "node_modules")
        .unwrap();
    assert_eq!(nm.kind, EntryKind::Directory);
    assert_eq!(nm.classification.rule_id, "node.node-modules");
    assert_eq!(nm.bytes, Some(o.index[&web.join("node_modules")].bytes));
    // Sorted largest first.
    let sizes: Vec<u64> = listing
        .entries
        .iter()
        .map(|e| e.bytes.unwrap_or(0))
        .collect();
    assert!(sizes.windows(2).all(|w| w[0] >= w[1]));

    let root_listing = explorer::list_directory(&f.home, &f.home, &o.index, &classifier).unwrap();
    assert!(root_listing.parent.is_none());
}

#[test]
fn heavy_directories_are_where_files_live() {
    let f = fixture();
    let o = run(&f);
    let heavy: Vec<&str> = o
        .summary
        .heavy_directories
        .iter()
        .map(|d| d.path.as_str())
        .collect();
    let downloads = f.home.join("Downloads");
    assert!(heavy.contains(&downloads.to_str().unwrap()));
    // Pure container folders (no files of their own) are not "heavy".
    assert!(!heavy.contains(&f.home.join("code").to_str().unwrap()));
    assert!(!heavy.contains(&f.home.to_str().unwrap()));
}

#[test]
fn measure_matches_the_scanner_and_ignores_symlinks() {
    let f = fixture();
    let o = run(&f);
    let web = f.home.join("code/web");
    std::os::unix::fs::symlink(f.home.join("Downloads"), web.join("link")).unwrap();
    let measured = diskpilot_lib::scanner::measure(&web);
    let scanned = o.index[&web].bytes;
    // The new symlink adds at most one small entry; the 400 KB target is not followed.
    assert!(
        measured >= scanned && measured < scanned + 50_000,
        "{scanned} vs {measured}"
    );
}

/// Comparable view of a summary: everything except ids and timings.
fn comparable(o: &ScanOutcome) -> serde_json::Value {
    let mut v = serde_json::to_value(&o.summary).unwrap();
    for key in ["id", "startedAtMs", "finishedAtMs", "durationMs"] {
        v.as_object_mut().unwrap().remove(key);
    }
    v
}

fn with_app_containers(f: &Fixture) {
    write(
        &f.home
            .join("Library/Containers/com.example.chat/Data/Library/Caches/media.bin"),
        250_000,
    );
    write(
        &f.home
            .join("Library/Containers/com.example.chat/Data/Documents/db.sqlite"),
        90_000,
    );
    write(
        &f.home
            .join("Library/Containers/com.example.maps/Data/tiles/t.bin"),
        60_000,
    );
    fs::create_dir_all(f.home.join("Library/Containers/com.example.empty/Data/a/b")).unwrap();
}

#[test]
fn app_containers_measured_last_give_identical_final_results() {
    let f = fixture();
    with_app_containers(&f);
    let plain = run(&f);

    let deferred_opts = opts(&f).deferring_app_containers();
    let partials = Mutex::new(Vec::new());
    let last_progress = Mutex::new(None);
    let fin = scan_with_partial(
        &deferred_opts,
        &AtomicBool::new(false),
        &|p| *last_progress.lock().unwrap() = Some(p),
        &|o| partials.lock().unwrap().push(o),
    )
    .unwrap();

    // The final results are exactly what a normal scan produces.
    for (k, v) in &plain.index {
        assert_eq!(fin.index.get(k), Some(v), "{}", k.display());
    }
    assert_eq!(fin.index.len(), plain.index.len());
    let (a, b) = (comparable(&fin), comparable(&plain));
    for key in b.as_object().unwrap().keys() {
        assert_eq!(a[key], b[key], "{key}");
    }

    // One partial result, published before the containers were measured.
    let partials = partials.into_inner().unwrap();
    assert_eq!(partials.len(), 1);
    let partial = &partials[0];
    let containers = f.home.join("Library/Containers");
    assert!(!partial
        .index
        .contains_key(&containers.join("com.example.chat")));
    assert!(partial.summary.total_bytes < fin.summary.total_bytes);
    assert!(
        partial.summary.total_bytes + 400_000 <= fin.summary.total_bytes,
        "container files are missing from the partial total"
    );
    // Folders outside the containers are already final in the partial result.
    for dir in ["code", "code/web", "Downloads", "Library/Caches"] {
        let p = f.home.join(dir);
        assert_eq!(partial.index[&p], fin.index[&p], "{dir}");
    }

    let progress = last_progress.into_inner().unwrap().unwrap();
    assert_eq!(progress.containers_total, 3);
    assert_eq!(progress.containers_done, 3);
}

#[test]
fn no_partial_result_without_app_containers() {
    let f = fixture();
    let calls = Mutex::new(0);
    let o = scan_with_partial(
        &opts(&f).deferring_app_containers(),
        &AtomicBool::new(false),
        &|_| {},
        &|_| *calls.lock().unwrap() += 1,
    )
    .unwrap();
    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(comparable(&o), comparable(&run(&f)));
}

#[test]
fn a_scan_rooted_in_app_containers_measures_everything_at_once() {
    let f = fixture();
    with_app_containers(&f);
    let containers = f.home.join("Library/Containers");
    let mut o = ScanOptions::new("test", &containers, &f.home).deferring_app_containers();
    o.inherit_ancestor_context = false;
    let calls = Mutex::new(0);
    let out = scan_with_partial(&o, &AtomicBool::new(false), &|_| {}, &|_| {
        *calls.lock().unwrap() += 1
    })
    .unwrap();
    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(out.summary.file_count, 3);
}

#[test]
fn personal_files_are_grouped_by_type() {
    use diskpilot_lib::filekinds::FileKind;
    let f = fixture();
    let mb = 1024 * 1024;
    write(&f.home.join("Movies/trip.MP4"), 3 * mb);
    write(&f.home.join("Desktop/clip.mov"), 2 * mb);
    write(&f.home.join("Movies/tiny.mp4"), 1_000);
    // Files that belong to an app or a tool are not listed.
    write(&f.home.join("Movies/Edit.fcpbundle/Media/a.mp4"), 5 * mb);
    write(&f.home.join("Library/Caches/app/v.mp4"), 5 * mb);
    write(&f.home.join(".hidden/v.mp4"), 5 * mb);
    write(&f.home.join("code/web/node_modules/x/v.mp4"), 5 * mb);
    let o = run(&f);

    let videos = o
        .summary
        .file_kinds
        .iter()
        .find(|g| g.kind == FileKind::Videos)
        .expect("videos found");
    assert_eq!(videos.file_count, 3);
    let names: Vec<&str> = videos.files.iter().map(|e| e.name.as_str()).collect();
    // Largest first; the 1 KB file counts in the total but is not listed.
    assert_eq!(names, ["trip.MP4", "clip.mov"]);
    assert!(videos.bytes >= 5 * mb as u64);
    let installers = o
        .summary
        .file_kinds
        .iter()
        .find(|g| g.kind == FileKind::Installers)
        .expect("installers found");
    assert_eq!(installers.file_count, 1);
    // The fixture installer is under 1 MB, so it is counted but not listed.
    assert!(installers.files.is_empty());
}
