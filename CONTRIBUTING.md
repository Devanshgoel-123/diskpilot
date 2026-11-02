# Contributing to DiskPilot

Thanks for helping. DiskPilot touches people's files, so correctness and safety come before features.

## Development setup

Requirements: macOS 11+, [Rust](https://rustup.rs) (stable), Node.js 22, [pnpm](https://pnpm.io) 10, and Xcode Command Line Tools (`xcode-select --install`).

```sh
git clone https://github.com/Devanshgoel-123/diskpilot.git
cd diskpilot
pnpm install
pnpm app:dev        # runs the desktop app with hot reload
```

`pnpm dev` alone starts only the web UI in a browser, without the engine. It shows a notice instead of data.

### Checks (CI runs all of these)

```sh
pnpm format:check && pnpm lint && pnpm typecheck && pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Try the scanner without the UI:

```sh
cd src-tauri && cargo run --release --example scan -- ~/some/folder
```

## Project layout

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). In short: `src-tauri/src` is the Rust engine (scanner, classifier, safety, commands) and `src/` is the React UI, which talks to the engine only through `src/lib/api.ts`.

## Adding a classification rule

Most contributions will be teaching DiskPilot about a new location. Rules live in `src-tauri/src/classifier/rules.rs`.

1. Add a `DirRule` (or `FileRule`) with a unique `id`, the right category, and an **explanation** that says what the data is and what happens if it is removed. Be honest about uncertainty and use `Confidence::Medium` when inferring.
2. Place it **before** any broader rule it overlaps with (the first match wins).
3. Use `Leaf` for self-contained items (nothing inside needs separate classification) and `Container` for broad areas.
4. Add a test in `classifier/mod.rs`.
5. If it belongs to a developer tool, add its id prefix to `TOOL_DEFS` in `src/lib/analysis.ts`.

A classification never decides that something is safe to delete. Keep explanations factual.

## Adding a cleanup policy

Recommendations come from `src-tauri/src/recommend/policies.rs`. A policy maps a classifier rule to a tier (`LikelySafe`, `ReviewFirst`, `Keep`), the methods DiskPilot may use (Trash, permanent delete, or none), why it may be safe, the risks, and optionally a command the user can run themselves. Permanent delete is only for data that is regenerable without user input. `Keep` policies must not be actionable (a test enforces this).

## Rules for code that touches files

- Never call `fs::remove_*`, `fs::rename`, `trash::delete` or anything destructive outside `cleanup/executor.rs`, which routes every path through `safety::authorize_destructive`.
- Docker changes go only through the `DockerAction` enum in `docker/actions.rs`. Never add volume removal or `--force`.
- Never build shell command strings. Use `Command::new(..).arg(..)`.
- Never follow symlinks during scanning: use `symlink_metadata`.
- Never show a number in the UI that did not come from the scanner or the OS.

## Commits and pull requests

- Small, focused commits using [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, `docs:`, `test:`, `chore:`).
- Fill in the PR template, including the safety checklist.
- Do not commit personal paths, scan output, or screenshots containing private file names.
