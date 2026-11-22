//! Volume capacity: `statfs(2)` on macOS, the volume APIs on Windows.
//!
//! Note: macOS Finder's "available" figure also counts purgeable space (local
//! Time Machine snapshots, iCloud-optimized files). `statfs` reports only
//! space that is free right now, so DiskPilot's number can be lower.

#[cfg(windows)]
pub use windows::{disk_info, drives, list_drives};

#[cfg(unix)]
pub use unix::{disk_info, list_drives};

#[cfg(unix)]
mod unix {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use crate::model::{DiskInfo, DriveInfo, DriveKind};

    // statfs field widths differ between platforms; the conversions are no-ops on
    // macOS but keep this portable.
    #[allow(clippy::useless_conversion)]
    pub fn disk_info(path: &Path) -> std::io::Result<DiskInfo> {
        let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL")
        })?;
        // SAFETY: `stat` is a plain C struct that statfs fully initializes on
        // success; we only read it when the call returns 0.
        let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let block = u64::from(stat.f_bsize);
        let total = u64::from(stat.f_blocks).saturating_mul(block);
        let available = u64::from(stat.f_bavail).saturating_mul(block);
        let free = u64::from(stat.f_bfree).saturating_mul(block);
        Ok(DiskInfo {
            mount_point: c_chars_to_string(&stat.f_mntonname),
            file_system: c_chars_to_string(&stat.f_fstypename),
            total_bytes: total,
            available_bytes: available,
            used_bytes: total.saturating_sub(free),
        })
    }

    /// The volume that holds `home`; macOS shows no drive picker.
    pub fn list_drives(home: &Path) -> Vec<DriveInfo> {
        disk_info(home)
            .map(|d| {
                vec![DriveInfo {
                    label: String::new(),
                    root: d.mount_point,
                    file_system: d.file_system,
                    total_bytes: d.total_bytes,
                    available_bytes: d.available_bytes,
                    used_bytes: d.used_bytes,
                    kind: DriveKind::Fixed,
                }]
            })
            .unwrap_or_default()
    }

    fn c_chars_to_string(chars: &[libc::c_char]) -> String {
        // SAFETY: statfs NUL-terminates these fixed-size buffers.
        unsafe { CStr::from_ptr(chars.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
        GetVolumePathNameW,
    };

    use crate::model::{DiskInfo, DriveInfo, DriveKind};
    use crate::platform::wide;

    // GetDriveTypeW results (WinBase.h).
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    fn from_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    pub fn disk_info(path: &Path) -> std::io::Result<DiskInfo> {
        let w = wide(path.as_os_str());
        let mut root = [0u16; 1024];
        // SAFETY: both buffers are valid for the lengths given.
        if unsafe { GetVolumePathNameW(w.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: `root` is NUL-terminated; the out pointers are valid.
        if unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut fs_name = [0u16; 64];
        // SAFETY: only the file-system name buffer is requested.
        let named = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs_name.as_mut_ptr(),
                fs_name.len() as u32,
            )
        } != 0;
        Ok(DiskInfo {
            mount_point: from_wide(&root),
            file_system: if named {
                from_wide(&fs_name)
            } else {
                String::new()
            },
            total_bytes: total,
            available_bytes: avail,
            used_bytes: total.saturating_sub(free),
        })
    }

    fn drive_type(root: &Path) -> u32 {
        let w = wide(root.as_os_str());
        // SAFETY: `w` is NUL-terminated.
        unsafe { GetDriveTypeW(w.as_ptr()) }
    }

    /// Fixed and removable drives with a file system, in letter order.
    pub fn drives() -> Vec<PathBuf> {
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| PathBuf::from(format!("{}:\\", char::from(b'A' + i))))
            .filter(|root| {
                let ty = drive_type(root);
                (ty == DRIVE_FIXED || ty == DRIVE_REMOVABLE) && disk_info(root).is_ok()
            })
            .collect()
    }

    fn volume_label(root: &Path) -> String {
        let w = wide(root.as_os_str());
        let mut name = [0u16; 261];
        // SAFETY: only the volume name buffer is requested; `w` is NUL-terminated.
        let ok = unsafe {
            GetVolumeInformationW(
                w.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        } != 0;
        if ok {
            from_wide(&name)
        } else {
            String::new()
        }
    }

    /// Every drive from `drives()` with its label and capacity. `_home` keeps
    /// the signature the same as on macOS.
    pub fn list_drives(_home: &Path) -> Vec<DriveInfo> {
        drives()
            .into_iter()
            .filter_map(|root| {
                let d = disk_info(&root).ok()?;
                Some(DriveInfo {
                    root: root.display().to_string(),
                    label: volume_label(&root),
                    file_system: d.file_system,
                    total_bytes: d.total_bytes,
                    available_bytes: d.available_bytes,
                    used_bytes: d.used_bytes,
                    kind: if drive_type(&root) == DRIVE_REMOVABLE {
                        DriveKind::Removable
                    } else {
                        DriveKind::Fixed
                    },
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_consistent_capacity_for_temp_dir() {
        let info = disk_info(&std::env::temp_dir()).unwrap();
        assert!(info.total_bytes > 0);
        assert!(info.available_bytes <= info.total_bytes);
        assert!(info.used_bytes <= info.total_bytes);
        assert!(!info.mount_point.is_empty());
    }

    #[test]
    fn lists_the_home_drive() {
        let home = std::env::temp_dir();
        let drives = list_drives(&home);
        assert!(!drives.is_empty());
        assert!(drives
            .iter()
            .all(|d| d.total_bytes > 0 && !d.root.is_empty()));
    }
}
