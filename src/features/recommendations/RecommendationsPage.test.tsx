import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CleanupRecord, Recommendation } from "@/lib/types";

vi.mock("@/lib/api", () => ({
  api: {
    openOwnerApp: () => Promise.resolve(null),
    revealInFinder: () => Promise.resolve(null),
    getAppBadges: () => Promise.resolve({}),
    createCleanupPlan: () => Promise.resolve(null),
    discardCleanupPlan: () => Promise.resolve(),
  },
  errorMessage: (e: unknown) => String(e),
  onCleanupProgress: () => Promise.resolve(() => {}),
}));

vi.mock("@/features/trash/EmptyTrashButton", () => ({
  EmptyTrashButton: () => null,
}));

// The real dialog plans and executes before calling onFinished. Here we only
// need to drive that callback so the page's post-cleanup selection state can
// be asserted without exercising the engine.
vi.mock("./CleanupDialog", () => ({
  CleanupDialog: ({ onFinished }: { onFinished: (record: CleanupRecord) => void }) => (
    <button type="button" onClick={() => onFinished({ id: "done" } as CleanupRecord)}>
      Finish cleanup
    </button>
  ),
}));

import { useApp } from "@/lib/store";

import { RecommendationsPage } from "./RecommendationsPage";

function item(
  id: string,
  tier: Recommendation["tier"] = "likelySafe",
  extra: Partial<Recommendation> = {},
): Recommendation {
  return {
    id,
    title: `Item ${id}`,
    path: `/tmp/${id}`,
    bytes: 100,
    fileCount: 1,
    tier,
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
    methods: ["trash"],
    alternative: null,
    estimatedReclaimBytes: 100,
    lastActivityMs: null,
    notes: [],
    source: "scan",
    ...extra,
  };
}

const recs = [item("a"), item("b"), item("c"), item("d", "reviewFirst")];

let consoleErrorCalls: unknown[][];
let consoleErrorSpy: { mockRestore: () => void };

beforeEach(() => {
  useApp.setState({
    summary: { id: "scan", startedAtMs: 1 } as never,
    recommendations: recs,
    scanning: false,
    preselect: null,
  });
  const spy = vi.spyOn(console, "error").mockImplementation(() => {});
  consoleErrorSpy = spy;
  consoleErrorCalls = spy.mock.calls;
});
afterEach(() => {
  // A controlled input must always have an onChange handler and a change
  // listener. Any React warning here means that contract was broken again.
  const controlled = consoleErrorCalls
    .map((call) => String(call[0]))
    .filter(
      (message) =>
        message.includes("controlled") ||
        message.includes("onChange") ||
        message.includes("read-only field") ||
        message.includes("defaultChecked"),
    );
  expect(controlled).toEqual([]);
  consoleErrorSpy.mockRestore();
  cleanup();
});

const boxes = () => screen.getAllByRole("checkbox") as HTMLInputElement[];
const box = (index: number): HTMLInputElement => {
  const found = boxes()[index];
  if (!found) throw new Error(`missing checkbox at ${index}`);
  return found;
};
const clickBox = (index: number, shiftKey = false) => fireEvent.click(box(index), { shiftKey });

describe("shift-click range selection on Cleanup", () => {
  it("selects the rows between the anchor and the target", () => {
    render(<RecommendationsPage />);

    clickBox(0);
    clickBox(2, true);

    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(true);
    expect(box(2).checked).toBe(true);
    expect(box(3).checked).toBe(false);
  });

  it("clears the range when the target is unchecked", () => {
    render(<RecommendationsPage />);

    clickBox(0);
    clickBox(2, true);
    clickBox(0, true);

    expect(box(0).checked).toBe(false);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(false);
  });

  it("works upward across tiers", () => {
    render(<RecommendationsPage />);

    clickBox(3);
    clickBox(1, true);

    expect(box(0).checked).toBe(false);
    expect(box(1).checked).toBe(true);
    expect(box(2).checked).toBe(true);
    expect(box(3).checked).toBe(true);
  });

  it("uses a plain click as the new anchor after a range", () => {
    render(<RecommendationsPage />);

    clickBox(0);
    clickBox(2, true);
    clickBox(3);

    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(true);
    expect(box(2).checked).toBe(true);
    expect(box(3).checked).toBe(true);

    clickBox(1, true);
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(false);
    expect(box(3).checked).toBe(false);
  });

  it("starts a new anchor after Clear selection", () => {
    render(<RecommendationsPage />);

    clickBox(0);
    clickBox(2, true);
    fireEvent.click(screen.getByRole("button", { name: "Clear selection" }));

    clickBox(3, true);
    expect(box(0).checked).toBe(false);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(false);
    expect(box(3).checked).toBe(true);
  });

  it("restricts the range to the rows shown by the tier filter", () => {
    render(<RecommendationsPage />);
    fireEvent.click(screen.getByRole("tab", { name: /Review first/ }));
    expect(boxes()).toHaveLength(1);

    clickBox(0, true);
    expect(box(0).checked).toBe(true);
  });

  it("keeps a hidden anchor out of the range when the filter changes", () => {
    // Two items in each of two tiers, so the anchor can be set on All, hidden
    // by a tier tab and then revealed again while another range is selected.
    useApp.setState({
      recommendations: [item("a"), item("b"), item("c", "reviewFirst"), item("d", "reviewFirst")],
    });
    render(<RecommendationsPage />);

    // Anchor on a Likely safe row, then hide it behind the Review first tab.
    clickBox(0);
    fireEvent.click(screen.getByRole("tab", { name: /Review first/ }));
    expect(boxes()).toHaveLength(2);

    // Set a fresh in-tab anchor and shift-click across the two visible rows.
    // The hidden Likely safe rows must not join this range.
    clickBox(1);
    clickBox(0, true);
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(true);

    // Back on All, the row hidden by the tab throughout the range actions is
    // still unchecked.
    fireEvent.click(screen.getByRole("tab", { name: /^All/ }));
    expect(boxes()).toHaveLength(4);
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(true);
    expect(box(3).checked).toBe(true);
  });

  it("skips non-actionable rows inside a shift-click range", () => {
    useApp.setState({
      recommendations: [
        item("a"),
        item("b", "likelySafe", { methods: [], title: "Item b (manual)" }),
        item("c"),
        item("d"),
      ],
    });
    render(<RecommendationsPage />);

    // The manual row has no checkbox, so only a/b/c/d actionable rows are
    // counted. Anchor on the first, shift-click the last.
    expect(boxes()).toHaveLength(3);
    clickBox(0);
    clickBox(2, true);

    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(true);
    expect(box(2).checked).toBe(true);
    expect(screen.queryByRole("checkbox", { name: "Select Item b (manual)" })).toBeNull();
  });

  it("restricts the range to rows matching the active kind filter", () => {
    useApp.setState({
      recommendations: [
        item("a", "likelySafe", { marker: { kind: "olderVersion", detail: "old" } }),
        item("b", "likelySafe"),
        item("c", "likelySafe", { marker: { kind: "olderVersion", detail: "old" } }),
        item("d", "likelySafe"),
      ],
    });
    render(<RecommendationsPage />);

    fireEvent.click(screen.getByRole("button", { name: /Older versions/ }));
    expect(boxes()).toHaveLength(2);

    // Both visible rows are marked, but with only two rows the range covers
    // them both. Hidden b/d must stay unchecked when the filter is removed.
    clickBox(0);
    clickBox(1, true);
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Show all" }));
    expect(boxes()).toHaveLength(4);
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(true);
    expect(box(3).checked).toBe(false);
  });

  it("resets the anchor after Select all likely safe", () => {
    render(<RecommendationsPage />);

    // Set an anchor on the first row before Select all replaces the selection.
    clickBox(0);
    fireEvent.click(screen.getByRole("button", { name: "Select all likely safe" }));
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(true);
    expect(box(2).checked).toBe(true);
    expect(box(3).checked).toBe(false);

    // Shift-click the second row. With the anchor reset by Select all, only b
    // changes. If the earlier a anchor had survived, the a..b range would also
    // uncheck a.
    clickBox(1, true);
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(true);
    expect(box(3).checked).toBe(false);
  });

  it("resets the anchor after the cleanup dialog finishes", () => {
    render(<RecommendationsPage />);

    clickBox(0);
    clickBox(2, true);
    expect(box(0).checked).toBe(true);
    expect(box(1).checked).toBe(true);
    expect(box(2).checked).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Review cleanup…" }));
    fireEvent.click(screen.getByRole("button", { name: "Finish cleanup" }));

    // Selection is cleared. A shift-click straight after the cleanup must not
    // resurrect the old a anchor and reselect the whole a..d range.
    expect(box(0).checked).toBe(false);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(false);

    clickBox(3, true);
    expect(box(0).checked).toBe(false);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(false);
    expect(box(3).checked).toBe(true);
  });

  it("supports a direct change event without a click modifier", () => {
    render(<RecommendationsPage />);

    fireEvent.change(box(0), { target: { checked: true } });
    expect(box(0).checked).toBe(true);
  });
});
