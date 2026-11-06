//! Data structures shared between the Rust engine and the frontend.
//!
//! Every struct here is serialized with camelCase field names. The TypeScript
//! mirror lives in `src/lib/types.ts`; keep the two in sync.

use serde::{Deserialize, Serialize};

/// Storage category. Every scanned byte is attributed to exactly one category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Cache,
    BuildArtifacts,
    DeveloperTools,
    DockerStorage,
    ApplicationData,
    Logs,
    Downloads,
    UserData,
    BinaryData,
    SystemFiles,
    Unknown,
}

impl Category {
    pub const ALL: [Category; 11] = [
        Category::Cache,
        Category::BuildArtifacts,
        Category::DeveloperTools,
        Category::DockerStorage,
        Category::ApplicationData,
        Category::Logs,
        Category::Downloads,
        Category::UserData,
        Category::BinaryData,
        Category::SystemFiles,
        Category::Unknown,
    ];

    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        self as usize
    }
}

/// How sure the classifier is that a classification is correct.
/// This is *not* a statement about whether something is safe to delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// The classifier's verdict for a path, always with a human-readable reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category: Category,
    /// Short name of what was detected, e.g. "Xcode DerivedData".
    pub label: String,
    /// Why this classification was made and what the item is.
    pub explanation: String,
    /// Stable identifier of the rule that matched (or "unmatched").
    pub rule_id: String,
    pub confidence: Confidence,
    /// The directory where the rule matched, when the classification is
    /// inherited from an ancestor folder.
    pub matched_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEnvironment {
    pub home_dir: String,
    pub os: String,
    pub arch: String,
    pub app_version: String,
    /// Phase 1 ships no destructive operations at all.
    pub read_only: bool,
    /// Why an update cannot be installed in place, such as running straight
    /// from the disk image; `None` when it can.
    pub update_blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scan_id: String,
    pub files_scanned: u64,
    pub dirs_scanned: u64,
    pub bytes_scanned: u64,
    pub current_path: String,
    pub elapsed_ms: u64,
    /// App containers set aside to measure after everything else. Zero when
    /// the scan has none.
    pub containers_total: u64,
    /// App containers measured so far.
    pub containers_done: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanStatus {
    Completed,
    Cancelled,
    Failed,
}

impl ScanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ScanStatus::Completed => "completed",
            ScanStatus::Cancelled => "cancelled",
            ScanStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> ScanStatus {
        match s {
            "completed" => ScanStatus::Completed,
            "cancelled" => ScanStatus::Cancelled,
            _ => ScanStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFinished {
    pub scan_id: String,
    pub status: ScanStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: Category,
    pub bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    /// Bytes actually allocated on disk.
    pub bytes: u64,
    /// Apparent (logical) file length.
    pub logical_bytes: u64,
    pub modified_ms: Option<i64>,
    pub classification: Classification,
}

/// A directory the classifier recognized, with its total size.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedLocation {
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    /// True for broad areas (e.g. `~/Library/Caches`) that can contain other
    /// detected locations; false for self-contained items (e.g. `node_modules`).
    pub container: bool,
    pub classification: Classification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySize {
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    pub classification: Classification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub id: String,
    pub root: String,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub duration_ms: u64,
    /// Allocated bytes of everything scanned (hard links counted once).
    pub total_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub symlink_count: u64,
    /// Mount points inside the root that were not entered.
    pub skipped_volumes: u64,
    pub error_count: u64,
    pub error_samples: Vec<ScanIssue>,
    pub categories: Vec<CategoryTotal>,
    pub largest_files: Vec<FileEntry>,
    pub detected_locations: Vec<DetectedLocation>,
    /// Folders holding most of their data in their own files (not in
    /// subfolders): where storage physically accumulates.
    pub heavy_directories: Vec<DirectorySize>,
    /// Personal files by type (videos, images, ...), largest types first.
    /// Empty in snapshots saved before 0.6.
    #[serde(default)]
    pub file_kinds: Vec<crate::filekinds::FileKindGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    /// Allocated bytes. `None` when the size is unknown (e.g. the folder was
    /// not readable or lives on another volume).
    pub bytes: Option<u64>,
    pub file_count: Option<u64>,
    pub modified_ms: Option<i64>,
    pub extension: Option<String>,
    pub classification: Classification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    pub path: String,
    pub parent: Option<String>,
    pub scan_root: String,
    pub bytes: Option<u64>,
    pub entries: Vec<DirectoryEntry>,
    /// Entries left out because the folder is very large.
    pub omitted_count: u64,
    pub omitted_bytes: u64,
    pub classification: Classification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetails {
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    pub bytes: Option<u64>,
    pub logical_bytes: Option<u64>,
    pub file_count: Option<u64>,
    pub created_ms: Option<i64>,
    pub modified_ms: Option<i64>,
    pub accessed_ms: Option<i64>,
    pub readonly: bool,
    pub symlink_target: Option<String>,
    pub classification: Classification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRecord {
    pub id: String,
    pub root: String,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub duration_ms: u64,
    pub total_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub error_count: u64,
    pub status: ScanStatus,
}

/// Risk tier of a cleanup recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    /// Low-risk, well-understood, regenerable. Still reviewed and confirmed.
    LikelySafe,
    /// Possibly disposable, but something may depend on it.
    ReviewFirst,
    /// Important or not understood well enough to suggest removal.
    Keep,
}

/// How a planned file cleanup is carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupMethod {
    /// Move to the macOS Trash (recoverable until the Trash is emptied).
    Trash,
    /// Delete permanently. Offered only for regenerable data.
    Delete,
}

impl CleanupMethod {
    pub fn describe(self) -> &'static str {
        match self {
            CleanupMethod::Trash => "Move to Trash",
            CleanupMethod::Delete => "Delete permanently",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    /// Stable within a scan: the item's path.
    pub id: String,
    pub title: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    pub tier: Tier,
    pub confidence: Confidence,
    pub classification: Classification,
    /// What DiskPilot recognized and how.
    pub detected_because: String,
    /// Why removing it is likely fine (empty for "keep").
    pub safe_because: String,
    /// What could go wrong.
    pub risks: String,
    /// Human description of the proposed action.
    pub proposed_action: String,
    /// Methods DiskPilot can perform. Empty means "no automated action".
    pub methods: Vec<CleanupMethod>,
    /// A command the user can run themselves instead (never run by DiskPilot).
    pub alternative: Option<String>,
    pub estimated_reclaim_bytes: u64,
    pub last_activity_ms: Option<i64>,
    pub notes: Vec<String>,
    /// What produced it: "scan" or a developer-tool analyzer id.
    pub source: String,
    /// Set when the item is an older version, a leftover, an installer or a
    /// duplicate, with the evidence in `detail`.
    #[serde(default)]
    pub marker: Option<Marker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerKind {
    /// A newer version of the same thing is installed.
    OlderVersion,
    /// Belongs to an app, project or tool that is no longer there.
    Leftover,
    /// An installer for an app that is already installed.
    Installer,
    /// An identical copy of another file next to it.
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub kind: MarkerKind,
    /// The evidence, e.g. "DataGrip 2025.2 is newer".
    pub detail: String,
}

impl Recommendation {
    pub fn marked(mut self, kind: MarkerKind, detail: impl Into<String>) -> Self {
        self.marker = Some(Marker {
            kind,
            detail: detail.into(),
        });
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub recommendation_id: String,
    pub title: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    pub tier: Tier,
    /// Exactly what will be done, e.g. "Move to Trash".
    pub operation: String,
    /// For an extra copy of a duplicate: the copy that stays. Checked again
    /// right before this item is moved, so the last copy is never removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_copy: Option<String>,
    /// The path was a regular file when the plan was made. Checked again
    /// right before it is removed, so a file replaced by a folder since the
    /// review is not removed in its place.
    #[serde(default)]
    pub was_file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRejection {
    pub recommendation_id: String,
    pub path: String,
    pub reason: String,
}

/// A previewable, single-use cleanup plan. Nothing happens until it is
/// executed by id after explicit user confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub id: String,
    pub created_at_ms: i64,
    pub method: CleanupMethod,
    pub items: Vec<PlanItem>,
    pub rejected: Vec<PlanRejection>,
    pub total_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemStatus {
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionStatus {
    Succeeded,
    Partial,
    Failed,
}

impl ActionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionStatus::Succeeded => "succeeded",
            ActionStatus::Partial => "partial",
            ActionStatus::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "succeeded" => ActionStatus::Succeeded,
            "partial" => ActionStatus::Partial,
            _ => ActionStatus::Failed,
        }
    }
    pub fn from_items(items: &[CleanupItemResult]) -> Self {
        let ok = items
            .iter()
            .filter(|i| i.status == ItemStatus::Succeeded)
            .count();
        if ok == items.len() && ok > 0 {
            ActionStatus::Succeeded
        } else if ok > 0 {
            ActionStatus::Partial
        } else {
            ActionStatus::Failed
        }
    }
}

impl ItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemStatus::Succeeded => "succeeded",
            ItemStatus::Failed => "failed",
            ItemStatus::Skipped => "skipped",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "succeeded" => ItemStatus::Succeeded,
            "skipped" => ItemStatus::Skipped,
            _ => ItemStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItemResult {
    /// A filesystem path, or a Docker object such as "image sha256:…".
    pub target: String,
    /// The exact operation or command executed.
    pub operation: String,
    pub bytes: u64,
    pub status: ItemStatus,
    pub error: Option<String>,
}

/// One executed cleanup action, as recorded in history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRecord {
    pub id: String,
    /// "files" or "docker".
    pub kind: String,
    pub title: String,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub status: ActionStatus,
    /// Bytes removed from disk (permanent deletes and Docker removals).
    pub bytes_freed: u64,
    /// Bytes moved to the Trash (freed only when the Trash is emptied).
    pub bytes_trashed: u64,
    pub items: Vec<CleanupItemResult>,
}
