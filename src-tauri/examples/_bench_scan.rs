//! Times a read-only scan. Usage: cargo run --release --example _bench_scan -- [root] [runs]
use diskpilot_lib::scanner::{scan_with_partial, ScanOptions};
use std::sync::atomic::AtomicBool;
use std::time::Instant;
fn main() {
    let home = dirs::home_dir().unwrap();
    let mut args = std::env::args().skip(1);
    let root = args.next().map(Into::into).unwrap_or_else(|| home.clone());
    let runs: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    // PORTABLE=1 lists folders with read_dir + lstat instead of getattrlistbulk.
    #[cfg(target_os = "macos")]
    diskpilot_lib::scanner::FORCE_PORTABLE.store(
        std::env::var_os("PORTABLE").is_some(),
        std::sync::atomic::Ordering::Relaxed,
    );
    for i in 0..runs {
        let mut opts = ScanOptions::new("bench", &root, &home);
        if std::env::var_os("NO_DEFER").is_none() {
            opts = opts.deferring_app_containers();
        }
        let t = Instant::now();
        let o = scan_with_partial(&opts, &AtomicBool::new(false), &|_| {}, &|p| {
            println!(
                "  partial at {:.2}s  files={} bytes={:.1}GB",
                t.elapsed().as_secs_f64(),
                p.summary.file_count,
                p.summary.total_bytes as f64 / 1e9
            );
        })
        .unwrap();
        let s = &o.summary;
        println!(
            "run {i}: {:.2}s  files={} dirs={} bytes={:.1}GB index={}",
            t.elapsed().as_secs_f64(),
            s.file_count,
            s.dir_count,
            s.total_bytes as f64 / 1e9,
            o.index.len()
        );
    }
}
