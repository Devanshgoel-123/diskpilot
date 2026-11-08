//! Returning a virtual disk's free space to Windows.
//!
//! Two ways, both previewed first and confirmed by typing the disk's name:
//!
//! - **Compact**: stop WSL (`wsl --shutdown`, which also stops Docker
//!   Desktop), then shrink the file as administrator with `Optimize-VHD`
//!   when the Hyper-V module is installed, or with `diskpart` otherwise.
//! - **Make sparse**: `wsl --manage <name> --set-sparse true`, after which
//!   the disk shrinks by itself as Linux frees space.
//!
//! Administrator steps run through a UAC prompt (`ShellExecuteExW` with
//! "runas") with a script written to a new private folder in `%TEMP%`, and
//! the exact script text is part of the preview. Nothing runs without a
//! confirmed, unexpired preview whose commands still match.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{file_size, system32, valid_distro_name, DiskKind, VirtualDisk};
use crate::model::{ActionStatus, CleanupItemResult, CleanupRecord, ItemStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompactMethod {
    /// Shrink the file now.
    Compact,
    /// Make the disk sparse so it shrinks by itself from now on.
    Sparse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompactTool {
    /// `Optimize-VHD` from the Hyper-V PowerShell module.
    OptimizeVhd,
    /// `diskpart`'s `compact vdisk`, on every edition of Windows.
    Diskpart,
    /// `wsl --manage --set-sparse`.
    WslManage,
}

/// Stands for the script file's path in a command until it is written.
pub const SCRIPT: &str = "<script>";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactStep {
    /// What the step does, for the preview and history.
    pub label: String,
    /// Program and arguments exactly as run. An administrator step names its
    /// script file as `<script>`; the file is written when the step runs.
    pub command: Vec<String>,
    /// Runs as administrator, after a UAC prompt.
    pub elevated: bool,
    /// The script's full text, for an administrator step.
    pub script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactPreview {
    /// Set by the command that stores the preview; execution names it.
    #[serde(default)]
    pub id: String,
    pub disk: VirtualDisk,
    pub method: CompactMethod,
    pub tool: Option<CompactTool>,
    pub title: String,
    pub steps: Vec<CompactStep>,
    /// What happens.
    pub effects: Vec<String>,
    /// What to know or do first.
    pub warnings: Vec<String>,
    /// Set when this cannot run; the reason is shown instead.
    pub blocked: Option<String>,
    /// The name the person types to confirm. Checked by the engine.
    pub confirm_name: String,
    /// Space on the drive before.
    pub before_bytes: u64,
    /// Roughly what returns to Windows, when the space used inside is known.
    pub estimated_reclaim_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactResult {
    pub preview: CompactPreview,
    pub succeeded: bool,
    /// Space on the drive before and after.
    pub before_bytes: u64,
    pub after_bytes: u64,
    pub freed_bytes: u64,
    pub output: String,
    pub error: Option<String>,
    pub record: CleanupRecord,
}

/// What the person reviewed and confirmed.
#[derive(Debug, Clone)]
pub struct Reviewed {
    pub disk_id: String,
    pub path: String,
    pub method: CompactMethod,
    pub steps: Vec<CompactStep>,
    pub confirm_name: String,
}

impl Reviewed {
    pub fn of(p: &CompactPreview) -> Self {
        Reviewed {
            disk_id: p.disk.id.clone(),
            path: p.disk.path.clone(),
            method: p.method,
            steps: p.steps.clone(),
            confirm_name: p.confirm_name.clone(),
        }
    }
}

/// The name to type: the distribution's, or "docker" for Docker's disks.
pub fn confirm_name(disk: &VirtualDisk) -> String {
    match disk.kind {
        DiskKind::Distro => disk.distro.clone().unwrap_or_else(|| disk.name.clone()),
        DiskKind::DockerData | DiskKind::DockerSystem => "docker".into(),
    }
}

/// The disk's path as scripts must see it: a plain drive path ending in
/// `.vhdx`, without the `\\?\` prefix, and nothing a script could read as
/// more than a path.
pub fn script_path(path: &str) -> Result<String, String> {
    let plain = path.trim_start_matches("\\\\?\\");
    let b = plain.as_bytes();
    let drive = b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\';
    if !drive {
        return Err("The disk is not on a local drive, so DiskPilot does not compact it.".into());
    }
    if !plain.to_ascii_lowercase().ends_with(".vhdx") {
        return Err("Only .vhdx disks can be compacted.".into());
    }
    if plain
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '*' | '?'))
        || plain[2..].contains(':')
    {
        return Err("The disk's path has characters DiskPilot does not pass to a script.".into());
    }
    Ok(plain.to_string())
}

/// `Optimize-VHD` in a PowerShell script. The path is a single-quoted
/// literal, so nothing in it is expanded.
pub fn powershell_script(path: &str) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'\r\nOptimize-VHD -Path '{}' -Mode Full\r\n",
        path.replace('\'', "''")
    )
}

/// `compact vdisk` in a diskpart script. The disk is attached read-only.
pub fn diskpart_script(path: &str) -> String {
    format!(
        "select vdisk file=\"{path}\"\r\nattach vdisk readonly\r\ncompact vdisk\r\ndetach vdisk\r\n"
    )
}

fn arg(s: impl Into<String>) -> String {
    s.into()
}

/// Build what compacting or making `disk` sparse would run. Pure: the
/// caller says whether the Hyper-V module is installed and whether Docker
/// is running.
pub fn build_preview(
    disk: &VirtualDisk,
    method: CompactMethod,
    hyperv: bool,
    docker_running: bool,
    system_root: &Path,
) -> CompactPreview {
    let wsl = system32(system_root, "wsl.exe").display().to_string();
    let confirm = confirm_name(disk);
    let mut p = CompactPreview {
        id: String::new(),
        disk: disk.clone(),
        method,
        tool: None,
        title: String::new(),
        steps: Vec::new(),
        effects: Vec::new(),
        warnings: Vec::new(),
        blocked: None,
        confirm_name: confirm,
        before_bytes: disk.allocated_bytes,
        estimated_reclaim_bytes: disk.reclaimable_bytes,
    };
    match method {
        CompactMethod::Compact => {
            p.title = format!("Compact {}", disk.name);
            let path = match script_path(&disk.path) {
                Ok(path) => path,
                Err(reason) => {
                    p.blocked = Some(reason);
                    return p;
                }
            };
            p.steps.push(CompactStep {
                label: "Stop WSL and everything running in it".into(),
                command: vec![wsl, arg("--shutdown")],
                elevated: false,
                script: None,
            });
            // Optimize-VHD's -Path accepts wildcards; brackets in a path
            // would be read as one, so diskpart handles those.
            if hyperv && !path.contains(['[', ']']) {
                p.tool = Some(CompactTool::OptimizeVhd);
                p.steps.push(CompactStep {
                    label: "Compact the disk with Optimize-VHD (administrator)".into(),
                    command: vec![
                        system_root
                            .join("System32")
                            .join("WindowsPowerShell")
                            .join("v1.0")
                            .join("powershell.exe")
                            .display()
                            .to_string(),
                        arg("-NoProfile"),
                        arg("-NonInteractive"),
                        arg("-ExecutionPolicy"),
                        arg("Bypass"),
                        arg("-File"),
                        arg(SCRIPT),
                    ],
                    elevated: true,
                    script: Some(powershell_script(&path)),
                });
            } else {
                p.tool = Some(CompactTool::Diskpart);
                p.steps.push(CompactStep {
                    label: "Compact the disk with diskpart (administrator)".into(),
                    command: vec![
                        system32(system_root, "diskpart.exe").display().to_string(),
                        arg("/s"),
                        arg(SCRIPT),
                    ],
                    elevated: true,
                    script: Some(diskpart_script(&path)),
                });
            }
            p.effects.push(
                "The .vhdx file shrinks to about the space Linux uses inside it. Nothing inside the disk is deleted.".into(),
            );
            p.warnings.push(
                "wsl --shutdown stops every WSL distribution and Docker Desktop's engine. Save your work in them first.".into(),
            );
            if docker_running || disk.kind != DiskKind::Distro {
                p.warnings.push(
                    "Quit Docker Desktop before you continue. Otherwise it starts WSL again and the disk stays in use.".into(),
                );
            }
            p.warnings.push(
                "Windows asks for administrator permission to compact the disk. A large disk can take several minutes; do not start WSL until it finishes.".into(),
            );
        }
        CompactMethod::Sparse => {
            p.title = format!("Let {} shrink by itself", disk.name);
            p.tool = Some(CompactTool::WslManage);
            let name = match (&disk.kind, &disk.distro) {
                (DiskKind::Distro, Some(name)) if valid_distro_name(name) => name.clone(),
                (DiskKind::Distro, _) => {
                    p.blocked =
                        Some("This distribution's name cannot be passed to wsl.exe safely.".into());
                    return p;
                }
                _ => {
                    p.blocked = Some(
                        "Docker Desktop manages its own disk. Compact it instead, or use Docker Desktop's settings.".into(),
                    );
                    return p;
                }
            };
            if disk.sparse {
                p.blocked = Some("This disk is already sparse.".into());
                return p;
            }
            p.steps.push(CompactStep {
                label: format!("Stop {name}"),
                command: vec![wsl.clone(), arg("--terminate"), name.clone()],
                elevated: false,
                script: None,
            });
            p.steps.push(CompactStep {
                label: "Make the disk sparse".into(),
                command: vec![
                    wsl,
                    arg("--manage"),
                    name.clone(),
                    arg("--set-sparse"),
                    arg("true"),
                ],
                elevated: false,
                script: None,
            });
            p.effects.push(
                "From now on, space freed inside Linux returns to Windows by itself. Space that is already free returns after Linux trims the disk, or after compacting it once.".into(),
            );
            p.warnings
                .push(format!("{name} stops first. Save your work in it."));
            p.warnings.push(
                "Needs WSL 2.0 or later. Some WSL versions turn sparse disks off because of a data corruption risk; if WSL refuses, DiskPilot does not override it.".into(),
            );
            // The space only returns as Linux frees it, so nothing is
            // promised up front.
            p.estimated_reclaim_bytes = None;
        }
    }
    p
}

/// Whether `fresh` runs exactly what was reviewed, for the same disk, and
/// the person typed its name.
pub fn check_confirmed(
    fresh: &CompactPreview,
    reviewed: &Reviewed,
    typed: &str,
) -> Result<(), String> {
    if let Some(reason) = &fresh.blocked {
        return Err(reason.clone());
    }
    if fresh.disk.id != reviewed.disk_id
        || fresh.disk.path != reviewed.path
        || fresh.method != reviewed.method
        || fresh.steps != reviewed.steps
        || fresh.confirm_name != reviewed.confirm_name
    {
        return Err(
            "The disk or the steps changed since you reviewed them. Review it again.".into(),
        );
    }
    if typed.trim() != reviewed.confirm_name {
        return Err(format!("Type {} to confirm.", reviewed.confirm_name));
    }
    Ok(())
}

/// A command line for display and history.
pub fn command_line(step: &CompactStep) -> String {
    let mut line = step
        .command
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if step.elevated {
        line.push_str(" (as administrator)");
    }
    line
}

/// Arguments joined for `ShellExecuteExW`, which takes one string. Every
/// argument is quoted when it has a space; none may contain a quote.
pub fn parameters(args: &[String]) -> Option<String> {
    if args.iter().any(|a| a.contains('"')) {
        return None;
    }
    Some(
        args.iter()
            .map(|a| {
                if a.contains(' ') || a.is_empty() {
                    format!("\"{a}\"")
                } else {
                    a.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Whether the Hyper-V PowerShell module (with `Optimize-VHD`) is
/// installed. A fixed command; nothing from the person is in it.
pub fn hyperv_available(system_root: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let ps = system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    matches!(
        super::run_captured(
            &ps,
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "if (Get-Command Optimize-VHD -ErrorAction SilentlyContinue) { 'yes' } else { 'no' }",
            ],
            Duration::from_secs(30),
        ),
        Some((true, out)) if out.trim() == "yes"
    )
}

/// The disk as found now, sized but not measured inside.
pub fn fresh_disk(home: &Path, id: &str) -> Option<VirtualDisk> {
    let d = super::find_disks(home).into_iter().find(|d| d.id == id)?;
    let size = file_size(&d.path)?;
    Some(VirtualDisk {
        id: d.id,
        kind: d.kind,
        name: d.name,
        distro: d.distro,
        path: d
            .path
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_string(),
        file_bytes: size.len,
        allocated_bytes: size.allocated,
        sparse: size.sparse,
        running: None,
        is_default: d.is_default,
        used_bytes: None,
        reclaimable_bytes: None,
        usage_note: String::new(),
    })
}

/// A new folder only this user can reach (`%TEMP%` is in the profile),
/// created fresh so nothing can be planted in it beforehand.
fn private_dir() -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("DiskPilot-wsl-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&dir)
        .map_err(|e| format!("Could not create a folder for the script: {e}"))?;
    Ok(dir)
}

/// Write `text` to a new file, then read it back: what runs is what was
/// reviewed.
fn write_script(dir: &Path, name: &str, text: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    let path = dir.join(name);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("Could not write the script: {e}"))?;
    f.write_all(text.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|e| format!("Could not write the script: {e}"))?;
    drop(f);
    let back = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if back != text {
        return Err("The script changed after it was written. Nothing was run.".into());
    }
    Ok(path)
}

/// How long an administrator step may take before DiskPilot stops waiting.
#[cfg(windows)]
const ELEVATED_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// Run a program as administrator through a UAC prompt and wait for it.
#[cfg(windows)]
fn run_elevated(program: &str, args: &[String]) -> Result<u32, String> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_CANCELLED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
        SHELLEXECUTEINFOW,
    };

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let params = parameters(args).ok_or("An argument contains a quote. Nothing was run.")?;
    let verb = wide("runas");
    let file = wide(program);
    let params = wide(&params);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        nShow: 0, // SW_HIDE
        ..Default::default()
    };
    // SAFETY: every string is NUL-terminated and outlives the call; `info`
    // is fully initialized with its size set.
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        // SAFETY: reads the calling thread's last error only.
        let err = unsafe { GetLastError() };
        return Err(if err == ERROR_CANCELLED {
            "You declined the administrator prompt, so the disk was not compacted.".into()
        } else {
            format!("Windows could not start {program} as administrator (error {err}).")
        });
    }
    if info.hProcess.is_null() {
        return Err("Windows did not report the administrator step's result.".into());
    }
    let ms = ELEVATED_TIMEOUT.as_millis().min(u128::from(u32::MAX - 1)) as u32;
    // SAFETY: `hProcess` is a valid process handle owned by this function.
    let waited = unsafe { WaitForSingleObject(info.hProcess, ms) };
    let mut code = 1u32;
    // SAFETY: as above; `code` is a valid out pointer.
    let got = unsafe { GetExitCodeProcess(info.hProcess, &mut code) };
    // SAFETY: closes the handle ShellExecuteExW returned, once.
    unsafe { CloseHandle(info.hProcess) };
    if waited == WAIT_TIMEOUT {
        return Err("Compacting is still running after two hours. DiskPilot stopped waiting; check the disk's size again later.".into());
    }
    if waited != WAIT_OBJECT_0 || got == 0 {
        return Err("Could not read the administrator step's result.".into());
    }
    Ok(code)
}

#[cfg(not(windows))]
fn run_elevated(_program: &str, _args: &[String]) -> Result<u32, String> {
    Err("Compacting virtual disks is only available on Windows.".into())
}

/// Run one step. Returns its output, or why it failed.
fn run_step(step: &CompactStep, dir: &mut Option<PathBuf>) -> Result<String, String> {
    let (program, args) = step
        .command
        .split_first()
        .ok_or("The step has no command.")?;
    if !step.elevated {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        return match super::run_captured(Path::new(program), &args, Duration::from_secs(5 * 60)) {
            Some((true, out)) => Ok(out.trim().to_string()),
            Some((false, out)) => Err(match out.trim().lines().last() {
                Some(line) if !line.is_empty() => line.to_string(),
                _ => format!("{} failed.", command_line(step)),
            }),
            None => Err(format!("{} did not finish in time.", command_line(step))),
        };
    }
    let text = step.script.as_deref().ok_or("The step has no script.")?;
    if dir.is_none() {
        *dir = Some(private_dir()?);
    }
    let folder = dir.as_deref().ok_or("No script folder.")?;
    let name = if program.to_ascii_lowercase().ends_with("powershell.exe") {
        "compact.ps1"
    } else {
        "compact.txt"
    };
    let script = write_script(folder, name, text)?;
    let args: Vec<String> = args
        .iter()
        .map(|a| {
            if a == SCRIPT {
                script.display().to_string()
            } else {
                a.clone()
            }
        })
        .collect();
    match run_elevated(program, &args)? {
        0 => Ok(String::new()),
        code => Err(format!(
            "The disk could not be compacted (exit code {code}). Make sure WSL and Docker Desktop are not running, then try again."
        )),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Run what the person reviewed, after checking it against the disk as it
/// is now, and measure the file before and after.
pub fn execute(
    fresh: CompactPreview,
    reviewed: &Reviewed,
    typed: &str,
) -> Result<CompactResult, String> {
    check_confirmed(&fresh, reviewed, typed)?;
    let path = PathBuf::from(&fresh.disk.path);
    let before = file_size(&path)
        .map(|s| s.allocated)
        .unwrap_or(fresh.before_bytes);
    let started = now_ms();
    let mut items = Vec::new();
    let mut outputs = Vec::new();
    let mut error: Option<String> = None;
    let mut dir = None;
    for step in &fresh.steps {
        let line = command_line(step);
        if error.is_some() {
            items.push(CleanupItemResult {
                target: fresh.disk.path.clone(),
                operation: line,
                bytes: 0,
                status: ItemStatus::Skipped,
                error: Some("Not run because an earlier step failed.".into()),
            });
            continue;
        }
        match run_step(step, &mut dir) {
            Ok(out) => {
                if !out.is_empty() {
                    outputs.push(out);
                }
                items.push(CleanupItemResult {
                    target: fresh.disk.path.clone(),
                    operation: line,
                    bytes: 0,
                    status: ItemStatus::Succeeded,
                    error: None,
                });
            }
            Err(message) => {
                items.push(CleanupItemResult {
                    target: fresh.disk.path.clone(),
                    operation: line,
                    bytes: 0,
                    status: ItemStatus::Failed,
                    error: Some(message.clone()),
                });
                error = Some(message);
            }
        }
    }
    if let Some(d) = dir {
        let _ = std::fs::remove_dir_all(d);
    }
    let after = file_size(&path).map(|s| s.allocated).unwrap_or(before);
    let freed = before.saturating_sub(after);
    if let Some(last) = items
        .iter_mut()
        .rev()
        .find(|i| i.status == ItemStatus::Succeeded)
    {
        last.bytes = freed;
    }
    let succeeded = error.is_none();
    let status = match (succeeded, freed > 0) {
        (true, _) => ActionStatus::Succeeded,
        (false, true) => ActionStatus::Partial,
        (false, false) => ActionStatus::Failed,
    };
    let record = CleanupRecord {
        id: format!("wsl-{started}"),
        kind: "wsl".into(),
        title: fresh.title.clone(),
        started_at_ms: started,
        finished_at_ms: now_ms(),
        status,
        bytes_freed: freed,
        bytes_trashed: 0,
        items,
    };
    Ok(CompactResult {
        preview: fresh,
        succeeded,
        before_bytes: before,
        after_bytes: after,
        freed_bytes: freed,
        output: outputs.join("\n"),
        error,
        record,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(kind: DiskKind, name: &str, path: &str) -> VirtualDisk {
        VirtualDisk {
            id: format!("distro:{name}"),
            kind,
            name: name.into(),
            distro: (kind == DiskKind::Distro).then(|| name.to_string()),
            path: path.into(),
            file_bytes: 100,
            allocated_bytes: 100,
            sparse: false,
            running: Some(false),
            is_default: false,
            used_bytes: Some(30),
            reclaimable_bytes: Some(70),
            usage_note: String::new(),
        }
    }

    const ROOT: &str = "C:\\Windows";
    const UBUNTU: &str = "C:\\Users\\a b\\AppData\\Local\\Packages\\Ubuntu\\LocalState\\ext4.vhdx";

    #[test]
    fn compacting_stops_wsl_then_runs_optimize_vhd_as_administrator() {
        let d = disk(DiskKind::Distro, "Ubuntu", UBUNTU);
        let p = build_preview(&d, CompactMethod::Compact, true, false, Path::new(ROOT));
        assert!(p.blocked.is_none());
        assert_eq!(p.tool, Some(CompactTool::OptimizeVhd));
        assert_eq!(p.confirm_name, "Ubuntu");
        assert_eq!(p.steps.len(), 2);
        assert!(p.steps[0].command[0].ends_with("wsl.exe"));
        assert_eq!(p.steps[0].command[1], "--shutdown");
        assert!(!p.steps[0].elevated);
        let ps = &p.steps[1];
        assert!(ps.elevated);
        assert!(ps.command[0].ends_with("powershell.exe"));
        assert_eq!(ps.command.last().unwrap(), SCRIPT);
        assert_eq!(
            ps.script.as_deref().unwrap(),
            format!(
                "$ErrorActionPreference = 'Stop'\r\nOptimize-VHD -Path '{UBUNTU}' -Mode Full\r\n"
            )
        );
        assert!(p.warnings.iter().any(|w| w.contains("wsl --shutdown")));
        assert_eq!(p.estimated_reclaim_bytes, Some(70));
    }

    #[test]
    fn without_hyper_v_diskpart_attaches_read_only() {
        let d = disk(
            DiskKind::DockerData,
            "Docker Desktop data",
            "\\\\?\\C:\\Users\\a\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx",
        );
        let p = build_preview(&d, CompactMethod::Compact, false, true, Path::new(ROOT));
        assert_eq!(p.tool, Some(CompactTool::Diskpart));
        assert_eq!(p.confirm_name, "docker");
        let step = &p.steps[1];
        assert_eq!(step.command[1..], ["/s".to_string(), SCRIPT.to_string()]);
        assert_eq!(
            step.script.as_deref().unwrap(),
            "select vdisk file=\"C:\\Users\\a\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx\"\r\nattach vdisk readonly\r\ncompact vdisk\r\ndetach vdisk\r\n"
        );
        assert!(p.warnings.iter().any(|w| w.contains("Quit Docker Desktop")));
    }

    #[test]
    fn scripts_only_get_plain_vhdx_paths() {
        assert!(script_path("C:\\x\\ext4.VHDX").is_ok());
        assert!(script_path("\\\\?\\D:\\wsl\\ext4.vhdx").is_ok());
        assert!(script_path("\\\\server\\share\\ext4.vhdx").is_err());
        assert!(script_path("C:\\x\\ext4.vhd").is_err());
        assert!(script_path("C:\\x\"\r\ndelete\\ext4.vhdx").is_err());
        assert!(script_path("C:\\x\\a:b.vhdx").is_err());
        // A quote in a PowerShell literal is doubled, not interpreted.
        assert!(powershell_script("C:\\it's\\ext4.vhdx").contains("'C:\\it''s\\ext4.vhdx'"));
        // Brackets would be wildcards to Optimize-VHD; diskpart takes those.
        let d = disk(DiskKind::Distro, "Ubuntu", "C:\\x[1]\\ext4.vhdx");
        let p = build_preview(&d, CompactMethod::Compact, true, false, Path::new(ROOT));
        assert_eq!(p.tool, Some(CompactTool::Diskpart));
        let bad = disk(DiskKind::Distro, "Ubuntu", "\\\\server\\s\\ext4.vhdx");
        let p = build_preview(&bad, CompactMethod::Compact, true, false, Path::new(ROOT));
        assert!(p.blocked.is_some());
        assert!(p.steps.is_empty());
    }

    #[test]
    fn sparse_is_for_distributions_only() {
        let d = disk(DiskKind::Distro, "Debian", UBUNTU);
        let p = build_preview(&d, CompactMethod::Sparse, false, false, Path::new(ROOT));
        assert!(p.blocked.is_none());
        assert_eq!(p.tool, Some(CompactTool::WslManage));
        assert_eq!(p.steps[0].command[1..], ["--terminate", "Debian"]);
        assert_eq!(
            p.steps[1].command[1..],
            ["--manage", "Debian", "--set-sparse", "true"]
        );
        assert!(p.steps.iter().all(|s| !s.elevated));
        assert!(p.estimated_reclaim_bytes.is_none());

        let docker = disk(DiskKind::DockerData, "Docker Desktop data", UBUNTU);
        assert!(build_preview(
            &docker,
            CompactMethod::Sparse,
            false,
            false,
            Path::new(ROOT)
        )
        .blocked
        .is_some());
        let already = VirtualDisk {
            sparse: true,
            ..d.clone()
        };
        assert!(build_preview(
            &already,
            CompactMethod::Sparse,
            false,
            false,
            Path::new(ROOT)
        )
        .blocked
        .is_some());
        let odd = VirtualDisk {
            distro: Some("--shutdown".into()),
            ..d
        };
        assert!(
            build_preview(&odd, CompactMethod::Sparse, false, false, Path::new(ROOT))
                .blocked
                .is_some()
        );
    }

    #[test]
    fn running_needs_the_typed_name_and_the_reviewed_steps() {
        let d = disk(DiskKind::Distro, "Ubuntu", UBUNTU);
        let shown = build_preview(&d, CompactMethod::Compact, true, false, Path::new(ROOT));
        let reviewed = Reviewed::of(&shown);
        let fresh = shown.clone();
        assert!(check_confirmed(&fresh, &reviewed, "Ubuntu").is_ok());
        assert!(check_confirmed(&fresh, &reviewed, " Ubuntu ").is_ok());
        assert!(check_confirmed(&fresh, &reviewed, "").is_err());
        assert!(check_confirmed(&fresh, &reviewed, "ubuntu").is_err());
        // The Hyper-V module went away: the tool changed, so review again.
        let changed = build_preview(&d, CompactMethod::Compact, false, false, Path::new(ROOT));
        assert!(check_confirmed(&changed, &reviewed, "Ubuntu")
            .unwrap_err()
            .contains("Review it again"));
        // A blocked preview never runs.
        let blocked = CompactPreview {
            blocked: Some("no".into()),
            ..shown
        };
        assert!(check_confirmed(&blocked, &reviewed, "Ubuntu").is_err());
    }

    #[test]
    fn execute_refuses_without_confirmation_and_runs_nothing() {
        let d = disk(DiskKind::Distro, "Ubuntu", UBUNTU);
        let shown = build_preview(&d, CompactMethod::Compact, true, false, Path::new(ROOT));
        let reviewed = Reviewed::of(&shown);
        assert!(execute(shown, &reviewed, "wrong").is_err());
    }

    #[test]
    fn parameters_are_quoted_for_shell_execute() {
        let args = vec![
            "/s".to_string(),
            "C:\\Users\\a b\\AppData\\Local\\Temp\\x\\compact.txt".to_string(),
        ];
        assert_eq!(
            parameters(&args).unwrap(),
            "/s \"C:\\Users\\a b\\AppData\\Local\\Temp\\x\\compact.txt\""
        );
        assert!(parameters(&["a\"b".to_string()]).is_none());
        let step = CompactStep {
            label: String::new(),
            command: vec!["wsl.exe".into(), "--shutdown".into()],
            elevated: false,
            script: None,
        };
        assert_eq!(command_line(&step), "wsl.exe --shutdown");
    }

    #[test]
    fn scripts_are_written_to_a_new_folder_and_read_back() {
        let dir = private_dir().unwrap();
        let path =
            write_script(&dir, "compact.txt", "select vdisk file=\"C:\\x.vhdx\"\r\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "select vdisk file=\"C:\\x.vhdx\"\r\n"
        );
        // An existing file is never overwritten.
        assert!(write_script(&dir, "compact.txt", "other").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
