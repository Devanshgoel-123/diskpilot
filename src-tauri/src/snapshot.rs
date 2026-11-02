//! The most recent completed scan, saved so results survive a restart.
//!
//! The file holds the scan summary and the per-folder size index. It lives in
//! the app's data folder, is readable only by the user, and never leaves the
//! Mac. Folder sizes are stored, not file contents.
//!
//! Layout: an 8-byte magic header, then a deflate stream containing
//! `u32` summary length, summary JSON, `u64` folder count, and one record per
//! folder sorted by path: shared-prefix length, suffix length and suffix bytes
//! (front coding), then the folder's statistics as varints.

use std::fs;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::model::ScanSummary;
use crate::scanner::{DirIndex, DirStats, ScanOutcome};

pub const FILE_NAME: &str = "last-scan.bin";
const MAGIC: &[u8; 8] = b"DPSNAP1\n";
/// Refuse to load anything larger; a real snapshot is far smaller.
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn write_varint(w: &mut impl Write, mut v: u64) -> io::Result<()> {
    let mut buf = [0u8; 10];
    let mut i = 0;
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            buf[i] = byte;
            i += 1;
            break;
        }
        buf[i] = byte | 0x80;
        i += 1;
    }
    w.write_all(&buf[..i])
}

fn read_varint(r: &mut impl Read) -> io::Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        value |= u64::from(b[0] & 0x7f) << shift;
        if b[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(invalid("varint too long"))
}

fn to_usize(v: u64) -> io::Result<usize> {
    usize::try_from(v).map_err(|_| invalid("length out of range"))
}

/// Serialize a scan into `w`.
pub fn encode(outcome: &ScanOutcome, w: impl Write) -> io::Result<()> {
    let mut w = w;
    w.write_all(MAGIC)?;
    let mut z = DeflateEncoder::new(w, Compression::fast());

    let summary = serde_json::to_vec(&outcome.summary).map_err(io::Error::other)?;
    let len = u32::try_from(summary.len()).map_err(|_| invalid("summary too large"))?;
    z.write_all(&len.to_le_bytes())?;
    z.write_all(&summary)?;

    let mut entries: Vec<(&[u8], &DirStats)> = outcome
        .index
        .iter()
        .map(|(p, s)| (p.as_os_str().as_bytes(), s))
        .collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    z.write_all(&(entries.len() as u64).to_le_bytes())?;

    let mut prev: &[u8] = &[];
    for (path, s) in entries {
        let shared = prev.iter().zip(path).take_while(|(a, b)| a == b).count();
        write_varint(&mut z, shared as u64)?;
        write_varint(&mut z, (path.len() - shared) as u64)?;
        z.write_all(&path[shared..])?;
        for v in [
            s.bytes,
            s.files,
            s.dirs,
            s.largest_child_dir,
            s.direct_bytes,
        ] {
            write_varint(&mut z, v)?;
        }
        z.write_all(&[u8::from(s.readable)])?;
        prev = path;
    }
    z.finish()?.flush()
}

/// Read a scan written by [`encode`].
pub fn decode(r: impl Read) -> io::Result<ScanOutcome> {
    let mut r = r;
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid("not a DiskPilot scan file"));
    }
    let mut z = BufReader::new(DeflateDecoder::new(r));

    let mut len = [0u8; 4];
    z.read_exact(&mut len)?;
    let mut summary = vec![0u8; u32::from_le_bytes(len) as usize];
    z.read_exact(&mut summary)?;
    let summary: ScanSummary = serde_json::from_slice(&summary).map_err(io::Error::other)?;

    let mut count = [0u8; 8];
    z.read_exact(&mut count)?;
    let count = to_usize(u64::from_le_bytes(count))?;
    let mut index = DirIndex::with_capacity(count.min(4_000_000));
    let mut prev: Vec<u8> = Vec::new();
    for _ in 0..count {
        let shared = to_usize(read_varint(&mut z)?)?;
        let suffix = to_usize(read_varint(&mut z)?)?;
        if shared > prev.len() || suffix > 64 * 1024 {
            return Err(invalid("corrupt path record"));
        }
        let mut path = prev[..shared].to_vec();
        let start = path.len();
        path.resize(start + suffix, 0);
        z.read_exact(&mut path[start..])?;
        let mut v = [0u64; 5];
        for slot in &mut v {
            *slot = read_varint(&mut z)?;
        }
        let mut readable = [0u8; 1];
        z.read_exact(&mut readable)?;
        index.insert(
            PathBuf::from(std::ffi::OsString::from_vec(path.clone())),
            DirStats {
                bytes: v[0],
                files: v[1],
                dirs: v[2],
                largest_child_dir: v[3],
                direct_bytes: v[4],
                readable: readable[0] != 0,
            },
        );
        prev = path;
    }
    Ok(ScanOutcome { summary, index })
}

/// Save atomically: write a private temporary file, then rename it.
pub fn save(dir: &Path, outcome: &ScanOutcome) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{FILE_NAME}.tmp"));
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    let result = encode(outcome, BufWriter::new(&file)).and_then(|()| file.sync_all());
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, dir.join(FILE_NAME))
}

/// Load the saved scan, if there is a usable one. A file that can't be read
/// is removed so it doesn't fail again on every launch.
pub fn load(dir: &Path) -> Option<ScanOutcome> {
    let path = dir.join(FILE_NAME);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let result = fs::File::open(&path).and_then(|f| decode(BufReader::new(f)));
    match result {
        Ok(outcome) if Path::new(&outcome.summary.root).is_dir() => Some(outcome),
        Ok(_) => None,
        Err(e) => {
            eprintln!("diskpilot: discarding unreadable saved scan: {e}");
            let _ = fs::remove_file(&path);
            None
        }
    }
}

/// Size of the saved scan file in bytes, if one exists.
pub fn file_size(dir: &Path) -> Option<u64> {
    fs::symlink_metadata(dir.join(FILE_NAME))
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

pub fn remove(dir: &Path) -> io::Result<()> {
    match fs::remove_file(dir.join(FILE_NAME)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{scan, ScanOptions};
    use std::sync::atomic::AtomicBool;

    fn sample() -> (tempfile::TempDir, ScanOutcome) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("home");
        for rel in ["a/b/c", "a/bb", "Library/Caches/x", "dé jà/ü"] {
            fs::create_dir_all(root.join(rel)).unwrap();
            fs::write(root.join(rel).join("f.bin"), vec![1u8; 9000]).unwrap();
        }
        let opts = ScanOptions::new("s1", &root, &root);
        let outcome = scan(&opts, &AtomicBool::new(false), &|_| {}).unwrap();
        (dir, outcome)
    }

    #[test]
    fn round_trips_summary_and_index() {
        let (_dir, outcome) = sample();
        let mut buf = Vec::new();
        encode(&outcome, &mut buf).unwrap();
        let back = decode(buf.as_slice()).unwrap();
        assert_eq!(back.index, outcome.index);
        assert_eq!(back.summary.id, outcome.summary.id);
        assert_eq!(back.summary.total_bytes, outcome.summary.total_bytes);
        assert_eq!(
            back.summary.largest_files.len(),
            outcome.summary.largest_files.len()
        );
    }

    #[test]
    fn rejects_foreign_and_truncated_files() {
        assert!(decode(&b"not a snapshot"[..]).is_err());
        let (_dir, outcome) = sample();
        let mut buf = Vec::new();
        encode(&outcome, &mut buf).unwrap();
        buf.truncate(buf.len() / 2);
        assert!(decode(buf.as_slice()).is_err());
    }

    #[test]
    fn saves_privately_loads_and_removes() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, outcome) = sample();
        let data = dir.path().join("data");
        save(&data, &outcome).unwrap();
        let mode = fs::metadata(data.join(FILE_NAME))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(file_size(&data).unwrap() > 0);
        let loaded = load(&data).unwrap();
        assert_eq!(loaded.index, outcome.index);
        remove(&data).unwrap();
        assert!(load(&data).is_none());
        remove(&data).unwrap();
    }

    #[test]
    fn discards_corrupt_file_and_missing_root() {
        let (dir, outcome) = sample();
        let data = dir.path().join("data");
        fs::create_dir_all(&data).unwrap();
        fs::write(data.join(FILE_NAME), b"garbage").unwrap();
        assert!(load(&data).is_none());
        assert!(!data.join(FILE_NAME).exists());

        save(&data, &outcome).unwrap();
        fs::remove_dir_all(dir.path().join("home")).unwrap();
        assert!(load(&data).is_none());
    }
}
