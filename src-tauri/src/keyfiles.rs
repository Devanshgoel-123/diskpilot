//! Keys & secrets: private keys, certificate bundles, `.env` files and cloud
//! credentials found on this Mac, with what makes each one risky, and labels
//! the person adds (which provider a key is for) so they can find it later.
//!
//! How it is kept safe:
//! - Files are found by **name only**, during the regular scan.
//! - To tell a private key from a certificate, at most the first
//!   [`HEADER_BYTES`] bytes are read. Nothing read is stored, logged, shown or
//!   sent anywhere; only the conclusion ("private key") is kept.
//! - The list and the labels are saved **encrypted** (AES-256-GCM) in
//!   DiskPilot's data folder, with the key in the macOS Keychain, so the
//!   saved file alone does not reveal where keys are.
//! - Nothing here is a recommendation, so key files never appear in the
//!   Cleanup list or in anything the Assistant sees.
//! - There is no export. Actions are: show in Finder, copy the path, make the
//!   file readable only by you, and move it to the Trash.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Most of a file ever read: enough for a PEM header, nothing more.
pub const HEADER_BYTES: u64 = 512;
/// Files at most this big are compared to find copies of the same key.
const HASH_LIMIT: u64 = 1024 * 1024;
const OLD_DAYS: i64 = 730;
const DAY_MS: i64 = 86_400_000;
const STORE_FILE: &str = "keyfiles.bin";
const KEYCHAIN_SERVICE: &str = "dev.diskpilot.app";
const KEYCHAIN_ACCOUNT: &str = "keys-index";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    SshKey,
    PrivateKey,
    /// A `.p12`/`.pfx`/keystore: a certificate together with its private key.
    KeyBundle,
    /// A public certificate. Not secret, listed so people can label it.
    Certificate,
    EnvFile,
    CloudCredentials,
    PasswordDatabase,
}

impl KeyKind {
    pub fn label(self) -> &'static str {
        match self {
            KeyKind::SshKey => "SSH key",
            KeyKind::PrivateKey => "Private key",
            KeyKind::KeyBundle => "Certificate with key",
            KeyKind::Certificate => "Certificate",
            KeyKind::EnvFile => "Environment file",
            KeyKind::CloudCredentials => "Cloud credentials",
            KeyKind::PasswordDatabase => "Password database",
        }
    }

    /// Whether the file holds a secret (a certificate on its own does not).
    pub fn secret(self) -> bool {
        self != KeyKind::Certificate
    }
}

/// A file the scan found by its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub path: PathBuf,
    pub kind: KeyKind,
}

// ---- Finding by name (runs inside the scan, so it must be cheap) ----------

/// Folders whose files are never anyone's own keys: dependencies, caches and
/// tool data, where test fixtures and bundled certificates live.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "vendor",
    "site-packages",
    "dist-packages",
    "Pods",
    "Carthage",
    ".cargo",
    ".rustup",
    ".npm",
    ".pnpm-store",
    ".yarn",
    ".gradle",
    ".m2",
    "DerivedData",
    ".cache",
    ".Trash",
    "Library",
    "Applications",
    "extensions",
    ".local",
    ".venv",
    "venv",
    "target",
    "build",
    "dist",
    ".next",
    // Language and SDK installs ship test certificates.
    ".pyenv",
    ".nvm",
    ".rbenv",
    ".asdf",
    ".sdkman",
    ".gem",
    ".bun",
    ".deno",
    ".conda",
    "anaconda3",
    "miniconda3",
    ".pub-cache",
    // Test fixtures are made-up keys.
    "testdata",
    "fixtures",
    "__fixtures__",
];

/// Whether files directly in `dir` should be looked at. Called once per
/// folder. Only folders inside `home` are considered.
pub fn dir_wanted(dir: &Path, home: &Path) -> bool {
    let Ok(rel) = dir.strip_prefix(home) else {
        return false;
    };
    // `go/pkg/mod` holds downloaded modules.
    let mut prev: Option<&OsStr> = None;
    for c in rel.components() {
        let Component::Normal(name) = c else { continue };
        if SKIP_DIRS.iter().any(|s| name == OsStr::new(s)) {
            return false;
        }
        if name == "mod" && prev == Some(OsStr::new("pkg")) {
            return false;
        }
        prev = Some(name);
    }
    true
}

const SSH_NOT_KEYS: &[&str] = &[
    "known_hosts",
    "known_hosts.old",
    "config",
    "authorized_keys",
    "authorized_keys2",
    "environment",
    "rc",
];

/// What a file is, judging by its name and its folder's name.
pub fn classify(name: &OsStr, parent: Option<&OsStr>) -> Option<KeyKind> {
    let name = name.to_str()?;
    let parent = parent.and_then(OsStr::to_str).unwrap_or("");
    if !maybe_key(name, parent) {
        return None;
    }
    let lower = name.to_ascii_lowercase();

    if parent == ".ssh" {
        if lower.ends_with(".pub")
            || SSH_NOT_KEYS
                .iter()
                .any(|n| lower == *n || lower.starts_with(&format!("{n}.")))
            || lower.ends_with(".sock")
            || lower.starts_with('.')
        {
            return None;
        }
        return Some(KeyKind::SshKey);
    }
    if lower.starts_with("id_")
        && ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"]
            .iter()
            .any(|k| lower.starts_with(k))
        && !lower.ends_with(".pub")
    {
        return Some(KeyKind::SshKey);
    }
    if lower == ".env" || (lower.starts_with(".env.") && !is_env_template(&lower)) {
        return Some(KeyKind::EnvFile);
    }
    match (parent, lower.as_str()) {
        (".aws", "credentials") | (".kube", "config") => return Some(KeyKind::CloudCredentials),
        (_, ".git-credentials" | ".netrc" | ".pgpass" | "application_default_credentials.json") => {
            return Some(KeyKind::CloudCredentials)
        }
        _ => {}
    }
    if lower.ends_with(".json")
        && (lower.starts_with("client_secret")
            || lower.contains("service-account")
            || lower.contains("service_account")
            || lower == "credentials.json")
    {
        return Some(KeyKind::CloudCredentials);
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e)?;
    match ext {
        "pem" | "key" | "p8" => Some(KeyKind::PrivateKey),
        "ppk" => Some(KeyKind::SshKey),
        "p12" | "pfx" | "jks" | "keystore" | "bks" => Some(KeyKind::KeyBundle),
        "kdbx" => Some(KeyKind::PasswordDatabase),
        _ => None,
    }
}

/// A quick test without allocating, so the scan pays almost nothing for the
/// vast majority of files that cannot be keys.
fn maybe_key(name: &str, parent: &str) -> bool {
    if matches!(parent, ".ssh" | ".aws" | ".kube") {
        return true;
    }
    let b = name.as_bytes();
    if b.first() == Some(&b'.') || (b.len() > 3 && b[..3].eq_ignore_ascii_case(b"id_")) {
        return true;
    }
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    [
        "pem", "key", "p8", "ppk", "p12", "pfx", "jks", "keystore", "bks", "kdbx", "json",
    ]
    .iter()
    .any(|e| ext.eq_ignore_ascii_case(e))
}

fn is_env_template(lower: &str) -> bool {
    [
        "example", "sample", "template", "dist", "defaults", "schema",
    ]
    .iter()
    .any(|w| lower.ends_with(w))
}

/// Confirm a `.pem`/`.key` by its header. Returns the refined kind, or None
/// when the file is not a key or certificate at all (a `.key` can be many
/// things). Reads at most [`HEADER_BYTES`]; nothing read is kept.
pub fn confirm(path: &Path, kind: KeyKind) -> Option<KeyKind> {
    if kind != KeyKind::PrivateKey {
        return Some(kind);
    }
    let mut buf = Vec::with_capacity(HEADER_BYTES as usize);
    std::fs::File::open(path)
        .ok()?
        .take(HEADER_BYTES)
        .read_to_end(&mut buf)
        .ok()?;
    let has = |needle: &[u8]| memchr::memmem::find(&buf, needle).is_some();
    let ext_p8 = path.extension().is_some_and(|e| e == "p8");
    let found = if has(b"PRIVATE KEY-----") {
        Some(KeyKind::PrivateKey)
    } else if has(b"-----BEGIN CERTIFICATE") {
        Some(KeyKind::Certificate)
    } else if ext_p8 && has(b"-----BEGIN") {
        Some(KeyKind::PrivateKey)
    } else {
        None
    };
    buf.fill(0);
    found
}

// ---- Report --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub provider: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskKind {
    OthersCanRead,
    EasyToShare,
    InGitProject,
    Copies,
    Old,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk {
    pub kind: RiskKind,
    pub text: String,
    /// Worth fixing (shown in amber), as opposed to just good to know.
    pub serious: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyFile {
    pub path: String,
    pub name: String,
    pub kind: KeyKind,
    pub kind_label: String,
    pub bytes: u64,
    pub modified_ms: Option<i64>,
    pub label: Option<Label>,
    pub risks: Vec<Risk>,
    /// Other users can read it and DiskPilot can make it private.
    pub can_fix_permissions: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysReport {
    pub generated_at_ms: i64,
    pub files: Vec<KeyFile>,
    /// Whether any scan has looked for keys yet.
    pub scanned: bool,
    pub scanned_at_ms: Option<i64>,
}

fn in_git_project(path: &Path, home: &Path) -> Option<PathBuf> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == home || !d.starts_with(home) {
            return None;
        }
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Whether git would ignore the file, or None when git cannot be asked
/// without prompting to install developer tools.
fn git_ignored(repo: &Path, path: &Path) -> Option<bool> {
    let tools = Path::new("/Library/Developer/CommandLineTools/usr/bin/git").exists()
        || Path::new("/Applications/Xcode.app").exists();
    if !tools {
        return None;
    }
    let mut cmd = std::process::Command::new("/usr/bin/git");
    cmd.arg("-C")
        .arg(repo)
        .args(["check-ignore", "-q", "--"])
        .arg(path);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.code() == Some(0)),
            Ok(None) if start.elapsed() > std::time::Duration::from_secs(2) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
}

fn digest(path: &Path, bytes: u64) -> Option<[u8; 32]> {
    use sha2::Digest;
    if bytes == 0 || bytes > HASH_LIMIT {
        return None;
    }
    let mut data = std::fs::read(path).ok()?;
    let out: [u8; 32] = sha2::Sha256::digest(&data).into();
    data.fill(0);
    Some(out)
}

/// Build the report for the files in the store: drop files that are gone,
/// confirm kinds, and work out what makes each one risky.
pub fn report(
    found: &[Found],
    labels: &BTreeMap<String, Label>,
    home: &Path,
    now_ms: i64,
    scanned_at_ms: Option<i64>,
) -> (KeysReport, Vec<Found>) {
    let uid = unsafe { libc::getuid() };
    struct Item {
        found: Found,
        meta: std::fs::Metadata,
        hash: Option<[u8; 32]>,
    }
    let items: Vec<Item> = found
        .iter()
        .filter_map(|f| {
            // Never follow links: a link is not where the key is.
            let meta = f.path.symlink_metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let kind = confirm(&f.path, f.kind)?;
            Some(Item {
                hash: digest(&f.path, meta.len()),
                found: Found {
                    path: f.path.clone(),
                    kind,
                },
                meta,
            })
        })
        .collect();
    let mut copies: HashMap<[u8; 32], usize> = HashMap::new();
    for i in &items {
        if let Some(h) = i.hash {
            *copies.entry(h).or_default() += 1;
        }
    }
    let share_dirs = ["Downloads", "Desktop", "Documents"].map(|d| home.join(d));

    let mut files = Vec::with_capacity(items.len());
    for i in &items {
        let path = &i.found.path;
        let kind = i.found.kind;
        let mode = i.meta.permissions().mode();
        let mut risks = Vec::new();
        let others_read = mode & 0o044 != 0;
        if kind.secret() && others_read {
            risks.push(Risk {
                kind: RiskKind::OthersCanRead,
                text: "Other accounts on this Mac can read it.".into(),
                serious: true,
            });
        }
        if let Some(dir) = share_dirs.iter().find(|d| path.starts_with(d)) {
            if kind.secret() {
                let place = dir.file_name().and_then(OsStr::to_str).unwrap_or("");
                risks.push(Risk {
                    kind: RiskKind::EasyToShare,
                    text: match place {
                        "Downloads" => {
                            "In Downloads, where files get shared and uploaded by mistake.".into()
                        }
                        _ => format!("On {place}, which iCloud can sync to your other devices."),
                    },
                    serious: true,
                });
            }
        }
        if kind.secret() {
            if let Some(repo) = in_git_project(path, home) {
                match git_ignored(&repo, path) {
                    Some(true) => {}
                    Some(false) => risks.push(Risk {
                        kind: RiskKind::InGitProject,
                        text: "In a git project and not in .gitignore, so it could be committed."
                            .into(),
                        serious: true,
                    }),
                    None => risks.push(Risk {
                        kind: RiskKind::InGitProject,
                        text: "In a git project. Check that .gitignore covers it.".into(),
                        serious: false,
                    }),
                }
            }
        }
        if let Some(n) = i.hash.and_then(|h| copies.get(&h)).filter(|n| **n > 1) {
            risks.push(Risk {
                kind: RiskKind::Copies,
                text: format!("{n} identical copies on this Mac."),
                serious: false,
            });
        }
        let modified_ms = i.meta.modified().ok().and_then(crate::scanner::to_ms);
        if let Some(ms) = modified_ms {
            let days = (now_ms - ms) / DAY_MS;
            if days >= OLD_DAYS {
                risks.push(Risk {
                    kind: RiskKind::Old,
                    text: format!(
                        "Not changed in {} years; it may be expired or no longer used.",
                        days / 365
                    ),
                    serious: false,
                });
            }
        }
        let key = path.display().to_string();
        files.push(KeyFile {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            kind,
            kind_label: kind.label().into(),
            bytes: i.meta.len(),
            modified_ms,
            label: labels.get(&key).cloned(),
            can_fix_permissions: kind.secret() && others_read && i.meta.uid() == uid,
            risks,
            path: key,
        });
    }
    // Most serious first, then by name.
    files.sort_by(|a, b| {
        let s = |f: &KeyFile| f.risks.iter().filter(|r| r.serious).count();
        s(b).cmp(&s(a)).then_with(|| a.name.cmp(&b.name))
    });
    let kept = items.into_iter().map(|i| i.found).collect();
    (
        KeysReport {
            generated_at_ms: now_ms,
            files,
            scanned: scanned_at_ms.is_some(),
            scanned_at_ms,
        },
        kept,
    )
}

// ---- Encrypted store ---------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub version: u32,
    /// The scan the list comes from.
    pub scan_id: Option<String>,
    pub scanned_at_ms: Option<i64>,
    pub files: Vec<Found>,
    pub labels: BTreeMap<String, Label>,
}

/// Where the encryption key comes from: the Keychain in the app, a fixed
/// key in tests.
pub trait KeySource {
    fn key(&self, create: bool) -> Result<Option<[u8; 32]>, AppError>;
}

pub struct Keychain;

impl KeySource for Keychain {
    fn key(&self, create: bool) -> Result<Option<[u8; 32]>, AppError> {
        use base64::Engine;
        let keychain = |e: keyring::Error| AppError::Other(format!("Keychain error: {e}"));
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(keychain)?;
        match entry.get_password() {
            Ok(text) => {
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(text.trim())
                    .map_err(|_| AppError::Other("The key in the Keychain is damaged.".into()))?;
                let key: [u8; 32] = raw
                    .try_into()
                    .map_err(|_| AppError::Other("The key in the Keychain is damaged.".into()))?;
                Ok(Some(key))
            }
            Err(keyring::Error::NoEntry) if create => {
                use aes_gcm::aead::rand_core::RngCore;
                let mut key = [0u8; 32];
                aes_gcm::aead::OsRng.fill_bytes(&mut key);
                entry
                    .set_password(&base64::engine::general_purpose::STANDARD.encode(key))
                    .map_err(keychain)?;
                Ok(Some(key))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain(e)),
        }
    }
}

fn cipher(key: &[u8; 32]) -> aes_gcm::Aes256Gcm {
    use aes_gcm::KeyInit;
    aes_gcm::Aes256Gcm::new(key.into())
}

pub fn load(data_dir: &Path, keys: &dyn KeySource) -> Result<Store, AppError> {
    use aes_gcm::aead::Aead;
    let path = data_dir.join(STORE_FILE);
    let Ok(blob) = std::fs::read(&path) else {
        return Ok(Store::default());
    };
    let Some(key) = keys.key(false)? else {
        // The key is gone (Keychain reset): the old list cannot be read.
        return Ok(Store::default());
    };
    if blob.len() < 12 {
        return Ok(Store::default());
    }
    let (nonce, data) = blob.split_at(12);
    let plain = cipher(&key)
        .decrypt(nonce.into(), data)
        .map_err(|_| AppError::Other("The saved list of keys could not be read.".into()))?;
    let store = serde_json::from_slice(&plain).unwrap_or_default();
    Ok(store)
}

pub fn save(data_dir: &Path, keys: &dyn KeySource, store: &Store) -> Result<(), AppError> {
    use aes_gcm::aead::{Aead, AeadCore};
    let key = keys
        .key(true)?
        .ok_or_else(|| AppError::Other("No encryption key.".into()))?;
    let mut plain = serde_json::to_vec(store).map_err(|e| AppError::Other(e.to_string()))?;
    let nonce = aes_gcm::Aes256Gcm::generate_nonce(&mut aes_gcm::aead::OsRng);
    let data = cipher(&key)
        .encrypt(&nonce, plain.as_slice())
        .map_err(|_| AppError::Other("Could not encrypt the list of keys.".into()))?;
    plain.fill(0);
    let mut blob = nonce.to_vec();
    blob.extend(data);
    std::fs::create_dir_all(data_dir).map_err(|e| AppError::io(data_dir, &e))?;
    let path = data_dir.join(STORE_FILE);
    let tmp = data_dir.join(format!(".{STORE_FILE}.tmp"));
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| AppError::io(&tmp, &e))?;
        f.write_all(&blob).map_err(|e| AppError::io(&tmp, &e))?;
        f.sync_all().map_err(|e| AppError::io(&tmp, &e))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| AppError::io(&path, &e))
}

/// Bring the store up to date with a newer scan: the scan's list replaces
/// the old one, labels are kept for files that are still found.
pub fn merge_scan(store: &mut Store, scan_id: &str, scanned_at_ms: i64, found: Vec<Found>) -> bool {
    // A scan's early results and its final results share an id.
    if store.scan_id.as_deref() == Some(scan_id) && store.scanned_at_ms == Some(scanned_at_ms) {
        return false;
    }
    store.scan_id = Some(scan_id.to_string());
    store.scanned_at_ms = Some(scanned_at_ms);
    store.files = found;
    store.version = 1;
    true
}

/// Keep labels only for files in the list.
pub fn prune_labels(store: &mut Store) {
    let present: std::collections::HashSet<String> = store
        .files
        .iter()
        .map(|f| f.path.display().to_string())
        .collect();
    store.labels.retain(|k, _| present.contains(k));
}

// ---- Changes -----------------------------------------------------------------

/// The file must be one DiskPilot found; paths from the UI are only used to
/// look it up.
pub fn listed<'a>(store: &'a Store, path: &str) -> Result<&'a Found, AppError> {
    store
        .files
        .iter()
        .find(|f| f.path.as_os_str() == OsStr::new(path))
        .ok_or_else(|| AppError::Other("That file is not in the list of keys.".into()))
}

pub const PROVIDERS: &[&str] = &[
    "AWS",
    "Google Cloud",
    "Azure",
    "GitHub",
    "GitLab",
    "Apple",
    "Firebase",
    "Cloudflare",
    "Vercel",
    "Stripe",
    "OpenAI",
    "Anthropic",
    "DigitalOcean",
    "SSH server",
    "Other",
];

pub fn set_label(store: &mut Store, path: &str, label: Option<Label>) -> Result<(), AppError> {
    listed(store, path)?;
    match label {
        Some(mut l) => {
            l.provider = l.provider.trim().chars().take(40).collect();
            l.note = l.note.trim().chars().take(120).collect();
            if l.provider.is_empty() {
                store.labels.remove(path);
            } else {
                store.labels.insert(path.to_string(), l);
            }
        }
        None => {
            store.labels.remove(path);
        }
    }
    Ok(())
}

/// Make a key readable only by its owner (mode 600). Only regular files the
/// user owns, never through a link.
pub fn make_private(store: &Store, path: &str) -> Result<(), AppError> {
    let f = listed(store, path)?;
    let meta = f
        .path
        .symlink_metadata()
        .map_err(|e| AppError::io(&f.path, &e))?;
    if !meta.is_file() || meta.uid() != unsafe { libc::getuid() } {
        return Err(AppError::Other(
            "Only your own files can be changed.".into(),
        ));
    }
    std::fs::set_permissions(&f.path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| AppError::io(&f.path, &e))
}

/// Move a key file to the Trash, through the same safety gate as cleanups.
pub fn move_to_trash(store: &mut Store, path: &str, home: &Path) -> Result<(), AppError> {
    listed(store, path)?;
    let checked = crate::safety::authorize_destructive(path, home, home)?;
    trash::delete(&checked)
        .map_err(|e| AppError::Other(format!("Could not move it to the Trash: {e}")))?;
    store
        .files
        .retain(|f| f.path.as_os_str() != OsStr::new(path));
    store.labels.remove(path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestKey([u8; 32]);
    impl KeySource for TestKey {
        fn key(&self, _create: bool) -> Result<Option<[u8; 32]>, AppError> {
            Ok(Some(self.0))
        }
    }

    fn c(name: &str, parent: &str) -> Option<KeyKind> {
        classify(OsStr::new(name), Some(OsStr::new(parent)))
    }

    #[test]
    fn finds_keys_by_name_and_skips_lookalikes() {
        assert_eq!(c("id_ed25519", "x"), Some(KeyKind::SshKey));
        assert_eq!(c("id_ed25519.pub", "x"), None);
        assert_eq!(c("github", ".ssh"), Some(KeyKind::SshKey));
        assert_eq!(c("known_hosts", ".ssh"), None);
        assert_eq!(c("config", ".ssh"), None);
        assert_eq!(c("work.pub", ".ssh"), None);
        assert_eq!(c("prod.pem", "Downloads"), Some(KeyKind::PrivateKey));
        assert_eq!(
            c("AuthKey_ABC123.p8", "Downloads"),
            Some(KeyKind::PrivateKey)
        );
        assert_eq!(c("dist.p12", "x"), Some(KeyKind::KeyBundle));
        assert_eq!(c(".env", "app"), Some(KeyKind::EnvFile));
        assert_eq!(c(".env.production", "app"), Some(KeyKind::EnvFile));
        assert_eq!(c(".env.example", "app"), None);
        assert_eq!(c("credentials", ".aws"), Some(KeyKind::CloudCredentials));
        assert_eq!(c("credentials", "notes"), None);
        assert_eq!(c("config", ".kube"), Some(KeyKind::CloudCredentials));
        assert_eq!(
            c("my-service-account.json", "x"),
            Some(KeyKind::CloudCredentials)
        );
        assert_eq!(
            c("client_secret_123.json", "x"),
            Some(KeyKind::CloudCredentials)
        );
        assert_eq!(c("vault.kdbx", "x"), Some(KeyKind::PasswordDatabase));
        assert_eq!(c("photo.jpg", "x"), None);
        assert_eq!(c("package.json", "x"), None);
    }

    #[test]
    fn dependency_and_library_folders_are_skipped() {
        let home = Path::new("/Users/me");
        assert!(dir_wanted(Path::new("/Users/me/.ssh"), home));
        assert!(dir_wanted(Path::new("/Users/me/code/app"), home));
        assert!(!dir_wanted(
            Path::new("/Users/me/code/app/node_modules/x/test"),
            home
        ));
        assert!(!dir_wanted(Path::new("/Users/me/Library/Keychains"), home));
        assert!(!dir_wanted(
            Path::new("/Users/me/go/pkg/mod/github.com/x"),
            home
        ));
        assert!(!dir_wanted(Path::new("/opt/x"), home));
        assert!(!dir_wanted(
            Path::new("/Users/me/.pyenv/versions/3.12/lib/test"),
            home
        ));
        assert!(!dir_wanted(Path::new("/Users/me/code/app/testdata"), home));
    }

    #[test]
    fn a_pem_is_confirmed_by_its_header_only() {
        let dir = crate::test_support::sandbox();
        let key = dir.path().join("a.pem");
        std::fs::write(&key, "-----BEGIN RSA PRIVATE KEY-----\nMIIE...\n").unwrap();
        let cert = dir.path().join("b.pem");
        std::fs::write(&cert, "-----BEGIN CERTIFICATE-----\nMIID...\n").unwrap();
        let other = dir.path().join("c.key");
        std::fs::write(&other, "not a key at all").unwrap();
        // A header past the first 512 bytes is not looked for.
        let late = dir.path().join("d.pem");
        std::fs::write(
            &late,
            format!("{}-----BEGIN PRIVATE KEY-----", "x".repeat(600)),
        )
        .unwrap();
        assert_eq!(
            confirm(&key, KeyKind::PrivateKey),
            Some(KeyKind::PrivateKey)
        );
        assert_eq!(
            confirm(&cert, KeyKind::PrivateKey),
            Some(KeyKind::Certificate)
        );
        assert_eq!(confirm(&other, KeyKind::PrivateKey), None);
        assert_eq!(confirm(&late, KeyKind::PrivateKey), None);
    }

    #[test]
    fn report_flags_readable_shared_copied_and_never_includes_contents() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let secret = "-----BEGIN OPENSSH PRIVATE KEY-----\nSUPERSECRETMATERIAL\n";
        let dl = home.join("Downloads/prod.pem");
        let ssh = home.join(".ssh/id_ed25519");
        std::fs::create_dir_all(dl.parent().unwrap()).unwrap();
        std::fs::create_dir_all(ssh.parent().unwrap()).unwrap();
        std::fs::write(&dl, secret).unwrap();
        std::fs::write(&ssh, secret).unwrap();
        std::fs::set_permissions(&dl, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o600)).unwrap();
        let found = vec![
            Found {
                path: dl.clone(),
                kind: KeyKind::PrivateKey,
            },
            Found {
                path: ssh.clone(),
                kind: KeyKind::SshKey,
            },
            Found {
                path: home.join("gone.pem"),
                kind: KeyKind::PrivateKey,
            },
        ];
        let (r, kept) = report(&found, &BTreeMap::new(), &home, 1_800_000_000_000, Some(1));
        assert_eq!(kept.len(), 2, "missing files are dropped");
        let first = &r.files[0];
        assert_eq!(first.name, "prod.pem", "riskiest first");
        let kinds: Vec<RiskKind> = first.risks.iter().map(|r| r.kind).collect();
        assert!(kinds.contains(&RiskKind::OthersCanRead));
        assert!(kinds.contains(&RiskKind::EasyToShare));
        assert!(kinds.contains(&RiskKind::Copies));
        assert!(first.can_fix_permissions);
        let ssh_row = r.files.iter().find(|f| f.name == "id_ed25519").unwrap();
        assert!(!ssh_row
            .risks
            .iter()
            .any(|r| r.kind == RiskKind::OthersCanRead));
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("SUPERSECRET"));
    }

    #[test]
    fn the_store_is_encrypted_and_only_its_key_opens_it() {
        let dir = crate::test_support::sandbox();
        let mut store = Store::default();
        merge_scan(
            &mut store,
            "scan-1",
            5,
            vec![Found {
                path: "/Users/me/Downloads/prod.pem".into(),
                kind: KeyKind::PrivateKey,
            }],
        );
        set_label(
            &mut store,
            "/Users/me/Downloads/prod.pem",
            Some(Label {
                provider: "AWS".into(),
                note: "production".into(),
            }),
        )
        .unwrap();
        save(dir.path(), &TestKey([7; 32]), &store).unwrap();

        let raw = std::fs::read(dir.path().join(STORE_FILE)).unwrap();
        assert!(
            memchr::memmem::find(&raw, b"prod.pem").is_none(),
            "no path in plain text"
        );
        assert!(
            memchr::memmem::find(&raw, b"AWS").is_none(),
            "no label in plain text"
        );
        let mode = std::fs::metadata(dir.path().join(STORE_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        assert_eq!(load(dir.path(), &TestKey([7; 32])).unwrap(), store);
        assert!(
            load(dir.path(), &TestKey([8; 32])).is_err(),
            "a different key cannot read it"
        );
        assert!(
            set_label(&mut store, "/etc/passwd", None).is_err(),
            "only listed files"
        );
        assert!(
            !merge_scan(&mut store, "scan-1", 5, vec![]),
            "same scan changes nothing"
        );
    }

    #[test]
    fn permissions_are_fixed_only_for_listed_files() {
        let dir = crate::test_support::sandbox();
        let f = dir.path().join("k.pem");
        std::fs::write(&f, "x").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        let store = Store {
            files: vec![Found {
                path: f.clone(),
                kind: KeyKind::PrivateKey,
            }],
            ..Default::default()
        };
        make_private(&store, &f.display().to_string()).unwrap();
        assert_eq!(
            std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(make_private(&store, "/etc/hosts").is_err());
    }
}
