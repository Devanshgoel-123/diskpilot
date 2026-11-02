//! Test helpers.

/// A temporary directory outside protected system locations. The system temp
/// dir lives under `/private/var/folders`, which the safety layer (correctly)
/// refuses to modify.
pub fn sandbox() -> tempfile::TempDir {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-sandbox");
    std::fs::create_dir_all(&base).expect("create sandbox base");
    tempfile::tempdir_in(base).expect("create sandbox")
}
