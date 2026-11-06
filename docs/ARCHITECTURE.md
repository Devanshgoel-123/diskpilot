# Architecture

DiskPilot is a Tauri 2 app: a React/TypeScript UI in a system WebView and a Rust engine in the same process. The UI calls the engine only through typed commands (`src-tauri/src/commands.rs` ↔ `src/lib/api.ts`).

## Safety pipeline

```
Scanner ──▶ Classifier ──▶ Recommendation engine ──▶ Cleanup planner ──▶ Safety gate ──▶ Executor
(read)      (pure)          recommend/                 cleanup/planner     safety.rs        cleanup/executor
```

- **Scanner** (`scanner/`) only reads. It walks with `read_dir` + `symlink_metadata`, so links are never followed, skips directories on another device (`st_dev`), and sums `st_blocks × 512` (allocated bytes), counting multiply-linked inodes once. Work is parallelized with a dedicated rayon pool (16 MB stacks for deep trees). Progress is reported through a callback every 120 ms; cancellation is an `AtomicBool`.
- **Classifier** (`classifier/`) is pure. `rules.rs` is a data table; `mod.rs` matches it. It takes a `Probe` trait to check for sibling/child entries, so the scanner answers from listings it already holds and tests run without a filesystem.
- **Safety** (`safety.rs`) validates every UI-supplied path: absolute, no `.`/`..`, no NUL, confined to the scan root. `authorize_destructive` additionally rejects protected locations and any path whose target or parents are symlinks. It is the only permitted entry point for the future executor.
- **Recommendation engine** (`recommend/`) maps classifier rules to policies (`policies.rs`): tier, allowed methods, why it may be safe, risks, an alternative command, and whether to recommend per child folder (e.g. one entry per app in `~/Library/Caches`). Nested actionable items are dropped so nothing is counted or removed twice.
- **Planner** (`cleanup/planner.rs`) takes recommendation ids from the UI, looks up paths in the engine's own recommendations, authorizes each one, and returns a plan with exact paths, warnings and rejected items. Plans are stored server-side, single-use, and expire after 10 minutes.
- **Executor** (`cleanup/executor.rs`) re-authorizes every path, then moves it to the Trash (`trash` crate) or deletes it (`remove_dir_all`, symlink-safe). It is the only module that removes files, behind a `FileOps` trait so tests can observe it. Results are written to SQLite (`cleanup_actions`, `cleanup_items`).

## Developer tools analysis

`devtools/` contains one analyzer per tool (`editors.rs` for Cursor and VS Code, `agents.rs`, `node.rs`, `xcode.rs`, `toolchains.rs` for Rust and Python). Each reads the tool's on-disk layout and returns a `ToolReport` (sections of items, each with a state: current, stale, orphaned, superseded or informational) plus recommendations for items that are demonstrably unused, such as workspace storage whose project folder no longer exists (resolved from `workspace.json`), extension folders absent from the editor's `extensions.json` registry, CLI versions other than the one the launcher symlink points to, DerivedData whose `WorkspacePath` no longer exists, and Claude Code sessions whose recorded working directory no longer exists. `agent_data.rs` describes AI agent and editor locations (Claude Code, the Claude app, Codex, GitHub Copilot, and extra Cursor and VS Code data) in a table where each entry has a kind: caches, logs and downloaded updates are recommended as likely safe; stale database backups and edit checkpoints as review first; history and configuration are reported only. Cursor's chat database is opened read-only to count stored chats; DiskPilot never writes to another application's database. Sizes come from the scan index when available and are otherwise measured with `scanner::measure`, which follows the scanner's rules. Analyzer recommendations are stored next to the scan's recommendations and merged (duplicates and nested items removed) before planning, so they use the same planner, safety gate and executor.

## Assistant

`assistant/` is optional and inactive until the user stores an API key (`keychain.rs`, one macOS Keychain entry per provider). `provider.rs` lists the supported providers (Anthropic, OpenAI, OpenRouter) as data: host, base URL, key format and request format. `client.rs` sends JSON over HTTPS only to the provider's own host and never follows redirects. `anthropic.rs` builds Messages API requests (`x-api-key`, `anthropic-version: 2026-10-21`, and the `server-side-fallback-2026-07-01` beta with `fallbacks: "default"` for models that support it); `openai.rs` builds Chat Completions requests, used for OpenAI and OpenRouter, and lists the models that support tools. `mod.rs` runs one provider-neutral tool-use loop over a normalized turn: the conversation is append-only (assistant content, including thinking blocks, is kept exactly as returned), refusals are checked before content is read, tool results for one turn are returned in a single message, and rounds are capped. `tools.rs` defines the only tools the model can call. All of them read DiskPilot's in-memory data or run read-only Docker commands; paths are reported relative to `~`, and file contents are never read. The model can highlight existing recommendations for the user to review, but cannot initiate any action.

## Docker

`docker/cli.rs` finds the CLI in known locations (GUI apps do not inherit the shell PATH) and runs it with an argument vector, a minimal PATH and a timeout. `docker/parse.rs` handles Docker's human sizes and local timestamps. `docker/mod.rs` builds the overview from `docker system df -v` and links images, containers and volumes (Compose image labels pin the exact image id). `docker/actions.rs` defines the only operations DiskPilot can run, all of them about reclaiming space: `stop` (running containers), `rm` (stopped containers, no `-f`, no `-v`), `image rm` (no `-f`), `volume rm` (one volume no container uses, no `-f`, confirmed by typing its name), and `builder prune --all --force` for unused build cache. Each has a preview, and execution re-fetches Docker state and re-checks preconditions first.

## Classification model

Each rule has a matcher (`HomeRelative`, `Absolute`, `Name`, `NameWithSibling`, `Contains`), a category, a label, an explanation and a confidence. The first matching rule wins, so specific rules come first.

Rules are **Containers** or **Leaves**:

- A _Container_ (e.g. `~/Library/Caches`, a Git project) sets the category for its subtree but lets more specific rules match inside it.
- A _Leaf_ (e.g. `node_modules`, Docker's VM folder) claims its whole subtree; no rule is evaluated below it. This is why a `dist/` inside `node_modules` is counted as dependencies, not as project build output.

Files take their folder's classification, except inside containers that allow it (Documents, Desktop, code projects), where extension rules can refine them (e.g. `.gguf` → Blob/binary data). Classification never looks at size, and it never decides that something may be deleted.

During a scan the classification context is passed down the recursion, so each folder is matched once. Every byte is attributed to exactly one category, so category totals add up to the scan total (an integration test checks this).

## Data flow

1. `start_scan` validates the root, spawns a thread and returns an id.
2. The scanner emits `scan://progress` events. Folders inside other apps' containers (`~/Library/Containers/*`) are measured last, because macOS checks every folder opened there one at a time. Everything else is published first as `scan://partial`, then the containers are merged into their ancestors. The scanner then builds a `ScanSummary` (category totals, top 500 files, detected locations, heavy folders, error samples) and a `DirIndex` (path → size) kept in memory.
3. `scan://finished` is emitted and the run is recorded in SQLite (`<app data>/diskpilot.sqlite3`).
4. The UI fetches the summary. The Explorer calls `list_directory`, which lists the folder live and takes folder sizes from the index.

## Frontend

- `src/lib/types.ts` mirrors `model.rs` (camelCase via serde).
- `src/lib/store.ts` (zustand) owns scan lifecycle, navigation and settings.
- `src/lib/analysis.ts` holds pure derivations (nesting-safe sums, tool grouping), unit-tested.
- `src/features/*` contain one folder per screen; `src/components/ui` holds shadcn-style primitives and `components/shared` app-level components.
- Design tokens are CSS variables in `src/styles/globals.css`; dark is primary, light is a separately tuned theme.

## Tauri capabilities

Slow commands (folder listings, Docker, cleanup) run on a blocking thread pool so the UI never stalls.

`src-tauri/capabilities/default.json` grants only `core:default`, window dragging, the folder picker (`dialog:allow-open`) and opening URLs. There is no fs or shell plugin. Reveal in Finder is a Rust command that runs `/usr/bin/open -R <path>` with the path as a separate argument, for paths inside the scanned folder only.
