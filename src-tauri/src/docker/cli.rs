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
    #[error("Docker is not installed (no docker CLI found)")]
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
    pub fn locate(home: &Path) -> Option<Self> {
        candidate_paths(home)
            .into_iter()
            .find(|p| p.metadata().is_ok_and(|m| m.is_file()))
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
            let _ = stdout.read_to_string(&mut s);
            s
        });
        let err_t = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
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
