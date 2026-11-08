use std::ffi::OsStr;
use std::fs::Metadata;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE, SYSTEMTIME};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_OFFLINE,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

use super::FileIdentity;

/// Cluster size assumed when only the length is known. NTFS uses 4 KiB on
/// almost every volume; the scanner reads exact allocation sizes instead.
const CLUSTER: u64 = 4096;

/// Attributes of a cloud file (OneDrive and others) whose data is not on
/// this PC. Reading it would download it, so it is never opened for data.
pub const NOT_LOCAL: u32 =
    FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN;

/// Every file reports the same device: walks never follow reparse points,
/// which are the only way into another volume (see the module docs).
pub fn device(_meta: &Metadata) -> u64 {
    0
}

/// Unknown from metadata alone; [`identity_at`] reads it.
pub fn link_count(_meta: &Metadata) -> u64 {
    1
}

/// Not available from metadata alone on Windows; use [`identity_at`].
pub fn identity(_meta: &Metadata) -> Option<FileIdentity> {
    None
}

/// UTF-16, NUL-terminated, for the wide Win32 calls.
pub fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Identity of the item at `path` itself. Opens it for attributes only, so a
/// cloud file is not downloaded and nothing is locked.
pub fn identity_at(path: &Path) -> Option<FileIdentity> {
    let w = wide(path.as_os_str());
    // SAFETY: `w` is NUL-terminated and outlives the call; the handle is
    // closed below.
    let h = unsafe {
        CreateFileW(
            w.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: plain data filled in by the call.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(h, &mut info) } != 0;
    unsafe { CloseHandle(h) };
    ok.then(|| FileIdentity {
        volume: u64::from(info.dwVolumeSerialNumber),
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        links: u64::from(info.nNumberOfLinks),
    })
}

/// Hidden or system files, which Explorer does not show by default.
pub fn hidden(meta: &Metadata) -> bool {
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};
    meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

/// Whether the file's data is on this computer. A cloud file that is online
/// only must not be read: reading it downloads it.
pub fn is_local(meta: &Metadata) -> bool {
    meta.file_attributes() & NOT_LOCAL == 0
}

/// Whether the file could be deleted now: opening it for delete access fails
/// while another program has it open without sharing deletion.
pub fn deletable(path: &Path) -> bool {
    use windows_sys::Win32::Storage::FileSystem::DELETE;
    let w = wide(path.as_os_str());
    // SAFETY: `w` is NUL-terminated; the handle is closed right away.
    let h = unsafe {
        CreateFileW(
            w.as_ptr(),
            DELETE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return false;
    }
    unsafe { CloseHandle(h) };
    true
}

/// Bytes the file occupies on this PC: nothing for a cloud file that is
/// online only, otherwise its length rounded up to whole clusters.
pub fn allocated_bytes(meta: &Metadata) -> u64 {
    if meta.is_dir() || meta.file_attributes() & NOT_LOCAL != 0 {
        return 0;
    }
    meta.len().div_ceil(CLUSTER).saturating_mul(CLUSTER)
}

pub fn read_only(meta: &Metadata) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_READONLY != 0
}

/// Paths are stored as UTF-8. A name that is not valid Unicode (an unpaired
/// surrogate) is stored as-is and rejected on load, which only costs a
/// rescan.
pub fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

pub fn path_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

/// The UTC offset of local time at `secs` since the epoch, and the local
/// weekday (0 = Sunday) and hour. Daylight saving follows the rules that
/// applied at that moment.
pub fn local_time(secs: i64) -> (i64, u32, u32) {
    // FILETIME counts 100 ns intervals since 1601-01-01.
    const EPOCH_DIFF: i64 = 11_644_473_600;
    let ticks = (secs.saturating_add(EPOCH_DIFF)).max(0) as u64 * 10_000_000;
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: plain data in and out.
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        if FileTimeToSystemTime(&ft, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
        {
            return (0, 0, 0);
        }
        let as_secs = |t: &SYSTEMTIME| {
            days_from_civil(i64::from(t.wYear), i64::from(t.wMonth), i64::from(t.wDay)) * 86_400
                + i64::from(t.wHour) * 3600
                + i64::from(t.wMinute) * 60
                + i64::from(t.wSecond)
        };
        (
            as_secs(&local) - as_secs(&utc),
            u32::from(local.wDayOfWeek),
            u32::from(local.wHour),
        )
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 11, 8), 20_765);
    }
}
