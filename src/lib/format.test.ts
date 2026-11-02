import { describe, expect, it } from "vitest";

import {
  basename,
  formatBytes,
  formatDuration,
  formatPercent,
  formatRelative,
  tildify,
} from "./format";

describe("formatBytes", () => {
  it("uses decimal units like Finder", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1000)).toBe("1.0 KB");
    expect(formatBytes(1_500_000)).toBe("1.5 MB");
    expect(formatBytes(245_000_000_000)).toBe("245 GB");
    expect(formatBytes(2_000_000_000_000)).toBe("2.0 TB");
  });
  it("shows a dash for unknown sizes", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(undefined)).toBe("—");
  });
});

describe("formatPercent", () => {
  it("handles edge cases", () => {
    expect(formatPercent(1, 0)).toBe("0%");
    expect(formatPercent(1, 10_000)).toBe("<0.1%");
    expect(formatPercent(5, 100)).toBe("5.0%");
    expect(formatPercent(42, 100)).toBe("42%");
  });
});

describe("formatDuration", () => {
  it("scales units", () => {
    expect(formatDuration(250)).toBe("250 ms");
    expect(formatDuration(4_200)).toBe("4.2 s");
    expect(formatDuration(42_000)).toBe("42 s");
    expect(formatDuration(125_000)).toBe("2m 5s");
  });
});

describe("formatRelative", () => {
  const now = 1_700_000_000_000;
  it("describes past times", () => {
    expect(formatRelative(now - 10_000, now)).toBe("just now");
    expect(formatRelative(now - 5 * 60_000, now)).toBe("5 minutes ago");
    expect(formatRelative(now - 3 * 86_400_000, now)).toBe("3 days ago");
    expect(formatRelative(null, now)).toBe("—");
  });
});

describe("paths", () => {
  it("tildifies only real home prefixes", () => {
    expect(tildify("/Users/a/Library", "/Users/a")).toBe("~/Library");
    expect(tildify("/Users/a", "/Users/a")).toBe("~");
    expect(tildify("/Users/ab/x", "/Users/a")).toBe("/Users/ab/x");
    expect(tildify("/tmp", undefined)).toBe("/tmp");
  });
  it("extracts basenames", () => {
    expect(basename("/a/b/c.txt")).toBe("c.txt");
    expect(basename("/a/b/")).toBe("b");
  });
});
