import { fireEvent, render, screen } from "@testing-library/react";

import type { Classification, DirectoryEntry, DirectoryListing, Recommendation } from "@/lib/types";

import { FolderSummary } from "./FolderSummary";
import { recommendationsUnder } from "./underPath";

const cls = (category: Classification["category"]): Classification => ({
  category,
  label: category === "unknown" ? "Unclassified" : "Label",
  explanation: "",
  ruleId: "r",
  confidence: "high",
  matchedPath: null,
});

const dir = (name: string, bytes: number): DirectoryEntry => ({
  name,
  path: `/u/${name}`,
  kind: "directory",
  bytes,
  fileCount: 1,
  modifiedMs: null,
  extension: null,
  classification: cls("cache"),
});

const listing: DirectoryListing = {
  path: "/u",
  parent: null,
  scanRoot: "/u",
  bytes: 1000,
  entries: [dir("big", 700), dir("mid", 200), dir("tiny", 5)],
  omittedCount: 0,
  omittedBytes: 0,
  classification: cls("unknown"),
};

const rec = (id: string, path: string, bytes: number) =>
  ({ id, path, estimatedReclaimBytes: bytes, methods: ["trash"] }) as unknown as Recommendation;

describe("FolderSummary", () => {
  it("names the biggest items with their share and opens one", () => {
    const onOpen = vi.fn();
    render(<FolderSummary listing={listing} cleanup={[]} onOpen={onOpen} onReview={vi.fn()} />);
    expect(screen.getByText("70%")).toBeVisible();
    expect(screen.getByText("20%")).toBeVisible();
    // Under 1% of the folder: not named, counted in "more".
    expect(screen.queryByText("tiny")).toBeNull();
    expect(screen.getByText("and 1 more")).toBeVisible();
    expect(screen.getByText("Nothing to clean up here")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /big/ }));
    expect(onOpen).toHaveBeenCalledWith(listing.entries[0]);
  });

  it("reviews only the recommendations inside this folder", () => {
    const onReview = vi.fn();
    const cleanup = [rec("a", "/u/big/cache", 300), rec("b", "/u", 10), rec("c", "/other", 99)];
    render(
      <FolderSummary listing={listing} cleanup={cleanup} onOpen={vi.fn()} onReview={onReview} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Review 2" }));
    expect(onReview).toHaveBeenCalledWith(["a", "b"]);
  });
});

describe("recommendationsUnder", () => {
  it("matches the folder and what is inside it, not siblings with the same prefix", () => {
    const recs = [rec("a", "/u/big", 1), rec("b", "/u/big/x", 1), rec("c", "/u/bigger", 1)];
    expect(recommendationsUnder("/u/big", recs).map((r) => r.id)).toEqual(["a", "b"]);
  });
});
