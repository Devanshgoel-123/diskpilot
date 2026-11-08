import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api", () => ({
  api: {
    getTrashSummary: () =>
      Promise.resolve({ access: "granted", items: 3, totalBytes: 2_500_000_000, totalFiles: 3 }),
  },
}));

import { setOs } from "@/lib/platform";

import { EmptyTrashButton } from "./EmptyTrashButton";

afterEach(() => {
  cleanup();
  setOs("macos");
});

describe("EmptyTrashButton wording", () => {
  it("says Trash on macOS", async () => {
    render(<EmptyTrashButton />);
    expect(await screen.findByRole("button", { name: "Empty Trash, 2.5 GB" })).toBeVisible();
  });

  it("says Recycle Bin on Windows", async () => {
    setOs("windows");
    render(<EmptyTrashButton />);
    expect(await screen.findByRole("button", { name: "Empty Recycle Bin, 2.5 GB" })).toBeVisible();
    expect(screen.getByText("Empty Recycle Bin")).toBeVisible();
  });
});
