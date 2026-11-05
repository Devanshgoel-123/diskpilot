//! Times a read-only scan. Usage: cargo run --release --example _bench_scan -- [root] [runs]
use diskpilot_lib::scanner::{scan, ScanOptions};
use std::sync::atomic::AtomicBool;
use std::time::Instant;
fn main() {
    let home = dirs::home_dir().unwrap();
    let mut args = std::env::args().skip(1);
    let root = args.next().map(Into::into).unwrap_or_else(|| home.clone());
    let runs: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    for i in 0..runs {
        let opts = ScanOptions::new("bench", &root, &home);
        let t = Instant::now();
        let o = scan(&opts, &AtomicBool::new(false), &|_| {}).unwrap();
        let s = &o.summary;
        println!("run {i}: {:.2}s  files={} dirs={} bytes={:.1}GB index={}",
            t.elapsed().as_secs_f64(), s.file_count, s.dir_count, s.total_bytes as f64/1e9, o.index.len());
    }
}
