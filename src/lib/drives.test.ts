import { describe, expect, it } from "vitest";

import { driveAsDisk, driveFor, driveName, isDriveRoot } from "./drives";
import type { DriveInfo } from "./types";

const drive = (root: string, label: string, kind: DriveInfo["kind"] = "fixed"): DriveInfo => ({
  root,
  label,
  fileSystem: "NTFS",
  totalBytes: 1000,
  availableBytes: 400,
  usedBytes: 600,
  kind,
});

const c = drive("C:\\", "");
const d = drive("D:\\", "Data");
const e = drive("E:\\", "", "removable");
const drives = [c, d, e];

describe("drives", () => {
  it("names drives the way File Explorer does", () => {
    expect(driveName(c)).toBe("Local Disk (C:)");
    expect(driveName(d)).toBe("Data (D:)");
    expect(driveName(e)).toBe("USB Drive (E:)");
  });

  it("finds the drive that holds a path", () => {
    expect(driveFor("C:\\Users\\Dev", drives)?.root).toBe("C:\\");
    expect(driveFor("d:\\games", drives)?.root).toBe("D:\\");
    expect(driveFor("D:\\", drives)?.root).toBe("D:\\");
    expect(driveFor("F:\\x", drives)).toBeNull();
    expect(driveFor(null, drives)).toBeNull();
  });

  it("tells a whole drive from a folder on it", () => {
    expect(isDriveRoot("D:\\", drives)).toBe(true);
    expect(isDriveRoot("d:\\", drives)).toBe(true);
    expect(isDriveRoot("D:\\Data", drives)).toBe(false);
    expect(isDriveRoot(null, drives)).toBe(false);
  });

  it("converts to the storage strip's shape", () => {
    expect(driveAsDisk(d)).toEqual({
      mountPoint: "D:\\",
      fileSystem: "NTFS",
      totalBytes: 1000,
      availableBytes: 400,
      usedBytes: 600,
    });
  });
});
