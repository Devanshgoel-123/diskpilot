//! Personal files grouped by type (videos, images, audio and so on), so a
//! person can review, preview and clear one kind of file in one place.
//!
//! Only files a person put somewhere themselves count: files under the home
//! folder that are outside `~/Library`, hidden folders, app and document
//! bundles (a Photos library, a Final Cut library) and dependency folders.
//! Those belong to an app, and removing one file breaks it. The same folders
//! are skipped by the duplicate search.

use std::ffi::OsStr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::duplicates::{skipped_dir_name, SKIPPED_HOME_DIRS};
use crate::model::FileEntry;

/// Files below this size are counted in a type's total but not listed.
pub const MIN_LISTED_BYTES: u64 = 1024 * 1024;
/// Files listed per type, largest first.
pub const LISTED_PER_KIND: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Videos,
    Images,
    Audio,
    Archives,
    Installers,
    Documents,
}

impl FileKind {
    pub const ALL: [FileKind; 6] = [
        FileKind::Videos,
        FileKind::Images,
        FileKind::Audio,
        FileKind::Archives,
        FileKind::Installers,
        FileKind::Documents,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        self as usize
    }

    fn extensions(self) -> &'static [&'static str] {
        match self {
            FileKind::Videos => &[
                "mp4", "m4v", "mov", "mkv", "avi", "wmv", "flv", "webm", "mpg", "mpeg", "3gp",
                "mts", "m2ts",
            ],
            FileKind::Images => &[
                "jpg", "jpeg", "png", "gif", "heic", "heif", "webp", "tif", "tiff", "bmp", "psd",
                "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2",
            ],
            FileKind::Audio => &[
                "mp3", "m4a", "aac", "wav", "aif", "aiff", "flac", "ogg", "opus", "wma", "caf",
            ],
            FileKind::Archives => &["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst"],
            FileKind::Installers => &["dmg", "pkg", "mpkg", "iso", "exe", "msi", "apk", "ipa"],
            FileKind::Documents => &[
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "epub",
            ],
        }
    }

    /// The type of a file, from its extension.
    pub fn of(name: &OsStr) -> Option<FileKind> {
        let name = name.to_str()?;
        if name.starts_with('.') {
            return None;
        }
        let (_, ext) = name.rsplit_once('.')?;
        let ext = ext.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|k| k.extensions().contains(&ext.as_str()))
    }
}

/// Whether files directly inside `dir` are personal files: under `home`,
/// and not inside `~/Library`, a hidden folder, a bundle or a dependency
/// folder.
pub fn personal_dir(dir: &Path, home: &Path) -> bool {
    let Ok(rel) = dir.strip_prefix(home) else {
        return false;
    };
    if SKIPPED_HOME_DIRS.iter().any(|d| rel.starts_with(d)) {
        return false;
    }
    rel.components()
        .all(|c| c.as_os_str().to_str().is_some_and(|n| !skipped_dir_name(n)))
}

/// One type's total and its largest files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileKindGroup {
    pub kind: FileKind,
    pub bytes: u64,
    pub file_count: u64,
    /// Largest first; at most [`LISTED_PER_KIND`], each at least
    /// [`MIN_LISTED_BYTES`].
    pub files: Vec<FileEntry>,
}

/// Recommendations to move the chosen files to the Trash. A path is accepted
/// only when the last scan listed it under a type and it is still the same
/// regular file (same size); anything else is rejected with a reason.
pub fn removal_recommendations(
    groups: &[FileKindGroup],
    selected: &[String],
    classify: &dyn Fn(&Path) -> crate::model::Classification,
) -> (
    Vec<crate::model::Recommendation>,
    Vec<crate::model::PlanRejection>,
) {
    use crate::model::{CleanupMethod, Confidence, PlanRejection, Recommendation, Tier};
    let listed: std::collections::HashMap<&str, &FileEntry> = groups
        .iter()
        .flat_map(|g| g.files.iter().map(|f| (f.path.as_str(), f)))
        .collect();
    let mut recs = Vec::new();
    let mut rejected = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in selected {
        if !seen.insert(p.as_str()) {
            continue;
        }
        let reject = |reason: &str| PlanRejection {
            recommendation_id: p.clone(),
            path: p.clone(),
            reason: reason.to_string(),
        };
        let Some(entry) = listed.get(p.as_str()) else {
            rejected.push(reject("Not listed by the last scan. Scan again."));
            continue;
        };
        let path = Path::new(p);
        // Same size and, within a little rounding, the same modification time:
        // a file replaced by another of the same size is not the one chosen.
        let same_time = |m: &std::fs::Metadata| match entry.modified_ms {
            Some(listed) => m
                .modified()
                .ok()
                .and_then(crate::scanner::to_ms)
                .is_some_and(|now| (now - listed).abs() <= 2_000),
            None => true,
        };
        match path.symlink_metadata() {
            Ok(m) if m.file_type().is_file() && m.len() == entry.logical_bytes && same_time(&m) => {
            }
            Ok(_) => {
                rejected.push(reject("It changed since the scan. Scan again."));
                continue;
            }
            Err(_) => {
                rejected.push(reject("It is no longer there."));
                continue;
            }
        }
        recs.push(Recommendation {
            id: p.clone(),
            title: entry.name.clone(),
            path: p.clone(),
            bytes: entry.bytes,
            file_count: 1,
            tier: Tier::ReviewFirst,
            confidence: Confidence::High,
            classification: classify(path),
            detected_because: "You chose it in Files by type.".into(),
            safe_because:
                "It goes to the Trash, so you can put it back until the Trash is emptied.".into(),
            risks: "This is your own file. DiskPilot cannot tell whether you still need it.".into(),
            proposed_action: "Move to Trash".into(),
            methods: vec![CleanupMethod::Trash],
            alternative: None,
            estimated_reclaim_bytes: entry.bytes,
            last_activity_ms: entry.modified_ms,
            notes: Vec::new(),
            source: "file-kinds".into(),
            marker: None,
            clear_in_app: None,
        });
    }
    (recs, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &Path, bytes: u64) -> FileEntry {
        FileEntry {
            path: path.display().to_string(),
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            bytes,
            logical_bytes: bytes,
            modified_ms: None,
            classification: crate::test_support::recommendation().classification,
        }
    }

    #[test]
    fn only_listed_unchanged_files_are_planned() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let (a, b, c) = (home.join("a.mp4"), home.join("b.mp4"), home.join("c.mp4"));
        std::fs::write(&a, vec![1u8; 10]).unwrap();
        std::fs::write(&b, vec![1u8; 10]).unwrap();
        let groups = vec![FileKindGroup {
            kind: FileKind::Videos,
            bytes: 30,
            file_count: 3,
            files: vec![entry(&a, 10), entry(&b, 9), entry(&c, 10)],
        }];
        let s = |p: &Path| p.display().to_string();
        let other = s(&home.join("other.mp4"));
        let selected = vec![s(&a), s(&a), s(&b), s(&c), other.clone()];
        let classify = |_: &Path| crate::test_support::recommendation().classification;
        let (recs, rejected) = removal_recommendations(&groups, &selected, &classify);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].path, s(&a));
        assert_eq!(recs[0].methods, vec![crate::model::CleanupMethod::Trash]);
        let reasons: Vec<(&str, &str)> = rejected
            .iter()
            .map(|r| (r.path.as_str(), r.reason.as_str()))
            .collect();
        assert_eq!(
            reasons,
            [
                (s(&b).as_str(), "It changed since the scan. Scan again."),
                (s(&c).as_str(), "It is no longer there."),
                (other.as_str(), "Not listed by the last scan. Scan again."),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_swapped_for_a_link_or_folder_is_refused() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let elsewhere = home.join("elsewhere.mp4");
        std::fs::write(&elsewhere, vec![1u8; 10]).unwrap();
        let (link, folder) = (home.join("link.mp4"), home.join("folder.mp4"));
        // Both were 10-byte videos at scan time; now one is a link to another
        // file of the same size, the other a folder.
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        std::fs::create_dir(&folder).unwrap();
        let groups = vec![FileKindGroup {
            kind: FileKind::Videos,
            bytes: 20,
            file_count: 2,
            files: vec![entry(&link, 10), entry(&folder, 10)],
        }];
        let selected: Vec<String> = [&link, &folder]
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let classify = |_: &Path| crate::test_support::recommendation().classification;
        let (recs, rejected) = removal_recommendations(&groups, &selected, &classify);
        assert!(recs.is_empty());
        assert!(rejected
            .iter()
            .all(|r| r.reason == "It changed since the scan. Scan again."));
        assert_eq!(rejected.len(), 2);
        assert!(elsewhere.exists());
    }

    #[test]
    fn a_same_size_file_written_since_the_scan_is_refused() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let a = home.join("a.mp4");
        std::fs::write(&a, vec![1u8; 10]).unwrap();
        let mut listed = entry(&a, 10);
        let now = crate::scanner::to_ms(std::time::SystemTime::now()).unwrap();
        let groups = |e: FileEntry| {
            vec![FileKindGroup {
                kind: FileKind::Videos,
                bytes: 10,
                file_count: 1,
                files: vec![e],
            }]
        };
        let classify = |_: &Path| crate::test_support::recommendation().classification;
        let sel = vec![a.display().to_string()];

        listed.modified_ms = Some(now);
        let (recs, _) = removal_recommendations(&groups(listed.clone()), &sel, &classify);
        assert_eq!(recs.len(), 1, "unchanged file");

        listed.modified_ms = Some(now - 3_600_000); // listed an hour ago
        let (recs, rejected) = removal_recommendations(&groups(listed), &sel, &classify);
        assert!(recs.is_empty());
        assert_eq!(rejected[0].reason, "It changed since the scan. Scan again.");
    }

    #[test]
    fn nothing_selected_plans_nothing() {
        let classify = |_: &Path| crate::test_support::recommendation().classification;
        let (recs, rejected) = removal_recommendations(&[], &[], &classify);
        assert!(recs.is_empty() && rejected.is_empty());
    }

    #[test]
    fn types_come_from_the_extension_in_any_case() {
        assert_eq!(FileKind::of(OsStr::new("trip.MP4")), Some(FileKind::Videos));
        assert_eq!(
            FileKind::of(OsStr::new("IMG_1.heic")),
            Some(FileKind::Images)
        );
        assert_eq!(
            FileKind::of(OsStr::new("Xcode.dmg")),
            Some(FileKind::Installers)
        );
        assert_eq!(
            FileKind::of(OsStr::new("a.tar.gz")),
            Some(FileKind::Archives)
        );
        assert_eq!(FileKind::of(OsStr::new("notes.txt")), None);
        assert_eq!(FileKind::of(OsStr::new("mp4")), None);
        assert_eq!(FileKind::of(OsStr::new(".hidden.mp4")), None);
    }

    #[test]
    fn only_folders_a_person_manages_hold_personal_files() {
        let home = Path::new("/Users/a");
        let yes = [
            "/Users/a",
            "/Users/a/Downloads",
            "/Users/a/Movies/Trips 2025",
            "/Users/a/Music/Recordings",
            "/Users/a/Movies/TV Shows I made",
        ];
        for d in yes {
            assert!(personal_dir(Path::new(d), home), "{d}");
        }
        let no = [
            "/Users/b/Downloads",
            "/Users/a/Library/Caches",
            "/Users/a/.cache",
            "/Users/a/Pictures/Photos Library.photoslibrary/originals/0",
            "/Users/a/Movies/Edit.fcpbundle/Media",
            "/Users/a/code/app/node_modules/x",
            "/Users/a/Applications/Tool.app/Contents",
            "/Users/a/go/pkg/mod",
            "/Users/a/Music/Music/Media.localized/Music/Artist",
            "/Users/a/Music/iTunes/iTunes Media/Music",
            "/Users/a/Movies/TV/Media.localized/Movies",
            "/Users/a/Pictures/Photo Booth Library/Pictures",
            "/Users/a/Pictures/Old.photolibrary/Masters",
            "/Users/a/VMs/Windows.utm/Data",
            "/Users/a/Parallels/Win.pvm",
            "/Users/a/Pictures/Catalog.cocatalog/Originals",
        ];
        for d in no {
            assert!(!personal_dir(Path::new(d), home), "{d}");
        }
    }
}
