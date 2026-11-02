//! Minimal Claude Messages API client over HTTPS.
//!
//! Rust has no official Anthropic SDK, so this sends the documented raw HTTP
//! request (`POST /v1/messages`). The API key is read from the macOS Keychain
//! by the caller and only ever travels in the `x-api-key` header.

use std::time::Duration;

use serde_json::Value;

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const API_VERSION: &str = "2026-10-21";
/// Server-side refusal fallback ("default" routing by refusal category).
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("The Anthropic API rejected the API key. Check it in Settings.")]
    Unauthorized,
    #[error("Rate limited by the Anthropic API. Try again in a moment.")]
    RateLimited,
    #[error("The Anthropic API is temporarily unavailable ({0}). Try again shortly.")]
    Unavailable(u16),
    #[error("The request was rejected: {0}")]
    BadRequest(String),
    #[error("Could not reach the Anthropic API: {0}")]
    Network(String),
    #[error("Unexpected response from the Anthropic API: {0}")]
    Invalid(String),
}

/// Sends one Messages API request. A trait so the conversation loop can be
/// tested without network access.
pub trait Transport: Send + Sync {
    fn send(&self, api_key: &str, beta: Option<&str>, body: &Value) -> Result<Value, ApiError>;
}

pub struct HttpTransport {
    client: reqwest::blocking::Client,
}

impl HttpTransport {
    pub fn new() -> Result<Self, ApiError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Ok(Self { client })
    }

    fn send_once(
        &self,
        api_key: &str,
        beta: Option<&str>,
        body: &Value,
    ) -> Result<Value, ApiError> {
        let mut req = self
            .client
            .post(API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(body);
        if let Some(beta) = beta {
            req = req.header("anthropic-beta", beta);
        }
        let resp = req.send().map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let json: Value = resp.json().map_err(|e| ApiError::Invalid(e.to_string()))?;
        match status {
            200 => Ok(json),
            401 | 403 => Err(ApiError::Unauthorized),
            429 => Err(ApiError::RateLimited),
            400 | 404 | 413 => Err(ApiError::BadRequest(
                json.pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("invalid request")
                    .to_string(),
            )),
            s => Err(ApiError::Unavailable(s)),
        }
    }
}

impl Transport for HttpTransport {
    fn send(&self, api_key: &str, beta: Option<&str>, body: &Value) -> Result<Value, ApiError> {
        // One retry for transient failures.
        match self.send_once(api_key, beta, body) {
            Err(ApiError::Unavailable(_) | ApiError::RateLimited | ApiError::Network(_)) => {
                std::thread::sleep(Duration::from_secs(2));
                self.send_once(api_key, beta, body)
            }
            other => other,
        }
    }
}
