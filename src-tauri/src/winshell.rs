//! Windows shell integration: thumbnails and icons drawn by the shell, File
//! Explorer, starting programs and links, and registry reads.
//!
//! Nothing here removes files. Programs and links are started with
//! `ShellExecuteExW` and an explicit file and parameter string, never through
//! a command shell. The pixel and path helpers at the top are plain logic so
//! they are tested on every platform.

/// Turn the shell's 32-bit BGRA pixels into straight RGBA, in place.
///
/// Icons come with premultiplied alpha; photos often come with an alpha
/// channel of zero, meaning "no alpha". Both are handled.
pub fn bgra_to_rgba(px: &mut [u8]) {
    let chunks = || px.chunks_exact(4);
    let no_alpha = chunks().all(|p| p[3] == 0);
    let premultiplied = !no_alpha && chunks().all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
        if no_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] > 0 && p[3] < 255 {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

#[cfg(windows)]
pub use imp::*;

#[cfg(windows)]
mod imp {
    use std::ffi::{c_void, OsStr};
    use std::path::Path;

    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::{ERROR_CANCELLED, ERROR_MORE_DATA, ERROR_SUCCESS, SIZE};
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, IBindCtx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        REG_SAM_FLAGS, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };
    use windows::Win32::UI::Shell::{
        ILFree, IShellItemImageFactory, SHCreateItemFromParsingName, SHLoadIndirectString,
        SHOpenFolderAndSelectItems, SHParseDisplayName, ShellExecuteExW, SEE_MASK_FLAG_NO_UI,
        SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SIIGBF,
    };
    pub use windows::Win32::UI::Shell::{SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::bgra_to_rgba;

    /// PNG bytes for straight RGBA pixels.
    pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(rgba).ok()?;
        }
        Some(out)
    }

    fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
        crate::platform::wide(s.as_ref())
    }

    /// COM for the current thread, set up once and released when the
    /// thread ends. Tauri's blocking pool reuses threads, so this is cheap.
    struct Com(bool);

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: balances the successful CoInitializeEx below.
                unsafe { CoUninitialize() };
            }
        }
    }

    thread_local! {
        // SAFETY: CoInitializeEx with no reserved pointer; a failure (COM
        // already set up in another mode) leaves COM usable and is not undone.
        static COM: Com = Com(unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok()
        });
    }

    pub fn ensure_com() {
        COM.with(|_| {});
    }

    /// A PNG of what the shell shows for a file or shell item: its thumbnail
    /// (photos, videos, documents) or its icon, fitted in `px` pixels.
    /// `name` is a file path or a shell parsing name such as
    /// `shell:AppsFolder\<AppUserModelID>`.
    pub fn shell_image_png(name: &OsStr, px: u32, flags: SIIGBF) -> Option<Vec<u8>> {
        ensure_com();
        let w = wide(name);
        let px = px.clamp(16, 1024) as i32;
        // SAFETY: `w` is NUL-terminated and outlives the call.
        let factory: IShellItemImageFactory =
            unsafe { SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None::<&IBindCtx>) }.ok()?;
        // SAFETY: a valid factory; the returned bitmap is ours to delete.
        let bitmap = unsafe { factory.GetImage(SIZE { cx: px, cy: px }, flags) }.ok()?;
        let png = bitmap_png(bitmap);
        // SAFETY: the bitmap came from GetImage and is not used afterwards.
        unsafe {
            let _ = DeleteObject(bitmap.into());
        }
        png
    }

    fn bitmap_png(bitmap: HBITMAP) -> Option<Vec<u8>> {
        let mut info = BITMAP::default();
        // SAFETY: `info` is a BITMAP of the size passed.
        let n = unsafe {
            GetObjectW(
                bitmap.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut info as *mut BITMAP as *mut c_void),
            )
        };
        let (w, h) = (info.bmWidth, info.bmHeight.abs());
        if n == 0 || !(1..=4096).contains(&w) || !(1..=4096).contains(&h) {
            return None;
        }
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // Negative: rows top to bottom.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        // SAFETY: the screen DC is released below; `px` holds w*h 32-bit
        // pixels, as the header describes.
        let lines = unsafe {
            let dc = GetDC(None);
            let lines = GetDIBits(
                dc,
                bitmap,
                0,
                h as u32,
                Some(px.as_mut_ptr().cast()),
                &mut header,
                DIB_RGB_COLORS,
            );
            ReleaseDC(None, dc);
            lines
        };
        if lines != h {
            return None;
        }
        bgra_to_rgba(&mut px);
        encode_png(w as u32, h as u32, &px)
    }

    /// Open File Explorer at the item's folder with the item selected.
    pub fn reveal(path: &Path) -> windows::core::Result<()> {
        ensure_com();
        let w = wide(path.as_os_str());
        let mut pidl = std::ptr::null_mut();
        // SAFETY: `w` is NUL-terminated; `pidl` receives an ID list that is
        // freed below.
        unsafe {
            SHParseDisplayName(PCWSTR(w.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None)?;
            // With no child items, the folder ID list is the item to select.
            let result = SHOpenFolderAndSelectItems(pidl, None, 0);
            ILFree(Some(pidl));
            result
        }
    }

    /// Start a program, or open a link or shell item, with ShellExecuteEx.
    /// No command shell is involved: `file` is what runs and `params` is
    /// passed to it as its command line. With `quiet`, Windows shows no
    /// dialog of its own when nothing can open `file`.
    pub fn shell_open(
        file: &OsStr,
        params: Option<&OsStr>,
        quiet: bool,
    ) -> windows::core::Result<()> {
        ensure_com();
        let verb = wide("open");
        let file = wide(file);
        let params = params.map(wide);
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | if quiet { SEE_MASK_FLAG_NO_UI } else { 0 },
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: params
                .as_ref()
                .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        // SAFETY: every string in `info` is NUL-terminated and outlives the
        // call; SEE_MASK_NOASYNC makes the call finish before returning.
        unsafe { ShellExecuteExW(&mut info) }
    }

    /// Whether the error means the person declined (for example at the UAC
    /// prompt).
    pub fn is_cancelled(e: &windows::core::Error) -> bool {
        e.code() == HRESULT::from_win32(ERROR_CANCELLED.0)
    }

    /// Resolve an indirect string such as `@{Package?ms-resource://...}`.
    pub fn indirect_string(source: &str) -> Option<String> {
        ensure_com();
        let w = wide(source);
        let mut buf = [0u16; 512];
        // SAFETY: `w` is NUL-terminated; the buffer length is passed.
        unsafe { SHLoadIndirectString(PCWSTR(w.as_ptr()), &mut buf, None) }.ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    /// An open registry key, closed when dropped.
    pub struct RegKey(HKEY, REG_SAM_FLAGS);

    impl Drop for RegKey {
        fn drop(&mut self) {
            // SAFETY: the key was opened by RegOpenKeyExW and is closed once.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    impl RegKey {
        /// Open `path` under `root` for reading. `view` is 0 or one of the
        /// `KEY_WOW64_*` flags.
        pub fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Option<RegKey> {
            let w = wide(path);
            let mut key = HKEY::default();
            // SAFETY: `w` is NUL-terminated; `key` receives a handle we close.
            let r =
                unsafe { RegOpenKeyExW(root, PCWSTR(w.as_ptr()), None, KEY_READ | view, &mut key) };
            (r == ERROR_SUCCESS).then_some(RegKey(key, view))
        }

        pub fn subkey(&self, name: &str) -> Option<RegKey> {
            RegKey::open(self.0, name, self.1)
        }

        pub fn subkey_names(&self) -> Vec<String> {
            let mut out = Vec::new();
            let mut buf = [0u16; 256];
            for i in 0.. {
                let mut len = buf.len() as u32;
                // SAFETY: `buf` holds `len` characters.
                let r = unsafe {
                    RegEnumKeyExW(
                        self.0,
                        i,
                        Some(windows::core::PWSTR(buf.as_mut_ptr())),
                        &mut len,
                        None,
                        None,
                        None,
                        None,
                    )
                };
                if r == ERROR_MORE_DATA {
                    continue; // A name over 255 characters is not valid; skip it.
                }
                if r != ERROR_SUCCESS {
                    break;
                }
                out.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            out
        }

        /// A string value, with environment variables expanded.
        pub fn string(&self, name: &str) -> Option<String> {
            let name = wide(name);
            let mut buf: Vec<u16> = vec![0; 260];
            for _ in 0..4 {
                let mut bytes = (buf.len() * 2) as u32;
                // SAFETY: `buf` holds `bytes` bytes; `name` is NUL-terminated.
                let r = unsafe {
                    RegGetValueW(
                        self.0,
                        PCWSTR::null(),
                        PCWSTR(name.as_ptr()),
                        RRF_RT_REG_SZ,
                        None,
                        Some(buf.as_mut_ptr().cast()),
                        Some(&mut bytes),
                    )
                };
                if r == ERROR_MORE_DATA {
                    buf = vec![0; bytes as usize / 2 + 1];
                    continue;
                }
                if r != ERROR_SUCCESS {
                    return None;
                }
                let chars = &buf[..(bytes as usize / 2).min(buf.len())];
                let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
                let s = String::from_utf16_lossy(&chars[..end]).trim().to_string();
                return (!s.is_empty()).then_some(s);
            }
            None
        }

        pub fn dword(&self, name: &str) -> Option<u32> {
            let name = wide(name);
            let mut value = 0u32;
            let mut bytes = 4u32;
            // SAFETY: `value` is 4 bytes, as `bytes` says.
            let r = unsafe {
                RegGetValueW(
                    self.0,
                    PCWSTR::null(),
                    PCWSTR(name.as_ptr()),
                    RRF_RT_REG_DWORD,
                    None,
                    Some((&mut value as *mut u32).cast()),
                    Some(&mut bytes),
                )
            };
            (r == ERROR_SUCCESS).then_some(value)
        }
    }

    /// Whether the taskbar uses the light theme, so the tray icon is drawn
    /// dark. The default (no setting) is the dark taskbar.
    pub fn light_taskbar() -> bool {
        RegKey::open(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            REG_SAM_FLAGS(0),
        )
        .and_then(|k| k.dword("SystemUsesLightTheme"))
            == Some(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_premultiplied_icons_and_opaque_photos() {
        // Half-transparent red, premultiplied, in BGRA.
        let mut icon = vec![0, 0, 64, 128, 0, 0, 0, 0];
        bgra_to_rgba(&mut icon);
        assert_eq!(icon, vec![128, 0, 0, 128, 0, 0, 0, 0]);

        // A photo with no alpha channel at all.
        let mut photo = vec![10, 20, 30, 0];
        bgra_to_rgba(&mut photo);
        assert_eq!(photo, vec![30, 20, 10, 255]);

        // Straight alpha (a channel above alpha) is left as it is.
        let mut straight = vec![200, 0, 0, 100];
        bgra_to_rgba(&mut straight);
        assert_eq!(straight, vec![0, 0, 200, 100]);
    }

    #[cfg(windows)]
    #[test]
    fn encodes_a_png() {
        let png = encode_png(1, 1, &[1, 2, 3, 4]).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[cfg(windows)]
    #[test]
    fn reads_expanded_registry_strings_and_draws_icons() {
        use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY};
        let key = RegKey::open(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion",
            KEY_WOW64_64KEY,
        )
        .expect("CurrentVersion key");
        let pf = key.string("ProgramFilesDir").expect("ProgramFilesDir");
        assert!(pf.to_lowercase().ends_with("program files"), "{pf}");
        // REG_EXPAND_SZ, expanded on read.
        let path = key.string("ProgramFilesPath").expect("ProgramFilesPath");
        assert!(!path.contains('%'), "{path}");
        assert!(!key.subkey_names().is_empty());

        let notepad = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap())
            .join("System32")
            .join("notepad.exe");
        let png = shell_image_png(notepad.as_os_str(), 64, SIIGBF_ICONONLY).expect("an icon");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
