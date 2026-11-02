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
- Anything that sends local data off the machine

## Security principles

- **Local only.** No account, telemetry, or network access is needed to scan.
- **One gate for destructive operations.** Every deletion passes `safety::authorize_destructive` when it is planned and again immediately before it runs: absolute paths only, no `.`/`..`, confined to the scanned folder and the home folder, protected system and home locations rejected, and no symbolic link in the target or its parents.
- **Plans, not paths.** The UI sends recommendation ids; the paths come from the engine. Plans are single-use and expire after 10 minutes.
- **Trash first.** Permanent deletion is only offered for regenerable data, and uses `std::fs::remove_dir_all`, which does not follow symbolic links.
- **Docker.** Only predefined actions with validated ids. No `--force`, no volume removal, preconditions re-checked against fresh Docker state before running.
- **No shell strings.** External programs are run with `std::process::Command` and individual arguments, never via a shell.
- **Minimal Tauri capabilities.** The frontend has no generic filesystem or shell plugin; it can only call DiskPilot's typed commands, open a folder picker, and open URLs.
- **No LLM-executed commands.** Any future AI feature can only explain and suggest; it can never run commands.

## Supported versions

Only the latest release receives security fixes while the project is pre-1.0.
