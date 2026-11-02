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
