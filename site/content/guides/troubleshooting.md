---
title: DiskPilot troubleshooting and common questions
h1: Troubleshooting
nav: Troubleshooting
description: Fixes for the questions people hit most with DiskPilot: permission prompts, skipped folders, the Trash, updates and Homebrew.
summary: Permission prompts, skipped folders, the Trash, updates and Homebrew.
order: 6
updated: 2026-11-10
---

## macOS keeps asking for permission

macOS asks once per protected folder, such as Desktop, Documents and Downloads, before any app reads it. Allow DiskPilot to measure those folders, or deny it and they are skipped. You can change your answer in **System Settings > Privacy & Security > Files and Folders**.

## Some folders are listed as skipped

Folders such as Mail, Messages and the Photos library need Full Disk Access, which DiskPilot does not ask for. They are listed as skipped so the totals stay honest. To include them, grant Full Disk Access in **System Settings > Privacy & Security**. DiskPilot reads sizes only and keeps them on your Mac.

## The space did not come back after a cleanup

Items go to the Trash first. The space is freed when you empty it, which you can do from DiskPilot. Storage in System Settings also updates on its own schedule, so it can lag by a few minutes.

## Finder doesn't offer Put Back

From version 0.6.3, DiskPilot moves items to the Trash itself instead of asking Finder, so it is quiet and much faster. The items are in the Trash as usual, but Finder may not offer Put Back for them. To restore one, drag it out of the Trash to where you want it.

## Smaller files are missing from Files by type

From version 0.6.2, files of 25 KB or more are listed. Results saved by earlier versions listed only files of 1 MB or more, so scan again once after updating.

## Updates

From version 0.6, DiskPilot checks GitHub once a day and shows a banner when a new version is ready. Click Update to install it and restart. To check now, or to turn the daily check off, open **Settings > Updates**. Coming from 0.5.1 or earlier, download the new version once by hand.

## Homebrew says the tap is not trusted

Homebrew asks you to trust a new tap once:

```sh
brew trust devanshgoel-123/tap
```

After that, `brew upgrade --cask diskpilot` works, though DiskPilot also updates itself.

## Something else

[Open an issue on GitHub](https://github.com/Devanshgoel-123/diskpilot/issues/new) with your macOS version, your DiskPilot version (in **Settings > About**) and what you expected to happen. Scan results stay on your Mac, so include a screenshot if it helps.
