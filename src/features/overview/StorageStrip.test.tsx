import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import type { DiskInfo, ScanSummary } from "@/lib/types";

import { StorageStrip } from "./StorageStrip";

const GB = 1_000_000_000;
const disk = (used: number): DiskInfo => ({
  mountPoint: "/",
  fileSystem: "apfs",
  totalBytes: 245 * GB,
  usedBytes: used * GB,
  availableBytes: (245 - used) * GB,
});
const summary = (cache: number) =>
  ({
    totalBytes: (20 + cache) * GB,
    categories: [
      { category: "developerTools", bytes: 20 * GB, fileCount: 1 },
      { category: "cache", bytes: cache * GB, fileCount: 1 },
    ],
  }) as unknown as ScanSummary;

const strip = (used: number, cache: number) => (
  <TooltipProvider>
    <StorageStrip
      disk={disk(used)}
      summary={summary(cache)}
      recommendations={[]}
      onReview={() => {}}
    />
  </TooltipProvider>
);

describe("Storage strip", () => {
  it("shows its numbers straight away, without counting up from zero", () => {
    render(strip(193, 9));
    expect(screen.getByText("193 GB")).toBeVisible();
    expect(screen.getAllByText("52.0 GB").length).toBeGreaterThan(0);
  });

  it("moves to the new numbers after a cleanup", async () => {
    const { rerender } = render(strip(193, 9));
    rerender(strip(185, 5));
    const wait = { timeout: 5000 };
    expect(await screen.findByText("185 GB", {}, wait)).toBeVisible();
    expect(await screen.findAllByText("60.0 GB", {}, wait)).not.toHaveLength(0);
    expect(await screen.findByText("5.0 GB", {}, wait)).toBeVisible();
  });
});
