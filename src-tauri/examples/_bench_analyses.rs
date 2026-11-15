//! Times the analyses that can run next to a scan, without a scan index.
//! Usage: cargo run --release --example _bench_analyses
use diskpilot_lib::classifier::Classifier;
use diskpilot_lib::{agents, devtools};
use std::time::Instant;
fn main() {
    let home = dirs::home_dir().unwrap();
    let now = diskpilot_lib::scanner::now_ms();
    let ctx = devtools::Ctx {
        home: &home,
        index: None,
        summary: None,
        classifier: Classifier::new(&home),
        now_ms: now,
    };
    let t = Instant::now();
    let (dev, _) = devtools::analyze(&ctx);
    println!("devtools: {:.2}s", t.elapsed().as_secs_f64());
    let t = Instant::now();
    let _ = agents::analyze(&agents::Input {
        ctx: &ctx,
        data_dir: None,
        tools: &dev.tools,
    });
    println!("agents:   {:.2}s", t.elapsed().as_secs_f64());
}
