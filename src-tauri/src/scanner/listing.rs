//! Directory listing with per-entry metadata, without following symlinks.
//!
//! On macOS one `getattrlistbulk` call returns a batch of entries together
//! with their attributes, instead of one `lstat` per entry. On Windows one
//! `GetFileInformationByHandleEx` call does the same from a single directory
//! handle. Other platforms, and file systems that support neither, use
//! `read_dir` + `lstat`.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use crate::platform;
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Dir,
    File,
    Symlink,
    /// Sockets, pipes, devices: sized but never a "largest file".
    Other,
}

/// The attributes the scanner uses for one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stat {
    pub kind: Kind,
    /// Allocated bytes on disk.
    pub alloc: u64,
    /// Apparent size.
    pub logical: u64,
    pub modified_ms: Option<i64>,
    pub nlink: u64,
    pub ino: u64,
    /// A folder on another volume, or a firmlink to one. Never descended.
    pub other_volume: bool,
}

impl Stat {
    pub fn from_metadata(meta: &std::fs::Metadata, root_dev: u64) -> Self {
        let ft = meta.file_type();
        let kind = if ft.is_dir() {
            Kind::Dir
        } else if ft.is_symlink() {
            Kind::Symlink
        } else if ft.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        let is_dir = kind == Kind::Dir;
        Stat {
            kind,
            alloc: super::allocated_bytes(meta),
            // A folder's apparent size and link count are unused, and the
            // bulk listing does not report them.
            logical: if is_dir { 0 } else { meta.len() },
            modified_ms: meta.modified().ok().and_then(super::to_ms),
            nlink: if is_dir {
                1
            } else {
                platform::link_count(meta)
            },
            ino: platform::identity(meta).map_or(0, |id| id.index),
            other_volume: kind == Kind::Dir && platform::device(meta) != root_dev,
        }
    }
}

pub(crate) struct Listing {
    pub entries: Vec<(OsString, Stat)>,
    /// Entries (or the listing itself, part-way) that could not be read.
    pub errors: Vec<(PathBuf, io::Error)>,
}

/// Testing and benchmarking only: list with `read_dir` + `lstat` everywhere.
#[doc(hidden)]
pub static FORCE_PORTABLE: AtomicBool = AtomicBool::new(false);

/// List `dir`. Fails only if the folder cannot be opened at all.
pub(crate) fn list(dir: &Path, root_dev: u64) -> io::Result<Listing> {
    if !FORCE_PORTABLE.load(std::sync::atomic::Ordering::Relaxed) {
        #[cfg(target_os = "macos")]
        match bulk::list(dir) {
            Err(e) if bulk::unsupported(&e) => {}
            other => return other,
        }
        #[cfg(windows)]
        match win::list(dir) {
            Err(e) if win::unsupported(&e) => {}
            other => return other,
        }
    }
    list_portable(dir, root_dev)
}

pub(crate) fn list_portable(dir: &Path, root_dev: u64) -> io::Result<Listing> {
    let reader = std::fs::read_dir(dir)?;
    let mut listing = Listing {
        entries: Vec::new(),
        errors: Vec::new(),
    };
    for entry in reader {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                listing.errors.push((dir.to_path_buf(), err));
                continue;
            }
        };
        let child = entry.path();
        // On Windows the entry's metadata comes from the directory listing
        // itself (links not followed); `symlink_metadata` would open every
        // file.
        #[cfg(windows)]
        let meta = entry.metadata();
        #[cfg(not(windows))]
        let meta = std::fs::symlink_metadata(&child);
        match meta {
            Ok(meta) => listing
                .entries
                .push((entry.file_name(), Stat::from_metadata(&meta, root_dev))),
            Err(err) => listing.errors.push((child, err)),
        }
    }
    Ok(listing)
}

#[cfg(target_os = "macos")]
mod bulk {
    use super::{Kind, Listing, Stat};
    use std::cell::RefCell;
    use std::ffi::{CString, OsStr, OsString};
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    // Not exported by the libc crate.
    const ATTR_CMN_ERROR: u32 = 0x2000_0000;
    const SF_FIRMLINK: u32 = 0x0080_0000;
    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const VLNK: u32 = 5;

    /// 64 KiB holds a few hundred entries per call; dua-cli uses the same.
    /// `u64` elements keep the buffer 8-byte aligned.
    const BUF_WORDS: usize = 64 * 1024 / 8;

    thread_local! {
        // A listing finishes before the scanner recurses, so one buffer per
        // worker thread is enough.
        static BUF: RefCell<Vec<u64>> = RefCell::new(vec![0; BUF_WORDS]);
    }

    pub(super) fn unsupported(e: &io::Error) -> bool {
        matches!(e.raw_os_error(), Some(libc::ENOTSUP | libc::EINVAL))
    }

    struct Fd(libc::c_int);
    impl Drop for Fd {
        fn drop(&mut self) {
            unsafe { libc::close(self.0) };
        }
    }

    fn attrlist() -> libc::attrlist {
        let mut al: libc::attrlist = unsafe { std::mem::zeroed() };
        al.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
        // Values come back in this fixed order: returned-attrs, error, then
        // each group in bit order. `parse` relies on it.
        al.commonattr = libc::ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_ERROR
            | libc::ATTR_CMN_NAME
            | libc::ATTR_CMN_OBJTYPE
            | libc::ATTR_CMN_MODTIME
            | libc::ATTR_CMN_FLAGS
            | libc::ATTR_CMN_FILEID;
        al.dirattr = libc::ATTR_DIR_MOUNTSTATUS | libc::ATTR_DIR_ALLOCSIZE;
        al.fileattr =
            libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE | libc::ATTR_FILE_DATALENGTH;
        al
    }

    pub(super) fn list(dir: &Path) -> io::Result<Listing> {
        let c = CString::new(dir.as_os_str().as_bytes())
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
        let fd = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = Fd(fd);
        let mut al = attrlist();
        let mut listing = Listing {
            entries: Vec::new(),
            errors: Vec::new(),
        };
        BUF.with_borrow_mut(|words| {
            let len = words.len() * 8;
            loop {
                let n = unsafe {
                    libc::getattrlistbulk(
                        fd.0,
                        (&mut al as *mut libc::attrlist).cast(),
                        words.as_mut_ptr().cast(),
                        len,
                        0,
                    )
                };
                if n < 0 {
                    let err = io::Error::last_os_error();
                    if listing.entries.is_empty() && listing.errors.is_empty() {
                        // Nothing read yet: let the caller decide (fallback
                        // on ENOTSUP, otherwise an unreadable folder).
                        return Err(err);
                    }
                    listing.errors.push((dir.to_path_buf(), err));
                    return Ok(());
                }
                if n == 0 {
                    return Ok(());
                }
                // SAFETY: the kernel wrote at most `len` bytes into the buffer.
                let buf: &[u8] = unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), len) };
                let mut base = 0usize;
                for _ in 0..n {
                    let Some((rec_len, entry)) = parse(&buf[base..]) else {
                        listing.errors.push((
                            dir.to_path_buf(),
                            io::Error::new(io::ErrorKind::InvalidData, "malformed attributes"),
                        ));
                        return Ok(());
                    };
                    match entry {
                        Ok((name, stat)) => listing.entries.push((name, stat)),
                        Err((name, err)) => listing.errors.push((dir.join(name), err)),
                    }
                    base += rec_len;
                }
            }
        })?;
        Ok(listing)
    }

    type Parsed = Result<(OsString, Stat), (OsString, io::Error)>;

    /// Parse one record. Returns its length and the entry.
    fn parse(rec: &[u8]) -> Option<(usize, Parsed)> {
        let mut off = 0usize;
        let rec_len = take::<u32>(rec, &mut off)? as usize;
        let rec = rec.get(..rec_len)?;
        let ret: libc::attribute_set_t = take(rec, &mut off)?;
        let err = if ret.commonattr & ATTR_CMN_ERROR != 0 {
            take::<u32>(rec, &mut off)?
        } else {
            0
        };
        let name_at = off;
        let name_ref: libc::attrreference_t = take(rec, &mut off)?;
        let name_start = name_at.checked_add(usize::try_from(name_ref.attr_dataoffset).ok()?)?;
        // attr_length counts the trailing NUL.
        let name_len = (name_ref.attr_length as usize).checked_sub(1)?;
        let name = OsStr::from_bytes(rec.get(name_start..name_start + name_len)?).to_os_string();
        if err != 0 {
            return Some((
                rec_len,
                Err((name, io::Error::from_raw_os_error(err as i32))),
            ));
        }

        let objtype = if ret.commonattr & libc::ATTR_CMN_OBJTYPE != 0 {
            take::<u32>(rec, &mut off)?
        } else {
            0
        };
        let modified_ms = if ret.commonattr & libc::ATTR_CMN_MODTIME != 0 {
            let ts: libc::timespec = take(rec, &mut off)?;
            ts.tv_sec
                .checked_mul(1000)
                .and_then(|ms| ms.checked_add(ts.tv_nsec / 1_000_000))
        } else {
            None
        };
        let flags = if ret.commonattr & libc::ATTR_CMN_FLAGS != 0 {
            take::<u32>(rec, &mut off)?
        } else {
            0
        };
        let ino = if ret.commonattr & libc::ATTR_CMN_FILEID != 0 {
            take::<u64>(rec, &mut off)?
        } else {
            0
        };
        let mut mount_status = 0u32;
        let mut alloc = 0i64;
        let mut logical = 0i64;
        let mut nlink = 1u32;
        if objtype == VDIR {
            if ret.dirattr & libc::ATTR_DIR_MOUNTSTATUS != 0 {
                mount_status = take(rec, &mut off)?;
            }
            if ret.dirattr & libc::ATTR_DIR_ALLOCSIZE != 0 {
                alloc = take(rec, &mut off)?;
            }
        } else {
            if ret.fileattr & libc::ATTR_FILE_LINKCOUNT != 0 {
                nlink = take(rec, &mut off)?;
            }
            if ret.fileattr & libc::ATTR_FILE_ALLOCSIZE != 0 {
                alloc = take(rec, &mut off)?;
            }
            if ret.fileattr & libc::ATTR_FILE_DATALENGTH != 0 {
                logical = take(rec, &mut off)?;
            }
        }
        let kind = match objtype {
            VDIR => Kind::Dir,
            VREG => Kind::File,
            VLNK => Kind::Symlink,
            _ => Kind::Other,
        };
        let stat = Stat {
            kind,
            alloc: alloc.max(0) as u64,
            logical: logical.max(0) as u64,
            modified_ms,
            nlink: u64::from(nlink),
            ino,
            // Bulk results report a mount point with its parent's device, so
            // ask for the mount status instead. Firmlinks join the system and
            // data volumes; lstat would show the other volume's device there.
            other_volume: kind == Kind::Dir
                && (mount_status & libc::DIR_MNTSTATUS_MNTPOINT != 0 || flags & SF_FIRMLINK != 0),
        };
        Some((rec_len, Ok((name, stat))))
    }

    fn take<T: Copy>(buf: &[u8], off: &mut usize) -> Option<T> {
        let end = off.checked_add(std::mem::size_of::<T>())?;
        let bytes = buf.get(*off..end)?;
        *off = end;
        // SAFETY: `bytes` holds size_of::<T>() bytes; T is plain old data.
        Some(unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast()) })
    }
}

/// Bytes of the per-thread buffer the Windows listing reads into. Testing
/// and benchmarking may change it; see `scan_bench`.
#[cfg(windows)]
#[doc(hidden)]
pub static LIST_BUFFER_BYTES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(win::DEFAULT_BUF_BYTES);

#[cfg(windows)]
mod win {
    use super::{Kind, Listing, Stat};
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_FUNCTION, ERROR_INVALID_LEVEL, ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FileFullDirectoryInfo, FileFullDirectoryRestartInfo, FileIdExtdDirectoryInfo,
        FileIdExtdDirectoryRestartInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    /// 64 KiB holds about 500 typical entries per call, so most folders
    /// need one call. Larger buffers only help folders with thousands of
    /// entries; `scan_bench` measures the difference.
    pub(super) const DEFAULT_BUF_BYTES: usize = 64 * 1024;

    thread_local! {
        // A listing finishes before the scanner recurses, so one buffer per
        // worker thread is enough. `u64` elements keep it 8-byte aligned,
        // as the entries require.
        static BUF: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    }

    /// Reparse tags with this bit redirect to another name: symbolic links,
    /// junctions and volume mount points. Those are links and never
    /// descended. Other reparse points (cloud files, deduplicated and
    /// compressed system files, app aliases) are ordinary files and folders
    /// with a file-system filter, exactly as `std::fs` treats them.
    const NAME_SURROGATE: u32 = 0x2000_0000;

    /// Both directory information classes were refused: list another way.
    pub(super) fn unsupported(e: &io::Error) -> bool {
        e.raw_os_error().is_some_and(|c| {
            [
                ERROR_INVALID_LEVEL,
                ERROR_INVALID_PARAMETER,
                ERROR_NOT_SUPPORTED,
                ERROR_INVALID_FUNCTION,
            ]
            .contains(&(c as u32))
        })
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: a handle this module opened and nothing else closes.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The entry layout one information class returns.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Class {
        /// `FILE_ID_EXTD_DIR_INFO`: has the reparse tag and the file id.
        IdExtd,
        /// `FILE_FULL_DIR_INFO`: the reparse tag is in `EaSize`, no file id.
        Full,
    }

    impl Class {
        fn restart(self) -> FILE_INFO_BY_HANDLE_CLASS {
            match self {
                Class::IdExtd => FileIdExtdDirectoryRestartInfo,
                Class::Full => FileFullDirectoryRestartInfo,
            }
        }
        fn next(self) -> FILE_INFO_BY_HANDLE_CLASS {
            match self {
                Class::IdExtd => FileIdExtdDirectoryInfo,
                Class::Full => FileFullDirectoryInfo,
            }
        }
        fn name_offset(self) -> usize {
            match self {
                Class::IdExtd => 88,
                Class::Full => 68,
            }
        }
    }

    /// A path Win32 accepts at any length: `\\?\` is added when the path is
    /// long and does not have it yet.
    pub(super) fn wide_path(dir: &Path) -> Vec<u16> {
        // Folder paths must leave room for an 8.3 name below MAX_PATH.
        const LIMIT: usize = 248;
        let w: Vec<u16> = dir.as_os_str().encode_wide().collect();
        let prefixed = w.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
            || w.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16]);
        let mut out = if w.len() < LIMIT || prefixed {
            w
        } else {
            let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
            let abs: Vec<u16> = abs
                .as_os_str()
                .encode_wide()
                .map(|c| if c == b'/' as u16 { b'\\' as u16 } else { c })
                .collect();
            let mut out: Vec<u16> = Vec::with_capacity(abs.len() + 8);
            if abs.starts_with(&[b'\\' as u16, b'\\' as u16]) {
                // \\server\share\x -> \\?\UNC\server\share\x
                out.extend(r"\\?\UNC\".encode_utf16());
                out.extend_from_slice(&abs[2..]);
            } else {
                out.extend(r"\\?\".encode_utf16());
                out.extend_from_slice(&abs);
            }
            out
        };
        out.push(0);
        out
    }

    fn read_u32(b: &[u8], off: usize) -> Option<u32> {
        Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
    }
    fn read_u64(b: &[u8], off: usize) -> Option<u64> {
        Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
    }

    /// Milliseconds since the Unix epoch from a FILETIME, as
    /// `Metadata::modified` would report it (`None` before 1970).
    pub(super) fn filetime_ms(ft: i64) -> Option<i64> {
        const EPOCH: i64 = 116_444_736_000_000_000;
        (ft >= EPOCH).then(|| (ft - EPOCH) / 10_000)
    }

    /// Kind of an entry from its attributes and reparse tag.
    pub(crate) fn kind_of(attrs: u32, tag: u32) -> Kind {
        if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 && tag & NAME_SURROGATE != 0 {
            Kind::Symlink
        } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
            Kind::Dir
        } else {
            Kind::File
        }
    }

    /// Parse one entry. `None` if it is malformed; `Some(None)` for `.` and
    /// `..`.
    fn parse(class: Class, e: &[u8]) -> Option<Option<(OsString, Stat)>> {
        let write_time = read_u64(e, 24)? as i64;
        let eof = read_u64(e, 40)?;
        let alloc = read_u64(e, 48)?;
        let attrs = read_u32(e, 56)?;
        let name_bytes = read_u32(e, 60)? as usize;
        let tag = if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            match class {
                Class::IdExtd => read_u32(e, 68)?,
                Class::Full => read_u32(e, 64)?,
            }
        } else {
            0
        };
        let ino = match class {
            Class::IdExtd => read_u64(e, 72)?,
            Class::Full => 0,
        };
        let start = class.name_offset();
        let raw = e.get(start..start.checked_add(name_bytes)?)?;
        let name: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        if name == [b'.' as u16] || name == [b'.' as u16, b'.' as u16] {
            return Some(None);
        }
        let kind = kind_of(attrs, tag);
        let is_dir = kind == Kind::Dir;
        Some(Some((
            OsString::from_wide(&name),
            Stat {
                kind,
                // A folder's own size is not counted on Windows, as in
                // `platform::allocated_bytes`. For files this is the space
                // used on this PC: zero for a cloud file that is online only.
                alloc: if is_dir { 0 } else { alloc },
                logical: if is_dir { 0 } else { eof },
                modified_ms: filetime_ms(write_time as i64),
                // Directory listings do not report link counts; the
                // scanner asks where hard links are common.
                nlink: 1,
                ino,
                // Mount points are reparse points, so never descended.
                other_volume: false,
            },
        )))
    }

    pub(super) fn list(dir: &Path) -> io::Result<Listing> {
        let w = wide_path(dir);
        // SAFETY: `w` is NUL-terminated and outlives the call; the handle is
        // closed by `Handle`.
        let h = unsafe {
            CreateFileW(
                w.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let h = Handle(h);
        let mut listing = Listing {
            entries: Vec::new(),
            errors: Vec::new(),
        };
        let want = super::LIST_BUFFER_BYTES
            .load(Ordering::Relaxed)
            .clamp(4096, 16 << 20);
        BUF.with_borrow_mut(|words| {
            if words.len() * 8 != want {
                *words = vec![0u64; want.div_ceil(8)];
            }
            let len = words.len() * 8;
            let read = |class: FILE_INFO_BY_HANDLE_CLASS, words: &mut Vec<u64>| -> bool {
                // SAFETY: the buffer is `len` bytes, 8-byte aligned.
                unsafe {
                    GetFileInformationByHandleEx(h.0, class, words.as_mut_ptr().cast(), len as u32)
                        != 0
                }
            };
            // The first call restarts the scan and tells which class works.
            let mut class = Class::IdExtd;
            if !read(class.restart(), words) {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    return Ok(());
                }
                if !unsupported(&err) {
                    return Err(err);
                }
                class = Class::Full;
                if !read(class.restart(), words) {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                        return Ok(());
                    }
                    return Err(err);
                }
            }
            loop {
                // SAFETY: the call wrote at most `len` bytes into the buffer.
                let buf: &[u8] = unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), len) };
                let mut off = 0usize;
                loop {
                    let Some(entry) = buf.get(off..) else { break };
                    let next = read_u32(entry, 0).unwrap_or(0) as usize;
                    let entry = if next == 0 {
                        entry
                    } else {
                        &entry[..next.min(entry.len())]
                    };
                    match parse(class, entry) {
                        Some(Some(item)) => listing.entries.push(item),
                        Some(None) => {}
                        None => {
                            listing.errors.push((
                                dir.to_path_buf(),
                                io::Error::new(io::ErrorKind::InvalidData, "malformed entry"),
                            ));
                            return Ok(());
                        }
                    }
                    if next == 0 {
                        break;
                    }
                    off += next;
                }
                if !read(class.next(), words) {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
                        listing.errors.push((dir.to_path_buf(), err));
                    }
                    return Ok(());
                }
            }
        })?;
        Ok(listing)
    }
}

#[cfg(all(test, windows))]
mod win_tests {
    use super::*;
    use std::os::windows::fs::MetadataExt;
    use std::process::Command;

    fn sorted(mut l: Listing) -> Vec<(OsString, Stat)> {
        l.entries.sort_by(|a, b| a.0.cmp(&b.0));
        l.entries
    }

    /// Fields both listings report the same way. Allocation differs on
    /// purpose: the portable path estimates it from the length.
    fn comparable(v: &[(OsString, Stat)]) -> Vec<(OsString, Kind, u64, Option<i64>)> {
        v.iter()
            .map(|(n, s)| (n.clone(), s.kind, s.logical, s.modified_ms))
            .collect()
    }

    #[test]
    fn fast_listing_matches_read_dir() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("empty"), b"").unwrap();
        std::fs::write(d.join("big.bin"), vec![1u8; 300_000]).unwrap();
        std::fs::write(d.join("naïve ✓ name"), b"x").unwrap();
        std::fs::hard_link(d.join("big.bin"), d.join("link.bin")).unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/f"), b"y").unwrap();
        std::fs::create_dir(d.join("empty-dir")).unwrap();
        // Symbolic links need a privilege CI runners have; skip if missing.
        let have_symlink =
            std::os::windows::fs::symlink_file(d.join("big.bin"), d.join("sym")).is_ok();
        let junction = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(d.join("junction"))
            .arg(d.join("sub"))
            .output()
            .is_ok_and(|o| o.status.success());
        // Many entries, so the fast reader needs more than one call.
        std::fs::create_dir(d.join("many")).unwrap();
        for i in 0..3000 {
            std::fs::write(
                d.join(format!("many/file-with-a-longish-name-{i:05}")),
                b"z",
            )
            .unwrap();
        }
        // A folder whose path is longer than MAX_PATH.
        let mut long = d.to_path_buf();
        while long.as_os_str().len() < 300 {
            long.push("a-folder-name-that-is-quite-long");
        }
        std::fs::create_dir_all(&long).unwrap();
        std::fs::write(long.join("deep.txt"), b"deep").unwrap();

        for dir in [d.to_path_buf(), d.join("many"), long.clone()] {
            let fast = win::list(&dir).unwrap();
            assert!(fast.errors.is_empty());
            let portable = list_portable(&dir, 0).unwrap();
            assert!(portable.errors.is_empty());
            assert_eq!(
                comparable(&sorted(fast)),
                comparable(&sorted(portable)),
                "{}",
                dir.display()
            );
        }
        assert_eq!(win::list(&d.join("empty-dir")).unwrap().entries.len(), 0);

        let fast = sorted(win::list(d).unwrap());
        let stat = |n: &str| fast.iter().find(|(x, _)| x == n).unwrap().1;
        assert_eq!(stat("sub").kind, Kind::Dir);
        assert_eq!(stat("big.bin").kind, Kind::File);
        assert!(stat("big.bin").alloc >= 300_000);
        if have_symlink {
            assert_eq!(stat("sym").kind, Kind::Symlink);
        }
        if junction {
            assert_eq!(stat("junction").kind, Kind::Symlink);
        }
        // File ids match what the file itself reports, and both names of a
        // hard-linked file share one.
        let id = crate::platform::identity_at(&d.join("big.bin")).unwrap();
        assert_eq!(stat("big.bin").ino, id.index);
        assert_eq!(stat("link.bin").ino, id.index);
        assert_ne!(stat("empty").ino, id.index);
        let meta = std::fs::symlink_metadata(d.join("big.bin")).unwrap();
        assert_eq!(stat("big.bin").logical, meta.file_size());
    }

    #[test]
    fn long_paths_get_the_prefix() {
        let short = Path::new(r"C:\Users\x");
        assert_eq!(
            String::from_utf16_lossy(&win::wide_path(short)),
            "C:\\Users\\x\0"
        );
        let long = format!(r"C:\{}", "a\\".repeat(200));
        let w = String::from_utf16_lossy(&win::wide_path(Path::new(&long)));
        assert!(w.starts_with(r"\\?\C:\a\a"), "{w}");
        let unc = format!(r"\\server\share\{}", "b\\".repeat(200));
        let w = String::from_utf16_lossy(&win::wide_path(Path::new(&unc)));
        assert!(w.starts_with(r"\\?\UNC\server\share\b"), "{w}");
    }

    #[test]
    fn missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(&dir.path().join("nope"), 0).is_err());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    fn sorted(mut l: Listing) -> Vec<(OsString, Stat)> {
        l.entries.sort_by(|a, b| a.0.cmp(&b.0));
        l.entries
    }

    #[test]
    fn bulk_matches_lstat() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("empty"), b"").unwrap();
        std::fs::write(d.join("big.bin"), vec![1u8; 300_000]).unwrap();
        std::fs::write(d.join("naïve ✓ name"), b"x").unwrap();
        std::fs::hard_link(d.join("big.bin"), d.join("link.bin")).unwrap();
        symlink(d.join("big.bin"), d.join("sym")).unwrap();
        symlink("/nowhere", d.join("dangling")).unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/f"), b"y").unwrap();
        let fifo = std::ffi::CString::new(d.join("fifo").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        // Many entries, so the bulk reader needs more than one call.
        std::fs::create_dir(d.join("many")).unwrap();
        for i in 0..3000 {
            std::fs::write(
                d.join(format!("many/file-with-a-longish-name-{i:05}")),
                b"z",
            )
            .unwrap();
        }

        let dev = std::fs::symlink_metadata(d).unwrap().dev();
        for dir in [d.to_path_buf(), d.join("many")] {
            let bulk = bulk::list(&dir).unwrap();
            assert!(bulk.errors.is_empty());
            let portable = list_portable(&dir, dev).unwrap();
            assert_eq!(sorted(bulk), sorted(portable), "{}", dir.display());
        }
        let bulk = sorted(bulk::list(d).unwrap());
        let kind = |n: &str| bulk.iter().find(|(x, _)| x == n).unwrap().1.kind;
        assert_eq!(kind("sub"), Kind::Dir);
        assert_eq!(kind("sym"), Kind::Symlink);
        assert_eq!(kind("fifo"), Kind::Other);
        assert_eq!(kind("big.bin"), Kind::File);
    }

    #[test]
    fn unreadable_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let res = list(&locked, 0);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(res.is_err());
    }
}
