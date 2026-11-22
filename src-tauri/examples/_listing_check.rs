//! Local-only: scans a root with read_dir+lstat and with getattrlistbulk and
//! reports where the results differ.
#[cfg(target_os = "macos")]
use diskpilot_lib::scanner::{scan, ScanOptions, FORCE_PORTABLE};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::time::Instant;

/// getattrlistbulk exists on macOS only.
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This check compares macOS listing methods.");
}

#[cfg(target_os = "macos")]
fn main() {
    let home = dirs::home_dir().unwrap();
    let root: std::path::PathBuf = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or(home.clone());
    let run = |portable: bool| {
        FORCE_PORTABLE.store(portable, Ordering::Relaxed);
        let t = Instant::now();
        let o = scan(
            &ScanOptions::new("chk", &root, &home),
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
        println!(
            "{}: {:.2}s files={} dirs={} bytes={} symlinks={} skipped_vols={} errors={}",
            if portable { "portable" } else { "bulk    " },
            t.elapsed().as_secs_f64(),
            o.summary.file_count,
            o.summary.dir_count,
            o.summary.total_bytes,
            o.summary.symlink_count,
            o.summary.skipped_volumes,
            o.summary.error_count
        );
        o
    };
    let a = run(true);
    let b = run(false);
    let mut diffs: Vec<_> = a
        .index
        .iter()
        .filter_map(|(p, sa)| {
            let sb = b.index.get(p)?;
            (sa.direct_bytes != sb.direct_bytes || sa.files != sb.files).then(|| {
                (
                    sa.direct_bytes as i64 - sb.direct_bytes as i64,
                    sa.files as i64 - sb.files as i64,
                    p.clone(),
                )
            })
        })
        .collect();
    let only_a = a.index.keys().filter(|p| !b.index.contains_key(*p)).count();
    let only_b = b.index.keys().filter(|p| !a.index.contains_key(*p)).count();
    diffs.sort_by_key(|d| std::cmp::Reverse(d.0.abs()));
    println!(
        "dirs differing in direct bytes/files: {}  only-portable: {only_a}  only-bulk: {only_b}",
        diffs.len()
    );
    for d in diffs.iter().take(15) {
        println!("  bytes {:+} files {:+}  {}", d.0, d.1, d.2.display());
    }
    let fa: Vec<_> = a
        .summary
        .largest_files
        .iter()
        .map(|f| (&f.path, f.bytes, f.logical_bytes, f.modified_ms))
        .collect();
    let fb: Vec<_> = b
        .summary
        .largest_files
        .iter()
        .map(|f| (&f.path, f.bytes, f.logical_bytes, f.modified_ms))
        .collect();
    println!("largest files identical: {}", fa == fb);
    if fa != fb {
        for (x, y) in fa.iter().zip(&fb) {
            if x != y {
                println!("  {x:?}\n  {y:?}");
            }
        }
    }
}
