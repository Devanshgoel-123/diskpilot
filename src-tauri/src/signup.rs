//! The email signup DiskPilot asks for on first launch.
//!
//! The email, DiskPilot's version, the macOS version and the kind of Mac are
//! sent to diskpilot.live, so the maker can tell people about updates (many
//! early copies are out of date) and ask for feedback. Nothing about files,
//! folders or scans is sent. A copy is kept in the app's data folder so the
//! window does not ask again; if the Mac is offline it is sent on a later
//! launch.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const FILE: &str = "signup.json";
/// Where signups go. Only this host is ever contacted for them.
pub const ENDPOINT: &str = "https://diskpilot.live/api/signup";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signup {
    pub email: String,
    pub signed_up_at_ms: i64,
    /// Whether diskpilot.live has received it.
    pub sent: bool,
}

/// What is sent: the email and the versions, nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub email: String,
    pub app_version: String,
    pub os_version: String,
    pub arch: String,
}

/// A trimmed, lowercased address if it looks like an email, else why not.
pub fn normalize_email(raw: &str) -> Result<String, String> {
    let email = raw.trim().to_lowercase();
    let invalid = || Err("Enter a valid email address, like name@gmail.com.".to_string());
    if email.is_empty()
        || email.len() > 254
        || email.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return invalid();
    }
    let Some((local, domain)) = email.split_once('@') else {
        return invalid();
    };
    let domain_ok = domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && !domain.contains('@');
    if local.is_empty() || local.len() > 64 || !domain_ok {
        return invalid();
    }
    if let Some(why) = not_real(local, domain) {
        return Err(why);
    }
    Ok(email)
}

/// Domains that can't receive mail, or only for a few minutes.
const THROWAWAY: &[&str] = &[
    "example.com", "example.org", "example.net", "test.com", "xx.com", "xxx.com", "abc.com",
    "mailinator.com", "guerrillamail.com", "sharklasers.com", "10minutemail.com", "yopmail.com",
    "temp-mail.org", "tempmail.com", "trashmail.com", "maildrop.cc", "getnada.com",
    "dispostable.com", "dontsp.am", "throwawaymail.com", "fakeinbox.com", "mailnesia.com",
    "spamgourmet.com", "burnermail.io", "emailondeck.com", "mohmal.com",
];
/// Top-level domains reserved for examples and tests (RFC 2606).
const RESERVED_TLDS: &[&str] = &["test", "example", "invalid", "localhost", "local"];
/// Common slips of the big providers, with what was meant.
const TYPOS: &[(&str, &str)] = &[
    ("gmal.com", "gmail.com"), ("gmial.com", "gmail.com"), ("gmai.com", "gmail.com"),
    ("gamil.com", "gmail.com"), ("gnail.com", "gmail.com"), ("gmail.co", "gmail.com"),
    ("gmail.con", "gmail.com"), ("gmail.cm", "gmail.com"), ("hotmial.com", "hotmail.com"),
    ("hotmal.com", "hotmail.com"), ("yahooo.com", "yahoo.com"), ("yaho.com", "yahoo.com"),
    ("outlok.com", "outlook.com"), ("outlook.co", "outlook.com"), ("iclod.com", "icloud.com"),
    ("icloud.co", "icloud.com"),
];

/// Why an address that looks right still won't reach a person, if it won't.
fn not_real(local: &str, domain: &str) -> Option<String> {
    if let Some((_, meant)) = TYPOS.iter().find(|(typo, _)| *typo == domain) {
        return Some(format!("Did you mean {local}@{meant}?"));
    }
    let tld = domain.rsplit('.').next().unwrap_or("");
    if THROWAWAY.contains(&domain) || RESERVED_TLDS.contains(&tld) {
        return Some("Use an email you read. Fixes and new versions are sent there.".to_string());
    }
    let base = local.split('+').next().unwrap_or(local);
    let placeholder = ["test", "tester", "asdf", "qwerty", "fake", "none", "null", "noreply",
                       "no-reply", "nobody", "abc", "user", "name", "email", "mail"]
        .contains(&base.trim_end_matches(|c: char| c.is_ascii_digit()))
        || base.starts_with("tester")
        || (base.len() >= 2 && base.chars().all(|c| c == base.chars().next().unwrap_or(' ')));
    placeholder.then(|| "Use an email you read. Fixes and new versions are sent there.".to_string())
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

pub fn load(dir: &Path) -> Option<Signup> {
    let bytes = fs::read(path(dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Saved readable only by this user, like the saved scan.
pub fn save(dir: &Path, signup: &Signup) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(signup).unwrap_or_default())?;
        f.sync_all()?;
    }
    fs::rename(tmp, path(dir))
}

/// macOS's version, such as "15.1", from the kernel.
pub fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "kern.osproductversion"])
            .output()
        {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
    }
    "unknown".to_string()
}

pub fn payload(email: &str) -> Payload {
    Payload {
        email: email.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: os_version(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

/// Sends the signup to diskpilot.live. Ok once the server has stored it.
pub fn send(p: &Payload) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .user_agent(concat!("DiskPilot/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .post(ENDPOINT)
        .json(p)
        .send()
        .map_err(|e| e.to_string())?;
    if res.status().is_success() {
        Ok(())
    } else {
        Err(format!("diskpilot.live answered {}", res.status()))
    }
}

/// Records the signup locally, then tries to send it. The window can carry
/// on either way: an unsent signup is sent again on a later launch.
pub fn submit(
    dir: &Path,
    raw_email: &str,
    now_ms: i64,
    send: impl Fn(&Payload) -> Result<(), String>,
) -> Result<Signup, String> {
    let email = normalize_email(raw_email)?;
    let mut signup = Signup {
        email,
        signed_up_at_ms: now_ms,
        sent: false,
    };
    save(dir, &signup).map_err(|e| format!("Could not save the signup: {e}"))?;
    if send(&payload(&signup.email)).is_ok() {
        signup.sent = true;
        let _ = save(dir, &signup);
    }
    Ok(signup)
}

/// Sends a signup that was saved while offline. Returns whether one was sent.
pub fn send_pending(dir: &Path, send: impl Fn(&Payload) -> Result<(), String>) -> bool {
    let Some(mut signup) = load(dir) else {
        return false;
    };
    if signup.sent || send(&payload(&signup.email)).is_err() {
        return false;
    }
    signup.sent = true;
    save(dir, &signup).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn accepts_and_cleans_ordinary_addresses() {
        assert_eq!(
            normalize_email("  Riya.Sharma@Proton.ME ").unwrap(),
            "riya.sharma@proton.me"
        );
        assert!(normalize_email("a.b+tag@mail.co.uk").is_ok());
    }

    #[test]
    fn rejects_throwaway_and_placeholder_addresses() {
        for bad in [
            "xxx@xx.com",
            "tester49xyz@icloud.com",
            "jeppe@dontsp.am",
            "name@example.com",
            "a@mail.test",
            "test123@gmail.com",
            "aaaa@gmail.com",
            "asdf@yahoo.com",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad} should be refused");
        }
        assert!(normalize_email("lucasjuan14+diskpilot@gmail.com").is_ok());
        assert!(normalize_email("michel.sirois@cgocable.ca").is_ok());
    }

    #[test]
    fn suggests_the_provider_for_a_typo() {
        assert_eq!(
            normalize_email("theerasak.spark@gmal.com").unwrap_err(),
            "Did you mean theerasak.spark@gmail.com?"
        );
    }

    #[test]
    fn rejects_things_that_are_not_emails() {
        for bad in [
            "",
            "name",
            "name@",
            "@example.com",
            "name@example",
            "na me@example.com",
            "a@b..com",
            "a@.com",
            "a@b.com.",
            "a@b@c.com",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn sends_only_the_email_and_versions() {
        let v = serde_json::to_value(payload("a@b.com")).unwrap();
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["appVersion", "arch", "email", "osVersion"]);
    }

    #[test]
    fn an_offline_signup_is_kept_and_sent_later() {
        let dir = crate::test_support::sandbox();
        let s = submit(dir.path(), "me@proton.me", 1, |_| Err("offline".into())).unwrap();
        assert!(!s.sent);
        assert_eq!(load(dir.path()).unwrap().email, "me@proton.me");

        let sent = RefCell::new(Vec::new());
        assert!(send_pending(dir.path(), |p| {
            sent.borrow_mut().push(p.email.clone());
            Ok(())
        }));
        assert_eq!(sent.into_inner(), ["me@proton.me"]);
        assert!(load(dir.path()).unwrap().sent);
        // Nothing is sent twice.
        assert!(!send_pending(dir.path(), |_| panic!("sent again")));
    }

    #[test]
    fn an_invalid_email_saves_nothing() {
        let dir = crate::test_support::sandbox();
        assert!(submit(dir.path(), "nope", 1, |_| Ok(())).is_err());
        assert!(load(dir.path()).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_signup_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::test_support::sandbox();
        submit(dir.path(), "me@proton.me", 1, |_| Ok(())).unwrap();
        let mode = fs::metadata(dir.path().join(FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
