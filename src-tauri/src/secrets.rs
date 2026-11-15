//! Offline check for credentials pasted into AI coding agents' transcripts.
//!
//! Agents keep full conversation logs on disk, and anything pasted into a
//! chat or printed by a command (API keys, tokens, private keys) ends up in
//! them. This module reads those logs line by line and reports where
//! well-known credential formats appear.
//!
//! Nothing leaves the Mac and the credentials themselves are never returned:
//! findings carry only a masked form (a short prefix and the last four
//! characters) so the user can recognize which key it was. Checking changes
//! nothing; `remove_from_file` replaces the keys in one transcript, when the
//! user asks, with a marker and leaves the rest of the conversation as it was.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use serde::Serialize;

use crate::scanner::{now_ms, to_ms};

/// Where each agent keeps its transcripts, relative to the home folder.
const SOURCES: &[(&str, &str)] = &[
    ("Claude Code", ".claude/projects"),
    ("Claude Code", ".claude/history.jsonl"),
    ("Codex CLI", ".codex/sessions"),
    ("Codex CLI", ".codex/history.jsonl"),
    ("Gemini CLI", ".gemini/tmp"),
];
const EXTENSIONS: &[&str] = &["jsonl", "json"];
/// Lines longer than this are cut; transcripts can embed whole files.
const MAX_LINE: usize = 4 * 1024 * 1024;

struct Pattern {
    kind: &'static str,
    prefix: &'static str,
    /// Characters after the prefix, at least.
    min_len: usize,
    body: fn(u8) -> bool,
}

fn token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}
fn upper_or_digit(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}
fn alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

/// Ordered so the more specific prefix wins (`sk-ant-` before `sk-`).
const PATTERNS: &[Pattern] = &[
    Pattern {
        kind: "Anthropic API key",
        prefix: "sk-ant-",
        min_len: 32,
        body: token_char,
    },
    Pattern {
        kind: "OpenAI API key",
        prefix: "sk-proj-",
        min_len: 32,
        body: token_char,
    },
    Pattern {
        kind: "Stripe secret key",
        prefix: "sk_live_",
        min_len: 20,
        body: alnum,
    },
    Pattern {
        kind: "OpenAI API key",
        prefix: "sk-",
        min_len: 40,
        body: token_char,
    },
    Pattern {
        kind: "GitHub token",
        prefix: "github_pat_",
        min_len: 40,
        body: token_char,
    },
    Pattern {
        kind: "GitHub token",
        prefix: "ghp_",
        min_len: 36,
        body: alnum,
    },
    Pattern {
        kind: "GitHub token",
        prefix: "gho_",
        min_len: 36,
        body: alnum,
    },
    Pattern {
        kind: "GitHub token",
        prefix: "ghs_",
        min_len: 36,
        body: alnum,
    },
    Pattern {
        kind: "GitHub token",
        prefix: "ghu_",
        min_len: 36,
        body: alnum,
    },
    Pattern {
        kind: "AWS access key",
        prefix: "AKIA",
        min_len: 16,
        body: upper_or_digit,
    },
    Pattern {
        kind: "Google API key",
        prefix: "AIza",
        min_len: 35,
        body: token_char,
    },
    Pattern {
        kind: "Slack token",
        prefix: "xoxb-",
        min_len: 20,
        body: token_char,
    },
    Pattern {
        kind: "Slack token",
        prefix: "xoxp-",
        min_len: 20,
        body: token_char,
    },
    Pattern {
        kind: "Hugging Face token",
        prefix: "hf_",
        min_len: 30,
        body: alnum,
    },
    Pattern {
        kind: "npm token",
        prefix: "npm_",
        min_len: 36,
        body: alnum,
    },
    Pattern {
        kind: "Private key",
        prefix: "-----BEGIN ",
        min_len: 0,
        body: |_| false,
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretMatch {
    pub kind: &'static str,
    /// Prefix and last four characters only.
    pub masked: String,
    /// 1-based line of the first occurrence.
    pub line: u64,
    pub occurrences: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptFinding {
    pub agent: &'static str,
    pub path: String,
    pub bytes: u64,
    pub modified_ms: Option<i64>,
    pub matches: Vec<SecretMatch>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsReport {
    pub generated_at_ms: i64,
    pub files_checked: u64,
    pub bytes_checked: u64,
    /// Agents whose transcripts were found.
    pub agents: Vec<&'static str>,
    pub findings: Vec<TranscriptFinding>,
    pub cancelled: bool,
}

fn mask(kind: &str, prefix: &str, token: &str) -> String {
    if kind == "Private key" {
        return "-----BEGIN … PRIVATE KEY-----".into();
    }
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}…{tail}")
}

/// A real key mixes letters and digits; placeholders such as
/// `sk-ant-xxxxxxxx…` or `AKIAEXAMPLEEXAMPLE` do not.
fn looks_real(body: &[u8]) -> bool {
    let digits = body.iter().filter(|b| b.is_ascii_digit()).count();
    let letters = body.iter().filter(|b| b.is_ascii_alphabetic()).count();
    let distinct = {
        let mut seen = [false; 256];
        body.iter()
            .filter(|&&b| !std::mem::replace(&mut seen[b as usize], true))
            .count()
    };
    digits >= 2 && letters >= 4 && distinct >= 10
}

/// One credential in a line: what it is and the bytes it covers.
struct Span {
    kind: &'static str,
    prefix: &'static str,
    start: usize,
    /// End of the prefix; the key's body runs from here to `end`.
    body: usize,
    end: usize,
}

/// Where a private key block that starts at `start` ends. Transcripts keep it
/// inside one JSON string with escaped newlines, so the block ends at its
/// `-----END … PRIVATE KEY-----` line or, if that was cut off, where the
/// base64 stops. Never runs past a quote, and never ends between a backslash
/// and the character it escapes, so the JSON stays valid when it is replaced.
fn private_key_end(line: &[u8], header_end: usize) -> usize {
    let window = &line[header_end..line.len().min(header_end + 16 * 1024)];
    if let Some(e) = find(window, b"-----END ") {
        if let Some(k) = find(&window[e..], b"PRIVATE KEY-----") {
            return header_end + e + k + "PRIVATE KEY-----".len();
        }
    }
    let run = window
        .iter()
        .take_while(|&&b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'\\'))
        .count();
    let mut end = header_end + run;
    let slashes = line[header_end..end]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count();
    if slashes % 2 == 1 {
        end -= 1;
    }
    end
}

/// Credentials in one line, in the order their patterns are listed.
fn spans(line: &[u8]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for p in PATTERNS {
        let pre = p.prefix.as_bytes();
        let mut from = 0;
        while let Some(off) = find(&line[from..], pre) {
            let start = from + off;
            from = start + pre.len();
            if start > 0 && token_char(line[start - 1]) {
                continue; // inside a longer word
            }
            if out.iter().any(|s| start >= s.start && start < s.end) {
                continue; // already matched by a more specific prefix
            }
            if p.kind == "Private key" {
                let rest = &line[from..line.len().min(from + 40)];
                if let Some(h) = find(rest, b"PRIVATE KEY-----") {
                    let header_end = from + h + "PRIVATE KEY-----".len();
                    out.push(Span {
                        kind: p.kind,
                        prefix: p.prefix,
                        start,
                        body: from,
                        end: private_key_end(line, header_end),
                    });
                    from = header_end;
                }
                continue;
            }
            let len = line[from..].iter().take_while(|&&b| (p.body)(b)).count();
            if len < p.min_len || !looks_real(&line[from..from + len]) {
                continue;
            }
            out.push(Span {
                kind: p.kind,
                prefix: p.prefix,
                start,
                body: from,
                end: from + len,
            });
        }
    }
    out
}

/// Credentials in one line: (kind, masked form).
fn scan_line(line: &[u8]) -> Vec<(&'static str, String)> {
    spans(line)
        .into_iter()
        .map(|s| {
            let token = String::from_utf8_lossy(&line[s.body..s.end]);
            (s.kind, mask(s.kind, s.prefix, &token))
        })
        .collect()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn skip_rest_of_line(reader: &mut impl BufRead) {
    loop {
        let (done, used) = match reader.fill_buf() {
            Ok([]) | Err(_) => return,
            Ok(chunk) => match chunk.iter().position(|&b| b == b'\n') {
                Some(i) => (true, i + 1),
                None => (false, chunk.len()),
            },
        };
        reader.consume(used);
        if done {
            return;
        }
    }
}

fn scan_file(path: &Path, cancel: &AtomicBool) -> Vec<SecretMatch> {
    let Ok(f) = File::open(path) else {
        return Vec::new();
    };
    let mut reader = BufReader::with_capacity(256 * 1024, f);
    let mut found: BTreeMap<(&'static str, String), SecretMatch> = BTreeMap::new();
    let mut buf = Vec::new();
    let mut line_no = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        buf.clear();
        // Read at most MAX_LINE bytes of the line, then skip the rest, so a
        // file with one enormous line is never loaded into memory.
        match reader
            .by_ref()
            .take(MAX_LINE as u64)
            .read_until(b'\n', &mut buf)
        {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if buf.last() != Some(&b'\n') && buf.len() == MAX_LINE {
            skip_rest_of_line(&mut reader);
        }
        line_no += 1;
        for (kind, masked) in scan_line(&buf) {
            found
                .entry((kind, masked.clone()))
                .and_modify(|m| m.occurrences += 1)
                .or_insert(SecretMatch {
                    kind,
                    masked,
                    line: line_no,
                    occurrences: 1,
                });
        }
    }
    found.into_values().collect()
}

fn transcript_files(root: &Path) -> Vec<PathBuf> {
    let Ok(meta) = root.symlink_metadata() else {
        return Vec::new();
    };
    if meta.is_file() {
        return vec![root.to_path_buf()];
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(t) = e.file_type() else { continue };
            let p = e.path();
            if t.is_dir() {
                stack.push(p);
            } else if t.is_file()
                && p.extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| EXTENSIONS.contains(&x))
            {
                out.push(p);
            }
        }
    }
    out
}

pub fn check(home: &Path, cancel: &AtomicBool) -> SecretsReport {
    let mut agents: Vec<&'static str> = Vec::new();
    let mut files: Vec<(&'static str, PathBuf)> = Vec::new();
    for (agent, rel) in SOURCES {
        let found = transcript_files(&home.join(rel));
        if !found.is_empty() && !agents.contains(agent) {
            agents.push(agent);
        }
        files.extend(found.into_iter().map(|p| (*agent, p)));
    }
    let bytes_checked = files
        .iter()
        .filter_map(|(_, p)| p.symlink_metadata().ok())
        .map(|m| m.len())
        .sum();
    let mut findings: Vec<TranscriptFinding> = files
        .par_iter()
        .filter_map(|(agent, path)| {
            let matches = scan_file(path, cancel);
            if matches.is_empty() {
                return None;
            }
            let meta = path.symlink_metadata().ok();
            Some(TranscriptFinding {
                agent,
                path: path.display().to_string(),
                bytes: meta.as_ref().map_or(0, |m| m.len()),
                modified_ms: meta.and_then(|m| m.modified().ok()).and_then(to_ms),
                matches,
            })
        })
        .collect();
    findings.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms).then(a.path.cmp(&b.path)));
    SecretsReport {
        generated_at_ms: now_ms(),
        files_checked: files.len() as u64,
        bytes_checked,
        agents,
        findings,
        cancelled: cancel.load(Ordering::Relaxed),
    }
}

/// What replaces each key in a transcript. Plain text, so it stays valid
/// inside a JSON string.
pub const REMOVED: &str = "[removed by DiskPilot]";

/// A transcript written to this recently may belong to a conversation that is
/// still open; the agent would keep appending to the file being replaced.
pub const OPEN_WINDOW_MS: i64 = 2 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum RemoveError {
    #[error("This file is not one of the agent transcripts DiskPilot checks.")]
    NotATranscript,
    #[error("This conversation was used in the last two minutes. Close it in the agent, then try again.")]
    InUse,
    #[error(
        "The transcript changed while DiskPilot was working on it. Nothing was changed; try again."
    )]
    Changed,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Is `path` a transcript file inside one of the agents' folders? Both sides
/// are resolved, so a link cannot point the edit somewhere else.
fn is_transcript(home: &Path, path: &Path) -> bool {
    let Ok(meta) = path.symlink_metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    let ext_ok = path
        .extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| EXTENSIONS.contains(&x));
    let Ok(real) = path.canonicalize() else {
        return false;
    };
    ext_ok
        && SOURCES.iter().any(|(_, rel)| {
            home.join(rel)
                .canonicalize()
                .is_ok_and(|root| real == root || real.starts_with(&root))
        })
}

/// Replace every credential in one line; returns how many were replaced.
fn redact_line(line: &[u8], out: &mut Vec<u8>) -> u64 {
    let mut found = spans(line);
    if found.is_empty() {
        out.extend_from_slice(line);
        return 0;
    }
    found.sort_by_key(|s| s.start);
    let mut at = 0;
    let mut count = 0;
    for s in found {
        if s.start < at {
            at = at.max(s.end); // overlaps one already replaced
            continue;
        }
        out.extend_from_slice(&line[at..s.start]);
        out.extend_from_slice(REMOVED.as_bytes());
        at = s.end;
        count += 1;
    }
    out.extend_from_slice(&line[at..]);
    count
}

fn stamp(meta: &std::fs::Metadata) -> (u64, Option<std::time::SystemTime>) {
    (meta.len(), meta.modified().ok())
}

/// Replace the credentials in one transcript with [`REMOVED`] and keep
/// everything else byte for byte. The new file is written next to the old one
/// and moved over it in one step, keeping its permissions, so the transcript
/// is never half-written. No copy of the old file is kept: that would keep
/// the keys. Returns how many keys were replaced.
pub fn remove_from_file(home: &Path, path: &Path, now: i64) -> Result<u64, RemoveError> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    if !is_transcript(home, path) {
        return Err(RemoveError::NotATranscript);
    }
    let before = path.metadata()?;
    if before
        .modified()
        .ok()
        .and_then(to_ms)
        .is_some_and(|m| now - m < OPEN_WINDOW_MS)
    {
        return Err(RemoveError::InUse);
    }
    let dir = path.parent().ok_or(RemoveError::NotATranscript)?;
    let name = path
        .file_name()
        .ok_or(RemoveError::NotATranscript)?
        .to_string_lossy();
    let tmp = dir.join(format!(".{name}.diskpilot-{}", std::process::id()));

    let result = (|| -> Result<u64, RemoveError> {
        let mut reader = BufReader::with_capacity(256 * 1024, File::open(path)?);
        let mut out = std::io::BufWriter::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?,
        );
        let mut line = Vec::new();
        let mut redacted = Vec::new();
        let mut count = 0;
        loop {
            line.clear();
            if reader.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            redacted.clear();
            count += redact_line(&line, &mut redacted);
            out.write_all(&redacted)?;
        }
        let file = out.into_inner().map_err(|e| e.into_error())?;
        file.set_permissions(std::fs::Permissions::from_mode(
            before.permissions().mode() & 0o7777,
        ))?;
        file.sync_all()?;
        // An agent that appended meanwhile would lose those lines.
        if stamp(&path.metadata()?) != stamp(&before) {
            return Err(RemoveError::Changed);
        }
        if count > 0 {
            std::fs::rename(&tmp, path)?;
        }
        Ok(count)
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // Built at runtime so this file never contains a credential-shaped string.
    fn fake(prefix: &str, body: &str) -> String {
        format!("{prefix}{body}")
    }

    #[test]
    fn finds_and_masks_known_formats() {
        let ant = fake("sk-ant-", "api03-a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuV");
        let gh = fake("ghp_", "a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuVwXyZ");
        let aws = fake("AKIA", "Z7Q3M9X2B5N8C4V1");
        let line = format!(r#"{{"text":"export KEY={ant} and {gh}; aws {aws}"}}"#);
        let found = scan_line(line.as_bytes());
        let kinds: Vec<&str> = found.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec!["Anthropic API key", "GitHub token", "AWS access key"]
        );
        assert_eq!(found[0].1, "sk-ant-…sTuV");
        assert!(found.iter().all(|(_, m)| !m.contains("a1B2c3")));
    }

    #[test]
    fn an_enormous_line_is_skipped_without_losing_line_numbers() {
        let dir = crate::test_support::sandbox();
        let file = dir.path().join("t.jsonl");
        let ant = fake("sk-ant-", "api03-a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuV");
        let mut text = "x".repeat(MAX_LINE * 2);
        text.push('\n');
        text.push_str(&format!("key {ant}\n"));
        std::fs::write(&file, text).unwrap();
        let found = scan_file(&file, &AtomicBool::new(false));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
    }

    #[test]
    fn ignores_placeholders_and_words() {
        let line = format!(
            "use {} or {} like task-ant-{}",
            fake("sk-ant-", &"x".repeat(40)),
            fake("AKIA", "EXAMPLEEXAMPLEEX"),
            "abcdefghij0123456789abcdefghij0123"
        );
        assert!(scan_line(line.as_bytes()).is_empty());
    }

    #[test]
    fn detects_private_keys() {
        let line = format!("{}RSA PRIVATE KEY-----\\nMIIE", "-----BEGIN ");
        assert_eq!(scan_line(line.as_bytes())[0].0, "Private key");
    }

    #[test]
    fn reports_files_with_counts_and_first_line() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let key = fake("hf_", "a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuVw");
        let log = home.join(".claude/projects/-Users-x-app/s1.jsonl");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(
            &log,
            format!("{{}}\n{{\"t\":\"{key}\"}}\n{{\"t\":\"again {key}\"}}\n"),
        )
        .unwrap();
        std::fs::write(log.with_file_name("clean.jsonl"), "{\"t\":\"nothing\"}\n").unwrap();

        let r = check(&home, &AtomicBool::new(false));
        assert_eq!(r.files_checked, 2);
        assert_eq!(r.agents, vec!["Claude Code"]);
        assert_eq!(r.findings.len(), 1);
        let m = &r.findings[0].matches[0];
        assert_eq!(
            (m.kind, m.line, m.occurrences),
            ("Hugging Face token", 2, 2)
        );
    }

    fn transcript(home: &Path, body: &str) -> PathBuf {
        let log = home.join(".claude/projects/-Users-x-app/s1.jsonl");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, body).unwrap();
        log
    }
    const LATER: i64 = i64::MAX / 2;

    #[test]
    fn removing_replaces_only_the_keys() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let key = fake("hf_", "a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuVw");
        let pem = format!(
            "{}OPENSSH PRIVATE KEY-----\\nb3BlbnNzaC1rZXk\\nAAAA+/==\\n-----END OPENSSH PRIVATE KEY-----",
            "-----BEGIN "
        );
        let body = format!(
            "{{\"t\":\"use {key} now\"}}\n{{\"t\":\"clean\"}}\n{{\"k\":\"{pem}\",\"n\":1}}\n"
        );
        let log = transcript(&home, &body);
        std::fs::set_permissions(&log, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .unwrap();

        assert_eq!(remove_from_file(&home, &log, LATER).unwrap(), 2);
        let after = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            after,
            format!("{{\"t\":\"use {REMOVED} now\"}}\n{{\"t\":\"clean\"}}\n{{\"k\":\"{REMOVED}\",\"n\":1}}\n")
        );
        for l in after.lines() {
            serde_json::from_str::<serde_json::Value>(l).unwrap();
        }
        let mode = std::os::unix::fs::PermissionsExt::mode(&log.metadata().unwrap().permissions());
        assert_eq!(mode & 0o777, 0o600);
        assert!(scan_file(&log, &AtomicBool::new(false)).is_empty());
    }

    #[test]
    fn a_cut_off_private_key_is_removed_without_breaking_the_json() {
        let pem = format!("{}RSA PRIVATE KEY-----\\nMIIEow+/AB\\\"", "-----BEGIN ");
        let line = format!("{{\"k\":\"{pem} tail\"}}\n");
        let mut out = Vec::new();
        assert_eq!(redact_line(line.as_bytes(), &mut out), 1);
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("MIIE"));
        serde_json::from_str::<serde_json::Value>(text.trim()).unwrap();
    }

    #[test]
    fn removing_refuses_recent_and_outside_files() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let key = fake("hf_", "a1B2c3D4e5F6g7H8i9J0kLmNoPqRsTuVw");
        let log = transcript(&home, &format!("{key}\n"));
        assert!(matches!(
            remove_from_file(&home, &log, now_ms()),
            Err(RemoveError::InUse)
        ));
        let outside = home.join("notes.jsonl");
        std::fs::write(&outside, format!("{key}\n")).unwrap();
        assert!(matches!(
            remove_from_file(&home, &outside, LATER),
            Err(RemoveError::NotATranscript)
        ));
        let link = home.join(".claude/projects/-Users-x-app/link.jsonl");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(matches!(
            remove_from_file(&home, &link, LATER),
            Err(RemoveError::NotATranscript)
        ));
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            format!("{key}\n")
        );
    }
}
