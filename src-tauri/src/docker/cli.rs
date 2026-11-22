//! Locating and running the Docker CLI.
//!
//! The CLI is executed directly with an argument vector (never through a
//! shell), with a timeout, and with a minimal PATH. GUI apps on macOS do not
//! inherit the user's shell PATH, so well-known install locations are probed.
//! On Windows, Docker Desktop's own copy is tried first, then `docker.exe` on
//! PATH, and the CLI runs without a console window.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("Docker is not installed (no signed docker CLI found)")]
    NotInstalled,
    #[error("Docker is installed but not running. Start Docker Desktop (or your Docker engine) and try again.")]
    NotRunning,
    #[error("Docker did not respond within {0} seconds")]
    Timeout(u64),
    #[error("docker {command} failed: {message}")]
    Failed { command: String, message: String },
    #[error("Could not read Docker's output: {0}")]
    Parse(String),
    #[error("Invalid Docker identifier: {0}")]
    InvalidId(String),
    #[error("{0}")]
    Blocked(String),
}

#[cfg(not(windows))]
/// Code requirement for a binary signed with an Apple Developer ID
/// certificate (Docker Desktop, OrbStack, Rancher Desktop and so on).
const DEVELOPER_ID: &str = "anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists";

#[cfg(not(windows))]
/// Homebrew's package folders. Homebrew's docker is not Developer ID signed.
const HOMEBREW_CELLARS: &[&str] = &["/opt/homebrew/Cellar/", "/usr/local/Cellar/"];

/// Whether DiskPilot may run this docker binary. Child processes inherit
/// DiskPilot's Full Disk Access, so a `docker` planted in a user-writable
/// folder such as `~/.docker/bin` must never run. The binary must carry a
/// valid Developer ID signature, or be a Homebrew-installed package.
#[cfg(not(windows))]
fn is_trusted(real_path: &Path) -> bool {
    let text = real_path.to_string_lossy();
    if HOMEBREW_CELLARS.iter().any(|c| text.starts_with(c)) {
        return true;
    }
    cached_verdict(real_path, |p| {
        Command::new("/usr/bin/codesign")
            .arg("--verify")
            .arg("--strict")
            .arg(format!("-R={DEVELOPER_ID}"))
            .arg(p)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// Verifying a signature takes a moment; remember the answer per file
/// version.
fn cached_verdict(real_path: &Path, check: impl FnOnce(&Path) -> bool) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::SystemTime;

    type Verdicts = HashMap<PathBuf, (Option<SystemTime>, bool)>;
    static CACHE: OnceLock<Mutex<Verdicts>> = OnceLock::new();
    let modified = real_path.metadata().and_then(|m| m.modified()).ok();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((m, ok)) = cache.lock().ok().and_then(|c| c.get(real_path).copied()) {
        if m == modified {
            return ok;
        }
    }
    let ok = check(real_path);
    if let Ok(mut c) = cache.lock() {
        c.insert(real_path.to_path_buf(), (modified, ok));
    }
    ok
}

/// Whether DiskPilot may run this docker.exe: it is in a folder only
/// administrators can change (Program Files, Windows), or it carries a valid
/// Authenticode signature. A `docker.exe` planted in a folder the user can
/// write to, such as one early on PATH, is refused.
#[cfg(windows)]
fn is_trusted(real_path: &Path) -> bool {
    in_protected_dir(real_path, &protected_roots())
        || cached_verdict(real_path, authenticode_signed)
}

/// Folders only administrators can write to.
#[cfg(windows)]
fn protected_roots() -> Vec<PathBuf> {
    [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "SystemRoot",
    ]
    .iter()
    .filter_map(std::env::var_os)
    .filter(|v| !v.is_empty())
    .map(PathBuf::from)
    .collect()
}

/// Comparable Windows path: no `\\?\` prefix, one separator, lowercase.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = text.trim_start_matches("\\\\?\\").to_lowercase();
    text.trim_end_matches('\\').to_string()
}

/// Whether `path` is inside one of `roots`.
pub fn in_protected_dir(path: &Path, roots: &[PathBuf]) -> bool {
    let p = windows_key(path);
    roots.iter().any(|r| {
        let r = windows_key(r);
        !r.is_empty() && p.len() > r.len() && p.starts_with(&r) && p[r.len()..].starts_with('\\')
    })
}

/// A valid Authenticode signature from a publisher Windows trusts.
/// Revocation is not checked online, so this never waits on the network.
#[cfg(windows)]
fn authenticode_signed(path: &Path) -> bool {
    use windows_sys::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
        WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE,
        WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };
    let wide = crate::platform::wide(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: wide.as_ptr(),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and `file` are initialized with their sizes and point
    // at `wide`, which outlives both calls; the second call releases the
    // state the first one allocated.
    unsafe {
        let status = WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
        status == 0
    }
}

/// Most output DiskPilot reads from one docker command.
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DockerCli {
    path: PathBuf,
}

/// Candidate CLI locations, in priority order.
pub fn candidate_paths(home: &Path) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let (_, local) = crate::devtools::windows_app_data(home);
        windows_candidates(
            std::env::var_os("ProgramFiles")
                .map(PathBuf::from)
                .as_deref(),
            &local,
            std::env::var_os("PATH").as_deref(),
        )
    }
    #[cfg(not(windows))]
    vec![
        PathBuf::from("/usr/local/bin/docker"),
        PathBuf::from("/opt/homebrew/bin/docker"),
        home.join(".docker/bin/docker"),
        PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker"),
        home.join(".orbstack/bin/docker"),
        home.join(".colima/bin/docker"),
    ]
}

/// Docker Desktop's own CLI, then `docker.exe` in each absolute PATH
/// folder, then Rancher Desktop's. Only absolute paths, each once.
pub fn windows_candidates(
    program_files: Option<&Path>,
    local_app_data: &Path,
    path_var: Option<&std::ffi::OsStr>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(pf) = program_files {
        out.push(pf.join("Docker\\Docker\\resources\\bin\\docker.exe"));
    }
    if let Some(var) = path_var {
        for dir in var.to_string_lossy().split(';') {
            let dir = dir.trim().trim_matches('"');
            if !dir.is_empty() {
                out.push(Path::new(dir).join("docker.exe"));
            }
        }
    }
    out.push(
        local_app_data
            .join("Programs\\Rancher Desktop\\resources\\resources\\win32\\bin\\docker.exe"),
    );
    let mut seen = std::collections::HashSet::new();
    out.into_iter()
        .filter(|p| is_windows_absolute(p))
        .filter(|p| seen.insert(windows_key(p)))
        .collect()
}

/// `C:\...` or `\\server\...`, judged by text so it can be tested on any
/// platform.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_windows_absolute(path: &Path) -> bool {
    let t = path.to_string_lossy();
    let b = t.as_bytes();
    (b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/'))
        || t.starts_with("\\\\")
}

impl DockerCli {
    /// The first candidate that exists and is trusted (see [`is_trusted`]).
    /// Symlinks are resolved first, so the binary that runs is the one checked.
    pub fn locate(home: &Path) -> Option<Self> {
        candidate_paths(home)
            .into_iter()
            .filter_map(|p| crate::platform::canonicalize(&p).ok())
            .filter(|p| p.metadata().is_ok_and(|m| m.is_file()))
            .find(|p| is_trusted(p))
            .map(|path| Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run `docker <args>` and return stdout. Errors carry stderr.
    pub fn run(&self, args: &[&str], timeout: Duration) -> Result<String, DockerError> {
        let mut command = Command::new(&self.path);
        command
            .args(args)
            .env("PATH", minimal_path(self.path.parent()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::devtools::hide_console(&mut command);
        let mut child = command.spawn().map_err(|e| DockerError::Failed {
            command: args.join(" "),
            message: e.to_string(),
        })?;

        // Drain pipes on threads so a large output can't deadlock the child.
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let out_t = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stdout.by_ref().take(MAX_OUTPUT).read_to_string(&mut s);
            // Keep draining so the child never blocks on a full pipe.
            let _ = std::io::copy(&mut stdout, &mut std::io::sink());
            s
        });
        let err_t = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.by_ref().take(MAX_OUTPUT).read_to_string(&mut s);
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
            s
        });

        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DockerError::Timeout(timeout.as_secs()));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(e) => {
                    return Err(DockerError::Failed {
                        command: args.join(" "),
                        message: e.to_string(),
                    })
                }
            }
        };
        let out = out_t.join().unwrap_or_default();
        let err = err_t.join().unwrap_or_default();
        if status.success() {
            Ok(out)
        } else if is_daemon_down(&err) {
            Err(DockerError::NotRunning)
        } else {
            Err(DockerError::Failed {
                command: args.first().copied().unwrap_or("").to_string(),
                message: err
                    .trim()
                    .lines()
                    .last()
                    .unwrap_or("unknown error")
                    .to_string(),
            })
        }
    }
}

/// The CLI's own folder (for its credential helpers), then system folders.
fn minimal_path(cli_dir: Option<&Path>) -> String {
    let mut path_env = String::new();
    if let Some(dir) = cli_dir {
        path_env.push_str(&dir.to_string_lossy());
        path_env.push(if cfg!(windows) { ';' } else { ':' });
    }
    if cfg!(windows) {
        let root = crate::wsl::system_root();
        path_env.push_str(&format!(
            "{};{}",
            root.join("System32").display(),
            root.display()
        ));
    } else {
        path_env.push_str("/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin");
    }
    path_env
}

pub fn is_daemon_down(stderr: &str) -> bool {
    stderr.contains("Cannot connect to the Docker daemon")
        || stderr.contains("Is the docker daemon running")
        || stderr.contains("error during connect")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_daemon_down_messages() {
        assert!(is_daemon_down(
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"
        ));
        assert!(!is_daemon_down("Error: No such image: foo"));
    }

    #[cfg(not(windows))]
    #[test]
    fn only_signed_or_homebrew_binaries_are_trusted() {
        // A plain file, as malware would plant in ~/.docker/bin, is refused.
        let dir = std::env::temp_dir().join(format!("dp-docker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("docker");
        std::fs::write(&fake, "#!/bin/sh\n").unwrap();
        assert!(!is_trusted(&fake));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(is_trusted(Path::new(
            "/opt/homebrew/Cellar/docker/27.0/bin/docker"
        )));
    }

    #[test]
    fn candidates_are_absolute() {
        assert!(candidate_paths(Path::new("/Users/test"))
            .iter()
            .all(|p| p.is_absolute()));
    }

    #[test]
    fn windows_candidates_put_docker_desktop_first() {
        let path = std::ffi::OsString::from(
            "C:\\Windows\\System32;relative\\dir;\"C:\\Program Files\\Docker\\Docker\\resources\\bin\";C:\\Users\\a\\scoop\\shims",
        );
        let c = windows_candidates(
            Some(Path::new("C:\\Program Files")),
            Path::new("C:\\Users\\a\\AppData\\Local"),
            Some(&path),
        );
        let text: Vec<String> = c
            .iter()
            .map(|p| p.to_string_lossy().replace('/', "\\"))
            .collect();
        assert_eq!(
            text,
            vec![
                "C:\\Program Files\\Docker\\Docker\\resources\\bin\\docker.exe",
                "C:\\Windows\\System32\\docker.exe",
                "C:\\Users\\a\\scoop\\shims\\docker.exe",
                "C:\\Users\\a\\AppData\\Local\\Programs\\Rancher Desktop\\resources\\resources\\win32\\bin\\docker.exe",
            ]
        );
    }

    #[test]
    fn only_admin_folders_count_as_protected() {
        let roots = vec![
            PathBuf::from("C:\\Program Files"),
            PathBuf::from("C:\\Windows"),
        ];
        assert!(in_protected_dir(
            Path::new("\\\\?\\C:\\Program Files\\Docker\\Docker\\resources\\bin\\docker.exe"),
            &roots
        ));
        assert!(in_protected_dir(
            Path::new("c:/windows/system32/x.exe"),
            &roots
        ));
        assert!(!in_protected_dir(
            Path::new("C:\\Program Files Evil\\docker.exe"),
            &roots
        ));
        assert!(!in_protected_dir(
            Path::new("C:\\Users\\a\\docker.exe"),
            &roots
        ));
        assert!(!in_protected_dir(Path::new("C:\\Program Files"), &roots));
    }

    #[cfg(windows)]
    #[test]
    fn an_unsigned_docker_outside_program_files_is_refused() {
        let dir = crate::test_support::sandbox();
        let fake = dir.path().join("docker.exe");
        std::fs::write(&fake, b"MZ not really").unwrap();
        assert!(!is_trusted(&fake));
    }

    #[cfg(windows)]
    #[test]
    fn a_hung_command_times_out_on_windows() {
        let ping = crate::wsl::system32(&crate::wsl::system_root(), "PING.EXE");
        let cli = DockerCli { path: ping };
        assert!(matches!(
            cli.run(&["-n", "10", "127.0.0.1"], Duration::from_millis(300)),
            Err(DockerError::Timeout(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn runs_programs_without_a_shell_and_times_out() {
        // `/bin/sleep` stands in for a hung docker command.
        let cli = DockerCli {
            path: PathBuf::from("/bin/sleep"),
        };
        assert!(matches!(
            cli.run(&["5"], Duration::from_millis(200)),
            Err(DockerError::Timeout(_))
        ));
        // Shell metacharacters are passed literally, not interpreted.
        let echo = DockerCli {
            path: PathBuf::from("/bin/echo"),
        };
        assert_eq!(
            echo.run(&["a; rm -rf /tmp/x"], Duration::from_secs(5))
                .unwrap(),
            "a; rm -rf /tmp/x\n"
        );
    }
}
