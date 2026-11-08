//! WSL 2 and Docker Desktop virtual disks on Windows.
//!
//! Every WSL 2 distribution and Docker Desktop keep their Linux file system
//! in one `.vhdx` file. The file grows as Linux writes data but does not
//! shrink when that data is deleted, so a disk can hold far more space on
//! Windows than Linux uses. This module lists the disks with their size on
//! disk and, where it can be measured without starting anything, how much of
//! it Linux still uses.
//!
//! Reading never starts a stopped distribution. Changes (compacting a disk,
//! or making it sparse so it shrinks by itself) go through [`compact`]: a
//! single-use preview the person confirms by typing the disk's name, with
//! the exact commands shown first, and administrator steps behind a UAC
//! prompt.

pub mod compact;
#[cfg(windows)]
mod registry;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// What a virtual disk belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiskKind {
    /// A WSL 2 distribution such as Ubuntu.
    Distro,
    /// Docker Desktop's images, containers, volumes and build cache.
    DockerData,
    /// Docker Desktop's own small system distribution.
    DockerSystem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDisk {
    /// Stable id for this disk: `distro:{GUID}` or `docker:{file}`.
    pub id: String,
    pub kind: DiskKind,
    /// The distribution's name, or "Docker Desktop data".
    pub name: String,
    /// The registered WSL distribution, when there is one.
    pub distro: Option<String>,
    /// The `.vhdx` file.
    pub path: String,
    /// The file's length.
    pub file_bytes: u64,
    /// Space the file takes on the drive (less than its length when sparse).
    pub allocated_bytes: u64,
    /// The file is sparse, so space Linux frees returns to Windows by itself.
    pub sparse: bool,
    /// Whether the distribution is running; `None` when not known.
    pub running: Option<bool>,
    pub is_default: bool,
    /// Space Linux uses inside the disk, when it could be measured.
    pub used_bytes: Option<u64>,
    /// Roughly what compacting returns to Windows: space on the drive minus
    /// space in use inside.
    pub reclaimable_bytes: Option<u64>,
    /// How `used_bytes` was found, or why it is unknown.
    pub usage_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslOverview {
    /// False on macOS.
    pub supported: bool,
    pub disks: Vec<VirtualDisk>,
    /// Space all disks take on the drive.
    pub total_bytes: u64,
    /// Sum of the reclaimable estimates that are known.
    pub reclaimable_bytes: u64,
    pub message: Option<String>,
}

/// A distribution as registered under
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss\{GUID}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegisteredDistro {
    pub guid: String,
    pub name: Option<String>,
    pub base_path: Option<String>,
    pub vhd_file_name: Option<String>,
    /// 1 or 2; missing on some older registrations.
    pub version: Option<u32>,
}

/// A disk found on this computer, before it is measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDisk {
    pub id: String,
    pub kind: DiskKind,
    pub name: String,
    pub distro: Option<String>,
    pub path: PathBuf,
    pub is_default: bool,
}

pub const DOCKER_DATA_NAME: &str = "Docker Desktop data";
pub const DOCKER_SYSTEM_NAME: &str = "Docker Desktop system";

/// The `.vhdx` path for a registered distribution. `BasePath` may carry the
/// `\\?\` prefix; it is kept so long paths keep working.
pub fn vhdx_path(d: &RegisteredDistro) -> Option<PathBuf> {
    let base = d.base_path.as_deref().filter(|b| !b.is_empty())?;
    let file = d
        .vhd_file_name
        .as_deref()
        .filter(|f| !f.is_empty())
        .unwrap_or("ext4.vhdx");
    if file.contains(['\\', '/']) || file.contains("..") {
        return None;
    }
    Some(Path::new(base).join(file))
}

/// Distributions with a WSL 2 disk, from registry data. WSL 1 keeps files
/// in a folder, not a disk, and is left out.
pub fn distro_disks(registered: &[RegisteredDistro], default_guid: Option<&str>) -> Vec<FoundDisk> {
    registered
        .iter()
        .filter(|d| d.version.unwrap_or(2) == 2)
        .filter_map(|d| {
            let name = d.name.clone().filter(|n| !n.is_empty())?;
            let path = vhdx_path(d)?;
            let kind = match name.as_str() {
                "docker-desktop-data" => DiskKind::DockerData,
                "docker-desktop" => DiskKind::DockerSystem,
                _ => DiskKind::Distro,
            };
            Some(FoundDisk {
                id: format!("distro:{}", d.guid.trim_matches(['{', '}'])),
                kind,
                name: name.clone(),
                distro: Some(name),
                path,
                is_default: default_guid.is_some_and(|g| g.eq_ignore_ascii_case(&d.guid)),
            })
        })
        .collect()
}

/// `CustomWslDistroDir` from Docker Desktop's settings, when the person
/// moved its disks.
pub fn docker_custom_dir(settings_json: &str) -> Option<PathBuf> {
    let v: serde_json::Value = serde_json::from_str(settings_json).ok()?;
    v.as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("customWslDistroDir"))
        .and_then(|(_, v)| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

/// Where Docker Desktop keeps its disks: `docker_data.vhdx` in `disk`
/// (Docker Desktop 4.30 and later), or `ext4.vhdx` in `data` (earlier), and
/// its system distribution in `main`.
pub fn docker_disk_candidates(local_app_data: &Path, custom: Option<&Path>) -> Vec<FoundDisk> {
    let default = local_app_data.join("Docker").join("wsl");
    let mut roots = vec![default];
    if let Some(c) = custom {
        roots.insert(0, c.to_path_buf());
    }
    let mut out = Vec::new();
    for root in roots {
        for (kind, rel) in [
            (DiskKind::DockerData, ["disk", "docker_data.vhdx"]),
            (DiskKind::DockerData, ["data", "ext4.vhdx"]),
            (DiskKind::DockerSystem, ["main", "ext4.vhdx"]),
        ] {
            let path = root.join(rel[0]).join(rel[1]);
            out.push(FoundDisk {
                id: format!("docker:{}", path.display()),
                kind,
                name: match kind {
                    DiskKind::DockerSystem => DOCKER_SYSTEM_NAME,
                    _ => DOCKER_DATA_NAME,
                }
                .into(),
                distro: None,
                path,
                is_default: false,
            });
        }
    }
    out
}

/// Comparable form of a path: without `\\?\`, separators unified, lowercase
/// (Windows paths are case-insensitive).
pub fn path_key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    text.trim_start_matches("\\\\?\\").to_lowercase()
}

/// Registered distributions first; a Docker disk already registered as a
/// distribution (`docker-desktop-data`) is listed once.
pub fn merge(distros: Vec<FoundDisk>, docker: Vec<FoundDisk>) -> Vec<FoundDisk> {
    let mut out = distros;
    for d in docker {
        if !out.iter().any(|o| path_key(&o.path) == path_key(&d.path)) {
            out.push(d);
        }
    }
    out
}

/// Decode what `wsl.exe` printed: UTF-16LE unless `WSL_UTF8=1` took effect.
pub fn decode_output(bytes: &[u8]) -> String {
    let utf16 = bytes.len() >= 2
        && bytes.len() % 2 == 0
        && bytes.iter().skip(1).step_by(2).all(|b| *b == 0);
    if utf16 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .to_string()
    } else {
        String::from_utf8_lossy(bytes).replace('\0', "")
    }
}

/// Names from `wsl.exe --list --running --quiet`.
pub fn parse_running(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().trim_matches('\0'))
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Used bytes of `/` from `df -Pk /` (POSIX format, 1024-byte blocks).
pub fn parse_df_used(text: &str) -> Option<u64> {
    let line = text.lines().skip(1).find(|l| !l.trim().is_empty())?;
    let used: u64 = line.split_whitespace().nth(2)?.parse().ok()?;
    used.checked_mul(1024)
}

/// Whether `name` is safe to pass as a distribution name: what WSL itself
/// allows, and never something `wsl.exe` would read as an option.
pub fn valid_distro_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Space on the drive minus space used inside, when both are known.
pub fn reclaimable(allocated: u64, used: Option<u64>) -> Option<u64> {
    used.map(|u| allocated.saturating_sub(u))
}

/// Size facts about one `.vhdx` file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileSize {
    pub len: u64,
    pub allocated: u64,
    pub sparse: bool,
}

/// Length, space on the drive, and the sparse attribute. Links and folders
/// are refused: a disk is always a regular file.
pub fn file_size(path: &Path) -> Option<FileSize> {
    let meta = path.symlink_metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::{
            GetCompressedFileSizeW, FILE_ATTRIBUTE_SPARSE_FILE, INVALID_FILE_SIZE,
        };
        let wide = crate::platform::wide(path.as_os_str());
        let mut high = 0u32;
        // SAFETY: `wide` is a NUL-terminated UTF-16 path and `high` a valid
        // out pointer for the call's duration.
        let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
        let allocated = if low == INVALID_FILE_SIZE
            // SAFETY: reads the calling thread's last error only.
            && unsafe { windows_sys::Win32::Foundation::GetLastError() } != 0
        {
            meta.len()
        } else {
            (u64::from(high) << 32) | u64::from(low)
        };
        Some(FileSize {
            len: meta.len(),
            allocated,
            sparse: meta.file_attributes() & FILE_ATTRIBUTE_SPARSE_FILE != 0,
        })
    }
    #[cfg(not(windows))]
    Some(FileSize {
        len: meta.len(),
        allocated: crate::platform::allocated_bytes(&meta),
        sparse: false,
    })
}

/// Absolute path of a program in `System32`, so a same-named file earlier
/// on PATH never runs.
pub fn system32(system_root: &Path, program: &str) -> PathBuf {
    system_root.join("System32").join(program)
}

/// `%SystemRoot%`, normally `C:\Windows`.
pub fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
}

/// Run a program without a shell or console window. Returns whether it
/// succeeded and what it printed (stdout, then stderr).
pub fn run_captured(program: &Path, args: &[&str], timeout: Duration) -> Option<(bool, String)> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut command = Command::new(program);
    command
        .args(args)
        .env("WSL_UTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::devtools::hide_console(&mut command);
    let mut child = command.spawn().ok()?;
    let mut out = child.stdout.take()?;
    let mut err = child.stderr.take()?;
    let read_all = |r: &mut dyn Read| {
        let mut buf = Vec::new();
        let _ = r.take(4 * 1024 * 1024).read_to_end(&mut buf);
        let _ = std::io::copy(r, &mut std::io::sink());
        buf
    };
    let out_t = std::thread::spawn(move || read_all(&mut out));
    let err_t = std::thread::spawn(move || read_all(&mut err));
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => return None,
        }
    };
    let mut text = decode_output(&out_t.join().ok()?);
    let err = decode_output(&err_t.join().ok()?);
    if !err.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(err.trim());
    }
    Some((status.success(), text))
}

/// Disks registered with WSL and the ones Docker Desktop keeps, that exist.
pub fn find_disks(home: &Path) -> Vec<FoundDisk> {
    #[cfg(windows)]
    {
        let (registered, default) = registry::distributions();
        let (roaming, local) = crate::devtools::windows_app_data(home);
        let custom = ["settings-store.json", "settings.json"]
            .iter()
            .find_map(|f| {
                crate::devtools::read_small(&roaming.join("Docker").join(f), 4 * 1024 * 1024)
            })
            .and_then(|t| docker_custom_dir(&t));
        merge(
            distro_disks(&registered, default.as_deref()),
            docker_disk_candidates(&local, custom.as_deref()),
        )
        .into_iter()
        .filter(|d| file_size(&d.path).is_some())
        .collect()
    }
    #[cfg(not(windows))]
    {
        let _ = home;
        Vec::new()
    }
}

/// Measure every disk: its size on the drive and, without starting
/// anything, how much Linux uses inside.
pub fn overview(home: &Path, docker_used: Option<u64>) -> WslOverview {
    if !cfg!(windows) {
        return WslOverview {
            supported: false,
            disks: Vec::new(),
            total_bytes: 0,
            reclaimable_bytes: 0,
            message: Some("WSL is only available on Windows.".into()),
        };
    }
    let found = find_disks(home);
    let wsl = system32(&system_root(), "wsl.exe");
    let running: Option<Vec<String>> = if found.iter().any(|d| d.distro.is_some()) {
        match run_captured(
            &wsl,
            &["--list", "--running", "--quiet"],
            Duration::from_secs(15),
        ) {
            Some((true, text)) => Some(parse_running(&text)),
            // "There are no running distributions." exits with an error.
            Some((false, _)) => Some(Vec::new()),
            None => None,
        }
    } else {
        None
    };
    let disks: Vec<VirtualDisk> = found
        .into_iter()
        .filter_map(|d| {
            let size = file_size(&d.path)?;
            let is_running = d
                .distro
                .as_ref()
                .and_then(|n| running.as_ref().map(|r| r.iter().any(|x| x == n)));
            let (used, note) = match d.kind {
                DiskKind::DockerData => match docker_used {
                    Some(u) => (
                        Some(u),
                        "Docker reports this much in use for images, containers, volumes and build cache.".to_string(),
                    ),
                    None => (
                        None,
                        "Unknown until Docker Desktop is running.".to_string(),
                    ),
                },
                DiskKind::DockerSystem => (
                    None,
                    "Docker Desktop's own system files. Usually small.".to_string(),
                ),
                DiskKind::Distro => match (&d.distro, is_running) {
                    (Some(name), Some(true)) if valid_distro_name(name) => {
                        match measure_running(&wsl, name) {
                            Some(u) => (
                                Some(u),
                                "Measured inside the running distribution.".to_string(),
                            ),
                            None => (
                                None,
                                "Could not be measured inside the distribution.".to_string(),
                            ),
                        }
                    }
                    (_, Some(false)) => (
                        None,
                        "Unknown until it is running. DiskPilot does not start it to measure.".to_string(),
                    ),
                    _ => (None, "Unknown.".to_string()),
                },
            };
            Some(VirtualDisk {
                id: d.id,
                kind: d.kind,
                name: d.name,
                distro: d.distro,
                path: d.path.to_string_lossy().trim_start_matches("\\\\?\\").to_string(),
                file_bytes: size.len,
                allocated_bytes: size.allocated,
                sparse: size.sparse,
                running: is_running,
                is_default: d.is_default,
                used_bytes: used,
                reclaimable_bytes: reclaimable(size.allocated, used),
                usage_note: note,
            })
        })
        .collect();
    WslOverview {
        supported: true,
        total_bytes: disks.iter().map(|d| d.allocated_bytes).sum(),
        reclaimable_bytes: disks.iter().filter_map(|d| d.reclaimable_bytes).sum(),
        message: disks
            .is_empty()
            .then(|| "No WSL 2 or Docker Desktop disks were found.".to_string()),
        disks,
    }
}

/// Space used on `/` inside a distribution that is already running.
/// `--exec` runs `df` directly, without a Linux shell.
fn measure_running(wsl: &Path, name: &str) -> Option<u64> {
    let (ok, text) = run_captured(
        wsl,
        &["-d", name, "--exec", "df", "-Pk", "/"],
        Duration::from_secs(20),
    )?;
    ok.then(|| parse_df_used(&text)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(guid: &str, name: &str, base: &str, version: Option<u32>) -> RegisteredDistro {
        RegisteredDistro {
            guid: guid.into(),
            name: Some(name.into()),
            base_path: Some(base.into()),
            vhd_file_name: None,
            version,
        }
    }

    #[test]
    fn registered_wsl2_distributions_become_disks() {
        let registered = vec![
            reg(
                "{11111111-aaaa}",
                "Ubuntu-24.04",
                "C:\\Users\\a\\AppData\\Local\\Packages\\Ubuntu\\LocalState",
                Some(2),
            ),
            reg("{2222}", "Legacy", "C:\\wsl1", Some(1)),
            RegisteredDistro {
                vhd_file_name: Some("custom.vhdx".into()),
                ..reg("{3333}", "docker-desktop-data", "\\\\?\\D:\\docker", None)
            },
            RegisteredDistro {
                name: None,
                ..reg("{4444}", "", "C:\\x", Some(2))
            },
            RegisteredDistro {
                vhd_file_name: Some("..\\..\\evil.vhdx".into()),
                ..reg("{5555}", "Bad", "C:\\x", Some(2))
            },
        ];
        let disks = distro_disks(&registered, Some("{11111111-AAAA}"));
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].id, "distro:11111111-aaaa");
        assert_eq!(disks[0].kind, DiskKind::Distro);
        assert!(disks[0].is_default);
        assert!(disks[0].path.ends_with("ext4.vhdx"));
        assert_eq!(disks[1].kind, DiskKind::DockerData);
        assert!(disks[1].path.ends_with("custom.vhdx"));
        assert!(!disks[1].is_default);
    }

    #[test]
    fn docker_disks_are_listed_once() {
        let local = Path::new("C:\\Users\\a\\AppData\\Local");
        let custom =
            docker_custom_dir(r#"{"AutoStart":false,"CustomWslDistroDir":"D:\\DockerWSL"}"#);
        assert_eq!(custom, Some(PathBuf::from("D:\\DockerWSL")));
        assert!(docker_custom_dir(r#"{"customWslDistroDir":""}"#).is_none());
        let docker = docker_disk_candidates(local, custom.as_deref());
        assert_eq!(docker.len(), 6);
        assert!(docker[0].path.starts_with("D:\\DockerWSL"));
        assert_eq!(docker[0].kind, DiskKind::DockerData);
        assert_eq!(docker[2].kind, DiskKind::DockerSystem);

        // docker-desktop-data registered at the same file (case differs).
        let registered = FoundDisk {
            id: "distro:x".into(),
            kind: DiskKind::DockerData,
            name: "docker-desktop-data".into(),
            distro: Some("docker-desktop-data".into()),
            path: PathBuf::from(
                "\\\\?\\c:\\users\\a\\appdata\\local\\docker\\wsl\\data\\ext4.vhdx",
            ),
            is_default: false,
        };
        let docker = docker_disk_candidates(local, None);
        let all = merge(vec![registered], docker);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "distro:x");
    }

    #[test]
    fn reads_wsl_output_in_utf16_and_utf8() {
        let utf16: Vec<u8> = "Ubuntu\r\ndocker-desktop\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            parse_running(&decode_output(&utf16)),
            vec!["Ubuntu", "docker-desktop"]
        );
        assert_eq!(parse_running(&decode_output(b"Debian\n")), vec!["Debian"]);
        assert!(parse_running(&decode_output(b"")).is_empty());
    }

    #[test]
    fn reads_used_space_from_df() {
        let df = "Filesystem     1024-blocks     Used Available Capacity Mounted on\n/dev/sdc        1055762868 12345678 989733708       2% /\n";
        assert_eq!(parse_df_used(df), Some(12_345_678 * 1024));
        assert_eq!(parse_df_used("garbage"), None);
        assert_eq!(reclaimable(100, Some(30)), Some(70));
        assert_eq!(reclaimable(10, Some(30)), Some(0));
        assert_eq!(reclaimable(10, None), None);
    }

    #[test]
    fn distro_names_never_read_as_options() {
        assert!(valid_distro_name("Ubuntu-24.04"));
        assert!(valid_distro_name("my_distro.2"));
        assert!(!valid_distro_name("--shutdown"));
        assert!(!valid_distro_name("a b"));
        assert!(!valid_distro_name("a;b"));
        assert!(!valid_distro_name(""));
    }

    #[test]
    fn paths_compare_without_prefix_or_case() {
        assert_eq!(
            path_key(Path::new("\\\\?\\C:\\Users\\A\\x.vhdx")),
            path_key(Path::new("c:/users/a/x.vhdx"))
        );
    }

    #[test]
    fn macos_reports_unsupported() {
        if !cfg!(windows) {
            let o = overview(Path::new("/nonexistent"), None);
            assert!(!o.supported);
            assert!(o.disks.is_empty());
        }
    }
}
