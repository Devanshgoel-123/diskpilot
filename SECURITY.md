# Security Policy

DiskPilot reads, and in future releases deletes, files on your Mac. We treat any way to make it touch the wrong file as a security issue.

## Reporting a vulnerability

Please **do not open a public issue**. Report privately via
[GitHub Security Advisories](https://github.com/Devanshgoel-123/diskpilot/security/advisories/new).

Include what you found, how to reproduce it, and the impact. We aim to acknowledge reports within 7 days.

## In scope

- Any path that lets the UI or an external input cause file access outside the scanned folder
- Bypasses of `safety.rs` (path traversal, symlink tricks, protected-path checks)
- Command or argument injection in anything DiskPilot executes (`open`, and the Docker CLI in future)
- Tauri capability or IPC misconfiguration that exposes more than the typed commands
- Anything that sends local data off the machine

## Security principles

- **Local only.** No account, telemetry, or network access is needed to scan.
- **Read-only by default.** The current release contains no code that deletes or modifies user files.
- **One gate for destructive operations.** Every future deletion must pass `safety::authorize_destructive`: absolute paths only, no `.`/`..`, confined to an approved root, protected system and home locations rejected, and no symbolic link in the target or its parents.
- **No shell strings.** External programs are run with `std::process::Command` and individual arguments, never via a shell.
- **Minimal Tauri capabilities.** The frontend has no generic filesystem or shell plugin; it can only call DiskPilot's typed commands, open a folder picker, and open URLs.
- **No LLM-executed commands.** Any future AI feature can only explain and suggest; it can never run commands.

## Supported versions

Only the latest release receives security fixes while the project is pre-1.0.
