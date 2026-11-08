//! Windows path handling for the classifier, written as plain text
//! processing so it behaves the same on every platform and the Windows rule
//! table can be tested on macOS.
//!
//! Both `\` and `/` separate folders, the `\\?\` and `\\?\UNC\` prefixes are
//! understood, and names compare ignoring case, as NTFS does.

use std::path::{Path, PathBuf};

use super::rules::Root;

/// Marks a UNC path (`\\server\share\...`): the server and share follow.
const UNC: &str = "\\\\";

fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// Compare two names ignoring case. ASCII is compared without allocating;
/// other letters (a user name such as `Jörg`) by their lowercase form.
pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    if a.is_ascii() && b.is_ascii() {
        return false;
    }
    a.to_lowercase() == b.to_lowercase()
}

/// Whether `name` matches a rule part: `*` matches any name, `prefix*` any
/// name starting with `prefix`, anything else the same name.
pub fn part_matches(name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.len() >= prefix.len()
            && name.is_char_boundary(prefix.len())
            && eq_ignore_case(&name[..prefix.len()], prefix);
    }
    eq_ignore_case(name, pattern)
}

/// A Windows path split into names. For `C:\Users\me` the parts are
/// `["C:", "Users", "me"]`; for `\\server\share\x` they are
/// `["\\\\", "server", "share", "x"]`.
#[derive(Debug)]
pub struct WinPath<'a> {
    original: &'a str,
    pub parts: Vec<&'a str>,
    /// Byte range of each part in `original` (the UNC marker has none).
    spans: Vec<(usize, usize)>,
    /// How many parts form the root (`C:` is one, `\\server\share` three).
    root_parts: usize,
}

impl<'a> WinPath<'a> {
    pub fn parse(original: &'a str) -> Self {
        let mut parts = Vec::new();
        let mut spans = Vec::new();
        let mut start = 0;
        let mut unc = false;
        if let Some(rest) = strip_prefix_ignore_case(original, "\\\\?\\UNC\\") {
            start = original.len() - rest.len();
            unc = true;
        } else if original.starts_with("\\\\?\\") || original.starts_with("\\\\.\\") {
            start = 4;
        } else if original.starts_with("\\\\") || original.starts_with("//") {
            start = 2;
            unc = true;
        }
        if unc {
            parts.push(UNC);
            spans.push((start, start));
        }
        let mut seg_start = None;
        for (i, c) in original[start..].char_indices() {
            let i = start + i;
            if is_sep(c) {
                if let Some(s) = seg_start.take() {
                    parts.push(&original[s..i]);
                    spans.push((s, i));
                }
            } else if seg_start.is_none() {
                seg_start = Some(i);
            }
        }
        if let Some(s) = seg_start {
            parts.push(&original[s..]);
            spans.push((s, original.len()));
        }
        let root_parts = if unc {
            3.min(parts.len())
        } else if parts.first().is_some_and(|p| is_drive(p)) {
            1
        } else {
            0
        };
        Self {
            original,
            parts,
            spans,
            root_parts,
        }
    }

    /// The last name, unless the path is only a drive or share.
    pub fn file_name(&self) -> Option<&'a str> {
        if self.parts.len() > self.root_parts {
            self.parts.last().copied()
        } else {
            None
        }
    }

    /// The root folder as text, separator included (`C:\`).
    fn root_text(&self) -> Option<&'a str> {
        if self.root_parts == 0 {
            let first = self.spans.first()?.0;
            return (first > 0).then(|| &self.original[..first]);
        }
        let end = self.spans[self.root_parts - 1].1;
        let with_sep = self.original[end..].starts_with(is_sep);
        Some(&self.original[..if with_sep { end + 1 } else { end }])
    }

    /// The parent folder, cut from the text of `path`. On Windows this is
    /// exactly `path.parent()`.
    pub fn parent_path(&self, path: &'a Path) -> Option<&'a Path> {
        if cfg!(windows) && path.to_str() == Some(self.original) {
            return path.parent();
        }
        let n = self.parts.len();
        if n <= self.root_parts {
            return None;
        }
        if n == self.root_parts + 1 {
            return self.root_text().map(Path::new);
        }
        Some(Path::new(&self.original[..self.spans[n - 2].1]))
    }

    /// Whether the path is `root` followed by exactly `parts` (rule parts may
    /// use `*` patterns).
    pub fn is_root_plus<S: AsRef<str>>(&self, root: &[S], parts: &[&str]) -> bool {
        !root.is_empty()
            && self.parts.len() == root.len() + parts.len()
            && self.starts_with_parts(root)
            && self.parts[root.len()..]
                .iter()
                .zip(parts)
                .all(|(name, pattern)| part_matches(name, pattern))
    }

    /// Whether the path is a drive root (`D:`) followed by exactly `parts`.
    pub fn is_drive_plus(&self, parts: &[&str]) -> bool {
        self.parts.first().is_some_and(|d| is_drive(d))
            && self.parts.len() == 1 + parts.len()
            && self.parts[1..]
                .iter()
                .zip(parts)
                .all(|(name, pattern)| part_matches(name, pattern))
    }

    /// Whether the path is `prefix` or inside it.
    pub fn starts_with_parts<S: AsRef<str>>(&self, prefix: &[S]) -> bool {
        self.parts.len() >= prefix.len()
            && self
                .parts
                .iter()
                .zip(prefix)
                .all(|(a, b)| eq_ignore_case(a, b.as_ref()))
    }

    /// Whether the path ends with `parts` (rule parts may use `*` patterns).
    pub fn ends_with(&self, parts: &[&str]) -> bool {
        let n = self.parts.len();
        n >= self.root_parts + parts.len()
            && self.parts[n - parts.len()..]
                .iter()
                .zip(parts)
                .all(|(name, pattern)| part_matches(name, pattern))
    }
}

fn is_drive(part: &str) -> bool {
    let b = part.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    (s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix))
    .then(|| &s[prefix.len()..])
}

/// Every folder from the root down to `path` itself, cut from its text:
/// `C:\`, `C:\Users`, `C:\Users\me`.
pub fn ancestors(path: &str) -> Vec<&str> {
    let p = WinPath::parse(path);
    let mut out = Vec::with_capacity(p.parts.len() + 1);
    out.extend(p.root_text());
    for &(_, end) in &p.spans[p.root_parts..] {
        out.push(&path[..end]);
    }
    if out.is_empty() {
        out.push(path);
    }
    out
}

/// The well-known Windows folders the rule table is relative to.
#[derive(Debug, Clone)]
pub struct WindowsRoots {
    home: PathBuf,
    /// Parts of each [`Root`] except `AnyDrive`, in declaration order.
    parts: [Vec<String>; 7],
}

impl WindowsRoots {
    /// Roots from explicit folders.
    pub fn new(
        home: &str,
        local_app_data: &str,
        roaming_app_data: &str,
        program_data: &str,
        system_root: &str,
        temp: &str,
        system_drive: &str,
    ) -> Self {
        let split = |s: &str| -> Vec<String> {
            WinPath::parse(s)
                .parts
                .into_iter()
                .map(str::to_string)
                .collect()
        };
        Self {
            home: PathBuf::from(home),
            parts: [
                split(home),
                split(local_app_data),
                split(roaming_app_data),
                split(program_data),
                split(system_root),
                split(temp),
                split(system_drive),
            ],
        }
    }

    /// Windows' default layout for a profile folder such as `C:\Users\me`.
    pub fn for_home(home: &str) -> Self {
        let drive = WinPath::parse(home)
            .parts
            .first()
            .filter(|d| is_drive(d))
            .map_or_else(|| "C:".to_string(), |d| d.to_string());
        let home_trimmed = home.trim_end_matches(is_sep);
        let local = format!("{home_trimmed}\\AppData\\Local");
        Self::new(
            home,
            &local,
            &format!("{home_trimmed}\\AppData\\Roaming"),
            &format!("{drive}\\ProgramData"),
            &format!("{drive}\\Windows"),
            &format!("{local}\\Temp"),
            &drive,
        )
    }

    /// Roots from the environment (`%LOCALAPPDATA%`, `%APPDATA%`,
    /// `%ProgramData%`, `%SystemRoot%`, `%TEMP%`, `%SystemDrive%`), falling
    /// back to Windows' default layout. Short (8.3) names, which `%TEMP%`
    /// often uses, are expanded to long ones. The per-user folders come from
    /// the environment only when `home` is this user's profile
    /// (`%USERPROFILE%`); for another folder, the default layout inside it
    /// applies.
    #[cfg(windows)]
    pub fn from_env(home: &Path) -> Self {
        let home_text = home.to_string_lossy().into_owned();
        let defaults = Self::for_home(&home_text);
        let long = |v: std::ffi::OsString| -> String {
            let p = PathBuf::from(v);
            std::fs::canonicalize(&p)
                .unwrap_or(p)
                .to_string_lossy()
                .into_owned()
        };
        let this_user = std::env::var_os("USERPROFILE").is_some_and(|profile| {
            let profile = long(profile);
            let a = WinPath::parse(&profile);
            let b = WinPath::parse(&home_text);
            a.parts.len() == b.parts.len() && a.starts_with_parts(&b.parts)
        });
        let var = |name: &str, root: Root, per_user: bool| -> String {
            std::env::var_os(name)
                .filter(|v| !v.is_empty() && (this_user || !per_user))
                .map(long)
                .unwrap_or_else(|| defaults.parts(root).join("\\"))
        };
        Self::new(
            &home_text,
            &var("LOCALAPPDATA", Root::LocalAppData, true),
            &var("APPDATA", Root::RoamingAppData, true),
            &var("ProgramData", Root::ProgramData, false),
            &var("SystemRoot", Root::SystemRoot, false),
            &var("TEMP", Root::Temp, true),
            &var("SystemDrive", Root::SystemDrive, false),
        )
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The parts of a root. `AnyDrive` has none: it is matched separately.
    pub fn parts(&self, root: Root) -> &[String] {
        let i = match root {
            Root::Home => 0,
            Root::LocalAppData => 1,
            Root::RoamingAppData => 2,
            Root::ProgramData => 3,
            Root::SystemRoot => 4,
            Root::Temp => 5,
            Root::SystemDrive => 6,
            Root::AnyDrive => return &[],
        };
        &self.parts[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_drive_unc_and_verbatim_paths() {
        assert_eq!(
            WinPath::parse(r"C:\Users\me\AppData").parts,
            ["C:", "Users", "me", "AppData"]
        );
        assert_eq!(WinPath::parse("c:/users//me/").parts, ["c:", "users", "me"]);
        assert_eq!(
            WinPath::parse(r"\\?\C:\Users\me").parts,
            ["C:", "Users", "me"]
        );
        assert_eq!(
            WinPath::parse(r"\\server\share\x").parts,
            [UNC, "server", "share", "x"]
        );
        assert_eq!(
            WinPath::parse(r"\\?\UNC\server\share\x").parts,
            [UNC, "server", "share", "x"]
        );
        assert_eq!(WinPath::parse(r"C:\").file_name(), None);
        assert_eq!(WinPath::parse(r"\\server\share").file_name(), None);
        assert_eq!(WinPath::parse(r"C:\a.txt").file_name(), Some("a.txt"));
    }

    #[test]
    fn ancestors_run_from_the_root_down() {
        assert_eq!(
            ancestors(r"C:\Users\me\code"),
            [r"C:\", r"C:\Users", r"C:\Users\me", r"C:\Users\me\code"]
        );
        assert_eq!(ancestors(r"\\?\D:\x"), [r"\\?\D:\", r"\\?\D:\x"]);
        assert_eq!(
            ancestors(r"\\server\share\x\y"),
            [
                r"\\server\share\",
                r"\\server\share\x",
                r"\\server\share\x\y"
            ]
        );
        assert_eq!(ancestors(r"C:\"), [r"C:\"]);
        assert_eq!(ancestors("/Users/me"), ["/", "/Users", "/Users/me"]);
    }

    #[test]
    fn parents_are_cut_from_the_text() {
        let text = r"C:\Users\me\code\app\bin";
        let p = WinPath::parse(text);
        assert_eq!(
            p.parent_path(Path::new(text)),
            Some(Path::new(r"C:\Users\me\code\app"))
        );
        let text = r"C:\Windows";
        let p = WinPath::parse(text);
        assert_eq!(p.parent_path(Path::new(text)), Some(Path::new(r"C:\")));
        let text = r"C:\";
        assert_eq!(WinPath::parse(text).parent_path(Path::new(text)), None);
    }

    #[test]
    fn names_compare_ignoring_case_and_patterns() {
        assert!(eq_ignore_case("AppData", "APPDATA"));
        assert!(eq_ignore_case("Jörg", "JÖRG"));
        assert!(!eq_ignore_case("Temp", "Tmp"));
        assert!(part_matches("webcache_4147", "webcache*"));
        assert!(part_matches("OneDrive - Contoso", "OneDrive*"));
        assert!(part_matches("Profile 3", "*"));
        assert!(!part_matches("cache", "webcache*"));
        let p = WinPath::parse(r"c:\users\ME\appdata\local\Temp");
        let roots = WindowsRoots::for_home(r"C:\Users\me");
        assert!(p.is_root_plus(roots.parts(Root::Temp), &[]));
        assert!(p.is_root_plus(roots.parts(Root::LocalAppData), &["TEMP"]));
        assert!(!p.is_root_plus(roots.parts(Root::LocalAppData), &[]));
    }

    #[test]
    fn default_roots_follow_the_profile_drive() {
        let roots = WindowsRoots::for_home(r"D:\Users\me");
        assert_eq!(roots.parts(Root::SystemRoot), ["D:", "Windows"]);
        assert_eq!(roots.parts(Root::SystemDrive), ["D:"]);
        assert_eq!(
            roots.parts(Root::RoamingAppData),
            ["D:", "Users", "me", "AppData", "Roaming"]
        );
        assert!(roots.parts(Root::AnyDrive).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn environment_roots_apply_to_this_user_only() {
        let profile = std::env::var("USERPROFILE").unwrap();
        let mine = WindowsRoots::from_env(Path::new(&profile));
        let local = std::env::var("LOCALAPPDATA").unwrap();
        let local = std::fs::canonicalize(&local).unwrap();
        let local = local.to_string_lossy();
        assert!(WinPath::parse(&local).starts_with_parts(mine.parts(Root::LocalAppData)));
        // Another folder used as a home gets the default layout inside it.
        let other = WindowsRoots::from_env(Path::new(r"D:\fixture\home"));
        assert_eq!(
            other.parts(Root::LocalAppData),
            ["D:", "fixture", "home", "AppData", "Local"]
        );
        assert_eq!(
            other.parts(Root::Temp),
            ["D:", "fixture", "home", "AppData", "Local", "Temp"]
        );
    }
}
