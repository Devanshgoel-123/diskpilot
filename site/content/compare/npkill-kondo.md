---
title: DiskPilot vs npkill and kondo for cleaning projects
h1: DiskPilot vs npkill and kondo
nav: vs npkill and kondo
description: npkill and kondo are command-line tools that delete node_modules and build output from your projects. DiskPilot does that in a Mac app, alongside Docker, Xcode and app caches.
summary: Fast command-line project cleaners, or a Mac app that covers projects and more.
order: 5
updated: 2026-11-09
---

We make DiskPilot, so we have a bias. npkill and kondo are free, open-source command-line tools that are excellent at their job. See [npkill](https://github.com/voidcosmos/npkill) and [kondo](https://github.com/tbillington/kondo) for current details.

## The short version

**npkill** finds `node_modules` folders and lets you delete them from an interactive list in your terminal. Run it with `npx npkill`.

**kondo** cleans build output and dependencies from many kinds of projects, such as Rust, Node, Unity and Gradle, from the command line.

**DiskPilot** finds the same project folders during its scan, and also covers what lives outside your projects: Docker, Xcode, package manager caches, editors and AI tools. It is a Mac app with a review step rather than a terminal tool.

## Side by side

|                                             | DiskPilot                    | npkill                | kondo                 |
| ------------------------------------------- | ---------------------------- | --------------------- | --------------------- |
| Interface                                   | Mac app                      | Terminal              | Terminal              |
| Price and source                            | Free, MIT                    | Free, open source     | Free, open source     |
| `node_modules`                              | Yes                          | Yes                   | Yes                   |
| Rust, Python, Gradle and other build output | Yes                          | No                    | Yes                   |
| Docker, Xcode, caches, editors, AI tools    | Yes                          | No                    | No                    |
| Platforms                                   | macOS                        | macOS, Linux, Windows | macOS, Linux, Windows |
| Removing things                             | Review, Trash first, history | Deletes directly      | Deletes directly      |

## npkill or kondo is better if

- You live in the **terminal** and want something you can run in seconds.
- You work on **Linux or Windows** too.
- You only want to clean **project folders**.

## DiskPilot is better if

- Project folders are only part of the problem, and the rest is in `~/Library` and Docker.
- You'd rather **review** a list and have things go to the Trash first than delete directly.

See also: [Find and delete old node_modules folders](/guides/delete-node-modules).
