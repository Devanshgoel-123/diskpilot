---
title: See what Claude Code and Codex can do without asking
h1: See what your AI coding agents can do without asking
nav: AI agent permissions
description: Where Claude Code, Codex and Cursor keep their permission settings on a Mac, which settings let an agent act without asking, and how to tighten them.
summary: Find the settings that let an agent run commands without asking.
order: 6
updated: 2026-11-15
---

Coding agents ask before they run a command or edit a file, until you tell them not to. Every "Yes, don't ask again" and every setting copied from a blog post is saved in a config file, and months later few people remember what they allowed.

## Where the settings live

- **Claude Code** reads `~/.claude/settings.json` and `~/.claude/settings.local.json`, and each project's own `.claude/settings.json`. Rules sit under `permissions`, in `allow`, `ask` and `deny` lists.
- **Codex** reads `~/.codex/config.toml`: `approval_policy` decides when it asks, and `sandbox_mode` decides what its commands can touch.
- **Cursor** keeps its agent's permissions in `~/.cursor/cli-config.json`.

## The settings worth a second look

In Claude Code:

- `"defaultMode": "bypassPermissions"` runs every tool without asking.
- An allow rule of `Bash` or `Bash(*)` runs any shell command, including `rm` and `curl`, without a prompt.
- An allow rule of `Edit` or `Write` with no path lets it change any file.
- `"skipDangerousModePermissionPrompt": true` hides the warning you would otherwise see before that mode starts.

In Codex:

- `approval_policy = "never"` means Codex never asks before running a command.
- `sandbox_mode = "danger-full-access"` runs commands outside the sandbox, with full access to your Mac.

None of these is wrong on its own. In a throwaway container they can be exactly what you want. On the Mac that holds your SSH keys and cloud credentials, each one is worth knowing about.

## Check them by hand

```sh
cat ~/.claude/settings.json
cat ~/.codex/config.toml
```

Look for the values above. Remember that each project can add its own rules on top of these files.

## Check them with DiskPilot

[DiskPilot](/) has an AI Agents page for Claude Code, Codex, Cursor and Antigravity. It reads these files on your Mac and lists, in plain words, what each agent may do. Settings that let an agent act without asking come first.

- **Remove a rule** with one click. DiskPilot edits only that rule, leaves the rest of the file exactly as it was, and saves a copy of the file beforehand.
- **Clear one-time approvals.** Commands you approved with "Yes, don't ask again" are listed separately and can be cleared together.
- **See MCP servers, plugins and skills.** Disconnect a server you no longer use and reconnect it later as it was, and find skills you haven't used in 30 days.

Keys and tokens in these files are never shown, and nothing is sent anywhere. Rules in each project's own settings are not shown; check those in the project.

## Keys in transcripts

Agents also keep every conversation on disk, including keys you pasted or that a command printed. The same page can check Claude Code, Codex CLI and Gemini CLI transcripts for well-known key formats, offline, and remove a found key from the transcript after you confirm. Revoke the key with its provider as well.
