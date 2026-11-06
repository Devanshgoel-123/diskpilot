//! The AI providers the assistant can use. Each is plain data: where requests
//! go, what its keys look like and where the key is kept.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    Openai,
    Openrouter,
}

/// How requests are shaped for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// Anthropic's Messages API.
    Anthropic,
    /// OpenAI's Chat Completions API, also spoken by OpenRouter.
    OpenAiChat,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Anthropic, Provider::Openai, Provider::Openrouter];

    pub fn id(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::Openai => "openai",
            Provider::Openrouter => "openrouter",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::Openai => "OpenAI",
            Provider::Openrouter => "OpenRouter",
        }
    }

    pub fn wire(self) -> Wire {
        match self {
            Provider::Anthropic => Wire::Anthropic,
            Provider::Openai | Provider::Openrouter => Wire::OpenAiChat,
        }
    }

    /// The only host this provider's key is ever sent to.
    pub fn host(self) -> &'static str {
        match self {
            Provider::Anthropic => "api.anthropic.com",
            Provider::Openai => "api.openai.com",
            Provider::Openrouter => "openrouter.ai",
        }
    }

    /// Base URL of the API, without a trailing slash.
    pub fn base_url(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Openai => "https://api.openai.com/v1",
            Provider::Openrouter => "https://openrouter.ai/api/v1",
        }
    }

    /// Keychain account name. Anthropic keeps the name used before other
    /// providers existed, so a stored key keeps working after an update.
    pub fn keychain_account(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic-api-key",
            Provider::Openai => "openai-api-key",
            Provider::Openrouter => "openrouter-api-key",
        }
    }

    pub fn env_var(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::Openai => "OPENAI_API_KEY",
            Provider::Openrouter => "OPENROUTER_API_KEY",
        }
    }

    /// Every key from this provider starts with this.
    pub fn key_prefix(self) -> &'static str {
        match self {
            Provider::Anthropic => "sk-ant-",
            Provider::Openai => "sk-",
            Provider::Openrouter => "sk-or-",
        }
    }

    /// Where people create a key.
    pub fn console_url(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://console.anthropic.com/settings/keys",
            Provider::Openai => "https://platform.openai.com/api-keys",
            Provider::Openrouter => "https://openrouter.ai/settings/keys",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_serde() {
        for p in Provider::ALL {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, format!("\"{}\"", p.id()));
            assert_eq!(serde_json::from_str::<Provider>(&json).unwrap(), p);
        }
    }

    #[test]
    fn every_base_url_is_https_on_its_own_host() {
        for p in Provider::ALL {
            assert!(p.base_url().starts_with(&format!("https://{}/", p.host())));
        }
    }

    #[test]
    fn keychain_accounts_are_distinct() {
        let mut names: Vec<_> = Provider::ALL.iter().map(|p| p.keychain_account()).collect();
        names.dedup();
        assert_eq!(names.len(), Provider::ALL.len());
    }
}
