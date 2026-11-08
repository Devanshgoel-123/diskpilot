/** Drive helpers for the Windows drive picker and per-drive capacity. */
import { isInside, samePath, splitPath } from "./paths";
import type { DiskInfo, DriveInfo } from "./types";

/** How File Explorer names a drive: "Data (D:)", or "Local Disk (C:)" without a label. */
export function driveName(d: DriveInfo): string {
  const letter = splitPath(d.root).root.replace(/\\$/, "");
  const label = d.label.trim() || (d.kind === "removable" ? "USB Drive" : "Local Disk");
  return letter && letter !== "/" ? `${label} (${letter})` : label || d.root;
}

/** The drive that holds `path`, if it is listed. */
export function driveFor(path: string | null | undefined, drives: DriveInfo[]): DriveInfo | null {
  if (!path) return null;
  return drives.find((d) => samePath(d.root, path) || isInside(path, d.root)) ?? null;
}

/** True when `root` is a whole drive rather than a folder on it. */
export function isDriveRoot(root: string | null | undefined, drives: DriveInfo[]): boolean {
  return !!root && drives.some((d) => samePath(d.root, root));
}

/** A drive's capacity in the shape the storage strip uses. */
export function driveAsDisk(d: DriveInfo): DiskInfo {
  return {
    mountPoint: d.root,
    fileSystem: d.fileSystem,
    totalBytes: d.totalBytes,
    availableBytes: d.availableBytes,
    usedBytes: d.usedBytes,
  };
}
