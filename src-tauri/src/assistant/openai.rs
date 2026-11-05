//! OpenAI's Chat Completions format, used for OpenAI and OpenRouter.
//!
//! The conversation is kept in this format for as long as the provider and
//! model stay the same; switching either starts a new conversation, so
//! nothing is ever translated between formats.

use serde_json::{json, Value};

use super::client::{ApiError, Request};
use super::provider::Provider;
use super::{ModelInfo, Stop, ToolCall, Turn, SYSTEM_PROMPT};

/// DiskPilot's read-only tools as OpenAI function tools.
pub fn tool_definitions() -> Value {
    let defs = super::tools::definitions();
    Value::Array(
        defs.as_array()
            .into_iter()
            .flatten()
            .map(|d| {
                json!({
                    "type": "function",
                    "function": {
                        "name": d["name"],
                        "description": d["description"],
                        "parameters": d["input_schema"],
                    }
                })
            })
            .collect(),
    )
}

fn headers(provider: Provider, api_key: &str) -> Vec<(&'static str, String)> {
    let mut h = vec![("authorization", format!("Bearer {api_key}"))];
    if provider == Provider::Openrouter {
        // OpenRouter's optional app attribution. Names the app, nothing about the user.
        h.push(("http-referer", "https://diskpilot.live".into()));
        h.push(("x-title", "DiskPilot".into()));
    }
    h
}

pub fn request(provider: Provider, api_key: &str, model: &str, messages: &[Value]) -> Request {
    let mut all = Vec::with_capacity(messages.len() + 1);
    all.push(json!({"role": "system", "content": SYSTEM_PROMPT}));
    all.extend_from_slice(messages);
    Request {
        provider,
        url: format!("{}/chat/completions", provider.base_url()),
        headers: headers(provider, api_key),
        body: Some(json!({
            "model": model,
            "messages": all,
            "tools": tool_definitions(),
        })),
    }
}

pub fn parse(provider: Provider, model: &str, json: &Value) -> Result<Turn, ApiError> {
    // OpenRouter can report an upstream failure inside a 200 response.
    if let Some(msg) = json.pointer("/error/message").and_then(Value::as_str) {
        return Err(ApiError::BadRequest(msg.to_string()));
    }
    let choice = json
        .pointer("/choices/0")
        .ok_or_else(|| ApiError::Invalid(provider, "no choices in the response".into()))?;
    let message = &choice["message"];
    let finish = choice["finish_reason"].as_str().unwrap_or("");
    let text = message["content"].as_str().unwrap_or("").to_string();
    let refusal = message["refusal"].as_str().filter(|s| !s.is_empty());

    let raw_calls = message["tool_calls"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let calls: Vec<ToolCall> = raw_calls
        .iter()
        .filter(|c| c["type"] == "function" || c.get("type").is_none())
        .map(|c| {
            let args = c["function"]["arguments"].as_str().unwrap_or("");
            ToolCall {
                id: c["id"].as_str().unwrap_or("").to_string(),
                name: c["function"]["name"].as_str().unwrap_or("").to_string(),
                // Malformed arguments become `null`; the tool reports the error.
                input: if args.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(args).unwrap_or(Value::Null)
                },
            }
        })
        .collect();

    // Rebuild the assistant message rather than echoing unknown fields back.
    let mut assistant = json!({"role": "assistant", "content": message["content"].clone()});
    if !raw_calls.is_empty() {
        assistant["tool_calls"] = Value::Array(raw_calls);
    }

    let stop = if refusal.is_some() || finish == "content_filter" {
        Stop::Refusal
    } else if !calls.is_empty() {
        // Some models report "stop" while still asking for tools.
        Stop::ToolUse
    } else if finish == "length" {
        Stop::MaxTokens
    } else {
        Stop::Done
    };

    Ok(Turn {
        assistant: Some(assistant),
        calls,
        text,
        stop,
        model: json["model"].as_str().unwrap_or(model).to_string(),
    })
}

/// One `tool` message per result, each answering the call with that id.
pub fn tool_results(results: Vec<(String, String, bool)>) -> Vec<Value> {
    results
        .into_iter()
        .map(|(id, content, is_error)| {
            json!({
                "role": "tool",
                "tool_call_id": id,
                "content": if is_error { format!("Error: {content}") } else { content },
            })
        })
        .collect()
}

/// Requests that check the key and list models.
pub fn list_requests(provider: Provider, api_key: &str) -> Vec<Request> {
    let get = |path: &str| Request {
        provider,
        url: format!("{}{path}", provider.base_url()),
        headers: headers(provider, api_key),
        body: None,
    };
    match provider {
        // OpenRouter's model list is public, so check the key separately first.
        Provider::Openrouter => vec![get("/key"), get("/models")],
        _ => vec![get("/models")],
    }
}

/// OpenAI lists every model, including ones for images, audio and
/// embeddings. Keep those that can chat and call tools.
fn openai_chat_model(id: &str) -> bool {
    let chat_family = id.starts_with("gpt-")
        || id.starts_with("chatgpt-")
        || (id.starts_with('o') && id[1..].starts_with(|c: char| c.is_ascii_digit()));
    let excluded = [
        "audio",
        "realtime",
        "tts",
        "transcribe",
        "image",
        "search",
        "embedding",
        "moderation",
        "instruct",
        "codex",
        "vision-preview",
    ];
    chat_family && !excluded.iter().any(|x| id.contains(x))
}

pub fn parse_models(provider: Provider, json: &Value) -> Vec<ModelInfo> {
    let data = json["data"].as_array().cloned().unwrap_or_default();
    match provider {
        Provider::Openrouter => data
            .iter()
            .filter(|m| {
                m["supported_parameters"]
                    .as_array()
                    .is_some_and(|p| p.iter().any(|x| x == "tools"))
            })
            .filter_map(|m| {
                let id = m["id"].as_str()?;
                Some(ModelInfo {
                    id: id.to_string(),
                    label: m["name"].as_str().unwrap_or(id).to_string(),
                })
            })
            .collect(),
        _ => {
            let mut models: Vec<(i64, ModelInfo)> = data
                .iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    openai_chat_model(id).then(|| {
                        (
                            m["created"].as_i64().unwrap_or(0),
                            ModelInfo {
                                id: id.to_string(),
                                label: id.to_string(),
                            },
                        )
                    })
                })
                .collect();
            // Newest first.
            models.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
            models.into_iter().map(|(_, m)| m).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_convert_to_function_tools() {
        let tools = tool_definitions();
        let tools = tools.as_array().unwrap();
        assert_eq!(
            tools.len(),
            super::super::tools::definitions().as_array().unwrap().len()
        );
        for t in tools {
            assert_eq!(t["type"], "function");
            assert!(t["function"]["name"].is_string());
            assert_eq!(t["function"]["parameters"]["type"], "object");
        }
    }

    #[test]
    fn request_prepends_the_system_prompt_and_uses_a_bearer_key() {
        let r = request(
            Provider::Openai,
            "sk-test",
            "gpt-x",
            &[json!({"role": "user", "content": "hi"})],
        );
        assert_eq!(r.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            r.headers[0],
            ("authorization", "Bearer sk-test".to_string())
        );
        let body = r.body.unwrap();
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hi");
        assert!(body.get("temperature").is_none());

        let r = request(Provider::Openrouter, "sk-or-x", "m", &[]);
        assert_eq!(r.url, "https://openrouter.ai/api/v1/chat/completions");
        assert!(r.headers.iter().any(|(k, _)| *k == "x-title"));
    }

    #[test]
    fn parses_tool_calls_and_rebuilds_the_assistant_message() {
        let resp = json!({
            "model": "gpt-x-2026",
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant", "content": null, "annotations": [],
                    "tool_calls": [
                        {"id": "call_1", "type": "function", "function": {"name": "list_folder", "arguments": "{\"path\":\"~\"}"}},
                        {"id": "call_2", "type": "function", "function": {"name": "get_docker_usage", "arguments": ""}}
                    ]
                }
            }]
        });
        let t = parse(Provider::Openai, "gpt-x", &resp).unwrap();
        assert_eq!(t.stop, Stop::ToolUse);
        assert_eq!(t.model, "gpt-x-2026");
        assert_eq!(t.calls.len(), 2);
        assert_eq!(t.calls[0].input["path"], "~");
        assert_eq!(t.calls[1].input, json!({}));
        let a = t.assistant.unwrap();
        assert!(a.get("annotations").is_none());
        assert_eq!(a["tool_calls"][0]["id"], "call_1");
    }

    #[test]
    fn malformed_arguments_reach_the_tool_as_null() {
        let resp = json!({"choices": [{"finish_reason": "tool_calls", "message": {"content": null,
            "tool_calls": [{"id": "c", "type": "function", "function": {"name": "list_folder", "arguments": "{oops"}}]}}]});
        let t = parse(Provider::Openai, "m", &resp).unwrap();
        assert_eq!(t.calls[0].input, Value::Null);
    }

    #[test]
    fn refusals_length_and_errors_are_recognised() {
        let refusal = json!({"choices": [{"finish_reason": "stop", "message": {"content": null, "refusal": "No."}}]});
        assert_eq!(
            parse(Provider::Openai, "m", &refusal).unwrap().stop,
            Stop::Refusal
        );
        let filtered =
            json!({"choices": [{"finish_reason": "content_filter", "message": {"content": ""}}]});
        assert_eq!(
            parse(Provider::Openai, "m", &filtered).unwrap().stop,
            Stop::Refusal
        );
        let cut = json!({"choices": [{"finish_reason": "length", "message": {"content": "Half"}}]});
        assert_eq!(
            parse(Provider::Openai, "m", &cut).unwrap().stop,
            Stop::MaxTokens
        );
        let upstream = json!({"error": {"message": "Provider returned error", "code": 502}});
        assert!(matches!(
            parse(Provider::Openrouter, "m", &upstream),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse(Provider::Openai, "m", &json!({})),
            Err(ApiError::Invalid(..))
        ));
    }

    #[test]
    fn tool_results_answer_each_call_by_id() {
        let msgs = tool_results(vec![
            ("a".into(), "{}".into(), false),
            ("b".into(), "No scan".into(), true),
        ]);
        assert_eq!(
            msgs[0],
            json!({"role": "tool", "tool_call_id": "a", "content": "{}"})
        );
        assert_eq!(msgs[1]["content"], "Error: No scan");
    }

    #[test]
    fn openai_model_list_keeps_chat_models_newest_first() {
        let list = json!({"data": [
            {"id": "gpt-old", "created": 1},
            {"id": "gpt-new", "created": 9},
            {"id": "o9-mini", "created": 5},
            {"id": "gpt-new-audio-preview", "created": 9},
            {"id": "text-embedding-3-large", "created": 9},
            {"id": "gpt-image-2", "created": 9},
            {"id": "omni-moderation-latest", "created": 9},
            {"id": "dall-e-3", "created": 9}
        ]});
        let ids: Vec<String> = parse_models(Provider::Openai, &list)
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["gpt-new", "o9-mini", "gpt-old"]);
    }

    #[test]
    fn openrouter_model_list_keeps_only_models_with_tools() {
        let list = json!({"data": [
            {"id": "a/with-tools", "name": "A: With tools", "supported_parameters": ["tools", "temperature"]},
            {"id": "b/no-tools", "name": "B", "supported_parameters": ["temperature"]},
            {"id": "c/unknown", "name": "C"}
        ]});
        let models = parse_models(Provider::Openrouter, &list);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].label, "A: With tools");
        assert_eq!(
            list_requests(Provider::Openrouter, "k")[0].url,
            "https://openrouter.ai/api/v1/key"
        );
    }
}
