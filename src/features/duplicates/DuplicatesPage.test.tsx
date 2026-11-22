import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DuplicateGroup, DuplicateReport } from "@/lib/types";

const group = (id: string, kept: string, names: string[]): DuplicateGroup => ({
  id,
  name: `${id}.jpg`,
  bytes: 1_000_000,
  files: names.map((n) => ({ path: `/Users/t/${n}.jpg`, modifiedMs: null, keep: n === kept })),
  reclaimableBytes: 2_000_000,
  sharesStorage: false,
});

// As shown: a1 (kept), a2, a3, b1, b2 (kept), b3.
const report: DuplicateReport = {
  root: "/Users/t",
  minBytes: 1024 * 1024,
  startedAtMs: 0,
  finishedAtMs: 1,
  filesExamined: 6,
  bytesHashed: 0,
  groups: [group("a", "a1", ["a1", "a2", "a3"]), group("b", "b2", ["b1", "b2", "b3"])],
  reclaimableBytes: 4_000_000,
  truncated: false,
  cancelled: false,
  unreadable: 0,
};

vi.mock("@/lib/api", () => ({
  api: {
    getDuplicates: () => Promise.resolve(report),
    duplicatesRunning: () => Promise.resolve(false),
  },
  errorMessage: (e: unknown) => String(e),
  onDuplicatesProgress: () => Promise.resolve(() => {}),
}));
vi.mock("@/lib/thumbnails", () => ({
  cachedThumbnail: () => undefined,
  loadThumbnail: () => new Promise(() => {}),
  anySmallThumbnail: () => undefined,
  clearThumbnails: () => {},
}));
vi.mock("@/components/shared/FileThumbnail", () => ({ FileThumbnail: () => null }));

import { useApp } from "@/lib/store";

import { DuplicatesPage } from "./DuplicatesPage";

const box = (name: string) =>
  screen.getByRole("checkbox", { name: new RegExp(`/${name}\\.jpg to Trash`) });

const checked = () =>
  ["a1", "a2", "a3", "b1", "b2", "b3"].filter((n) => (box(n) as HTMLInputElement).checked);

async function renderOpen() {
  render(<DuplicatesPage />);
  await screen.findByText("a.jpg");
  for (const b of screen.getAllByRole("button", { expanded: false })) fireEvent.click(b);
}

beforeEach(() => {
  useApp.setState({ env: null, summary: null });
});

describe("Duplicates Shift-click", () => {
  it("selects a range downward, skipping the copies being kept", async () => {
    await renderOpen();
    fireEvent.click(box("a2"));
    fireEvent.click(box("b3"), { shiftKey: true });
    expect(checked()).toEqual(["a2", "a3", "b1", "b3"]);
  });

  it("selects a range upward", async () => {
    await renderOpen();
    fireEvent.click(box("b3"));
    fireEvent.click(box("a3"), { shiftKey: true });
    expect(checked()).toEqual(["a3", "b1", "b3"]);
  });

  it("clears a range when the clicked copy is being unchecked", async () => {
    await renderOpen();
    fireEvent.click(screen.getByRole("button", { name: "Select extra copies" }));
    fireEvent.click(box("a2"));
    fireEvent.click(box("b1"), { shiftKey: true });
    expect(checked()).toEqual(["b3"]);
  });

  it("acts as a plain click when nothing was clicked before", async () => {
    await renderOpen();
    fireEvent.click(box("b1"), { shiftKey: true });
    expect(checked()).toEqual(["b1"]);
  });
});
