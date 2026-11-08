//! Fast scan source for NTFS: read the volume's Master File Table.
//!
//! Every file and folder on an NTFS volume has a record in the MFT, so one
//! sequential read of it (hundreds of MB, in large chunks) replaces opening
//! and listing every folder. This needs administrator rights to open the
//! volume. The volume is opened for reading only; nothing is written.
//! Records are parsed in `ntfs.rs`.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::io;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW, ReadFile,
    FILE_FLAG_NO_BUFFERING, FILE_FLAG_SEQUENTIAL_SCAN, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Ioctl::{FSCTL_GET_NTFS_VOLUME_DATA, NTFS_VOLUME_DATA_BUFFER};
use windows_sys::Win32::System::IO::{DeviceIoControl, OVERLAPPED};

use super::ntfs::{self, Builder, Run, Tree};
use crate::platform::{self, wide};

/// Bytes per read. Large sequential reads let the disk stream the MFT;
/// beyond a few MB the gain is negligible.
const CHUNK: usize = 4 << 20;
/// Alignment for raw volume reads (covers 512-byte and 4K sectors).
const ALIGN: usize = 4096;

struct Volume(HANDLE);
// SAFETY: a file handle may be used from any thread; reads pass their own
// offsets, so no file position is shared.
unsafe impl Send for Volume {}
unsafe impl Sync for Volume {}
impl Drop for Volume {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by `open_volume` and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// A heap buffer aligned for raw volume reads.
struct AlignedBuf {
    ptr: *mut u8,
    len: usize,
}
// SAFETY: the buffer is owned memory, moved between threads whole.
unsafe impl Send for AlignedBuf {}
impl AlignedBuf {
    fn new(len: usize) -> Self {
        let layout = Layout::from_size_align(len.max(ALIGN), ALIGN).expect("valid layout");
        // SAFETY: the layout has a non-zero size.
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "out of memory");
        Self { ptr, len }
    }
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: `ptr` holds `len` initialized bytes owned by self.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}
impl Drop for AlignedBuf {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.len.max(ALIGN), ALIGN).expect("valid layout");
        // SAFETY: allocated in `new` with this layout.
        unsafe { dealloc(self.ptr, layout) };
    }
}

fn unsupported(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, msg.to_string())
}

/// The volume holding `path`, as `\\?\Volume{…}`, opened for reading.
fn open_volume(path: &Path) -> io::Result<Volume> {
    // Volume functions want a plain drive path, not the `\\?\` form that
    // canonical paths use.
    let plain = match path.components().next() {
        Some(Component::Prefix(p)) => match p.kind() {
            Prefix::VerbatimDisk(d) | Prefix::Disk(d) => {
                let mut plain = PathBuf::from(format!("{}:\\", d as char));
                for c in path.components() {
                    if let Component::Normal(name) = c {
                        plain.push(name);
                    }
                }
                plain
            }
            _ => return Err(unsupported("not a local drive")),
        },
        _ => return Err(unsupported("not an absolute path")),
    };
    let w = wide(plain.as_os_str());
    let mut mount = [0u16; 1024];
    // SAFETY: buffers are valid for the lengths passed.
    if unsafe { GetVolumePathNameW(w.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut name = [0u16; 128];
    // SAFETY: `mount` is NUL-terminated by the previous call.
    if unsafe {
        GetVolumeNameForVolumeMountPointW(mount.as_ptr(), name.as_mut_ptr(), name.len() as u32)
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // `\\?\Volume{…}\` names the root folder; without the trailing
    // backslash it names the volume itself.
    let mut len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    if len > 0 && name[len - 1] == u16::from(b'\\') {
        len -= 1;
    }
    let mut volume: Vec<u16> = name[..len].to_vec();
    volume.push(0);
    // SAFETY: NUL-terminated path; the handle is owned by `Volume`.
    let h = unsafe {
        CreateFileW(
            volume.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            // Uncached, so what is read is what is on disk now, not an
            // earlier read of the volume.
            FILE_FLAG_NO_BUFFERING | FILE_FLAG_SEQUENTIAL_SCAN,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Volume(h))
}

fn volume_data(v: &Volume) -> io::Result<NTFS_VOLUME_DATA_BUFFER> {
    // SAFETY: plain data written by the call.
    let mut data: NTFS_VOLUME_DATA_BUFFER = unsafe { std::mem::zeroed() };
    let mut returned = 0u32;
    // SAFETY: the output buffer is valid for its size.
    let ok = unsafe {
        DeviceIoControl(
            v.0,
            FSCTL_GET_NTFS_VOLUME_DATA,
            std::ptr::null(),
            0,
            (&mut data as *mut NTFS_VOLUME_DATA_BUFFER).cast(),
            std::mem::size_of::<NTFS_VOLUME_DATA_BUFFER>() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    } != 0;
    if !ok {
        // Not NTFS (FAT, exFAT, ReFS) or not allowed.
        return Err(io::Error::last_os_error());
    }
    Ok(data)
}

/// Read exactly `buf.len()` bytes at `offset` on the volume.
fn read_at(v: &Volume, offset: u64, buf: &mut [u8]) -> io::Result<()> {
    let mut done = 0usize;
    while done < buf.len() {
        let at = offset + done as u64;
        // SAFETY: plain data; the offset fields select where to read.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.Anonymous.Anonymous.Offset = at as u32;
        ov.Anonymous.Anonymous.OffsetHigh = (at >> 32) as u32;
        let want = u32::try_from(buf.len() - done).unwrap_or(u32::MAX);
        let mut got = 0u32;
        // SAFETY: the buffer is valid for `want` bytes; the handle is
        // synchronous, so the call completes before returning.
        let ok = unsafe { ReadFile(v.0, buf[done..].as_mut_ptr(), want, &mut got, &mut ov) } != 0;
        if !ok {
            return Err(io::Error::last_os_error());
        }
        if got == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        done += got as usize;
    }
    Ok(())
}

struct Geometry {
    cluster: u64,
    record: usize,
}

/// Read record `number` of the MFT through the runs known so far.
fn read_record(v: &Volume, g: &Geometry, runs: &[Run], number: u64) -> io::Result<Vec<u8>> {
    let byte = number * g.record as u64;
    let mut vcn_start = 0u64;
    for run in runs {
        let run_bytes = run.clusters * g.cluster;
        if byte < vcn_start + run_bytes {
            let lcn = run.lcn.ok_or_else(|| unsupported("MFT record in a hole"))?;
            let at = lcn * g.cluster + (byte - vcn_start);
            let aligned = at - at % ALIGN as u64;
            let skip = (at - aligned) as usize;
            let mut buf = AlignedBuf::new((skip + g.record).next_multiple_of(ALIGN));
            read_at(v, aligned, buf.as_mut())?;
            let mut rec = buf.as_mut()[skip..skip + g.record].to_vec();
            ntfs::apply_fixups(&mut rec).map_err(unsupported)?;
            return Ok(rec);
        }
        vcn_start += run_bytes;
    }
    Err(unsupported("MFT record outside the MFT"))
}

/// Where the MFT is on the volume, from its own record 0 (and the records
/// its attribute list names when it is very fragmented).
fn mft_runs(v: &Volume, g: &Geometry, start_lcn: u64) -> io::Result<Vec<Run>> {
    let first = [Run {
        lcn: Some(start_lcn),
        clusters: (g.record as u64).div_ceil(g.cluster),
    }];
    let rec0 = read_record(v, g, &first, 0)?;
    let (_, runs) = ntfs::data_runs(&rec0).ok_or_else(|| unsupported("MFT has no data"))?;
    let mut segments = vec![(0u64, runs)];
    for number in ntfs::data_extension_records(&rec0, 0) {
        let rec = read_record(v, g, &segments[0].1, number)?;
        if let Some(segment) = ntfs::data_runs(&rec) {
            segments.push(segment);
        }
    }
    segments.sort_by_key(|(vcn, _)| *vcn);
    Ok(segments.into_iter().flat_map(|(_, runs)| runs).collect())
}

/// Read and parse the MFT of the volume holding `root`. Returns the tree
/// and the record number of `root`. Fails without administrator rights, on
/// file systems other than NTFS, and when cancelled.
pub(crate) fn load(root: &Path, cancel: &AtomicBool) -> io::Result<(Tree, u64)> {
    let root_record = platform::identity_at(root)
        .ok_or_else(|| unsupported("cannot identify the folder"))?
        .index
        & 0x0000_FFFF_FFFF_FFFF;
    let volume = open_volume(root)?;
    let data = volume_data(&volume)?;
    let g = Geometry {
        cluster: u64::from(data.BytesPerCluster),
        record: data.BytesPerFileRecordSegment as usize,
    };
    if !g.record.is_power_of_two()
        || !(1024..=65536).contains(&g.record)
        || g.cluster == 0
        || g.cluster % g.record as u64 != 0
    {
        // Records that straddle clusters (clusters under 1 KB) are not
        // supported; the normal scan handles those volumes.
        return Err(unsupported("unusual NTFS geometry"));
    }
    let start_lcn = u64::try_from(data.MftStartLcn).map_err(|_| unsupported("bad MFT start"))?;
    let runs = mft_runs(&volume, &g, start_lcn)?;
    let valid = u64::try_from(data.MftValidDataLength).unwrap_or(0);
    let records = valid / g.record as u64;
    let mut builder = Builder::new(records);

    let result = std::thread::scope(|s| -> io::Result<()> {
        // A reader thread keeps the disk busy while records are parsed.
        let (full_tx, full_rx) = mpsc::sync_channel::<io::Result<(u64, AlignedBuf, usize)>>(2);
        let (free_tx, free_rx) = mpsc::channel::<AlignedBuf>();
        let volume = &volume;
        let runs = &runs;
        let g = &g;
        s.spawn(move || {
            let mut vbyte = 0u64;
            for run in runs {
                let run_bytes = run.clusters * g.cluster;
                if let Some(lcn) = run.lcn {
                    let mut off = 0u64;
                    while off < run_bytes && vbyte + off < valid {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let left = (run_bytes - off).min(valid - (vbyte + off));
                        let n = (CHUNK as u64).min(left) as usize;
                        let mut buf = free_rx
                            .try_recv()
                            .unwrap_or_else(|_| AlignedBuf::new(CHUNK));
                        // Reads stay sector-aligned: round the length up.
                        let read_len = n.next_multiple_of(ALIGN).min(CHUNK);
                        let res =
                            read_at(volume, lcn * g.cluster + off, &mut buf.as_mut()[..read_len]);
                        let msg = res.map(|()| ((vbyte + off) / g.record as u64, buf, n));
                        let failed = msg.is_err();
                        if full_tx.send(msg).is_err() || failed {
                            return;
                        }
                        off += n as u64;
                    }
                }
                vbyte += run_bytes;
            }
        });
        for msg in full_rx {
            let (first, mut buf, n) = msg?;
            let usable = n / g.record * g.record;
            builder.add_chunk(first, &mut buf.as_mut()[..usable], g.record);
            let _ = free_tx.send(buf);
        }
        Ok(())
    });
    result?;
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let tree = builder.finish();
    if !tree.is_dir(root_record) {
        return Err(unsupported("folder not in the file table"));
    }
    Ok((tree, root_record))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CI runners are administrators; elsewhere this checks the fallback.
    #[test]
    fn reads_the_file_table_or_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/a.bin"), vec![1u8; 100_000]).unwrap();
        let root = crate::platform::canonicalize(dir.path()).unwrap();
        // New records reach the disk when Windows writes its cache, usually
        // within seconds.
        for _ in 0..40 {
            let (tree, id) = match load(&root, &AtomicBool::new(false)) {
                Ok(loaded) => loaded,
                Err(e) => {
                    assert!(!platform::is_elevated(), "{e}");
                    return;
                }
            };
            let found = tree.list(id).ok().and_then(|top| {
                let (_, sub) = top.entries.iter().find(|(n, _)| n == "sub")?;
                tree.list(sub.ino)
                    .ok()?
                    .entries
                    .into_iter()
                    .find(|(n, _)| n == "a.bin")
            });
            if let Some((_, a)) = found {
                if a.logical == 100_000 {
                    assert!(a.alloc >= 100_000);
                    assert_eq!(a.kind, super::super::listing::Kind::File);
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        panic!("the new file never appeared in the file table");
    }
}
