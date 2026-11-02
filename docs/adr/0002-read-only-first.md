# ADR 0002: Ship read-only first; one gate for destructive operations

**Status:** accepted (Phase 1); cleanup shipped in Phase 2 per ADR 0005

## Context

A cleanup tool that deletes the wrong folder destroys trust permanently.

## Decision

- v0.1 contains no destructive code at all.
- The scanner, classifier, recommendation engine, planner and executor are separate modules; only the executor may delete, and only paths returned by `safety::authorize_destructive`.
- Operations will be predefined and parameterized (no shell strings, no LLM-generated commands), previewed exactly, and confirmed by the user.
- `safety.rs` was written and tested in Phase 1, before any executor exists, so the executor is built on it rather than retrofitted.

## Consequences

Users get value (understanding) before they get risk (deletion). Phase 2 must not bypass the gate.
