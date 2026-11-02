//! Volume capacity via `statfs(2)`.
//!
//! Note: macOS Finder's "available" figure also counts purgeable space (local
//! Time Machine snapshots, iCloud-optimized files). `statfs` reports only
//! space that is free right now, so DiskPilot's number can be lower.

use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::model::DiskInfo;

// statfs field widths differ between platforms; the conversions are no-ops on
// macOS but keep this portable.
#[allow(clippy::useless_conversion)]
pub fn disk_info(path: &Path) -> std::io::Result<DiskInfo> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL"))?;
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

fn c_chars_to_string(chars: &[libc::c_char]) -> String {
    // SAFETY: statfs NUL-terminates these fixed-size buffers.
    unsafe { CStr::from_ptr(chars.as_ptr()) }
        .to_string_lossy()
        .into_owned()
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
}
