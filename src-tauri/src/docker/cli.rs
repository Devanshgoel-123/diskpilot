//! Locating and running the Docker CLI.
//!
//! The CLI is executed directly with an argument vector (never through a
//! shell), with a timeout, and with a minimal PATH. GUI apps on macOS do not
//! inherit the user's shell PATH, so well-known install locations are probed.

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

/// Code requirement for a binary signed with an Apple Developer ID
/// certificate (Docker Desktop, OrbStack, Rancher Desktop and so on).
const DEVELOPER_ID: &str = "anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists";

/// Homebrew's package folders. Homebrew's docker is not Developer ID signed.
const HOMEBREW_CELLARS: &[&str] = &["/opt/homebrew/Cellar/", "/usr/local/Cellar/"];

/// Whether DiskPilot may run this docker binary. Child processes inherit
/// DiskPilot's Full Disk Access, so a `docker` planted in a user-writable
/// folder such as `~/.docker/bin` must never run. The binary must carry a
/// valid Developer ID signature, or be a Homebrew-installed package.
fn is_trusted(real_path: &Path) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::SystemTime;

    let text = real_path.to_string_lossy();
    if HOMEBREW_CELLARS.iter().any(|c| text.starts_with(c)) {
        return true;
    }
    // Verifying a signature takes a moment; remember the answer per file version.
    type Verdicts = HashMap<PathBuf, (Option<SystemTime>, bool)>;
    static CACHE: OnceLock<Mutex<Verdicts>> = OnceLock::new();
    let modified = real_path.metadata().and_then(|m| m.modified()).ok();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((m, ok)) = cache.lock().ok().and_then(|c| c.get(real_path).copied()) {
        if m == modified {
            return ok;
        }
    }
    let ok = Command::new("/usr/bin/codesign")
        .arg("--verify")
        .arg("--strict")
        .arg(format!("-R={DEVELOPER_ID}"))
        .arg(real_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if let Ok(mut c) = cache.lock() {
        c.insert(real_path.to_path_buf(), (modified, ok));
    }
    ok
}

/// Most output DiskPilot reads from one docker command.
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DockerCli {
    path: PathBuf,
}

/// Candidate CLI locations, in priority order.
pub fn candidate_paths(home: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/local/bin/docker"),
        PathBuf::from("/opt/homebrew/bin/docker"),
        home.join(".docker/bin/docker"),
        PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker"),
        home.join(".orbstack/bin/docker"),
        home.join(".colima/bin/docker"),
    ]
}

impl DockerCli {
    /// The first candidate that exists and is trusted (see [`is_trusted`]).
    /// Symlinks are resolved first, so the binary that runs is the one checked.
    pub fn locate(home: &Path) -> Option<Self> {
        candidate_paths(home)
            .into_iter()
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .filter(|p| p.metadata().is_ok_and(|m| m.is_file()))
            .find(|p| is_trusted(p))
            .map(|path| Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run `docker <args>` and return stdout. Errors carry stderr.
    pub fn run(&self, args: &[&str], timeout: Duration) -> Result<String, DockerError> {
        let mut path_env = String::new();
        if let Some(dir) = self.path.parent() {
            path_env.push_str(&dir.to_string_lossy());
            path_env.push(':');
        }
        path_env.push_str("/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin");

        let mut child = Command::new(&self.path)
            .args(args)
            .env("PATH", path_env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DockerError::Failed {
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
