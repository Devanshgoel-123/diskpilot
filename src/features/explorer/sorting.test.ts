import { describe, expect, it } from "vitest";

import type { DirectoryEntry } from "@/lib/types";

import { sortEntries } from "./sorting";

function entry(name: string, bytes: number | null): DirectoryEntry {
  return {
    name,
    path: `/x/${name}`,
    kind: "file",
    bytes,
    fileCount: null,
    modifiedMs: null,
    extension: null,
    classification: {
      category: "unknown",
      label: "",
      explanation: "",
      ruleId: "unmatched",
      confidence: "low",
      matchedPath: null,
    },
  };
}

describe("sortEntries", () => {
  const items = [entry("b", 10), entry("a10", null), entry("a2", 30)];
  it("sorts by size with unknown sizes last", () => {
    expect(sortEntries(items, "size", true).map((e) => e.name)).toEqual(["a2", "b", "a10"]);
    expect(sortEntries(items, "size", false).map((e) => e.name)).toEqual(["b", "a2", "a10"]);
  });
  it("sorts names naturally", () => {
    expect(sortEntries(items, "name", false).map((e) => e.name)).toEqual(["a2", "a10", "b"]);
  });
  it("does not mutate the input", () => {
    sortEntries(items, "name", false);
    expect(items[0]?.name).toBe("b");
  });
});
