//! API key storage in the macOS Keychain. The key is never returned to the UI.

const SERVICE: &str = "dev.diskpilot.app";
const ACCOUNT: &str = "anthropic-api-key";

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("That does not look like an Anthropic API key (it should start with sk-ant-).")]
    Invalid,
    #[error("Keychain error: {0}")]
    Keychain(String),
}

fn entry() -> Result<keyring::Entry, KeyError> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| KeyError::Keychain(e.to_string()))
}

pub fn validate(key: &str) -> Result<&str, KeyError> {
    let key = key.trim();
    if key.starts_with("sk-ant-") && key.len() > 20 && !key.contains(char::is_whitespace) {
        Ok(key)
    } else {
        Err(KeyError::Invalid)
    }
}

pub fn store(key: &str) -> Result<(), KeyError> {
    let key = validate(key)?;
    entry()?
        .set_password(key)
        .map_err(|e| KeyError::Keychain(e.to_string()))
}

pub fn clear() -> Result<(), KeyError> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(KeyError::Keychain(e.to_string())),
    }
}

/// The stored key, or `ANTHROPIC_API_KEY` from the environment.
pub fn load() -> Option<(String, &'static str)> {
    if let Ok(Ok(key)) = entry().map(|e| e.get_password()) {
        return Some((key, "keychain"));
    }
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .map(|k| (k, "environment"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_key_shape() {
        assert!(validate("sk-ant-api03-abcdefghijklmnop").is_ok());
        assert!(validate("  sk-ant-api03-abcdefghijklmnop \n").is_ok());
        assert!(validate("sk-proj-123").is_err());
        assert!(validate("sk-ant-short").is_err());
        assert!(validate("sk-ant-abc def ghijklmnopqrstu").is_err());
    }
}
