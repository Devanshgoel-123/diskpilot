# ADR 0001: Tauri 2 with a Rust engine

**Status:** accepted

## Context

DiskPilot scans hundreds of thousands of files and will eventually delete some. It must be fast, small and auditable.

## Decision

Use Tauri 2: the UI is React in the system WebView, all filesystem work is in Rust. The frontend gets no generic fs or shell plugin; it can only call typed commands.

## Consequences

- The bundle is small and the scanner runs at native speed.
- Every filesystem capability is visible in `commands.rs`, which makes security review tractable.
- Contributors need a Rust toolchain.
