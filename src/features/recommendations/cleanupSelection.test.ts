import { describe, expect, it } from "vitest";

import type { Recommendation } from "@/lib/types";

import { applyRangeSelection } from "./cleanupSelection";

function item(id: string, actionable = true): Recommendation {
  return {
    id,
    title: id,
    path: `/tmp/${id}`,
    bytes: 1,
    fileCount: 1,
    tier: "likelySafe",
    confidence: "high",
    classification: {
      category: "cache",
      label: id,
      explanation: "",
      ruleId: "test",
      confidence: "high",
      matchedPath: null,
    },
    detectedBecause: "",
    safeBecause: "",
    risks: "",
    proposedAction: "",
    methods: actionable ? ["trash"] : [],
    alternative: null,
    estimatedReclaimBytes: 1,
    lastActivityMs: null,
    notes: [],
    source: "scan",
  };
}

const visible = [item("a"), item("b"), item("keep", false), item("c"), item("d")];

describe("applyRangeSelection", () => {
  it("selects every actionable row between the anchor and the target, downward", () => {
    const next = applyRangeSelection(visible, "a", "d", true, new Set());
    expect([...next]).toEqual(["a", "b", "c", "d"]);
  });

  it("selects the range upward", () => {
    const next = applyRangeSelection(visible, "d", "a", true, new Set());
    expect([...next]).toEqual(["a", "b", "c", "d"]);
  });

  it("clears the range when the target was unchecked", () => {
    const next = applyRangeSelection(
      visible,
      "a",
      "d",
      false,
      new Set(["a", "b", "c", "d", "outside"]),
    );
    expect([...next]).toEqual(["outside"]);
  });

  it("takes the clicked row's state, even when the anchor has the opposite state", () => {
    const next = applyRangeSelection(visible, "b", "d", false, new Set(["a", "b"]));
    expect([...next]).toEqual(["a"]);
  });

  it("changes only the target when the anchor is not in the visible list", () => {
    const current = new Set(["a"]);
    const next = applyRangeSelection(visible, "gone", "c", true, current);
    expect([...next]).toEqual(["a", "c"]);
  });

  it("does not select rows that have no automated action", () => {
    const next = applyRangeSelection(visible, "b", "d", true, new Set());
    expect(next.has("keep")).toBe(false);
  });
});
