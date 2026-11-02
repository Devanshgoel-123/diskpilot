# ADR 0007: An optional, read-only assistant with the user's own API key

**Status:** accepted

## Context

Natural-language questions ("what can I safely clean up?") are a useful entry point, but DiskPilot's core promise is local operation and safe cleanup. An assistant must not weaken either.

## Decision

- The assistant is disabled by default and inactive without an API key supplied by the user. The key is stored in the macOS Keychain and used only by the Rust engine; the interface can store or remove it but never read it.
- Requests are sent from Rust using the documented Messages API over HTTPS (Rust has no official SDK). The default model is Claude Opus 5.5; Sonnet 5.5 and Haiku 5.5 can be selected. Server-side refusal fallback (`fallbacks: "default"`) is enabled where supported, and refusals are checked before reading content.
- The model receives a fixed set of read-only tools over DiskPilot's in-memory data and read-only Docker commands. Paths are reported relative to `~`; file contents are never read.
- The model cannot act. It can highlight existing recommendations, which the user reviews and confirms through the standard plan and executor.
- The conversation is append-only and kept in memory; switching models starts a new conversation.

## Consequences

- Core functionality remains entirely local; the assistant is the only networked feature, and its data flow is disclosed in Settings.
- Answers are limited to what DiskPilot has measured; the assistant cannot browse the filesystem beyond the scanned folder.
- The tool-use loop is tested with a scripted transport; live API behavior depends on the user's key and account.
