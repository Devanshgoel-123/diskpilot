//! Directory listing with per-entry metadata, without following symlinks.
//!
//! On macOS one `getattrlistbulk` call returns a batch of entries together
//! with their attributes, instead of one `lstat` per entry. Other platforms,
//! and file systems that do not support it, use `read_dir` + `lstat`.

use std::ffi::OsString;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
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
            nlink: if is_dir { 1 } else { meta.nlink() },
            ino: meta.ino(),
            other_volume: kind == Kind::Dir && meta.dev() != root_dev,
        }
    }
}

pub(crate) struct Listing {
    pub entries: Vec<(OsString, Stat)>,
    /// Entries (or the listing itself, part-way) that could not be read.
    pub errors: Vec<(PathBuf, io::Error)>,
}

/// Testing and benchmarking only: list with `read_dir` + `lstat` everywhere.
#[cfg(target_os = "macos")]
#[doc(hidden)]
pub static FORCE_PORTABLE: AtomicBool = AtomicBool::new(false);

/// List `dir`. Fails only if the folder cannot be opened at all.
pub(crate) fn list(dir: &Path, root_dev: u64) -> io::Result<Listing> {
    #[cfg(target_os = "macos")]
    if !FORCE_PORTABLE.load(std::sync::atomic::Ordering::Relaxed) {
        match bulk::list(dir) {
            Err(e) if bulk::unsupported(&e) => {}
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
        match std::fs::symlink_metadata(&child) {
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

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
