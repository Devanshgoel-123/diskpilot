//! Developer tool: run the scanner from the command line (read-only).
//!
//!     cargo run --release --example scan -- [path]

use std::sync::atomic::AtomicBool;

use diskpilot_lib::scanner::{scan, ScanOptions};

fn gb(b: u64) -> String {
    format!("{:.2} GB", b as f64 / 1e9)
}

fn main() {
    let home = dirs::home_dir().expect("home folder");
    let root = std::env::args().nth(1).map_or(home.clone(), Into::into);
    let opts = ScanOptions::new("cli", root, home);
    let outcome = scan(&opts, &AtomicBool::new(false), &|_| {}).expect("scan");
    let s = &outcome.summary;
    println!(
        "{}: {} in {} files / {} dirs, {} ms, {} errors, {} other volumes skipped",
        s.root,
        gb(s.total_bytes),
        s.file_count,
        s.dir_count,
        s.duration_ms,
        s.error_count,
        s.skipped_volumes
    );
    println!("\nCategories:");
    for c in &s.categories {
        println!("  {:<16} {:>10}", format!("{:?}", c.category), gb(c.bytes));
    }
    println!("\nTop detected locations:");
    for l in s.detected_locations.iter().take(15) {
        println!(
            "  {:>10}  {:<28} {}",
            gb(l.bytes),
            l.classification.label,
            l.path
        );
    }
    println!("\nTop heavy directories:");
    for d in s.heavy_directories.iter().take(10) {
        println!("  {:>10}  {}", gb(d.bytes), d.path);
    }
}
