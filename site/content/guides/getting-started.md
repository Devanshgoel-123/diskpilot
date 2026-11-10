---
title: Getting started with DiskPilot on your Mac
h1: Getting started with DiskPilot
nav: Getting started
description: Install DiskPilot, run your first scan and clear space safely, in about five minutes.
summary: Install, scan, review and clear your first few gigabytes, step by step.
order: 1
updated: 2026-11-09
---

DiskPilot reads your disk, tells you what each large folder is, and moves things to the Trash only after you review them. This guide takes you from download to your first cleanup.

## Install

1. Download [DiskPilot.dmg](https://github.com/Devanshgoel-123/diskpilot/releases/latest/download/DiskPilot.dmg). One file works on Apple silicon and Intel Macs with macOS 11 or later.
2. Open it and drag DiskPilot into Applications.
3. Open DiskPilot. It is signed and notarized by Apple, so it opens without a security warning.

Prefer Homebrew? Trust the tap once, then install:

```sh
brew tap devanshgoel-123/tap
brew trust devanshgoel-123/tap
brew install --cask diskpilot
```

From version 0.6, DiskPilot updates itself: a banner appears when a new version is ready, and nothing installs until you click Update.

## Run your first scan

Click **Scan**. DiskPilot reads your home folder and changes nothing. Results start to appear within seconds and fill in as the scan goes on.

macOS may ask whether DiskPilot can read Desktop, Documents or Downloads. Allow it if you want those folders measured. DiskPilot does not need Full Disk Access; folders such as Mail, Messages and the Photos library are skipped and listed as skipped.

## Read the Overview

The Overview shows how your disk splits into apps, caches, build output, developer tools, Docker and your own files, and how much is likely safe to clear. Every folder DiskPilot recognises has a name, the rule that matched it and a sentence saying what it is.

## Review a cleanup

Open **Cleanup**. Each item has a tier:

- **Likely safe**: caches and build output that tools recreate, such as Xcode DerivedData or a package manager's download cache.
- **Review first**: things that come back but cost something, such as simulators or a project's dependencies.
- **Keep**: data you would lose, shown so you know where the space is, never suggested for removal.

Tick what you want gone and click **Review cleanup**. The review lists every item, its size and how it will be removed. Items go to the Trash first, so you can still change your mind until you empty it.

## Go further

- **Storage** lets you browse folders by size, see your own files by type, and find the largest files.
- **Duplicates** finds identical copies and shows a preview of each before you choose which to keep.
- **Docker** lists images, containers, volumes and build cache, and removes them through Docker itself.
- **History** shows every cleanup and how much space you have cleared.

Press <kbd>⌘</kbd><kbd>/</kbd> in the app to see every [keyboard shortcut](/shortcuts).
