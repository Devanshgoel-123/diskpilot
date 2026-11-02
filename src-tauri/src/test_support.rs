//! Test helpers.

/// A temporary directory outside protected system locations. The system temp
/// dir lives under `/private/var/folders`, which the safety layer (correctly)
/// refuses to modify.
pub fn sandbox() -> tempfile::TempDir {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-sandbox");
    std::fs::create_dir_all(&base).expect("create sandbox base");
    tempfile::tempdir_in(base).expect("create sandbox")
}

/// A minimal actionable recommendation for tests; override fields as needed.
pub fn recommendation() -> crate::model::Recommendation {
    use crate::model::{Category, Classification, CleanupMethod, Confidence, Tier};
    crate::model::Recommendation {
        id: String::new(),
        title: String::new(),
        path: String::new(),
        bytes: 10,
        file_count: 1,
        tier: Tier::LikelySafe,
        confidence: Confidence::High,
        classification: Classification {
            category: Category::Cache,
            label: String::new(),
            explanation: String::new(),
            rule_id: String::new(),
            confidence: Confidence::High,
            matched_path: None,
        },
        detected_because: String::new(),
        safe_because: String::new(),
        risks: String::new(),
        proposed_action: String::new(),
        methods: vec![CleanupMethod::Trash],
        alternative: None,
        estimated_reclaim_bytes: 10,
        last_activity_ms: None,
        notes: vec![],
        source: "scan".into(),
    }
}
