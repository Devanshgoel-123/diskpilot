//! Who may read a file, and keeping it that way when a file is rewritten.
//!
//! On a Mac this is the Unix mode (`600` = only you). Windows has no mode: a
//! file's access comes from its security descriptor's DACL, and a user folder
//! is not always private (some software adds its own account or group to the
//! profile). There, a file is "private" when only you, SYSTEM and
//! Administrators may read it, which is also what OpenSSH for Windows
//! requires of a private key.

use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::path::Path;

/// The file's permission bits (`0o7777`). Windows has none: 0, and
/// [`copy_access`] carries a rewritten file's access list over instead.
pub fn mode(meta: &Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o7777
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
}

/// Set a path's permission bits. On Windows a private mode (`600`, `700`)
/// makes the file or folder private with [`make_private`]; any other mode
/// leaves it as it is.
pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(windows)]
    {
        if mode != 0 && mode & 0o077 == 0 {
            make_private(path)
        } else {
            Ok(())
        }
    }
}

/// Set an open file's permission bits (Unix only; on Windows use
/// [`copy_access`] or [`set_mode`] with the path).
pub fn set_file_mode(file: &File, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (file, mode);
        Ok(())
    }
}

/// Create new files with these permission bits (Unix only).
pub fn create_with(options: &mut OpenOptions, mode: u32) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        options
    }
}

/// Give `to` the same access as `from`, so a rewritten file (written to a
/// temporary file and moved over the original) keeps who may read it.
pub fn copy_access(from: &Path, from_meta: &Metadata, to: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = from;
        set_mode(to, mode(from_meta))
    }
    #[cfg(windows)]
    {
        let _ = from_meta;
        windows::copy_dacl(from, to)
    }
}

/// Whether accounts other than the file's owner (and, on Windows, SYSTEM and
/// Administrators) can read it.
pub fn others_can_read(path: &Path, meta: &Metadata) -> bool {
    #[cfg(unix)]
    {
        let _ = path;
        mode(meta) & 0o044 != 0
    }
    #[cfg(windows)]
    {
        let _ = meta;
        windows::others_can_read(path).unwrap_or(false)
    }
}

/// Whether the file belongs to the account DiskPilot runs as.
pub fn is_mine(path: &Path, meta: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        meta.uid() == unsafe { libc::getuid() }
    }
    #[cfg(windows)]
    {
        let _ = meta;
        windows::is_mine(path).unwrap_or(false)
    }
}

/// Make a file readable only by you: mode 600 on a Mac; on Windows, a DACL
/// that no longer inherits from its folder and allows only you, SYSTEM and
/// Administrators.
pub fn make_private(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        set_mode(path, 0o600)
    }
    #[cfg(windows)]
    {
        windows::make_private(path)
    }
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use std::io;
    use std::path::Path;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
        NO_MULTIPLE_TRUSTEE, SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN,
        TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        CreateWellKnownSid, EqualSid, GetAce, GetSecurityDescriptorControl, GetTokenInformation,
        TokenUser, WinBuiltinAdministratorsSid, WinCreatorOwnerSid, WinLocalSystemSid, ACL,
        DACL_SECURITY_INFORMATION, NO_INHERITANCE, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED,
        SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_QUERY, TOKEN_USER,
        UNPROTECTED_DACL_SECURITY_INFORMATION, WELL_KNOWN_SID_TYPE,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
    const INHERIT_ONLY_ACE: u8 = 0x08;
    const FILE_READ_DATA: u32 = 0x0001;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_ALL: u32 = 0x1000_0000;
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;

    #[repr(C)]
    struct AceHeader {
        ace_type: u8,
        ace_flags: u8,
        ace_size: u16,
    }

    #[repr(C)]
    struct AccessAllowedAce {
        header: AceHeader,
        mask: u32,
        sid_start: u32,
    }

    fn wide(path: &Path) -> Vec<u16> {
        crate::platform::wide(path.as_os_str())
    }

    fn check(err: u32) -> io::Result<()> {
        if err == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(err as i32))
        }
    }

    /// A security descriptor returned by GetNamedSecurityInfoW, freed on drop.
    struct Descriptor {
        sd: PSECURITY_DESCRIPTOR,
        owner: PSID,
        dacl: *mut ACL,
    }

    impl Drop for Descriptor {
        fn drop(&mut self) {
            if !self.sd.is_null() {
                unsafe { LocalFree(self.sd as _) };
            }
        }
    }

    fn descriptor(path: &Path, info: u32) -> io::Result<Descriptor> {
        let name = wide(path);
        let mut d = Descriptor {
            sd: null_mut(),
            owner: null_mut(),
            dacl: null_mut(),
        };
        let err = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                info,
                if info & OWNER_SECURITY_INFORMATION != 0 {
                    &mut d.owner
                } else {
                    null_mut()
                },
                null_mut(),
                if info & DACL_SECURITY_INFORMATION != 0 {
                    &mut d.dacl
                } else {
                    null_mut()
                },
                null_mut(),
                &mut d.sd,
            )
        };
        check(err)?;
        Ok(d)
    }

    struct Sid(Vec<u8>);

    impl Sid {
        fn ptr(&self) -> PSID {
            self.0.as_ptr() as PSID
        }
    }

    fn well_known(kind: WELL_KNOWN_SID_TYPE) -> io::Result<Sid> {
        let mut buf = vec![0u8; 68]; // SECURITY_MAX_SID_SIZE
        let mut len = buf.len() as u32;
        let ok =
            unsafe { CreateWellKnownSid(kind, null_mut(), buf.as_mut_ptr() as PSID, &mut len) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        buf.truncate(len as usize);
        Ok(Sid(buf))
    }

    /// The account DiskPilot runs as. Owns the buffer GetTokenInformation
    /// filled: the TOKEN_USER's SID pointer points into that same buffer, so
    /// it must never be copied elsewhere. u64 storage keeps it aligned.
    struct Me(Vec<u64>);

    impl Me {
        fn sid(&self) -> PSID {
            unsafe { (*(self.0.as_ptr() as *const TOKEN_USER)).User.Sid }
        }
    }

    fn me() -> io::Result<Me> {
        let mut token: HANDLE = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0u32;
        unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len) };
        // u64 storage keeps TOKEN_USER aligned.
        let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * 8) as u32,
                &mut len,
            )
        };
        let err = io::Error::last_os_error();
        unsafe { CloseHandle(token) };
        if ok == 0 {
            return Err(err);
        }
        Ok(Me(buf))
    }

    fn equal(a: PSID, b: PSID) -> bool {
        unsafe { EqualSid(a, b) != 0 }
    }

    pub fn is_mine(path: &Path) -> io::Result<bool> {
        let d = descriptor(path, OWNER_SECURITY_INFORMATION)?;
        let me = me()?;
        Ok(!d.owner.is_null() && equal(d.owner, me.sid()))
    }

    pub fn others_can_read(path: &Path) -> io::Result<bool> {
        let d = descriptor(path, DACL_SECURITY_INFORMATION)?;
        // No DACL at all means everyone has full access.
        if d.dacl.is_null() {
            return Ok(true);
        }
        let me = me()?;
        let trusted = [
            well_known(WinLocalSystemSid)?,
            well_known(WinBuiltinAdministratorsSid)?,
            well_known(WinCreatorOwnerSid)?,
        ];
        let count = unsafe { (*d.dacl).AceCount };
        for i in 0..count as u32 {
            let mut ace: *mut c_void = null_mut();
            if unsafe { GetAce(d.dacl, i, &mut ace) } == 0 || ace.is_null() {
                continue;
            }
            let ace = ace as *const AccessAllowedAce;
            let header = unsafe { &(*ace).header };
            if header.ace_type != ACCESS_ALLOWED_ACE_TYPE
                || header.ace_flags & INHERIT_ONLY_ACE != 0
            {
                continue;
            }
            let mask = unsafe { (*ace).mask };
            if mask & (FILE_READ_DATA | GENERIC_READ | GENERIC_ALL) == 0 {
                continue;
            }
            let sid = unsafe { &(*ace).sid_start as *const u32 as PSID };
            if equal(sid, me.sid()) || trusted.iter().any(|t| equal(sid, t.ptr())) {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Full access for `sid`. On a folder the entry is inherited, so files
    /// created inside it later are private too.
    fn allow(sid: PSID, folder: bool) -> EXPLICIT_ACCESS_W {
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: SET_ACCESS,
            grfInheritance: if folder {
                SUB_CONTAINERS_AND_OBJECTS_INHERIT
            } else {
                NO_INHERITANCE
            },
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid as _,
            },
        }
    }

    pub fn make_private(path: &Path) -> io::Result<()> {
        let me = me()?;
        let system = well_known(WinLocalSystemSid)?;
        let admins = well_known(WinBuiltinAdministratorsSid)?;
        let folder = path.is_dir();
        let entries = [
            allow(me.sid(), folder),
            allow(system.ptr(), folder),
            allow(admins.ptr(), folder),
        ];
        let mut acl: *mut ACL = null_mut();
        check(unsafe {
            SetEntriesInAclW(entries.len() as u32, entries.as_ptr(), null(), &mut acl)
        })?;
        let name = wide(path);
        let err = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                // Protected: stop inheriting the folder's entries.
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null(),
            )
        };
        unsafe { LocalFree(acl as _) };
        check(err)
    }

    pub fn copy_dacl(from: &Path, to: &Path) -> io::Result<()> {
        let d = descriptor(from, DACL_SECURITY_INFORMATION)?;
        let mut control = 0u16;
        let mut revision = 0u32;
        unsafe { GetSecurityDescriptorControl(d.sd, &mut control, &mut revision) };
        let protection = if control & SE_DACL_PROTECTED != 0 {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        };
        let name = wide(to);
        check(unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | protection,
                null_mut(),
                null_mut(),
                d.dacl,
                null(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_file_in_your_own_folder_is_yours() {
        let dir = crate::test_support::sandbox();
        let f = dir.path().join("id_test");
        std::fs::write(&f, "x").unwrap();
        assert!(is_mine(&f, &f.metadata().unwrap()));
    }

    #[test]
    fn make_private_leaves_only_you_able_to_read() {
        let dir = crate::test_support::sandbox();
        let f = dir.path().join("prod.pem");
        std::fs::write(&f, "x").unwrap();
        make_private(&f).unwrap();
        assert!(!others_can_read(&f, &f.metadata().unwrap()));
        // Still readable by you.
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "x");
    }

    #[test]
    fn copy_access_carries_a_private_file_over() {
        let dir = crate::test_support::sandbox();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        make_private(&a).unwrap();
        copy_access(&a, &a.metadata().unwrap(), &b).unwrap();
        assert!(!others_can_read(&b, &b.metadata().unwrap()));
    }

    #[cfg(windows)]
    #[test]
    fn a_file_everyone_may_read_is_reported() {
        let dir = crate::test_support::sandbox();
        let f = dir.path().join("shared.pem");
        std::fs::write(&f, "x").unwrap();
        // Grant Everyone read, the way a file unzipped from elsewhere might be.
        let status = std::process::Command::new("icacls")
            .arg(&f)
            .args(["/grant", "*S-1-1-0:R"])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(others_can_read(&f, &f.metadata().unwrap()));
        make_private(&f).unwrap();
        assert!(!others_can_read(&f, &f.metadata().unwrap()));
    }
}
