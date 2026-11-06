# DiskPilot

**Open-source storage intelligence for macOS.** DiskPilot shows what is using your disk, explains what each item is, and helps you reclaim space safely.

DiskPilot is a native desktop application built with Tauri and Rust. It scans your storage locally, classifies every folder with an explanation, identifies developer data that can be regenerated, and performs cleanup only through a previewed, confirmed and audited workflow. It is built for developers whose Macs accumulate Docker images, `node_modules`, Xcode build data, AI coding tool data and package manager caches, and who want to know what is safe to remove before anything is removed.

> **Project status:** early development (version 0.5). Release builds are published on GitHub, signed with a Developer ID and notarized by Apple; see [Installation](#installation).

<!-- Screenshots will be added here. Contributors: please do not submit screenshots containing personal file names. -->

## Contents

- [Why DiskPilot](#why-diskpilot)
- [Features](#features)
- [Safety model](#safety-model)
- [Privacy](#privacy)
- [Installation](#installation)
- [Using DiskPilot](#using-diskpilot)
- [Development](#development)
- [Architecture](#architecture)
- [Roadmap](#roadmap)
- [Known limitations](#known-limitations)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)

## Why DiskPilot

On a development machine, storage disappears into places that are hard to see and harder to judge: Docker's virtual disk, dependency folders in dozens of projects, Xcode DerivedData, editor workspace state, superseded tool versions and assorted caches. macOS reports much of this as "System Data" or "Developer", and reclaiming it usually means a series of `du`, `docker system df` and `rm -rf` commands combined with guesswork about what is safe to remove.

DiskPilot replaces that process with three steps:

1. **Understand** where storage goes, with every item classified and explained.
2. **Judge** what can be removed, with each recommendation stating why it is likely safe and what could go wrong.
3. **Clean up** through a workflow that previews exact paths, requires confirmation, prefers the Trash, and records every action.

## Features

### Storage analysis

- A parallel Rust scanner measures the space actually allocated on disk. It never follows symbolic links, does not cross into other volumes, and counts hard-linked files once. A typical home folder of about 60 GB and 900,000 files scans in well under a minute on Apple silicon.
- The **Overview** shows the whole disk as one strip: scanned data by category, the rest of used space and free space, with likely recoverable space marked inside each category. Below it are the items most worth reviewing, where space goes, the largest recognized consumers and details of the last scan, including folders macOS did not permit DiskPilot to read.
- **Storage** browses folders by size, with sorting, filtering by name and category, a Cleanup column that shows each folder's tier or how much inside it can be cleaned up, and a details panel with live metadata and Reveal in Finder.
- **Applications** lists installed apps with the size of each app and the data it keeps in `~/Library` (caches, containers, support files), and the sandbox data left behind by apps that are no longer installed. Leftovers are reported for review in Finder and never removed automatically, since they can contain documents.
- **Storage > Largest items** lists the 500 largest files and the folders where data accumulates. Size alone is never treated as a reason to delete.
- **Duplicates** finds files with identical contents (same size, then SHA-256). Hidden folders, `~/Library`, app and document bundles, dependency folders and package caches are skipped, hard links are counted once, and APFS clones are flagged because removing one frees little or no space. You choose the copy that stays; one copy of every set always stays, and each copy is compared byte for byte with it again just before it is moved to the Trash.
- A **menu bar item** shows the space available on the startup disk, with an icon whose ring fills as the disk does, and opens Cleanup, Duplicates or Empty Trash. Closing the window keeps it running; Quit ends the app.

### Classification

Every folder and file is assigned to one of eleven categories: Cache, Build artifacts, Developer tools, Docker storage, Application data, Logs, Downloads, User data, Blob/binary data, System files and Unknown. Each classification shows the rule that produced it, its confidence, and the folder it was inherited from. Rules are plain data and recognize, among others, Docker Desktop, OrbStack and Colima data; Xcode DerivedData, archives, device support and simulators; `node_modules` and npm, pnpm, Yarn and Bun caches; Next.js and other framework build output; Rust `target` folders and Cargo caches; Python virtual environments; the Go module and build caches; Gradle and Maven caches; and Cursor, VS Code, Claude Code, Codex and Gemini CLI data.

### Cleanup recommendations

Recommendations are grouped into three tiers:

| Tier         | Meaning                                                                                                                                       |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Likely safe  | Well-understood data that is regenerated automatically, such as package manager caches and build output. Still reviewed and confirmed by you. |
| Review first | Possibly disposable, but an application or project may depend on it, such as dependencies of a project or editor state for a deleted project. |
| Keep         | Important or not sufficiently understood. Explained, never offered for removal.                                                               |

Each recommendation states why it was detected, why it may be safe, the risks, the confidence, the estimated space reclaimed and the proposed action. Where a tool has its own cleanup command (for example `pnpm store prune`, `nvm uninstall` or `xcrun simctl delete unavailable`), DiskPilot shows the command for you to run rather than deleting files itself.

### Safe cleanup

Selected recommendations are turned into a cleanup plan that lists every path to be affected. Nothing happens until you confirm. Items are moved to the Trash by default; permanent deletion is offered only when every selected item is regenerable data. Each path is verified again immediately before it is touched. The **History** page records every action with the affected paths, the exact operation or command, the space reclaimed and any errors.

**Empty Trash** lists what is in the Trash first, then removes exactly those items, one by one, with live progress, a Stop button and a per-item result. Reading the Trash requires Full Disk Access; without it, DiskPilot explains how to grant it.

### Docker Manager

- Disk usage for images, containers, volumes and build cache, with the space Docker reports as reclaimable.
- Containers with their state, image and Compose project, and what each one keeps on disk.
- Images with the containers that depend on them, and the space unique to each image versus layers shared with others.
- Removal of stopped containers, unused images and unused build cache, each previewed with the exact `docker` command before it runs.
- Volumes are listed and can be inspected, but DiskPilot never removes them.
- Diagnostics that explain where Docker's space is going, including the size of Docker Desktop's virtual disk image.
- Clear handling of Docker being absent or not running.

### Developer tools analysis

Dedicated analyzers inspect the on-disk layout of individual tools:

| Tool               | What is analyzed                                                                                                                                                                                                                                                                                 |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Cursor and VS Code | Workspace storage per project, including storage for projects that no longer exist; installed extensions versus leftover extension folders; caches and logs; leftover update downloads; stale state database backups; the size and number of chats in Cursor's chat history database (read-only) |
| Claude Code        | Installed versions, with the current version identified from the launcher; session transcripts per project, including sessions for folders that no longer exist; MCP and debug logs, shell snapshots, caches and edit checkpoints                                                                |
| Claude app         | Chromium and WebGPU caches, the app cache, downloaded updates and logs                                                                                                                                                                                                                           |
| Codex              | CLI and app logs, the app cache and downloaded updates; sessions and prompt history are listed but never recommended                                                                                                                                                                             |
| GitHub Copilot     | Copilot CLI logs and sessions; Copilot extensions and their storage in VS Code and Cursor, including storage left behind after the extension was uninstalled                                                                                                                                     |
| Other AI agents    | Cursor Agent CLI versions; Gemini CLI and Cursor agent data                                                                                                                                                                                                                                      |
| Node.js            | `node_modules` per project with the date its package files last changed; package manager caches; Node versions installed with nvm and the default version                                                                                                                                        |
| Xcode              | DerivedData per project, including data for deleted projects; archives; device support; simulators, including unavailable ones                                                                                                                                                                   |
| Rust               | Toolchains and the default toolchain; `target` folders per project; Cargo caches                                                                                                                                                                                                                 |
| Python             | Virtual environments; pip, uv and Poetry caches; downloaded Hugging Face models                                                                                                                                                                                                                  |
| Go                 | Module cache and build cache, including locations moved with `go env -w`; toolchains downloaded into the module cache; programs installed with `go install`. The module cache is read-only, so it is cleared with `go clean -modcache`, never by DiskPilot                                       |
| Local AI models    | Ollama models (with layers shared between models) and LM Studio models, listed for reference; Ollama layers no model uses and interrupted downloads are offered for cleanup                                                                                                                      |

**Credentials in AI transcripts.** Coding agents keep every conversation on disk, including keys that were pasted or printed. DiskPilot can check the Claude Code, Codex CLI and Gemini CLI transcripts for well-known credential formats (API keys, access tokens, private keys). The check runs offline, changes nothing, and shows only a masked form of each value.

Findings that can be cleaned up feed into the same recommendation and cleanup workflow. For AI agents, caches, logs and downloaded updates are marked likely safe; edit checkpoints, stale backups and sessions for deleted projects need review; conversation history, sign-in and settings are never recommended. DiskPilot reads other applications' databases but never modifies them.

### Assistant (optional)

An optional assistant answers questions about your storage in plain language, for example "What can I safely clean up?" or "Which projects have dependencies I have not touched in months?". It is disabled by default and uses your own API key from Anthropic, OpenAI or OpenRouter (one key for models from many companies), stored in the macOS Keychain. The assistant has read-only access to DiskPilot's data through a fixed set of tools. It cannot delete, move or run anything; when it suggests a cleanup, it highlights existing recommendations that you then review and confirm through the normal workflow.

## Safety model

DiskPilot modifies files only through a single, narrow path:

```
Scanner -> Classifier -> Recommendations -> Planner -> Safety gate -> Executor
(read)     (pure)        (explain only)     (preview)  (validate)     (act, record)
```

- The scanner, classifier, recommendation engine, developer-tools analyzers and assistant can only read. Removal happens exclusively in the cleanup executor.
- The interface sends recommendation identifiers, never paths. Paths come from the engine's own recommendations.
- Every path is validated when the plan is built and again immediately before execution: it must be absolute, inside your home folder and the scanned folder, outside protected system and home locations, and free of symbolic links in the path itself or any parent.
- Plans are single use and expire after ten minutes.
- Cleanup operations are predefined. There is no shell command construction and no unrestricted recursive deletion; permanent deletion uses a routine that does not follow symbolic links.
- Docker actions are limited to a closed set of operations with validated identifiers. DiskPilot never uses `--force`, never removes volumes, and never runs a prune without listing what it affects.
- The interface has no general filesystem or shell access; it can only call DiskPilot's typed commands.

## Privacy

- Scanning, classification, recommendations, cleanup and Docker management run entirely on your Mac. There is no account, no telemetry and no cloud service. The only automatic network request is a daily check of DiskPilot's GitHub releases for a newer version, which sends nothing about you or your Mac and can be turned off in Settings.
- DiskPilot works without Full Disk Access. macOS asks for permission the first time protected folders such as Desktop, Documents and Downloads are read, and folders such as Mail, Messages, iCloud Drive and the Photos library are skipped and listed as not measured.
- If you grant Full Disk Access in System Settings, DiskPilot can also measure those folders. It reads sizes only, keeps the results on your Mac, and never shares what is inside them with the assistant: the assistant sees each of them as a single total.
- The optional assistant is the only feature that uses the network. When you ask a question, the question and the storage data needed to answer it (folder and file names, paths with your home folder shown as `~`, sizes and categories; private folders such as Mail, Messages and Photos only as totals) are sent directly from your Mac to the API of the provider you chose (`api.anthropic.com`, `api.openai.com` or `openrouter.ai`). File contents are never read or sent. The API key never leaves the Rust process except in the request to that provider's own host; redirects are never followed.

## Installation

### Download

1. Download [DiskPilot.dmg](https://github.com/Devanshgoel-123/diskpilot/releases/latest/download/DiskPilot.dmg) from the [latest release](https://github.com/Devanshgoel-123/diskpilot/releases/latest). It is a universal build for Apple silicon and Intel Macs running macOS 11 or later.
2. Open the disk image and drag DiskPilot into Applications.
3. Open DiskPilot. Releases from 0.5.1 on are signed with a Developer ID and notarized by Apple, so macOS opens them without a warning. Versions before 0.5.1 were not: for those, choose **OK** when macOS warns you, then select **Open Anyway** in **System Settings > Privacy & Security**.

### Homebrew

```sh
brew tap devanshgoel-123/tap
brew trust devanshgoel-123/tap
brew install --cask diskpilot
```

Update with `brew upgrade --cask diskpilot`.

New versions are published as GitHub releases. From 0.6, DiskPilot checks for them once a day and shows **DiskPilot X is available** in the sidebar, with the release notes and an **Update and restart** button; you can also check in **Settings > Updates**. Each update is signed, and the app verifies the signature before installing it. Your scan history and settings are kept. Versions before 0.6 do not update themselves: download the new version once and replace the app in Applications.

### Build from source

You need:

- macOS 11 or later
- [Rust](https://rustup.rs) 1.82 or later
- Node.js 22 and [pnpm](https://pnpm.io) 10
- Xcode Command Line Tools (`xcode-select --install`)

```sh
git clone https://github.com/Devanshgoel-123/diskpilot.git
cd diskpilot
pnpm install
pnpm app:build
```

The application bundle and disk image are written to `src-tauri/target/release/bundle/`. For a universal build, add the Intel target with `rustup target add x86_64-apple-darwin` and run `pnpm tauri build --target universal-apple-darwin`.

## Using DiskPilot

1. Open DiskPilot and select **Scan**. By default your home folder is scanned; another folder can be chosen in Settings.
2. Review the **Overview** and browse **Storage** (folders and largest items) and **Applications** to see where space is used.
3. Open **Cleanup**, expand items to read the reasoning, and select the ones you want to remove.
4. Select **Review cleanup**, check the listed paths, confirm, and choose between moving to the Trash and permanent deletion where offered.
5. Use **Developer Tools** and **Docker** for tool-specific analysis, and **History** to see what was done.
6. Optionally, enable the **Assistant** in Settings and add an API key.

## Development

```sh
pnpm install
pnpm app:dev            # run the desktop app with hot reload
```

Quality checks (all run in continuous integration):

```sh
pnpm format:check && pnpm lint && pnpm typecheck && pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Run the scanner from the command line without the interface (read-only):

```sh
cd src-tauri && cargo run --release --example scan -- ~/code
```

### Project structure

```
src/                     React and TypeScript interface
  components/            design-system primitives and shared components
  features/              one folder per screen
  lib/                   typed API client, store, formatting and pure helpers
src-tauri/src/           Rust engine
  scanner/               parallel, read-only filesystem scanner
  classifier/            classification rules and matching
  recommend/             cleanup policies and recommendation engine
  cleanup/               cleanup planner and executor
  safety.rs              path validation and protected locations
  devtools/              per-tool analyzers
  docker/                Docker CLI integration and actions
  assistant/             optional assistant: API client, Keychain, read-only tools
  commands.rs            typed commands exposed to the interface
src-tauri/tests/         integration tests against real temporary folders
docs/                    architecture and decision records
```

Technology: Tauri 2, Rust, React 19, TypeScript (strict mode), Vite, Tailwind CSS 4, Radix primitives in the shadcn/ui style, Lucide icons, Recharts and SQLite.

## Architecture

The interface communicates with the Rust engine only through typed commands. Slow operations run on background threads so the interface stays responsive. Scan results are held in memory; scan and cleanup history are stored in a local SQLite database. The full design, including data flow, module boundaries and the Docker and assistant integrations, is described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and significant decisions are recorded in [docs/adr](docs/adr).

## Roadmap

| Phase | Scope                                                                                                                                                        | Status   |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- |
| 1     | Application foundation, design system, Overview, scanning, folder sizes, classification, Storage Explorer, Large Files                                       | Complete |
| 2     | Cleanup recommendations, risk tiers, previews, safe executor, cleanup history                                                                                | Complete |
| 3     | Docker Manager with previewed actions and diagnostics                                                                                                        | Complete |
| 4     | Developer tools analysis for Cursor, VS Code, Claude Code, Codex, Copilot, Node.js, Xcode, Rust and Python                                                   | Complete |
| 5     | Optional assistant with read-only access to storage data                                                                                                     | Complete |
| 6     | Saved scans, version and leftover detection, Empty Trash, duplicates, local AI models, transcript credential check, menu bar item, Full Disk Access guidance | Complete |

Planned next: a read-only MCP server so coding agents such as Claude Code and Cursor can ask DiskPilot where space went, incremental rescans, and additional analyzers (Homebrew, JetBrains IDEs, Android Studio).

Not planned: automatic or scheduled cleanup (every removal is approved by you), managing Docker containers beyond reclaiming space, and general-purpose "junk cleaning".

## Known limitations

- The available space shown excludes purgeable space, so it can be lower than the figure in Finder.
- APFS clones are counted once per copy in scan totals, so totals can exceed physical usage. (The duplicate finder does account for clones.)
- The last scan is saved and shown after a restart; cleanup measures every item again before you confirm.
- After a cleanup, sizes elsewhere in the app reflect the last scan until you rescan. Space moved to the Trash is freed only when the Trash is emptied.
- Folder listings show at most 1,000 entries; the remainder is summarized.
- Docker actions have been tested against Docker Desktop. OrbStack and Colima are detected but less thoroughly exercised.
- Project activity for `node_modules`, `target` folders and virtual environments is inferred from when dependency manifests last changed.

## Contributing

Contributions are welcome, particularly new classification rules, cleanup policies and tool analyzers. Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, conventions and the safety rules that apply to code touching the filesystem, and follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Please report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## License

DiskPilot is released under the [MIT License](LICENSE).
