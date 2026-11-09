---
title: Find and delete old node_modules folders on a Mac
h1: Find and delete old node_modules folders
nav: Delete old node_modules
description: How to find every node_modules folder on your Mac, see how much space they use, and remove the ones from projects you no longer work on.
summary: Find every node_modules folder, see what each costs, and remove old ones.
order: 5
updated: 2026-11-09
---

Every JavaScript project keeps its dependencies in a `node_modules` folder, often several hundred megabytes each. A year of side projects, tutorials and cloned repositories can add up to tens of gigabytes.

## Is it safe to delete?

Yes, as long as the project has a lockfile (`package-lock.json`, `pnpm-lock.yaml` or `yarn.lock`). Running the install command again restores the same packages. Until you do, the project will not run.

Also worth clearing: build output such as Next.js's `.next` folder, which the next build recreates.

## Find them from Terminal

List every `node_modules` folder under your home folder with its size, largest last:

```sh
find ~ -name node_modules -type d -prune -exec du -sh {} + 2>/dev/null | sort -h
```

Delete one:

```sh
rm -rf ~/code/old-project/node_modules
```

Command-line tools such as `npkill` make this interactive; see [DiskPilot vs npkill and kondo](/compare/npkill-kondo).

## Find them with DiskPilot

DiskPilot finds every `node_modules` folder during its scan. Cleanup lists those of 50 MB or more under **Review first**, each with its project and size, alongside build output for Next.js, Rust, Python, Gradle and other tools. Stop any dev server running from a project first, select the folders you no longer need, review the list and move them to the Trash. Package manager caches such as npm's and pnpm's are listed separately, since clearing them affects every project.

## Keep them from coming back

- With **pnpm**, packages are stored once on disk and linked into each project, so many projects cost far less space.
- Delete `node_modules` when you archive a project; the lockfile is enough to restore it.
