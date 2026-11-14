//! Times a read-only scan of a generated, deterministic folder tree, so scan
//! speed can be compared between commits on any machine (used by the `perf`
//! CI job). The tree is made once and reused.
//!
//! Usage: cargo run --release --example _bench_fixture -- <dir> [files] [runs]
//! Prints one line of JSON: {"files":…,"dirs":…,"scanMs":…,"runsMs":[…]}.
use diskpilot_lib::scanner::{scan, ScanOptions};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// Folder names that DiskPilot's rules recognise, so classification runs too.
const PROJECT_DIRS: &[&str] = &["node_modules", "target", ".git", "build", "src", "dist"];

fn make_tree(root: &Path, files: usize) {
    use rayon::prelude::*;
    let marker = root.join(format!(".fixture-v2-{files}"));
    if marker.exists() {
        return;
    }
    let _ = fs::remove_dir_all(root);
    // Small files (up to 255 bytes), written in parallel: making the tree is
    // setup, not what is measured, and the scan's work is the same.
    let per_dir = 50;
    (0..files.div_ceil(per_dir)).into_par_iter().for_each(|i| {
        let area = match i % 4 {
            0 => format!(
                "Projects/p{}/{}",
                i / 24,
                PROJECT_DIRS[i % PROJECT_DIRS.len()]
            ),
            1 => format!("Library/Caches/app{}/d{}", i % 60, i),
            2 => format!("Documents/folder{}/sub{}", i % 80, i),
            _ => format!("Downloads/batch{}", i),
        };
        let dir = root.join(area);
        fs::create_dir_all(&dir).unwrap();
        for j in 0..per_dir.min(files - i * per_dir) {
            let len = (i * 31 + j * 17) % 256;
            let ext = ["js", "rs", "json", "png", "txt", "o"][j % 6];
            let mut f = fs::File::create(dir.join(format!("f{j}.{ext}"))).unwrap();
            f.write_all(&[b'x'; 256][..len]).unwrap();
        }
    });
    fs::write(marker, b"").unwrap();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("a folder for the generated tree");
    let files: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let runs: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let root = Path::new(&root);
    let t = Instant::now();
    make_tree(root, files);
    eprintln!("tree ready in {:.1}s", t.elapsed().as_secs_f64());
    let root = root.canonicalize().unwrap();

    let mut times = Vec::new();
    let (mut count, mut dirs) = (0, 0);
    // One warm-up pass so every run reads from the same, cached state.
    for i in 0..=runs {
        let opts = ScanOptions::new("bench", &root, &root);
        let t = Instant::now();
        let o = scan(&opts, &AtomicBool::new(false), &|_| {}).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        (count, dirs) = (o.summary.file_count, o.summary.dir_count);
        if i > 0 {
            times.push(ms);
        }
    }
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    let runs: Vec<String> = times.iter().map(|t| format!("{t:.1}")).collect();
    println!(
        "{{\"files\":{count},\"dirs\":{dirs},\"scanMs\":{median:.1},\"runsMs\":[{}]}}",
        runs.join(",")
    );
}
