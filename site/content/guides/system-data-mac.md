---
title: What is System Data on a Mac, and how to shrink it
h1: What is System Data on a Mac?
nav: What is System Data?
description: Why macOS shows tens of gigabytes as System Data, what is usually inside it, and how to find and clear the parts that are safe to remove.
summary: What macOS puts under System Data, and how to find the parts you can clear.
order: 2
updated: 2026-11-09
---

Open **System Settings > General > Storage** and you will often find a large grey bar called **System Data**. macOS puts there everything it does not file under Applications, Documents, Photos and its other named categories. That makes it the place where most hidden storage ends up, and macOS does not say what is in it.

## What is usually inside

On most Macs, System Data is a mix of:

- **Caches** in `~/Library/Caches` and inside apps' own folders. Browsers, chat apps and package managers keep downloads here so they do not fetch them again.
- **Developer data** in `~/Library/Developer`: Xcode DerivedData, simulators and runtimes, and iOS device support files.
- **Virtual machine disks**, including Docker Desktop's and OrbStack's. One file can hold every image, container and volume you have.
- **App data** in `~/Library/Application Support` and `~/Library/Containers`, such as editor workspace storage and AI tools' local data.
- **Logs** and crash reports.
- **Local Time Machine snapshots**, which macOS keeps when you back up and removes on its own when space runs low. To see them, run `tmutil listlocalsnapshots /` in Terminal.

## Why it grows

Most of these folders grow without limit until something clears them. A single Xcode project can leave gigabytes of DerivedData. Every `docker pull` adds to Docker's disk. Caches for editors and AI tools are new enough that few cleanup tools know about them.

## How to see what's in yours

Finder hides `~/Library` and does not show folder sizes by default, so it is slow to look by hand. DiskPilot scans your home folder and names each large folder it recognises, with the rule that matched it and how sure it is. Folders it does not recognise are shown by size and marked as unknown, never suggested for removal.

## What is safe to clear

Caches and build output are designed to be recreated, so clearing them costs only time: the next build or launch is slower. Some things look like caches but are not:

- **Xcode archives** hold the debug symbols you need to read crash reports from shipped builds.
- **iPhone and iPad backups** may be the only copy of a device's photos and messages.
- **Docker volumes** often hold databases. Manage them through Docker, never by deleting the disk image.
- **Conversation history** in AI tools and **sign-in data** in apps are yours, not cache.

DiskPilot never suggests removing any of these, refuses system folders outright, and moves what you choose to the Trash first.

## Clearing it

1. [Install DiskPilot](/guides/getting-started) and run a scan.
2. Open **Cleanup** and start with **Likely safe**.
3. Review the list and confirm. Empty the Trash when you are sure; that is when macOS shows the space as free.

System Data in Storage settings updates on its own schedule, so it can take a few minutes to reflect what you cleared.
