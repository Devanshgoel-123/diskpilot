---
title: DiskPilot privacy
h1: Privacy
description: What DiskPilot sends from your Mac, why, and how to have your email changed or deleted.
updated: 2026-11-12
---

DiskPilot scans, analyses and cleans your Mac on the Mac itself. Nothing about your files, folders or scans leaves it. Three things do use the network, and this page lists each one.

## The email signup

From version 0.6.5, DiskPilot asks for your email address the first time it opens, before you can use it. It then sends this to `diskpilot.live`:

- your email address
- the DiskPilot version, for example 0.6.5
- the macOS version, for example 15.1
- whether the Mac has Apple silicon or an Intel processor

It is sent once, when you sign up. If the Mac is offline at that moment, DiskPilot keeps it on the Mac and sends it the next time it opens. A copy stays in DiskPilot's data folder (`~/Library/Application Support/dev.diskpilot.app/signup.json`), readable only by your user account, so you are not asked again.

**Why:** DiskPilot has no accounts, so before 0.6.5 there was no way to tell people that a fix was out or that their copy was outdated, or to ask what they would like changed. The email is used for that: news about new versions and fixes, and the occasional request for feedback. A few emails a year at most.

**What it is not used for:** it is not sold, shared or used for advertising, and it is not joined with anything about your files or how you use the app, because DiskPilot never sends that.

**Where it is kept:** in a database on Vercel, the host of diskpilot.live, which only the maker of DiskPilot can read. It is kept until you ask for it to be deleted.

**Changing or deleting it:** write to [devanshgoel.dev@gmail.com](mailto:devanshgoel.dev@gmail.com) from that address, and it will be changed or deleted. Every email also says how to stop receiving them.

## Update checks

Once a day DiskPilot asks GitHub whether a newer version is out, and downloads it if you choose to install it. GitHub sees that request like any other visit to a public file. DiskPilot sends nothing else with it.

## The optional assistant

The assistant is off until you add your own API key. When you ask it a question, the question and the storage information needed to answer it (folder and file names, sizes, categories) go to the AI provider you chose, under that provider's terms. Private folders such as Mail and Messages appear only as one total, never by name. Nothing is sent when you don't use it.

## This website

diskpilot.live counts visits with Vercel Web Analytics, which uses no cookies and does not identify you. The directory badges at the bottom of the page load from those directories.
