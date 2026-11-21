//! Programs still running from a folder DiskPilot is about to remove.
//!
//! Moving a project's `node_modules` (or its `.next` build) away while its dev
//! server runs breaks the server, and on Windows the files it holds open make
//! the move stop partway. So such items are refused, with the program to stop,
//! instead of being left to a warning that is easy to read past.
//!
//! A program counts when it was started from inside the item, or when it is a
//! JavaScript runtime (Node, Bun, Deno) working in the item's project: its
//! working folder is there, or its command line names it. Terminals and
//! editors open on the project do not count, so they never block a cleanup.

use std::path::{Path, PathBuf};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Folders that belong to the project around them: a dev server for that
/// project uses them while it runs.
const PROJECT_FOLDERS: &[&str] = &[
    "node_modules",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".parcel-cache",
    ".angular",
    ".vite",
];

/// Runtimes dev servers run on. Matched by executable name, without `.exe`.
const RUNTIMES: &[&str] = &["node", "bun", "deno"];

/// One running program, as much as could be read of it.
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub cmd: String,
}

/// The running programs at one moment.
pub struct Snapshot {
    procs: Vec<Proc>,
}

impl Snapshot {
    /// Read every running program, except DiskPilot itself.
    pub fn take() -> Self {
        let mut sys = System::new();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::Always)
                .with_cwd(UpdateKind::Always)
                .with_cmd(UpdateKind::Always),
        );
        let me = std::process::id();
        let procs = sys
            .processes()
            .values()
            .filter(|p| p.pid().as_u32() != me)
            .map(|p| Proc {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                exe: p.exe().map(Path::to_path_buf),
                cwd: p.cwd().map(Path::to_path_buf),
                cmd: p
                    .cmd()
                    .iter()
                    .map(|a| a.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
            })
            .collect();
        Self { procs }
    }

    #[cfg(test)]
    pub fn of(procs: Vec<Proc>) -> Self {
        Self { procs }
    }

    /// Why `item` must be left alone for now, if a program is using it.
    pub fn blocker(&self, item: &Path) -> Option<String> {
        let project = project_of(item);
        let users: Vec<&Proc> = self
            .procs
            .iter()
            .filter(|p| uses(p, item, project.as_deref()))
            .collect();
        let first = users.first()?;
        let more = match users.len() {
            1 => String::new(),
            2 => " and 1 other program".into(),
            n => format!(" and {} other programs", n - 1),
        };
        let what = format!("{} (process {}){more}", first.name, first.pid);
        Some(match project {
            Some(_) => format!(
                "{what} is running from this project. Stop its dev server first \
                 (Ctrl+C in its terminal), then review the cleanup again."
            ),
            None => format!(
                "{what} is running from this folder. Quit it first, then review the cleanup again."
            ),
        })
    }
}

/// The project folder around a project's dependency or build folder.
fn project_of(item: &Path) -> Option<PathBuf> {
    let name = item.file_name()?.to_string_lossy().to_lowercase();
    if PROJECT_FOLDERS.contains(&name.as_str()) {
        item.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

fn uses(p: &Proc, item: &Path, project: Option<&Path>) -> bool {
    if p.exe.as_deref().is_some_and(|exe| within(exe, item)) {
        return true;
    }
    let Some(project) = project else {
        return false;
    };
    let name = p.name.to_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    if !RUNTIMES.contains(&name) {
        return false;
    }
    p.cwd.as_deref().is_some_and(|cwd| within(cwd, project)) || mentions(&p.cmd, project)
}

/// Comparable form of a path: one separator, and no case on Windows and
/// macOS, whose file systems ignore it by default.
fn key(path: &str) -> String {
    let s = path.replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    s.trim_end_matches('/').to_lowercase()
}

fn within(path: &Path, dir: &Path) -> bool {
    let (p, d) = (
        key(&path.to_string_lossy()),
        key(&dir.to_string_lossy()),
    );
    p == d || p.strip_prefix(&d).is_some_and(|rest| rest.starts_with('/'))
}

/// Whether a command line names `dir` or something inside it. `C:\proj` does
/// not match `C:\project`.
fn mentions(cmd: &str, dir: &Path) -> bool {
    let (c, d) = (key(cmd), key(&dir.to_string_lossy()));
    if d.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(at) = c[from..].find(&d) {
        let end = from + at + d.len();
        match c[end..].chars().next() {
            None | Some('/' | '"' | '\'' | ' ') => return true,
            _ => from = end,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(name: &str, exe: Option<&str>, cwd: Option<&str>, cmd: &str) -> Proc {
        Proc {
            pid: 42,
            name: name.into(),
            exe: exe.map(PathBuf::from),
            cwd: cwd.map(PathBuf::from),
            cmd: cmd.into(),
        }
    }

    const NM: &str = "C:/Users/me/app/node_modules";

    #[test]
    fn a_dev_server_in_the_project_blocks_its_node_modules() {
        let by_cwd = Snapshot::of(vec![proc(
            "node.exe",
            Some("C:/Program Files/nodejs/node.exe"),
            Some("C:/Users/me/app"),
            "node server.js",
        )]);
        let why = by_cwd.blocker(Path::new(NM)).expect("blocked");
        assert!(why.contains("node.exe (process 42)"), "{why}");
        assert!(why.contains("Stop its dev server"), "{why}");

        let by_cmd = Snapshot::of(vec![proc(
            "node.exe",
            None,
            None,
            r#"node "C:\Users\me\app\node_modules\vite\bin\vite.js""#,
        )]);
        assert!(by_cmd.blocker(Path::new(NM)).is_some());
    }

    #[test]
    fn a_program_started_from_inside_the_folder_blocks_it() {
        let snap = Snapshot::of(vec![proc(
            "esbuild.exe",
            Some(r"C:\Users\me\app\node_modules\@esbuild\win32-x64\esbuild.exe"),
            None,
            "",
        )]);
        assert!(snap.blocker(Path::new(NM)).is_some());
    }

    #[test]
    fn terminals_editors_and_other_projects_do_not_block() {
        let snap = Snapshot::of(vec![
            proc("powershell.exe", None, Some("C:/Users/me/app"), "powershell"),
            proc("Code.exe", None, Some("C:/Users/me/app"), r"Code.exe C:\Users\me\app"),
            proc("node.exe", None, Some("C:/Users/me/app-two"), "node C:/Users/me/app-two/x.js"),
            proc("node", None, Some("/Users/me/other"), "node /Users/me/application/x.js"),
        ]);
        assert_eq!(snap.blocker(Path::new(NM)), None);
    }

    #[test]
    fn only_project_folders_look_at_the_project() {
        // A cache folder is not part of the project around it.
        let snap = Snapshot::of(vec![proc("node.exe", None, Some("C:/Users/me/AppData/Local"), "node")]);
        assert_eq!(snap.blocker(Path::new("C:/Users/me/AppData/Local/pip")), None);
    }

    #[test]
    fn paths_compare_without_case_or_separator_differences() {
        assert!(within(Path::new(r"\\?\C:\USERS\me\App\x"), Path::new("c:/users/me/app")));
        assert!(!within(Path::new("C:/Users/me/apps"), Path::new("C:/Users/me/app")));
        assert!(mentions("node C:/Users/me/app", Path::new(r"C:\Users\me\app")));
        assert!(!mentions("node C:/Users/me/apple/x.js", Path::new(r"C:\Users\me\app")));
    }

    /// Needs Node on PATH: `cargo test -- --ignored sees_a_real_node`.
    #[test]
    #[ignore]
    fn sees_a_real_node_process_working_in_a_project() {
        let dir = crate::test_support::sandbox();
        let project = crate::platform::canonicalize(dir.path()).unwrap().join("app");
        std::fs::create_dir_all(project.join("node_modules")).unwrap();
        let mut child = std::process::Command::new("node")
            .args(["-e", "setTimeout(() => {}, 20000)"])
            .current_dir(&project)
            .spawn()
            .expect("node on PATH");
        std::thread::sleep(std::time::Duration::from_millis(800));
        let why = Snapshot::take().blocker(&project.join("node_modules"));
        let _ = child.kill();
        let why = why.expect("the node process is seen");
        assert!(why.contains(&child.id().to_string()), "{why}");
        // Once it stops, the folder is free again.
        let _ = child.wait();
        assert_eq!(Snapshot::take().blocker(&project.join("node_modules")), None);
    }

    #[test]
    fn reads_real_processes() {
        // Smoke test: listing works and leaves DiskPilot (this test) out.
        let me = std::process::id();
        assert!(Snapshot::take().procs.iter().all(|p| p.pid != me));
    }
}
