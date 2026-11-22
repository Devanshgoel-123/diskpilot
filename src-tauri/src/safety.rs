//! Central path-safety policy.
//!
//! Every path that arrives from the frontend is validated here before any
//! filesystem call is made with it. The (future) cleanup executor must route
//! every destructive operation through [`authorize_destructive`]; nothing else
//! in the codebase is allowed to delete files.

#[cfg(not(windows))]
use std::path::Component;
use std::path::{Path, PathBuf};

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
    #[error("Path is not written in its full, exact form: {0}")]
    NotCanonical(String),
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
    #[cfg(windows)]
    {
        windows::normalize(raw).map(PathBuf::from)
    }
    #[cfg(not(windows))]
    parse_unix_path(raw)
}

#[cfg(not(windows))]
fn parse_unix_path(raw: &str) -> Result<PathBuf, SafetyError> {
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
    #[cfg(windows)]
    let inside = windows::starts_with(&path.to_string_lossy(), &root.to_string_lossy());
    #[cfg(not(windows))]
    let inside = path.starts_with(root);
    if inside {
        Ok(())
    } else {
        Err(SafetyError::OutsideRoot(path.display().to_string()))
    }
}

/// True for paths that must never be deleted or modified.
pub fn is_protected(path: &Path, home: &Path) -> bool {
    #[cfg(windows)]
    {
        windows::is_protected(
            &path.to_string_lossy(),
            &home.to_string_lossy(),
            &windows::system_folders_from_env(),
        )
    }
    #[cfg(not(windows))]
    is_protected_unix(path, home)
}

#[cfg(not(windows))]
fn is_protected_unix(path: &Path, home: &Path) -> bool {
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
    #[cfg(windows)]
    {
        windows::check_ancestors(&path, allowed_root)?;
        // A short (8.3) name such as `PROGRA~1`, a different case or a
        // trailing dot could name a protected folder in a way the checks
        // above do not recognize. Only the path exactly as Windows spells it
        // is accepted.
        let canonical = std::fs::canonicalize(&path)
            .map_err(|_| SafetyError::Missing(path.display().to_string()))?;
        if !windows::same_path_exactly(&canonical.to_string_lossy(), &path.to_string_lossy()) {
            return Err(SafetyError::NotCanonical(path.display().to_string()));
        }
        Ok(path)
    }
    #[cfg(not(windows))]
    {
        check_unix_ancestors(&path, allowed_root)?;
        Ok(path)
    }
}

#[cfg(not(windows))]
fn check_unix_ancestors(path: &Path, allowed_root: &Path) -> Result<(), SafetyError> {
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
    Ok(())
}

/// The Windows side of the gate, written as text processing on paths so it
/// is tested on every platform. Names compare ignoring case, `\\?\` and
/// `\\?\UNC\` prefixes are ignored, and both separators are accepted.
pub mod windows {
    use super::SafetyError;
    use crate::classifier::windows_paths::{eq_ignore_case, part_matches, WinPath};

    /// Folders at the root of every drive that are never modified.
    pub const PROTECTED_DRIVE_FOLDERS: &[&str] = &[
        "Windows",
        "Program Files",
        "Program Files (x86)",
        "ProgramData",
        "Recovery",
        "System Volume Information",
        "$Recycle.Bin",
        "Boot",
        "EFI",
        "PerfLogs",
        "Documents and Settings",
        "Config.Msi",
        "Windows.old",
        "$WINDOWS.~BT",
        "$WINDOWS.~WS",
        "$GetCurrent",
        "$WinREAgent",
        "$SysReset",
        "OneDriveTemp",
        "hiberfil.sys",
        "pagefile.sys",
        "swapfile.sys",
        "bootmgr",
        "BOOTNXT",
        "DumpStack.log",
        "DumpStack.log.tmp",
    ];

    /// Folders in the profile that are never removed as a whole (their
    /// contents may be, after review). `*` ends a name prefix.
    pub const PROTECTED_HOME_FOLDERS: &[&[&str]] = &[
        &["AppData"],
        &["AppData", "Local"],
        &["AppData", "LocalLow"],
        &["AppData", "Roaming"],
        &["AppData", "Local", "Packages"],
        &["AppData", "Local", "Temp"],
        &["AppData", "Local", "Microsoft"],
        &["AppData", "Local", "Programs"],
        &["AppData", "Roaming", "Microsoft"],
        &["AppData", "Roaming", "Microsoft", "Windows"],
        &["Documents"],
        &["Desktop"],
        &["Downloads"],
        &["Pictures"],
        &["Videos"],
        &["Music"],
        &["Favorites"],
        &["Contacts"],
        &["Links"],
        &["Saved Games"],
        &["Searches"],
        &["3D Objects"],
        &["OneDrive*"],
        &["iCloudDrive"],
        &["Dropbox"],
        &["Google Drive"],
        &[".ssh"],
        &[".gnupg"],
        &["NTUSER.DAT*"],
        &["ntuser.ini"],
        &["Application Data"],
        &["Local Settings"],
        &["My Documents"],
        &["Cookies"],
        &["NetHood"],
        &["PrintHood"],
        &["Recent"],
        &["SendTo"],
        &["Start Menu"],
        &["Templates"],
    ];

    /// Places in the profile where nothing is ever modified: keys,
    /// credentials and certificates.
    pub const PROTECTED_HOME_TREES: &[&[&str]] = &[
        &[".ssh"],
        &[".gnupg"],
        &["AppData", "Roaming", "Microsoft", "Protect"],
        &["AppData", "Roaming", "Microsoft", "Credentials"],
        &["AppData", "Roaming", "Microsoft", "Crypto"],
        &["AppData", "Roaming", "Microsoft", "SystemCertificates"],
        &["AppData", "Roaming", "Microsoft", "Vault"],
        &["AppData", "Local", "Microsoft", "Credentials"],
        &["AppData", "Local", "Microsoft", "Vault"],
    ];

    /// The system folders named by the environment, in case Windows or
    /// the program folders are somewhere unusual.
    #[cfg(windows)]
    pub fn system_folders_from_env() -> Vec<String> {
        [
            "SystemRoot",
            "windir",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "ProgramData",
            "CommonProgramFiles",
            "CommonProgramFiles(x86)",
        ]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .filter(|v| !v.is_empty())
        .collect()
    }

    /// Validate and normalize a path from the interface: an absolute drive
    /// (`C:\...`) or network (`\\server\share\...`) path, with `\\?\`
    /// prefixes kept, no `.` or `..`, no alternate data streams, no
    /// device paths and no names ending in a dot or space (which Windows
    /// would silently strip).
    pub fn normalize(raw: &str) -> Result<String, SafetyError> {
        let err_abs = || SafetyError::NotAbsolute(raw.to_string());
        if raw.starts_with("\\\\.\\") || raw.starts_with("//./") {
            return Err(err_abs());
        }
        let p = WinPath::parse(raw);
        let unc = p.parts.first() == Some(&"\\\\");
        let drive = !unc && p.parts.first().is_some_and(|d| is_drive(d));
        let names = if unc {
            if p.parts.len() < 3 {
                return Err(err_abs());
            }
            &p.parts[1..]
        } else if drive {
            // `C:` alone or `C:folder` is relative to the drive's current folder.
            let after_drive = raw_after_drive(raw);
            if !after_drive.starts_with(['\\', '/']) {
                return Err(err_abs());
            }
            &p.parts[1..]
        } else {
            return Err(err_abs());
        };
        for name in names {
            if *name == "." || *name == ".." {
                return Err(SafetyError::Traversal(raw.to_string()));
            }
            if name.contains(':') || name.contains(['<', '>', '"', '|', '?', '*']) {
                return Err(SafetyError::InvalidCharacters);
            }
            if name.ends_with(['.', ' ']) {
                return Err(SafetyError::NotCanonical(raw.to_string()));
            }
        }
        // A `\\?\` path stays one, so long paths keep working; every check
        // here ignores the prefix.
        let verbatim = raw.starts_with("\\\\?\\");
        let mut out = String::with_capacity(raw.len() + 8);
        if unc && verbatim {
            out.push_str("\\\\?\\UNC\\");
            out.push_str(&p.parts[1..].join("\\"));
        } else if unc {
            out.push_str("\\\\");
            out.push_str(&p.parts[1..].join("\\"));
        } else {
            if verbatim {
                out.push_str("\\\\?\\");
            }
            out.push_str(p.parts[0]);
            out.push('\\');
            out.push_str(&p.parts[1..].join("\\"));
        }
        Ok(out)
    }

    fn raw_after_drive(raw: &str) -> &str {
        let s = raw.strip_prefix("\\\\?\\").unwrap_or(raw);
        s.get(2..).unwrap_or("")
    }

    fn is_drive(part: &str) -> bool {
        let b = part.as_bytes();
        b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
    }

    /// `path` is `root` or inside it, ignoring case and `\\?\` prefixes.
    pub fn starts_with(path: &str, root: &str) -> bool {
        let root = WinPath::parse(root);
        !root.parts.is_empty() && WinPath::parse(path).starts_with_parts(&root.parts)
    }

    /// The same path, spelled the same way apart from letter case and a
    /// `\\?\` prefix.
    pub fn same_path_exactly(a: &str, b: &str) -> bool {
        let (a, b) = (WinPath::parse(a), WinPath::parse(b));
        a.parts.len() == b.parts.len() && a.starts_with_parts(&b.parts)
    }

    /// True for paths DiskPilot must never delete or modify on Windows.
    pub fn is_protected(path: &str, home: &str, system_folders: &[String]) -> bool {
        let p = WinPath::parse(path);
        let home = WinPath::parse(home);
        let unc = p.parts.first() == Some(&"\\\\");
        let root_len = if unc { 3 } else { 1 };
        // A drive or network share root, or anything that is not a full path.
        if p.parts.len() <= root_len || !(unc || is_drive(p.parts[0])) {
            return true;
        }
        // The profile folder and every folder above it (C:\Users).
        if home.starts_with_parts(&p.parts) {
            return true;
        }
        if PROTECTED_DRIVE_FOLDERS
            .iter()
            .any(|name| eq_ignore_case(p.parts[root_len], name))
        {
            return true;
        }
        if system_folders.iter().any(|f| {
            let f = WinPath::parse(f);
            f.parts.len() > 1 && p.starts_with_parts(&f.parts)
        }) {
            return true;
        }
        if !home.parts.is_empty() && p.starts_with_parts(&home.parts) {
            let rel = &p.parts[home.parts.len()..];
            let is = |pattern: &[&str]| {
                pattern.len() == rel.len()
                    && rel.iter().zip(pattern).all(|(n, pat)| part_matches(n, pat))
            };
            let inside = |pattern: &[&str]| {
                rel.len() >= pattern.len()
                    && rel.iter().zip(pattern).all(|(n, pat)| part_matches(n, pat))
            };
            if PROTECTED_HOME_FOLDERS.iter().any(|f| is(f))
                || PROTECTED_HOME_TREES.iter().any(|t| inside(t))
            {
                return true;
            }
        }
        false
    }

    /// Every folder between `root` and the target must be a real folder, not
    /// a link, junction or mount point that could send the operation
    /// elsewhere.
    #[cfg(windows)]
    pub fn check_ancestors(
        path: &std::path::Path,
        root: &std::path::Path,
    ) -> Result<(), SafetyError> {
        let root_text = root.to_string_lossy();
        let root_parts = WinPath::parse(&root_text).parts.len();
        let path_text = path.to_string_lossy();
        let target = WinPath::parse(&path_text);
        if target.parts.len() <= root_parts {
            return Ok(());
        }
        let mut current = std::path::PathBuf::from(normalize(&root_text)?);
        for name in &target.parts[root_parts..target.parts.len() - 1] {
            current.push(name);
            let m = current
                .symlink_metadata()
                .map_err(|_| SafetyError::Missing(current.display().to_string()))?;
            if m.file_type().is_symlink() {
                return Err(SafetyError::Symlink(current.display().to_string()));
            }
        }
        Ok(())
    }
}

/// The macOS gate.
#[cfg(all(test, not(windows)))]
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

    #[cfg(unix)]
    #[test]
    fn authorize_rejects_symlinks_and_symlinked_parents() {
        let dir = crate::test_support::sandbox();
        let root = crate::platform::canonicalize(dir.path()).unwrap();
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

#[cfg(test)]
mod windows_tests {
    //! The Windows gate, checked with Windows paths on every platform.
    use super::windows::*;
    use super::SafetyError;

    const HOME: &str = r"C:\Users\Test";

    fn protected(p: &str) -> bool {
        is_protected(
            p,
            HOME,
            &[r"C:\Windows".to_string(), r"D:\Apps".to_string()],
        )
    }

    #[test]
    fn normalizes_drive_unc_and_verbatim_paths() {
        assert_eq!(normalize(r"C:\Users\Test\").unwrap(), r"C:\Users\Test");
        assert_eq!(normalize("c:/users//test").unwrap(), r"c:\users\test");
        assert_eq!(
            normalize(r"\\?\C:\Users\Test\").unwrap(),
            r"\\?\C:\Users\Test"
        );
        assert_eq!(normalize(r"C:\").unwrap(), r"C:\");
        assert_eq!(normalize(r"\\server\share\x").unwrap(), r"\\server\share\x");
        assert_eq!(
            normalize(r"\\?\UNC\server\share\x").unwrap(),
            r"\\?\UNC\server\share\x"
        );
    }

    #[test]
    fn rejects_relative_device_and_tricky_paths() {
        for p in [r"Users\Test", r"C:Users", "C:", r"\Users\Test", r"\\server"] {
            assert!(
                matches!(normalize(p), Err(SafetyError::NotAbsolute(_))),
                "{p}"
            );
        }
        assert!(matches!(
            normalize(r"\\.\PhysicalDrive0"),
            Err(SafetyError::NotAbsolute(_))
        ));
        for p in [r"C:\Users\..\Windows", r"C:\Users\.\Test"] {
            assert!(
                matches!(normalize(p), Err(SafetyError::Traversal(_))),
                "{p}"
            );
        }
        // Alternate data streams and wildcards.
        for p in [r"C:\Users\Test\a.txt:secret", r"C:\Users\Test\*"] {
            assert!(
                matches!(normalize(p), Err(SafetyError::InvalidCharacters)),
                "{p}"
            );
        }
        // Windows strips a trailing dot or space, so `Windows.` is `Windows`.
        for p in [r"C:\Windows.", r"C:\Windows \System32"] {
            assert!(
                matches!(normalize(p), Err(SafetyError::NotCanonical(_))),
                "{p}"
            );
        }
    }

    #[test]
    fn protects_system_folders_drive_roots_and_profile_folders() {
        for p in [
            r"C:\",
            r"D:\",
            r"\\server\share",
            r"C:\Users",
            HOME,
            r"c:\users\test",
            r"C:\Windows",
            r"c:\WINDOWS\System32\drivers",
            r"\\?\C:\Windows\WinSxS",
            r"C:\Program Files\App",
            r"C:\Program Files (x86)\Steam\steamapps\shadercache",
            r"C:\ProgramData\Package Cache",
            r"C:\Recovery",
            r"E:\System Volume Information",
            r"D:\$Recycle.Bin\S-1-5-21",
            r"C:\Windows.old",
            r"C:\$WINDOWS.~BT\Sources",
            r"C:\hiberfil.sys",
            r"C:\pagefile.sys",
            r"D:\Apps\Tool",
            r"C:\Users\Test\AppData",
            r"C:\Users\Test\AppData\Local",
            r"C:\Users\Test\appdata\roaming",
            r"C:\Users\Test\AppData\Local\Temp",
            r"C:\Users\Test\AppData\Local\Packages",
            r"C:\Users\Test\Documents",
            r"C:\Users\Test\Downloads",
            r"C:\Users\Test\OneDrive - Contoso",
            r"C:\Users\Test\NTUSER.DAT",
            r"C:\Users\Test\.ssh\id_ed25519",
            r"C:\Users\Test\AppData\Roaming\Microsoft\Protect\S-1-5\key",
            r"C:\Users\Test\AppData\Local\Microsoft\Credentials\x",
        ] {
            assert!(protected(p), "{p} should be protected");
        }
    }

    #[test]
    fn allows_specific_targets_inside_the_profile() {
        for p in [
            r"C:\Users\Test\AppData\Local\Temp\setup-123",
            r"C:\Users\Test\AppData\Local\npm-cache",
            r"C:\Users\Test\AppData\Local\Google\Chrome\User Data\Default\Cache",
            r"C:\Users\Test\AppData\Local\Microsoft\Windows\WER",
            r"C:\Users\Test\code\app\node_modules",
            r"C:\Users\Test\Downloads\Setup.exe",
            r"\\?\C:\Users\Test\.nuget\packages",
            r"D:\SteamLibrary\steamapps\shadercache",
            r"D:\Projects\app\target",
        ] {
            assert!(!protected(p), "{p} should not be protected");
        }
    }

    #[test]
    fn containment_ignores_case_and_verbatim_prefixes() {
        assert!(starts_with(r"C:\Users\Test\code\app", r"\\?\c:\users\test"));
        assert!(starts_with(r"C:\Users\Test", r"C:\Users\Test"));
        assert!(!starts_with(r"C:\Users\Testing", r"C:\Users\Test"));
        assert!(!starts_with(r"D:\Users\Test", r"C:\Users\Test"));
        assert!(same_path_exactly(
            r"\\?\C:\Program Files",
            r"c:\program files"
        ));
        assert!(!same_path_exactly(r"C:\Program Files", r"C:\PROGRA~1"));
    }
}

#[cfg(all(test, windows))]
mod windows_gate_tests {
    use super::*;

    #[test]
    fn the_gate_uses_windows_rules_on_windows() {
        let home = Path::new(r"C:\Users\Test");
        assert!(is_protected(Path::new(r"C:\Windows\Temp"), home));
        assert!(is_protected(Path::new(r"c:\program files"), home));
        assert!(!is_protected(
            Path::new(r"C:\Users\Test\AppData\Local\npm-cache"),
            home
        ));
        assert_eq!(
            parse_user_path(r"C:/Users\Test\x\").unwrap(),
            PathBuf::from(r"C:\Users\Test\x")
        );
        assert!(ensure_within(
            Path::new(r"c:\users\test\x"),
            Path::new(r"\\?\C:\Users\Test")
        )
        .is_ok());
    }

    #[test]
    fn authorize_accepts_exact_paths_and_rejects_short_names_and_junctions() {
        let dir = crate::test_support::sandbox();
        let root = crate::platform::canonicalize(dir.path()).unwrap();
        let home = Path::new(r"C:\nonexistent-home");
        let real = root.join("Some Long Folder Name");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("file.txt"), b"x").unwrap();
        let file = real.join("file.txt");
        let ok = authorize_destructive(&file.to_string_lossy(), &root, home);
        assert!(ok.is_ok(), "{ok:?}");
        // A short (8.3) name for the folder is refused, when the volume has them.
        if let Some(short) = short_name(&real) {
            if short != real {
                assert!(matches!(
                    authorize_destructive(&short.join("file.txt").to_string_lossy(), &root, home),
                    Err(SafetyError::NotCanonical(_))
                ));
            }
        }
        // A junction is a link and is never followed.
        let junction = root.join("junction");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&real)
            .status()
            .is_ok_and(|s| s.success());
        if made {
            assert!(matches!(
                authorize_destructive(&junction.to_string_lossy(), &root, home),
                Err(SafetyError::Symlink(_))
            ));
            assert!(matches!(
                authorize_destructive(&junction.join("file.txt").to_string_lossy(), &root, home),
                Err(SafetyError::Symlink(_))
            ));
        }
    }

    fn short_name(path: &Path) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
        let wide = crate::platform::wide(path.as_os_str());
        let mut buf = vec![0u16; 1024];
        let n = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        (n > 0 && (n as usize) < buf.len())
            .then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..n as usize])))
    }
}
