//! Reading WSL's distribution list from the registry
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss`). Read-only.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};

use super::RegisteredDistro;

const LXSS: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Lxss";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// An open registry key, closed when dropped.
struct Key(HKEY);

impl Key {
    fn open(parent: HKEY, path: &str) -> Option<Self> {
        let path = wide(path);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` is NUL-terminated and `key` a valid out pointer.
        let status = unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, KEY_READ, &mut key) };
        (status == ERROR_SUCCESS).then_some(Key(key))
    }

    /// Names of the key's direct subkeys.
    fn subkeys(&self) -> Vec<String> {
        let mut out = Vec::new();
        for index in 0.. {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            // SAFETY: `buf` holds `len` UTF-16 units; the optional
            // arguments are null.
            let status = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if status != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        out
    }

    /// A string value (`REG_SZ`, or `REG_EXPAND_SZ` expanded).
    fn string(&self, name: &str) -> Option<String> {
        let name = wide(name);
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
        let mut bytes = 0u32;
        // SAFETY: a size query; the data pointer is null.
        let status = unsafe {
            RegGetValueW(
                self.0,
                std::ptr::null(),
                name.as_ptr(),
                flags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut bytes,
            )
        };
        if status != ERROR_SUCCESS && status != ERROR_MORE_DATA {
            return None;
        }
        let mut buf = vec![0u16; (bytes as usize).div_ceil(2) + 1];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `buf` holds `bytes` bytes.
        let status = unsafe {
            RegGetValueW(
                self.0,
                std::ptr::null(),
                name.as_ptr(),
                flags,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let units = &buf[..(bytes as usize / 2)];
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        Some(
            OsString::from_wide(&units[..end])
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn dword(&self, name: &str) -> Option<u32> {
        let name = wide(name);
        let mut value = 0u32;
        let mut bytes = 4u32;
        // SAFETY: `value` holds the 4 bytes a DWORD needs.
        let status = unsafe {
            RegGetValueW(
                self.0,
                std::ptr::null(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut bytes,
            )
        };
        (status == ERROR_SUCCESS).then_some(value)
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `Key::open` and is closed once.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

/// Registered distributions and the default one's GUID.
pub fn distributions() -> (Vec<RegisteredDistro>, Option<String>) {
    let Some(lxss) = Key::open(HKEY_CURRENT_USER, LXSS) else {
        return (Vec::new(), None);
    };
    let default = lxss.string("DefaultDistribution");
    let distros = lxss
        .subkeys()
        .into_iter()
        .filter(|guid| guid.starts_with('{'))
        .filter_map(|guid| {
            let key = Key::open(lxss.0, &guid)?;
            Some(RegisteredDistro {
                name: key.string("DistributionName"),
                base_path: key.string("BasePath"),
                vhd_file_name: key.string("VhdFileName"),
                version: key.dword("Version"),
                guid,
            })
        })
        .collect();
    (distros, default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_registry_without_failing() {
        // CI runners may have no WSL distributions; reading must still work
        // and every entry must have its GUID.
        let (distros, _default) = distributions();
        assert!(distros.iter().all(|d| d.guid.starts_with('{')));
    }

    #[test]
    fn reads_string_and_number_values() {
        // A key every Windows profile has.
        let key = Key::open(
            HKEY_CURRENT_USER,
            "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer",
        );
        assert!(key.is_some());
        let env = Key::open(HKEY_CURRENT_USER, "Environment").unwrap();
        // TEMP is a REG_EXPAND_SZ and comes back expanded.
        if let Some(temp) = env.string("TEMP") {
            assert!(!temp.contains('%'));
        }
        assert!(env.dword("DiskPilotDoesNotExist").is_none());
    }
}
