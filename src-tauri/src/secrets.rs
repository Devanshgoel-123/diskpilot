//! Offline check for credentials pasted into AI coding agents' transcripts.
//!
//! Agents keep full conversation logs on disk, and anything pasted into a
//! chat or printed by a command (API keys, tokens, private keys) ends up in
//! them. This module reads those logs line by line and reports where
//! well-known credential formats appear.
//!
//! Nothing leaves the Mac, nothing is changed, and the credentials themselves
//! are never returned: findings carry only a masked form (a short prefix and
//! the last four characters) so the user can recognize which key it was.

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

/// Credentials in one line: (kind, masked form).
fn scan_line(line: &[u8]) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for p in PATTERNS {
        let pre = p.prefix.as_bytes();
        let mut from = 0;
        while let Some(off) = find(&line[from..], pre) {
            let start = from + off;
            from = start + pre.len();
            if start > 0 && token_char(line[start - 1]) {
                continue; // inside a longer word
            }
            if taken.iter().any(|&(s, e)| start >= s && start < e) {
                continue; // already matched by a more specific prefix
            }
            if p.kind == "Private key" {
                let rest = &line[from..line.len().min(from + 40)];
                if find(rest, b"PRIVATE KEY-----").is_some() {
                    taken.push((start, from));
                    out.push((p.kind, mask(p.kind, "", "")));
                }
                continue;
            }
            let len = line[from..].iter().take_while(|&&b| (p.body)(b)).count();
            if len < p.min_len || !looks_real(&line[from..from + len]) {
                continue;
            }
            let token = String::from_utf8_lossy(&line[from..from + len]);
            taken.push((start, from + len));
            out.push((p.kind, mask(p.kind, p.prefix, &token)));
        }
    }
    out
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
}
