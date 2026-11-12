//! A small disk cache of file thumbnails, so a preview seen before shows at
//! once, also after DiskPilot restarts.
//!
//! Entries live only in DiskPilot's own cache folder (for example
//! `~/Library/Caches/dev.diskpilot.app/thumbnails`), never next to the files
//! they show. An entry is found by a hash of the file's path, size, modification
//! time and the thumbnail size, so a file that changed is simply not found and
//! gets a new thumbnail; the old entry ages out. The folder is kept under
//! [`MAX_BYTES`] by removing the least recently used entries.
//!
//! Removing files here is limited to entries this module wrote: names are 64
//! hex characters, directly inside the cache folder.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

/// Largest the cache folder may grow, in bytes. Thumbnails are about 2 KB
/// (rows) to 150 KB (previews), so this holds a few hundred previews and
/// thousands of row thumbnails.
pub const MAX_BYTES: u64 = 48 * 1024 * 1024;
/// Check the folder's size every this many writes.
const CHECK_EVERY: u32 = 64;

pub struct ThumbCache {
    dir: PathBuf,
    max_bytes: u64,
    writes: AtomicU32,
}

impl ThumbCache {
    /// A cache in `dir`, created if needed. `None` if it cannot be created.
    pub fn open(dir: PathBuf) -> Option<ThumbCache> {
        Self::with_limit(dir, MAX_BYTES)
    }

    pub fn with_limit(dir: PathBuf, max_bytes: u64) -> Option<ThumbCache> {
        fs::create_dir_all(&dir).ok()?;
        Some(ThumbCache {
            dir,
            max_bytes,
            writes: AtomicU32::new(0),
        })
    }

    /// The entry name for `file` at `px`, or `None` if the file cannot be
    /// read now (it is then not cached).
    fn key(file: &Path, px: f64) -> Option<String> {
        let meta = file.symlink_metadata().ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let mut h = Sha256::new();
        h.update(file.as_os_str().as_encoded_bytes());
        h.update([0]);
        h.update(meta.len().to_le_bytes());
        h.update(modified.to_le_bytes());
        h.update(px.to_bits().to_le_bytes());
        Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// The thumbnail saved for this exact version of `file`, if any.
    pub fn get(&self, file: &Path, px: f64) -> Option<Vec<u8>> {
        let entry = self.dir.join(Self::key(file, px)?);
        let bytes = fs::read(&entry).ok()?;
        // Mark it recently used, for trimming.
        if let Ok(f) = fs::File::options().append(true).open(&entry) {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    /// Saves a thumbnail for this version of `file`. Failures are ignored: the
    /// cache is only a shortcut.
    pub fn put(&self, file: &Path, px: f64, bytes: &[u8]) {
        let Some(key) = Self::key(file, px) else {
            return;
        };
        let tmp = self.dir.join(format!(".{key}.tmp"));
        let written = fs::File::create(&tmp)
            .and_then(|mut f| f.write_all(bytes))
            .and_then(|()| fs::rename(&tmp, self.dir.join(&key)));
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
            return;
        }
        if self.writes.fetch_add(1, Ordering::Relaxed) % CHECK_EVERY == 0 {
            self.trim();
        }
    }

    /// Removes the least recently used entries while the folder is over its
    /// limit. Only touches files this cache names.
    pub fn trim(&self) {
        let Ok(read) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut entries: Vec<(SystemTime, u64, PathBuf)> = read
            .flatten()
            .filter(|e| is_entry_name(&e.file_name().to_string_lossy()))
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                meta.is_file()
                    .then(|| (meta.modified().unwrap_or(UNIX_EPOCH), meta.len(), e.path()))
            })
            .collect();
        let mut total: u64 = entries.iter().map(|e| e.1).sum();
        if total <= self.max_bytes {
            return;
        }
        // Trim to three quarters, so the next trim is a while away.
        let target = self.max_bytes / 4 * 3;
        entries.sort_by_key(|e| e.0);
        for (_, len, path) in entries {
            if total <= target {
                break;
            }
            if path.parent() == Some(self.dir.as_path()) && fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}

fn is_entry_name(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sandbox;

    #[test]
    fn finds_a_thumbnail_until_the_file_changes() {
        let dir = sandbox();
        let cache = ThumbCache::open(dir.path().join("thumbnails")).unwrap();
        let photo = dir.path().join("photo.jpg");
        fs::write(&photo, b"first").unwrap();
        assert!(cache.get(&photo, 512.0).is_none());
        cache.put(&photo, 512.0, b"thumb-1");
        assert_eq!(cache.get(&photo, 512.0).as_deref(), Some(&b"thumb-1"[..]));
        // Another size is another entry.
        assert!(cache.get(&photo, 32.0).is_none());
        // A changed file (new size) is not found.
        fs::write(&photo, b"second version").unwrap();
        assert!(cache.get(&photo, 512.0).is_none());
        // Nothing is written next to the file itself.
        let beside: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(beside.len(), 2, "photo.jpg and the cache folder only");
    }

    #[test]
    fn a_changed_modification_time_is_a_new_entry() {
        let dir = sandbox();
        let cache = ThumbCache::open(dir.path().join("thumbnails")).unwrap();
        let photo = dir.path().join("photo.jpg");
        fs::write(&photo, b"same").unwrap();
        cache.put(&photo, 64.0, b"old");
        let f = fs::File::options().append(true).open(&photo).unwrap();
        f.set_modified(SystemTime::now() + std::time::Duration::from_secs(60))
            .unwrap();
        assert!(cache.get(&photo, 64.0).is_none());
    }

    #[test]
    fn trims_least_recently_used_entries_and_nothing_else() {
        let dir = sandbox();
        let folder = dir.path().join("thumbnails");
        let cache = ThumbCache::with_limit(folder.clone(), 4000).unwrap();
        fs::write(folder.join("not-ours.txt"), vec![0; 3000]).unwrap();
        let mut files = Vec::new();
        for i in 0..5 {
            let f = dir.path().join(format!("f{i}"));
            fs::write(&f, [i]).unwrap();
            cache.put(&f, 64.0, &[i; 1000]);
            files.push(f);
        }
        // Use the first one again so it counts as recent.
        let past = SystemTime::now() - std::time::Duration::from_secs(600);
        for (i, f) in files.iter().enumerate() {
            let entry = folder.join(ThumbCache::key(f, 64.0).unwrap());
            let when = past + std::time::Duration::from_secs(i as u64 * 10);
            fs::File::options()
                .append(true)
                .open(&entry)
                .unwrap()
                .set_modified(when)
                .unwrap();
        }
        assert!(cache.get(&files[0], 64.0).is_some());
        cache.trim();
        let kept: Vec<bool> = files.iter().map(|f| cache.get(f, 64.0).is_some()).collect();
        // 5,000 bytes of entries against a 4,000 limit, trimmed to 3,000:
        // the two oldest go, the recently used first one stays.
        assert_eq!(kept, [true, false, false, true, true]);
        assert!(folder.join("not-ours.txt").exists());
    }
}
