//! Skills: folders holding a `SKILL.md`, found in each agent's skill
//! folders, with when each was last used.
//!
//! Claude Code records skill use itself. The other agents do not, so their
//! last use is estimated from recent session files: a skill counts as used
//! when a session read its `SKILL.md`. Lists of every skill that agents put
//! at the start of a session are ignored, so having a skill installed does
//! not count as using it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::{Skill, SkillOrigin, SkillState};
use crate::devtools::{modified_ms, Ctx, RecSpec, DAY_MS};
use crate::model::{CleanupMethod, MarkerKind, Recommendation, Tier};

/// A skill not used for this many days is inactive.
pub const INACTIVE_DAYS: i64 = 30;
/// Skills installed more recently than this are never called inactive.
const GRACE_DAYS: i64 = 14;
/// Session files older than this are not read when estimating use.
const SESSION_DAYS: i64 = 120;
/// At most this much session data is read per agent, newest first.
const SESSION_BUDGET: u64 = 768 * 1024 * 1024;
const SKILL_FILE: &str = "SKILL.md";

/// A skill folder found on disk.
pub struct Found {
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    pub origin: SkillOrigin,
    pub plugin: Option<String>,
}

/// Skill folders directly inside `root` (each `<root>/<name>/SKILL.md`).
/// Symlinked skills are listed but never offered for removal.
pub fn find(root: &Path, origin: SkillOrigin, plugin: Option<&str>) -> Vec<Found> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Found> = rd
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !dir.join(SKILL_FILE).is_file() {
                return None;
            }
            let (fm_name, description) = front_matter(&dir.join(SKILL_FILE));
            Some(Found {
                name: fm_name.unwrap_or(name),
                description: description.unwrap_or_default(),
                dir,
                origin,
                plugin: plugin.map(str::to_string),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `name` and `description` from a SKILL.md YAML front matter. Only the
/// first 16 KB are read.
pub fn front_matter(path: &Path) -> (Option<String>, Option<String>) {
    use std::io::Read;
    let mut buf = Vec::new();
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.take(16 * 1024).read_to_end(&mut buf);
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let (mut name, mut description) = (None, None);
    let mut in_description = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(unquote(v));
            in_description = false;
        } else if let Some(v) = line.strip_prefix("description:") {
            let v = v.trim();
            in_description = v.is_empty() || v == "|" || v == ">" || v == ">-" || v == "|-";
            description = Some(if in_description {
                String::new()
            } else {
                unquote(v)
            });
        } else if in_description && line.starts_with([' ', '\t']) {
            let d = description.get_or_insert_with(String::new);
            if !d.is_empty() {
                d.push(' ');
            }
            d.push_str(line.trim());
        } else {
            in_description = false;
        }
    }
    let description = description.map(|d| {
        let d = d.trim().to_string();
        if d.chars().count() > 240 {
            format!("{}…", d.chars().take(239).collect::<String>())
        } else {
            d
        }
    });
    (name.filter(|n| !n.is_empty()), description)
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    v.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(v)
        .to_string()
}

/// How often a skill was used and when last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub count: u64,
    pub last_ms: i64,
}

/// Estimate skill use from session files under `roots`: for every
/// `.../<skill>/SKILL.md` a session read, the newest such session's time.
/// Keyed by skill folder name.
pub fn usage_from_sessions(roots: &[PathBuf], now_ms: i64) -> HashMap<String, Usage> {
    let since = now_ms - SESSION_DAYS * DAY_MS;
    let mut files: Vec<(PathBuf, i64, u64)> = Vec::new();
    for root in roots {
        collect_sessions(root, since, 0, &mut files);
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    let mut budget = SESSION_BUDGET;
    files.retain(|f| {
        if budget == 0 {
            return false;
        }
        budget = budget.saturating_sub(f.2);
        true
    });
    let per_file: Vec<(i64, Vec<String>)> = files
        .par_iter()
        .filter_map(|(path, mtime, _)| {
            let bytes = std::fs::read(path).ok()?;
            let names = skills_read_in(&bytes);
            (!names.is_empty()).then_some((*mtime, names))
        })
        .collect();
    let mut out: HashMap<String, Usage> = HashMap::new();
    for (mtime, names) in per_file {
        for n in names {
            let u = out.entry(n).or_default();
            u.count += 1;
            u.last_ms = u.last_ms.max(mtime);
        }
    }
    out
}

fn collect_sessions(dir: &Path, since: i64, depth: usize, out: &mut Vec<(PathBuf, i64, u64)>) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(t) = e.file_type() else { continue };
        let path = e.path();
        if t.is_dir() {
            collect_sessions(&path, since, depth + 1, out);
        } else if t.is_file()
            && path
                .extension()
                .is_some_and(|x| x == "jsonl" || x == "json" || x == "md" || x == "txt")
        {
            let Ok(meta) = e.metadata() else { continue };
            let Some(mtime) = meta.modified().ok().and_then(crate::scanner::to_ms) else {
                continue;
            };
            if mtime >= since {
                out.push((path, mtime, meta.len()));
            }
        }
    }
}

/// Skill folder names whose `SKILL.md` appears in a line of `bytes`, not
/// counting lines that list many skills at once (the catalog an agent
/// shows the model) or that come from the system or developer prompt.
pub fn skills_read_in(bytes: &[u8]) -> Vec<String> {
    let finder = memchr::memmem::Finder::new(b"/SKILL.md");
    let mut found: Vec<String> = Vec::new();
    // `pos` is always at the start of a line.
    let mut pos = 0;
    while pos < bytes.len() {
        let Some(i) = finder.find(&bytes[pos..]) else {
            break;
        };
        let at = pos + i;
        let ls = memchr::memrchr(b'\n', &bytes[pos..at]).map_or(pos, |n| pos + n + 1);
        let le = memchr::memchr(b'\n', &bytes[at..]).map_or(bytes.len(), |n| at + n);
        pos = le + 1;
        let line = &bytes[ls..le];
        let hits: Vec<usize> = finder.find_iter(line).collect();
        if hits.len() > 2
            || memchr::memmem::find(line, b"\"role\":\"developer\"").is_some()
            || memchr::memmem::find(line, b"\"role\":\"system\"").is_some()
            || memchr::memmem::find(line, b"\"role\": \"developer\"").is_some()
        {
            continue;
        }
        for h in hits {
            let before = &line[..h];
            let start = before
                .iter()
                .rposition(|&c| c == b'/' || c == b'\\' || c == b'"' || c == b' ')
                .map_or(0, |n| n + 1);
            if let Ok(name) = std::str::from_utf8(&before[start..]) {
                if !name.is_empty() && !found.iter().any(|f| f == name) {
                    found.push(name.to_string());
                }
            }
        }
    }
    found
}

/// Build the report rows for found skills, with recommendations for
/// inactive skills the user installed.
///
/// `usage` maps a skill key (from `key`) to its use; `known` says whether
/// the agent's use can be read at all.
pub fn rows(
    ctx: &Ctx,
    agent_id: &str,
    agent_name: &str,
    found: Vec<Found>,
    usage: &HashMap<String, Usage>,
    known: bool,
    key: impl Fn(&Found) -> String,
) -> (Vec<Skill>, Vec<Recommendation>) {
    let mut skills = Vec::new();
    let mut recs = Vec::new();
    for f in found {
        let k = key(&f);
        let u = usage.get(&k).copied();
        let bytes = ctx.size(&f.dir);
        let installed_ms = modified_ms(&f.dir.join(SKILL_FILE)).or_else(|| modified_ms(&f.dir));
        let state = if !known || f.origin == SkillOrigin::BuiltIn {
            SkillState::Unknown
        } else {
            let last = u.map(|u| u.last_ms).filter(|&ms| ms > 0);
            let recent = |ms: i64| ctx.days_since(ms) < INACTIVE_DAYS;
            let new = installed_ms.is_some_and(|ms| ctx.days_since(ms) < GRACE_DAYS);
            match last {
                Some(ms) if recent(ms) => SkillState::Active,
                _ if new => SkillState::Active,
                _ => SkillState::Inactive,
            }
        };
        let is_link = f
            .dir
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink());
        let mut recommendation_id = None;
        if state == SkillState::Inactive && f.origin == SkillOrigin::User && !is_link {
            let when = match u.map(|u| u.last_ms).filter(|&ms| ms > 0) {
                Some(ms) => format!("last used {} days ago", ctx.days_since(ms)),
                None => "not used in the sessions DiskPilot can see".into(),
            };
            let rec = RecSpec {
                source: agent_id,
                title: format!("Inactive {agent_name} skill · {}", f.name),
                path: &f.dir,
                bytes,
                tier: Tier::ReviewFirst,
                methods: &[CleanupMethod::Trash],
                detected_because: format!(
                    "A skill you installed for {agent_name}, {when}. Skills are loaded into every session's list of abilities, so unused ones add noise."
                ),
                safe_because: "Only this skill's folder is moved; the agent keeps working without it.",
                risks: "The agent can no longer use this skill until you install it again. Restore it from the Trash if you change your mind.",
                proposed_action: "Move the skill folder to the Trash",
                alternative: None,
                last_activity_ms: u.map(|u| u.last_ms).filter(|&ms| ms > 0).or(installed_ms),
                notes: vec![format!("Start a new {agent_name} session afterwards so it reloads its skills.")],
            }
            .build(ctx)
            .marked(MarkerKind::Leftover, format!("Not used in {INACTIVE_DAYS} days"));
            recommendation_id = Some(rec.id.clone());
            recs.push(rec);
        }
        skills.push(Skill {
            id: format!("{agent_id}:{}", f.dir.display()),
            name: f.name.clone(),
            description: f.description.clone(),
            path: f.dir.display().to_string(),
            bytes,
            origin: f.origin,
            plugin: f.plugin.clone(),
            last_used_ms: u.map(|u| u.last_ms).filter(|&ms| ms > 0),
            uses: u.map(|u| u.count),
            state,
            recommendation_id,
        });
    }
    (skills, recs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_and_folded_description() {
        let dir = crate::test_support::sandbox();
        let skill = dir.path().join("pdf");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: pdf-tools\ndescription: >\n  Fill and merge\n  PDF forms.\nlicense: MIT\n---\n# Body\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("not-a-skill")).unwrap();
        let found = find(dir.path(), SkillOrigin::User, None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "pdf-tools");
        assert_eq!(found[0].description, "Fill and merge PDF forms.");
    }

    #[test]
    fn a_session_reading_a_skill_counts_but_the_skill_catalog_does_not() {
        let catalog = br#"{"role":"developer","content":"- a (file: /h/.codex/skills/a/SKILL.md)"}
{"type":"message","content":"skills: /h/s/a/SKILL.md /h/s/b/SKILL.md /h/s/c/SKILL.md"}
{"type":"function_call","arguments":"{\"cmd\":\"cat /h/.codex/skills/pdf/SKILL.md\"}"}
"#;
        assert_eq!(skills_read_in(catalog), vec!["pdf".to_string()]);
    }

    #[test]
    fn usage_takes_the_newest_session_and_counts_sessions() {
        let dir = crate::test_support::sandbox();
        let s = dir.path().join("sessions/2026/11/01");
        std::fs::create_dir_all(&s).unwrap();
        for n in 0..3 {
            std::fs::write(
                s.join(format!("r{n}.jsonl")),
                "{\"type\":\"function_call\",\"arguments\":\"sed -n 1,80p /x/skills/deploy/SKILL.md\"}\n",
            )
            .unwrap();
        }
        let now = crate::scanner::to_ms(std::time::SystemTime::now()).unwrap();
        let u = usage_from_sessions(&[dir.path().join("sessions")], now);
        assert_eq!(u["deploy"].count, 3);
        assert!(now - u["deploy"].last_ms < DAY_MS);
    }
}
