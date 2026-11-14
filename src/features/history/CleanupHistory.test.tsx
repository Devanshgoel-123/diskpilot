import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { useApp } from "@/lib/store";
import type { CleanupRecord } from "@/lib/types";

import { CleanupHistory } from "./CleanupHistory";

const record = (n: number): CleanupRecord => ({
  id: `c${n}`,
  kind: "files",
  title: `Cleanup number ${n}`,
  startedAtMs: 1_800_000_000_000 - n * 60_000,
  finishedAtMs: 1_800_000_000_000 - n * 60_000,
  status: "succeeded",
  bytesFreed: 0,
  bytesTrashed: 1_000_000,
  items: [],
});

describe("Cleanup history", () => {
  it("shows a long history a page at a time", () => {
    useApp.setState({ cleanups: Array.from({ length: 20 }, (_, i) => record(i + 1)) });
    render(<CleanupHistory />);
    expect(screen.getByText("Cleanup number 1")).toBeVisible();
    expect(screen.queryByText("Cleanup number 9")).toBeNull();
    expect(screen.getByText("1–8 of 20")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Cleanup number 9")).toBeVisible();
    expect(screen.queryByText("Cleanup number 1")).toBeNull();
  });

  it("has no pager when everything fits on one page", () => {
    useApp.setState({ cleanups: [record(1), record(2)] });
    render(<CleanupHistory />);
    expect(screen.queryByRole("navigation", { name: /Pages of/ })).toBeNull();
  });
});
