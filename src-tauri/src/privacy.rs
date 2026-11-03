//! Personal data that macOS protects, and whether DiskPilot can read it.
//!
//! With Full Disk Access, the scan measures folders such as Mail, Messages and
//! the Photos library, which are often among the largest on a Mac. DiskPilot
//! only reads sizes and keeps the results on this Mac. Their contents are
//! still never shared with the optional assistant: it sees each private area
//! as one total, never the names of the files inside.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Private areas, relative to the home folder.
const PRIVATE_AREAS: &[(&str, &str)] = &[
    ("Library/Mail", "Mail"),
    ("Library/Containers/com.apple.mail", "Mail"),
    ("Library/Messages", "Messages"),
    ("Library/Containers/com.apple.MobileSMS", "Messages"),
    ("Library/Mobile Documents", "iCloud Drive"),
    ("Library/Safari", "Safari"),
    ("Library/Containers/com.apple.Safari", "Safari"),
    ("Library/Cookies", "Cookies"),
    ("Library/Accounts", "Accounts"),
    ("Library/Calendars", "Calendars"),
    ("Library/Application Support/AddressBook", "Contacts"),
    ("Library/Group Containers/group.com.apple.notes", "Notes"),
    ("Library/Containers/com.apple.Notes", "Notes"),
    ("Library/Application Support/CallHistoryDB", "Call history"),
    (
        "Library/Application Support/CallHistoryTransactions",
        "Call history",
    ),
    ("Library/Biome", "Usage history"),
    ("Library/Suggestions", "Usage history"),
    ("Library/IdentityServices", "Messages"),
    (".Trash", "Trash"),
];
/// Bundles that hold a personal library wherever they are stored.
const PRIVATE_BUNDLES: &[(&str, &str)] = &[
    ("photoslibrary", "Photos library"),
    ("musiclibrary", "Music library"),
    ("tvlibrary", "TV library"),
];

/// The private area containing `path`: its folder and a short name.
pub fn private_area(path: &Path, home: &Path) -> Option<(PathBuf, &'static str)> {
    for (rel, label) in PRIVATE_AREAS {
        let root = home.join(rel);
        if starts_with_ignoring_case(path, &root) {
            return Some((root, label));
        }
    }
    let mut current = PathBuf::new();
    for comp in path.components() {
        current.push(comp);
        let ext = current
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if let Some((_, label)) = PRIVATE_BUNDLES
            .iter()
            .find(|(e, _)| Some(*e) == ext.as_deref())
        {
            return Some((current, label));
        }
    }
    None
}

/// `Path::starts_with`, but ignoring ASCII case in each component. macOS
/// volumes are case-insensitive by default, so `~/library/mail` is the Mail
/// folder too and must count as private.
fn starts_with_ignoring_case(path: &Path, prefix: &Path) -> bool {
    let mut path = path.components();
    prefix.components().all(|want| {
        path.next().is_some_and(|got| {
            let (got, want) = (got.as_os_str(), want.as_os_str());
            match (got.to_str(), want.to_str()) {
                (Some(g), Some(w)) => g.eq_ignore_ascii_case(w),
                _ => got == want,
            }
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FullDiskAccess {
    Granted,
    Denied,
    /// The check could not tell (the probe file does not exist).
    Unknown,
}

/// Whether this process has Full Disk Access. macOS offers no API for this;
/// the usual check is whether the user's privacy database can be opened,
/// which only Full Disk Access allows. The file is opened, never read.
pub fn full_disk_access(home: &Path) -> FullDiskAccess {
    let probes = [
        home.join("Library/Application Support/com.apple.TCC/TCC.db"),
        home.join("Library/Safari/Bookmarks.plist"),
    ];
    let mut denied = false;
    for p in probes {
        match std::fs::File::open(&p) {
            Ok(_) => return FullDiskAccess::Granted,
            Err(e) if e.kind() == ErrorKind::PermissionDenied => denied = true,
            Err(_) => {}
        }
    }
    if denied {
        FullDiskAccess::Denied
    } else {
        FullDiskAccess::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_private_areas() {
        let home = Path::new("/Users/a");
        let (root, label) = private_area(
            Path::new("/Users/a/Library/Mail/V10/x/Attachments/p.pdf"),
            home,
        )
        .unwrap();
        assert_eq!(
            (root.as_path(), label),
            (Path::new("/Users/a/Library/Mail"), "Mail")
        );
        let (root, label) = private_area(
            Path::new("/Volumes/X/Pics/Family.photoslibrary/originals/1.heic"),
            home,
        )
        .unwrap();
        assert_eq!(root, Path::new("/Volumes/X/Pics/Family.photoslibrary"));
        assert_eq!(label, "Photos library");
        assert!(private_area(Path::new("/Users/a/Library/Caches/x"), home).is_none());
        assert!(private_area(Path::new("/Users/a/Library"), home).is_none());
        // macOS volumes are case-insensitive: a different spelling is the same folder.
        for p in [
            "/Users/a/library/mail/V10",
            "/Users/a/LIBRARY/Messages/x",
            "/users/A/Library/mobile documents",
        ] {
            assert!(private_area(Path::new(p), home).is_some(), "{p}");
        }
        assert!(private_area(Path::new("/Users/a/Library/Mailbox"), home).is_none());
    }
}
