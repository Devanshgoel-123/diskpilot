//! Times read-only scans on any platform.
//!
//! Usage: cargo run --release --example scan_bench -- [root] [runs]
//!
//! Environment:
//! - `PORTABLE=1`: list folders with `read_dir` instead of the batched
//!   listing (getattrlistbulk on macOS, GetFileInformationByHandleEx on
//!   Windows).
//! - `FAST=1`: read the NTFS file table (Windows, run as administrator).
//! - `BUF=<KiB>[,<KiB>…]` (Windows): listing buffer sizes to compare; each
//!   size runs `runs` times.
//!
//! The first run warms the file-system cache; compare the later ones.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use diskpilot_lib::scanner::{scan, ScanOptions, FORCE_PORTABLE};

fn main() {
    let home = dirs::home_dir().expect("home folder");
    let mut args = std::env::args().skip(1);
    let root = args.next().map(Into::into).unwrap_or_else(|| home.clone());
    let runs: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    FORCE_PORTABLE.store(std::env::var_os("PORTABLE").is_some(), Ordering::Relaxed);
    let fast = std::env::var_os("FAST").is_some();
    println!(
        "root={} runs={runs} portable={} fast={fast} elevated={}",
        std::path::Path::new(&root).display(),
        FORCE_PORTABLE.load(Ordering::Relaxed),
        diskpilot_lib::platform::is_elevated(),
    );
    let sizes: Vec<usize> = std::env::var("BUF")
        .ok()
        .map(|s| s.split(',').filter_map(|k| k.trim().parse().ok()).collect())
        .unwrap_or_default();
    if sizes.is_empty() {
        bench(&root, &home, runs, fast, "");
    }
    for kib in sizes {
        set_buffer(kib * 1024);
        bench(&root, &home, runs, fast, &format!("buf={kib}KiB "));
    }
}

#[cfg(windows)]
fn set_buffer(bytes: usize) {
    diskpilot_lib::scanner::LIST_BUFFER_BYTES.store(bytes, Ordering::Relaxed);
}

#[cfg(not(windows))]
fn set_buffer(_bytes: usize) {
    eprintln!("BUF only applies on Windows");
}

fn bench(root: &std::path::Path, home: &std::path::Path, runs: usize, fast: bool, label: &str) {
    for i in 0..runs {
        let mut opts = ScanOptions::new("bench", root, home);
        opts.fast = fast;
        let t = Instant::now();
        let o = scan(&opts, &AtomicBool::new(false), &|_| {}).expect("scan");
        let secs = t.elapsed().as_secs_f64();
        let s = &o.summary;
        println!(
            "{label}run {i}: {secs:.2}s  files={} dirs={} bytes={:.2}GB errors={} \
             symlinks={} ({:.0} entries/s)",
            s.file_count,
            s.dir_count,
            s.total_bytes as f64 / 1e9,
            s.error_count,
            s.symlink_count,
            (s.file_count + s.dir_count) as f64 / secs.max(1e-9),
        );
    }
}
