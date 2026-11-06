//! Anthropic's Messages API.
//!
//! Assistant content (including thinking and fallback blocks) is kept in the
//! conversation exactly as returned.

use serde_json::{json, Value};

use super::client::{ApiError, Request};
use super::provider::Provider;
use super::{ModelInfo, Stop, ToolCall, Turn, SYSTEM_PROMPT};

pub const API_VERSION: &str = "2026-10-21";
/// Server-side refusal fallback ("default" routing by refusal category).
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Models offered for Anthropic. The first is the default.
pub const MODELS: &[(&str, &str)] = &[
    ("claude-opus-5-5", "Claude Opus 5.5"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
    ("claude-haiku-5-5", "Claude Haiku 5.5"),
];

pub fn models() -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|(id, label)| ModelInfo {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
}

pub fn is_known_model(model: &str) -> bool {
    MODELS.iter().any(|(id, _)| *id == model)
}

/// Server-side refusal fallback is available for Opus and Sonnet, not Haiku.
pub fn uses_fallback(model: &str) -> bool {
    model != "claude-haiku-5-5"
}

/// Build the request body. Thinking is left at the model default (adaptive)
/// and no sampling parameters are sent.
pub fn request_body(model: &str, messages: &[Value]) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": 16000,
        "system": [{"type": "text", "text": SYSTEM_PROMPT, "cache_control": {"type": "ephemeral"}}],
        "tools": super::tools::definitions(),
        "output_config": {"effort": "medium"},
        "messages": messages,
    });
    if uses_fallback(model) {
        body["fallbacks"] = json!("default");
    }
    body
}

fn headers(api_key: &str) -> Vec<(&'static str, String)> {
    vec![
        ("x-api-key", api_key.to_string()),
        ("anthropic-version", API_VERSION.to_string()),
    ]
}

pub fn request(api_key: &str, model: &str, messages: &[Value]) -> Request {
    let mut headers = headers(api_key);
    if uses_fallback(model) {
        headers.push(("anthropic-beta", FALLBACK_BETA.to_string()));
    }
    Request {
        provider: Provider::Anthropic,
        url: format!("{}/messages", Provider::Anthropic.base_url()),
        headers,
        body: Some(request_body(model, messages)),
    }
}

pub fn parse(model: &str, json: &Value) -> Result<Turn, ApiError> {
    let stop_reason = json["stop_reason"].as_str().unwrap_or("");
    let content = json["content"].as_array().cloned().unwrap_or_default();
    let text = content
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let calls = content
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| ToolCall {
            id: b["id"].as_str().unwrap_or("").to_string(),
            name: b["name"].as_str().unwrap_or("").to_string(),
            input: b["input"].clone(),
        })
        .collect();
    let stop = match stop_reason {
        "refusal" => Stop::Refusal,
        "tool_use" => Stop::ToolUse,
        "pause_turn" => Stop::PauseTurn,
        "max_tokens" => Stop::MaxTokens,
        _ => Stop::Done,
    };
    // A refusal with no content leaves nothing to keep in the history.
    let assistant = (!(stop == Stop::Refusal && content.is_empty()))
        .then(|| json!({"role": "assistant", "content": content}));
    Ok(Turn {
        assistant,
        calls,
        text,
        stop,
        model: json["model"].as_str().unwrap_or(model).to_string(),
    })
}

/// All results for one turn go back in a single user message.
pub fn tool_results(results: Vec<(String, String, bool)>) -> Vec<Value> {
    let blocks: Vec<Value> = results
        .into_iter()
        .map(|(id, content, is_error)| {
            json!({"type": "tool_result", "tool_use_id": id, "content": content, "is_error": is_error})
        })
        .collect();
    vec![json!({"role": "user", "content": blocks})]
}

/// Checks the key without spending tokens.
pub fn list_request(api_key: &str) -> Request {
    Request {
        provider: Provider::Anthropic,
        url: format!("{}/models", Provider::Anthropic.base_url()),
        headers: headers(api_key),
        body: None,
    }
}
