# Security Policy

DiskPilot reads and, when you confirm a cleanup, removes files on your Mac. We treat any way to make it touch the wrong file as a security issue.

## Reporting a vulnerability

Please **do not open a public issue**. Report privately via
[GitHub Security Advisories](https://github.com/Devanshgoel-123/diskpilot/security/advisories/new).

Include what you found, how to reproduce it, and the impact. We aim to acknowledge reports within 7 days.

## In scope

- Any path that lets the UI or an external input cause file access outside the scanned folder
- Bypasses of `safety.rs` (path traversal, symlink tricks, protected-path checks)
- Command or argument injection in anything DiskPilot executes (`open`, the Docker CLI)
- Any way to make a cleanup or Docker action affect something other than what its preview showed
- Tauri capability or IPC misconfiguration that exposes more than the typed commands
- Anything that sends local data off the machine, other than the optional assistant's documented requests
- Any way for the assistant (or content it reads) to cause an action, read file contents, or obtain the API key

## Security principles

- **Local by default.** No account, telemetry or network access is needed to scan, analyze or clean up. The optional assistant is the only networked feature; it is off by default and contacts only the API of the provider the user chose (Anthropic, OpenAI or OpenRouter), sending each key only to its own provider's host.
- **Updates.** Once a day (unless turned off in Settings) the app requests `latest.json` from this repository's latest GitHub release. The request carries no identifier. An update is installed only when the user chooses to, and only after its minisign signature is verified against the public key built into the app; the update must also be the same Developer ID-signed, notarized app.
- **Assistant isolation.** The API key is stored in the macOS Keychain and used only by the Rust engine; it is never returned to the interface. The model can call only read-only tools, sees paths relative to the home folder and never file contents, and cannot trigger cleanup.
- **One gate for destructive operations.** Every deletion passes `safety::authorize_destructive` when it is planned and again immediately before it runs: absolute paths only, no `.`/`..`, confined to the scanned folder and the home folder, protected system and home locations rejected, and no symbolic link in the target or its parents.
- **Plans, not paths.** The UI sends recommendation ids; the paths come from the engine. Plans are single-use and expire after 10 minutes.
- **Trash first.** Permanent deletion is only offered for regenerable data, and uses `std::fs::remove_dir_all`, which does not follow symbolic links.
- **Docker.** Only predefined actions with validated ids. No `--force`, no volume removal, preconditions re-checked against fresh Docker state before running.
- **No shell strings.** External programs are run with `std::process::Command` and individual arguments, never via a shell.
- **Minimal Tauri capabilities.** The frontend has no generic filesystem or shell plugin; it can only call DiskPilot's typed commands, open a folder picker, and open URLs.
- **No model-executed commands.** The assistant can explain and highlight recommendations; every action still goes through the preview and confirmation flow.

## Supported versions

Only the latest release receives security fixes while the project is pre-1.0.
