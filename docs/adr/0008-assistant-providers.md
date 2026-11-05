# ADR 0008: Anthropic, OpenAI and OpenRouter as assistant providers

**Status:** accepted. Extends [ADR 0007](0007-optional-assistant.md).

## Context

The assistant required an Anthropic key. Many people already pay for OpenAI, or prefer OpenRouter, which gives one key for models from many companies. Supporting them must not weaken the assistant's guarantees: read-only tools, a disclosed data flow, and a key that only reaches its own provider.

## Decision

- Providers are data (`assistant/provider.rs`): id, label, host, base URL, key prefix, Keychain account and environment variable. Each provider has its own Keychain entry. Anthropic keeps the account name used before, so existing keys keep working.
- Two request formats: Anthropic's Messages API (`anthropic.rs`, unchanged behavior) and OpenAI's Chat Completions (`openai.rs`), which OpenRouter also accepts. Each response is normalized into a `Turn` (text, tool calls, stop reason), and one loop in `mod.rs` runs the tools.
- The conversation is stored in the active provider's format. Switching provider or model starts a new conversation, so nothing is translated between formats.
- The tool list stays single-sourced in `tools.rs` and is converted to OpenAI function tools. The tools remain read-only for every provider.
- Anthropic offers a fixed list of models. OpenAI and OpenRouter models are listed live from the provider (`GET /models`, which spends no tokens), keeping only models that can call tools: for OpenAI, chat model families; for OpenRouter, models whose `supported_parameters` include `tools`. Listing also checks the key; OpenRouter's model list is public, so its key is checked with `GET /key` first.
- The HTTP client refuses any URL that is not on the provider's own host, uses HTTPS only and follows no redirects, so a key cannot be forwarded elsewhere. A key whose prefix belongs to another provider is rejected with a message naming that provider.

## Consequences

- Settings shows a provider picker, a key field per provider, and a model menu filled from the provider. The "What is sent" text names the exact host.
- New OpenAI-compatible providers need a table entry, not a new client.
- OpenAI and OpenRouter answers do not get Anthropic-specific features (server-side refusal fallback, prompt caching, effort). Refusals are still detected (`refusal` field or `content_filter`).
- Behavior is tested with a scripted transport for both formats; live behavior depends on the user's key and the models their account can use.
