//! Central path-safety policy.
//!
//! Every path that arrives from the frontend is validated here before any
//! filesystem call is made with it. The (future) cleanup executor must route
//! every destructive operation through [`authorize_destructive`]; nothing else
//! in the codebase is allowed to delete files.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SafetyError {
    #[error("Path must be absolute: {0}")]
    NotAbsolute(String),
    #[error("Path must not contain '..' or '.' segments: {0}")]
    Traversal(String),
    #[error("Path contains invalid characters")]
    InvalidCharacters,
    #[error("Path is outside the scanned folder: {0}")]
    OutsideRoot(String),
    #[error("Path is protected and can never be modified: {0}")]
    Protected(String),
    #[error("Path is a symbolic link; DiskPilot does not follow links: {0}")]
    Symlink(String),
    #[error("Path does not exist: {0}")]
    Missing(String),
}

/// System locations that DiskPilot must never modify, regardless of user input.
pub const PROTECTED_PREFIXES: &[&str] = &[
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/private/etc",
    "/private/var/db",
    "/private/var/vm",
    "/Library/Apple",
    "/Applications",
    "/Volumes",
    "/dev",
    "/cores",
    "/private",
    "/Library",
    "/opt",
    "/var",
    "/etc",
];

/// Folders directly under home that are never deletable as a whole
/// (their contents may be, after review).
pub const PROTECTED_HOME_CHILDREN: &[&str] = &[
    "Library",
    "Documents",
    "Desktop",
    "Downloads",
    "Pictures",
    "Movies",
    "Music",
    "Public",
    "Applications",
    ".ssh",
    ".gnupg",
    "Library/Keychains",
    "Library/Mobile Documents",
    "Library/CloudStorage",
    "Library/Application Support",
    "Library/Containers",
    "Library/Group Containers",
    "Library/Preferences",
    "Library/Caches",
];

/// Parse a path received from the UI: absolute, no `.`/`..`, no NUL bytes.
pub fn parse_user_path(raw: &str) -> Result<PathBuf, SafetyError> {
    if raw.contains('\0') {
        return Err(SafetyError::InvalidCharacters);
    }
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err(SafetyError::NotAbsolute(raw.to_string()));
    }
    // `Path::components` silently drops interior `.` segments, so inspect the
    // raw segments as well.
    if raw.split('/').any(|seg| seg == "." || seg == "..") {
        return Err(SafetyError::Traversal(raw.to_string()));
    }
    for comp in path.components() {
        match comp {
            Component::ParentDir | Component::CurDir => {
                return Err(SafetyError::Traversal(raw.to_string()))
            }
            Component::Prefix(_) => return Err(SafetyError::NotAbsolute(raw.to_string())),
            Component::RootDir | Component::Normal(_) => {}
        }
    }
    // Re-collect components to drop trailing slashes and duplicate separators.
    Ok(path.components().collect())
}

/// Ensure `path` is `root` or inside it (component-wise, so `/a/bc` is not
/// inside `/a/b`).
pub fn ensure_within(path: &Path, root: &Path) -> Result<(), SafetyError> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(SafetyError::OutsideRoot(path.display().to_string()))
    }
}

/// True for paths that must never be deleted or modified.
pub fn is_protected(path: &Path, home: &Path) -> bool {
    if path.parent().is_none() || path == home {
        return true;
    }
    // Any ancestor of home (e.g. /Users) is protected.
    if home.starts_with(path) {
        return true;
    }
    if PROTECTED_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        return true;
    }
    PROTECTED_HOME_CHILDREN
        .iter()
        .any(|child| path == home.join(child))
}

/// The gate every destructive operation must pass. Validates the raw path,
/// confines it to `allowed_root`, rejects protected locations and refuses to
/// operate on a path whose final component is a symbolic link.
///
/// Phase 1 has no executor yet; this function exists (and is tested) so the
/// executor is built on top of it rather than retrofitted later.
pub fn authorize_destructive(
    raw: &str,
    allowed_root: &Path,
    home: &Path,
) -> Result<PathBuf, SafetyError> {
    let path = parse_user_path(raw)?;
    ensure_within(&path, allowed_root)?;
    if is_protected(&path, home) {
        return Err(SafetyError::Protected(path.display().to_string()));
    }
    let meta = path
        .symlink_metadata()
        .map_err(|_| SafetyError::Missing(path.display().to_string()))?;
    if meta.file_type().is_symlink() {
        return Err(SafetyError::Symlink(path.display().to_string()));
    }
    // Every ancestor between the root and the target must be a real directory,
    // otherwise a symlinked parent could redirect the operation elsewhere.
    let mut current = allowed_root.to_path_buf();
    if let Ok(rel) = path.strip_prefix(allowed_root) {
        let mut comps: Vec<_> = rel.components().collect();
        comps.pop();
        for comp in comps {
            current.push(comp);
            let m = current
                .symlink_metadata()
                .map_err(|_| SafetyError::Missing(current.display().to_string()))?;
            if m.file_type().is_symlink() {
                return Err(SafetyError::Symlink(current.display().to_string()));
            }
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/test";

    #[test]
    fn rejects_relative_and_traversal_paths() {
        assert!(matches!(
            parse_user_path("foo/bar"),
            Err(SafetyError::NotAbsolute(_))
        ));
        assert!(matches!(
            parse_user_path("/Users/test/../../etc"),
            Err(SafetyError::Traversal(_))
        ));
        assert!(matches!(
            parse_user_path("/Users/./test"),
            Err(SafetyError::Traversal(_))
        ));
        assert!(matches!(
            parse_user_path("/a\0b"),
            Err(SafetyError::InvalidCharacters)
        ));
    }

    #[test]
    fn normalizes_separators() {
        assert_eq!(
            parse_user_path("/Users//test/Library/").unwrap(),
            PathBuf::from("/Users/test/Library")
        );
    }

    #[test]
    fn within_is_component_wise() {
        let root = Path::new("/Users/test/code");
        assert!(ensure_within(Path::new("/Users/test/code/app"), root).is_ok());
        assert!(ensure_within(root, root).is_ok());
        assert!(ensure_within(Path::new("/Users/test/codebase"), root).is_err());
        assert!(ensure_within(Path::new("/Users/test"), root).is_err());
    }

    #[test]
    fn protects_system_and_home_roots() {
        let home = Path::new(HOME);
        for p in [
            "/",
            "/Users",
            HOME,
            "/System/Library",
            "/usr/local/bin",
            "/Applications/Safari.app",
            "/Users/test/Documents",
            "/Users/test/Library",
            "/Users/test/Library/Caches",
            "/Users/test/.ssh",
            "/private/var/folders/ab/cd",
            "/Library/Caches",
            "/opt/homebrew",
        ] {
            assert!(is_protected(Path::new(p), home), "{p} should be protected");
        }
    }

    #[test]
    fn allows_specific_targets_inside_home() {
        let home = Path::new(HOME);
        for p in [
            "/Users/test/Library/Caches/com.example.app",
            "/Users/test/code/app/node_modules",
            "/Users/test/Library/Developer/Xcode/DerivedData/Proj-abc",
        ] {
            assert!(
                !is_protected(Path::new(p), home),
                "{p} should not be protected"
            );
        }
    }

    #[test]
    fn authorize_rejects_outside_root_and_protected() {
        let home = Path::new(HOME);
        let root = Path::new("/Users/test/code");
        assert!(matches!(
            authorize_destructive("/Users/test/Documents/x", root, home),
            Err(SafetyError::OutsideRoot(_))
        ));
        assert!(matches!(
            authorize_destructive("/Users/test/Library", Path::new(HOME), home),
            Err(SafetyError::Protected(_))
        ));
    }

    #[test]
    fn authorize_rejects_symlinks_and_symlinked_parents() {
        let dir = crate::test_support::sandbox();
        let root = dir.path().canonicalize().unwrap();
        let home = Path::new("/nonexistent-home");
        let real = root.join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("file"), b"x").unwrap();
        std::os::unix::fs::symlink(&real, root.join("link")).unwrap();

        let ok = authorize_destructive(real.join("file").to_str().unwrap(), &root, home);
        assert!(ok.is_ok(), "{ok:?}");
        assert!(matches!(
            authorize_destructive(root.join("link").to_str().unwrap(), &root, home),
            Err(SafetyError::Symlink(_))
        ));
        assert!(matches!(
            authorize_destructive(root.join("link/file").to_str().unwrap(), &root, home),
            Err(SafetyError::Symlink(_))
        ));
        assert!(matches!(
            authorize_destructive(root.join("missing").to_str().unwrap(), &root, home),
            Err(SafetyError::Missing(_))
        ));
    }
}
