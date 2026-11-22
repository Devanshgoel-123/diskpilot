//! Directory listings and item details for the Storage Explorer.
//!
//! Folder sizes come from the most recent scan's index; file metadata is read
//! live. Nothing here modifies the filesystem.

use std::cmp::Reverse;
use std::fs;
use std::path::Path;

use crate::classifier::{Classifier, FsProbe, Resolved};
use crate::error::{AppError, AppResult};
use crate::model::{DirectoryEntry, DirectoryListing, EntryKind, ItemDetails};
use crate::scanner::{allocated_bytes, to_ms, DirIndex};

/// Listings are capped so a folder with 100k files can't flood the UI.
pub const MAX_ENTRIES: usize = 1000;

fn kind_of(meta: &fs::Metadata) -> EntryKind {
    let ft = meta.file_type();
    if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Directory
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

fn resolved_or_unmatched(r: Option<crate::classifier::Context>) -> Resolved {
    r.map_or(Resolved::Unmatched, Resolved::Dir)
}

pub fn list_directory(
    path: &Path,
    scan_root: &Path,
    index: &DirIndex,
    classifier: &Classifier,
) -> AppResult<DirectoryListing> {
    let reader = fs::read_dir(path).map_err(|e| AppError::io(path, &e))?;
    let ctx = classifier.context_at(path, &FsProbe);

    let mut entries: Vec<DirectoryEntry> = Vec::new();
    for entry in reader.flatten() {
        let child = entry.path();
        let Ok(meta) = fs::symlink_metadata(&child) else {
            continue;
        };
        let kind = kind_of(&meta);
        let (bytes, file_count, classification) = match kind {
            EntryKind::Directory => {
                let stats = index.get(&child);
                let child_ctx = classifier.descend(&child, ctx.clone(), &FsProbe).0;
                (
                    stats.map(|s| s.bytes),
                    stats.map(|s| s.files),
                    resolved_or_unmatched(child_ctx).to_classification(),
                )
            }
            _ => (
                Some(allocated_bytes(&meta)),
                None,
                classifier
                    .resolve_file(&child, ctx.as_ref())
                    .to_classification(),
            ),
        };
        entries.push(DirectoryEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            extension: if kind == EntryKind::File {
                child
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
            } else {
                None
            },
            path: child.display().to_string(),
            kind,
            bytes,
            file_count,
            modified_ms: meta.modified().ok().and_then(to_ms),
            classification,
        });
    }

    entries.sort_by_key(|e| Reverse(e.bytes.unwrap_or(0)));
    let mut omitted_count = 0;
    let mut omitted_bytes = 0;
    if entries.len() > MAX_ENTRIES {
        for e in entries.drain(MAX_ENTRIES..) {
            omitted_count += 1;
            omitted_bytes += e.bytes.unwrap_or(0);
        }
    }

    Ok(DirectoryListing {
        path: path.display().to_string(),
        parent: if path == scan_root {
            None
        } else {
            path.parent().map(|p| p.display().to_string())
        },
        scan_root: scan_root.display().to_string(),
        bytes: index.get(path).map(|s| s.bytes),
        entries,
        omitted_count,
        omitted_bytes,
        classification: resolved_or_unmatched(ctx).to_classification(),
    })
}

pub fn item_details(
    path: &Path,
    index: &DirIndex,
    classifier: &Classifier,
) -> AppResult<ItemDetails> {
    let meta = fs::symlink_metadata(path).map_err(|e| AppError::io(path, &e))?;
    let kind = kind_of(&meta);
    let is_dir = kind == EntryKind::Directory;
    let stats = if is_dir { index.get(path) } else { None };
    Ok(ItemDetails {
        path: path.display().to_string(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        kind,
        bytes: if is_dir {
            stats.map(|s| s.bytes)
        } else {
            Some(allocated_bytes(&meta))
        },
        logical_bytes: if is_dir { None } else { Some(meta.len()) },
        file_count: stats.map(|s| s.files),
        created_ms: meta.created().ok().and_then(to_ms),
        modified_ms: meta.modified().ok().and_then(to_ms),
        accessed_ms: meta.accessed().ok().and_then(to_ms),
        readonly: crate::platform::read_only(&meta),
        symlink_target: if kind == EntryKind::Symlink {
            fs::read_link(path).ok().map(|t| t.display().to_string())
        } else {
            None
        },
        classification: classifier.classify_path(path, is_dir, &FsProbe),
    })
}
