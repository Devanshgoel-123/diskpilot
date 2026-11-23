import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { useApp } from "@/lib/store";
import type { Recommendation, ScanSummary, Tier } from "@/lib/types";

import { RecommendationsPage } from "./RecommendationsPage";

function rec(title: string, tier: Tier, over: Partial<Recommendation> = {}): Recommendation {
  return {
    id: `/Users/t/${title}`,
    title,
    path: `/Users/t/${title}`,
    bytes: 1_000,
    fileCount: 1,
    tier,
    confidence: "high",
    classification: {
      category: "cache",
      label: title,
      explanation: "",
      ruleId: "test",
      confidence: "high",
      matchedPath: null,
    },
    detectedBecause: "",
    safeBecause: "",
    risks: "",
    proposedAction: "",
    methods: tier === "keep" ? [] : ["trash"],
    alternative: null,
    estimatedReclaimBytes: 1_000,
    lastActivityMs: null,
    notes: [],
    source: "scan",
    clearInApp: null,
    ...over,
  };
}

const installer = { marker: { kind: "installer" as const, detail: "Installed already" } };

// Shown in the All view as: s1 s2 s3 s4 | r1 r2 (manual) r3 r4 | k1
const recommendations = [
  rec("s1", "likelySafe"),
  rec("s2", "likelySafe", installer),
  rec("s3", "likelySafe"),
  rec("s4", "likelySafe", installer),
  rec("r1", "reviewFirst", installer),
  rec("r2", "reviewFirst", { methods: [] }),
  rec("r3", "reviewFirst"),
  rec("r4", "reviewFirst"),
  rec("k1", "keep"),
];

const box = (title: string) => screen.getByRole("checkbox", { name: `Select ${title}` });
const click = (title: string) => fireEvent.click(box(title));
const shiftClick = (title: string) => fireEvent.click(box(title), { shiftKey: true });
const checked = () =>
  screen
    .getAllByRole("checkbox")
    .filter((b) => (b as HTMLInputElement).checked)
    .map((b) => b.getAttribute("aria-label") ?? "")
    // The section headers' own "Select all" boxes.
    .filter((label) => !label.startsWith("Select all"))
    .map((label) => label.replace("Select ", ""));

beforeEach(() => {
  useApp.setState({
    summary: { id: "s" } as ScanSummary,
    recommendations,
    scanning: false,
    preselect: null,
  });
  render(<RecommendationsPage />);
});
afterEach(cleanup);

describe("shift-click range selection", () => {
  it("selects a range downward", () => {
    click("s1");
    shiftClick("s3");
    expect(checked()).toEqual(["s1", "s2", "s3"]);
  });

  it("selects a range upward", () => {
    click("s4");
    shiftClick("s2");
    expect(checked()).toEqual(["s2", "s3", "s4"]);
  });

  it("spans tiers in the All view and skips rows without a checkbox", () => {
    click("s3");
    shiftClick("r3");
    expect(checked()).toEqual(["s3", "s4", "r1", "r3"]);
    expect(screen.queryByRole("checkbox", { name: "Select r2" })).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "Select k1" })).toBeNull();
  });

  it("clears a range when the clicked row is unchecked", () => {
    click("s1");
    shiftClick("r3");
    expect(checked()).toEqual(["s1", "s2", "s3", "s4", "r1", "r3"]);
    click("s2");
    shiftClick("r1");
    expect(checked()).toEqual(["s1", "r3"]);
  });

  it("follows the tier tab", () => {
    fireEvent.click(screen.getByRole("tab", { name: /Review first/ }));
    expect(screen.queryByRole("checkbox", { name: "Select s1" })).toBeNull();
    click("r4");
    shiftClick("r1");
    expect(checked()).toEqual(["r1", "r3", "r4"]);
    fireEvent.click(screen.getByRole("tab", { name: /^All/ }));
    expect(checked()).toEqual(["r1", "r3", "r4"]);
  });

  it("follows the No longer needed filter", () => {
    fireEvent.click(screen.getByRole("button", { name: /Installers/ }));
    click("s2");
    shiftClick("r1");
    expect(checked()).toEqual(["s2", "s4", "r1"]);
    fireEvent.click(screen.getByRole("button", { name: /Installers/ }));
    expect(checked()).toEqual(["s2", "s4", "r1"]);
  });

  it("Clear resets the anchor", () => {
    click("s1");
    fireEvent.click(screen.getByRole("button", { name: "Clear selection" }));
    shiftClick("s3");
    expect(checked()).toEqual(["s3"]);
    // The shift-click itself set a new anchor.
    shiftClick("s1");
    expect(checked()).toEqual(["s1", "s2", "s3"]);
  });
});

describe("selecting a whole section", () => {
  const all = (name: string) => screen.getByRole("checkbox", { name }) as HTMLInputElement;

  it("selects every item of the section from its header, then clears them", () => {
    fireEvent.click(all("Select all 4 likely safe"));
    expect(checked()).toEqual(["s1", "s2", "s3", "s4"]);
    expect(all("Select all 4 likely safe").checked).toBe(true);

    fireEvent.click(all("Select all 4 likely safe"));
    expect(checked()).toEqual([]);
  });

  it("shows a dash when only some are selected, and leaves other sections alone", () => {
    click("r3");
    click("s1");
    const safe = all("Select all 4 likely safe");
    expect(safe.checked).toBe(false);
    expect(safe.indeterminate).toBe(true);

    fireEvent.click(safe);
    expect(checked()).toEqual(["s1", "s2", "s3", "s4", "r3"]);
  });

  it("counts only items that can be removed", () => {
    // r2 has no automated action, and Keep has none at all.
    expect(all("Select all 3 review first")).toBeVisible();
    expect(screen.queryByRole("checkbox", { name: /Select all .* keep/ })).toBeNull();
  });
});
