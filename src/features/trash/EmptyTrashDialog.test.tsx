import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TrashListing } from "@/lib/types";

let sendListing: (l: TrashListing) => void = () => {};
vi.mock("@/lib/api", () => ({
  api: {
    previewTrash: () => new Promise(() => {}),
    discardTrashPreview: () => Promise.resolve(null),
    getTrashSummary: () => new Promise(() => {}),
  },
  errorMessage: (e: unknown) => String(e),
  onTrashListing: (cb: (l: TrashListing) => void) => {
    sendListing = cb;
    return Promise.resolve(() => {});
  },
  onTrashProgress: () => Promise.resolve(() => {}),
}));

import { EmptyTrashDialog } from "./EmptyTrashDialog";
import { useTrash } from "./trashStore";

const listing = (over: Partial<TrashListing> = {}): TrashListing => ({
  previewId: "p",
  entriesDone: 1,
  entriesTotal: 4,
  bytes: 2_500_000_000,
  files: 12_480,
  current: "Old Project",
  ...over,
});

beforeEach(() => {
  window.matchMedia = ((q: string) => ({
    matches: q.includes("reduce"),
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
  useTrash.setState({ open: true, session: 1 });
});

describe("Empty Trash while the Trash is listed", () => {
  it("counts up what has been measured so far instead of only a spinner", () => {
    render(<EmptyTrashDialog />);
    expect(screen.getByText("Looking in the Trash…")).toBeVisible();
    expect(screen.queryByRole("progressbar")).toBeNull();

    act(() => sendListing(listing()));
    expect(screen.getByText("2.5 GB")).toBeVisible();
    expect(screen.getByText("in 12,480 files so far")).toBeVisible();
    expect(screen.getByText("Measuring “Old Project”")).toBeVisible();
    expect(screen.getByText("1 of 4 items")).toBeVisible();
    expect(screen.getByRole("progressbar", { name: "Measuring the Trash" })).toHaveAttribute(
      "aria-valuenow",
      "1",
    );

    act(() => sendListing(listing({ entriesDone: 4, current: null, files: 20_001 })));
    expect(screen.getByText("Sorting by size…")).toBeVisible();
    expect(screen.getByText("4 of 4 items")).toBeVisible();
  });
});
