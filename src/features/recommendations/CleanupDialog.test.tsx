import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { CleanupPlan, CleanupRecord } from "@/lib/types";

const BUSY = "node.exe (process 7) is running from this project. Stop its dev server first.";

const plan: CleanupPlan = {
  id: "plan-1",
  createdAtMs: 0,
  method: "trash",
  totalBytes: 100,
  items: [
    {
      recommendationId: "old",
      title: "Old workspace",
      path: "/home/me/old",
      bytes: 100,
      fileCount: 1,
      tier: "reviewFirst",
      operation: "Move to Trash",
    },
  ],
  rejected: [{ recommendationId: "live", path: "/home/me/app/node_modules", reason: BUSY }],
  warnings: [],
};

const record: CleanupRecord = {
  id: "plan-1",
  kind: "files",
  title: "Move to Trash 1 item",
  startedAtMs: 0,
  finishedAtMs: 1,
  status: "succeeded",
  bytesFreed: 0,
  bytesTrashed: 100,
  items: [
    {
      target: "/home/me/old",
      operation: "Move to Trash: /home/me/old",
      bytes: 100,
      status: "succeeded",
      error: null,
    },
  ],
};

vi.mock("@/lib/api", () => ({
  api: {
    createCleanupPlan: () => Promise.resolve(plan),
    executeCleanupPlan: () => Promise.resolve(record),
    discardCleanupPlan: () => Promise.resolve(),
    getCleanupHistory: () => Promise.resolve([]),
    getCleanupActivity: () => Promise.resolve([]),
    getDiskInfo: () => Promise.resolve(null),
    getDisks: () => Promise.resolve([]),
  },
  errorMessage: (e: unknown) => String(e),
  onCleanupProgress: () => Promise.resolve(() => {}),
}));
vi.mock("@/lib/sounds", () => ({ playCleanupDone: () => {} }));

import { CleanupDialog } from "./CleanupDialog";

afterEach(cleanup);

describe("items left out of a cleanup", () => {
  it("are named before confirming and again in the results", async () => {
    render(
      <CleanupDialog
        open
        ids={["old", "live"]}
        allowedMethods={["trash"]}
        onOpenChange={() => {}}
        onFinished={() => {}}
      />,
    );
    // Before: only the free item is offered, and the busy one says why.
    const before = await screen.findByRole("list", { name: "Not removed" });
    expect(within(before).getByText(BUSY)).toBeInTheDocument();
    expect(screen.getByText(/1 item is left out/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "I reviewed these paths" }));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Move 1 item to/ }));
    });

    // After: the result counts it as not removed and still says why.
    expect(await screen.findByText("Cleanup partly complete")).toBeInTheDocument();
    expect(screen.getByText(/1 of 2 items done; 1 not removed/)).toBeInTheDocument();
    const after = screen.getByRole("list", { name: "Not removed" });
    expect(within(after).getByText(BUSY)).toBeInTheDocument();
  });
});
