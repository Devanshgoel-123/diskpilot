---
title: How to clear Xcode DerivedData, simulators and device support
h1: How to clear Xcode's storage on a Mac
nav: Clear Xcode storage
description: What Xcode's DerivedData, simulators, device support files and archives are, which are safe to delete, and the commands that clean them.
summary: DerivedData, simulators, device support and archives: what to delete and what to keep.
order: 4
updated: 2026-11-09
---

Xcode can use more space than any other app on a developer's Mac, almost all of it in `~/Library/Developer`. Some of it is safe to delete any time. Some of it you will want back.

## DerivedData: safe to delete

`~/Library/Developer/Xcode/DerivedData` holds intermediate build products and indexes. Xcode recreates it on the next build, which takes longer than usual. Delete the folder of a project you no longer work on, or the whole thing:

```sh
rm -rf ~/Library/Developer/Xcode/DerivedData
```

Quit Xcode first. Projects with a custom build location keep a `DerivedData` folder next to the project too.

## Simulators: review first

Simulator devices and runtimes live in `~/Library/Developer/CoreSimulator`. Each device keeps its installed apps and data, so check that none holds test state you need. Remove simulators for runtimes that are no longer installed:

```sh
xcrun simctl delete unavailable
```

List and remove whole runtimes, which are often several gigabytes each (Xcode 15 or later):

```sh
xcrun simctl runtime list
xcrun simctl runtime delete <identifier>
```

## iOS device support: safe, but it comes back

`~/Library/Developer/Xcode/iOS DeviceSupport` holds debug symbols copied from each device and OS version you connected. Delete folders for versions you no longer test on; Xcode copies them again the next time you connect a device with that version.

## Archives: keep

`~/Library/Developer/Xcode/Archives` holds the builds you distributed, with the dSYM debug symbols needed to read crash reports from them. They are not recreated. Keep the archives of any version still in use, and export the dSYMs before deleting older ones.

## With DiskPilot

DiskPilot recognises each of these folders and tiers them: DerivedData as **Likely safe**, simulators and device support as **Review first**, archives as **Keep**. It also finds DerivedData folders in custom build locations. You review every item before it moves to the Trash.
