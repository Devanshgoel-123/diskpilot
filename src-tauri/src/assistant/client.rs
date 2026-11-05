//! Minimal HTTPS client for the providers' JSON APIs.
//!
//! Rust has no official SDK for these APIs, so this sends the documented raw
//! HTTP requests. The API key is read from the macOS Keychain by the caller
//! and only ever travels in the provider's auth header, to the provider's own
//! host: redirects are never followed.

use std::time::Duration;

use serde_json::Value;

use super::provider::Provider;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{label} rejected the API key. Check it in Settings.", label = .0.label())]
    Unauthorized(Provider),
    #[error("Rate limited by {label}. Try again in a moment.", label = .0.label())]
    RateLimited(Provider),
    #[error("{label} is temporarily unavailable ({status}). Try again shortly.", label = .0.label(), status = .1)]
    Unavailable(Provider, u16),
    #[error("The request was rejected: {0}")]
    BadRequest(String),
    #[error("Could not reach {label}: {detail}", label = .0.label(), detail = .1)]
    Network(Provider, String),
    #[error("Unexpected response from {label}: {detail}", label = .0.label(), detail = .1)]
    Invalid(Provider, String),
}

/// One JSON request to a provider. `body: None` sends a GET.
#[derive(Debug, Clone)]
pub struct Request {
    pub provider: Provider,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Value>,
}

/// Sends one request. A trait so the conversation loop can be tested
/// without network access.
pub trait Transport: Send + Sync {
    fn send(&self, req: &Request) -> Result<Value, ApiError>;
}

pub struct HttpTransport {
    client: reqwest::blocking::Client,
}

impl HttpTransport {
    pub fn new() -> Result<Self, ApiError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|e| ApiError::Network(Provider::Anthropic, e.to_string()))?;
        Ok(Self { client })
    }

    fn send_once(&self, req: &Request) -> Result<Value, ApiError> {
        let p = req.provider;
        if !req.url.starts_with(&format!("https://{}/", p.host())) {
            return Err(ApiError::Invalid(p, "request to an unexpected host".into()));
        }
        let mut builder = match &req.body {
            Some(body) => self.client.post(&req.url).json(body),
            None => self.client.get(&req.url),
        };
        for (name, value) in &req.headers {
            builder = builder.header(*name, value);
        }
        let resp = builder
            .send()
            .map_err(|e| ApiError::Network(p, e.to_string()))?;
        let status = resp.status().as_u16();
        let json: Value = resp
            .json()
            .map_err(|e| ApiError::Invalid(p, e.to_string()))?;
        check_status(p, status, json)
    }
}

/// Map an HTTP status and body to a result. Both providers put a readable
/// message at `error.message`.
pub fn check_status(p: Provider, status: u16, json: Value) -> Result<Value, ApiError> {
    let message = || {
        json.pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("invalid request")
            .to_string()
    };
    match status {
        200 => Ok(json),
        401 | 403 => Err(ApiError::Unauthorized(p)),
        429 => Err(ApiError::RateLimited(p)),
        400 | 402 | 404 | 413 | 422 => Err(ApiError::BadRequest(message())),
        s if (300..400).contains(&s) => Err(ApiError::Invalid(p, format!("redirected ({s})"))),
        s => Err(ApiError::Unavailable(p, s)),
    }
}

impl Transport for HttpTransport {
    fn send(&self, req: &Request) -> Result<Value, ApiError> {
        // One retry for transient failures.
        match self.send_once(req) {
            Err(ApiError::Unavailable(..) | ApiError::RateLimited(_) | ApiError::Network(..)) => {
                std::thread::sleep(Duration::from_secs(2));
                self.send_once(req)
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn statuses_map_to_errors_that_name_the_provider() {
        let p = Provider::Openai;
        assert!(check_status(p, 200, json!({})).is_ok());
        let e = check_status(p, 401, json!({})).unwrap_err();
        assert!(e.to_string().starts_with("OpenAI rejected"));
        let e = check_status(p, 400, json!({"error": {"message": "bad tool"}})).unwrap_err();
        assert_eq!(e.to_string(), "The request was rejected: bad tool");
        assert!(matches!(
            check_status(p, 302, json!({})),
            Err(ApiError::Invalid(..))
        ));
        assert!(matches!(
            check_status(p, 503, json!({})),
            Err(ApiError::Unavailable(_, 503))
        ));
    }

    #[test]
    fn requests_to_other_hosts_are_refused_before_sending() {
        let t = HttpTransport::new().unwrap();
        let req = Request {
            provider: Provider::Openai,
            url: "https://example.com/v1/models".into(),
            headers: vec![("authorization", "Bearer k".into())],
            body: None,
        };
        assert!(matches!(t.send_once(&req), Err(ApiError::Invalid(..))));
        // A look-alike host that merely starts with the provider's name.
        let req = Request {
            url: "https://api.openai.com.evil.test/v1/models".into(),
            ..req
        };
        assert!(matches!(t.send_once(&req), Err(ApiError::Invalid(..))));
    }
}
