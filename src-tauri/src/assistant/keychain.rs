//! API key storage in the macOS Keychain, one entry per provider. Keys are
//! never returned to the UI.

use super::provider::Provider;

const SERVICE: &str = "dev.diskpilot.app";

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("That does not look like an {label} API key (it should start with {prefix}).", label = .0.label(), prefix = .0.key_prefix())]
    Invalid(Provider),
    #[error("That looks like an {found} key. Choose {found} as the provider to use it.", found = .0.label())]
    WrongProvider(Provider),
    #[error("Keychain error: {0}")]
    Keychain(String),
}

fn entry(provider: Provider) -> Result<keyring::Entry, KeyError> {
    keyring::Entry::new(SERVICE, provider.keychain_account())
        .map_err(|e| KeyError::Keychain(e.to_string()))
}

/// The provider a key was evidently issued by, judged by its prefix.
fn issued_by(key: &str) -> Option<Provider> {
    if key.starts_with("sk-ant-") {
        Some(Provider::Anthropic)
    } else if key.starts_with("sk-or-") {
        Some(Provider::Openrouter)
    } else {
        None
    }
}

pub fn validate(provider: Provider, key: &str) -> Result<&str, KeyError> {
    let key = key.trim();
    if let Some(other) = issued_by(key).filter(|p| *p != provider) {
        return Err(KeyError::WrongProvider(other));
    }
    if key.starts_with(provider.key_prefix())
        && key.len() > 20
        && !key.contains(char::is_whitespace)
    {
        Ok(key)
    } else {
        Err(KeyError::Invalid(provider))
    }
}

pub fn store(provider: Provider, key: &str) -> Result<(), KeyError> {
    let key = validate(provider, key)?;
    entry(provider)?
        .set_password(key)
        .map_err(|e| KeyError::Keychain(e.to_string()))
}

pub fn clear(provider: Provider) -> Result<(), KeyError> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(KeyError::Keychain(e.to_string())),
    }
}

/// The stored key, or the provider's environment variable.
pub fn load(provider: Provider) -> Option<(String, &'static str)> {
    if let Ok(Ok(key)) = entry(provider).map(|e| e.get_password()) {
        return Some((key, "keychain"));
    }
    std::env::var(provider.env_var())
        .ok()
        .filter(|k| !k.is_empty())
        .map(|k| (k, "environment"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_anthropic_keys() {
        let p = Provider::Anthropic;
        assert!(validate(p, "sk-ant-api03-abcdefghijklmnop").is_ok());
        assert!(validate(p, "  sk-ant-api03-abcdefghijklmnop \n").is_ok());
        assert!(validate(p, "sk-proj-1234567890abcdefghij").is_err());
        assert!(validate(p, "sk-ant-short").is_err());
        assert!(validate(p, "sk-ant-abc def ghijklmnopqrstu").is_err());
    }

    #[test]
    fn validates_openai_keys() {
        let p = Provider::Openai;
        assert!(validate(p, "sk-proj-1234567890abcdefghij").is_ok());
        assert!(validate(p, "sk-1234567890abcdefghijklmn").is_ok());
        assert!(validate(p, "pk-1234567890abcdefghijklmn").is_err());
    }

    #[test]
    fn validates_openrouter_keys() {
        let p = Provider::Openrouter;
        assert!(validate(p, "sk-or-v1-1234567890abcdefghij").is_ok());
        assert!(validate(p, "sk-proj-1234567890abcdefghij").is_err());
    }

    #[test]
    fn a_key_from_another_provider_is_named() {
        let e = validate(Provider::Openai, "sk-or-v1-1234567890abcdefghij").unwrap_err();
        assert!(matches!(e, KeyError::WrongProvider(Provider::Openrouter)));
        assert!(e.to_string().contains("OpenRouter"));
        let e = validate(Provider::Openai, "sk-ant-api03-abcdefghijklmnop").unwrap_err();
        assert!(matches!(e, KeyError::WrongProvider(Provider::Anthropic)));
    }
}
