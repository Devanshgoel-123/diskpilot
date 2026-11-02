# Architecture

DiskPilot is a Tauri 2 app: a React/TypeScript UI in a system WebView and a Rust engine in the same process. The UI calls the engine only through typed commands (`src-tauri/src/commands.rs` ↔ `src/lib/api.ts`).

## Safety pipeline

```
Scanner ──▶ Classifier ──▶ Recommendation engine ──▶ Command planner ──▶ Safety gate ──▶ Executor
(read)      (pure)          (Phase 2)                  (Phase 2)           safety.rs        (Phase 2)
```

- **Scanner** (`scanner/`) only reads. It walks with `read_dir` + `symlink_metadata`, so links are never followed, skips directories on another device (`st_dev`), and sums `st_blocks × 512` (allocated bytes), counting multiply-linked inodes once. Work is parallelized with a dedicated rayon pool (16 MB stacks for deep trees). Progress is reported through a callback every 120 ms; cancellation is an `AtomicBool`.
- **Classifier** (`classifier/`) is pure. `rules.rs` is a data table; `mod.rs` matches it. It takes a `Probe` trait to check for sibling/child entries, so the scanner answers from listings it already holds and tests run without a filesystem.
- **Safety** (`safety.rs`) validates every UI-supplied path: absolute, no `.`/`..`, no NUL, confined to the scan root. `authorize_destructive` additionally rejects protected locations and any path whose target or parents are symlinks. It is the only permitted entry point for the future executor.
- **Executor** does not exist yet. No module calls `fs::remove_*`.

## Classification model

Each rule has a matcher (`HomeRelative`, `Absolute`, `Name`, `NameWithSibling`, `Contains`), a category, a label, an explanation and a confidence. The first matching rule wins, so specific rules come first.

Rules are **Containers** or **Leaves**:

- A _Container_ (e.g. `~/Library/Caches`, a Git project) sets the category for its subtree but lets more specific rules match inside it.
- A _Leaf_ (e.g. `node_modules`, Docker's VM folder) claims its whole subtree; no rule is evaluated below it. This is why a `dist/` inside `node_modules` is counted as dependencies, not as project build output.

Files take their folder's classification, except inside containers that allow it (Documents, Desktop, code projects), where extension rules can refine them (e.g. `.gguf` → Blob/binary data). Classification never looks at size, and it never decides that something may be deleted.

During a scan the classification context is passed down the recursion, so each folder is matched once. Every byte is attributed to exactly one category, so category totals add up to the scan total (an integration test checks this).

## Data flow

1. `start_scan` validates the root, spawns a thread and returns an id.
2. The scanner emits `scan://progress` events, then builds a `ScanSummary` (category totals, top 500 files, detected locations, heavy folders, error samples) and a `DirIndex` (path → size) kept in memory.
3. `scan://finished` is emitted and the run is recorded in SQLite (`<app data>/diskpilot.sqlite3`).
4. The UI fetches the summary. The Explorer calls `list_directory`, which lists the folder live and takes folder sizes from the index.

## Frontend

- `src/lib/types.ts` mirrors `model.rs` (camelCase via serde).
- `src/lib/store.ts` (zustand) owns scan lifecycle, navigation and settings.
- `src/lib/analysis.ts` holds pure derivations (nesting-safe sums, tool grouping), unit-tested.
- `src/features/*` contain one folder per screen; `src/components/ui` holds shadcn-style primitives and `components/shared` app-level components.
- Design tokens are CSS variables in `src/styles/globals.css`; dark is primary, light is a separately tuned theme.

## Tauri capabilities

`src-tauri/capabilities/default.json` grants only `core:default`, window dragging, the folder picker (`dialog:allow-open`) and opening URLs. There is no fs or shell plugin. Reveal in Finder is a Rust command that runs `/usr/bin/open -R <path>` with the path as a separate argument, for paths inside the scanned folder only.
