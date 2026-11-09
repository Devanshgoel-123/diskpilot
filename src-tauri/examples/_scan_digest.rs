//! Prints a digest of a scan's results (everything except timings), so two
//! builds can be checked for identical output on the same tree.
//! Usage: cargo run --release --example _scan_digest -- [root]
//! Env: PORTABLE=1 lists with read_dir + lstat; NO_DEFER=1 as in _bench_scan.
use diskpilot_lib::scanner::{scan, ScanOptions};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let home = dirs::home_dir().unwrap();
    let root = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| home.clone());
    diskpilot_lib::scanner::FORCE_PORTABLE
        .store(std::env::var_os("PORTABLE").is_some(), Ordering::Relaxed);
    let mut opts = ScanOptions::new("digest", &root, &home);
    if std::env::var_os("NO_DEFER").is_none() {
        opts = opts.deferring_app_containers();
    }
    let o = scan(&opts, &AtomicBool::new(false), &|_| {}).unwrap();
    let mut s = o.summary;
    s.started_at_ms = 0;
    s.finished_at_ms = 0;
    s.duration_ms = 0;
    // Workers record errors in whatever order they meet them.
    s.error_samples.sort_by(|a, b| a.path.cmp(&b.path));
    let mut v = serde_json::to_value(&s).unwrap();
    let mut part = |key: &str| {
        let x = v.as_object_mut().unwrap().remove(key).unwrap();
        hex(&Sha256::digest(x.to_string().as_bytes()))
    };
    println!(
        "files={} dirs={} bytes={} errors={}",
        s.file_count, s.dir_count, s.total_bytes, s.error_count
    );
    for key in [
        "categories",
        "largestFiles",
        "fileKinds",
        "detectedLocations",
        "heavyDirectories",
        "errorSamples",
    ] {
        println!("{key:18} {}", part(key));
    }
    println!(
        "{:18} {}",
        "rest",
        hex(&Sha256::digest(v.to_string().as_bytes()))
    );
    let mut idx: Vec<_> = o.index.into_iter().collect();
    idx.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = Sha256::new();
    for (p, st) in &idx {
        h.update(format!("{}|{st:?}\n", p.display()).as_bytes());
    }
    println!(
        "{:18} {} ({} folders)",
        "index",
        hex(&h.finalize()),
        idx.len()
    );
}
