# DiskPilot

**Understand your storage. Know what is safe. Clean it confidently.**

DiskPilot is a free, open-source macOS app that shows what is using your disk, explains what each thing is, and (in an upcoming release) helps you clean it up safely. It is built for developers whose Macs fill up with Docker images, `node_modules`, Xcode DerivedData, AI coding tool data and package manager caches, and for anyone who wants to understand their storage without running terminal commands.

> **Status: early development (v0.1, read-only).** Scanning, classification and exploration work today. DiskPilot cannot delete anything yet; cleanup ships only once its safety layer is complete. See the [roadmap](#roadmap).

<!-- Screenshots: to be added. Please do not submit screenshots containing personal file names. -->

## The problem

A 256 GB MacBook used for development fills up fast, and macOS's storage view mostly says "System Data" or "Developer". The usual fix is a mix of `du -sh`, `docker system df`, `rm -rf ~/Library/Developer/Xcode/DerivedData` and guesswork about what is safe to remove. DiskPilot makes that process visual, explains every item, and never assumes that big means disposable.

## Features

### Available now

- **Real filesystem scanning.** A parallel Rust scanner measures space actually allocated on disk, counts hard-linked files once, never follows symbolic links, and does not cross into other volumes. On an M-series Mac it scans a ~60 GB home folder with ~900k files in under a minute.
- **Overview dashboard.** Disk capacity (from the OS), how much of it the scan accounts for, a breakdown by category, the largest recognized storage consumers, and details of the last scan, including folders macOS did not let it read.
- **Classification with explanations.** Every folder and file is put in one of 11 categories (Cache, Build artifacts, Developer tools, Docker storage, Application data, Logs, Downloads, User data, Blob/binary data, System files, Unknown). Each classification shows _why_: the rule that matched, its confidence, and the folder it was inherited from.
- **Storage Explorer.** Browse folders by size with sorting, name and category filters, and a details panel with live metadata.
- **Large Files.** The 500 largest files, plus the folders where data actually accumulates. Search, filter, sort, inspect and reveal in Finder. Size alone never makes something "safe".
- **Developer storage detection.** Docker Desktop / OrbStack / Colima data, Xcode DerivedData, archives, device support and simulators, `node_modules`, npm / pnpm / Yarn / Bun caches, nvm, Next.js and other build output, Rust `target/` and Cargo caches, Python virtualenvs, Gradle and Maven caches, Cursor, VS Code, Claude Code, Codex and Gemini CLI data.
- **Scan history** stored locally in SQLite.
- Dark-first interface with light mode.

### Not built yet

Cleanup recommendations, any form of deletion, the Docker Manager (it does not talk to Docker yet), deep per-tool analysis, and the optional AI assistant. These screens exist in the app but say clearly that they are not implemented.

## Installation

There are no signed releases yet. Build from source (below). Releases will be published on the [Releases](https://github.com/Devanshgoel-123/diskpilot/releases) page.

macOS will ask for permission the first time DiskPilot reads protected folders such as Desktop, Documents and Downloads. DiskPilot does not request Full Disk Access; folders it cannot read are listed after the scan.

## Development

Requirements: macOS 11+, Rust (stable), Node.js 22, pnpm 10, Xcode Command Line Tools.

```sh
pnpm install
pnpm app:dev                 # desktop app with hot reload
pnpm app:build               # .app and .dmg in src-tauri/target/release/bundle
```

Checks:

```sh
pnpm lint && pnpm typecheck && pnpm test
cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test
```

Run the scanner from the terminal (read-only):

```sh
cd src-tauri && cargo run --release --example scan -- ~/code
```

## Architecture

```
React UI (src/)  ──typed commands──▶  Rust engine (src-tauri/src/)
                                       ├─ scanner      read-only, parallel, no symlinks, one volume
                                       ├─ classifier   pure rules → category + explanation
                                       ├─ explorer     folder listings from the scan index
                                       ├─ disk         capacity via statfs
                                       ├─ history      SQLite scan log
                                       ├─ safety       the single gate for UI-supplied paths
                                       └─ (Phase 2)    recommendations → planner → executor
```

Stack: Tauri 2, React 19, TypeScript (strict), Vite, Tailwind CSS 4, shadcn/ui-style components on Radix, Lucide, Recharts, Rust, SQLite.

The full design, including data flow and module boundaries, is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Key decisions are recorded in [docs/adr](docs/adr).

## Security principles

- **Local only.** No account, cloud backend, telemetry or API key.
- **Read-only by default.** v0.1 contains no code that modifies or deletes your files.
- **Strict pipeline separation.** The scanner and classifier cannot act. Future deletions must go planner → central safety gate → executor, with predefined operations, validated paths, protected system and home locations, no symlink following, no shell strings, an exact preview and your explicit confirmation.
- **Minimal permissions.** The UI has no generic filesystem or shell access, only DiskPilot's typed commands.
- **No fake numbers.** Every metric comes from the scanner or the OS.

Details and how to report a vulnerability: [SECURITY.md](SECURITY.md).

## Roadmap

| Phase | Scope                                                                                                                                    | Status                                       |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| 1     | Tauri app, design system, navigation, Overview, real scanning, folder sizes, classification, Storage Explorer, Large Files, scan history | ✅ Done                                      |
| 2     | Cleanup recommendations (Likely safe / Review first / Keep), previews, safe executor, cleanup history                                    | Next                                         |
| 3     | Docker Manager: images, containers, volumes, build cache, safe Docker actions                                                            | Planned                                      |
| 4     | Deep developer tool analysis: Cursor, npm/pnpm/Yarn, Xcode, stale `node_modules`                                                         | Planned (path-based detection already ships) |
| 5     | Optional AI assistant for natural-language questions; explains and suggests, never executes                                              | Planned                                      |

Known limitations in v0.1:

- "Available" excludes purgeable space, so it can be lower than Finder's figure.
- APFS clones are counted once per copy, so totals can exceed physical usage.
- Folder listings show at most 1,000 entries (the rest are summarized).
- The scan index lives in memory; reopening the app requires a rescan to browse.

## Contributing

Contributions are welcome, especially new classification rules. Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

[MIT](LICENSE)
