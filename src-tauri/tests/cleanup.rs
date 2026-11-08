//! End-to-end: scan → recommend → plan → execute on a disposable fixture.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use diskpilot_lib::classifier::Classifier;
use diskpilot_lib::cleanup::{executor, planner};
use diskpilot_lib::model::{ActionStatus, CleanupMethod, Tier};
use diskpilot_lib::recommend::recommend;
use diskpilot_lib::scanner::{scan, ScanOptions};

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![1u8; bytes]).unwrap();
}

#[test]
fn full_pipeline_removes_only_planned_regenerable_data() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let home = diskpilot_lib::platform::canonicalize(dir.path())
        .unwrap()
        .join("home");
    // 60 MB of Xcode DerivedData (likely safe), a node_modules (review first),
    // and personal documents (no recommendation).
    write(
        &home.join("Library/Developer/Xcode/DerivedData/App/big.o"),
        60_000_000,
    );
    write(&home.join("code/web/package.json"), 10);
    write(&home.join("code/web/node_modules/dep/blob.js"), 60_000_000);
    write(&home.join("Documents/thesis.pdf"), 60_000_000);

    let mut opts = ScanOptions::new("e2e", &home, &home);
    opts.inherit_ancestor_context = false;
    let outcome = scan(&opts, &AtomicBool::new(false), &|_| {}).unwrap();
    let classifier = Classifier::new(&home).with_floor(&home);
    let recs = recommend(&outcome.summary, &outcome.index, &classifier, 0);

    let derived = recs
        .iter()
        .find(|r| r.path.ends_with("DerivedData"))
        .expect("derived data rec");
    assert_eq!(derived.tier, Tier::LikelySafe);
    let nm = recs
        .iter()
        .find(|r| r.path.ends_with("node_modules"))
        .expect("node_modules rec");
    assert_eq!(nm.tier, Tier::ReviewFirst);
    assert!(
        !recs
            .iter()
            .any(|r| Path::new(&r.path).starts_with(home.join("Documents"))),
        "documents are never recommended"
    );

    // node_modules does not allow permanent deletion: it is rejected, not removed.
    let ids = vec![derived.id.clone(), nm.id.clone()];
    let plan = planner::build_plan(
        "p".into(),
        &recs,
        &ids,
        CleanupMethod::Delete,
        &home,
        &home,
        0,
    );
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.rejected.len(), 1);

    let record = executor::execute(&plan, &home, &home, &executor::RealFileOps);
    assert_eq!(record.status, ActionStatus::Succeeded);
    assert!(record.bytes_freed >= 60_000_000);
    assert!(!home.join("Library/Developer/Xcode/DerivedData").exists());
    assert!(home.join("Library/Developer/Xcode").exists());
    assert!(home.join("code/web/node_modules/dep/blob.js").exists());
    assert!(home.join("Documents/thesis.pdf").exists());
}
