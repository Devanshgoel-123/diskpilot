use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::FileIdentity;

/// The device a file is on. Two folders with different devices are on
/// different volumes.
pub fn device(meta: &Metadata) -> u64 {
    meta.dev()
}

/// Number of hard links to the file.
pub fn link_count(meta: &Metadata) -> u64 {
    meta.nlink()
}

/// Identity from metadata already read; always available on Unix.
pub fn identity(meta: &Metadata) -> Option<FileIdentity> {
    Some(FileIdentity {
        volume: meta.dev(),
        index: meta.ino(),
        links: meta.nlink(),
    })
}

/// Identity of the file at `path` itself (links are not followed).
pub fn identity_at(path: &Path) -> Option<FileIdentity> {
    identity(&std::fs::symlink_metadata(path).ok()?)
}

/// Hidden by a file attribute (dot names are handled by callers).
pub fn hidden(_meta: &Metadata) -> bool {
    false
}

/// Whether the file's data is on this computer.
pub fn is_local(_meta: &Metadata) -> bool {
    true
}

/// Bytes the file occupies on disk.
pub fn allocated_bytes(meta: &Metadata) -> u64 {
    // st_blocks is always in 512-byte units, independent of the fs block size.
    meta.blocks().saturating_mul(512)
}

/// Whether the owner may not write to the item.
pub fn read_only(meta: &Metadata) -> bool {
    meta.permissions().readonly() || meta.mode() & 0o200 == 0
}

/// The raw bytes of a path, as stored in snapshots.
pub fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

/// The inverse of [`path_bytes`].
pub fn path_from_bytes(bytes: Vec<u8>) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Some(std::ffi::OsString::from_vec(bytes).into())
}

/// The UTC offset of local time at `secs` since the epoch, and the local
/// weekday (0 = Sunday) and hour.
pub fn local_time(secs: i64) -> (i64, u32, u32) {
    let t = secs as libc::time_t;
    // SAFETY: `tm` is plain data written by localtime_r; both pointers are
    // valid for the call.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    // `c_long` is 32 bits on some targets.
    #[allow(clippy::useless_conversion)]
    let offset = i64::from(tm.tm_gmtoff);
    (
        offset,
        u32::try_from(tm.tm_wday).unwrap_or(0),
        u32::try_from(tm.tm_hour).unwrap_or(0),
    )
}

/// Whether DiskPilot runs with administrator rights (as root here).
pub fn is_elevated() -> bool {
    // SAFETY: no arguments, cannot fail.
    unsafe { libc::geteuid() == 0 }
}

/// Starting DiskPilot again with administrator rights is a Windows feature.
pub fn relaunch_elevated() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Only available on Windows",
    ))
}
