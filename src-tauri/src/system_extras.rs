//! Windows system areas that take a lot of space but belong to Windows:
//! the hibernation and page files, the previous installation, update caches,
//! the component store and crash dumps.
//!
//! DiskPilot only reports them. Each item says what it is and names the
//! Windows tool or exact command that reclaims the space; DiskPilot never
//! removes any of it. Sizes come from file metadata only: nothing is opened
//! or read, links and junctions are not followed, and folders Windows does
//! not let a standard user read are reported as partly measured instead of
//! asking for administrator rights.
//!
//! On macOS there are no such items and the list is empty.

// Only Windows builds the list; tests exercise it everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

use crate::classifier::rules::Root;

/// How completely an item could be measured without administrator rights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Measured {
    /// Every file was counted.
    Complete,
    /// Some folders could not be read; the size is a lower bound.
    Partial,
    /// Nothing could be read; the size is unknown.
    Unavailable,
}

/// One system area, as shown on the System extras card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemExtra {
    pub id: &'static str,
    pub label: &'static str,
    /// The files and folders that exist, as full paths.
    pub paths: Vec<String>,
    /// Bytes on disk. A lower bound when `measured` is `partial`; 0 when
    /// `unavailable`.
    pub bytes: u64,
    pub measured: Measured,
    pub explanation: &'static str,
    /// Why the size may mislead, when it may.
    pub size_note: Option<&'static str>,
    /// What to do, in Windows' own words.
    pub how_to_reclaim: &'static str,
    /// The exact command that reclaims the space, for the person to run.
    pub command: Option<String>,
    /// A command that only reports, run before `command` (DISM's analysis).
    pub check_command: Option<&'static str>,
    /// A Settings page to open (`ms-settings:` URI).
    pub settings_uri: Option<&'static str>,
    /// Whether `command` needs an administrator prompt.
    pub needs_admin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
}

struct ExtraDef {
    id: &'static str,
    label: &'static str,
    locations: &'static [(Root, &'static [&'static str], Kind)],
    explanation: &'static str,
    size_note: Option<&'static str>,
    how_to_reclaim: &'static str,
    /// `{drive}` is replaced with the system drive, such as `C:`.
    command: Option<&'static str>,
    check_command: Option<&'static str>,
    settings_uri: Option<&'static str>,
    needs_admin: bool,
}

const STORAGE: &str = "ms-settings:storagesense";

static EXTRAS: &[ExtraDef] = &[
    ExtraDef {
        id: "hibernation-file",
        label: "Hibernation file",
        locations: &[(Root::SystemDrive, &["hiberfil.sys"], Kind::File)],
        explanation: "Holds the contents of memory while the PC hibernates. Fast Startup uses it too, to start Windows faster after a shutdown.",
        size_note: None,
        how_to_reclaim: "Turning hibernation off removes the file, and also turns off Fast Startup, so the PC starts a little slower from shut down. `powercfg /h /type reduced` keeps Fast Startup with a smaller file instead. Run either in a terminal opened as administrator; `powercfg /h on` turns it back on.",
        command: Some("powercfg /h off"),
        check_command: None,
        settings_uri: None,
        needs_admin: true,
    },
    ExtraDef {
        id: "virtual-memory",
        label: "Page file",
        locations: &[
            (Root::SystemDrive, &["pagefile.sys"], Kind::File),
            (Root::SystemDrive, &["swapfile.sys"], Kind::File),
        ],
        explanation: "Windows' virtual memory: when memory is full, Windows moves data here. swapfile.sys does the same for Microsoft Store apps. Windows grows and shrinks them as needed.",
        size_note: None,
        how_to_reclaim: "Leave it managed by Windows. A smaller or missing page file can make programs crash when memory runs out and stops Windows from saving crash dumps. If you must change it: Performance Options › Advanced › Virtual memory › Change.",
        command: Some("SystemPropertiesPerformance"),
        check_command: None,
        settings_uri: None,
        needs_admin: true,
    },
    ExtraDef {
        id: "previous-windows",
        label: "Previous Windows installation",
        locations: &[(Root::SystemDrive, &["Windows.old"], Kind::Dir)],
        explanation: "The Windows installation from before the last upgrade, kept so you can go back to it. Windows removes it by itself after about 10 days.",
        size_note: Some("Only an administrator can read all of it, so the size may be partial."),
        how_to_reclaim: "Settings › System › Storage › Temporary files, then select Previous Windows installation(s). After that you can no longer go back to the previous version.",
        command: None,
        check_command: None,
        settings_uri: Some(STORAGE),
        needs_admin: false,
    },
    ExtraDef {
        id: "upgrade-files",
        label: "Windows upgrade files",
        locations: &[
            (Root::SystemDrive, &["$WINDOWS.~BT"], Kind::Dir),
            (Root::SystemDrive, &["$WINDOWS.~WS"], Kind::Dir),
            (Root::SystemDrive, &["$GetCurrent"], Kind::Dir),
            (Root::SystemDrive, &["$WinREAgent"], Kind::Dir),
        ],
        explanation: "Temporary files and logs from a Windows feature update. They are only needed while an upgrade is in progress or being rolled back.",
        size_note: Some("Only an administrator can read all of it, so the size may be partial."),
        how_to_reclaim: "Settings › System › Storage › Temporary files, then select Temporary Windows installation files and Windows upgrade log files.",
        command: None,
        check_command: None,
        settings_uri: Some(STORAGE),
        needs_admin: false,
    },
    ExtraDef {
        id: "update-cache",
        label: "Windows Update downloads",
        locations: &[(Root::SystemRoot, &["SoftwareDistribution", "Download"], Kind::Dir)],
        explanation: "Update packages Windows Update downloaded. Windows removes them after installing the updates; a large folder usually means an update is pending or keeps failing.",
        size_note: None,
        how_to_reclaim: "Let pending updates finish installing. If the folder stays large, run the Windows Update troubleshooter: Settings › System › Troubleshoot › Other troubleshooters › Windows Update.",
        command: None,
        check_command: None,
        settings_uri: Some("ms-settings:troubleshoot"),
        needs_admin: false,
    },
    ExtraDef {
        id: "delivery-optimization",
        label: "Delivery Optimization files",
        locations: &[
            (
                Root::SystemRoot,
                &["ServiceProfiles", "NetworkService", "AppData", "Local", "Microsoft", "Windows", "DeliveryOptimization"],
                Kind::Dir,
            ),
            (Root::SystemRoot, &["SoftwareDistribution", "DeliveryOptimization"], Kind::Dir),
        ],
        explanation: "Pieces of Windows and Store app updates kept so other PCs can download them from this one.",
        size_note: Some("Windows does not let other programs read this folder, so its size is often unknown. Settings › System › Storage › Temporary files shows it."),
        how_to_reclaim: "Settings › System › Storage › Temporary files, then select Delivery Optimization Files. To cap its size, use Settings › Windows Update › Advanced options › Delivery Optimization.",
        command: Some("Delete-DeliveryOptimizationCache -Force"),
        check_command: None,
        settings_uri: Some("ms-settings:delivery-optimization"),
        needs_admin: true,
    },
    ExtraDef {
        id: "component-store",
        label: "Component store (WinSxS)",
        locations: &[(Root::SystemRoot, &["WinSxS"], Kind::Dir)],
        explanation: "Every Windows component, plus older versions kept so updates can be uninstalled and Windows can repair itself.",
        size_note: Some("Most files here are hard links to files in System32, so this size overstates what WinSxS really uses. DISM's analysis reports the true size and how much cleanup would free."),
        how_to_reclaim: "In a terminal opened as administrator, run the analysis first, then the cleanup. Cleanup removes component versions that newer updates replaced; afterwards those updates cannot be uninstalled. Never delete files in WinSxS directly.",
        command: Some("DISM /Online /Cleanup-Image /StartComponentCleanup"),
        check_command: Some("DISM /Online /Cleanup-Image /AnalyzeComponentStore"),
        settings_uri: None,
        needs_admin: true,
    },
    ExtraDef {
        id: "memory-dumps",
        label: "System crash dumps",
        locations: &[
            (Root::SystemRoot, &["MEMORY.DMP"], Kind::File),
            (Root::SystemRoot, &["Minidump"], Kind::Dir),
            (Root::SystemRoot, &["LiveKernelReports"], Kind::Dir),
        ],
        explanation: "Memory dumps Windows wrote when the system or a driver crashed. They help diagnose crashes; Windows does not need them to run.",
        size_note: None,
        how_to_reclaim: "Open Disk Cleanup, choose Clean up system files, then select System error memory dump files and System error minidump files.",
        command: Some("cleanmgr /d {drive}"),
        check_command: None,
        settings_uri: None,
        needs_admin: true,
    },
    ExtraDef {
        id: "error-reports",
        label: "Windows error reports",
        locations: &[(Root::ProgramData, &["Microsoft", "Windows", "WER"], Kind::Dir)],
        explanation: "Problem reports Windows Error Reporting collected for programs and services, waiting to be sent or already sent to Microsoft.",
        size_note: None,
        how_to_reclaim: "Open Disk Cleanup, choose Clean up system files, then select System created Windows Error Reporting files.",
        command: Some("cleanmgr /d {drive}"),
        check_command: None,
        settings_uri: None,
        needs_admin: true,
    },
];

/// The system areas that exist on this PC, with their sizes. Empty on
/// macOS. Slow folders (WinSxS, Windows.old) are measured in parallel.
pub fn system_extras() -> Vec<SystemExtra> {
    #[cfg(windows)]
    {
        let home = dirs::home_dir().unwrap_or_default();
        let roots = crate::classifier::WindowsRoots::from_env(&home);
        let drive = roots.parts(Root::SystemDrive).join("\\");
        collect(
            |root, parts| {
                let mut path = roots.parts(root).join("\\");
                if path.len() == 2 {
                    path.push('\\'); // `C:` alone means the drive's current folder.
                }
                let mut p = PathBuf::from(path);
                p.extend(parts);
                p
            },
            &drive,
        )
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Build the list, locating each root with `locate` (so tests can point the
/// roots at a sandbox).
fn collect(locate: impl Fn(Root, &[&str]) -> PathBuf + Sync, drive: &str) -> Vec<SystemExtra> {
    EXTRAS
        .par_iter()
        .filter_map(|def| {
            let mut paths = Vec::new();
            let mut bytes = 0u64;
            let mut readable = 0usize;
            let mut complete = true;
            for &(root, parts, kind) in def.locations {
                let path = locate(root, parts);
                let Ok(meta) = std::fs::symlink_metadata(&path) else {
                    // Missing, or hidden from this user: Windows answers
                    // "access denied" for some system folders. Report
                    // those as present but unmeasured.
                    if exists_but_unreadable(&path) {
                        paths.push(path.display().to_string());
                        complete = false;
                    }
                    continue;
                };
                if meta.file_type().is_symlink() || (kind == Kind::Dir) != meta.is_dir() {
                    continue;
                }
                paths.push(path.display().to_string());
                let (b, all) = measure(&path, &meta);
                bytes += b;
                readable += 1;
                complete &= all;
            }
            if paths.is_empty() {
                return None;
            }
            let measured = match (readable, complete) {
                (0, _) => Measured::Unavailable,
                (_, true) => Measured::Complete,
                (_, false) => Measured::Partial,
            };
            Some(SystemExtra {
                id: def.id,
                label: def.label,
                paths,
                bytes,
                measured,
                explanation: def.explanation,
                size_note: def.size_note,
                how_to_reclaim: def.how_to_reclaim,
                command: def.command.map(|c| c.replace("{drive}", drive)),
                check_command: def.check_command,
                settings_uri: def.settings_uri,
                needs_admin: def.needs_admin,
            })
        })
        .collect()
}

fn exists_but_unreadable(path: &Path) -> bool {
    matches!(
        std::fs::symlink_metadata(path),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied
    )
}

/// Bytes on disk of a file or a folder tree, from metadata only, and
/// whether every folder could be read. Links and junctions are not
/// followed; files that are online only (cloud placeholders) count as 0.
fn measure(path: &Path, meta: &std::fs::Metadata) -> (u64, bool) {
    if !meta.is_dir() {
        // The hibernation and page files are not compressed or sparse, so
        // their length is what they use.
        return (meta.len(), true);
    }
    let mut total = 0u64;
    let mut complete = true;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            complete = false;
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            let Ok(meta) = entry.metadata() else {
                complete = false;
                continue;
            };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else {
                total += crate::platform::allocated_bytes(&meta);
            }
        }
    }
    (total, complete)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox_locate(base: &Path) -> impl Fn(Root, &[&str]) -> PathBuf + Sync + '_ {
        move |root, parts| {
            let mut p = base.join(format!("{root:?}"));
            p.extend(parts);
            p
        }
    }

    #[test]
    fn reports_only_areas_that_exist_with_their_commands() {
        let dir = crate::test_support::sandbox();
        let base = dir.path();
        let locate = sandbox_locate(base);
        std::fs::create_dir_all(locate(Root::SystemDrive, &[])).unwrap();
        std::fs::write(
            locate(Root::SystemDrive, &["hiberfil.sys"]),
            vec![0u8; 4096],
        )
        .unwrap();
        let winsxs = locate(Root::SystemRoot, &["WinSxS", "amd64_x"]);
        std::fs::create_dir_all(&winsxs).unwrap();
        std::fs::write(winsxs.join("a.dll"), vec![0u8; 10_000]).unwrap();
        let minidump = locate(Root::SystemRoot, &["Minidump"]);
        std::fs::create_dir_all(&minidump).unwrap();
        std::fs::write(minidump.join("1.dmp"), vec![0u8; 100]).unwrap();

        let extras = collect(&locate, "C:");
        let ids: Vec<&str> = extras.iter().map(|e| e.id).collect();
        assert_eq!(ids, ["hibernation-file", "component-store", "memory-dumps"]);

        let hiber = &extras[0];
        assert_eq!(hiber.bytes, 4096);
        assert_eq!(hiber.measured, Measured::Complete);
        assert_eq!(hiber.command.as_deref(), Some("powercfg /h off"));
        assert!(hiber.needs_admin);
        assert!(hiber.how_to_reclaim.contains("Fast Startup"));

        let sxs = &extras[1];
        assert!(sxs.bytes >= 10_000);
        assert!(sxs.size_note.unwrap().contains("hard links"));
        assert_eq!(
            sxs.check_command,
            Some("DISM /Online /Cleanup-Image /AnalyzeComponentStore")
        );
        assert_eq!(
            sxs.command.as_deref(),
            Some("DISM /Online /Cleanup-Image /StartComponentCleanup")
        );

        let dumps = &extras[2];
        assert_eq!(dumps.command.as_deref(), Some("cleanmgr /d C:"));
        assert_eq!(dumps.paths.len(), 1);
    }

    #[test]
    fn a_file_where_a_folder_is_expected_is_ignored() {
        let dir = crate::test_support::sandbox();
        let locate = sandbox_locate(dir.path());
        std::fs::create_dir_all(locate(Root::SystemDrive, &[])).unwrap();
        // `Windows.old` as a file is not a previous installation.
        std::fs::write(locate(Root::SystemDrive, &["Windows.old"]), b"x").unwrap();
        assert!(collect(&locate, "C:").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn links_inside_an_area_are_not_followed() {
        let dir = crate::test_support::sandbox();
        let locate = sandbox_locate(dir.path());
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("big"), vec![0u8; 1_000_000]).unwrap();
        let dumps = locate(Root::SystemRoot, &["Minidump"]);
        std::fs::create_dir_all(&dumps).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dumps.join("link")).unwrap();
        let extras = collect(&locate, "C:");
        assert_eq!(extras[0].id, "memory-dumps");
        assert!(extras[0].bytes < 1_000_000);
    }

    #[test]
    fn every_area_says_how_to_reclaim_it() {
        for def in EXTRAS {
            assert!(!def.explanation.is_empty() && !def.how_to_reclaim.is_empty());
            assert!(
                def.command.is_some() || def.settings_uri.is_some(),
                "{} names no tool",
                def.id
            );
            if let Some(uri) = def.settings_uri {
                assert!(uri.starts_with("ms-settings:"), "{}", def.id);
            }
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn nothing_is_reported_on_macos() {
        assert!(system_extras().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn reads_this_pc_without_administrator_rights() {
        // CI runners have a page file and a WinSxS folder. Measuring must
        // work as a standard user and never fail.
        let extras = system_extras();
        let sxs = extras.iter().find(|e| e.id == "component-store");
        assert!(sxs.is_some_and(|e| e.bytes > 0), "{extras:?}");
    }
}
