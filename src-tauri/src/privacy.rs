//! Personal data that macOS protects, and whether DiskPilot can read it.
//! On Windows, the same idea covers mail, credentials and chat data in the
//! profile's AppData folders.
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
    ("Library/Application Support/MobileSync", "iPhone backups"),
    (".Trash", "Trash"),
];
/// Private areas on Windows, relative to the profile folder.
const WINDOWS_PRIVATE_AREAS: &[(&str, &str)] = &[
    ("AppData/Local/Microsoft/Outlook", "Mail"),
    ("AppData/Roaming/Microsoft/Outlook", "Mail"),
    ("Documents/Outlook Files", "Mail"),
    (
        "AppData/Local/Packages/microsoft.windowscommunicationsapps_8wekyb3d8bbwe",
        "Mail",
    ),
    (
        "AppData/Local/Packages/Microsoft.OutlookForWindows_8wekyb3d8bbwe",
        "Mail",
    ),
    ("AppData/Local/Comms", "Mail"),
    ("AppData/Roaming/Thunderbird", "Mail"),
    ("AppData/Roaming/Microsoft/Credentials", "Accounts"),
    ("AppData/Local/Microsoft/Credentials", "Accounts"),
    ("AppData/Roaming/Microsoft/Protect", "Accounts"),
    ("AppData/Roaming/Microsoft/Vault", "Accounts"),
    ("AppData/Local/Microsoft/Vault", "Accounts"),
    ("AppData/Roaming/Microsoft/Windows/Recent", "Usage history"),
    ("AppData/Local/ConnectedDevicesPlatform", "Usage history"),
    ("AppData/Local/Microsoft/Windows/History", "Usage history"),
    ("AppData/Local/Microsoft/Windows/INetCookies", "Cookies"),
    (
        "AppData/Local/Packages/Microsoft.YourPhone_8wekyb3d8bbwe",
        "Messages",
    ),
    (
        "AppData/Local/Packages/5319275A.WhatsAppDesktop_cv1g1gvanyjgm",
        "Messages",
    ),
    ("AppData/Roaming/Telegram Desktop", "Messages"),
    (
        "AppData/Local/Packages/Microsoft.MicrosoftStickyNotes_8wekyb3d8bbwe",
        "Notes",
    ),
    (
        "AppData/Roaming/Apple Computer/MobileSync",
        "iPhone backups",
    ),
    ("Apple/MobileSync", "iPhone backups"),
];
/// Bundles that hold a personal library wherever they are stored.
const PRIVATE_BUNDLES: &[(&str, &str)] = &[
    ("photoslibrary", "Photos library"),
    ("musiclibrary", "Music library"),
    ("tvlibrary", "TV library"),
];

/// The private area containing `path`: its folder and a short name.
pub fn private_area(path: &Path, home: &Path) -> Option<(PathBuf, &'static str)> {
    let areas = if cfg!(windows) {
        WINDOWS_PRIVATE_AREAS
    } else {
        PRIVATE_AREAS
    };
    private_area_in(areas, path, home)
}

fn private_area_in(
    areas: &[(&str, &'static str)],
    path: &Path,
    home: &Path,
) -> Option<(PathBuf, &'static str)> {
    for (rel, label) in areas {
        let root = rel.split('/').fold(home.to_path_buf(), |p, c| p.join(c));
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
/// folder too and must count as private. NTFS ignores case as well.
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

    #[test]
    fn recognizes_windows_private_areas() {
        // The table is plain data, so it is checked here with any separator.
        let home = Path::new("/Users/a");
        let area = |p: &str| private_area_in(WINDOWS_PRIVATE_AREAS, Path::new(p), home);
        let (root, label) = area("/Users/a/AppData/Local/Microsoft/Outlook/me.ost").unwrap();
        assert_eq!(label, "Mail");
        assert_eq!(root, Path::new("/Users/a/AppData/Local/Microsoft/Outlook"));
        assert_eq!(
            area("/Users/a/appdata/roaming/microsoft/credentials/x").map(|a| a.1),
            Some("Accounts")
        );
        assert_eq!(
            area("/Users/a/AppData/Roaming/Telegram Desktop/tdata").map(|a| a.1),
            Some("Messages")
        );
        assert!(area("/Users/a/AppData/Local/Temp/x").is_none());
        assert!(area("/Users/a/AppData").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windows_private_areas_are_used_on_windows() {
        let home = Path::new(r"C:\Users\a");
        let (_, label) = private_area(
            Path::new(r"C:\Users\a\AppData\Local\Microsoft\Outlook\a.ost"),
            home,
        )
        .unwrap();
        assert_eq!(label, "Mail");
        assert!(private_area(Path::new(r"C:\Users\a\Library\Mail"), home).is_none());
    }
}
