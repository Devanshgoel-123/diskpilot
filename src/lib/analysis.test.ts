import { describe, expect, it } from "vitest";

import {
  groupByTool,
  isInside,
  outermost,
  regenerableBytes,
  summarizeSelection,
  topConsumers,
} from "./analysis";
import type { DetectedLocation } from "./types";

function loc(path: string, bytes: number, ruleId: string, container = false): DetectedLocation {
  return {
    path,
    bytes,
    fileCount: 1,
    container,
    classification: {
      category: "cache",
      label: ruleId,
      explanation: "x",
      ruleId,
      confidence: "high",
      matchedPath: path,
    },
  };
}

describe("isInside", () => {
  it("compares path components, not prefixes", () => {
    expect(isInside("/a/b/c", "/a/b")).toBe(true);
    expect(isInside("/a/bc", "/a/b")).toBe(false);
    expect(isInside("/a/b", "/a/b")).toBe(false);
  });
});

describe("outermost", () => {
  it("removes nested items so sums do not double count", () => {
    const items = [loc("/h/.nvm", 10, "nvm.home"), loc("/h/.nvm/versions", 8, "nvm.versions")];
    expect(outermost(items).map((i) => i.path)).toEqual(["/h/.nvm"]);
  });
});

describe("regenerableBytes", () => {
  it("sums only cache and build artifacts", () => {
    expect(
      regenerableBytes([
        { category: "cache", bytes: 5, fileCount: 1 },
        { category: "buildArtifacts", bytes: 3, fileCount: 1 },
        { category: "userData", bytes: 100, fileCount: 1 },
      ]),
    ).toBe(8);
  });
});

describe("topConsumers", () => {
  it("skips containers and sorts by size", () => {
    const result = topConsumers(
      [
        loc("/h/Library", 100, "library", true),
        loc("/h/a/node_modules", 5, "node.node-modules"),
        loc("/h/.npm", 9, "npm.cache"),
      ],
      5,
    );
    expect(result.map((r) => r.path)).toEqual(["/h/.npm", "/h/a/node_modules"]);
  });
});

describe("groupByTool", () => {
  it("groups by rule prefix without double counting", () => {
    const groups = groupByTool([
      loc("/h/.nvm", 10, "nvm.home", true),
      loc("/h/.nvm/versions", 8, "nvm.versions"),
      loc("/h/p/node_modules", 4, "node.node-modules"),
      loc("/h/Library/Containers/com.docker.docker", 50, "docker.desktop-vm"),
    ]);
    const node = groups.find((g) => g.id === "node");
    expect(node?.bytes).toBe(14);
    expect(node?.locations).toHaveLength(3);
    expect(groups[0]?.id).toBe("docker");
  });
});

describe("summarizeSelection", () => {
  const rec = (id: string, bytes: number, methods: ("trash" | "delete")[]) =>
    ({ id, estimatedReclaimBytes: bytes, methods }) as unknown as import("./types").Recommendation;
  const recs = [rec("a", 10, ["trash", "delete"]), rec("b", 5, ["trash"]), rec("c", 99, [])];

  it("offers permanent delete only when every selected item allows it", () => {
    expect(summarizeSelection(recs, new Set(["a"])).methods).toEqual(["trash", "delete"]);
    expect(summarizeSelection(recs, new Set(["a", "b"])).methods).toEqual(["trash"]);
  });
  it("ignores non-actionable items in totals", () => {
    const s = summarizeSelection(recs, new Set(["a", "b", "c"]));
    expect(s.count).toBe(2);
    expect(s.bytes).toBe(15);
  });
  it("handles an empty selection", () => {
    expect(summarizeSelection(recs, new Set())).toEqual({ count: 0, bytes: 0, methods: [] });
  });
});
